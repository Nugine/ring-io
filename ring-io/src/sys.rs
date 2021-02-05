#![allow(missing_debug_implementations, non_upper_case_globals, dead_code)]

use std::os::unix::io::RawFd;
use std::ptr;

use bitflags::bitflags;

pub const __NR_io_uring_setup: libc::c_long = 425;
pub const __NR_io_uring_enter: libc::c_long = 426;
pub const __NR_io_uring_register: libc::c_long = 427;

/// # Safety
/// syscall: io_uring_register
pub unsafe fn io_uring_register(
    fd: libc::c_int,
    opcode: libc::c_uint,
    arg: *const libc::c_void,
    nr_args: libc::c_uint,
) -> libc::c_int {
    libc::syscall(__NR_io_uring_register, fd, opcode, arg, nr_args) as libc::c_int
}

/// # Safety
/// syscall: io_uring_setup
pub unsafe fn io_uring_setup(entries: libc::c_uint, p: *mut RawRingParams) -> libc::c_int {
    libc::syscall(__NR_io_uring_setup, entries, p) as libc::c_int
}

/// # Safety
/// syscall: io_uring_enter
pub unsafe fn io_uring_enter(
    fd: libc::c_int,
    to_submit: libc::c_uint,
    min_complete: libc::c_uint,
    flags: libc::c_uint,
    arg: *const libc::c_void,
    size: libc::c_uint,
) -> libc::c_int {
    libc::syscall(
        __NR_io_uring_enter,
        fd,
        to_submit,
        min_complete,
        flags,
        arg,
        size,
    ) as libc::c_int
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawCQE {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

bitflags! {
    #[repr(transparent)]
    pub struct SetupFlags: u32 {
        /// io_context is polled
        const IOPOLL        = 1 << 0;

        /// SQ poll thread
        const SQPOLL        = 1 << 1;

        /// sq_thread_cpu is valid
        const SQ_AFF        = 1 << 2;

        /// app defines CQ size
        const CQSIZE        = 1 << 3;

        /// clamp SQ/CQ ring sizes
        const CLAMP         = 1 << 4;

        /// attach to existing wq
        const ATTACH_WQ     = 1 << 5;

        /// start with ring disabled
        const R_DISABLED    = 1 << 6;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct FeatureFlags: u32 {
        const SINGLE_MMAP       = 1 << 0;
        const NODROP            = 1 << 1;
        const SUBMIT_STABLE     = 1 << 2;
        const RW_CUR_POS        = 1 << 3;
        const CUR_PERSONALITY   = 1 << 4;
        const FAST_POLL         = 1 << 5;
        const POLL_32BITS       = 1 << 6;
        const SQPOLL_NONFIXED   = 1 << 7;
        const EXT_ARG           = 1 << 8;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct EnterFlags: u32 {
        const GETEVENTS	= 1 << 0;
        const SQ_WAKEUP	= 1 << 1;
        const SQ_WAIT	= 1 << 2;
        const EXT_ARG	= 1 << 3;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct SQEFlags: u8 {
        /// use fixed fileset
        const FIXED_FILE        = 1 << 0;

        /// issue after inflight IO
        const IO_DRAIN          = 1 << 1;

        /// links next sqe
        const IO_LINK           = 1 << 2;

        /// like LINK, but stronger
        const IO_HARDLINK       = 1 << 3;

        /// always go async
        const ASYNC             = 1 << 4;

        /// select buffer from sqe->buf_group
        const BUFFER_SELECT     = 1 << 5;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct SQFlags: u32 {
        /// needs io_uring_enter wakeup
        const NEED_WAKEUP	    = 1 << 0;

        /// CQ ring is overflown
        const CQ_OVERFLOW	    = 1 << 1;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct OpFlags: u16 {
        const SUPPORTED         = 1 << 0;
    }
}

bitflags! {
    #[repr(transparent)]
    pub struct FsyncFlags: u32 {
        const FSYNC_DATASYNC    = 1 << 0;
    }
}

#[repr(C)]
pub struct RawProbeOp {
    pub op: u8,
    pub resv: u8,
    pub flags: OpFlags,
    pub resv2: u32,
}

#[repr(C)]
pub struct RawProbe {
    /// last opcode supported
    pub last_op: u8,
    /// length of ops[] array below
    pub ops_len: u8,
    pub resv: u16,
    pub resv2: [u32; 3],
    pub ops: [RawProbeOp; 0],
}

#[repr(u32)]
pub enum RegisterOp {
    RegisterBuffers = 0,
    UnregisterBuffers = 1,
    RegisterFiles = 2,
    UnregisterFiles = 3,
    RegisterEventfd = 4,
    UnregisterEventfd = 5,
    RegisterFilesUpdate = 6,
    RegisterEventfdAsync = 7,
    RegisterProbe = 8,
    RegisterPersonality = 9,
    UnregisterPersonality = 10,
    RegisterRestrictions = 11,
    RegisterEnableRings = 12,
}

#[repr(C)]
pub struct RawRingParams {
    pub sq_entries: u32,
    pub cq_entries: u32,
    pub flags: u32,
    pub sq_thread_cpu: u32,
    pub sq_thread_idle: u32,
    pub features: u32,
    pub wq_fd: u32,
    pub resv: [u32; 3],
    pub sq_off: RawSqOffsets,
    pub cq_off: RawCqOffsets,
}

#[repr(C)]
pub struct RawSqOffsets {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub flags: u32,
    pub dropped: u32,
    pub array: u32,
    pub resv1: u32,
    pub resv2: u64,
}

#[repr(C)]
pub struct RawCqOffsets {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub overflow: u32,
    pub cqes: u32,
    pub flags: u32,
    pub resv1: u32,
    pub resv2: u64,
}

pub const IORING_OFF_SQ_RING: u64 = 0;
pub const IORING_OFF_CQ_RING: u64 = 0x8000000;
pub const IORING_OFF_SQES: u64 = 0x10000000;

#[repr(C)]
pub struct RawSQE {
    opcode: RingOp,
    flags: SQEFlags,
    ioprio: u16,
    fd: i32,
    arg1: u64,
    arg2: u64,
    len: u32,
    cmd_flags: u32,
    user_data: u64,
    buf_index_or_group: u16,
    personality: u16,
    arg3: u32,
    pad: [u64; 2],
}

#[non_exhaustive]
#[repr(u8)]
pub enum RingOp {
    Nop,
    ReadV,
    WriteV,
    Fsync,
    ReadFixed,
    WriteFixed,
    PollAdd,
    PollRemove,
    SyncFileRange,
    SendMsg,
    RecvMsg,
    Timeout,
    TimeoutRemove,
    Accept,
    AsyncCancel,
    LinkTimeout,
    Connect,
    FAllocate,
    OpenAt,
    Close,
    FilesUpdate,
    Statx,
    Read,
    Write,
    FAdvise,
    MAdvise,
    Send,
    Recv,
    OpenAt2,
    EpollCtl,
    Splice,
    ProvideBuffers,
    RemoveBuffers,
    Tee,
    Shutdown,
    RenameAt,
    UnlinkAt,
    MkDirAt,
}

impl RawSQE {
    pub fn set_user_data(&mut self, user_data: u64) {
        self.user_data = user_data;
    }

    pub fn enable_flags(&mut self, flags: SQEFlags) {
        self.flags |= flags;
    }

    pub fn set_flags(&mut self, flags: SQEFlags) {
        self.flags = flags;
    }

    unsafe fn uninit_prep_rw(
        sqe: *mut Self,
        op: RingOp,
        fd: RawFd,
        addr: *const (),
        len: u32,
        offset: u64,
    ) {
        ptr::write(
            sqe,
            RawSQE {
                opcode: op,
                flags: SQEFlags::empty(),
                ioprio: 0,
                fd,
                arg1: offset,
                arg2: addr as u64,
                len,
                cmd_flags: 0,
                user_data: 0,
                buf_index_or_group: 0,
                personality: 0,
                arg3: 0,
                pad: [0, 0],
            },
        );
    }

    /// # Safety
    /// See [`SQE`](crate::sqe::SQE)
    pub unsafe fn prep_nop(sqe: *mut Self) {
        Self::uninit_prep_rw(sqe, RingOp::Nop, -1, ptr::null(), 0, 0)
    }

    /// # Safety
    /// See [`SQE`](crate::sqe::SQE)
    pub unsafe fn prep_readv(
        sqe: *mut Self,
        fd: RawFd,
        iovecs: *const libc::iovec,
        nr_vecs: u32,
        offset: libc::off_t,
    ) {
        Self::uninit_prep_rw(
            sqe,
            RingOp::ReadV,
            fd,
            iovecs.cast(),
            nr_vecs,
            offset as u64,
        );
    }

    /// # Safety
    /// See [`SQE`](crate::sqe::SQE)
    pub unsafe fn prep_writev(
        sqe: *mut Self,
        fd: RawFd,
        iovecs: *const libc::iovec,
        nr_vecs: u32,
        offset: libc::off_t,
    ) {
        Self::uninit_prep_rw(
            sqe,
            RingOp::WriteV,
            fd,
            iovecs.cast(),
            nr_vecs,
            offset as u64,
        )
    }

    /// # Safety
    /// See [`SQE`](crate::sqe::SQE)
    pub unsafe fn prep_fsync(sqe: *mut Self, fd: RawFd, flags: FsyncFlags) {
        Self::uninit_prep_rw(sqe, RingOp::Fsync, fd, ptr::null(), 0, 0);
        (*sqe).cmd_flags = flags.bits();
    }
}
