//! Conformance tests for the agent observer.
//!
//! These drive the observer's [`poll`] loop over synthetic panes and a
//! synthetic process table, then assert on the tracing events it emits. The
//! process table is injected through [`ProcessSource`] so attribution is
//! exercised without a real OS process table, and the pane replays a scripted
//! stream of screen/title frames so a full lifecycle can be checked
//! deterministically.
//!
//! The regression these guard against: on kernels without `CONFIG_PROC_CHILDREN`
//! the old descent (via `/proc/<pid>/task/<pid>/children`) could not reach an
//! agent running below the pane's shell, so the observer reported `Unknown` /
//! `agent=None` forever and logged nothing after startup.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CString, OsString};
use std::io;
use std::time::UNIX_EPOCH;

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;

use super::status::{AgentStatus, StatusHub};
use super::*;
use crate::observability::v1::{PaneProcess, ScreenTail};

fn poll(
    observability: &dyn ServerObservability,
    detectors: &[Box<dyn AgentDetector>],
    source: &dyn ProcessSource,
    hub: Option<&StatusHub>,
    panes: &mut HashMap<PaneId, TrackedPane>,
) {
    super::poll(
        observability,
        detectors,
        source,
        hub,
        panes,
        Instant::now(),
        Duration::ZERO,
    );
}

/// A synthetic process table: `(pid, ppid)` edges plus per-pid program names.
struct FakeProcessSource {
    table: Vec<(u32, u32)>,
    programs: HashMap<u32, Vec<OsString>>,
    arguments: HashMap<u32, Vec<OsString>>,
    /// Per-pid working directory, seeded with [`with_cwd`](Self::with_cwd).
    cwd: HashMap<u32, PathBuf>,
    /// Per-directory file listing, newest first, seeded with
    /// [`with_files`](Self::with_files).
    files: HashMap<PathBuf, Vec<PathBuf>>,
    /// Per-file contents served through `read_span`, seeded with
    /// [`with_file_content`](Self::with_file_content).
    file_contents: HashMap<PathBuf, Vec<u8>>,
    /// Per-file modification time, seeded with
    /// [`with_file_modified`](Self::with_file_modified). Absent means unreadable.
    file_modified: HashMap<PathBuf, SystemTime>,
    /// Per-pid start time, seeded with
    /// [`with_start_time`](Self::with_start_time). Absent means unreadable.
    start_time: HashMap<u32, SystemTime>,
    /// Per-pid environment, seeded with [`with_environ`](Self::with_environ).
    environ: HashMap<u32, Vec<(OsString, OsString)>>,
    open_files: HashMap<u32, Vec<PathBuf>>,
    has_process_table: bool,
    /// Number of full-table scans, so tests can assert the process table is read
    /// once per poll cadence rather than once per pane or per output frame.
    scans: Rc<Cell<usize>>,
}

impl FakeProcessSource {
    fn new(
        table: Vec<(u32, u32)>,
        programs: HashMap<u32, Vec<OsString>>,
        arguments: HashMap<u32, Vec<OsString>>,
        has_process_table: bool,
    ) -> Self {
        Self {
            table,
            programs,
            arguments,
            cwd: HashMap::new(),
            files: HashMap::new(),
            file_contents: HashMap::new(),
            file_modified: HashMap::new(),
            start_time: HashMap::new(),
            environ: HashMap::new(),
            open_files: HashMap::new(),
            has_process_table,
            scans: Rc::new(Cell::new(0)),
        }
    }

    /// Seed `pid`'s working directory.
    fn with_cwd(mut self, pid: u32, cwd: PathBuf) -> Self {
        self.cwd.insert(pid, cwd);
        self
    }

    /// Seed `dir`'s file listing, newest modified first.
    fn with_files(mut self, dir: PathBuf, files: Vec<PathBuf>) -> Self {
        self.files.insert(dir, files);
        self
    }

    /// Seed the bytes `read_span` serves for `path`.
    fn with_file_content(mut self, path: PathBuf, content: &[u8]) -> Self {
        self.file_contents.insert(path, content.to_vec());
        self
    }

    /// Seed `path`'s modification time, as seconds after an arbitrary epoch.
    fn with_file_modified(mut self, path: PathBuf, seconds: u64) -> Self {
        self.file_modified
            .insert(path, UNIX_EPOCH + Duration::from_secs(seconds));
        self
    }

    /// Seed `pid`'s start time, on the same scale as
    /// [`with_file_modified`](Self::with_file_modified).
    fn with_start_time(mut self, pid: u32, seconds: u64) -> Self {
        self.start_time
            .insert(pid, UNIX_EPOCH + Duration::from_secs(seconds));
        self
    }

    /// Seed `pid`'s environment.
    fn with_environ(mut self, pid: u32, vars: &[(&str, &str)]) -> Self {
        self.environ.insert(
            pid,
            vars.iter()
                .map(|(name, value)| (OsString::from(*name), OsString::from(*value)))
                .collect(),
        );
        self
    }

    fn with_open_file(mut self, pid: u32, path: &str) -> Self {
        self.open_files
            .entry(pid)
            .or_default()
            .push(PathBuf::from(path));
        self
    }
}

impl ProcessSource for FakeProcessSource {
    fn process_table(&self) -> Option<Vec<(u32, u32)>> {
        self.scans.set(self.scans.get() + 1);
        self.has_process_table.then(|| self.table.clone())
    }

    fn programs(&self, pid: u32) -> Vec<OsString> {
        self.programs.get(&pid).cloned().unwrap_or_default()
    }

    fn arguments(&self, pid: u32) -> Vec<OsString> {
        self.arguments.get(&pid).cloned().unwrap_or_default()
    }

    fn cwd(&self, pid: u32) -> Option<PathBuf> {
        self.cwd.get(&pid).cloned()
    }

    fn files_newest_first(&self, dir: &Path) -> Vec<PathBuf> {
        self.files.get(dir).cloned().unwrap_or_default()
    }

