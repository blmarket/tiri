#![cfg(unix)]
mod common;
use common::Server;
use std::time::{Duration, Instant};

const QUERY: &str = "printf '\\033]4;1;?\\033\\\\\\033[c'";
const PALETTE: &[u8] = b"\x1b]4;1;rgb:cdcd/0000/0000\x1b\\";

fn alive(server: &Server, step: &str) {
    std::thread::sleep(Duration::from_millis(60));
    assert!(
        server.run(&["has-session"]).status.success(),
        "server died after: {step}"
    );
}

fn recorder(server: &Server, out: &std::path::Path) {
    let _ = std::fs::remove_file(out);
    server.success(&[
        "respawn-pane",
        "-k",
        &format!(
            "stty raw -echo; {QUERY}; exec dd bs=1 of={} 2>/dev/null",
            out.display()
        ),
    ]);
}

fn wait_for(out: &std::path::Path, needle: &[u8], within: Duration) -> Vec<u8> {
    let deadline = Instant::now() + within;
    loop {
        let bytes = std::fs::read(out).unwrap_or_default();
        if bytes.windows(needle.len()).any(|w| w == needle) || Instant::now() > deadline {
            return bytes;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn input_requests_reply_queue_timeout_cancel_and_drop() {
    let server = Server::new();
    server.start(80, 24, 1);
    let mut term = server.attach_terminal(80, 24);
    term.drain();
    let dir = std::env::temp_dir().join(format!("tiri-ir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out");

    recorder(&server, &out);
    std::thread::sleep(Duration::from_millis(150));
    term.drain();
    term.send(PALETTE);
    let got = wait_for(&out, b"\x1b[?1;2c", Duration::from_secs(2));
    let palette = got.windows(8).position(|w| w == b"\x1b]4;1;rg");
    let da = got.windows(7).position(|w| w == b"\x1b[?1;2c");
    assert!(
        matches!((palette, da), (Some(p), Some(d)) if p < d),
        "reply then queued DA: {:?}",
        String::from_utf8_lossy(&got)
    );
    alive(&server, "reply");

    recorder(&server, &out);
    let got = wait_for(&out, b"\x1b[?1;2c", Duration::from_secs(2));
    assert!(
        got.windows(7).any(|w| w == b"\x1b[?1;2c") && !got.windows(8).any(|w| w == b"\x1b]4;1;rg"),
        "timeout releases queued DA only: {:?}",
        String::from_utf8_lossy(&got)
    );
    alive(&server, "timeout");

    recorder(&server, &out);
    std::thread::sleep(Duration::from_millis(100));
    server.success(&["kill-pane", "-a"]);
    server.success(&["respawn-pane", "-k", "sleep 30"]);
    term.drain();
    term.send(PALETTE);
    alive(&server, "drop with pending request");

    recorder(&server, &out);
    std::thread::sleep(Duration::from_millis(100));
    server.success(&["detach-client", "-t", &term.tty]);
    alive(&server, "cancel on detach");
    let got = wait_for(&out, b"\x1b[?1;2c", Duration::from_secs(2));
    assert!(
        got.windows(7).any(|w| w == b"\x1b[?1;2c"),
        "cancel keeps queued DA flowing: {:?}",
        String::from_utf8_lossy(&got)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
