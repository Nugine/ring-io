#![allow(dead_code)]

use crate::sys::{self, RawCQE, RawCqOffsets, RawSQE, RawSqOffsets};
use crate::sys::{EnterFlags, FeatureFlags, SetupFlags};
use crate::utils::{guard, last_errno, libc_call, mmap_offset, mmap_offset_mut, AtomicRef};
use crate::utils::{Errno, KU32Ptr, RawArrayPtr};
use crate::{CompletionQueue, Registrar, SubmissionQueue};

use std::mem::MaybeUninit;
use std::os::unix::io::RawFd;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::{fmt, io, mem, ptr};

use crossbeam_utils::CachePadded;
use parking_lot::Mutex;

pub struct RingBuilder {
    entries: u32,
    params: sys::RawRingParams,
}

unsafe impl Send for RingBuilder {}
unsafe impl Sync for RingBuilder {}

pub struct Ring {
    pub(crate) ring_fd: RawFd,

    pub(crate) setup_flags: SetupFlags,
    pub(crate) features: FeatureFlags,

    sq_mmap_size: usize,
    cq_mmap_size: usize,

    sq_mmap: *mut (),
    cq_mmap: *mut (),
    sqe_mmap: *mut (),

    pub(crate) sq: RingSq,
    pub(crate) cq: RingCq,
}

unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

#[repr(C)]
pub(crate) struct RingSq {
    pub khead: KU32Ptr,
    pub ktail: KU32Ptr,
    pub kring_mask: u32,
    pub kring_entries: u32,
    pub kdropped: KU32Ptr,
    pub kflags: KU32Ptr,
    pub array: RawArrayPtr<u32>,
    pub sqes: RawArrayPtr<RawSQE>,

    pub rhead: CachePadded<AtomicU32>,
    pub rtail: CachePadded<AtomicU32>,

    pub pop_lock: CachePadded<Mutex<()>>,
    pub push_lock: CachePadded<Mutex<()>>,
}

#[repr(C)]
pub(crate) struct RingCq {
    pub khead: KU32Ptr,
    pub ktail: KU32Ptr,
    pub kring_mask: u32,
    pub kring_entries: u32,
    pub koverflow: KU32Ptr,
    pub kflags: Option<KU32Ptr>,
    pub cqes: RawArrayPtr<RawCQE>,

    pub pop_lock: Mutex<()>,
}

impl fmt::Debug for Ring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ring")
            .field("ring_fd", &self.ring_fd)
            .field("setup_flags", &self.setup_flags)
            .field("features", &self.features)
            .finish()
    }
}

impl fmt::Debug for RingBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RingBuilder")
            .field("entries", &self.entries)
            .finish()
    }
}

impl RingBuilder {
    pub fn setup(mut self) -> io::Result<Ring> {
        unsafe { ring_setup_with_params(self.entries, &mut self.params).map_err(From::from) }
    }
}

impl Drop for Ring {
    fn drop(&mut self) {
        unsafe { ring_destroy(self) }
    }
}

impl Ring {
    pub fn with_entries(entries: u32) -> RingBuilder {
        RingBuilder {
            entries,
            params: unsafe { MaybeUninit::zeroed().assume_init() },
        }
    }

    pub fn ring_fd(&self) -> RawFd {
        self.ring_fd
    }

    pub fn sq_entries(&self) -> u32 {
        self.sq.kring_entries
    }

    pub fn cq_entries(&self) -> u32 {
        self.cq.kring_entries
    }

    pub fn features(&self) -> FeatureFlags {
        self.features
    }

    pub fn split(self) -> (SubmissionQueue, CompletionQueue, Registrar) {
        let ring = Arc::new(self);
        unsafe {
            let sq = SubmissionQueue::split_from(Arc::clone(&ring));
            let cq = CompletionQueue::split_from(Arc::clone(&ring));
            let reg = Registrar::split_from(ring);
            (sq, cq, reg)
        }
    }

    pub(crate) unsafe fn enter<T: Copy>(
        &self,
        to_submit: u32,
        min_complete: u32,
        flags: EnterFlags,
        arg: Option<&T>,
    ) -> Result<i32, Errno> {
        let size: u32 = mem::size_of::<T>() as u32;
        let arg: *const libc::c_void = mem::transmute(arg);
        let flags = flags.bits();
        let ret = sys::io_uring_enter(self.ring_fd, to_submit, min_complete, flags, arg, size);
        if ret < 0 {
            return Err(last_errno());
        }
        Ok(ret)
    }
}

const U32_SIZE: usize = mem::size_of::<u32>();
const CQE_SIZE: usize = mem::size_of::<RawCQE>();
const SQE_SIZE: usize = mem::size_of::<RawSQE>();

unsafe fn ring_mmap(fd: RawFd, size: usize, offset: u64) -> Result<*mut (), Errno> {
    let ptr = libc::mmap(
        ptr::null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED | libc::MAP_POPULATE,
        fd,
        offset as i64,
    );
    if ptr == libc::MAP_FAILED {
        return Err(last_errno());
    }
    Ok(ptr.cast())
}

