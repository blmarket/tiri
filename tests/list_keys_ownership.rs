//! Exercise the combined prefix and root key listing through a live server.

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
            std::env::temp_dir().join(format!("tiri-list-keys-{}-{stamp}", std::process::id()));
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
            .env_remove("TMUX")
            .output()
            .expect("run tiri")
    }

    fn success(&self, args: &[&str]) -> Vec<u8> {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        output.stdout
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
fn notes_listing_combines_tables_filters_and_repeats() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "keys", "sleep 30"]);
    server.success(&["unbind-key", "-a", "-T", "prefix"]);
    server.success(&["unbind-key", "-a", "-T", "root"]);
    server.success(&[
        "bind-key",
        "-T",
        "prefix",
        "-N",
        "prefix-note",
        "C-a",
        "display-message",
        "prefix",
    ]);
    server.success(&[
        "bind-key",
        "-T",
        "root",
        "-N",
        "root-note",
        "C-b",
        "display-message",
        "root",
    ]);

    let listing = || {
        server.success(&[
            "list-keys",
            "-N",
            "-F",
            "#{key_table}|#{key_string}|#{key_note}",
        ])
    };
    let expected = b"prefix|C-a|prefix-note\nroot|C-b|root-note\n";
    assert_eq!(listing(), expected);
    assert_eq!(listing(), expected);

    let all = server.success(&["list-keys", "-F", "#{key_table}|#{key_string}"]);
    assert!(all
        .windows(b"prefix|C-a\n".len())
        .any(|row| row == b"prefix|C-a\n"));
    assert!(all
        .windows(b"root|C-b\n".len())
        .any(|row| row == b"root|C-b\n"));

    let filtered = server.success(&[
        "list-keys",
        "-N",
        "-F",
        "#{key_table}|#{key_string}|#{key_note}",
        "C-b",
    ]);
    assert_eq!(filtered, b"root|C-b|root-note\n");

    let table = server.success(&[
        "list-keys",
        "-N",
        "-T",
        "prefix",
        "-F",
        "#{key_table}|#{key_string}|#{key_note}",
    ]);
    assert_eq!(table, b"prefix|C-a|prefix-note\n");

    server.success(&["bind-key", "-T", "prefix", "-N", "updated-note", "C-a"]);
    assert_eq!(listing(), b"prefix|C-a|updated-note\nroot|C-b|root-note\n");

    server.success(&["unbind-key", "-a", "-T", "root"]);
    assert_eq!(listing(), b"prefix|C-a|updated-note\n");
    server.success(&["unbind-key", "-a", "-T", "prefix"]);
    assert_eq!(listing(), b"");
}
