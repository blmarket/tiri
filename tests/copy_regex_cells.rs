//! Exercise regex byte offsets mapped back to copy-mode cells in a live pane.

#![cfg(unix)]

use std::fs;
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
        let directory =
            std::env::temp_dir().join(format!("tiri-copy-regex-{}-{stamp}", std::process::id()));
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
            .env("LC_ALL", "C")
            .output()
            .expect("run tiri")
    }

    fn command(&self, args: &[&str]) -> Vec<u8> {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        output.stdout
    }

    fn search(&self, direction: &str, pattern: &str, cursor: &[u8]) {
        self.command(&["send-keys", "-X", direction, pattern]);
        assert_eq!(
            self.command(&[
                "display-message",
                "-p",
                "#{copy_cursor_x}:#{copy_cursor_y}:#{search_count}",
            ]),
            cursor,
            "{direction} {pattern}"
        );
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
fn regex_search_and_formats_preserve_multibyte_cells_and_invalid_patterns() {
    let server = Server::new();
    server.command(&[
        "new-session",
        "-d",
        "-x",
        "40",
        "-y",
        "8",
        "-s",
        "copy-regex",
        "printf 'alpha émega\\nbeta 漢字 gamma\\n'; sleep 30",
    ]);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = server.command(&["capture-pane", "-p", "-S", "0"]);
        if output.starts_with("alpha émega\nbeta 漢字 gamma\n".as_bytes()) {
            break;
        }
        assert!(Instant::now() < deadline, "pane output missing: {output:?}");
        thread::sleep(Duration::from_millis(20));
    }

    server.command(&["set-option", "-g", "@regex_text", "zABzAB"]);
    // Expected values also checked against the pinned tmux revision.
    for (format, expected) in [
        ("#{m/r:^ab$,ab}", "1\n"),
        ("#{m/r:^ab$,aB}", "0\n"),
        ("#{m/ri:^ab$,aB}", "1\n"),
        ("#{m/r:[,ab}", "0\n"),
        ("#{m/i:A*,abc}", "1\n"),
        ("#{C/r:^alpha}", "1\n"),
        ("#{C/r:^beta.*gamma$}", "2\n"),
        ("#{C/r:^BETA}", "0\n"),
        ("#{C/ri:^BETA}", "2\n"),
        ("#{C/r:[}", "0\n"),
        ("#{C:émega}", "1\n"),
        (r"#{s/(a)(b)/\2\1/i:@regex_text}", "zBAzBA\n"),
        ("#{s/[/x/:@regex_text}", "zABzAB\n"),
    ] {
        assert_eq!(
            server.command(&["display-message", "-p", format]),
            expected.as_bytes(),
            "{format}"
        );
    }

    server.command(&["copy-mode"]);
    server.search("search-forward", "漢字", b"5:1:1\n");
    server.search("search-forward", "gamma", b"10:1:1\n");
    server.search("search-backward", "émega", b"6:0:1\n");
    server.search("search-forward", "漢字 gamma", b"5:1:1\n");
}

#[test]
fn regex_search_spans_wrapped_lines_in_both_directions() {
    let server = Server::new();
    server.command(&[
        "new-session",
        "-d",
        // A lone pane is half the window: ten columns.
        "-x",
        "21",
        "-y",
        "8",
        "-s",
        "copy-regex-wrap",
        "printf 'ABCDEFGHIJé漢XYZ\\n'; sleep 30",
    ]);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = server.command(&["capture-pane", "-p", "-S", "0"]);
        if output.starts_with("ABCDEFGHIJ\n".as_bytes()) {
            break;
        }
        assert!(Instant::now() < deadline, "pane output missing: {output:?}");
        thread::sleep(Duration::from_millis(20));
    }

    server.command(&["copy-mode"]);
    server.search("search-forward", "Jé", b"9:0:1\n");
    server.search("search-forward", "é漢", b"0:1:1\n");
    server.search("search-backward", "Jé", b"9:0:1\n");
}
