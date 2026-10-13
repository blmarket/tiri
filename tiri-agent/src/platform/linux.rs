//! Linux implementation of the operating-system boundary.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{ForkOutcome, Platform, ProcessInfo};

/// The Linux platform implementation selected by the native server.
pub struct Linux;

impl Platform for Linux {
    type OutputWakeup = tiri_rt::unix::Notification;

    fn new_output_wakeup() -> io::Result<Self::OutputWakeup> {
        tiri_rt::unix::Notification::new()
    }

    unsafe fn fork_pty(size: libc::winsize) -> io::Result<ForkOutcome> {
        let mut master = -1;
        let pid = unsafe { libc::forkpty(&mut master, ptr::null_mut(), ptr::null(), &size) };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            return Ok(ForkOutcome::Child);
        }
        Ok(ForkOutcome::Parent {
            pid,
            master: unsafe { OwnedFd::from_raw_fd(master) },
        })
    }

    unsafe fn close_fds_from(lowest: RawFd) {
        unsafe { tiri_rt::unix::close_from(lowest) };
    }

    fn pane_cwd(pty: BorrowedFd<'_>) -> Option<PathBuf> {
        // Mirrors tmux's osdep_get_cwd: prefer the foreground process group,
        // then fall back to the session leader. The group id is only a pid
        // while the group leader lives; a job whose leader has exited (a shell
        // pipeline, or a wrapper that exec'd away) leaves a group whose id
        // names no process, and /proc/<pgrp>/cwd is then unreadable. The
        // session leader is the pane's own shell, so it still answers.
        let read_cwd = |pid: libc::pid_t| {
            (pid > 0)
                .then(|| PathBuf::from(format!("/proc/{pid}/cwd")))
                .and_then(|path| fs::read_link(path).ok())
        };
        let foreground_pgrp = tiri_rt::unix::terminal_foreground_group(pty).unwrap_or(-1);
        read_cwd(foreground_pgrp)
            .or_else(|| read_cwd(tiri_rt::unix::terminal_session(pty).unwrap_or(-1)))
    }

    fn peer_uid(socket: BorrowedFd<'_>) -> Option<u32> {
        tiri_rt::unix::peer_credentials(socket)
            .ok()
            .map(|(uid, _)| uid)
    }

    fn process_table() -> Option<Vec<ProcessInfo>> {
        let entries = fs::read_dir("/proc").ok()?;
        let mut table = Vec::new();
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            if let Some(ppid) = read_ppid(pid) {
                table.push(ProcessInfo { pid, ppid });
            }
        }
        Some(table)
    }

    fn process_programs(pid: u32) -> Vec<OsString> {
        let mut programs = Vec::new();

        if let Ok(comm) = fs::read_to_string(format!("/proc/{pid}/comm")) {
            let comm = comm.trim();
            if !comm.is_empty() {
                programs.push(OsString::from(comm));
            }
        }

        if let Some(program) = Self::process_arguments(pid).first() {
            programs.push(program.clone());
        }

        programs
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

    fn process_cwd(pid: u32) -> Option<PathBuf> {
        fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    fn process_open_files(pid: u32) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| fs::read_link(entry.path()).ok())
            .collect()
    }

    fn process_start_time(pid: u32) -> Option<SystemTime> {
        let since_boot = read_start_ticks(pid)? as f64 / clock_ticks_per_second()? as f64;
        boot_time()?.checked_add(Duration::from_secs_f64(since_boot))
    }

    fn process_environ(pid: u32) -> Vec<(OsString, OsString)> {
        use std::os::unix::ffi::OsStrExt;
        let Ok(environ) = fs::read(format!("/proc/{pid}/environ")) else {
            return Vec::new();
        };
        environ
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .filter_map(|entry| {
                // Only the first `=` separates name from value; values may
                // contain any number of them.
                let split = entry.iter().position(|byte| *byte == b'=')?;
                Some((
                    OsStr::from_bytes(&entry[..split]).to_owned(),
                    OsStr::from_bytes(&entry[split + 1..]).to_owned(),
                ))
            })
            .collect()
    }

    fn process_waiting_for_tty(pid: u32) -> Option<bool> {
        // Two readings have to agree, because neither is conclusive alone.
        //
        // `/proc/PID/wchan` names the kernel function the task is parked in.
        // A tty read parks in `wait_woken` — but so does a socket read, so the
        // symbol only narrows the field to "asleep in a driver read".
        //
        // `/proc/PID/syscall` then says which descriptor that read is on: for
        // the read-family calls a tty read can be sitting in, the first
        // argument is the file descriptor. Resolving it through `fd/` tells a
        // terminal apart from a socket, which is what the symbol could not.
        //
        // A process waiting on the terminal through `poll`/`select` instead
        // parks in `poll_schedule_timeout` and passes a pointer rather than a
        // descriptor, so it reads as running. That is the conservative
        // direction: a busy pane is never mistaken for one waiting on you.
        let wchan = fs::read_to_string(format!("/proc/{pid}/wchan")).ok()?;
        if !TTY_READ_WCHAN.contains(&wchan.trim()) {
            return Some(false);
        }
        let syscall = fs::read_to_string(format!("/proc/{pid}/syscall")).ok()?;
        // "running" for a task on a cpu, "-1 ..." when the registers are gone.
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

/// Seconds since the epoch at which the kernel booted, from `/proc/stat`'s
/// `btime` line. Process start times are recorded relative to this instant.
fn boot_time() -> Option<SystemTime> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let seconds: u64 = stat
        .lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// Parse the start time from `/proc/<pid>/stat`, in clock ticks since boot.
/// The `comm` field can contain spaces and parentheses, so fields are read
/// after the final `)`: the tokens there begin at `state`, making `starttime`
/// (field 22 overall) the twentieth.
fn read_start_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

fn clock_ticks_per_second() -> Option<u64> {
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (ticks > 0).then_some(ticks as u64)
}

/// Parse the parent pid from `/proc/<pid>/stat`. The `comm` field can contain
/// spaces and parentheses, so fields are read after the final `)`: the tokens
/// there are `state ppid ...`, making ppid the second one.
fn read_ppid(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn process_open_files_reports_current_process_file() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("tiri-open-file-{}-{nonce}", std::process::id()));
        let file = File::create(&path).expect("create temporary file");

        let open_files = Linux::process_open_files(std::process::id());
        assert!(open_files.contains(&path), "open files: {open_files:?}");

        drop(file);
        fs::remove_file(path).expect("remove temporary file");
    }

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

        // SAFETY: the child branch touches only async-signal-safe libc calls
        // and terminates in `_exit`, which `body` is required to guarantee.
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

        // The child has to reach its read (or its loop) before the probe means
        // anything; a freshly forked task is briefly running either way.
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
