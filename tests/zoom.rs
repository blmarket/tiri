//! Zoom is window state: the zoomed pane, always the active one, takes the
//! whole window and the other panes are hidden in their places in the strip.

#![cfg(unix)]

mod common;

use common::Server;
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

fn flags(server: &Server) -> String {
    server.success(&["list-panes", "-F", "#{pane_flags}"])
}

/// Restart every pane with `cat`, and wait for each to run it.
fn cats(server: &Server, panes: &[String]) {
    for pane in panes {
        server.success(&["respawn-pane", "-k", "-t", pane, "cat"]);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    for pane in panes {
        while server
            .success(&[
                "display-message",
                "-p",
                "-t",
                pane,
                "#{pane_current_command}",
            ])
            .trim_end()
            != "cat"
        {
            assert!(Instant::now() < deadline, "{pane} never ran cat");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Wait until `pane` shows `text`.
fn wait_for(server: &Server, pane: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !server
        .success(&["capture-pane", "-p", "-t", pane])
        .contains(text)
    {
        assert!(Instant::now() < deadline, "{pane} never showed {text}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn zoom_gives_the_pane_the_window_and_unzoom_gives_it_back() {
    let server = Server::new();
    server.start(41, 10, 3);
    let panes = ids(&server);
    assert_eq!(server.display("#{window_zoomed_flag}"), "0");

    server.success(&["resize-pane", "-Z", "-t", &panes[0]]);
    assert_eq!(
        server.display("#{pane_id}"),
        panes[0],
        "zoom selects the pane"
    );
    assert_eq!(columns(&server), [(0, 41), (21, 20), (42, 20)]);
    assert_eq!(server.display("#{window_zoomed_flag}"), "1");
    assert!(server.display("#{window_flags}").ends_with('Z'));
    assert_eq!(flags(&server), "*Z\n\n-\n");
    assert_eq!(server.root_size(), (62, 10), "the layout keeps the strip");

    server.success(&["resize-pane", "-Z"]);
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20)]);
    assert_eq!(server.display("#{window_zoomed_flag}"), "0");
    assert_eq!(flags(&server), "*\n\n-\n");
}

#[test]
fn a_lone_pane_does_not_zoom() {
    let server = Server::new();
    server.start(80, 10, 1);
    server.success(&["resize-pane", "-Z"]);
    assert_eq!(server.display("#{window_zoomed_flag}"), "0");
    assert_eq!(columns(&server), [(0, 39)]);
}

#[test]
fn the_zoomed_pane_fills_the_view_and_hidden_panes_are_not_drawn() {
    let server = Server::new();
    server.start(41, 24, 3);
    server.success(&["set", "-g", "status", "off"]);
    let panes = ids(&server);
    for (pane, text) in panes.iter().zip(["AAAAA", "BBBBB", "CCCCC"]) {
        server.success(&[
            "respawn-pane",
            "-k",
            "-t",
            pane,
            &format!("printf {text}; exec sleep 60"),
        ]);
        wait_for(&server, pane, text);
    }
    server.success(&["resize-pane", "-Z", "-t", &panes[1]]);
    let mut client = server.attach_terminal(41, 24);
    assert_eq!(client.display(&server, "#{window_bigger}"), "0");
    client.wait_screen(|screen| screen.row(0).starts_with("BBBBB"));

    // Output in a hidden pane reaches its screen but not the client.
    server.success(&[
        "respawn-pane",
        "-k",
        "-t",
        &panes[2],
        "printf DDDDD; exec sleep 60",
    ]);
    wait_for(&server, &panes[2], "DDDDD");
    std::thread::sleep(Duration::from_millis(100));
    client.drain();
    assert!((0..24).all(|y| !client.screen.row(y).contains("DDDDD")));

    // The mouse finds only the zoomed pane.
    server.success(&["set", "-g", "mouse", "on"]);
    client.click(30, 5);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(server.display("#{pane_id}"), panes[1]);
}

#[test]
fn synchronized_input_skips_hidden_panes() {
    let server = Server::new();
    server.start(41, 10, 2);
    let panes = ids(&server);
    cats(&server, &panes);
    server.success(&["set", "-w", "synchronize-panes", "on"]);
    server.success(&["send-keys", "-t", &panes[1], "shown", "Enter"]);
    wait_for(&server, &panes[0], "shown");

    server.success(&["resize-pane", "-Z", "-t", &panes[1]]);
    server.success(&["send-keys", "-t", &panes[1], "zoomed", "Enter"]);
    wait_for(&server, &panes[1], "zoomed");
    std::thread::sleep(Duration::from_millis(100));
    assert!(!server
        .success(&["capture-pane", "-p", "-t", &panes[0]])
        .contains("zoomed"));
}

/// Three half panes of a 41-column window, the first zoomed.
fn zoomed() -> (Server, Vec<String>) {
    let server = Server::new();
    server.start(41, 10, 3);
    let panes = ids(&server);
    server.success(&["resize-pane", "-Z", "-t", &panes[0]]);
    assert_eq!(server.display("#{window_zoomed_flag}"), "1");
    (server, panes)
}

fn is_zoomed(server: &Server) -> bool {
    server.display("#{window_zoomed_flag}") == "1"
}

#[test]
fn selecting_a_hidden_pane_unzooms() {
    let (server, panes) = zoomed();
    server.success(&["select-pane", "-t", &panes[0]]);
    assert!(is_zoomed(&server), "the zoomed pane is not hidden");
    server.success(&["select-pane", "-t", &panes[2]]);
    assert!(!is_zoomed(&server));
    assert_eq!(server.display("#{pane_id}"), panes[2]);
    assert_eq!(columns(&server), [(0, 20), (21, 20), (42, 20)]);
}

type PaneListChange = dyn Fn(&Server, &[String]);

#[test]
fn changing_the_pane_list_unzooms() {
    let changes: [&PaneListChange; 4] = [
        &|server, _| {
            server.success(&["new-pane", "-d", "sleep 60"]);
        },
        &|server, panes| {
            server.success(&["kill-pane", "-t", &panes[2]]);
        },
        &|server, panes| {
            server.success(&["kill-pane", "-t", &panes[0]]);
        },
        &|server, panes| {
            server.success(&["swap-pane", "-s", &panes[1], "-t", &panes[2]]);
        },
    ];
    for change in changes {
        let (server, panes) = zoomed();
        change(&server, &panes);
        assert!(!is_zoomed(&server));
        assert!(columns(&server).iter().all(|&(_, width)| width == 20));
    }
}

type PaneCommand = dyn Fn(&[String]) -> Vec<String>;

#[test]
fn layout_and_pane_moving_commands_unzoom() {
    let commands: [&PaneCommand; 6] = [
        &|_| vec!["select-layout".into()],
        &|_| vec!["kill-pane".into(), "-a".into(), "-f".into(), "0".into()],
        &|panes| {
            vec![
                "break-pane".into(),
                "-d".into(),
                "-s".into(),
                panes[2].clone(),
            ]
        },
        &|panes| {
            ["join-pane", "-s", &panes[0], "-t", &panes[0]]
                .map(String::from)
                .to_vec()
        },
        &|_| vec!["resize-pane".into(), "-W".into()],
        &|_| vec!["resize-pane".into()],
    ];
    for command in commands {
        let (server, panes) = zoomed();
        let args = command(&panes);
        let _ = server.run(&args.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(!is_zoomed(&server), "{args:?} kept the zoom");
    }
    let (server, _) = zoomed();
    server.success(&["resize-pane", "-T"]);
    assert!(is_zoomed(&server), "trimming history keeps the zoom");
}

#[test]
fn killing_the_window_unzooms_it_first() {
    let (server, _) = zoomed();
    server.success(&["new-window", "-d", "sleep 60"]);
    server.success(&[
        "set-hook",
        "-g",
        "window-layout-changed",
        "set -g @unzoomed yes",
    ]);
    server.success(&["kill-window", "-t", ":0"]);
    assert_eq!(server.display("#{@unzoomed}"), "yes");
}

#[test]
fn a_window_resize_keeps_the_zoom() {
    let (server, panes) = zoomed();
    server.success(&["resize-window", "-x", "61"]);
    assert!(is_zoomed(&server));
    assert_eq!(server.display("#{pane_id}"), panes[0]);
    assert_eq!(columns(&server), [(0, 61), (31, 30), (62, 30)]);
    server.success(&["resize-pane", "-Z"]);
    assert_eq!(columns(&server), [(0, 30), (31, 30), (62, 30)]);
}

fn zoomed_pane(server: &Server) -> Option<String> {
    is_zoomed(server).then(|| server.display("#{pane_id}"))
}

#[test]
fn selecting_with_z_moves_the_zoom() {
    let (server, panes) = zoomed();
    server.success(&["select-pane", "-Z", "-t", &panes[2]]);
    assert_eq!(zoomed_pane(&server), Some(panes[2].clone()));
    server.success(&["last-pane", "-Z"]);
    assert_eq!(zoomed_pane(&server), Some(panes[0].clone()));
    server.success(&["select-pane", "-Z", "-R"]);
    assert_eq!(zoomed_pane(&server), Some(panes[1].clone()));
    server.success(&["select-pane", "-R"]);
    assert_eq!(zoomed_pane(&server), None);
    assert_eq!(server.display("#{pane_id}"), panes[2]);
    server.success(&["resize-pane", "-Z"]);
    server.success(&["last-pane"]);
    assert_eq!(zoomed_pane(&server), None);
}

#[test]
fn swapping_and_rotating_with_z_keep_the_active_pane_zoomed() {
    let (server, panes) = zoomed();
    server.success(&["swap-pane", "-Z", "-s", &panes[0], "-t", &panes[1]]);
    assert_eq!(
        ids(&server),
        [&panes[1], &panes[0], &panes[2]].map(String::clone)
    );
    assert_eq!(zoomed_pane(&server), Some(panes[1].clone()));
    assert_eq!(columns(&server), [(0, 41), (21, 20), (42, 20)]);
    server.success(&["rotate-window", "-Z"]);
    assert_eq!(zoomed_pane(&server), Some(server.display("#{pane_id}")));
    server.success(&["rotate-window"]);
    assert_eq!(zoomed_pane(&server), None);
}

#[test]
fn new_panes_take_the_zoom_with_z() {
    let (server, panes) = zoomed();
    let new = server.success(&["new-pane", "-Z", "-P", "-F", "#{pane_id}", "sleep 60"]);
    assert_eq!(zoomed_pane(&server), Some(new.trim_end().to_owned()));
    server.success(&["split-window", "-d", "-Z", "-t", &panes[1], "sleep 60"]);
    assert_eq!(zoomed_pane(&server), Some(new.trim_end().to_owned()));
    server.success(&["new-pane", "-d", "sleep 60"]);
    assert_eq!(zoomed_pane(&server), None);
    // -Z zooms even a window that was not zoomed.
    server.success(&["new-pane", "-Z", "sleep 60"]);
    assert!(is_zoomed(&server));
}

#[test]
fn switching_a_client_to_a_hidden_pane_with_z_moves_the_zoom() {
    let (server, panes) = zoomed();
    let client = server.attach_terminal(41, 10);
    server.success(&["switch-client", "-c", &client.tty, "-Z", "-t", &panes[1]]);
    assert_eq!(zoomed_pane(&server), Some(panes[1].clone()));
    server.success(&["switch-client", "-c", &client.tty, "-t", &panes[2]]);
    assert_eq!(zoomed_pane(&server), None);
    assert_eq!(server.display("#{pane_id}"), panes[2]);
}

#[test]
fn zoom_runs_its_hooks_and_shows_in_formats() {
    let server = Server::new();
    server.start(41, 10, 3);
    let panes = ids(&server);
    server.success(&["select-pane", "-t", &panes[0]]);
    server.success(&["set-hook", "-g", "window-zoomed", "set -gF @log '#{@log}Z'"]);
    server.success(&[
        "set-hook",
        "-g",
        "window-unzoomed",
        "set -gF @log '#{@log}U'",
    ]);
    let layout = server.display("#{window_layout}");
    assert_eq!(server.display("#{window_visible_layout}"), layout);

    server.success(&["resize-pane", "-Z", "-t", &panes[0]]);
    assert_eq!(server.display("#{@log}"), "Z");
    let pane = |pane: &str, format: &str| {
        server
            .success(&["display-message", "-p", "-t", pane, format])
            .trim_end()
            .to_owned()
    };
    let sizes = "#{pane_zoomed_flag} #{pane_width}x#{pane_height} \
                 #{pane_unzoomed_width}x#{pane_unzoomed_height}";
    assert_eq!(pane(&panes[0], sizes), "1 41x10 20x10");
    assert_eq!(pane(&panes[1], sizes), "0 20x10 20x10");
    assert_eq!(
        server.display("#{window_layout}"),
        layout,
        "the layout keeps the strip"
    );
    let visible = server.display("#{window_visible_layout}");
    let id = panes[0].trim_start_matches('%');
    assert!(
        visible.ends_with(&format!(
            r#""L":{{"t":"p","w":41,"h":10,"x":0,"y":0,"a":true,"i":0,"I":"%{id}"}}}}"#
        )),
        "{visible}"
    );

    server.success(&["resize-pane", "-Z"]);
    assert_eq!(server.display("#{@log}"), "ZU");
    assert_eq!(server.display("#{window_visible_layout}"), layout);
    assert_eq!(
        server
            .success(&["show", "-gwv", "display-panes-format"])
            .trim_end(),
        "#[align=right]#{pane_unzoomed_width}x#{pane_unzoomed_height}"
    );
}

#[test]
fn modes_entered_with_z_zoom_until_they_exit() {
    let commands: [&[&str]; 6] = [
        &["choose-tree", "-Z"],
        &["choose-client", "-Z"],
        &["choose-buffer", "-Z"],
        &["customize-mode", "-Z"],
        &["switch-mode", "-Z"],
        &["find-window", "-Z", "sleep"],
    ];
    for command in commands {
        let server = Server::new();
        server.start(41, 10, 2);
        let panes = ids(&server);
        server.success(&["set-buffer", "a buffer"]);
        let _client = server.attach_terminal(41, 10);
        server.success(&[&command[..1], &["-t", &panes[0]], &command[1..]].concat());
        assert_eq!(
            server.display("#{pane_in_mode}"),
            "1",
            "{command:?} entered no mode"
        );
        assert_eq!(zoomed_pane(&server), Some(panes[0].clone()), "{command:?}");
        server.success(&["copy-mode", "-q", "-t", &panes[0]]);
        assert_eq!(zoomed_pane(&server), None, "{command:?} kept the zoom");
    }
}

#[test]
fn a_mode_leaves_a_zoom_it_did_not_make() {
    let (server, panes) = zoomed();
    server.success(&["choose-tree", "-Z", "-t", &panes[0]]);
    server.success(&["copy-mode", "-q", "-t", &panes[0]]);
    assert_eq!(zoomed_pane(&server), Some(panes[0].clone()));
    server.success(&["resize-pane", "-Z"]);
    server.success(&["choose-tree", "-t", &panes[0]]);
    assert_eq!(zoomed_pane(&server), None, "no -Z, no zoom");
}

#[test]
fn display_panes_zooms_unless_given_z() {
    let server = Server::new();
    server.start(41, 10, 3);
    let panes = ids(&server);
    server.success(&["set", "-g", "pane-border-status", "top"]);
    server.success(&["set", "-g", "pane-scrollbars", "on"]);
    let _client = server.attach_terminal(41, 10);
    let unzoomed = server.success(&[
        "display-message",
        "-p",
        "-t",
        &panes[2],
        "#{pane_width}x#{pane_height}",
    ]);
    server.success(&["display-panes", "-d", "0", "-t", &panes[2]]);
    assert_eq!(zoomed_pane(&server), Some(panes[2].clone()));
    let pane = |format: &str| {
        server
            .success(&["display-message", "-p", "-t", &panes[2], format])
            .trim_end()
            .to_owned()
    };
    assert_eq!(
        pane("#{pane_top} #{pane_width}x#{pane_height}"),
        "0 41x10",
        "no pane status or scrollbar"
    );
    assert_eq!(
        pane("#{pane_unzoomed_width}x#{pane_unzoomed_height}"),
        unzoomed.trim_end(),
        "the unzoomed size keeps the status line and scrollbar"
    );
    server.success(&["copy-mode", "-q", "-t", &panes[2]]);
    assert_eq!(zoomed_pane(&server), None);
    assert_eq!(pane("#{pane_top} #{pane_height}"), "1 9");

    server.success(&["display-panes", "-Z", "-d", "0", "-t", &panes[2]]);
    assert_eq!(pane("#{pane_in_mode}"), "1");
    assert_eq!(zoomed_pane(&server), None);
}

#[test]
fn z_in_the_prefix_and_strip_tables_toggles_the_zoom() {
    let server = Server::new();
    server.start(41, 10, 2);
    let mut client = server.attach_terminal(41, 10);
    for keys in [&b"\x02z"[..], b"\x01z"] {
        client.send(keys);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !is_zoomed(&server) {
            assert!(Instant::now() < deadline, "{keys:?} did not zoom");
            std::thread::sleep(Duration::from_millis(20));
        }
        server.success(&["resize-pane", "-Z"]);
    }
}
