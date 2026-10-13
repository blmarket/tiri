//! Exercise the value paths used by show-options through a live server.

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
            "tiri-show-options-value-{}-{stamp}",
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
            .env_remove("TMUX")
            .env("LC_ALL", "C")
            .output()
            .expect("run tiri")
    }

    fn successful(&self, args: &[&str]) -> Vec<u8> {
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
fn scalar_empty_array_and_recursive_array_values() {
    let server = Server::new();
    server.successful(&["new-session", "-d", "-s", "show-values", "sleep 30"]);

    server.successful(&["set-option", "-g", "@owner", "text \\ $ #[]"]);
    assert_eq!(
        server.successful(&["show-options", "-g", "-F", "#{option_value}", "@owner"]),
        b"text \\ $ #[]\n"
    );
    server.successful(&["set-option", "-g", "status-interval", "17"]);
    assert_eq!(
        server.successful(&[
            "show-options",
            "-g",
            "-F",
            "#{option_value}",
            "status-interval"
        ]),
        b"17\n"
    );

    server.successful(&["set-option", "-g", "status-format", ""]);
    assert_eq!(
        server.successful(&["show-options", "-g", "status-format"]),
        b"status-format\n"
    );
    assert!(server
        .successful(&["show-options", "-gv", "status-format"])
        .is_empty());
    assert_eq!(
        server.successful(&[
            "show-options",
            "-g",
            "-F",
            "#{option_has_value}:#{option_value}",
            "status-format"
        ]),
        b"0:\n"
    );

    server.successful(&["set-option", "-g", "status-format[5]", "five"]);
    server.successful(&["set-option", "-g", "status-format[9]", "nine"]);
    assert_eq!(
        server.successful(&["show-options", "-g", "status-format"]),
        b"status-format[5] five\nstatus-format[9] nine\n"
    );
    assert_eq!(
        server.successful(&["show-options", "-g", "status-format[9]"]),
        b"status-format[9] nine\n"
    );
}
