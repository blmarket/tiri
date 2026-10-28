//! Pane process queries, delegated to the per-OS platform boundary.

use std::ffi::CString;
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStrExt;
use tiri_sys::platform::{CurrentPlatform, Platform};

/// The foreground process's name, as tmux's osdep_get_name.
pub(crate) fn osdep_get_name_cstring(fd: BorrowedFd<'_>) -> Option<CString> {
    let name = CurrentPlatform::pane_name(fd)?;
    CString::new(name.as_bytes()).ok()
}

/// The foreground process group's (or session leader's) working directory,
/// as tmux's osdep_get_cwd.
pub(crate) fn osdep_get_cwd(fd: BorrowedFd<'_>) -> Option<CString> {
    let path = CurrentPlatform::pane_cwd(fd)?;
    CString::new(path.as_os_str().as_bytes()).ok()
}