    fn open_files(&self, pid: u32) -> Vec<PathBuf> {
        self.open_files.get(&pid).cloned().unwrap_or_default()
    }

    fn file_len(&self, path: &Path) -> Option<u64> {
        self.file_contents
            .get(path)
            .map(|content| content.len() as u64)
    }

    fn file_modified(&self, path: &Path) -> Option<SystemTime> {
        self.file_modified.get(path).copied()
    }

    fn start_time(&self, pid: u32) -> Option<SystemTime> {
        self.start_time.get(&pid).copied()
    }

    fn environ(&self, pid: u32) -> Vec<(OsString, OsString)> {
        self.environ.get(&pid).cloned().unwrap_or_default()
    }

    fn read_span(&self, path: &Path, offset: u64, max_len: usize) -> Option<Vec<u8>> {
        let content = self.file_contents.get(path)?;
        let start = (offset as usize).min(content.len());
        let end = (start + max_len).min(content.len());
        Some(content[start..end].to_vec())
    }
}

/// One replayed observation: an optional window title and a screen tail.
#[derive(Clone)]
struct Frame {
    title: Option<CString>,
    screen: CString,
}

fn frame(title: Option<&str>, screen: &str) -> Frame {
    Frame {
        title: title.map(|value| CString::new(value).expect("title")),
        screen: CString::new(screen).expect("screen"),
    }
}

/// A pane that replays scripted frames. `output_revision` and the tail's
/// revision advance with the cursor, so each [`step`](Self::step) presents new
/// output to the observer's revision guard.
struct ScriptedPane {
    child_pid: u32,
    frames: Vec<Frame>,
    cursor: Cell<usize>,
}

impl ScriptedPane {
    fn new(child_pid: u32, frames: Vec<Frame>) -> Rc<Self> {
        Rc::new(Self {
            child_pid,
            frames,
            cursor: Cell::new(0),
        })
    }

    fn step(&self) {
        let cursor = self.cursor.get();
        if cursor + 1 < self.frames.len() {
            self.cursor.set(cursor + 1);
        }
    }

    fn current(&self) -> (usize, Frame) {
        let cursor = self.cursor.get();
        (cursor, self.frames[cursor].clone())
    }
}

impl PaneObservability for ScriptedPane {
    fn process(&self) -> io::Result<PaneProcess> {
        Ok(PaneProcess {
            child_pid: Some(self.child_pid),
            exited: false,
        })
    }

    fn output_revision(&self) -> io::Result<u64> {
        Ok(self.cursor.get() as u64)
    }

    fn screen(&self, _lines: usize) -> io::Result<ScreenTail> {
        let (revision, frame) = self.current();
        Ok(ScreenTail {
            revision: revision as u64,
            text: frame.screen,
            cursor_visible: false,
            cursor_shape: 0,
        })
    }

    fn title(&self) -> io::Result<Option<CString>> {
        Ok(self.current().1.title)
    }
}

/// A server exposing a single fixed pane.
struct FakeServer {
    pane: Rc<ScriptedPane>,
}

impl ServerObservability for FakeServer {
    fn pane_ids(&self) -> io::Result<Vec<PaneId>> {
        Ok(vec![PaneId(0)])
    }

    fn resolve_pane(&self, _id: PaneId) -> io::Result<Option<Rc<dyn PaneObservability>>> {
        Ok(Some(self.pane.clone()))
    }
}

/// A server exposing several fixed panes, one per `PaneId(index)`.
struct MultiPaneServer {
    panes: Vec<Rc<ScriptedPane>>,
}

impl ServerObservability for MultiPaneServer {
    fn pane_ids(&self) -> io::Result<Vec<PaneId>> {
        Ok((0..self.panes.len()).map(|i| PaneId(i as u32)).collect())
    }

    fn resolve_pane(&self, id: PaneId) -> io::Result<Option<Rc<dyn PaneObservability>>> {
        Ok(self
            .panes
            .get(id.0 as usize)
            .map(|pane| pane.clone() as Rc<dyn PaneObservability>))
    }
}

thread_local! {
    static CAPTURED_LOGS: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Sends formatted events to the current test thread's capture buffer.
#[derive(Clone, Copy)]
struct CaptureWriter;

impl io::Write for CaptureWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        CAPTURED_LOGS.with_borrow_mut(|buffer| buffer.extend_from_slice(bytes));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        *self
    }
}

/// Captures only this thread's events. Dynamic interest keeps tracing's shared
/// callsite cache from substituting another thread's subscriber decision.
fn capture_logs(body: impl FnOnce()) -> String {
    let previous = CAPTURED_LOGS.take();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter)
            .with_ansi(false)
            .with_filter(tracing_subscriber::filter::dynamic_filter_fn(
                |metadata, _| *metadata.level() <= tracing::Level::INFO,
            )),
    );
    tracing::subscriber::with_default(subscriber, body);
    String::from_utf8(CAPTURED_LOGS.replace(previous)).unwrap()
}

/// Assert `haystack` contains each of `needles` in order.
fn assert_ordered(haystack: &str, needles: &[&str]) {
    let mut from = 0;
    for needle in needles {
        match haystack[from..].find(needle) {
            Some(offset) => from += offset + needle.len(),
            None => panic!("expected {needle:?} after offset {from} in:\n{haystack}"),
        }
    }
}

