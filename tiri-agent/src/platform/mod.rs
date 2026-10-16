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
use std::time::SystemTime;

/// One process table record visible to the current platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
}

/// Compile-time contract implemented by each supported operating system.
pub trait Platform {
    /// Return the current directory of the process occupying the foreground of
    /// the pane represented by `pty`.
    fn pane_cwd(pty: BorrowedFd<'_>) -> Option<PathBuf>;

    /// Return the visible `(pid, parent pid)` process table, when available.
    fn process_table() -> Option<Vec<ProcessInfo>> {
        None
    }

    /// Candidate executable/script names for `pid`, most specific first.
    fn process_programs(_pid: u32) -> Vec<OsString> {
        Vec::new()
    }

    /// Full process argument vector, including `argv[0]`, when readable.
    fn process_arguments(_pid: u32) -> Vec<OsString> {
        Vec::new()
    }

    /// The working directory of `pid`, when readable. This reads the *target*
    /// process's cwd (e.g. an agent running in a pane), not the server's own —
    /// used to locate an agent's per-project session state. `None` when the
    /// process is unreadable or the platform does not support inspection.
    fn process_cwd(_pid: u32) -> Option<PathBuf> {
        None
    }

    /// Paths currently referenced by `pid`'s open file descriptors. Empty when
    /// the process is unreadable or the platform does not support inspection.
    fn process_open_files(_pid: u32) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Wall-clock time at which `pid` began running, when readable. Used to
    /// date session state against the process that would have written it.
    /// `None` when the process is unreadable or the platform does not support
    /// inspection.
    fn process_start_time(_pid: u32) -> Option<SystemTime> {
        None
    }

    /// `pid`'s environment as `(name, value)` pairs. Empty when the process is
    /// unreadable or the platform does not support inspection.
    fn process_environ(_pid: u32) -> Vec<(OsString, OsString)> {
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
