//! OSC 133 status parsing through a real pane and the format callback.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Server {
    binary: PathBuf,
    directory: PathBuf,
    socket: PathBuf,
}

impl Server {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("tiri-osc133-{}-{stamp}", std::process::id()));
        fs::create_dir(&directory).expect("create private socket directory");
        let socket = directory.join("socket");
        Self {
            binary: PathBuf::from(env!("CARGO_BIN_EXE_tiri")),
            directory,
            socket,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.binary)
            .args(["-f", "/dev/null", "-S"])
            .arg(&self.socket)
            .args(args)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env("TMUX", "")
            .env("LC_ALL", "C")
            .output()
            .expect("run tiri")
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

#[test]
fn pane_reports_osc_133_exit_status_with_following_parameters() {
    let server = Server::new();
    let create = server.run(&[
        "new-session",
        "-d",
        "-s",
        "osc133-test",
        "printf '\\033]133;D;42;k=v\\007'; sleep 30",
    ]);
    assert!(
        create.status.success(),
        "new-session failed: {:?}",
        create.stderr
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = server.run(&["display-message", "-p", "#{pane_command_status}"]);
        assert!(
            status.status.success(),
            "display-message failed: {:?}",
            status.stderr
        );
        if status.stdout == b"42\n" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "OSC 133 status was not reported: {:?}",
            status.stdout
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn clearing_a_line_preserves_shell_markers_and_their_columns() {
    let server = Server::new();
    let create = server.run(&[
        "new-session",
        "-d",
        "-s",
        "osc133-clear",
        "printf '\\033]133;A\\007p>\\033]133;B\\007run\\033]133;C\\007out\\033]133;D;7\\007\\033[2K'; sleep 30",
    ]);
    assert!(create.status.success(), "{:?}", create.stderr);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let capture = server.run(&["capture-pane", "-p", "-R"]);
        assert!(capture.status.success(), "{:?}", capture.stderr);
        let text = String::from_utf8(capture.stdout).expect("ASCII grid diagnostic");
        let first = text.lines().nth(1).expect("first grid row");
        if first.contains("0/0 osc133=0,2,5,8,7") {
            assert!(first.contains("flags=START_PROMPT,START_COMMAND,START_OUTPUT,END_OUTPUT["));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "shell markers lost after clear: {first}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
