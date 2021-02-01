use crate::sys;
use crate::utils::resultify;

use std::{fmt, io, ptr};

#[repr(transparent)]
pub struct CQE {
    cqe: sys::RawCQE,
}

unsafe impl Send for CQE {}
unsafe impl Sync for CQE {}

impl Clone for CQE {
    fn clone(&self) -> Self {
        Self {
            cqe: unsafe { ptr::read(&self.cqe) },
        }
    }
}

impl fmt::Debug for CQE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CQE")
            .field("user_data", &self.user_data())
            .field("res", &self.raw_result())
            .field("flags", &self.raw_flags())
            .finish()
    }
}

impl CQE {
    pub fn user_data(&self) -> u64 {
        self.cqe.user_data
    }

    pub fn raw_result(&self) -> i32 {
        self.cqe.res
    }

    pub fn raw_flags(&self) -> u32 {
        self.cqe.flags
    }

    pub fn io_result(&self) -> io::Result<u32> {
        resultify(self.cqe.res)
    }

    pub fn is_err(&self) -> bool {
        self.cqe.res < 0
    }
}
