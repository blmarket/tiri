//! Session group members stay ordered while sessions remain owned globally.

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
            std::env::temp_dir().join(format!("tiri-session-group-{}-{stamp}", std::process::id()));
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
fn member_order_survives_add_remove_and_shared_window_refs() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "alpha", "sleep 30"]);
    let alpha_window = server.success(&["list-windows", "-t", "alpha", "-F", "#{window_id}"]);

    server.success(&["new-session", "-d", "-s", "beta", "-t", "alpha"]);
    server.success(&["new-session", "-d", "-s", "gamma", "-t", "beta"]);
    let group = server.success(&[
        "display-message",
        "-p",
        "-t",
        "alpha",
        "#{session_group_list}:#{session_group_size}",
    ]);
    assert_eq!(group, "alpha,beta,gamma:3\n");

    for name in ["beta", "gamma"] {
        let window = server.success(&["list-windows", "-t", name, "-F", "#{window_id}"]);
        assert_eq!(
            window, alpha_window,
            "group member {name} lost shared window"
        );
    }

    server.success(&["kill-session", "-t", "beta"]);
    let group = server.success(&[
        "display-message",
        "-p",
        "-t",
        "alpha",
        "#{session_group_list}:#{session_group_size}",
    ]);
    assert_eq!(group, "alpha,gamma:2\n");

    server.success(&["kill-session", "-t", "alpha"]);
    let remaining = server.success(&[
        "display-message",
        "-p",
        "-t",
        "gamma",
        "#{session_group_list}:#{session_group_size}",
    ]);
    assert_eq!(remaining, "gamma:1\n");
    let window = server.success(&["list-windows", "-t", "gamma", "-F", "#{window_id}"]);
    assert_eq!(window, alpha_window, "remaining member lost shared window");
}

#[test]
fn trait_rename_rekeys_session_without_changing_its_pane_and_rejects_duplicates() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "alpha", "sleep 30"]);
    server.success(&["new-session", "-d", "-s", "beta", "sleep 30"]);
    let identity = server.success(&[
        "display-message",
        "-p",
        "-t",
        "alpha",
        "#{session_id}:#{pane_id}",
    ]);
    server.success(&["rename-session", "-t", "alpha", "gamma"]);
    assert!(!server
        .run(&["has-session", "-t", "=alpha"])
        .status
        .success());
    assert_eq!(
        server.success(&[
            "display-message",
            "-p",
            "-t",
            "gamma",
            "#{session_id}:#{pane_id}",
        ]),
        identity
    );
    let duplicate = server.run(&["rename-session", "-t", "gamma", "beta"]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("duplicate session: beta"));
    server.success(&["has-session", "-t", "=gamma"]);
}

#[test]
fn last_window_target_observes_history_without_selecting_it() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "history", "sleep 30"]);
    server.success(&["new-window", "-t", "history:1", "sleep 30"]);
    server.success(&["select-window", "-t", "history:0"]);
    assert_eq!(
        server.success(&[
            "display-message",
            "-p",
            "-t",
            "history:!",
            "#{window_index}",
        ]),
        "1\n"
    );
    assert_eq!(
        server.success(&["display-message", "-p", "-t", "history", "#{window_index}",]),
        "0\n"
    );
    server.success(&["last-window", "-t", "history"]);
    assert_eq!(
        server.success(&["display-message", "-p", "-t", "history", "#{window_index}",]),
        "1\n"
    );
}

#[test]
fn break_pane_adopts_existing_process_instead_of_spawning_a_replacement() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "adopt", "sleep 30"]);
    let pane = server.success(&[
        "new-pane",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-t",
        "adopt:0",
        "sleep 30",
    ]);
    let pane = pane.trim();
    let pid = server.success(&["display-message", "-p", "-t", pane, "#{pane_pid}"]);
    server.success(&["break-pane", "-d", "-s", pane, "-t", "adopt:4"]);
    assert_eq!(
        server
            .success(&["display-message", "-p", "-t", "adopt:4", "#{pane_id}",])
            .trim(),
        pane
    );
    assert_eq!(
        server.success(&["display-message", "-p", "-t", "adopt:4", "#{pane_pid}",]),
        pid
    );
}
