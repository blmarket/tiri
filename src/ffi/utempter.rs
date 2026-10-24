//! Bounded calls into utempter.

use std::ffi::CStr;
use std::os::fd::{AsRawFd, BorrowedFd};

extern "C" {
    fn utempter_add_record(
        master_fd: ::core::ffi::c_int,
        hostname: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
    fn utempter_remove_record(master_fd: ::core::ffi::c_int) -> ::core::ffi::c_int;
}

/// Add a utmp record for the pty `master`, then signal ourselves so the
/// server reaps utempter's helper.
pub fn utempter_add(master: BorrowedFd<'_>, hostname: &CStr) {
    unsafe {
        utempter_add_record(master.as_raw_fd(), hostname.as_ptr());
        libc::kill(std::process::id() as libc::pid_t, libc::SIGCHLD);
    }
}

/// Remove the utmp record for the pty `master`; see [`utempter_add`].
pub fn utempter_remove(master: BorrowedFd<'_>) {
    unsafe {
        utempter_remove_record(master.as_raw_fd());
        libc::kill(std::process::id() as libc::pid_t, libc::SIGCHLD);
    }
}
