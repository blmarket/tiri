//! Authoritative job declarations.

use super::abi::pid_t;
use std::cell::RefCell;
use std::rc::Rc;
/// Every live job, newest first. The server thread owns them; observers never
/// retain a process or stream.
pub struct jobs {
    pub(crate) storage: Vec<refbox::RefBox<job>>,
}

pub const JOB_NOWAIT: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

pub const JOB_SHOWSTDERR: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;

pub const JOB_KEEPWRITE: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;

pub const JOB_PTY: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;

pub const JOB_DEFAULTSHELL: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;

pub struct job {
    pub state: job_state,
    pub flags: ::core::ffi::c_int,
    pub cmd: Option<std::ffi::CString>,
    pub pid: pid_t,
    pub tty: [u8; 32],
    pub status: std::process::ExitStatus,
    pub fd: Option<std::os::fd::OwnedFd>,
    pub event: crate::reactor::StreamHandle,
    pub updatecb: job_update_cb,
    pub completecb: job_complete_cb,
    pub freecb: job_free_cb,
}

impl job {
    pub fn empty() -> Self {
        Self {
            state: Default::default(),
            flags: Default::default(),
            cmd: Default::default(),
            pid: Default::default(),
            tty: Default::default(),
            status: Default::default(),
            fd: Default::default(),
            event: Default::default(),
            updatecb: Default::default(),
            completecb: Default::default(),
            freecb: Default::default(),
        }
    }
}

pub type job_update_cb = Option<Rc<RefCell<Option<Box<dyn FnMut(&refbox::Weak<job>)>>>>>;

pub fn job_update_callback(callback: impl FnMut(&refbox::Weak<job>) + 'static) -> job_update_cb {
    Some(Rc::new(RefCell::new(Some(Box::new(callback)))))
}

pub struct JobCompletion {
    pub status: std::process::ExitStatus,
    pub output: Vec<u8>,
}

pub type job_complete_cb = Option<Box<dyn FnOnce(JobCompletion)>>;

pub type job_free_cb = Option<Box<dyn FnOnce()>>;

pub type job_state = ::core::ffi::c_uint;
