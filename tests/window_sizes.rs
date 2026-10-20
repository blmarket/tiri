//! Window size is the `window-size` policy result. Panes form one strip of
//! half- and full-width panes that may extend past it without feeding back
//! into it.

#![cfg(unix)]

mod common;

use common::Server;
use std::io::{BufRead as _, BufReader};
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

fn columns(server: &Server) -> Vec<(u32, u32)> {
    server
        .panes()
        .into_iter()
        .map(|(_, left, width, _, _)| (left, width))
        .collect()
}

fn ids(server: &Server) -> Vec<String> {
    server.panes().into_iter().map(|(id, ..)| id).collect()
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

#[test]
fn panes_are_half_width_in_one_row() {
    let server = Server::new();
    server.start(81, 10, 3);
    assert_eq!(columns(&server), [(0, 40), (41, 40), (82, 40)]);
    assert!(server
        .panes()
        .iter()
        .all(|&(_, _, _, top, height)| (top, height) == (0, 10)));
    assert_eq!(server.window_size(), (81, 10));
    assert_eq!(server.root_size(), (122, 10));
}

#[test]
fn even_width_leaves_a_spare_column_and_a_lone_pane_stays_half() {
    let server = Server::new();
    server.start(80, 10, 1);
    assert_eq!(columns(&server), [(0, 39)]);
    assert_eq!(server.root_size(), (39, 10));
    server.success(&["new-pane", "sleep 60"]);
    assert_eq!(columns(&server), [(0, 39), (40, 39)]);
    assert_eq!(server.root_size(), (79, 10));
    assert_eq!(server.window_size(), (80, 10));
}

#[test]
fn new_pane_goes_after_its_target_or_before_it_with_b() {
    let server = Server::new();
    server.start(41, 10, 1);
    let first = server.display("#{pane_id}");
    let second = new_pane(&server, &[]);
    assert_eq!(server.display("#{pane_id}"), second, "new pane is focused");
    let third = new_pane(&server, &["-d", "-t", &first]);
    assert_eq!(ids(&server), [&first, &third, &second].map(String::clone));
    assert_eq!(server.display("#{pane_id}"), second, "-d keeps the focus");
    let fourth = new_pane(&server, &["-b", "-t", &first]);
    assert_eq!(
        ids(&server),
        [&fourth, &first, &third, &second].map(String::clone)
    );
    assert_eq!(server.display("#{pane_id}"), fourth);
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20), (63, 20)]);
}

#[test]
fn split_window_is_an_alias_for_insertion() {
    let server = Server::new();
    server.start(41, 10, 1);
    let first = server.display("#{pane_id}");
    for flags in [&["-h"][..], &["-v", "-l", "5"], &["-f", "-p", "30"]] {
        server.success(
            &[
                &["split-window", "-t", first.as_str()][..],
                flags,
                &["sleep 60"],
            ]
            .concat(),
        );
    }
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20), (63, 20)]);
    assert_eq!(ids(&server)[0], first);
    let output = server.run(&["new-pane", "-h", "sleep 60"]);
    assert!(!output.status.success(), "new-pane takes no split flags");
}

#[test]
fn removing_a_pane_closes_the_gap_without_resizing() {
    let server = Server::new();
    server.start(41, 10, 4);
    let before = ids(&server);
    server.count_events();

    server.success(&["kill-pane", "-t", &before[1]]);
    assert_eq!(
        ids(&server),
        [&before[0], &before[2], &before[3]].map(String::clone)
    );
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20)]);
    let (layout_changed, resized) = server.events();
    assert!(layout_changed > 0, "removal fired no window-layout-changed");
    assert_eq!(resized, 0);
    assert_eq!(server.window_size(), (41, 10));

    server.count_events();
    server.success(&["kill-pane", "-t", &before[3]]);
    assert_eq!(columns(&server), [(0, 20), (21, 20)]);
    assert!(server.events().0 > 0);
}

#[test]
fn resizing_the_window_rearranges_the_strip() {
    let server = Server::new();
    server.start(41, 10, 3);
    server.success(&["resize-window", "-x", "61", "-y", "7"]);
    assert_eq!(columns(&server), [(0, 30), (31, 30), (62, 30)]);
    assert!(server.panes().iter().all(|pane| pane.4 == 7));
    server.success(&["resize-window", "-x", "2"]);
    assert_eq!(columns(&server), [(0, 1), (2, 1), (4, 1)]);
    assert_eq!(server.window_size(), (2, 7));
}

