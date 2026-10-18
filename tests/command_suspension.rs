//! What a command's suspension does at once and what waits for its
//! continuation: hooks and control-mode guards belong to the suspension, and
//! the continuation's own failure never drops the rest of its group.

#![cfg(unix)]

mod common;

use common::Server;
use std::io::{Read as _, Write as _};
use std::process::Stdio;
use std::time::Duration;

/// A load-buffer of a missing file suspends on the client's read and fails in
/// its continuation.
const MISSING: &str = "/nonexistent/tiri-command-suspension";

#[test]
fn the_after_hook_is_queued_when_the_command_suspends() {
    let server = Server::new();
    server.start(80, 24, 1);
    server.success(&["set-hook", "-g", "after-load-buffer", "set -g @hooked 1"]);
    let output = server.run(&["load-buffer", MISSING]);
    assert!(!output.status.success());
    assert_eq!(server.display("#{@hooked}"), "1");
}

#[test]
fn a_continuation_error_keeps_the_rest_of_its_group() {
    let server = Server::new();
    server.start(80, 24, 1);
    let output = server.run(&["load-buffer", MISSING, ";", "set", "-g", "@after", "1"]);
    assert!(!output.status.success());
    assert_eq!(server.display("#{@after}"), "1");

    let output = server.run(&[
        "select-window",
        "-t",
        "nosuch",
        ";",
        "set",
        "-g",
        "@sync",
        "1",
    ]);
    assert!(!output.status.success());
    assert_eq!(
        server.display("#{@sync}"),
        "",
        "an exec error drops its group"
    );
}

#[test]
fn control_mode_ends_a_suspended_command_before_its_output() {
    let server = Server::new();
    server.start(80, 24, 1);
    let mut child = server
        .command()
        .args(["-C", "attach"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn control client");
    let mut stdin = child.stdin.take().expect("control stdin");
    writeln!(stdin, "run-shell 'sleep 0.2; echo done'").expect("send command");
    std::thread::sleep(Duration::from_millis(600));
    drop(stdin);
    let mut output = String::new();
    child
        .stdout
        .take()
        .expect("control stdout")
        .read_to_string(&mut output)
        .expect("read control output");
    child.wait().expect("control client exits");

    let guards: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("%begin") || line.starts_with("%end") || *line == "done")
        .map(|line| line.split(' ').next().unwrap())
        .collect();
    assert_eq!(
        guards,
        ["%begin", "%end", "%begin", "%end", "done"],
        "attach, then run-shell ended at its suspension:\n{output}"
    );
}
