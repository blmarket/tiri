#![cfg(unix)]

mod common;

use common::Server;
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn launch(server: &Server, args: &[&str]) -> Child {
    server
        .command()
        .arg("display-popup")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "condition did not become true");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finish(command: &mut Child) -> ExitStatus {
    let mut status = None;
    wait_for(|| {
        status = command.try_wait().unwrap();
        status.is_some()
    });
    status.unwrap()
}

#[test]
fn popup_is_a_targetable_pane_shared_by_clients_of_its_window() {
    let server = Server::new();
    server.start(80, 24, 1);
    let original = server.display("#{pane_id}");
    let mut first = server.attach_terminal(80, 24);
    let mut second = server.attach_terminal(80, 24);
    let mut command = launch(
        &server,
        &["-E", "printf 'POPUP:%s' \"$TMUX_PANE\"; read answer"],
    );
    first.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("POPUP:")));
    let popup = server.display("#{pane_id}");
    assert_ne!(popup, original);
    assert_eq!(
        server.display("#{pane_floating_flag} #{pane_modal_flag}"),
        "1 1"
    );
    assert_eq!(
        server
            .success(&["list-panes", "-F", "#{pane_id}"])
            .lines()
            .count(),
        2
    );
    server.success(&["display-popup", "-E", "exit 0"]);
    assert_eq!(server.display("#{pane_id}"), popup);
    let label = format!("POPUP:{popup}");
    second.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains(&label)));
    assert!(server
        .success(&["capture-pane", "-p", "-t", &popup])
        .contains(&label));
    server.success(&["select-pane", "-t", &original]);
    assert_eq!(server.display("#{pane_id}"), popup);
    server.success(&["send-keys", "-t", &popup, "Enter"]);
    assert!(finish(&mut command).success());
    assert_eq!(server.display("#{pane_id}"), original);
    assert_eq!(
        server.success(&["list-panes", "-F", "#{pane_id}"]).trim(),
        original
    );
}

#[test]
fn popup_preserves_tiled_geometry_and_the_zoomed_pane() {
    let server = Server::new();
    server.start(80, 24, 2);
    let original = server.display("#{pane_id}");
    server.success(&["resize-pane", "-Z"]);
    let before = server.panes();
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = launch(&server, &["-E", "printf ZOOM_POPUP; read answer"]);
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("ZOOM_POPUP")));
    assert_eq!(server.display("#{window_zoomed_flag}"), "1");
    for pane in before {
        assert!(server.panes().contains(&pane));
    }
    let layout = server.display("#{window_visible_layout}");
    assert!(layout.contains("\"z\":0"), "{layout}");
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(129));
    assert_eq!(server.display("#{pane_id}"), original);
    assert_eq!(server.display("#{window_zoomed_flag}"), "1");
}

#[test]
fn popup_captures_bindings_and_cancel_is_a_pane_close_policy() {
    let server = Server::new();
    server.start(80, 24, 1);
    server.success(&["bind-key", "-n", "x", "set -g @escaped 1"]);
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = launch(
        &server,
        &["printf READY; read answer; printf 'GOT:%s' \"$answer\"; read answer"],
    );
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("READY")));
    terminal.send(b"x\r");
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("GOT:x")));
    assert_eq!(server.display("#{@escaped}"), "");
    terminal.send(b"\x1b");
    assert_eq!(finish(&mut command).code(), Some(129));
    assert_eq!(
        server
            .success(&["list-panes", "-F", "#{pane_id}"])
            .lines()
            .count(),
        1
    );
}

#[test]
fn retained_failed_popup_requires_k_for_any_key_closure() {
    let server = Server::new();
    server.start(80, 24, 1);
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = launch(&server, &["-EE", "printf FAILED_POPUP; exit 7"]);
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("FAILED_POPUP")));
    wait_for(|| server.display("#{pane_dead}") == "1");
    terminal.send(b"\x1b");
    std::thread::sleep(Duration::from_millis(100));
    assert!(command.try_wait().unwrap().is_none());
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(7));

    let mut command = launch(&server, &["-EE", "-k", "printf KEY_POPUP; exit 8"]);
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("KEY_POPUP")));
    wait_for(|| server.display("#{pane_dead}") == "1");
    terminal.send(b" ");
    assert_eq!(finish(&mut command).code(), Some(8));
}

