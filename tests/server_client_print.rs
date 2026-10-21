//! Command output printed to a client: an attached terminal client shows it in
//! view mode on its active pane, and a control client receives it as a line.

#![cfg(unix)]

mod common;

use common::{Screen, Server};
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

fn count(screen: &Screen, text: &str) -> usize {
    (0..screen.cells.len())
        .filter(|&y| screen.find(y, text).is_some())
        .count()
}

#[test]
fn attached_client_shows_output_in_view_mode() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "alpha", "sleep 60"]);
    server.success(&[
        "bind-key",
        "-n",
        "F12",
        "run-shell",
        "printf 'first-line\\nsecond-line\\npartial-tail'",
    ]);
    let mut client = server.attach_terminal(80, 24);

    client.send(b"\x1b[24~");
    client.wait_screen(|screen| {
        count(screen, "first-line") == 1
            && count(screen, "second-line") == 1
            && count(screen, "partial-tail") == 1
    });
    assert_eq!(client.display(&server, "#{pane_mode}"), "view-mode");

    // Already in view mode: the output is appended, not a fresh mode.
    client.send(b"\x1b[24~");
    client.wait_screen(|screen| {
        count(screen, "first-line") == 2
            && count(screen, "second-line") == 2
            && count(screen, "partial-tail") == 2
    });
    assert_eq!(client.display(&server, "#{pane_mode}"), "view-mode");
}

#[test]
fn control_client_receives_message_line() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "alpha", "sleep 60"]);
    let mut child = server
        .command()
        .args(["-C", "attach"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn control client");
    let stdout = child.stdout.take().expect("control stdout");
    let (lines, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });

    let mut stdin = child.stdin.take().expect("control stdin");
    writeln!(stdin, "display-message printed-to-control").expect("send command");
    loop {
        let line = received
            .recv_timeout(Duration::from_secs(5))
            .expect("control client printed the message");
        if line == "%message printed-to-control" {
            break;
        }
    }

    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
}