unsafe fn ring_new_sq(sq_mmap: *mut (), sq_off: &RawSqOffsets, sqe_mmap: *mut ()) -> RingSq {
    let khead = KU32Ptr::new_unchecked(mmap_offset_mut(sq_mmap, sq_off.head));
    let ktail = KU32Ptr::new_unchecked(mmap_offset_mut(sq_mmap, sq_off.tail));

    let kring_mask = mmap_offset::<u32>(sq_mmap, sq_off.ring_mask).read();
    let kring_entries = mmap_offset::<u32>(sq_mmap, sq_off.ring_entries).read();

    let array = RawArrayPtr::new_unchecked(mmap_offset_mut(sq_mmap, sq_off.array));

    let rhead;
    let rtail;
    {
        let head = khead.load_relaxed();
        let tail = ktail.unsync_load();
        assert_eq!(head, tail);

        rhead = head.wrapping_sub(kring_entries);
        rtail = tail;

        for i in 0..kring_entries {
            array.write_at((rhead + i) & kring_mask, i);
        }
    }

    RingSq {
        khead,
        ktail,
        kring_mask,
        kring_entries,
        kdropped: KU32Ptr::new_unchecked(mmap_offset_mut(sq_mmap, sq_off.dropped)),
        kflags: KU32Ptr::new_unchecked(mmap_offset_mut(sq_mmap, sq_off.flags)),
        array,
        sqes: RawArrayPtr::new_unchecked(sqe_mmap.cast::<RawSQE>()),
        rhead: CachePadded::new(AtomicU32::new(rhead)),
        rtail: CachePadded::new(AtomicU32::new(rtail)),
        pop_lock: CachePadded::new(Mutex::new(())),
        push_lock: CachePadded::new(Mutex::new(())),
    }
}

unsafe fn ring_new_cq(cq_mmap: *mut (), cq_off: &RawCqOffsets) -> RingCq {
    let khead = KU32Ptr::new_unchecked(mmap_offset_mut(cq_mmap, cq_off.head));
    let ktail = KU32Ptr::new_unchecked(mmap_offset_mut(cq_mmap, cq_off.tail));
    let kflags = if cq_off.flags != 0 {
        Some(KU32Ptr::new_unchecked(mmap_offset_mut(
            cq_mmap,
            cq_off.flags,
        )))
    } else {
        None
    };

    RingCq {
        khead,
        ktail,
        kring_mask: mmap_offset::<u32>(cq_mmap, cq_off.ring_mask).read(),
        kring_entries: mmap_offset::<u32>(cq_mmap, cq_off.ring_entries).read(),
        koverflow: KU32Ptr::new_unchecked(mmap_offset_mut(cq_mmap, cq_off.overflow)),
        kflags,
        cqes: RawArrayPtr::new_unchecked(mmap_offset_mut(cq_mmap, cq_off.cqes)),
        pop_lock: Mutex::new(()),
    }
}

pub unsafe fn ring_setup_with_params(
    entries: u32,
    params: &mut sys::RawRingParams,
) -> Result<Ring, Errno> {
    let ring_fd = libc_call(|| sys::io_uring_setup(entries, params))?;
    let ring_fd_guard: _ = guard(|| libc::close(ring_fd));

    let p = &*params;

    // FIXME: check unknown flags?
    let setup_flags = sys::SetupFlags::from_bits_truncate(p.flags);
    let features = sys::FeatureFlags::from_bits_truncate(p.features);

    let is_single_mmap = features.contains(sys::FeatureFlags::SINGLE_MMAP);

    // FIXME: check overflow?
    let (sq_mmap_size, cq_mmap_size) = {
        let sq_sz = (p.sq_off.array as usize) + (p.sq_entries as usize) * U32_SIZE;
        let cq_sz = (p.cq_off.cqes as usize) + (p.cq_entries as usize) * CQE_SIZE;
        if is_single_mmap {
            let max_sz = sq_sz.max(cq_sz);
            (max_sz, max_sz)
        } else {
            (sq_sz, cq_sz)
        }
    };

    let sq_mmap = ring_mmap(ring_fd, sq_mmap_size, sys::IORING_OFF_SQ_RING)?;
    let sq_mmap_guard = guard(|| libc::munmap(sq_mmap.cast(), sq_mmap_size));

    let cq_mmap = if is_single_mmap {
        sq_mmap
    } else {
        ring_mmap(ring_fd, cq_mmap_size, sys::IORING_OFF_CQ_RING)?
    };
    let cq_mmap_guard = guard(|| libc::munmap(cq_mmap.cast(), cq_mmap_size));

    let sqes_size = (p.sq_entries as usize) * SQE_SIZE;
    let sqe_mmap = ring_mmap(ring_fd, sqes_size, sys::IORING_OFF_SQES)?;

    let sq = ring_new_sq(sq_mmap, &p.sq_off, sqe_mmap);
    let cq = ring_new_cq(cq_mmap, &p.cq_off);

    ring_fd_guard.cancel();
    sq_mmap_guard.cancel();
    cq_mmap_guard.cancel();

    let ring = Ring {
        ring_fd,
        setup_flags,
        features,
        sq_mmap_size,
        cq_mmap_size,
        sq_mmap,
        cq_mmap,
        sqe_mmap,
        sq,
        cq,
    };

    Ok(ring)
}

pub unsafe fn ring_destroy(ring: &mut Ring) {
    let kring_entries = ring.sq.kring_entries;
    let sqes_size = (kring_entries as usize) * SQE_SIZE;

    libc::munmap(ring.sqe_mmap.cast(), sqes_size);

    libc::munmap(ring.sq_mmap.cast(), ring.sq_mmap_size);
    if ring.cq_mmap != ring.sq_mmap {
        libc::munmap(ring.cq_mmap.cast(), ring.cq_mmap_size);
    }

    libc::close(ring.ring_fd);
}
