//! OSC 8 IDs through a real pane, hyperlink store, and capture output.

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
            std::env::temp_dir().join(format!("tiri-osc8-{}-{stamp}", std::process::id()));
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
fn osc8_id_survives_parser_paths_and_remains_in_captured_links() {
    let server = Server::new();
    let script = concat!(
        "printf '",
        "\\033]8;id=alpha=42;https://a.example\\007A\\033]8;;\\007",
        "\\033]8;id=alpha=42;https://a.example\\007B\\033]8;;\\007",
        "\\033]8;id=alpha=42;https://b.example\\007C",
        "\\033]8;id=unused;\\007",
        "\\033]8;id=orphan:foo\\007",
        "\\033]8;id=one:id=two;https://bad.example\\007D",
        "\\033]8;id=;https://empty.example\\007E\\033]8;;\\007",
        "\\n\\033]8;id=alpha=42;https://a.example\\007F\\033]8;;\\007",
        "\\033]8;id=other;https://next.example\\007G\\033]8;;\\007",
        "'; sleep 30",
    );
    let create = server.run(&["new-session", "-d", "-s", "osc8-test", script]);
    assert!(
        create.status.success(),
        "new-session failed: {:?}",
        create.stderr
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let plain = server.run(&["capture-pane", "-p", "-S", "0"]);
        assert!(plain.status.success(), "capture failed: {:?}", plain.stderr);
        if plain.stdout.starts_with(b"ABCDE\n") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pane output missing: {:?}",
            plain.stdout
        );
        thread::sleep(Duration::from_millis(20));
    }

    let escaped = server.run(&["capture-pane", "-p", "-e", "-S", "0"]);
    assert!(
        escaped.status.success(),
        "capture failed: {:?}",
        escaped.stderr
    );
    assert!(
        escaped.stdout.starts_with(
            concat!(
                "\x1b]8;id=alpha=42;https://a.example\x1b\\AB",
                "\x1b]8;id=alpha=42;https://b.example\x1b\\C",
                "\x1b]8;;\x1b\\D",
                "\x1b]8;;https://empty.example\x1b\\E\x1b]8;;\x1b\\\n",
            )
            .as_bytes()
        ),
        "unexpected hyperlink capture: {:?}",
        escaped.stdout
    );

    let links = server.run(&["capture-pane", "-p", "-H", "-S", "0"]);
    assert!(links.status.success(), "capture failed: {:?}", links.stderr);
    assert_eq!(
        links.stdout,
        b"https://a.example https://b.example https://empty.example\nhttps://next.example\n"
    );
}
