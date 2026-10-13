//! Exercise provider publication through the real command and control protocols.
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(8);
const HMUX_STATUS: &str = "#I:#{P:#{?#{m:*fable*,#{pane_agent_model}},#[bg=red],#{?#{m:*luna*,#{pane_agent_model}},#[bg=brightblue],}}#{pane_state_emoji}#[default],#{?#{>:#{window_panes},1},#{?window_active,#[nounderscore],#[underscore]},}#{?#{m:*fable*,#{pane_agent_model}},#[bg=red],#{?#{m:*luna*,#{pane_agent_model}},#[bg=brightblue],}}#{pane_state_emoji}#[default]} #{?git_worktree,#{git_worktree}#{?git_subdir,/,},#{?pane_current_path,#{b:pane_current_path},#{b:session_path}}}#{?git_action, [#{git_action}#{?git_action_total, #{git_action_step}/#{git_action_total},}],}#{?window_flags,#{window_flags}, }";

struct Server {
    root: PathBuf,
    child: Child,
}

impl Server {
    fn start(plugins: Option<&str>, config: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "hmux-plugins-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("config"), config).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_hmux"));
        command
            .args(["-D", "-S"])
            .arg(root.join("socket"))
            .arg("-f")
            .arg(root.join("config"))
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env_remove("TMUX_C2RS_PLUGINS")
            .env("SHELL", "/bin/sh")
            .env("TERM", "tmux-256color")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(plugins) = plugins {
            command.env("TMUX_C2RS_PLUGINS", plugins);
        }
        let mut server = Self {
            child: command.spawn().unwrap(),
            root,
        };
        let deadline = Instant::now() + TIMEOUT;
        while !server.root.join("socket").exists() {
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "server exited at startup"
            );
            assert!(Instant::now() < deadline, "server did not create socket");
            thread::sleep(Duration::from_millis(10));
        }
        server
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hmux"));
        command
            .arg("-S")
            .arg(self.root.join("socket"))
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TERM", "tmux-256color");
        command
    }

    fn run(&self, args: &[&str]) -> String {
        let output = self.command().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }

    fn pane(&self, directory: &str) -> String {
        self.run(&[
            "new-session",
            "-d",
            "-s",
            "plugins",
            "-P",
            "-F",
            "#{pane_id}",
            "-c",
            directory,
            "/bin/sh",
        ])
    }

    fn format(&self, pane: &str, format: &str) -> String {
        self.run(&["display-message", "-p", "-t", pane, format])
    }

    fn await_format(&self, pane: &str, format: &str, expected: &str) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let actual = self.format(pane, format);
            if actual == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{format}: expected {expected:?}, got {actual:?}"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn repo(&self) -> PathBuf {
        // The metadata layout needed by the plugin; no git executable/config
        // or commit identity is required for an unborn branch.
        let repo = self.root.join("project");
        fs::create_dir_all(repo.join(".git/refs/heads")).unwrap();
        fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        repo
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self
            .command()
            .arg("kill-server")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn defaults_match_hmux_and_plugin_output_preserves_user_options() {
    let server = Server::start(None, "");
    let repo = server.repo();
    let pane = server.pane(repo.to_str().unwrap());
    server.run(&[
        "set-option",
        "-p",
        "-t",
        &pane,
        "@git_branch",
        "user-branch",
    ]);
    server.run(&[
        "set-option",
        "-p",
        "-t",
        &pane,
        "@pane_agent_state",
        "user-state",
    ]);
    server.await_format(
        &pane,
        "#{git_branch}:#{pane_agent_state}:#{pane_state_emoji}",
        "main:none:💲",
    );
    assert_eq!(
        server.format(&pane, "#{@git_branch}:#{@pane_agent_state}"),
        "user-branch:user-state"
    );
    assert_eq!(
        server.run(&["show-options", "-gwv", "window-status-format"]),
        HMUX_STATUS
    );
    assert_eq!(
        server.run(&["show-options", "-gwv", "window-status-current-format"]),
        HMUX_STATUS
    );
    assert_eq!(server.run(&["show-options", "-gv", "prefix"]), "C-b");
    assert!(server
        .format(&pane, "#{E:window-status-format}")
        .contains("💲#[default] project"));
    server.run(&["new-pane", "-d", "-t", &pane, "-c", repo.to_str().unwrap()]);
    server.await_format(
        &pane,
        "#{E:window-status-format}",
        "0:#[nounderscore]💲#[default]💲#[default] project*",
    );
    let inactive = server.run(&[
        "new-window",
        "-d",
        "-t",
        &format!("{}:", server.format(&pane, "#{session_id}")),
        "-c",
        repo.to_str().unwrap(),
        "-P",
        "-F",
        "#{pane_id}",
    ]);
    server.run(&[
        "new-pane",
        "-d",
        "-t",
        &inactive,
        "-c",
        repo.to_str().unwrap(),
    ]);
    server.await_format(
        &inactive,
        "#{E:window-status-format}",
        "1:#[underscore]💲#[default]💲#[default] project ",
    );
    let all = server.run(&["display-message", "-a", "-t", &pane]);
    let plugin_fields: Vec<_> = all
        .lines()
        .filter(|line| {
            line.starts_with("git_")
                || line.starts_with("pane_agent")
                || line.starts_with("pane_state_emoji=")
        })
        .collect();
    assert_eq!(plugin_fields.len(), 15, "{plugin_fields:?}");
    assert!(plugin_fields.contains(&"git_branch=main"));
    fs::write(repo.join(".git/HEAD"), "ref: refs/heads/updated\n").unwrap();
    server.await_format(&pane, "#{git_branch}", "updated");
    assert_eq!(
        server.format(&pane, "#{@git_branch}:#{@pane_agent_state}"),
        "user-branch:user-state"
    );
}

