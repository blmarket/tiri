//! Each terminal client views the strip through its own window-sized view.
//! The view rests on a pane's first column and shows the active pane
//! completely; explicit panning lasts until a pane is selected.

#![cfg(unix)]

mod common;

use common::{Server, TerminalClient};
use std::time::Duration;

/// A server whose manually sized `sx`-column window holds `panes` half panes,
/// with the status line off so the window fills a 24-row terminal.
fn strip(sx: u32, panes: usize) -> Server {
    let server = Server::new();
    server.start(sx, 24, panes);
    server.success(&["set", "-g", "status", "off"]);
    server
}

fn ids(server: &Server) -> Vec<String> {
    server.panes().into_iter().map(|(id, ..)| id).collect()
}

fn select(server: &Server, pane: &str) {
    server.success(&["select-pane", "-t", pane]);
}

/// Restart `pane` with `script`, then wait until its first row shows `text`.
fn paint(server: &Server, pane: &str, script: &str, text: &str) {
    server.success(&["respawn-pane", "-k", "-t", pane, script]);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !server
        .success(&["capture-pane", "-p", "-t", pane])
        .lines()
        .next()
        .is_some_and(|row| row.contains(text))
    {
        assert!(
            std::time::Instant::now() < deadline,
            "{pane} never showed {text}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Fill the start of each pane's first row with its own letter, A onwards.
fn label(server: &Server, ids: &[String]) {
    for (pane, letter) in ids.iter().zip('A'..) {
        let text = letter.to_string().repeat(5);
        paint(
            server,
            pane,
            &format!("printf {text}; exec sleep 60"),
            &text,
        );
    }
}

/// Whether the screen's first row shows each pane's label at its first column,
/// for a view starting at `offset`.
fn shows_labels(client: &TerminalClient, starts: &[u32], offset: u32) -> bool {
    starts.iter().zip('A'..).all(|(&start, letter)| {
        let text = letter.to_string().repeat(5);
        let Some(column) = start.checked_sub(offset) else {
            return true;
        };
        column as usize + 5 > client.screen.cells[0].len()
            || client
                .screen
                .row(0)
                .chars()
                .skip(column as usize)
                .take(5)
                .eq(text.chars())
    })
}

#[test]
fn the_view_rests_on_pane_boundaries_from_both_directions() {
    for (sx, starts) in [(80, [0, 40, 80, 120]), (81, [0, 41, 82, 123])] {
        let server = strip(sx, 4);
        let ids = ids(&server);
        label(&server, &ids);
        let mut client = server.attach_terminal(sx as u16, 24);
        client.wait_offset(&server, starts[2]);
        select(&server, &ids[2]);
        assert_eq!(client.offset(&server), starts[2]);
        select(&server, &ids[1]);
        assert_eq!(client.offset(&server), starts[1]);
        select(&server, &ids[0]);
        assert_eq!(client.offset(&server), 0);
        select(&server, &ids[2]);
        assert_eq!(client.offset(&server), starts[1]);
        client.wait_screen(|screen| screen.row(0).starts_with("BBBBB"));
        assert!(shows_labels(&client, &starts, starts[1]));
        select(&server, &ids[3]);
        assert_eq!(client.offset(&server), starts[2]);
        client.wait_screen(|screen| screen.row(0).starts_with("CCCCC"));
        assert!(shows_labels(&client, &starts, starts[2]));
    }
}

#[test]
fn the_extent_ends_a_window_width_past_the_last_pane_start() {
    let server = strip(80, 4);
    let client = server.attach_terminal(80, 24);
    client.wait_offset(&server, 80);
    server.success(&["refresh-client", "-t", &client.tty, "-R", "999"]);
    assert_eq!(client.offset(&server), 120);
    let lone = strip(80, 1);
    let client = lone.attach_terminal(80, 24);
    assert_eq!(client.display(&lone, "#{window_bigger}"), "0");
    let pair = strip(80, 2);
    let client = pair.attach_terminal(80, 24);
    assert_eq!(client.display(&pair, "#{window_bigger}"), "0");
    pair.success(&["refresh-client", "-t", &client.tty, "-R", "999"]);
    assert_eq!(client.offset(&pair), 40);
}

#[test]
fn a_shrinking_strip_brings_the_view_back_over_its_panes() {
    let pair = strip(80, 2);
    let halves = ids(&pair);
    let client = pair.attach_terminal(80, 24);
    assert_eq!(client.offset(&pair), 0);
    pair.success(&["resize-pane", "-Z", "-t", &halves[1]]);
    client.wait_offset(&pair, 40);
    pair.success(&["resize-pane", "-Z", "-t", &halves[1]]);
    client.wait_offset(&pair, 0);
    assert_eq!(client.display(&pair, "#{window_bigger}"), "0");
    let server = strip(80, 4);
    let panes = ids(&server);
    let client = server.attach_terminal(80, 24);
    client.wait_offset(&server, 80);
    server.success(&["kill-pane", "-t", &panes[3]]);
    client.wait_offset(&server, 40);
    server.success(&["refresh-client", "-t", &client.tty, "-R", "999"]);
    assert_eq!(client.offset(&server), 80);
}

#[test]
fn cursor_movement_inside_a_visible_pane_does_not_move_the_view() {
    let server = strip(80, 3);
    let ids = ids(&server);
    let mut client = server.attach_terminal(80, 24);
    select(&server, &ids[0]);
    select(&server, &ids[2]);
    client.wait_offset(&server, 40);
    paint(
        &server,
        &ids[2],
        r"printf '\033[1;39Hz\033[1;1Hy'; exec sleep 60",
        "y",
    );
    client.wait_screen(|screen| screen.cells[0][40].ch == 'y' && (screen.x, screen.y) == (41, 0));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(client.offset(&server), 40);
}

#[test]
fn panning_lasts_until_a_pane_is_selected() {
    let server = strip(80, 4);
    let ids = ids(&server);
    let client = server.attach_terminal(80, 24);
    select(&server, &ids[0]);
    assert_eq!(client.offset(&server), 0);
    server.success(&["refresh-client", "-t", &client.tty, "-R", "25"]);
    assert_eq!(client.offset(&server), 25);
    select(&server, &ids[1]);
    assert_eq!(client.offset(&server), 40);
    server.success(&["refresh-client", "-t", &client.tty, "-L", "15"]);
    assert_eq!(client.offset(&server), 25);
    select(&server, &ids[1]);
    assert_eq!(client.offset(&server), 25);
    server.success(&["refresh-client", "-t", &client.tty, "-c"]);
    assert_eq!(client.offset(&server), 40);
}

#[test]
fn clients_of_different_widths_keep_their_own_views() {
    let server = strip(80, 3);
    let ids = ids(&server);
    let wide = server.attach_terminal(120, 24);
    let narrow = server.attach_terminal(40, 24);
    let normal = server.attach_terminal(80, 24);
    let offsets = || {
        [&wide, &narrow, &normal]
            .map(|client| client.offset(&server))
            .to_vec()
    };
    assert_eq!(offsets(), [40, 80, 40]);
    select(&server, &ids[0]);
    assert_eq!(offsets(), [0, 0, 0]);
    select(&server, &ids[1]);
    assert_eq!(offsets(), [0, 40, 0]);
    assert_eq!(server.window_size(), (80, 24));
}

#[test]
fn a_client_narrower_than_the_active_pane_follows_its_cursor() {
    let server = strip(80, 3);
    let ids = ids(&server);
    server.success(&["resize-pane", "-Z", "-t", &ids[1]]);
    assert_eq!(
        server
            .panes()
            .iter()
            .map(|&(_, left, width, ..)| (left, width))
            .collect::<Vec<_>>(),
        [(0, 39), (40, 80), (121, 39)]
    );
    let mut narrow = server.attach_terminal(40, 24);
    let normal = server.attach_terminal(80, 24);
    narrow.wait_offset(&server, 121);
    select(&server, &ids[1]);
    assert_eq!(narrow.offset(&server), 40);
    assert_eq!(normal.offset(&server), 40);
    paint(
        &server,
        &ids[1],
        concat!(
            r"stty -echo; printf '\033[1;71H!'; read x; printf '\033[1;11H?';",
            r" read x; printf '\033[1;31H'; exec sleep 60"
        ),
        "!",
    );
    narrow.wait_offset(&server, 72);
    narrow.wait_screen(|screen| screen.cells[0][38].ch == '!' && screen.x == 39);
    server.success(&["send-keys", "-t", &ids[1], "Enter"]);
    narrow.wait_offset(&server, 51);
    server.success(&["send-keys", "-t", &ids[1], "Enter"]);
    narrow.wait_screen(|screen| (screen.x, screen.y) == (19, 0));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(narrow.offset(&server), 51);
    assert_eq!(normal.offset(&server), 40);
}

#[test]
fn mouse_clicks_select_the_pane_under_them_after_panning() {
    let server = strip(80, 4);
    server.success(&["set", "-g", "mouse", "on"]);
    let ids = ids(&server);
    let mut client = server.attach_terminal(80, 24);
    select(&server, &ids[0]);
    server.success(&["refresh-client", "-t", &client.tty, "-R", "25"]);
    assert_eq!(client.offset(&server), 25);
    client.click(20, 5);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while server.display("#{pane_id}") != ids[1] {
        assert!(
            std::time::Instant::now() < deadline,
            "click selected no pane"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(client.offset(&server), 40);
}

#[test]
fn left_and_right_stop_at_the_strip_ends() {
    let server = strip(80, 3);
    let ids = ids(&server);
    let active = || server.display("#{pane_id}");
    server.success(&["select-pane", "-R"]);
    assert_eq!(active(), ids[2]);
    server.success(&["select-pane", "-L"]);
    server.success(&["select-pane", "-L"]);
    assert_eq!(active(), ids[0]);
    server.success(&["select-pane", "-L"]);
    assert_eq!(active(), ids[0]);
    assert!(!server
        .run(&["select-pane", "-t", "{left-of}"])
        .status
        .success());
    server.success(&["select-pane", "-t", "{right-of}"]);
    assert_eq!(active(), ids[1]);
    server.success(&["select-pane", "-U"]);
    server.success(&["select-pane", "-D"]);
    for target in ["{up-of}", "{down-of}"] {
        assert!(
            !server.run(&["select-pane", "-t", target]).status.success(),
            "{target} found a pane"
        );
    }
    assert_eq!(active(), ids[1]);
}

#[test]
fn positions_are_measured_across_the_panes() {
    let server = strip(80, 3);
    let ids = ids(&server);
    let at = |target: &str| {
        server
            .success(&["display", "-p", "-t", target, "#{pane_id}"])
            .trim_end()
            .to_owned()
    };
    assert_eq!(at("{left}"), ids[0]);
    assert_eq!(at("{top}"), ids[1]);
    assert_eq!(at("{right}"), ids[2]);
    let at_right = server.success(&["list-panes", "-F", "#{pane_at_right}"]);
    assert_eq!(at_right.lines().collect::<Vec<_>>(), ["0", "0", "1"]);
    let lone = strip(80, 1);
    assert_eq!(lone.display("#{pane_at_right}"), "1");
    assert_eq!(
        lone.success(&["display", "-p", "-t", "{right}", "#{pane_id}"])
            .trim_end(),
        lone.display("#{pane_id}")
    );
}
