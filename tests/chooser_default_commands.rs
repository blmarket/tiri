//! Choosers given no template run their built-in default command on the
//! chosen item.

#![cfg(unix)]

mod common;

use common::{Server, TerminalClient};
use std::time::{Duration, Instant};

/// A server with sessions `alpha` and `beta`, and a terminal client on
/// `alpha`.
fn two_sessions() -> (Server, TerminalClient) {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "alpha", "sleep 60"]);
    server.success(&["new-session", "-d", "-s", "beta", "sleep 60"]);
    let client = server.attach_terminal(80, 24);
    server.success(&["switch-client", "-c", &client.tty, "-t", "alpha"]);
    (server, client)
}

fn wait_session(server: &Server, client: &TerminalClient, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.display(server, "#{session_name}") != expected {
        assert!(
            Instant::now() < deadline,
            "client stayed on {}, expected {expected}",
            client.display(server, "#{session_name}")
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn choose_tree_switches_to_the_chosen_session() {
    let (server, mut client) = two_sessions();
    server.success(&["choose-tree", "-s", "-t", "alpha:0.0"]);
    client.wait_screen(|screen| (0..screen.cells.len()).any(|y| screen.find(y, "beta").is_some()));
    client.send(b"\x1b[B\r");
    wait_session(&server, &client, "beta");
}

#[test]
fn switch_mode_switches_to_the_chosen_session() {
    let (server, mut client) = two_sessions();
    server.success(&["switch-mode", "-s", "-t", "alpha:0.0"]);
    client.wait_screen(|screen| {
        (0..screen.cells.len()).any(|y| screen.find(y, "(search)").is_some())
    });
    client.send(b"beta\r");
    wait_session(&server, &client, "beta");
}