/// The heart of the reproduction: a `claude` process running *below* the pane's
/// shell must be attributed, and its lifecycle transitions must be logged with
/// the detected agent. Before the fix the tree walk could not descend to the
/// grandchild and the observer logged only the startup `Unknown`.
#[test]
fn claude_below_shell_is_detected_and_transitions_are_logged() {
    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (
                200u32,
                vec![OsString::from("/usr/local/bin/claude_code/claude")],
            ),
        ]),
        HashMap::new(),
        true,
    );

    let pane = ScriptedPane::new(
        100,
        vec![
            frame(Some("✳ tiri"), "earlier output\n──────────\n❯ "),
            frame(Some("◐ tiri"), "running a tool…"),
            frame(
                None,
                "Edit file src/main.rs?\n\
                 ──────────────────────\n\
                 ❯ 1. Yes\n  2. No\n\
                 Do you want to proceed? · Esc to cancel",
            ),
            frame(Some("✳ tiri"), "done\n──────────\n❯ "),
        ],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();

    let hub = StatusHub::new();
    let logs = capture_logs(|| {
        let mut panes = HashMap::new();
        for _ in 0..pane.frames.len() {
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
            pane.step();
        }
    });

    let snap = hub.snapshot();
    assert_eq!(
        snap.panes.get(&PaneId(0)),
        Some(&AgentStatus {
            agent: "claude",
            pid: Some(200),
            session_id: None,
            model: None,
            state: AgentState::Idle,
        }),
        "hub should reflect the final classified status"
    );
    assert!(
        snap.revision > 1,
        "hub revision should advance on transitions"
    );

    assert!(
        logs.contains("agent=Some(\"claude\")"),
        "expected the detected agent to be logged:\n{logs}"
    );
    assert_ordered(
        &logs,
        &["state=Idle", "state=Working", "state=Blocked", "state=Idle"],
    );
    assert!(
        !logs.contains("agent=None"),
        "claude should never be reported as an unidentified pane:\n{logs}"
    );
}

/// When a `claude` process is attributed to a pane, its session id is read from
/// the newest transcript in the project directory derived from the agent's cwd,
/// and published on the pane's status so it can surface as
/// `#{pane_agent_session_id}`.
#[test]
fn claude_session_id_is_read_from_the_newest_cwd_transcript() {
    let session_id = "df817fbe-b190-483c-ba9c-a4c5d2f9d04c";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");

    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (
                200u32,
                vec![OsString::from("/usr/local/bin/claude_code/claude")],
            ),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_files(
        transcript_dir.clone(),
        vec![
            transcript_dir.join(format!("{session_id}.jsonl")),
            transcript_dir.join("5f0b4e66-2f15-406d-bbd1-fb4eaa4284a5.jsonl"),
        ],
    )
    .with_file_content(
        transcript_dir.join(format!("{session_id}.jsonl")),
        br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
{"type":"assistant","message":{"model":"claude-fable-5","role":"assistant"}}
"#,
    );

    let pane = ScriptedPane::new(
        100,
        vec![frame(Some("✳ tiri"), "earlier output\n──────────\n❯ ")],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();

    let hub = StatusHub::new();
    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(status.agent, "claude");
    assert_eq!(status.pid, Some(200));
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(session_id).expect("session id").as_c_str()),
        "the newest transcript in the cwd's project directory should be attributed"
    );
    assert_eq!(
        status.model.as_deref(),
        Some(c"claude-fable-5"),
        "the model last named in the session transcript should be published"
    );
}

/// Claude stamps every process it spawns with its session id and its own pid.
/// While such a process is alive the session is known outright, so attribution
/// must take it over any cwd guess — even one pointing at a newer, actively
/// growing neighbour transcript that the heuristic would otherwise prefer.
#[test]
fn descendant_environment_stamp_names_the_session_exactly() {
    let mine = "22222222-2222-4222-8222-222222222222";
    let neighbour = "11111111-1111-4111-8111-111111111111";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let mine_path = transcript_dir.join(format!("{mine}.jsonl"));
    let neighbour_path = transcript_dir.join(format!("{neighbour}.jsonl"));

    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100), (300, 200)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
            (300u32, vec![OsString::from("rg")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_start_time(200, 100)
    .with_environ(
        300,
        &[
            ("CLAUDE_CODE_SESSION_ID", mine),
            ("CLAUDE_PID", "200"),
            ("PATH", "/usr/bin"),
        ],
    )
    .with_files(
        transcript_dir.clone(),
        vec![neighbour_path.clone(), mine_path.clone()],
    )
    .with_file_modified(neighbour_path.clone(), 300)
    .with_file_modified(mine_path.clone(), 200)
    .with_file_content(
        neighbour_path,
        br#"{"type":"assistant","message":{"model":"claude-fable-5","role":"assistant"}}
"#,
    )
    .with_file_content(
        mine_path,
        br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
"#,
    );

    let pane = ScriptedPane::new(
        100,
        vec![frame(
            Some("\u{2733} tiri"),
            "earlier output\n──────────\n❯ ",
        )],
    );
    let server = FakeServer { pane };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(mine).expect("session id").as_c_str()),
        "the stamp on a spawned process should outrank the newest-transcript guess"
    );
    assert_eq!(
        status.model.as_deref(),
        Some(c"claude-opus-5"),
        "the model must come from the stamped session's own transcript"
    );
}

/// The stamp is an environment variable, so it is inherited: an agent launched
/// by another agent carries the launching session's id, as does everything below
/// it. Only the owning-pid variable distinguishes the two, so a stamp naming
/// some other agent must be ignored rather than attributed to this pane.
#[test]
fn inherited_environment_stamp_from_another_agent_is_ignored() {
    let outer = "11111111-1111-4111-8111-111111111111";
    let mine = "22222222-2222-4222-8222-222222222222";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let mine_path = transcript_dir.join(format!("{mine}.jsonl"));

    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100), (300, 200)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
            (300u32, vec![OsString::from("rg")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_start_time(200, 100)
    .with_environ(
        200,
        &[("CLAUDE_CODE_SESSION_ID", outer), ("CLAUDE_PID", "900")],
    )
    .with_environ(
        300,
        &[("CLAUDE_CODE_SESSION_ID", outer), ("CLAUDE_PID", "900")],
    )
    .with_files(transcript_dir.clone(), vec![mine_path.clone()])
    .with_file_modified(mine_path.clone(), 200)
    .with_file_content(
        mine_path,
        br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
"#,
    );

    let pane = ScriptedPane::new(
        100,
        vec![frame(
            Some("\u{2733} tiri"),
            "earlier output\n──────────\n❯ ",
        )],
    );
    let server = FakeServer { pane };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(mine).expect("session id").as_c_str()),
        "an inherited stamp owned by a different agent must not be attributed"
    );
    assert_eq!(status.model.as_deref(), Some(c"claude-opus-5"));
}

