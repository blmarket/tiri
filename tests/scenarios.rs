//! End-to-end scenarios: a user's sequence of commands, and where every pane
//! is after each step. A pane's strip width (half or full) is its own: it
//! goes with the pane to another window and is measured there against that
//! window's width.

#![cfg(unix)]

mod common;

use common::Server;

/// Each pane of `window` as (id, first column, width), in strip order.
fn strip(server: &Server, window: &str) -> Vec<(String, u32, u32)> {
    server
        .success(&[
            "list-panes",
            "-t",
            window,
            "-F",
            "#{pane_id} #{pane_left} #{pane_width}",
        ])
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split(' ').collect();
            let number = |index: usize| fields[index].parse().expect("numeric pane field");
            (fields[0].to_owned(), number(1), number(2))
        })
        .collect()
}

/// `strip`, written as the expected panes and their columns.
fn expect(panes: &[(&String, u32, u32)]) -> Vec<(String, u32, u32)> {
    panes
        .iter()
        .map(|&(id, left, width)| (id.clone(), left, width))
        .collect()
}

fn new_pane(server: &Server, args: &[&str]) -> String {
    let id = server.success(
        &[
            &["new-pane", "-P", "-F", "#{pane_id}"][..],
            args,
            &["sleep 60"],
        ]
        .concat(),
    );
    id.trim_end().to_owned()
}

/// A new window with one pane: its id and the pane's id.
fn new_window(server: &Server) -> (String, String) {
    let ids = server.success(&[
        "new-window",
        "-P",
        "-F",
        "#{window_id} #{pane_id}",
        "sleep 60",
    ]);
    let (window, pane) = ids.trim_end().split_once(' ').expect("two ids");
    (window.to_owned(), pane.to_owned())
}

/// An 81-column window with one pane: its id and the pane's id. Two halves
/// and their separator fill 81 columns, so a half pane is 40 wide.
fn first_window(server: &Server) -> (String, String) {
    server.start(81, 10, 1);
    (server.display("#{window_id}"), server.display("#{pane_id}"))
}

#[test]
fn adding_panes() {
    let server = Server::new();
    let (a, p0) = first_window(&server);
    assert_eq!(strip(&server, &a), expect(&[(&p0, 0, 40)]));

    let p1 = new_pane(&server, &[]);
    assert_eq!(strip(&server, &a), expect(&[(&p0, 0, 40), (&p1, 41, 40)]));
    assert_eq!(server.display("#{pane_id}"), p1);

    let p2 = new_pane(&server, &["-t", &p0]);
    let p3 = new_pane(&server, &["-b", "-t", &p0]);
    assert_eq!(
        strip(&server, &a),
        expect(&[(&p3, 0, 40), (&p0, 41, 40), (&p2, 82, 40), (&p1, 123, 40)])
    );

    let p4 = new_pane(&server, &["-d", "-t", &p1]);
    assert_eq!(server.display("#{pane_id}"), p3);
    assert_eq!(strip(&server, &a).last(), Some(&(p4, 164, 40)));
}

#[test]
fn a_full_width_pane_stays_full_when_moved_to_another_window_and_back() {
    let server = Server::new();
    let (a, p0) = first_window(&server);
    let p1 = new_pane(&server, &[]);
    let p2 = new_pane(&server, &[]);
    server.success(&["resize-pane", "-Z", "-t", &p1]);
    assert_eq!(
        strip(&server, &a),
        expect(&[(&p0, 0, 40), (&p1, 41, 81), (&p2, 123, 40)])
    );

    let (b, q0) = new_window(&server);
    server.success(&["join-pane", "-s", &p1, "-t", &q0]);
    assert_eq!(strip(&server, &b), expect(&[(&q0, 0, 40), (&p1, 41, 81)]));
    assert_eq!(strip(&server, &a), expect(&[(&p0, 0, 40), (&p2, 41, 40)]));

    server.success(&["join-pane", "-b", "-s", &p1, "-t", &p0]);
    assert_eq!(
        strip(&server, &a),
        expect(&[(&p1, 0, 81), (&p0, 82, 40), (&p2, 123, 40)])
    );
    assert_eq!(strip(&server, &b), expect(&[(&q0, 0, 40)]));
}

