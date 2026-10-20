//! A control client's last output when the server goes away: the `%exit` line,
//! and for `-CC` the `ESC \` that ends its DCS wrapper.

#![cfg(unix)]

mod common;

use common::Server;
use std::fs::File;
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::fd::FromRawFd as _;
use std::process::Stdio;

#[test]
fn control_client_ends_with_exit_line() {
    let server = Server::new();
    server.success(&["new-session", "-d"]);
    let mut child = server
        .command()
        .args(["-C", "attach"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn control client");
    let mut stdin = child.stdin.take().expect("control stdin");
    stdin.write_all(b"kill-server\n").expect("send kill-server");
    let output = child.wait_with_output().expect("control client output");
    drop(stdin);
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 control output");
    assert!(stdout.ends_with("\n%exit\n"), "{stdout:?}");
    assert_eq!(output.stderr, b"");
}

#[test]
fn control_control_client_ends_with_exit_line_and_string_terminator() {
    let server = Server::new();
    server.success(&["new-session", "-d"]);
    let (mut master, mut slave) = (-1, -1);
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "openpty: {}", std::io::Error::last_os_error());
    let mut master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    let mut child = server
        .command()
        .args(["-CC", "attach"])
        .stdin(slave.try_clone().expect("clone terminal"))
        .stdout(slave.try_clone().expect("clone terminal"))
        .stderr(slave)
        .spawn()
        .expect("spawn control client");
    master
        .write_all(b"kill-server\n")
        .expect("send kill-server");
    assert!(child.wait().expect("control client exit").success());
    let mut output = Vec::new();
    let mut bytes = [0; 4096];
    loop {
        match master.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => output.extend_from_slice(&bytes[..n]),
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => panic!("terminal read: {error}"),
        }
    }
    let output = String::from_utf8(output).expect("UTF-8 control output");
    // The client's line discipline echoes input sent before it went raw.
    assert!(output.contains("\x1bP1000p%begin "), "{output:?}");
    assert!(output.ends_with("\r\n%exit\r\n\x1b\\"), "{output:?}");
}
