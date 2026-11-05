#![cfg(unix)]

mod common;

use common::Server;
use std::time::{Duration, Instant};

const STATUS: &str = "#{pane_agent}|#{pane_agent_state}|#{pane_state_emoji}";

fn wait_for_status(server: &Server, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = server.display(STATUS);
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "status stayed {actual:?}, expected {expected:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn send(server: &Server, text: &str) {
    server.success(&["send-keys", "-l", text]);
    server.success(&["send-keys", "Enter"]);
}

#[test]
fn osc_reports_update_builtin_formats_without_plugins_or_agent_detection() {
    let server = Server::new();
    let output = server.command()
        .env("TMUX_C2RS_PLUGINS", "none")
        .args([
            "new-session", "-d",
            r#"stty -echo; while IFS= read -r state; do case "$state" in prompt) printf '\033]133;A\007';; reset) printf '\033c';; *) printf '\033]7501;state=%s:app=demo\007' "$state";; esac; done"#,
        ])
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for_status(&server, "|none|⌛");
    for (state, emoji) in [
        ("idle", "💤"),
        ("working", "🔄"),
        ("blocked", "✋"),
        ("done", "✅"),
        ("error", "❌"),
    ] {
        send(&server, state);
        wait_for_status(&server, &format!("demo|{state}|{emoji}"));
    }
    send(&server, "unknown");
    send(&server, "working:id=sub/1");
    send(&server, "working:msg=!!!");
    wait_for_status(&server, "demo|error|❌");
    for clear in ["clear", "prompt", "reset"] {
        send(&server, "working");
        wait_for_status(&server, "demo|working|🔄");
        send(&server, clear);
        wait_for_status(&server, "|none|⌛");
    }
    assert_eq!(
        server.display("#{pane_agent_pid}|#{pane_agent_session_id}|#{pane_agent_model}"),
        "||"
    );
    let formats = server.success(&["display-message", "-a"]);
    assert!(formats.lines().any(|line| line == "pane_agent_state=none"));
    for removed in [
        "pane_agent_pid=",
        "pane_agent_session_id=",
        "pane_agent_model=",
    ] {
        assert!(!formats.lines().any(|line| line.starts_with(removed)));
    }
}

#[test]
fn process_exit_clears_transient_reports_and_retains_completed_reports() {
    for (state, expected) in [
        ("working", "|none|🛑"),
        ("blocked", "|none|🛑"),
        ("done", "demo|done|✅"),
        ("error", "demo|error|❌"),
    ] {
        let server = Server::new();
        server.start(80, 24, 1);
        server.success(&["set-option", "-w", "remain-on-exit", "on"]);
        let script = format!("printf '\\033]7501;state={state}:app=demo\\007'");
        server.success(&["respawn-pane", "-k", &script]);
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.display("#{pane_dead}") != "1" {
            assert!(Instant::now() < deadline, "pane did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
        wait_for_status(&server, expected);
    }
}

fn configure_emoji_status(server: &Server) {
    server.success(&["set-option", "-g", "status-interval", "0"]);
    server.success(&[
        "set-option",
        "-g",
        "status-format[0]",
        "STATE:#{pane_state_emoji}",
    ]);
    server.success(&["set-option", "-g", "automatic-rename", "off"]);
}

fn wait_for_emoji(client: &mut common::TerminalClient, emoji: &str) {
    client.wait_screen(|screen| {
        screen
            .row(screen.cells.len() - 1)
            .contains(&format!("STATE:{emoji}"))
    });
}

#[test]
fn foreground_changes_redraw_status_without_periodic_status_refresh() {
    let server = Server::new();
    let output = server
        .command()
        .env("TMUX_C2RS_PLUGINS", "none")
        .args(["new-session", "-d", "/bin/sh"])
        .output()
        .unwrap();
    assert!(output.status.success());
    configure_emoji_status(&server);
    let mut client = server.attach_terminal(80, 24);
    wait_for_emoji(&mut client, "💲");
    client.send(b"sleep 2\r");
    wait_for_emoji(&mut client, "🔧");
    wait_for_emoji(&mut client, "💲");
}

#[test]
fn same_process_exec_redraws_status_without_terminal_activity() {
    use std::os::unix::fs::symlink;

    let server = Server::new();
    let root = std::env::temp_dir().join(format!("tiri-process-exec-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let gate = root.join("go");
    let remote = root.join("ssh");
    symlink("/bin/sh", &remote).unwrap();
    let script = format!(
        "while [ ! -e '{}' ]; do sleep 0.05; done; exec '{}' -c 'sleep 1000; :'",
        gate.display(),
        remote.display()
    );
    let output = server
        .command()
        .env("TMUX_C2RS_PLUGINS", "none")
        .args(["new-session", "-d", &script])
        .output()
        .unwrap();
    assert!(output.status.success());
    configure_emoji_status(&server);
    let mut client = server.attach_terminal(80, 24);
    wait_for_emoji(&mut client, "🔧");
    let pid = server.display("#{pane_pid}");
    std::fs::write(&gate, "").unwrap();
    wait_for_emoji(&mut client, "📡");
    assert_eq!(server.display("#{pane_pid}"), pid);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn shell_events_redraw_without_changing_foreground_process() {
    let server = Server::new();
    server.success(&["new-session", "-d", "/bin/sh"]);
    configure_emoji_status(&server);
    let mut client = server.attach_terminal(80, 24);
    wait_for_emoji(&mut client, "💲");
    let pid = server.display("#{pane_pid}");
    client.send(br"printf '\033]133;C\007'; read value; printf '\033]133;A\007'");
    client.send(b"\r");
    wait_for_emoji(&mut client, "🔧");
    client.send(b"done\r");
    wait_for_emoji(&mut client, "💲");
    assert_eq!(server.display("#{pane_pid}"), pid);
}
