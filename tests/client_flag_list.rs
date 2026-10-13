//! Exercise the writable flag-list parser through a live control client.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
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
            std::env::temp_dir().join(format!("tiri-client-flags-{}-{stamp}", std::process::id()));
        fs::create_dir(&directory).expect("create private socket directory");
        let socket = directory.join("socket");
        Self {
            binary: PathBuf::from(env!("CARGO_BIN_EXE_tiri")),
            directory,
            socket,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .args(["-f", "/dev/null", "-S"])
            .arg(&self.socket)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env("TMUX", "")
            .env("LC_ALL", "C");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run tiri")
    }

    fn control_client(&self) -> Child {
        self.command()
            .args(["-C", "attach-session", "-t", "flags-test"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start control client")
    }

    fn client_flags(&self, client_name: &str) -> String {
        let clients = self.run(&["list-clients", "-F", "#{client_name}|#{client_flags}"]);
        assert!(
            clients.status.success(),
            "list-clients: {:?}",
            clients.stderr
        );
        let line = String::from_utf8(clients.stdout).expect("client flags are UTF-8");
        let (name, flags) = line.trim_end().split_once('|').expect("client and flags");
        assert_eq!(name, client_name);
        flags.to_owned()
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
fn comma_list_sets_and_clears_control_client_flags() {
    let server = Server::new();
    let create = server.run(&["new-session", "-d", "-s", "flags-test", "sleep 30"]);
    assert!(create.status.success(), "new-session: {:?}", create.stderr);
    let mut control = server.control_client();

    let deadline = Instant::now() + Duration::from_secs(5);
    let client_name = loop {
        let clients = server.run(&["list-clients", "-F", "#{client_name}"]);
        assert!(
            clients.status.success(),
            "list-clients: {:?}",
            clients.stderr
        );
        let listing = String::from_utf8(clients.stdout).expect("client name is UTF-8");
        if let Some(name) = listing.lines().next() {
            if !name.is_empty() {
                break name.to_owned();
            }
        }
        assert!(Instant::now() < deadline, "control client did not attach");
        thread::sleep(Duration::from_millis(20));
    };

    let refresh = server.run(&[
        "refresh-client",
        "-t",
        &client_name,
        "-f",
        "ignore-size,read-only",
    ]);
    assert!(
        refresh.status.success(),
        "refresh-client: {:?}",
        refresh.stderr
    );
    let initial = server.client_flags(&client_name);
    let initial: Vec<_> = initial.split(',').collect();
    assert!(initial.contains(&"ignore-size"), "flags: {initial:?}");
    assert!(initial.contains(&"read-only"), "flags: {initial:?}");

    let refresh = server.run(&[
        "refresh-client",
        "-t",
        &client_name,
        "-f",
        ",unknown,!ignore-size,",
    ]);
    assert!(
        refresh.status.success(),
        "refresh-client: {:?}",
        refresh.stderr
    );
    let final_flags = server.client_flags(&client_name);
    let flags: Vec<_> = final_flags.split(',').collect();
    assert!(flags.contains(&"read-only"), "flags: {flags:?}");
    assert!(!flags.contains(&"ignore-size"), "flags: {flags:?}");

    control.kill().expect("stop control client");
    control.wait().expect("reap control client");
}