/// A freshly started agent has no transcript of its own until its first turn
/// completes, so the newest file in the shared project directory during that
/// window belongs to a neighbour. Adopting it publishes the neighbour's session
/// and model, and because the pane then sits idle no later correlation ever
/// revisits the choice — the wrong model stays on the status line for the life
/// of the process. A transcript that went silent before this agent even started
/// cannot be the one this agent is writing, so it must not be adopted.
#[test]
fn transcript_older_than_the_agent_is_not_adopted() {
    let neighbour = "11111111-1111-4111-8111-111111111111";
    let mine = "22222222-2222-4222-8222-222222222222";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let neighbour_path = transcript_dir.join(format!("{neighbour}.jsonl"));
    let mine_path = transcript_dir.join(format!("{mine}.jsonl"));

    let mut source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_start_time(200, 200)
    .with_files(transcript_dir.clone(), vec![neighbour_path.clone()])
    .with_file_modified(neighbour_path.clone(), 100)
    .with_file_content(
        neighbour_path.clone(),
        br#"{"type":"assistant","message":{"model":"claude-fable-5","role":"assistant"}}
"#,
    );

    let idle = frame(Some("\u{2733} tiri"), "earlier output\n──────────\n❯ ");
    let pane = ScriptedPane::new(100, vec![idle.clone(), idle]);
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);

        let snap = hub.snapshot();
        let status = snap.panes.get(&PaneId(0)).expect("status published");
        assert_eq!(status.agent, "claude");
        assert_eq!(
            status.session_id, None,
            "a transcript last written before the agent started is not its own"
        );
        assert_eq!(
            status.model, None,
            "no session attributed means no model to publish"
        );

        source.files.insert(
            transcript_dir.clone(),
            vec![mine_path.clone(), neighbour_path],
        );
        source
            .file_modified
            .insert(mine_path.clone(), UNIX_EPOCH + Duration::from_secs(300));
        source.file_contents.insert(
            mine_path,
            br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
"#
            .to_vec(),
        );
        pane.step();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(mine).expect("session id").as_c_str()),
        "the agent's own transcript should be adopted once it exists"
    );
    assert_eq!(
        status.model.as_deref(),
        Some(c"claude-opus-5"),
        "the model published must come from this agent's own session"
    );
}

/// Two panes running the same agent in the same working directory share one
/// project transcript directory, so "newest transcript" alone would flip both
/// panes to whichever session wrote last. A pane whose own transcript keeps
/// growing — or that is not even working — must keep its attribution when a
/// sibling pane's newer session appears and streams.
#[test]
fn same_cwd_sibling_transcript_does_not_steal_attribution() {
    let mine = "11111111-1111-4111-8111-111111111111";
    let sibling = "22222222-2222-4222-8222-222222222222";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let mine_path = transcript_dir.join(format!("{mine}.jsonl"));
    let sibling_path = transcript_dir.join(format!("{sibling}.jsonl"));

    let mut source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_files(transcript_dir.clone(), vec![mine_path.clone()])
    .with_file_content(
        mine_path.clone(),
        br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
"#,
    );

    let idle = frame(Some("\u{2733} tiri"), "earlier output\n──────────\n❯ ");
    let working = frame(Some("⠹ tiri"), "some streamed output");
    let mut frames = vec![idle.clone()];
    frames.extend(std::iter::repeat_n(working, 8));
    frames.push(idle);
    let pane = ScriptedPane::new(100, frames);
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
        source.files.insert(
            transcript_dir.clone(),
            vec![sibling_path.clone(), mine_path.clone()],
        );
        source
            .file_contents
            .insert(sibling_path.clone(), Vec::new());
        for round in 0..(pane.frames.len() - 1) {
            source
                .file_contents
                .get_mut(&sibling_path)
                .unwrap()
                .extend_from_slice(
                    br#"{"type":"assistant","message":{"model":"claude-fable-5"}}
"#,
                );
            if round % 2 == 0 {
                source
                    .file_contents
                    .get_mut(&mine_path)
                    .unwrap()
                    .extend_from_slice(
                        br#"{"type":"progress"}
"#,
                    );
            }
            pane.step();
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
        }
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(mine).expect("session id").as_c_str()),
        "a growing attributed transcript must not be displaced by a sibling"
    );
    assert_eq!(status.model.as_deref(), Some(c"claude-opus-5"));
}

/// The counterpart: when the pane's own agent starts a new session in the same
/// process (its old transcript goes permanently silent while the new one grows
/// with the pane's turns), correlation must adopt the new transcript — once the
/// old one has been silent long enough to tell a dead session from a tool call.
#[test]
fn silent_transcript_yields_to_newer_one_growing_while_pane_works() {
    let old = "11111111-1111-4111-8111-111111111111";
    let new = "22222222-2222-4222-8222-222222222222";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let old_path = transcript_dir.join(format!("{old}.jsonl"));
    let new_path = transcript_dir.join(format!("{new}.jsonl"));

    let mut source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_files(transcript_dir.clone(), vec![old_path.clone()])
    .with_file_content(
        old_path.clone(),
        br#"{"type":"assistant","message":{"model":"claude-opus-5","role":"assistant"}}
"#,
    );

    let idle = frame(Some("\u{2733} tiri"), "earlier output\n──────────\n❯ ");
    let working = frame(Some("⠹ tiri"), "some streamed output");
    let mut frames = vec![idle];
    frames.extend(std::iter::repeat_n(working, 12));
    let pane = ScriptedPane::new(100, frames);
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
        source
            .files
            .insert(transcript_dir.clone(), vec![new_path.clone(), old_path]);
        source.file_contents.insert(new_path.clone(), Vec::new());
        for _ in 0..(TRANSCRIPT_SILENT_POLLS + TRANSCRIPT_ADOPTION_POLLS + 1) {
            source
                .file_contents
                .get_mut(&new_path)
                .unwrap()
                .extend_from_slice(
                    br#"{"type":"assistant","message":{"model":"claude-fable-5"}}
"#,
                );
            pane.step();
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
        }
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(new).expect("session id").as_c_str()),
        "sustained correlated growth must re-attribute the pane"
    );
    assert_eq!(status.model.as_deref(), Some(c"claude-fable-5"));
}

