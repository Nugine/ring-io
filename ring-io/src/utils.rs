#![allow(dead_code)]

use std::mem::ManuallyDrop;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};
use std::{io, mem};

// -----------------------------------------------------------------------------

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct KU32Ptr(NonNull<AtomicU32>);

impl KU32Ptr {
    pub unsafe fn new_unchecked(p: *mut AtomicU32) -> Self {
        Self(NonNull::new_unchecked(p))
    }
}

// -----------------------------------------------------------------------------

#[repr(transparent)]
#[derive(Debug)]
pub struct ReadOnlyPtr<T: Copy>(NonNull<T>);

impl<T: Copy> Clone for ReadOnlyPtr<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<T: Copy> Copy for ReadOnlyPtr<T> {}

impl<T: Copy> ReadOnlyPtr<T> {
    pub unsafe fn new_unchecked(p: *const T) -> Self {
        Self(NonNull::new_unchecked(p as *mut T))
    }

    pub fn read(self) -> T {
        unsafe { *self.0.as_ref() }
    }
}

// -----------------------------------------------------------------------------

#[repr(transparent)]
pub struct RawArrayPtr<T: Copy>(NonNull<T>);

impl<T: Copy> Clone for RawArrayPtr<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<T: Copy> Copy for RawArrayPtr<T> {}

impl<T: Copy> RawArrayPtr<T> {
    pub unsafe fn new_unchecked(p: *mut T) -> Self {
        Self(NonNull::new_unchecked(p))
    }

    pub unsafe fn read_at(self, index: usize) -> T {
        self.0.as_ptr().add(index).read()
    }

    pub unsafe fn write_at(self, index: usize, value: T) {
        self.0.as_ptr().add(index).write(value)
    }

    pub unsafe fn get_raw(self, index: usize) -> *const T {
        self.0.as_ptr().add(index)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Errno(i32);

pub fn last_errno() -> Errno {
    Errno(unsafe { libc::__errno_location().read() })
}

impl From<Errno> for io::Error {
    fn from(errno: Errno) -> Self {
        io::Error::from_raw_os_error(errno.0)
    }
}

// -----------------------------------------------------------------------------

#[inline(always)]
pub fn libc_call(f: impl FnOnce() -> i32) -> Result<i32, Errno> {
    let ret = f();
    if ret < 0 {
        return Err(last_errno());
    }
    Ok(ret)
}

// -----------------------------------------------------------------------------

pub struct Guard<F: FnOnce() -> T, T = ()>(ManuallyDrop<F>);

impl<F: FnOnce() -> T, T> Drop for Guard<F, T> {
    fn drop(&mut self) {
        let f = unsafe { ManuallyDrop::take(&mut self.0) };
        drop(f());
    }
}

impl<F: FnOnce() -> T, T> Guard<F, T> {
    pub fn trigger(self) {
        drop(self)
    }

    pub fn cancel(mut self) {
        let f = unsafe { ManuallyDrop::take(&mut self.0) };
        drop(f);
        mem::forget(self);
    }
}

pub fn guard<F: FnOnce() -> T, T>(f: F) -> Guard<F, T> {
    Guard(ManuallyDrop::new(f))
}

// -----------------------------------------------------------------------------

pub unsafe fn mmap_offset<T>(base: *mut (), offset: u32) -> *const T {
    base.cast::<u8>().add(offset as usize).cast()
}

pub unsafe fn mmap_offset_mut<T>(base: *mut (), offset: u32) -> *mut T {
    base.cast::<u8>().add(offset as usize).cast()
}

// -----------------------------------------------------------------------------

pub fn resultify(x: i32) -> io::Result<u32> {
    if x >= 0 {
        Ok(x as u32)
    } else {
        Err(io::Error::from_raw_os_error(-x))
    }
}

// -----------------------------------------------------------------------------

pub fn ptr_cast<T: ?Sized, U>(ptr: *const T) -> *const U {
    ptr.cast()
}

pub fn ptr_cast_mut<T: ?Sized, U>(ptr: *mut T) -> *mut U {
    ptr.cast()
}

// -----------------------------------------------------------------------------

pub trait AtomicRef {
    type Target: Copy;

    unsafe fn unsync_read(self) -> Self::Target;
    unsafe fn unsync_write(self, val: Self::Target);

    fn load_relaxed(self) -> Self::Target;
    fn store_relaxed(self, val: Self::Target);

    fn load_acquire(self) -> Self::Target;
    fn store_release(self, val: Self::Target);
}

impl AtomicRef for KU32Ptr {
    type Target = u32;

    #[inline(always)]
    unsafe fn unsync_read(self) -> u32 {
        (self.0.as_ptr() as *const u32).read()
    }

    #[inline(always)]
    unsafe fn unsync_write(self, val: u32) {
        (self.0.as_ptr() as *mut u32).write(val)
    }

    #[inline(always)]
    fn load_relaxed(self) -> u32 {
        unsafe { AtomicU32::load(self.0.as_ref(), Ordering::Relaxed) }
    }

    #[inline(always)]
    fn store_relaxed(self, val: u32) {
        unsafe { AtomicU32::store(self.0.as_ref(), val, Ordering::Relaxed) }
    }

    #[inline(always)]
    fn load_acquire(self) -> u32 {
        unsafe { AtomicU32::load(self.0.as_ref(), Ordering::Acquire) }
    }

    #[inline(always)]
    fn store_release(self, val: u32) {
        unsafe { AtomicU32::store(self.0.as_ref(), val, Ordering::Release) }
    }
}

impl AtomicRef for &'_ AtomicU32 {
    type Target = u32;

    unsafe fn unsync_read(self) -> Self::Target {
        ptr_cast::<_, u32>(self).read()
    }

    unsafe fn unsync_write(self, val: Self::Target) {
        (ptr_cast::<_, u32>(self) as *mut u32).write(val)
    }

    fn load_relaxed(self) -> Self::Target {
        self.load(Ordering::Relaxed)
    }

    fn store_relaxed(self, val: Self::Target) {
        self.store(val, Ordering::Relaxed)
    }

    fn load_acquire(self) -> Self::Target {
        self.load(Ordering::Acquire)
    }

    fn store_release(self, val: Self::Target) {
        self.store(val, Ordering::Release)
    }
}

// -----------------------------------------------------------------------------
