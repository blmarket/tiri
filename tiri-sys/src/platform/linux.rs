//! Linux implementation of the operating-system boundary.

use std::ffi::OsString;
use std::fs;
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use super::Platform;

/// The Linux platform implementation selected by the native server.
pub struct Linux;

impl Platform for Linux {
    fn pane_cwd(pty: BorrowedFd<'_>) -> Option<PathBuf> {
        let read_cwd = |pid: libc::pid_t| {
            (pid > 0)
                .then(|| PathBuf::from(format!("/proc/{pid}/cwd")))
                .and_then(|path| fs::read_link(path).ok())
        };
        let foreground_pgrp = crate::terminal_foreground_group(pty).unwrap_or(-1);
        read_cwd(foreground_pgrp).or_else(|| read_cwd(crate::terminal_session(pty).unwrap_or(-1)))
    }

    fn pane_name(pty: BorrowedFd<'_>) -> Option<OsString> {
        let foreground_pgrp = crate::terminal_foreground_group(pty).ok()?;
        let cmdline = fs::read(format!("/proc/{foreground_pgrp}/cmdline")).ok()?;
        let name = cmdline.split(|&byte| byte == 0).next()?;
        (!name.is_empty()).then(|| OsString::from_vec(name.to_vec()))
    }

    fn process_arguments(pid: u32) -> Vec<OsString> {
        fs::read(format!("/proc/{pid}/cmdline"))
            .map(|cmdline| {
                cmdline
                    .split(|byte| *byte == 0)
                    .filter(|arg| !arg.is_empty())
                    .map(|arg| OsString::from_vec(arg.to_vec()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn process_waiting_for_tty(pid: u32) -> Option<bool> {
        let wchan = fs::read_to_string(format!("/proc/{pid}/wchan")).ok()?;
        if !TTY_READ_WCHAN.contains(&wchan.trim()) {
            return Some(false);
        }
        let syscall = fs::read_to_string(format!("/proc/{pid}/syscall")).ok()?;
        let Some(descriptor) = syscall
            .split_whitespace()
            .nth(1)
            .and_then(|argument| argument.strip_prefix("0x"))
            .and_then(|argument| u32::from_str_radix(argument, 16).ok())
        else {
            return Some(false);
        };
        let Ok(target) = fs::read_link(format!("/proc/{pid}/fd/{descriptor}")) else {
            return Some(false);
        };
        Some(target.to_str().is_some_and(|target| {
            target.starts_with("/dev/pts/") || target.starts_with("/dev/tty")
        }))
    }
}

/// The `wchan` symbols a task blocked reading a terminal parks in.
/// `wait_woken` is what current kernels report for `n_tty_read`; the two named
/// functions are what older ones reported directly.
const TTY_READ_WCHAN: [&str; 3] = ["wait_woken", "n_tty_read", "tty_read"];

#[cfg(test)]
mod tests {
    use std::io;
    use std::ptr;
    use std::time::Duration;

    use super::{Linux, Platform};

    /// Fork a child onto the slave end of a fresh pty and run `probe` against
    /// it once it has had a moment to reach its steady state. `body` runs in
    /// the child and must never return.
    fn with_pty_child(body: unsafe fn() -> !, probe: impl FnOnce(u32)) {
        let (mut master, mut slave) = (-1, -1);
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
            )
        };
        assert_eq!(opened, 0, "openpty: {}", io::Error::last_os_error());

        let child = unsafe { libc::fork() };
        assert!(child >= 0, "fork: {}", io::Error::last_os_error());
        if child == 0 {
            unsafe {
                libc::close(master);
                libc::dup2(slave, 0);
                libc::setsid();
                libc::ioctl(slave, libc::TIOCSCTTY, 0);
                body();
            }
        }

        std::thread::sleep(Duration::from_millis(250));
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            probe(child as u32);
        }));

        unsafe {
            libc::kill(child, libc::SIGKILL);
            let mut status = 0;
            libc::waitpid(child, &mut status, 0);
            libc::close(slave);
            libc::close(master);
        }
        if let Err(panic) = observed {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn a_process_parked_in_a_terminal_read_is_reported_as_waiting() {
        unsafe fn read_stdin_forever() -> ! {
            let mut byte = 0u8;
            loop {
                unsafe { libc::read(0, (&mut byte as *mut u8).cast(), 1) };
            }
        }

        with_pty_child(read_stdin_forever, |pid| {
            assert_eq!(Linux::process_waiting_for_tty(pid), Some(true));
        });
    }

    /// The reading above is only worth anything if work does not look like it.
    /// A spinning process is the plain case, and a process asleep on something
    /// that is not the terminal — a timer — is the one `wchan` alone would get
    /// wrong, since it parks in a different symbol than a terminal read does.
    #[test]
    fn a_busy_or_otherwise_sleeping_process_is_not_reported_as_waiting() {
        fn spin_forever() -> ! {
            loop {
                std::hint::spin_loop();
            }
        }

        unsafe fn sleep_forever() -> ! {
            loop {
                unsafe { libc::sleep(30) };
            }
        }

        with_pty_child(spin_forever, |pid| {
            assert_eq!(Linux::process_waiting_for_tty(pid), Some(false));
        });
        with_pty_child(sleep_forever, |pid| {
            assert_eq!(Linux::process_waiting_for_tty(pid), Some(false));
        });
    }
}
