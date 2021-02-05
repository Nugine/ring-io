use crate::ring::RingCq;
use crate::sys::{EnterFlags, RawCQE, SQFlags};
use crate::utils::{AtomicRef, RawArrayPtr};
use crate::{Ring, CQE};

use std::mem::{self, MaybeUninit};
use std::sync::Arc;
use std::{io, slice};

#[derive(Debug)]
pub struct CompletionQueue {
    ring: Arc<Ring>,
}

impl CompletionQueue {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }

    pub fn needs_flush(&self) -> bool {
        let sq = &self.ring.sq;
        let flags = sq.kflags.load_relaxed();
        flags & SQFlags::CQ_OVERFLOW.bits() != 0
    }

    pub fn flush(&self) -> io::Result<()> {
        unsafe { self.ring.enter::<()>(0, 0, EnterFlags::GETEVENTS, None)? };
        Ok(())
    }

    pub fn ready(&self) -> u32 {
        let cq = &self.ring.cq;
        let ktail: u32 = cq.ktail.load_acquire();
        let khead: u32 = cq.khead.load_relaxed();
        ktail.wrapping_sub(khead)
    }

    /// # Safety
    /// `n_cqes` must not be greater than `cq.ready()`
    pub unsafe fn advance(&mut self, n_cqes: u32) {
        if n_cqes == 0 {
            return;
        }
        let cq = &self.ring.cq;
        let new_khead = cq.khead.load_relaxed().wrapping_add(n_cqes);
        cq.khead.store_release(new_khead);
    }

    pub fn peek_cqe(&mut self) -> Option<&CQE> {
        unsafe {
            let cq = &self.ring.cq;
            let ktail = cq.ktail.load_acquire();
            let khead = cq.khead.load_relaxed();
            let ready = ktail.wrapping_sub(khead);
            if ready > 0 {
                mem::transmute(cq.cqes.get_raw(khead & cq.kring_mask))
            } else {
                None
            }
        }
    }

    pub fn peek_batch_cqe<'c, 's: 'c>(
        &'s mut self,
        cqes: &'c mut [MaybeUninit<&'s CQE>],
    ) -> &'c [&'s CQE] {
        unsafe fn peek_batch_cqe(
            cq: &RingCq,
            cqes: RawArrayPtr<*const RawCQE>,
            cap: usize,
        ) -> usize {
            if cap == 0 {
                return 0;
            }
            let ktail = cq.ktail.load_acquire();
            let khead = cq.khead.load_relaxed();
            let ready = ktail.wrapping_sub(khead);
            if ready == 0 {
                return 0;
            }
            let len = (cap as u32).min(ready);
            for i in 0..len {
                let cqe = cq.cqes.get_raw(khead.wrapping_add(i) & cq.kring_mask);
                cqes.write_at(i, cqe);
            }
            len as usize
        }

        unsafe {
            let cq = &self.ring.cq;
            let cap = cqes.len();
            let raw_cqes = mem::transmute(cqes.as_mut_ptr());
            let len = peek_batch_cqe(cq, raw_cqes, cap);
            slice::from_raw_parts(cqes.as_ptr().cast(), len)
        }
    }

    pub fn pop_cqe(&mut self) -> Option<CQE> {
        unsafe {
            let cq = &self.ring.cq;
            let ktail = cq.ktail.load_acquire();
            let khead = cq.khead.load_relaxed();
            let ready = ktail.wrapping_sub(khead);
            if ready > 0 {
                let cqe = cq.cqes.get_raw(khead & cq.kring_mask).cast::<CQE>().read();
                cq.khead.store_release(khead.wrapping_add(1));
                Some(cqe)
            } else {
                None
            }
        }
    }

    pub fn pop_batch_cqe(&mut self, cqes: &mut [MaybeUninit<CQE>]) -> &[CQE] {
        unsafe {
            let cq = &self.ring.cq;
            let cap = cqes.len();
            let raw_cqes = mem::transmute(cqes.as_mut_ptr());
            let len = pop_batch_cqe(cq, raw_cqes, cap);
            slice::from_raw_parts(cqes.as_ptr().cast(), len)
        }
    }

    pub fn sync_pop_batch_cqe(&self, cqes: &mut [MaybeUninit<CQE>]) -> &[CQE] {
        unsafe {
            let cq = &self.ring.cq;
            let _pop_guard = cq.pop_lock.lock();
            let cap = cqes.len();
            let raw_cqes = mem::transmute(cqes.as_mut_ptr());
            let len = pop_batch_cqe(cq, raw_cqes, cap);
            slice::from_raw_parts(cqes.as_ptr().cast(), len)
        }
    }

    pub fn wait_cqes(&mut self, count: u32) -> io::Result<u32> {
        unsafe {
            let cq = &self.ring.cq;

            let ktail = cq.ktail.load_acquire();
            let khead = cq.khead.load_relaxed();
            let ready = ktail.wrapping_sub(khead);
            if ready >= count {
                return Ok(ready);
            }

            let flags = EnterFlags::GETEVENTS;
            self.ring.enter::<()>(0, count, flags, None)?;

            let ktail = cq.ktail.load_acquire();
            let ready = ktail.wrapping_sub(khead);
            debug_assert!(ready >= count);

            Ok(ready)
        }
    }
}

unsafe fn pop_batch_cqe(cq: &RingCq, cqes: RawArrayPtr<RawCQE>, cap: usize) -> usize {
    if cap == 0 {
        return 0;
    }

    let ktail = cq.ktail.load_acquire();
    let khead = cq.khead.load_relaxed();
    let ready = ktail.wrapping_sub(khead);

    if ready == 0 {
        return 0;
    }

    let len = (cap as u32).min(ready);

    for i in 0..len {
        let cqe_ptr = cq.cqes.get_raw(khead.wrapping_add(i) & cq.kring_mask);
        cqes.write_at(i, cqe_ptr.read());
    }
    cq.khead.store_release(khead.wrapping_add(len));

    len as usize
}