#[test]
fn recalculation_is_silent_when_nothing_moves() {
    let server = Server::new();
    server.start(41, 10, 4);
    server.count_events();
    for _ in 0..3 {
        server.success(&["set", "-w", "window-size", "manual"]);
        server.success(&["set", "-w", "pane-scrollbars", "off"]);
    }
    assert_eq!(server.events(), (0, 0), "recalculation was not silent");
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20), (63, 20)]);
}

#[test]
fn border_status_takes_a_row_from_every_pane_without_resizing() {
    let server = Server::new();
    server.start(41, 12, 3);
    server.count_events();
    server.success(&["set", "-w", "pane-border-status", "top"]);
    assert!(server
        .panes()
        .iter()
        .all(|&(_, _, _, top, height)| (top, height) == (1, 11)));
    assert_eq!(server.display("#{pane_at_top}#{pane_at_bottom}"), "11");
    server.success(&["set", "-w", "pane-border-status", "bottom"]);
    assert!(server
        .panes()
        .iter()
        .all(|&(_, _, _, top, height)| (top, height) == (0, 11)));
    assert_eq!(server.display("#{pane_at_top}#{pane_at_bottom}"), "11");
    server.success(&["set", "-w", "pane-border-status", "off"]);
    assert!(server
        .panes()
        .iter()
        .all(|&(_, _, _, top, height)| (top, height) == (0, 12)));
    assert_eq!(server.window_size(), (41, 12));
    assert_eq!(server.events().1, 0);
    server.success(&["set", "-w", "pane-border-format", "#{pane_index}"]);
}

#[test]
fn swapping_reorders_the_strip_and_stops_at_the_ends() {
    let server = Server::new();
    server.start(41, 10, 3);
    let before = ids(&server);
    server.success(&["select-pane", "-t", &before[2]]);

    server.success(&["swap-pane", "-D"]);
    assert_eq!(ids(&server), before);
    server.success(&["swap-pane", "-U"]);
    assert_eq!(
        ids(&server),
        [&before[0], &before[2], &before[1]].map(String::clone)
    );
    assert_eq!(
        server.display("#{pane_id}"),
        before[2],
        "focus moves with the pane"
    );
    server.success(&["swap-pane", "-U"]);
    assert_eq!(
        ids(&server),
        [&before[2], &before[0], &before[1]].map(String::clone)
    );
    server.success(&["swap-pane", "-U"]);
    assert_eq!(
        ids(&server),
        [&before[2], &before[0], &before[1]].map(String::clone)
    );
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20)]);
    assert_eq!(server.display("#{pane_left}"), "0");
}

#[test]
fn rotation_rotates_the_strip_order() {
    let server = Server::new();
    server.start(41, 10, 3);
    let before = ids(&server);
    server.success(&["rotate-window"]);
    assert_eq!(
        ids(&server),
        [&before[1], &before[2], &before[0]].map(String::clone)
    );
    server.success(&["rotate-window", "-D"]);
    assert_eq!(ids(&server), before);
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20)]);
}

#[test]
fn panes_move_between_strips() {
    let server = Server::new();
    server.start(41, 10, 2);
    let source = ids(&server);
    server.success(&["new-window", "sleep 60"]);
    let target = server.display("#{pane_id}");

    server.success(&["join-pane", "-s", &source[1], "-t", &target]);
    assert_eq!(ids(&server), [&target, &source[1]].map(String::clone));
    assert_eq!(columns(&server), [(0, 20), (21, 20)]);
    server.success(&["join-pane", "-b", "-s", &source[0], "-t", &target]);
    assert_eq!(
        ids(&server),
        [&source[0], &target, &source[1]].map(String::clone)
    );
    assert_eq!(server.display("#{session_windows}"), "1");

    server.success(&["break-pane", "-s", &target]);
    assert_eq!(ids(&server), [&target].map(String::clone));
    assert_eq!(columns(&server), [(0, 20)]);
    server.success(&["last-window"]);
    assert_eq!(ids(&server), [&source[0], &source[1]].map(String::clone));
    assert_eq!(columns(&server), [(0, 20), (21, 20)]);
}

/// A control client's notification lines, read on a thread as they arrive.
struct ControlLines {
    child: Child,
    lines: Receiver<String>,
    seen: Vec<String>,
}

