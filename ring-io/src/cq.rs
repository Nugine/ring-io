use crate::sys::{EnterFlags, RawCQE, SQFlags};
use crate::utils::{AtomicRef, RawArrayPtr};
use crate::{Ring, CQE};

use std::mem::MaybeUninit;
use std::sync::Arc;
use std::{io, mem, slice};

#[derive(Debug)]
pub struct CompletionQueue {
    ring: Arc<Ring>,
}

impl CompletionQueue {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }

    pub fn ready(&self) -> u32 {
        unsafe {
            let cq = &self.ring.cq;
            let tail: u32 = cq.ktail.load_acquire();
            let head: u32 = cq.khead.unsync_read();
            tail.wrapping_sub(head)
        }
    }

    /// # Safety
    /// `n_cqes` must not be greater than `cq.ready()`
    pub unsafe fn advance_unchecked(&mut self, n_cqes: u32) {
        if n_cqes == 0 {
            return;
        }

        let cq = &self.ring.cq;
        let new_khead = cq.khead.unsync_read().wrapping_add(n_cqes);
        cq.khead.store_release(new_khead);
    }

    pub fn advance(&mut self, n_cqes: u32) {
        if n_cqes > self.ready() {
            panic!("n_cqes must not be greater than cq.ready()");
        }
        unsafe { self.advance_unchecked(n_cqes) }
    }

    pub fn needs_flush(&self) -> bool {
        let sq = &self.ring.sq;
        let flags = sq.kflags.load_acquire(); // FIXME: Acquire or Relaxed?
        let flags = unsafe { SQFlags::from_bits_unchecked(flags) };
        flags.contains(SQFlags::CQ_OVERFLOW)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if self.needs_flush() {
            unsafe { self.ring.enter::<()>(0, 0, EnterFlags::GETEVENTS, None)? };
        }
        Ok(())
    }

    pub fn peek_cqe(&mut self) -> Option<&CQE> {
        unsafe {
            let cq = &self.ring.cq;
            let tail = cq.ktail.load_acquire();
            let head = cq.khead.unsync_read();
            let ready = tail.wrapping_sub(head);

            if ready > 0 {
                let mask = cq.kring_mask.read();
                mem::transmute(cq.cqes.get_raw((head & mask) as usize))
            } else {
                None
            }
        }
    }

    pub fn peek_batch_cqe<'c, 's: 'c>(
        &'s mut self,
        cqes: &'c mut [MaybeUninit<&'s CQE>],
    ) -> &'c [&'s CQE] {
        unsafe {
            let cq = &self.ring.cq;
            let tail = cq.ktail.load_acquire();
            let head = cq.khead.unsync_read();
            let ready = tail.wrapping_sub(head);

            if ready == 0 || cqes.is_empty() {
                return slice::from_raw_parts(cqes.as_ptr().cast(), 0);
            }

            let cqes_to_fill: RawArrayPtr<*const RawCQE> = mem::transmute(cqes.as_mut_ptr());
            let len = (cqes.len() as u32).min(ready);
            let mask = cq.kring_mask.read();

            let mut head = head;
            for i in 0..len {
                let cqe = cq.cqes.get_raw((head & mask) as usize);
                cqes_to_fill.write_at(i as usize, cqe);
                head = head.wrapping_add(1);
            }

            slice::from_raw_parts(cqes.as_ptr().cast(), len as usize)
        }
    }

    pub fn pop_cqe(&mut self) -> Option<CQE> {
        let cqe = self.peek_cqe()?;
        let cqe = cqe.clone();
        unsafe { self.advance_unchecked(1) };
        Some(cqe)
    }

    pub fn wait_cqes(&mut self, count: u32) -> io::Result<u32> {
        unsafe {
            let cq = &self.ring.cq;
            let head = cq.khead.unsync_read();

            let tail = cq.ktail.load_acquire();
            let ready = tail.wrapping_sub(head);
            if ready >= count {
                return Ok(ready);
            }

            let flags = EnterFlags::GETEVENTS;
            self.ring.enter::<()>(0, count, flags, None)?;

            let tail = cq.ktail.load_acquire();
            let ready = tail.wrapping_sub(head);
            debug_assert!(ready >= count);

            Ok(ready)
        }
    }
}
