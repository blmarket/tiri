//! Pane process queries, delegated to the per-OS platform boundary.

use std::ffi::CString;
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStrExt;
use tiri_agent::platform::{CurrentPlatform, Platform};

/// The foreground process group's argv[0], as tmux's osdep_get_name.
pub(crate) unsafe fn osdep_get_name_cstring(fd: ::core::ffi::c_int) -> Option<CString> {
    if fd < 0 {
        return None;
    }
    let pgrp = tiri_rt::unix::terminal_foreground_group(BorrowedFd::borrow_raw(fd))
        .ok()
        .filter(|&pgrp| pgrp > 0)?;
    let program = CurrentPlatform::process_arguments(pgrp as u32)
        .into_iter()
        .next()?;
    CString::new(program.as_bytes()).ok()
}

/// The foreground process group's (or session leader's) working directory,
/// as tmux's osdep_get_cwd.
pub(crate) unsafe fn osdep_get_cwd(fd: ::core::ffi::c_int) -> Option<CString> {
    if fd < 0 {
        return None;
    }
    let path = CurrentPlatform::pane_cwd(BorrowedFd::borrow_raw(fd))?;
    CString::new(path.as_os_str().as_bytes()).ok()
}