impl ControlLines {
    fn attach(server: &Server) -> Self {
        let mut child = server
            .command()
            .args(["-C", "attach"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn control client");
        let stdout = child.stdout.take().expect("control stdout");
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut control = Self {
            child,
            lines,
            seen: Vec::new(),
        };
        control.wait_for("%session-changed");
        control.seen.clear();
        control
    }

    /// The position among the lines seen of the first starting with `prefix`,
    /// reading until it arrives.
    fn wait_for(&mut self, prefix: &str) -> usize {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(index) = self.seen.iter().position(|line| line.starts_with(prefix)) {
                return index;
            }
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => self.seen.push(line),
                Err(_) => panic!("no {prefix} line in {:#?}", self.seen),
            }
        }
    }
}

impl Drop for ControlLines {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn removing_the_active_pane_reports_the_layout_before_its_replacement() {
    for remove in [&["kill-pane", "-t"][..], &["break-pane", "-d", "-s"]] {
        let server = Server::new();
        server.start(41, 10, 2);
        let ids = ids(&server);
        server.success(&["select-pane", "-t", &ids[0]]);
        let mut control = ControlLines::attach(&server);
        server.success(&[remove, &[ids[0].as_str()]].concat());
        let changed = control.wait_for(&format!("%window-pane-changed @0 {}", ids[1]));
        let layout = control.wait_for("%layout-change @0 ");
        assert!(layout < changed, "{remove:?}: {:#?}", control.seen);
    }
}

#[test]
fn insertion_is_refused_past_the_maximum_extent() {
    let server = Server::new();
    server.start(5000, 10, 3);
    assert_eq!(columns(&server), [(0, 2499), (2500, 2499), (5000, 2499)]);
    let before = ids(&server);
    let output = server.run(&["new-pane", "sleep 60"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("no space: the strip would pass its maximum width"));
    assert_eq!(ids(&server), before);
}

#[test]
fn width_toggle_switches_one_pane_between_half_and_full() {
    let server = Server::new();
    server.start(41, 10, 3);
    let before = ids(&server);
    server.success(&["select-pane", "-t", &before[2]]);
    server.count_events();

    server.success(&["resize-pane", "-W", "-t", &before[1]]);
    assert_eq!(columns(&server), [(0, 20), (21, 41), (63, 20)]);
    assert_eq!(ids(&server), before);
    assert_eq!(server.display("#{pane_id}"), before[2], "focus stays");
    let (layout_changed, resized) = server.events();
    assert!(layout_changed > 0, "toggle fired no window-layout-changed");
    assert_eq!(resized, 0);
    assert_eq!(server.window_size(), (41, 10));
    assert_eq!(server.root_size(), (83, 10));

    server.success(&["resize-pane", "-W", "-t", &before[1]]);
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20)]);
    server.success(&["resize-pane", "-W"]);
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 41)]);
}

#[test]
fn a_lone_full_pane_fills_the_window() {
    let server = Server::new();
    server.start(80, 10, 1);
    server.success(&["resize-pane", "-W"]);
    assert_eq!(columns(&server), [(0, 80)]);
    assert_eq!(server.root_size(), (80, 10));
    server.success(&["new-pane", "sleep 60"]);
    assert_eq!(columns(&server), [(0, 80), (81, 39)]);
    assert_eq!(server.root_size(), (120, 10));
}

#[test]
fn width_preference_survives_resizing() {
    let server = Server::new();
    server.start(41, 10, 3);
    let before = ids(&server);
    server.success(&["resize-pane", "-W", "-t", &before[0]]);
    server.success(&["resize-window", "-x", "61"]);
    assert_eq!(columns(&server), [(0, 61), (62, 30), (93, 30)]);
    server.success(&["resize-window", "-x", "2"]);
    assert_eq!(columns(&server), [(0, 2), (3, 1), (5, 1)]);
}

#[test]
fn the_strip_width_moves_with_the_pane() {
    let server = Server::new();
    server.start(41, 10, 3);
    let before = ids(&server);
    server.success(&["resize-pane", "-W", "-t", &before[0]]);

    server.success(&["swap-pane", "-D", "-t", &before[0]]);
    assert_eq!(
        ids(&server),
        [&before[1], &before[0], &before[2]].map(String::clone)
    );
    assert_eq!(columns(&server), [(0, 20), (21, 41), (63, 20)]);
    server.success(&["rotate-window", "-D"]);
    assert_eq!(
        ids(&server),
        [&before[2], &before[1], &before[0]].map(String::clone)
    );
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 41)]);

    server.success(&["new-window", "sleep 60"]);
    let target = server.display("#{pane_id}");
    server.success(&["join-pane", "-s", &before[0], "-t", &target]);
    assert_eq!(ids(&server), [&target, &before[0]].map(String::clone));
    assert_eq!(columns(&server), [(0, 20), (21, 41)]);
    server.success(&["resize-pane", "-W", "-t", &before[0]]);
    server.success(&["break-pane", "-s", &before[0]]);
    assert_eq!(columns(&server), [(0, 20)]);
}

