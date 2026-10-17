//! Exercise source-file path construction through a live server.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Server {
    directory: PathBuf,
    socket: PathBuf,
    absolute: PathBuf,
}

impl Server {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tiri-source-pattern-[test]+é-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("create private directory");
        let socket = directory.join("socket");
        let absolute = std::env::temp_dir().join(format!(
            "tiri-source-absolute-{}-{stamp}.conf",
            std::process::id()
        ));
        Self {
            directory,
            socket,
            absolute,
        }
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

    fn source(&self, path: &str) {
        let output = self.run(&["source-file", path]);
        assert!(output.status.success(), "source-file {path}: {output:?}");
    }

    fn option(&self, option: &str) -> Vec<u8> {
        let output = self.run(&["show-options", "-gqv", option]);
        assert!(output.status.success(), "show-options {option}: {output:?}");
        output.stdout
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.socket.exists() {
            let _ = self.run(&["kill-server"]);
        }
        let _ = fs::remove_dir_all(&self.directory);
        let _ = fs::remove_file(&self.absolute);
    }
}

#[test]
fn sources_absolute_relative_and_glob_paths_and_reports_no_match() {
    let server = Server::new();
    let created = server.run(&["new-session", "-d", "-s", "source-pattern", "sleep 30"]);
    assert!(created.status.success(), "new-session: {created:?}");

    fs::write(&server.absolute, "set-option -g @absolute yes\n").expect("write absolute file");
    server.source(server.absolute.to_str().expect("UTF-8 temporary path"));
    assert_eq!(server.option("@absolute"), b"yes\n");

    fs::write(
        server.directory.join("relative.conf"),
        "set-option -g @relative yes\n",
    )
    .expect("write relative file");
    server.source("relative.conf");
    assert_eq!(server.option("@relative"), b"yes\n");

    fs::write(
        server.directory.join("glob-a.conf"),
        "set-option -g @glob_a yes\n",
    )
    .expect("write first glob file");
    fs::write(
        server.directory.join("glob-b.conf"),
        "set-option -g @glob_b yes\n",
    )
    .expect("write second glob file");
    server.source("glob-*.conf");
    assert_eq!(server.option("@glob_a"), b"yes\n");
    assert_eq!(server.option("@glob_b"), b"yes\n");

    fs::write(
        server.directory.join("order-a.conf"),
        "set-option -g @source_order first\n",
    )
    .expect("write first ordered file");
    fs::write(
        server.directory.join("order-b.conf"),
        "set-option -g @source_order second\n",
    )
    .expect("write second ordered file");
    let ordered = server.run(&["source-file", "order-a.conf", "order-b.conf"]);
    assert!(
        ordered.status.success(),
        "multiple source files: {ordered:?}"
    );
    assert_eq!(server.option("@source_order"), b"second\n");

    let missing = server.run(&["source-file", "missing-*.conf"]);
    assert!(
        !missing.status.success(),
        "missing source file: {missing:?}"
    );
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("missing-*.conf"),
        "missing file diagnostic: {missing:?}"
    );
    let quiet = server.run(&["source-file", "-q", "missing-*.conf"]);
    assert!(quiet.status.success(), "quiet no-match: {quiet:?}");
}

#[test]
fn startup_sources_many_local_files_without_recursion_or_a_stuck_wait() {
    let server = Server::new();
    fs::write(server.directory.join("empty.conf"), "").unwrap();
    let startup = server.directory.join("startup.conf");
    let mut configuration = String::from("source-file");
    for _ in 0..4096 {
        configuration.push_str(" empty.conf");
    }
    configuration.push_str("\nset-option -g @startup-completed yes\n");
    fs::write(&startup, configuration).unwrap();
    let created = server.run(&[
        "-f",
        startup.to_str().unwrap(),
        "new-session",
        "-d",
        "-s",
        "local-source",
        "sleep 30",
    ]);
    assert!(created.status.success(), "startup source-file: {created:?}");
    assert_eq!(server.option("@startup-completed"), b"yes\n");
}
