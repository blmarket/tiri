//! Prompt callbacks may destroy their own owners during dispatch.

#![cfg(unix)]

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Server {
    directory: PathBuf,
    socket: PathBuf,
}

impl Server {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tiri-mode-prompt-cleanup-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create private directory");
        let socket = directory.join("socket");
        Self { directory, socket }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tiri"))
            .args(["-f", "/dev/null", "-S"])
            .arg(&self.socket)
            .args(args)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env("LC_ALL", "C")
            .env_remove("TMUX")
            .output()
            .expect("run tiri command")
    }

    fn command(&self, args: &[&str]) -> Output {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        output
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.socket.exists() {
            let _ = self.run(&["kill-server"]);
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct AttachedClient {
    master: File,
    pid: libc::pid_t,
}

impl AttachedClient {
    fn new(server: &Server) -> Self {
        let mut fd = -1;
        let mut size = libc::winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pid = unsafe {
            libc::forkpty(
                &mut fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        assert!(pid >= 0, "forkpty: {}", std::io::Error::last_os_error());
        if pid == 0 {
            let error = Command::new(env!("CARGO_BIN_EXE_tiri"))
                .args(["-f", "/dev/null", "-S"])
                .arg(&server.socket)
                .args(["attach-session", "-t", "mode:0.0"])
                .env("TERM", "xterm-256color")
                .env("SHELL", "/bin/sh")
                .env("LC_ALL", "C")
                .env_remove("TMUX")
                .exec();
            eprintln!("attach-session exec failed: {error}");
            unsafe { libc::_exit(127) };
        }
        let master = unsafe { File::from_raw_fd(fd) };
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0, "get PTY flags");
        assert!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                >= 0,
            "set PTY nonblocking"
        );
        Self { master, pid }
    }

    fn read_until(&mut self, expected: &[&[u8]]) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        while Instant::now() < deadline {
            let mut chunk = [0; 8192];
            match self.master.read(&mut chunk) {
                Ok(n) if n > 0 => output.extend_from_slice(&chunk[..n]),
                Ok(_) | Err(_)
                    if expected
                        .iter()
                        .all(|part| output.windows(part.len()).any(|window| window == *part)) =>
                {
                    return output
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                result => panic!("reading attached client: {result:?}"),
            }
            if expected
                .iter()
                .all(|part| output.windows(part.len()).any(|window| window == *part))
            {
                return output;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("terminal fragments {expected:?} missing from PTY output {output:?}");
    }
}

impl Drop for AttachedClient {
    fn drop(&mut self) {
        unsafe { libc::kill(self.pid, libc::SIGTERM) };
        let mut chunk = [0; 8192];
        while unsafe { libc::waitpid(self.pid, std::ptr::null_mut(), libc::WNOHANG) } == 0 {
            let _ = self.master.read(&mut chunk);
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn check_destruction(automatic: bool) {
    for (steps, tagged, label, expected) in [
        (5, false, "Kill pane 0?", "keep:0:1\nmode:0:1\nmode:1:1\n"),
        (4, false, "Kill window 0?", "keep:0:1\nmode:1:1\n"),
        (3, false, "Kill session mode?", "keep:0:1\n"),
        (4, true, "Kill 1 tagged?", "keep:0:1\nmode:1:1\n"),
    ] {
        let server = Server::new();
        server.command(&["new-session", "-d", "-s", "keep", "sleep 60"]);
        server.command(&["new-session", "-d", "-s", "mode", "sleep 60"]);
        server.command(&["new-window", "-d", "-t", "mode:1", "sleep 60"]);
        server.command(&["new-pane", "-d", "-t", "mode:0", "sleep 60"]);
        let mut client = AttachedClient::new(&server);
        client.read_until(&[b"[mode]"]);
        let output = server.command(&["list-clients", "-F", "#{client_tty}"]);
        let tty = String::from_utf8(output.stdout).unwrap();
        let tty = tty.trim();
        assert!(!tty.is_empty());
        let mut args = vec!["choose-tree", "-F", "PROMPT-CLEANUP-ROW", "-t", "mode:0.0"];
        if automatic {
            args.push("-y");
        }
        server.command(&args);
        client.read_until(&[b"PROMPT-CLEANUP-ROW"]);
        let mut args = vec!["send-keys", "-K", "-c", tty, "Home"];
        args.extend(std::iter::repeat_n("Down", steps));
        if tagged {
            args.push("t");
        }
        args.push(if tagged { "X" } else { "x" });
        server.command(&args);
        if !automatic {
            client.read_until(&[label.as_bytes()]);
            server.command(&["send-keys", "-K", "-c", tty, "y"]);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let output = server.command(&[
                "list-windows",
                "-a",
                "-F",
                "#{session_name}:#{window_index}:#{window_panes}",
            ]);
            if output.stdout == expected.as_bytes() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{label} (automatic={automatic}): {:?}",
                output
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn confirmed_prompts_can_destroy_their_own_mode() {
    check_destruction(false);
}

#[test]
fn queued_prompt_acceptance_can_destroy_its_own_mode() {
    check_destruction(true);
}
