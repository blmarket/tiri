//! `wait-for -Ev` printing to an attached client enters view mode, which fires
//! `pane-mode-changed` again from inside the waiter's own event callback.

#![cfg(unix)]

mod common;

use common::Server;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_verbose_event_waiter_re_entered_by_its_own_print_resumes_once() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-s", "s", "sleep 60"]);
    let mut sibling = server
        .command()
        .args(["wait-for", "-E", "-F", "#{==:1,0}", "pane-mode-changed"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sibling waiter");
    let waiters = || {
        server
            .success(&["wait-for", "-El", "pane-mode-changed"])
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    wait_until("the sibling waiter", || waiters().len() == 1);
    let [sibling_name] = &waiters()[..] else {
        panic!("one sibling waiter")
    };
    let sibling_name = sibling_name.clone();

    server.success(&[
        "bind",
        "-n",
        "F1",
        "wait-for -Ev pane-mode-changed ; set -gF @t '#{@t}K'",
    ]);
    let mut client = server.attach_terminal(80, 24);
    client.send(b"\x1bOP");
    wait_until("the key-bound waiter", || {
        client.drain();
        waiters().len() == 2
    });

    server.success(&["copy-mode", "-t", "s:0.0"]);
    wait_until("the key-bound waiter to resume", || {
        client.drain();
        server.display("#{@t}") == "K"
    });
    assert_eq!(waiters(), [sibling_name]);
    assert_eq!(server.display("#{pane_mode}"), "view-mode");
    assert!(sibling.try_wait().expect("sibling status").is_none());
    let _ = sibling.kill();
    let _ = sibling.wait();
}
