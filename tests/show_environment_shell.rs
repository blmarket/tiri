//! Exercise shell-form environment output through a live server.

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
            "tiri-show-environment-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create socket directory");
        let socket = directory.join("socket");
        Self { directory, socket }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tiri"));
        command
            .args(["-f", "/dev/null", "-S"])
            .arg(&self.socket)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env_remove("TMUX")
            .env("LC_ALL", "C");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run tiri")
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
fn shell_output_escapes_bytes_and_distinguishes_empty_from_cleared() {
    let server = Server::new();
    let created = server.run(&["new-session", "-d", "-s", "show-env", "sleep 30"]);
    assert!(
        created.status.success(),
        "new-session: {:?}",
        created.stderr
    );

    let set = server.run(&["set-environment", "-g", "OWNER_VALUE", "plain$`\"\\"]);
    assert!(set.status.success(), "set-environment: {:?}", set.stderr);
    let shown = server.run(&["show-environment", "-gs", "OWNER_VALUE"]);
    assert!(
        shown.status.success(),
        "show-environment: {:?}",
        shown.stderr
    );
    assert_eq!(
        shown.stdout,
        b"OWNER_VALUE=\"plain\\$\\`\\\"\\\\\"; export OWNER_VALUE;\n"
    );

    let empty = server.run(&["set-environment", "-g", "OWNER_EMPTY", ""]);
    assert!(empty.status.success(), "set empty: {:?}", empty.stderr);
    let shown_empty = server.run(&["show-environment", "-gs", "OWNER_EMPTY"]);
    assert_eq!(
        shown_empty.stdout,
        b"OWNER_EMPTY=\"\"; export OWNER_EMPTY;\n"
    );

    let clear = server.run(&["set-environment", "-gr", "OWNER_CLEARED"]);
    assert!(
        clear.status.success(),
        "clear environment: {:?}",
        clear.stderr
    );
    let shown_cleared = server.run(&["show-environment", "-gs", "OWNER_CLEARED"]);
    assert_eq!(shown_cleared.stdout, b"unset OWNER_CLEARED;\n");
}
