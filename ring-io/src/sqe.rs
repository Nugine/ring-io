use crate::sys::{FsyncFlags, RawSQE, SQEFlags};

use std::mem::MaybeUninit;
use std::os::unix::io::RawFd;
use std::{fmt, ptr};

/// TODO: Safety documentation
#[repr(transparent)]
pub struct SQE {
    sqe: RawSQE,
}

unsafe impl Send for SQE {}
unsafe impl Sync for SQE {}

impl Clone for SQE {
    fn clone(&self) -> Self {
        Self {
            sqe: unsafe { ptr::read(&self.sqe) },
        }
    }
}

impl fmt::Debug for SQE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SQE {{ .. }}")
    }
}

impl SQE {
    pub fn set_flags(&mut self, flags: SQEFlags) {
        self.sqe.set_flags(flags)
    }

    pub fn enable_flags(&mut self, flags: SQEFlags) {
        self.sqe.enable_flags(flags)
    }

    pub fn set_user_data(&mut self, user_data: u64) {
        self.sqe.set_user_data(user_data)
    }
}

impl PrepareSQE for SQE {
    fn as_raw_mut_sqe(&mut self) -> *mut SQE {
        self
    }
}

impl PrepareSQE for MaybeUninit<SQE> {
    fn as_raw_mut_sqe(&mut self) -> *mut SQE {
        self.as_mut_ptr()
    }
}

unsafe fn do_prep(this: &mut (impl PrepareSQE + ?Sized), f: impl FnOnce(*mut RawSQE)) -> &mut SQE {
    let sqe = this.as_raw_mut_sqe();
    f(sqe.cast());
    &mut *sqe
}

pub trait PrepareSQE {
    fn as_raw_mut_sqe(&mut self) -> *mut SQE;

    /// # Safety
    /// See [`SQE`]
    unsafe fn prep_nop(&mut self) -> &mut SQE {
        do_prep(self, |sqe| RawSQE::prep_nop(sqe))
    }

    /// # Safety
    /// See [`SQE`]
    unsafe fn prep_readv(
        &mut self,
        fd: RawFd,
        iovecs: *const libc::iovec,
        n_vecs: usize,
        offset: isize,
    ) -> &mut SQE {
        do_prep(self, |sqe| {
            RawSQE::prep_readv(sqe, fd, iovecs.cast(), n_vecs as u32, offset as libc::off_t)
        })
    }

    /// # Safety
    /// See [`SQE`]
    unsafe fn prep_writev(
        &mut self,
        fd: RawFd,
        iovecs: *const libc::iovec,
        n_vecs: usize,
        offset: isize,
    ) -> &mut SQE {
        do_prep(self, |sqe| {
            RawSQE::prep_writev(sqe, fd, iovecs, n_vecs as u32, offset as libc::off_t)
        })
    }

    /// # Safety
    /// See [`SQE`]
    unsafe fn prep_fsync(&mut self, fd: RawFd, flags: FsyncFlags) -> &mut SQE {
        do_prep(self, |sqe| RawSQE::prep_fsync(sqe, fd, flags))
    }

    // TODO: impl more prep_* methods
}
