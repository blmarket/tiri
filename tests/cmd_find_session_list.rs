//! Resolve a window shared by sessions through the live target finder.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let directory =
            std::env::temp_dir().join(format!("tiri-session-list-{}-{stamp}", std::process::id()));
        fs::create_dir(&directory).expect("create socket directory");
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
            .env("TMUX", "")
            .output()
            .expect("run tiri")
    }

    fn success(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 command output")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.run(&["kill-server"]);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn window_target_finds_a_member_session_before_and_after_linking() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "alpha", "sleep 30"]);
    let window_id = server.success(&["list-windows", "-t", "alpha", "-F", "#{window_id}"]);
    let window_id = window_id.trim();
    assert!(window_id.starts_with('@'), "window id: {window_id:?}");

    let selected = server.success(&["display-message", "-p", "-t", window_id, "#{session_name}"]);
    assert_eq!(selected, "alpha\n");

    server.success(&["new-session", "-d", "-s", "beta", "sleep 30"]);
    server.success(&["link-window", "-s", window_id, "-t", "beta:1"]);
    let selected = server.success(&["display-message", "-p", "-t", window_id, "#{session_name}"]);
    assert!(
        selected == "alpha\n" || selected == "beta\n",
        "selected: {selected:?}"
    );

    server.success(&["kill-session", "-t", "beta"]);
    let selected = server.success(&["display-message", "-p", "-t", window_id, "#{session_name}"]);
    assert_eq!(selected, "alpha\n");
}
