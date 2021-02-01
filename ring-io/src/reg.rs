use crate::sys::{self, RegisterOp};
use crate::utils::{libc_call, Errno};
use crate::Ring;

use std::os::unix::io::RawFd;
use std::sync::Arc;
use std::{io, ptr};

#[derive(Debug)]
pub struct Registrar {
    ring: Arc<Ring>,
}

impl Registrar {
    pub(crate) unsafe fn split_from(ring: Arc<Ring>) -> Self {
        Self { ring }
    }

    /// # Safety
    /// The buffers must be valid until they are unregistered or the ring is dropped.
    pub unsafe fn register_buffers(
        &self,
        iovecs: *const libc::iovec,
        n_vecs: usize,
    ) -> io::Result<()> {
        self.ring.register_buffers(iovecs, n_vecs)
    }

    pub fn unregister_buffers(&self) -> io::Result<()> {
        self.ring.unregister_buffers()
    }

    /// # Safety
    /// The files must not be closed until they are unregistered or the ring is dropped.
    pub unsafe fn register_files(&self, files: &[RawFd]) -> io::Result<()> {
        self.ring.register_files(files)
    }

    pub fn unregister_files(&self) -> io::Result<()> {
        self.ring.unregister_files()
    }
}

impl Ring {
    unsafe fn register(&self, op: RegisterOp, args: *const (), nr_args: u32) -> Result<(), Errno> {
        libc_call(|| sys::io_uring_register(self.ring_fd, op as u32, args.cast(), nr_args))?;
        Ok(())
    }

    /// # Safety
    /// The buffers must be valid until they are unregistered or the ring is dropped.
    pub unsafe fn register_buffers(
        &self,
        iovecs: *const libc::iovec,
        n_vecs: usize,
    ) -> io::Result<()> {
        self.register(RegisterOp::RegisterBuffers, iovecs.cast(), n_vecs as u32)?;
        Ok(())
    }

    pub fn unregister_buffers(&self) -> io::Result<()> {
        unsafe { self.register(RegisterOp::UnregisterBuffers, ptr::null(), 0)? };
        Ok(())
    }

    /// # Safety
    /// The files must not be closed until they are unregistered or the ring is dropped.
    pub unsafe fn register_files(&self, files: &[RawFd]) -> io::Result<()> {
        let files_ptr = files.as_ptr();
        let nr_files = files.len() as u32;
        self.register(RegisterOp::RegisterFiles, files_ptr.cast(), nr_files)?;
        Ok(())
    }

    pub fn unregister_files(&self) -> io::Result<()> {
        unsafe { self.register(RegisterOp::UnregisterFiles, ptr::null(), 0)? };
        Ok(())
    }
}