#[test]
fn a_moved_pane_is_sized_by_the_destination_window() {
    let server = Server::new();
    let (a, p0) = first_window(&server);
    let p1 = new_pane(&server, &[]);
    let p2 = new_pane(&server, &[]);
    server.success(&["resize-pane", "-Z", "-t", &p1]);

    let (b, q0) = new_window(&server);
    server.success(&["resize-window", "-t", &b, "-x", "41"]);
    assert_eq!(strip(&server, &b), expect(&[(&q0, 0, 20)]));

    server.success(&["join-pane", "-s", &p1, "-t", &q0]);
    server.success(&["join-pane", "-s", &p2, "-t", &p1]);
    assert_eq!(
        strip(&server, &b),
        expect(&[(&q0, 0, 20), (&p1, 21, 41), (&p2, 63, 20)])
    );
    assert_eq!(strip(&server, &a), expect(&[(&p0, 0, 40)]));
}

#[test]
fn a_pane_keeps_its_strip_width_through_a_tiling_window() {
    let server = Server::new();
    let (a, p0) = first_window(&server);
    let p1 = new_pane(&server, &[]);
    server.success(&["resize-pane", "-Z", "-t", &p1]);

    let (b, q0) = new_window(&server);
    server.success(&["select-layout", "-t", &b, "tiling"]);
    server.success(&["join-pane", "-s", &p1, "-t", &q0]);
    assert_eq!(strip(&server, &b), expect(&[(&q0, 0, 40), (&p1, 41, 40)]));

    server.success(&["join-pane", "-s", &p1, "-t", &p0]);
    assert_eq!(strip(&server, &a), expect(&[(&p0, 0, 40), (&p1, 41, 81)]));
}

#[test]
fn swapping_panes_across_windows_swaps_their_widths() {
    let server = Server::new();
    let (a, p0) = first_window(&server);
    let p1 = new_pane(&server, &[]);
    let p2 = new_pane(&server, &[]);
    server.success(&["resize-pane", "-Z", "-t", &p1]);
    let (b, q0) = new_window(&server);

    server.success(&["swap-pane", "-s", &p1, "-t", &q0]);
    assert_eq!(
        strip(&server, &a),
        expect(&[(&p0, 0, 40), (&q0, 41, 40), (&p2, 82, 40)])
    );
    assert_eq!(strip(&server, &b), expect(&[(&p1, 0, 81)]));
}

#[test]
fn break_pane_keeps_the_width_in_the_new_window() {
    let server = Server::new();
    let (a, p0) = first_window(&server);
    let p1 = new_pane(&server, &[]);
    server.success(&["resize-pane", "-Z", "-t", &p1]);

    server.success(&["break-pane", "-s", &p1]);
    let c = server.display("#{window_id}");
    assert_ne!(c, a);
    assert_eq!(strip(&server, &c), expect(&[(&p1, 0, 81)]));
    assert_eq!(strip(&server, &a), expect(&[(&p0, 0, 40)]));
}

#[test]
fn a_move_the_strip_cannot_take_is_refused_and_nothing_changes() {
    let server = Server::new();
    server.start(5000, 10, 3);
    let a = server.display("#{window_id}");
    let before = strip(&server, &a);
    let (b, q0) = new_window(&server);
    server.success(&["resize-window", "-t", &b, "-x", "5000"]);
    let q1 = new_pane(&server, &["-t", &q0]);
    server.success(&["resize-pane", "-Z", "-t", &q1]);
    let b_before = strip(&server, &b);
    assert_eq!(b_before, expect(&[(&q0, 0, 2499), (&q1, 2500, 5000)]));

    let output = server.run(&["join-pane", "-s", &q1, "-t", &before[2].0]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("no space: the strip would pass its maximum width"));
    assert_eq!(strip(&server, &a), before);
    assert_eq!(strip(&server, &b), b_before);
}