/// A working agent writes its transcript in bursts: one long tool call leaves it
/// untouched for a minute while the pane keeps its working spinner. A sibling
/// session streaming into the shared project directory throughout such a gap
/// must not be mistaken for this pane's own — the observed failure was a pane
/// running Fable reporting a neighbouring Opus session's model.
#[test]
fn neighbour_streaming_through_a_tool_call_does_not_steal_attribution() {
    let mine = "11111111-1111-4111-8111-111111111111";
    let neighbour = "22222222-2222-4222-8222-222222222222";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let mine_path = transcript_dir.join(format!("{mine}.jsonl"));
    let neighbour_path = transcript_dir.join(format!("{neighbour}.jsonl"));

    let mut source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_files(transcript_dir.clone(), vec![mine_path.clone()])
    .with_file_content(
        mine_path.clone(),
        br#"{"type":"assistant","message":{"model":"claude-fable-5","role":"assistant"}}
"#,
    );

    let idle = frame(Some("\u{2733} tiri"), "earlier output\n──────────\n❯ ");
    let working = frame(Some("⠹ tiri"), "some streamed output");
    let pane = ScriptedPane::new(100, vec![idle, working]);
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
        source.files.insert(
            transcript_dir.clone(),
            vec![neighbour_path.clone(), mine_path.clone()],
        );
        source
            .file_contents
            .insert(neighbour_path.clone(), Vec::new());
        for _ in 0..(TRANSCRIPT_SILENT_POLLS - 1) {
            source
                .file_contents
                .get_mut(&neighbour_path)
                .unwrap()
                .extend_from_slice(
                    br#"{"type":"assistant","message":{"model":"claude-opus-5"}}
"#,
                );
            pane.step();
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
        }
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(mine).expect("session id").as_c_str()),
        "a tool call's silence must not hand the pane to a streaming neighbour"
    );
    assert_eq!(
        status.model.as_deref(),
        Some(c"claude-fable-5"),
        "the pane must keep reporting its own session's model"
    );
}

/// Once the agent has named its session outright — the stamp its tool
/// subprocesses carry — the newest-transcript guess must never overrule it, no
/// matter how long the stamped transcript stays quiet or how steadily a
/// neighbour's grows. The stamp is only readable while a tool subprocess is
/// alive, so the guess would otherwise reclaim the pane between tool calls.
#[test]
fn stamped_session_is_not_displaced_by_a_streaming_neighbour() {
    let mine = "11111111-1111-4111-8111-111111111111";
    let neighbour = "22222222-2222-4222-8222-222222222222";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = claude::transcript_dir(&cwd).expect("HOME set in test environment");
    let mine_path = transcript_dir.join(format!("{mine}.jsonl"));
    let neighbour_path = transcript_dir.join(format!("{neighbour}.jsonl"));

    let mut source = FakeProcessSource::new(
        vec![(100, 1), (200, 100), (300, 200)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("claude")]),
            (300u32, vec![OsString::from("rg")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_cwd(200, cwd)
    .with_start_time(200, 100)
    .with_environ(
        300,
        &[("CLAUDE_CODE_SESSION_ID", mine), ("CLAUDE_PID", "200")],
    )
    .with_files(transcript_dir.clone(), vec![mine_path.clone()])
    .with_file_modified(mine_path.clone(), 200)
    .with_file_content(
        mine_path.clone(),
        br#"{"type":"assistant","message":{"model":"claude-fable-5","role":"assistant"}}
"#,
    );

    let idle = frame(Some("\u{2733} tiri"), "earlier output\n──────────\n❯ ");
    let working = frame(Some("⠹ tiri"), "some streamed output");
    let pane = ScriptedPane::new(100, vec![idle, working]);
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
        source.table.retain(|(pid, _)| *pid != 300);
        source.environ.remove(&300);
        source.files.insert(
            transcript_dir.clone(),
            vec![neighbour_path.clone(), mine_path.clone()],
        );
        source
            .file_contents
            .insert(neighbour_path.clone(), Vec::new());
        source.file_modified.insert(
            neighbour_path.clone(),
            UNIX_EPOCH + Duration::from_secs(300),
        );
        for _ in 0..(TRANSCRIPT_SILENT_POLLS + 4 * TRANSCRIPT_ADOPTION_POLLS) {
            source
                .file_contents
                .get_mut(&neighbour_path)
                .unwrap()
                .extend_from_slice(
                    br#"{"type":"assistant","message":{"model":"claude-opus-5"}}
"#,
                );
            pane.step();
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
        }
    });

    let snap = hub.snapshot();
    let status = snap.panes.get(&PaneId(0)).expect("status published");
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(mine).expect("session id").as_c_str()),
        "an exactly identified session must not be replaced by a directory guess"
    );
    assert_eq!(status.model.as_deref(), Some(c"claude-fable-5"));
}

