//! Exercise the trailing output copied after run-shell's complete lines.

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
        let directory = std::env::temp_dir().join(format!(
            "tiri-run-shell-partial-{}-{stamp}",
            std::process::id()
        ));
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
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.run(&["kill-server"]);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn run_shell_prints_trailing_bytes_and_stops_at_embedded_nul() {
    let server = Server::new();
    let create = server.run(&["new-session", "-d", "-s", "test", "sleep 30"]);
    assert!(create.status.success(), "new-session: {:?}", create.stderr);

    let output = server.run(&["run-shell", "printf 'complete\\npartial'"]);
    assert!(output.status.success(), "run-shell: {:?}", output.stderr);
    assert_eq!(output.stdout, b"complete\npartial\n");

    let output = server.run(&["run-shell", "-d", "0.01", "printf 'delayed'"]);
    assert!(
        output.status.success(),
        "delayed run-shell: {:?}",
        output.stderr
    );
    assert_eq!(output.stdout, b"delayed\n");

    let output = server.run(&["run-shell", "printf 'complete\\npartial\\000hidden'"]);
    assert!(output.status.success(), "run-shell: {:?}", output.stderr);
    assert_eq!(output.stdout, b"complete\npartial\n");
}

#[test]
fn delayed_background_command_survives_its_initiating_client() {
    let server = Server::new();
    let create = server.run(&["new-session", "-d", "-s", "test", "sleep 30"]);
    assert!(create.status.success(), "new-session: {:?}", create.stderr);

    let output = server.run(&[
        "run-shell",
        "-b",
        "-d",
        "0.05",
        "-C",
        "set-option -g @delayed-background done",
    ]);
    assert!(
        output.status.success(),
        "background run-shell: {:?}",
        output.stderr
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let output = server.run(&["show-options", "-gv", "@delayed-background"]);
        if output.status.success() && output.stdout == b"done\n" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "background command did not run"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
