#[cfg(target_os = "linux")]
use crate::src::ffi::utempter::{utempter_add_record, utempter_remove_record};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};

/// utempter looks the record up by the master descriptor, so the record is
/// removed before the descriptor closes.
pub(crate) struct PanePty {
    fd: OwnedFd,
    utmp: bool,
}

impl PanePty {
    pub(super) fn new(fd: OwnedFd) -> Self {
        Self { fd, utmp: false }
    }

    /// Add a utmp record named `tmux(<pid>).%<id>` unless one exists.
    #[cfg(target_os = "linux")]
    pub(super) fn utmp_add(&mut self, id: u32) {
        if self.utmp {
            return;
        }
        unsafe {
            let record = std::ffi::CString::new(format!("tmux({}).%{}", getpid(), id))
                .expect("pane login record contains no NUL");
            utempter_add_record(self.fd.as_raw_fd(), record.as_ptr());
            // utempter forks its helper; reap it through the server's handler.
            libc::kill(libc::getpid(), libc::SIGCHLD);
        }
        self.utmp = true;
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn utmp_add(&mut self, _id: u32) {}

    pub(super) fn utmp_remove(&mut self) {
        if !self.utmp {
            return;
        }
        #[cfg(target_os = "linux")]
        unsafe {
            utempter_remove_record(self.fd.as_raw_fd());
            libc::kill(libc::getpid(), libc::SIGCHLD);
        }
        self.utmp = false;
    }
}

impl Drop for PanePty {
    fn drop(&mut self) {
        self.utmp_remove();
    }
}

impl AsFd for PanePty {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl AsRawFd for PanePty {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
