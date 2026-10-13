//! Exercise the choose-buffer preview through an attached terminal.

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
            "tiri-buffer-preview-{}-{stamp}",
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
            ws_row: 24,
            ws_col: 80,
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
                .args(["attach-session", "-t", "preview"])
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
        panic!("preview fragments {expected:?} missing from PTY output {output:?}");
    }
}

impl Drop for AttachedClient {
    fn drop(&mut self) {
        unsafe { libc::kill(self.pid, libc::SIGTERM) };
        // Drain the terminal while the client exits: a full PTY buffer (only
        // 1KiB on macOS) would block its final writes forever.
        let mut chunk = [0; 8192];
        while unsafe { libc::waitpid(self.pid, std::ptr::null_mut(), libc::WNOHANG) } == 0 {
            let _ = self.master.read(&mut chunk);
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn preview_renders_long_line_then_short_escaped_line() {
    let server = Server::new();
    server.command(&[
        "new-session",
        "-d",
        "-x",
        "80",
        "-y",
        "24",
        "-s",
        "preview",
        "sleep 30",
    ]);
    for index in 0..8 {
        server.command(&["set-buffer", "-b", &format!("older-{index}"), "old"]);
    }
    let contents = format!("{}\n\tshort", "A".repeat(70));
    server.command(&["set-buffer", "-b", "sample", &contents]);

    let mut client = AttachedClient::new(&server);
    client.read_until(&[b"[preview]"]);
    server.command(&["choose-buffer", "-t", "preview:0.0"]);
    let output = client.read_until(&[b" AAAAAAAAAA", br" \tshort"]);
    assert!(output
        .windows(b"sample (sort: creation)".len())
        .any(|window| { window == b"sample (sort: creation)" }));
}
