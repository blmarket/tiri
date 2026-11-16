//! The boundary for operations whose implementation differs by operating
//! system.

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::Linux as CurrentPlatform;

#[cfg(target_os = "macos")]
mod darwin;

#[cfg(target_os = "macos")]
pub use darwin::Darwin as CurrentPlatform;

#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::os::fd::BorrowedFd;
use std::path::PathBuf;

/// Compile-time contract implemented by each supported operating system.
pub trait Platform {
    /// Return the current directory of the process occupying the foreground of
    /// the pane represented by `pty`.
    fn pane_cwd(pty: BorrowedFd<'_>) -> Option<PathBuf>;

    /// Return the name of the process occupying the foreground of the pane
    /// represented by `pty`, as this platform's tmux reports it.
    fn pane_name(pty: BorrowedFd<'_>) -> Option<OsString>;

    /// Full process argument vector, including `argv[0]`, when readable.
    fn process_arguments(_pid: u32) -> Vec<OsString> {
        Vec::new()
    }

    /// Whether `pid` is asleep waiting to read from a terminal, rather than
    /// running or waiting on something else. This separates a foreground
    /// command that has stopped to ask the user a question from one that is
    /// busy doing work.
    ///
    /// `None` means "cannot tell" — an unreadable process, or a platform with
    /// no way to ask. Callers must treat that as indistinguishable from
    /// running rather than inventing a wait.
    fn process_waiting_for_tty(_pid: u32) -> Option<bool> {
        None
    }
}