#[test]
fn widening_is_refused_past_the_maximum_extent() {
    let server = Server::new();
    server.start(5000, 10, 3);
    let before = ids(&server);
    server.count_events();
    let output = server.run(&["resize-pane", "-W", "-t", &before[0]]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("no space: the strip would pass its maximum width"));
    assert_eq!(columns(&server), [(0, 2499), (2500, 2499), (5000, 2499)]);
    assert_eq!(server.events(), (0, 0));
    server.success(&["resize-pane", "-W", "-t", &before[2]]);
    assert_eq!(columns(&server), [(0, 2499), (2500, 2499), (5000, 5000)]);
    server.success(&["resize-pane", "-W", "-t", &before[2]]);
    assert_eq!(columns(&server), [(0, 2499), (2500, 2499), (5000, 2499)]);
}

#[test]
fn narrowing_succeeds_after_a_resize_passed_the_maximum_extent() {
    let server = Server::new();
    server.start(4000, 10, 3);
    let before = ids(&server);
    server.success(&["resize-pane", "-W", "-t", &before[2]]);
    assert_eq!(columns(&server), [(0, 1999), (2000, 1999), (4000, 4000)]);
    server.success(&["resize-window", "-x", "6000"]);
    assert_eq!(columns(&server), [(0, 2999), (3000, 2999), (6000, 6000)]);
    server.success(&["resize-pane", "-W", "-t", &before[2]]);
    assert_eq!(columns(&server), [(0, 2999), (3000, 2999), (6000, 2999)]);
}

#[test]
fn a_lone_pane_widens_to_exactly_the_maximum() {
    let server = Server::new();
    server.start(10000, 10, 1);
    server.success(&["resize-pane", "-W"]);
    assert_eq!(columns(&server), [(0, 10000)]);
    let output = server.run(&["new-pane", "sleep 60"]);
    assert!(!output.status.success());
    server.success(&["resize-pane", "-W"]);
    assert_eq!(columns(&server), [(0, 4999)]);
    let output = server.run(&["new-pane", "sleep 60"]);
    assert!(!output.status.success());
}

#[test]
fn layout_only_commands_and_flags_are_gone() {
    let server = Server::new();
    server.start(41, 10, 2);
    for args in [
        &["select-layout", "tiled"][..],
        &["next-layout"],
        &["previous-layout"],
        &["resize-pane", "-x", "10"],
        &["resize-pane", "-L"],
        &["join-pane", "-h", "-s", "{last}"],
        &["set", "-w", "main-pane-width", "50"],
        &["set", "-w", "sticky-layout", "on"],
    ] {
        assert!(
            !server.run(args).status.success(),
            "{args:?} still accepted"
        );
    }
    server.success(&["resize-pane", "-T"]);
}

#[test]
fn menus_and_popups_are_available() {
    let server = Server::new();
    server.start(80, 24, 1);
    for args in [
        &["set", "-w", "menu-style", "fg=red"][..],
        &["set", "-w", "popup-border-lines", "double"],
    ] {
        server.success(args);
    }
    let commands = server.success(&["list-commands", "-F", "#{command_list_name}"]);
    assert!(commands.lines().any(|name| name == "display-menu"));
    assert!(commands.lines().any(|name| name == "display-popup"));
    let keys = server.success(&["list-keys"]);
    assert!(keys.contains("display-menu"));
}

#[test]
fn window_size_follows_policy_across_clients() {
    let server = Server::new();
    server.success(&["new-session", "-d", "-x", "40", "-y", "10", "sleep 60"]);
    let _small = server.attach_control(30, 10);
    let _large = server.attach_control(50, 12);
    server.success(&["set", "-w", "window-size", "largest"]);
    server.wait_for_size((50, 12));

    server.success(&["set", "-w", "window-size", "smallest"]);
    server.wait_for_size((30, 10));

    for _ in 0..3 {
        server.success(&["new-pane", "sleep 60"]);
    }
    assert_eq!(server.window_size(), (30, 10));
    assert_eq!(server.root_size(), (59, 10));

    server.success(&["set", "-w", "window-size", "manual"]);
    server.success(&["resize-window", "-x", "35", "-y", "11"]);
    assert_eq!(server.window_size(), (35, 11));
    server.success(&["set", "-w", "window-size", "largest"]);
    server.wait_for_size((50, 12));
}
