//! File-transfer interoperability with the unmodified reference from the Nix shell.
//! Run with TMUX_UPDATE_REFERENCE=/path/to/reference/tmux cargo test --test client_file_protocol.
#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Server {
    binary: PathBuf,
    directory: PathBuf,
    socket: PathBuf,
}
impl Server {
    fn new(binary: &Path, role: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tiri-file-wire-{role}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let server = Self {
            binary: binary.to_owned(),
            socket: directory.join("socket"),
            directory,
        };
        server.success(
            binary,
            &["new-session", "-d", "-s", "wire-test", "sleep 60"],
            None,
        );
        server
    }

    fn run(&self, binary: &Path, args: &[&str], input: Option<&[u8]>) -> Output {
        let mut child = Command::new(binary)
            .args(["-f", "/dev/null", "-S"])
            .arg(&self.socket)
            .args(args)
            .env_remove("TMUX")
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env("LC_ALL", "C")
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start client");
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = thread::spawn(move || read_all(stdout));
        let err = thread::spawn(move || read_all(stderr));
        let writer = input.map(|bytes| {
            let bytes = bytes.to_vec();
            let mut stdin = child.stdin.take().unwrap();
            thread::spawn(move || stdin.write_all(&bytes))
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{} {args:?} timed out", binary.display());
            }
            thread::sleep(Duration::from_millis(10));
        };
        if let Some(writer) = writer {
            let _ = writer.join().unwrap();
        }
        Output {
            status,
            stdout: out.join().unwrap(),
            stderr: err.join().unwrap(),
        }
    }

    fn success(&self, binary: &Path, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
        let output = self.run(binary, args, input);
        assert!(
            output.status.success(),
            "{} {args:?}: {output:?}",
            binary.display()
        );
        output.stdout
    }
}
fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    bytes
}
impl Drop for Server {
    fn drop(&mut self) {
        if self.socket.exists() {
            let _ = self.run(&self.binary, &["kill-server"], None);
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn exercise_files(server: &Server, client: &Path) {
    let data = (0..100_003).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let input = server.directory.join("input");
    fs::write(&input, &data).unwrap();
    server.success(
        client,
        &["load-buffer", "-b", "from-file", input.to_str().unwrap()],
        None,
    );
    assert_eq!(
        server.success(client, &["save-buffer", "-b", "from-file", "-"], None),
        data
    );

    server.success(
        client,
        &["load-buffer", "-b", "from-stdin", "-"],
        Some(&data),
    );
    assert_eq!(
        server.success(client, &["save-buffer", "-b", "from-stdin", "-"], None),
        data
    );

    let output = server.directory.join("output");
    server.success(
        client,
        &["save-buffer", "-b", "from-stdin", output.to_str().unwrap()],
        None,
    );
    assert_eq!(fs::read(&output).unwrap(), data);
    server.success(
        client,
        &[
            "save-buffer",
            "-a",
            "-b",
            "from-stdin",
            output.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(fs::read(&output).unwrap(), data.repeat(2));

    let pane = server.success(
        client,
        &[
            "new-pane",
            "-d",
            "-I",
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            "wire-test",
        ],
        Some(b"first line\r\nstreamed pane input\r\n"),
    );
    let pane = std::str::from_utf8(&pane).unwrap().trim();
    let screen = server.success(client, &["capture-pane", "-p", "-t", pane], None);
    assert!(String::from_utf8_lossy(&screen).contains("streamed pane input"));
    server.success(client, &["kill-pane", "-t", pane], None);

    let empty = server.directory.join("empty");
    fs::write(&empty, []).unwrap();
    server.success(client, &["load-buffer", empty.to_str().unwrap()], None);
    let missing = server.directory.join("missing");
    let error = server.run(client, &["load-buffer", missing.to_str().unwrap()], None);
    assert!(!error.status.success());
    assert!(
        String::from_utf8_lossy(&error.stderr).contains("No such file or directory"),
        "{error:?}"
    );

    let config = server.directory.join("source.conf");
    fs::write(
        &config,
        format!(
            "{}set-option -g @stream-wire complete\n",
            "# source data\n".repeat(3000)
        ),
    )
    .unwrap();
    server.success(client, &["source-file", config.to_str().unwrap()], None);
    assert_eq!(
        server.success(client, &["show-option", "-gqv", "@stream-wire"], None),
        b"complete\n"
    );
}

#[test]
fn file_transfers_interoperate_with_unmodified_tmux_in_both_roles() {
    let Some(reference) = std::env::var_os("TMUX_UPDATE_REFERENCE") else {
        eprintln!(
            "set TMUX_UPDATE_REFERENCE to the pinned reference to run interoperability checks"
        );
        return;
    };
    let reference = PathBuf::from(reference);
    let tiri = Path::new(env!("CARGO_BIN_EXE_tiri"));
    let tiri_server = Server::new(tiri, "tiri-server");
    exercise_files(&tiri_server, &reference);
    let reference_server = Server::new(&reference, "tmux-server");
    exercise_files(&reference_server, tiri);
}

#[test]
fn file_transfers_work_with_tiri_client_and_server() {
    let tiri = Path::new(env!("CARGO_BIN_EXE_tiri"));
    let server = Server::new(tiri, "tiri-both");
    exercise_files(&server, tiri);
}
