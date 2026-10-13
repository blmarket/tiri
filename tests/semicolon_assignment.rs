//! A shell-style assignment after `;` keeps the commands before it.

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
            "tiri-semicolon-assignment-{}-{stamp}",
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
            .current_dir(&self.directory)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env_remove("TMUX")
            .env("LC_ALL", "C")
            .output()
            .expect("run tiri")
    }

    fn source_line(&self, line: &str) {
        let path = self.directory.join("case.conf");
        fs::write(&path, format!("{line}\n")).expect("write config line");
        let output = self.run(&["source-file", path.to_str().expect("UTF-8 path")]);
        assert!(output.status.success(), "source-file {line:?}: {output:?}");
    }

    fn option(&self, option: &str) -> Vec<u8> {
        let output = self.run(&["show-options", "-gqv", option]);
        assert!(output.status.success(), "show-options {option}: {output:?}");
        output.stdout
    }

    fn environment(&self, name: &str) -> Vec<u8> {
        let output = self.run(&["show-environment", "-g", name]);
        assert!(
            output.status.success(),
            "show-environment {name}: {output:?}"
        );
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
fn commands_before_an_assignment_after_a_semicolon_run() {
    let server = Server::new();
    let created = server.run(&["new-session", "-d", "-s", "assignment", "sleep 30"]);
    assert!(created.status.success(), "new-session: {created:?}");

    for (line, options) in [
        ("set -g @a 1 ; A=1", &["@a"][..]),
        (
            "set -g @b 1 ; set -g @c 1 ; B=1 ; set -g @d 1",
            &["@b", "@c", "@d"][..],
        ),
        ("C=1 ; set -g @e 1", &["@e"][..]),
        ("set -g @f 1 ; set -g @g 1", &["@f", "@g"][..]),
    ] {
        server.source_line(line);
        for option in options {
            assert_eq!(server.option(option), b"1\n", "{option} after {line:?}");
        }
    }

    for name in ["A", "B", "C"] {
        assert_eq!(
            server.environment(name),
            format!("{name}=1\n").into_bytes(),
            "assignment {name}"
        );
    }
}