/// Pi keeps one static title across its lifecycle, so its current TUI controls
/// and status line must drive state while the newest cwd-scoped session file
/// supplies the session id.
#[test]
fn pi_lifecycle_and_session_are_published() {
    let session_id = "019f9757-7c53-7898-a7d1-c8c780212888";
    let cwd = PathBuf::from("/work/proj");
    let transcript_dir = pi::session_dir(&cwd).expect("HOME set in test environment");
    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("/usr/local/bin/pi")]),
        ]),
        HashMap::from([(200u32, vec![OsString::from("pi")])]),
        true,
    )
    .with_cwd(200, cwd)
    .with_files(
        transcript_dir.clone(),
        vec![transcript_dir.join(format!("2026-07-25T03-35-20-915Z_{session_id}.jsonl"))],
    );
    let pane = ScriptedPane::new(
        100,
        vec![
            frame(
                Some("π - proj"),
                "pi v0.80.6\n────────\n \n────────\n/work/proj",
            ),
            frame(
                Some("π - proj"),
                "running a tool\n⠹ Working...\n────────\n────────\n/work/proj",
            ),
            frame(
                Some("π - proj"),
                "⠹ Working...\n────────\nAllow command?\n→ Yes\n  No\n↑↓ navigate  enter select  esc cancel\n────────",
            ),
            frame(Some("π - proj"), "done\n────────\n \n────────\n/work/proj"),
        ],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    let logs = capture_logs(|| {
        let mut panes = HashMap::new();
        for _ in 0..pane.frames.len() {
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
            pane.step();
        }
    });

    assert_eq!(
        hub.snapshot().panes.get(&PaneId(0)),
        Some(&AgentStatus {
            agent: "pi",
            pid: Some(200),
            session_id: Some(CString::new(session_id).expect("session id")),
            model: None,
            state: AgentState::Idle,
        })
    );
    assert_ordered(
        &logs,
        &["state=Idle", "state=Working", "state=Blocked", "state=Idle"],
    );
}

/// An OpenCode process below the pane shell is attributed by its executable,
/// and its TUI interrupt and permission controls drive the lifecycle.
#[test]
fn opencode_below_shell_is_detected_and_transitions_are_logged() {
    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (
                200u32,
                vec![OsString::from("/run/current-system/sw/bin/opencode")],
            ),
        ]),
        HashMap::from([(
            200u32,
            vec![
                OsString::from("/run/current-system/sw/bin/opencode"),
                OsString::from("--yolo"),
            ],
        )]),
        true,
    );
    let pane = ScriptedPane::new(
        100,
        vec![
            frame(
                Some("OC | Executing tasks.md"),
                "done\nctrl+p commands  • OpenCode 1.18.23",
            ),
            frame(Some("OC | Executing tasks.md"), "working\nesc interrupt"),
            frame(
                Some("OC | Executing tasks.md"),
                "△ Permission required\n↑↓ select   enter confirm   esc dismiss",
            ),
            frame(
                Some("OC | Executing tasks.md"),
                "done\nctrl+p commands  • OpenCode 1.18.23",
            ),
        ],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    let logs = capture_logs(|| {
        let mut panes = HashMap::new();
        for _ in 0..pane.frames.len() {
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
            pane.step();
        }
    });

    assert_eq!(
        hub.snapshot().panes.get(&PaneId(0)),
        Some(&AgentStatus {
            agent: "opencode",
            pid: Some(200),
            session_id: None,
            model: None,
            state: AgentState::Idle,
        })
    );
    assert!(logs.contains("agent=Some(\"opencode\")"));
    assert!(!logs.contains("agent=None"));
    assert_ordered(
        &logs,
        &["state=Idle", "state=Working", "state=Blocked", "state=Idle"],
    );
}

/// agy sets no title at all, so its screen alone drives the lifecycle, and its
/// open conversation database supplies the session id. That database is sqlite,
/// which the line-oriented session scan cannot read: the model stays empty.
#[test]
fn agy_lifecycle_and_session_are_published() {
    let session_id = "019f9757-7c53-7898-a7d1-c8c780212888";
    let rule = "────────────────────────────────────────";
    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (
                200u32,
                vec![OsString::from("/run/current-system/sw/bin/agy")],
            ),
        ]),
        HashMap::from([(200u32, vec![OsString::from("agy")])]),
        true,
    )
    .with_open_file(
        200,
        &format!("/home/me/.gemini/antigravity-cli/conversations/{session_id}.db"),
    );
    let pane = ScriptedPane::new(
        100,
        vec![
            frame(
                None,
                &format!(
                    "Antigravity CLI 1.1.10\n{rule}\n> \n{rule}\n\
                     ? for shortcuts                Gemini 3.6 Flash · high"
                ),
            ),
            frame(
                None,
                &format!(
                    "⣷  Generating...\n{rule}\n> \n{rule}\n\
                     esc to cancel                  Gemini 3.6 Flash · high"
                ),
            ),
            frame(
                None,
                "● Bash(git status) (ctrl+o to expand)\nRequesting permission for:\ngit status\n\
                 Do you want to proceed?\n1. Yes\n4. No\n\
                 ↑/↓ Navigate · tab Amend · ctrl+g edit/expand command",
            ),
            frame(
                None,
                &format!(
                    "done\n{rule}\n> \n{rule}\n\
                     ? for shortcuts                Gemini 3.6 Flash · high"
                ),
            ),
        ],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    let logs = capture_logs(|| {
        let mut panes = HashMap::new();
        for _ in 0..pane.frames.len() {
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
            pane.step();
        }
    });

    assert_eq!(
        hub.snapshot().panes.get(&PaneId(0)),
        Some(&AgentStatus {
            agent: "agy",
            pid: Some(200),
            session_id: Some(CString::new(session_id).expect("session id")),
            model: None,
            state: AgentState::Idle,
        })
    );
    assert_ordered(
        &logs,
        &["state=Idle", "state=Working", "state=Blocked", "state=Idle"],
    );
}

