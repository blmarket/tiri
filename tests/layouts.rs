//! Besides the strip and tiling, four layouts are a shape over the pane order
//! alone, the first pane being the main one: main-vertical, main-centered,
//! main-horizontal and grid. select-layout names them, -n steps through every
//! layout, and #{window_layout_name} shows the current one.

#![cfg(unix)]

mod common;

use common::{Screen, Server};

/// A server whose manually sized `sx` by `sy` window holds `panes` panes in
/// `layout`, with the status line off so the window starts at the top row.
fn laid_out(layout: &str, sx: u32, sy: u32, panes: usize) -> Server {
    let server = Server::new();
    server.start(sx, sy, panes);
    server.success(&["set", "-g", "status", "off"]);
    server.success(&["select-layout", layout]);
    server
}

fn ids(server: &Server) -> Vec<String> {
    server.panes().into_iter().map(|(id, ..)| id).collect()
}

/// Each pane's first column, width, top row and height, in pane order.
fn rects(server: &Server) -> Vec<(u32, u32, u32, u32)> {
    server
        .panes()
        .into_iter()
        .map(|(_, left, width, top, height)| (left, width, top, height))
        .collect()
}

fn active(server: &Server) -> String {
    server.display("#{pane_id}")
}

#[test]
fn main_vertical_stacks_the_side_panes_on_the_right() {
    let server = laid_out("main-vertical", 81, 25, 3);
    assert_eq!(server.display("#{window_layout_name}"), "main-vertical");
    assert_eq!(
        rects(&server),
        [(0, 40, 0, 25), (41, 40, 0, 12), (41, 40, 13, 12)]
    );
}

#[test]
fn main_horizontal_lines_the_side_panes_up_below() {
    let server = laid_out("main-horizontal", 81, 25, 3);
    assert_eq!(
        rects(&server),
        [(0, 81, 0, 12), (0, 40, 13, 12), (41, 40, 13, 12)]
    );
}

#[test]
fn main_centered_puts_side_panes_on_both_sides() {
    let server = laid_out("main-centered", 81, 25, 4);
    assert_eq!(
        rects(&server),
        [
            (21, 40, 0, 25),
            (62, 19, 0, 12),
            (62, 19, 13, 12),
            (0, 20, 0, 25)
        ]
    );
    server.success(&["kill-pane", "-t", &ids(&server)[3]]);
    server.success(&["kill-pane", "-t", &ids(&server)[2]]);
    assert_eq!(rects(&server), [(0, 40, 0, 25), (41, 40, 0, 25)]);
}

#[test]
fn grid_gives_every_pane_an_equal_share() {
    let server = laid_out("grid", 81, 25, 4);
    assert_eq!(
        rects(&server),
        [
            (0, 40, 0, 12),
            (41, 40, 0, 12),
            (0, 40, 13, 12),
            (41, 40, 13, 12)
        ]
    );
    server.success(&["new-pane", "sleep 60"]);
    assert_eq!(
        rects(&server),
        [
            (0, 27, 0, 12),
            (28, 26, 0, 12),
            (55, 26, 0, 12),
            (0, 40, 13, 12),
            (41, 40, 13, 12)
        ]
    );
}

#[test]
fn the_pane_order_decides_the_main_pane() {
    let server = laid_out("main-vertical", 81, 25, 3);
    let panes = ids(&server);
    let tiles = rects(&server);
    server.success(&["select-pane", "-t", &panes[0]]);
    server.success(&["new-pane", "sleep 60"]);
    let added = active(&server);
    assert_eq!(ids(&server)[1], added);
    assert_eq!(rects(&server)[0], tiles[0]);
    server.success(&["kill-pane", "-t", &added]);
    server.success(&["swap-pane", "-s", &panes[2], "-t", &panes[0]]);
    assert_eq!(ids(&server)[0], panes[2]);
    assert_eq!(rects(&server), tiles);
    server.success(&["rotate-window"]);
    assert_eq!(ids(&server)[0], panes[1]);
    assert_eq!(rects(&server), tiles);
}

#[test]
fn directions_cross_the_separators() {
    let server = laid_out("main-horizontal", 81, 25, 3);
    let panes = ids(&server);
    server.success(&["select-pane", "-t", &panes[0]]);
    server.success(&["select-pane", "-D"]);
    assert_eq!(active(&server), panes[1]);
    server.success(&["select-pane", "-R"]);
    assert_eq!(active(&server), panes[2]);
    server.success(&["select-pane", "-U"]);
    assert_eq!(active(&server), panes[0]);
}

#[test]
fn next_steps_through_every_layout() {
    let server = laid_out("scrolling", 81, 25, 2);
    server.success(&["resize-pane", "-W", "-t", &ids(&server)[0]]);
    let mut seen = Vec::new();
    for _ in 0..6 {
        server.success(&["select-layout", "-n"]);
        seen.push(server.display("#{window_layout_name}"));
    }
    assert_eq!(
        seen,
        [
            "tiling",
            "main-vertical",
            "main-centered",
            "main-horizontal",
            "grid",
            "scrolling"
        ]
    );
    assert_eq!(rects(&server), [(0, 40, 0, 25), (41, 40, 0, 25)]);
}

#[test]
fn a_full_preset_refuses_a_new_pane() {
    let server = laid_out("main-vertical", 12, 6, 1);
    let mut refused = None;
    for _ in 0..20 {
        let output = server.run(&["new-pane", "sleep 60"]);
        if !output.status.success() {
            refused = Some(String::from_utf8_lossy(&output.stderr).into_owned());
            break;
        }
    }
    assert!(refused
        .expect("a full window refuses")
        .contains("no space for a new pane"));
    for (left, width, top, height) in rects(&server) {
        assert!(width >= 1 && height >= 1);
        assert!(left + width <= 12 && top + height <= 6);
    }
}

fn shows(screen: &Screen, y: usize, columns: std::ops::Range<usize>, ch: char) -> bool {
    columns.into_iter().all(|x| screen.cells[y][x].ch == ch)
}

#[test]
fn main_horizontal_draws_a_tee_where_the_side_panes_meet_the_main_one() {
    let server = laid_out("main-horizontal", 81, 25, 3);
    let mut client = server.attach_terminal(81, 25);
    client.wait_screen(|screen| {
        shows(screen, 12, 0..40, '─')
            && shows(screen, 12, 40..41, '┬')
            && shows(screen, 12, 41..81, '─')
    });
}