#[test]
fn popup_uses_pane_options_environment_and_working_directory() {
    let server = Server::new();
    server.start(80, 24, 1);
    server.success(&[
        "set-hook",
        "-g",
        "pane-title-changed",
        "set -g @popup-title-event 1",
    ]);
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = launch(
        &server,
        &[
            "-E",
            "-B",
            "-w",
            "30",
            "-h",
            "8",
            "-x",
            "4",
            "-y",
            "12",
            "-d",
            "/tmp",
            "-e",
            "POPUP_VALUE=custom",
            "-T",
            "Pane title",
            "printf '%s:%s' \"$POPUP_VALUE\" \"$PWD\"; read answer",
        ],
    );
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("custom:/tmp")));
    assert_eq!(
        server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top} #{pane_title}"),
        "30 8 4 4 Pane title"
    );
    assert_eq!(
        server
            .success(&["show-options", "-p", "-v", "pane-border-lines"])
            .trim(),
        "none"
    );
    wait_for(|| server.display("#{@popup-title-event}") == "1");
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(129));
}

#[test]
fn background_output_cannot_overwrite_the_popup_or_its_border() {
    let server = Server::new();
    server.success(&[
        "new-session",
        "-d",
        "-x",
        "80",
        "-y",
        "24",
        r"while :; do printf '\033[H\033[2JBACKGROUND'; sleep 0.03; done",
    ]);
    server.success(&["set", "-g", "window-size", "manual"]);
    let mut terminal = server.attach_terminal(80, 24);
    terminal.wait_screen(|screen| screen.row(0).contains("BACKGROUND"));
    let mut command = launch(
        &server,
        &[
            "-E",
            "-w",
            "30",
            "-h",
            "8",
            "-x",
            "10",
            "-y",
            "12",
            "printf FLOATING_CONTENT; read answer",
        ],
    );
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("FLOATING_CONTENT")));
    std::thread::sleep(Duration::from_millis(200));
    terminal.drain();
    let row = (0..24)
        .find(|&row| terminal.screen.row(row).contains("FLOATING_CONTENT"))
        .expect("popup still visible");
    let left = terminal.screen.find(row, "FLOATING_CONTENT").unwrap() - 1;
    assert_eq!(terminal.screen.cells[row - 1][left].ch, '┌');
    assert_eq!(terminal.screen.cells[row - 1][left + 29].ch, '┐');
    server.success(&["refresh-client"]);
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("FLOATING_CONTENT")));
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(129));
    terminal.wait_screen(|screen| {
        (0..24).any(|row| screen.row(row).contains("BACKGROUND"))
            && !(0..24).any(|row| screen.row(row).contains("FLOATING_CONTENT"))
    });
}

#[test]
fn popup_resize_clamps_its_geometry_and_restores_its_preferred_size() {
    let server = Server::new();
    server.start(80, 24, 1);
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = launch(
        &server,
        &[
            "-E",
            "-w",
            "40",
            "-h",
            "12",
            "-x",
            "30",
            "-y",
            "20",
            "printf RESIZE_POPUP; read answer",
        ],
    );
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("RESIZE_POPUP")));
    let before = server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top}");
    assert_eq!(before, "38 10 31 9");
    server.success(&["resize-window", "-x", "25", "-y", "10"]);
    assert_eq!(
        server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top}"),
        "23 8 1 1"
    );
    server.success(&["resize-window", "-x", "80", "-y", "24"]);
    assert_eq!(
        server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top}"),
        before
    );
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(129));
}

#[test]
fn popups_belong_to_windows_and_survive_client_detachment() {
    let server = Server::new();
    server.start(80, 24, 1);
    let first_window = server.display("#{window_id}");
    let second_window = server
        .success(&["new-window", "-d", "-P", "-F", "#{window_id}", "sleep 60"])
        .trim()
        .to_owned();
    let mut terminal = server.attach_terminal(80, 24);
    let mut first = launch(&server, &["-E", "printf FIRST_WINDOW_POPUP; read answer"]);
    terminal
        .wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("FIRST_WINDOW_POPUP")));
    let first_pane = server.display("#{pane_id}");
    let mut second = launch(
        &server,
        &[
            "-E",
            "-t",
            &second_window,
            "printf SECOND_WINDOW_POPUP; read answer",
        ],
    );
    wait_for(|| {
        server
            .success(&["list-panes", "-t", &second_window, "-F", "#{pane_id}"])
            .lines()
            .count()
            == 2
    });
    drop(terminal);
    let mut terminal = server.attach_terminal(80, 24);
    terminal
        .wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("FIRST_WINDOW_POPUP")));
    assert_eq!(server.display("#{pane_id}"), first_pane);
    server.success(&["display-popup", "-C", "-t", &second_window]);
    assert_eq!(finish(&mut second).code(), Some(129));
    assert!(first.try_wait().unwrap().is_none());
    server.success(&["display-popup", "-C", "-t", &first_window]);
    assert_eq!(finish(&mut first).code(), Some(129));
}

