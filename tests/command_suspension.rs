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

#[test]
fn a_source_line_failure_preserves_the_following_line() {
    let server = Server::new();
    server.start(80, 24, 1);
    let mut child = server
        .command()
        .args(["source-file", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("source stdin");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"select-window -t nosuch; set -g @skipped 1\nset -g @following 1\n")
        .expect("write source");
    let output = child.wait_with_output().expect("source completes");
    assert!(!output.status.success());
    assert_eq!(server.display("#{@skipped}"), "");
    assert_eq!(server.display("#{@following}"), "1");
}

#[test]
fn closing_a_popup_can_open_another_and_finish_the_command_sequence() {
    for close_from_command in [false, true] {
        let server = Server::new();
        server.start(80, 24, 1);
        server.success(&[
            "bind-key", "-n", "x",
            "display-popup -E 'printf FIRST_POPUP; read answer'; display-popup -E 'printf SECOND_POPUP; read answer'; set -g @popup-chain done",
        ]);
        let mut terminal = server.attach_terminal(80, 24);
        terminal.send(b"x");
        terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("FIRST_POPUP")));
        assert_eq!(server.display("#{@popup-chain}"), "");
        if close_from_command {
            server.success(&["display-popup", "-C"]);
        } else {
            terminal.send(b"\r");
        }
        terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("SECOND_POPUP")));
        assert_eq!(server.display("#{@popup-chain}"), "");
        if close_from_command {
            server.success(&["display-popup", "-C"]);
        } else {
            terminal.send(b"\r");
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while server.display("#{@popup-chain}") != "done" {
            assert!(
                std::time::Instant::now() < deadline,
                "popup sequence did not finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn popup_returns_its_process_status_to_the_awaiting_command_client() {
    let server = Server::new();
    server.start(80, 24, 1);
    let _terminal = server.attach_terminal(80, 24);
    let output = server.run(&["display-popup", "-E", "exit 7"]);
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn popup_with_close_exit_zero_waits_for_a_key_after_an_unsuccessful_exit() {
    let server = Server::new();
    server.start(80, 24, 1);
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = server
        .command()
        .args(["display-popup", "-EE", "printf POPUP_RETAINED; exit 7"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("POPUP_RETAINED")));
    assert!(command.try_wait().unwrap().is_none());
    terminal.send(b"\x1b");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = command.try_wait().unwrap() {
            assert_eq!(status.code(), Some(7));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "popup command did not return"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
