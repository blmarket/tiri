//! The tiling layout divides the window between the panes by a fixed rule over
//! the pane list: the first pane takes the window, and each next pane splits
//! the previous one's rectangle in two, first along the window's longer side
//! and then alternating between side by side and stacked. select-layout
//! switches a window between the scrolling strip and tiling, and closing a
//! pane gives its share back.

#![cfg(unix)]

mod common;

use common::{Screen, Server};

/// A server whose manually sized `sx` by `sy` window holds `panes` panes,
/// tiled, with the status line off so the window starts at the top row.
fn tiled(sx: u32, sy: u32, panes: usize) -> Server {
    let server = Server::new();
    server.start(sx, sy, panes);
    server.success(&["set", "-g", "status", "off"]);
    server.success(&["select-layout", "tiling"]);
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

/// A tiled 80 by 24 window of three panes: the first on the left, the second
/// above the third on the right.
fn left_and_right_column() -> (Server, Vec<String>) {
    let server = tiled(80, 24, 1);
    server.success(&["new-pane", "sleep 60"]);
    server.success(&["new-pane", "sleep 60"]);
    let ids = ids(&server);
    (server, ids)
}

#[test]
fn select_layout_switches_between_tiles_and_the_strip() {
    let server = tiled(81, 25, 3);
    assert!(server.display("#{window_layout}").contains("\"t\":\"v\""));
    assert_eq!(
        rects(&server),
        [(0, 40, 0, 25), (41, 40, 0, 12), (41, 40, 13, 12)]
    );
    server.success(&["select-layout", "scrolling"]);
    assert_eq!(
        rects(&server),
        [(0, 40, 0, 25), (41, 40, 0, 25), (82, 40, 0, 25)]
    );
    let refused = server.run(&["select-layout", "tiled"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("unknown layout: tiled"));
}

#[test]
fn a_new_pane_splits_the_active_pane_the_other_way_from_the_last_split() {
    let (server, ids) = left_and_right_column();
    assert_eq!(
        rects(&server),
        [(0, 40, 0, 24), (41, 39, 0, 12), (41, 39, 13, 11)]
    );
    assert_eq!(active(&server), ids[2]);
    server.success(&["kill-pane", "-t", &ids[1]]);
    assert_eq!(rects(&server), [(0, 40, 0, 24), (41, 39, 0, 24)]);
    server.success(&["kill-pane", "-t", &ids[0]]);
    assert_eq!(rects(&server), [(0, 80, 0, 24)]);
}

#[test]
fn directions_select_the_tile_across_the_separator() {
    let (server, ids) = left_and_right_column();
    server.success(&["select-pane", "-t", &ids[0]]);
    server.success(&["select-pane", "-R"]);
    assert_eq!(active(&server), ids[1]);
    server.success(&["select-pane", "-D"]);
    assert_eq!(active(&server), ids[2]);
    server.success(&["select-pane", "-D"]);
    assert_eq!(active(&server), ids[2]);
    server.success(&["select-pane", "-t", "{up-of}"]);
    assert_eq!(active(&server), ids[1]);
    server.success(&["select-pane", "-L"]);
    assert_eq!(active(&server), ids[0]);
    assert!(!server
        .run(&["select-pane", "-t", "{left-of}"])
        .status
        .success());
}

#[test]
fn swap_and_rotate_move_panes_between_tiles() {
    let (server, panes) = left_and_right_column();
    let tiles = rects(&server);
    server.success(&["swap-pane", "-s", &panes[0], "-t", &panes[2]]);
    assert_eq!(
        ids(&server),
        [&panes[2], &panes[1], &panes[0]].map(String::clone)
    );
    assert_eq!(rects(&server), tiles);
    server.success(&["rotate-window"]);
    assert_eq!(
        ids(&server),
        [&panes[1], &panes[0], &panes[2]].map(String::clone)
    );
    assert_eq!(rects(&server), tiles);
}

#[test]
fn widths_toggle_only_in_the_strip() {
    let server = tiled(80, 24, 2);
    let refused = server.run(&["resize-pane", "-W"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr)
        .contains("this layout can't change a pane's width"));
}

#[test]
fn tiles_never_shrink_below_a_pane() {
    let server = tiled(12, 4, 1);
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
        assert!(left + width <= 12 && top + height <= 4);
    }
}

#[test]
fn resizing_the_window_retiles_it() {
    let (server, _) = left_and_right_column();
    server.success(&["resize-window", "-x", "100", "-y", "30"]);
    assert_eq!(
        rects(&server),
        [(0, 50, 0, 30), (51, 49, 0, 15), (51, 49, 16, 14)]
    );
}

#[test]
fn a_pane_joining_a_tiled_window_takes_its_place_in_the_list() {
    let (server, panes) = left_and_right_column();
    server.success(&["new-window", "sleep 60"]);
    let moved = active(&server);
    server.success(&["join-pane", "-s", &moved, "-t", &panes[0]]);
    assert_eq!(
        ids(&server),
        [&panes[0], &moved, &panes[1], &panes[2]].map(String::clone)
    );
    assert_eq!(
        rects(&server),
        [
            (0, 40, 0, 24),
            (41, 39, 0, 12),
            (41, 19, 13, 11),
            (61, 19, 13, 11)
        ]
    );
}

fn shows(
    screen: &Screen,
    rows: std::ops::Range<usize>,
    columns: std::ops::Range<usize>,
    ch: char,
) -> bool {
    rows.into_iter()
        .all(|y| columns.clone().all(|x| screen.cells[y][x].ch == ch))
}

#[test]
fn horizontal_separators_meet_the_vertical_one_in_a_junction() {
    let (server, _) = left_and_right_column();
    let mut client = server.attach_terminal(80, 24);
    client.wait_screen(|screen| {
        shows(screen, 0..12, 40..41, '│')
            && shows(screen, 13..24, 40..41, '│')
            && shows(screen, 12..13, 40..41, '├')
            && shows(screen, 12..13, 41..80, '─')
    });
    server.success(&["select-layout", "scrolling"]);
    client.wait_screen(|screen| {
        shows(screen, 0..24, 39..40, '│') && (0..24).all(|y| !screen.row(y).contains('─'))
    });
}

#[test]
fn a_grid_crosses_its_separators() {
    let server = tiled(81, 25, 4);
    server.success(&["select-layout", "grid"]);
    let mut client = server.attach_terminal(81, 25);
    client.wait_screen(|screen| {
        shows(screen, 12..13, 40..41, '┼')
            && shows(screen, 12..13, 0..40, '─')
            && shows(screen, 12..13, 41..81, '─')
            && shows(screen, 0..12, 40..41, '│')
            && shows(screen, 13..25, 40..41, '│')
    });
}

#[test]
fn only_panes_on_the_edge_give_up_a_status_row() {
    let server = tiled(80, 24, 3);
    let before = rects(&server);
    server.success(&["set", "-w", "pane-border-status", "top"]);
    let expected = before
        .iter()
        .map(|&(left, width, top, height)| {
            if top == 0 {
                (left, width, 1, height - 1)
            } else {
                (left, width, top, height)
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(rects(&server), expected);
    assert!(before.iter().any(|&(_, _, top, _)| top != 0));
    server.success(&["set", "-w", "pane-border-status", "bottom"]);
    let expected = before
        .iter()
        .map(|&(left, width, top, height)| {
            if top + height == 24 {
                (left, width, top, height - 1)
            } else {
                (left, width, top, height)
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(rects(&server), expected);
    server.success(&["set", "-w", "pane-border-status", "off"]);
    assert_eq!(rects(&server), before);
}

#[test]
fn a_refused_join_leaves_both_windows_as_they_were() {
    let server = tiled(2, 2, 1);
    let full = ids(&server);
    let layout = |target: &str| {
        server.success(&[
            "list-panes",
            "-t",
            target,
            "-F",
            "#{pane_id} #{pane_left} #{pane_width} #{pane_top} #{pane_height}",
        ])
    };
    let destination = layout(&full[0]);
    server.success(&["new-window", "sleep 60"]);
    let moved = active(&server);
    let source = layout(&moved);
    let output = server.run(&["join-pane", "-s", &moved, "-t", &full[0]]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no space for a new pane"));
    assert_eq!(layout(&full[0]), destination);
    assert_eq!(layout(&moved), source);
}
