//! Pane process queries, delegated to the per-OS platform boundary.

use std::ffi::CString;
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStrExt;
use tiri_agent::platform::{CurrentPlatform, Platform};

/// The foreground process group's argv[0], as tmux's osdep_get_name.
pub(crate) fn osdep_get_name_cstring(fd: BorrowedFd<'_>) -> Option<CString> {
    let pgrp = tiri_rt::unix::terminal_foreground_group(fd)
        .ok()
        .filter(|&pgrp| pgrp > 0)?;
    let program = CurrentPlatform::process_arguments(pgrp as u32)
        .into_iter()
        .next()?;
    CString::new(program.as_bytes()).ok()
}

/// The foreground process group's (or session leader's) working directory,
/// as tmux's osdep_get_cwd.
pub(crate) fn osdep_get_cwd(fd: BorrowedFd<'_>) -> Option<CString> {
    let path = CurrentPlatform::pane_cwd(fd)?;
    CString::new(path.as_os_str().as_bytes()).ok()
}