#[test]
fn invalid_popup_arguments_leave_the_window_unchanged() {
    let server = Server::new();
    server.start(80, 24, 1);
    let _terminal = server.attach_terminal(80, 24);
    let original = server.display("#{pane_id}");
    for args in [
        &["display-popup", "-s", "not-a-style", "sleep 60"][..],
        &["display-popup", "-b", "not-a-border", "sleep 60"],
        &["display-popup", "-w", "0", "sleep 60"],
        &["display-popup", "-N", "sleep 60"],
    ] {
        assert!(!server.run(args).status.success(), "{args:?}");
        assert_eq!(server.display("#{pane_id}"), original);
        assert_eq!(
            server.success(&["list-panes", "-F", "#{pane_id}"]).trim(),
            original
        );
    }
}

#[test]
fn popup_does_not_need_space_in_the_tiled_layout() {
    let server = Server::new();
    server.start(40, 12, 3);
    server.success(&["select-layout", "tiling"]);
    server.success(&["resize-window", "-x", "1"]);
    let _terminal = server.attach_terminal(40, 12);
    let mut command = launch(&server, &["-E", "-B", "-w", "1", "-h", "4", "read answer"]);
    wait_for(|| {
        server
            .success(&["list-panes", "-F", "#{pane_id}"])
            .lines()
            .count()
            == 4
    });
    assert_eq!(server.display("#{pane_modal_flag}"), "1");
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(129));
}

#[test]
fn popup_can_be_zoomed_and_restored_as_an_ordinary_pane() {
    let server = Server::new();
    server.start(80, 24, 1);
    let original = server.display("#{pane_id}");
    let _terminal = server.attach_terminal(80, 24);
    let mut command = launch(&server, &["-E", "-w", "30", "-h", "8", "read answer"]);
    wait_for(|| server.display("#{pane_modal_flag}") == "1");
    let popup = server.display("#{pane_id}");
    let before = server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top}");
    let saved_layout = server.display("#{window_layout}");
    server.success(&["resize-pane", "-Z", "-t", &popup]);
    assert_eq!(
        server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top}"),
        "80 24 0 0"
    );
    let visible = server.display("#{window_visible_layout}");
    assert!(visible.contains(&popup), "{visible}");
    assert!(!visible.contains("\"z\":"), "{visible}");
    assert_eq!(server.display("#{window_layout}"), saved_layout);
    server.success(&["resize-pane", "-Z", "-t", &popup]);
    assert_eq!(
        server.display("#{pane_width} #{pane_height} #{pane_left} #{pane_top}"),
        before
    );
    server.success(&["resize-pane", "-Z", "-t", &popup]);
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(129));
    assert_eq!(
        server.display("#{pane_id} #{window_zoomed_flag}"),
        format!("{original} 0")
    );
}

#[test]
fn a_retained_modal_pane_blocks_status_mouse_bindings() {
    let server = Server::new();
    server.start(80, 24, 1);
    server.success(&["set", "-g", "mouse", "on"]);
    server.success(&[
        "bind-key",
        "-n",
        "MouseDown1Status",
        "set -g @status-clicked 1",
    ]);
    let mut terminal = server.attach_terminal(80, 24);
    let mut command = launch(&server, &["-EE", "printf RETAINED_MOUSE; exit 7"]);
    terminal.wait_screen(|screen| (0..24).any(|row| screen.row(row).contains("RETAINED_MOUSE")));
    wait_for(|| server.display("#{pane_dead}") == "1");
    terminal.click(10, 23);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(server.display("#{@status-clicked}"), "");
    assert!(command.try_wait().unwrap().is_none());
    server.success(&["display-popup", "-C"]);
    assert_eq!(finish(&mut command).code(), Some(7));
}