/// A Codex thread keeps its rollout file open; the filename is a persistent
/// source that does not depend on observing a short-lived tool subprocess.
#[test]
fn resumed_codex_session_id_is_read_from_its_open_rollout() {
    let session_id = "019f70ff-c7cc-7061-8d8d-870e288c61cd";
    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100), (250, 200), (300, 250)],
        HashMap::from([
            (100u32, vec![OsString::from("zsh")]),
            (200u32, vec![OsString::from("codex")]),
            (250u32, vec![OsString::from("codex.real")]),
            (300u32, vec![OsString::from("tool")]),
        ]),
        HashMap::new(),
        true,
    )
    .with_open_file(
        250,
        &format!(
            "/home/me/.codex/sessions/2026/07/17/rollout-2026-07-17T09-53-58-{session_id}.jsonl"
        ),
    );
    let pane = ScriptedPane::new(
        100,
        vec![frame(Some("codex - project"), "OpenAI Codex\n› ")],
    );
    let server = FakeServer { pane };
    let detectors = default_detectors();
    let hub = StatusHub::new();

    capture_logs(|| {
        let mut panes = HashMap::new();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    let status = &hub.snapshot().panes[&PaneId(0)];
    assert_eq!(status.agent, "codex");
    assert_eq!(status.pid, Some(200));
    assert_eq!(
        status.session_id.as_deref(),
        Some(CString::new(session_id).expect("session id").as_c_str())
    );
}

/// Non-interactive Codex has no TUI prompt or OSC lifecycle title. Its explicit
/// `exec` invocation is the lifecycle signal: it remains working while the
/// process exists, even when a static unrelated pane title looks like the
/// interactive detector's last-resort idle signal.
#[test]
fn headless_codex_is_working_for_the_process_lifetime() {
    let source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([
            (100, vec![OsString::from("zsh")]),
            (200, vec![OsString::from("codex")]),
        ]),
        HashMap::from([(
            200,
            vec![
                OsString::from("codex"),
                OsString::from("e"),
                OsString::from("--yolo"),
                OsString::from("review the repository"),
            ],
        )]),
        true,
    );
    let pane = ScriptedPane::new(
        100,
        vec![
            frame(Some("bn4"), "diff --git a/src/main.rs b/src/main.rs"),
            frame(Some("bn4"), "running cargo test"),
        ],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();
    let mut panes = HashMap::new();

    capture_logs(|| {
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
        pane.step();
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    assert_eq!(
        hub.snapshot().panes.get(&PaneId(0)),
        Some(&AgentStatus {
            agent: "codex",
            pid: Some(200),
            session_id: None,
            model: None,
            state: AgentState::Working,
        })
    );
}

/// When process attribution is unavailable, a normal terminal title must not be
/// enough to claim that an ordinary pane is Codex. This was the macOS failure:
/// the production source had no `/proc`, so the screen-only fallback passed a
/// static hostname title to the Codex detector, whose title idle signal is only
/// valid after a Codex process has already been identified.
#[test]
fn no_process_table_static_title_does_not_identify_codex() {
    let source = FakeProcessSource::new(Vec::new(), HashMap::new(), HashMap::new(), false);
    let pane = ScriptedPane::new(
        100,
        vec![frame(
            Some("hun-mbp"),
            "agentmon    no active runs\n\nPress n to prepare one",
        )],
    );
    let server = FakeServer { pane };
    let detectors = default_detectors();
    let hub = StatusHub::new();
    let mut panes = HashMap::new();

    capture_logs(|| {
        poll(&server, &detectors, &source, Some(&hub), &mut panes);
    });

    assert_eq!(
        hub.snapshot().panes.get(&PaneId(0)),
        Some(&AgentStatus {
            agent: "",
            pid: None,
            session_id: None,
            model: None,
            state: AgentState::Unknown,
        })
    );
}

/// Unit-level guard on the descent itself: attribution must follow parent-pid
/// edges to a grandchild, not a per-process children list.
#[test]
fn tree_walk_descends_to_grandchild_via_parent_edges() {
    let source = FakeProcessSource::new(
        vec![(11, 10), (12, 11)],
        HashMap::from([
            (10u32, vec![OsString::from("tiri")]),
            (11u32, vec![OsString::from("bash")]),
            (12u32, vec![OsString::from("node")]),
        ]),
        HashMap::from([(
            12u32,
            vec![OsString::from("node"), OsString::from("claude.js")],
        )]),
        true,
    );
    let detectors = default_detectors();

    let scan = find_agent_in_tree(&ProcessSnapshot::capture(&source), 10, &detectors);
    let claude = detectors
        .iter()
        .position(|d| d.label() == "claude")
        .unwrap();
    assert!(matches!(scan, TreeScan::Found { detector, pid: 12, .. } if detector == claude));
}

/// No agent anywhere in the tree resolves to `NotFound` (an ordinary pane).
#[test]
fn tree_walk_without_agent_reports_not_found() {
    let source = FakeProcessSource::new(
        vec![(21, 20)],
        HashMap::from([
            (20u32, vec![OsString::from("zsh")]),
            (21u32, vec![OsString::from("vim")]),
        ]),
        HashMap::new(),
        true,
    );
    let detectors = default_detectors();
    assert!(matches!(
        find_agent_in_tree(&ProcessSnapshot::capture(&source), 20, &detectors),
        TreeScan::NotFound
    ));
}

/// A steady, busy server must scan the process table on the poll cadence, not on
/// output churn. Several agents stream new output every poll (advancing their
/// revision), yet the dedicated poller captures one shared process snapshot per
/// tick — so the whole process table is scanned exactly once per poll regardless
/// of how many panes there are or how much output they produce. Before the
/// caching refactor each pane re-scanned the process table on every poll, so a
/// busy server burned a CPU core re-reading an identical table `panes × polls`
/// times.
#[test]
fn steady_server_scans_proc_once_per_poll_not_per_pane_or_output() {
    const PANES: u32 = 4;
    const POLLS: usize = 6;

    let mut table = Vec::new();
    let mut programs = HashMap::new();
    for n in 1..=PANES {
        let shell = n * 100;
        let agent = shell + 1;
        table.push((shell, 1));
        table.push((agent, shell));
        programs.insert(shell, vec![OsString::from("zsh")]);
        programs.insert(agent, vec![OsString::from("claude")]);
    }
    let source = FakeProcessSource::new(table, programs, HashMap::new(), true);
    let scans = source.scans.clone();

    let working_frames = (0..POLLS)
        .map(|i| frame(Some("⠹ tiri"), &format!("running tool step {i}…")))
        .collect::<Vec<_>>();
    let server = MultiPaneServer {
        panes: (1..=PANES)
            .map(|n| ScriptedPane::new(n * 100, working_frames.clone()))
            .collect(),
    };

    let detectors = default_detectors();
    let hub = StatusHub::new();
    let mut panes = HashMap::new();
    capture_logs(|| {
        for _ in 0..POLLS {
            poll(&server, &detectors, &source, Some(&hub), &mut panes);
            for pane in &server.panes {
                pane.step();
            }
        }
    });

    assert_eq!(
        scans.get(),
        POLLS,
        "process table should be scanned once per poll cadence, \
         independent of pane count and output churn"
    );

    let snap = hub.snapshot();
    for n in 0..PANES {
        assert_eq!(
            snap.panes.get(&PaneId(n)),
            Some(&AgentStatus {
                agent: "claude",
                pid: Some((n + 1) * 100 + 1),
                session_id: None,
                model: None,
                state: AgentState::Working,
            }),
            "pane {n} should be a working claude agent"
        );
    }
}

/// An unavailable process table resolves to `NoProcessTable` so callers can fall
/// back to screen-only detection.
#[test]
fn tree_walk_without_process_table_reports_unavailable() {
    let source = FakeProcessSource::new(Vec::new(), HashMap::new(), HashMap::new(), false);
    let detectors = default_detectors();
    assert!(matches!(
        find_agent_in_tree(&ProcessSnapshot::capture(&source), 1, &detectors),
        TreeScan::NoProcessTable
    ));
}

#[test]
fn log_captures_overlap_without_blocking_or_mixing_threads() {
    use std::sync::mpsc;
    use std::time::Duration;

    fn emit(label: &str) {
        tracing::info!(label);
    }

    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first = std::thread::spawn(move || {
        capture_logs(|| {
            emit("first-thread-marker");
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        })
    });
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let second = std::thread::spawn(move || {
        let logs = capture_logs(|| emit("second-thread-marker"));
        done_tx.send(logs).unwrap();
    });
    let result = done_rx.recv_timeout(Duration::from_secs(5));
    release_tx.send(()).unwrap();
    let first_logs = first.join().unwrap();
    second.join().unwrap();
    let second_logs = result.expect("another thread's capture must not block this one");
    assert!(first_logs.contains("first-thread-marker"));
    assert!(!first_logs.contains("second-thread-marker"));
    assert!(second_logs.contains("second-thread-marker"));
    assert!(!second_logs.contains("first-thread-marker"));
}

#[test]
fn idle_requires_two_seconds_and_active_states_cancel_pending_idle() {
    let source = FakeProcessSource::new(
        vec![(100, 1)],
        HashMap::from([(100, vec![OsString::from("codex")])]),
        HashMap::new(),
        true,
    );
    let pane = ScriptedPane::new(
        100,
        vec![
            frame(None, "• Working (2s • esc to interrupt)\n› "),
            frame(None, "› "),
            frame(None, "• Working (3s • esc to interrupt)\n› "),
            frame(None, "› "),
            frame(None, "› \nAllow command?"),
            frame(None, "› "),
            frame(None, "unrecognized screen"),
            frame(None, "› "),
        ],
    );
    let server = FakeServer { pane: pane.clone() };
    let detectors = default_detectors();
    let hub = StatusHub::new();
    let mut panes = HashMap::new();
    let start = Instant::now();
    for (frame_index, milliseconds, expected) in [
        (0, 0, AgentState::Working),
        (1, 200, AgentState::Working),
        (1, 2199, AgentState::Working),
        (2, 2200, AgentState::Working),
        (3, 2400, AgentState::Working),
        (4, 4300, AgentState::Blocked),
        (5, 4400, AgentState::Blocked),
        (6, 6200, AgentState::Blocked),
        (7, 6399, AgentState::Blocked),
        (7, 6400, AgentState::Idle),
        (0, 6600, AgentState::Working),
    ] {
        pane.cursor.set(frame_index);
        super::poll(
            &server,
            &detectors,
            &source,
            Some(&hub),
            &mut panes,
            start + Duration::from_millis(milliseconds),
            IDLE_DELAY,
        );
        assert_eq!(
            hub.snapshot().panes[&PaneId(0)].state,
            expected,
            "at {milliseconds}ms"
        );
    }
}

#[test]
fn initial_idle_and_replacement_process_each_need_their_own_grace_period() {
    let mut source = FakeProcessSource::new(
        vec![(100, 1), (200, 100)],
        HashMap::from([(200, vec![OsString::from("codex")])]),
        HashMap::new(),
        true,
    );
    let pane = ScriptedPane::new(100, vec![frame(None, "› ")]);
    let server = FakeServer { pane };
    let detectors = default_detectors();
    let hub = StatusHub::new();
    let mut panes = HashMap::new();
    let start = Instant::now();
    for (milliseconds, pid, expected) in [
        (0, 200, AgentState::Unknown),
        (2000, 200, AgentState::Idle),
        (2200, 300, AgentState::Unknown),
        (4199, 300, AgentState::Unknown),
        (4200, 300, AgentState::Idle),
    ] {
        source.table = vec![(100, 1), (pid, 100)];
        source.programs = HashMap::from([(pid, vec![OsString::from("codex")])]);
        super::poll(
            &server,
            &detectors,
            &source,
            Some(&hub),
            &mut panes,
            start + Duration::from_millis(milliseconds),
            IDLE_DELAY,
        );
        let status = hub.snapshot().panes[&PaneId(0)].clone();
        assert_eq!(status.state, expected, "at {milliseconds}ms");
        assert_eq!(status.pid, Some(pid));
    }
}
