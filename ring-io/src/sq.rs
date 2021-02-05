use crate::ring::RingSq;
use crate::sys::{EnterFlags, SQFlags, SetupFlags};
use crate::utils::{AtomicRef, Errno, RawArrayPtr};
use crate::{Ring, SQE};

use std::mem::{self, MaybeUninit};
use std::sync::atomic::fence;
use std::sync::atomic::Ordering::AcqRel;
use std::{io, slice};

use std::sync::Arc;

#[derive(Debug)]
pub struct SubmissionQueue {
    ring: Arc<Ring>,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SQEIndex(u32);

impl SubmissionQueue {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }

    pub fn available(&self) -> u32 {
        let sq = &self.ring.sq;
        let khead = sq.khead.load_acquire();
        let rhead = sq.rhead.load_acquire();
        khead.wrapping_sub(rhead)
    }

    pub fn prepared(&self) -> u32 {
        let sq = &self.ring.sq;
        let khead = sq.khead.load_acquire();
        let rtail = sq.rtail.load_acquire();
        rtail.wrapping_sub(khead)
    }

    /// # Safety
    /// TODO
    pub unsafe fn modify_sqe(
        &self,
        index: SQEIndex,
        f: impl FnOnce(&mut MaybeUninit<SQE>) -> &mut SQE,
    ) {
        let sq = &self.ring.sq;

        let sqe_ptr: *mut SQE = sq.sqes.get_raw_mut(index.0).cast();
        let ret_ptr: *mut SQE = f(&mut *sqe_ptr.cast());

        debug_assert_eq!(sqe_ptr, ret_ptr);
    }

    pub fn pop_sqe(&mut self) -> Option<SQEIndex> {
        let sq = &self.ring.sq;
        unsafe { pop_sqe(sq) }
    }

    pub fn pop_batch_sqe(&mut self, indices: &mut [MaybeUninit<SQEIndex>]) -> &[SQEIndex] {
        unsafe {
            let sq = &self.ring.sq;
            let cap = indices.len();
            let raw_indices = mem::transmute(indices.as_mut_ptr());
            let len = pop_batch_sqe(sq, raw_indices, cap);
            slice::from_raw_parts(indices.as_ptr().cast(), len)
        }
    }

    pub fn sync_pop_batch_cqe(&self, indices: &mut [MaybeUninit<SQEIndex>]) -> &[SQEIndex] {
        unsafe {
            let sq = &self.ring.sq;
            let _pop_guard = sq.pop_lock.lock();
            let cap = indices.len();
            let raw_indices = mem::transmute(indices.as_mut_ptr());
            let len = pop_batch_sqe(sq, raw_indices, cap);
            slice::from_raw_parts(indices.as_ptr().cast(), len)
        }
    }

    /// # Safety
    pub unsafe fn push_sqe(&mut self, index: SQEIndex) {
        let sq = &self.ring.sq;
        push_sqe(sq, index.0)
    }

    /// # Safety
    pub unsafe fn push_batch_sqe(&mut self, indices: &[SQEIndex]) {
        let sq = &self.ring.sq;
        let len = indices.len() as u32;
        let raw_indices = mem::transmute(indices.as_ptr());
        push_batch_sqe(sq, raw_indices, len)
    }

    /// # Safety
    pub unsafe fn sync_push_batch_sqe(&mut self, indices: &[SQEIndex]) {
        let sq = &self.ring.sq;
        let _push_guard = sq.push_lock.lock();
        let len = indices.len() as u32;
        let raw_indices = mem::transmute(indices.as_ptr());
        push_batch_sqe(sq, raw_indices, len)
    }

    pub fn submit(&self) -> io::Result<u32> {
        unsafe { Ok(submit_and_wait(&self.ring, 0)?) }
    }

    pub fn submit_and_wait(&self, n_wait: u32) -> io::Result<u32> {
        unsafe { Ok(submit_and_wait(&self.ring, n_wait)?) }
    }
}

unsafe fn pop_sqe(sq: &RingSq) -> Option<SQEIndex> {
    let khead = sq.khead.load_acquire();
    let rhead = sq.rhead.load_acquire();

    let available = khead.wrapping_sub(rhead);
    if available == 0 {
        return None;
    }
    let idx = sq.array.read_at(rhead & sq.kring_mask);

    sq.rhead.store_release(rhead.wrapping_add(1));
    Some(SQEIndex(idx))
}

unsafe fn pop_batch_sqe(sq: &RingSq, indices: RawArrayPtr<u32>, cap: usize) -> usize {
    if cap == 0 {
        return 0;
    }
    let khead = sq.khead.load_acquire();
    let rhead = sq.rhead.load_acquire();

    let available = khead.wrapping_sub(rhead);
    if available == 0 {
        return 0;
    }

    let len = (cap as u32).min(available);
    let raw_indices: RawArrayPtr<u32> = mem::transmute(indices);
    for i in 0..len {
        let idx = sq.array.read_at((rhead + i) & sq.kring_mask);
        raw_indices.write_at(i, idx)
    }

    sq.rhead.store_release(rhead.wrapping_add(len));
    len as usize
}

unsafe fn push_sqe(sq: &RingSq, index: u32) {
    let rtail = sq.rtail.load_acquire();
    sq.array.write_at(rtail & sq.kring_mask, index);
    sq.rtail.store_release(rtail.wrapping_add(1))
}

unsafe fn push_batch_sqe(sq: &RingSq, indices: RawArrayPtr<u32>, len: u32) {
    if len == 0 {
        return;
    }
    let rtail = sq.rtail.load_acquire();
    for i in 0..len {
        let idx = indices.read_at(i);
        let array_pos = rtail.wrapping_add(i) & sq.kring_mask;
        sq.array.write_at(array_pos, idx);
    }
    sq.rtail.store_release(rtail.wrapping_add(len))
}

unsafe fn submit_and_wait(ring: &Ring, n_wait: u32) -> Result<u32, Errno> {
    let sq = &ring.sq;

    let rtail = sq.rtail.load_acquire();
    let khead = sq.khead.load_relaxed();

    sq.ktail.store_release(rtail);
    let to_submit = rtail.wrapping_sub(khead);

    fence(AcqRel);

    let mut enter_flags = EnterFlags::empty();
    let needs_enter = if ring.setup_flags.contains(SetupFlags::SQPOLL) {
        let kflags = sq.kflags.load_relaxed();
        if kflags & SQFlags::NEED_WAKEUP.bits() != 0 {
            enter_flags |= EnterFlags::SQ_WAKEUP;
            true
        } else {
            false
        }
    } else {
        true
    };

    if n_wait > 0 || needs_enter {
        let ret = ring.enter::<()>(to_submit, n_wait, enter_flags, None)?;
        Ok(ret as u32)
    } else {
        Ok(to_submit)
    }
}