#[test]
fn selection_and_configuration_override_only_presentation_defaults() {
    for disabled in ["none", ""] {
        let server = Server::start(Some(disabled), "");
        let pane = server.pane(server.root.to_str().unwrap());
        assert_eq!(
            server.format(
                &pane,
                "#{git_branch}:#{pane_agent_state}:#{pane_state_emoji}"
            ),
            "::"
        );
        assert_eq!(
            server.run(&["show-options", "-gwv", "window-status-format"]),
            "#I:#W#{?window_flags,#{window_flags}, }"
        );
    }
    let server = Server::start(Some("git"), "set -g window-status-format 'my status'\nset -g window-status-current-format 'my current'\n");
    let repo = server.repo();
    let pane = server.pane(repo.to_str().unwrap());
    server.await_format(&pane, "#{git_branch}", "main");
    assert_eq!(
        server.format(&pane, "#{pane_agent_state}:#{pane_state_emoji}"),
        ":"
    );
    assert_eq!(
        server.run(&["show-options", "-gwv", "window-status-format"]),
        "my status"
    );
    assert_eq!(
        server.run(&["show-options", "-gwv", "window-status-current-format"]),
        "my current"
    );
}

struct Control {
    child: Child,
    lines: Receiver<String>,
    reader: Option<thread::JoinHandle<()>>,
}

impl Control {
    fn new(server: &Server) -> Self {
        let mut child = server
            .command()
            .args(["-C", "attach-session", "-t", "plugins"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            reader: Some(reader),
        }
    }

    fn send(&mut self, command: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
    }

    fn await_line(&self, contains: &str) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let line = self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!("no control notification containing {contains:?}: {error}")
                });
            if line.contains(contains) {
                return;
            }
        }
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn pane_loops_and_control_subscriptions_observe_cached_changes() {
    let server = Server::start(Some("git"), "");
    let repo = server.repo();
    let first = server.pane(repo.to_str().unwrap());
    server.await_format(&first, "#{git_branch}", "main");
    let second = server.run(&[
        "new-pane",
        "-d",
        "-t",
        &first,
        "-P",
        "-F",
        "#{pane_id}",
        "-c",
        server.root.to_str().unwrap(),
        "/bin/sh",
    ]);
    server.run(&["set-environment", "-g", "git_branch", "environment-value"]);
    assert_eq!(server.format(&second, "#{git_branch}"), "");
    assert_eq!(
        server.format(&first, "#{P:#{pane_id}=#{git_branch};}"),
        format!("{first}=main;{second}=;")
    );
    let mut control = Control::new(&server);
    control.send(&format!(
        "refresh-client -B 'plugin:{first}:#{{git_branch}}'"
    ));
    control.await_line(" : main");
    fs::write(repo.join(".git/HEAD"), "ref: refs/heads/next\n").unwrap();
    control.await_line(" : next");
    server.run(&["kill-pane", "-t", &first]);
    server.await_format(&second, "#{git_branch}", "");
}
