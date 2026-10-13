//! Prototype consumers of the native runtime observability contracts.
//!
//! A single background [`AgentObserver`] polls pane observability and classifies
//! each pane's agent lifecycle state. Detection is split into a generic harness
//! (this module) and per-agent [`AgentDetector`]s (e.g. [`codex`], [`claude`],
//! [`pi`], [`agy`], and [`opencode`]),
//! so adding an agent is a matter of adding a detector rather than another
//! poller. Running one observer with a detector registry — instead of one thread
//! per agent — avoids two observers fighting over the same pane, and matches how
//! Herdr identifies a pane's agent once and then reads only that agent's UI.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use tracing::{info, warn};

use crate::observability::v1::{PaneId, PaneObservability, ScreenSource, ServerObservability};
use crate::platform::{CurrentPlatform, Platform};

use manifest::{Bundle, Rules};

pub mod agy;
pub mod claude;
pub mod codex;
mod manifest;
pub mod opencode;
pub mod pi;
mod session_model;
pub mod status;

use session_model::ModelScan;
use status::{AgentStatus, StatusHub};

#[cfg(test)]
mod tests;

const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Rows read from the bottom of the visible screen. Herdr's rules describe the
/// visible screen, so scrollback is never read; the cap only bounds the cost of
/// a very tall pane.
const SCREEN_LINES: usize = 256;
// Filter transient idle frames during TUI redraws before consumers act on them.
const IDLE_DELAY: Duration = Duration::from_secs(2);

/// Screen/title-derived lifecycle state reported for an agent pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentState {
    /// The pane does not currently contain enough agent UI evidence.
    Unknown,
    /// The agent is showing its live input prompt.
    Idle,
    /// The agent is actively processing or running a tool.
    Working,
    /// The agent is waiting for a confirmation or answer.
    Blocked,
    /// The pane's child process has exited.
    Exited,
}

impl AgentState {
    /// The format spelling of this state, shared by `#{pane_agent_state}` and
    /// control-mode format subscriptions. [`Unknown`] maps to `"none"` — a pane
    /// with no definite agent lifecycle signal.
    ///
    /// [`Unknown`]: AgentState::Unknown
    pub fn wire_str(self) -> &'static str {
        match self {
            AgentState::Unknown => "none",
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
            AgentState::Exited => "exited",
        }
    }

    /// Compact status-bar representation, as the agent half of
    /// `#{pane_state_emoji}`. Unknown expands to an empty string, which is what
    /// lets that variable fall through to the pane's own class.
    ///
    /// Every glyph here is a single codepoint two columns wide, and a
    /// replacement must be too: the status renderer measures per codepoint, so
    /// one needing U+FE0F to reach emoji presentation would be counted as a
    /// single column while the terminal drew two, and the status line would
    /// drift. `PaneClass::emoji` carries the same requirement, and one test
    /// covers both.
    pub fn emoji(self) -> &'static str {
        match self {
            AgentState::Unknown => "",
            AgentState::Idle => "💤",
            AgentState::Working => "🔄",
            AgentState::Blocked => "✋",
            AgentState::Exited => "🏁",
        }
    }
}

/// The outcome of running a detector over one screen sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Detection {
    /// A classified state.
    State(AgentState),
    /// Transient UI (e.g. a scrollback/transcript viewer) that carries no
    /// lifecycle signal; the previously published state should be preserved.
    KeepPrevious,
}

/// Stable source used to attribute an agent process to its session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionIdSource {
    /// Inspect open files across the attributed agent process tree.
    ProcessTreeOpenFiles,
    /// Resolve the session from the attributed agent's working directory: the
    /// detector maps the cwd to a per-project session directory, and the newest
    /// file in it names the live session.
    AgentCwdTranscript,
}

/// The environment variables an agent stamps onto processes it spawns, naming
/// the session and the agent process that owns it.
pub(crate) struct SessionEnvStamp {
    /// Variable holding the session id.
    pub(crate) session_id: &'static str,
    /// Variable holding the pid of the agent that set it.
    pub(crate) owner_pid: &'static str,
}

/// Whether `text` is a canonical 8-4-4-4-12 hex UUID (case-insensitive).
pub(crate) fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

/// A per-agent recognizer: how to spot the agent's process and how to read its
/// terminal UI. Implementors are stateless — the harness owns all per-pane
/// state — so a single shared instance serves every pane.
pub(crate) trait AgentDetector {
    /// Stable label used in logs and reports (e.g. `"codex"`, `"claude"`,
    /// `"pi"`).
    fn label(&self) -> &'static str;

    /// Whether `program` — a process image name or a runtime-wrapped script
    /// name — belongs to this agent.
    fn matches_program(&self, program: &OsStr) -> bool;

    /// Whether a process argv identifies this agent. By default, check `argv[0]`
    /// and `argv[1]`: `argv[0]` covers ordinary direct execution, while `argv[1]`
    /// covers runtime wrappers such as `node path/to/claude.js` without making
    /// the platform layer understand language runtimes.
    fn matches_invocation(&self, arguments: &[OsString]) -> bool {
        arguments
            .iter()
            .take(2)
            .any(|argument| self.matches_program(argument))
    }

    /// Return a lifecycle state implied by the agent's invocation, if any.
    /// Most agents communicate state through their terminal UI. Non-interactive
    /// modes may instead have one state for the lifetime of the process.
    fn invocation_state(&self, _arguments: &[OsString]) -> Option<AgentState> {
        None
    }

    /// Where this agent exposes its stable session id. `None` means the agent
    /// has no discoverable session id.
    fn session_id_source(&self) -> Option<SessionIdSource>;

    /// Extract a session id from an open file owned by the agent process.
    fn session_id_from_open_file(&self, _path: &Path) -> Option<CString> {
        None
    }

    /// Map the attributed agent's working directory to the directory that holds
    /// its per-project session files, if the agent keeps any.
    fn session_dir_for_cwd(&self, _cwd: &Path) -> Option<PathBuf> {
        None
    }

    /// Extract a session id from a session file's name (not its full path).
    fn session_id_from_file_name(&self, _name: &OsStr) -> Option<CString> {
        None
    }

    /// The environment variables this agent stamps onto processes it spawns:
    /// `(session id, owning agent pid)`. The pid variable is what makes the
    /// stamp usable — the environment is inherited, so a nested agent carries
    /// its *parent's* session id, and only an owner that names the attributed
    /// process identifies a stamp as this agent's own. `None` when the agent
    /// does not stamp what it spawns.
    fn session_env_stamp(&self) -> Option<SessionEnvStamp> {
        None
    }

    /// Locate the session file `session_id` names, given the agent's working
    /// directory. Returns `None` when the id is not one this agent could have
    /// issued, which is what validates an id read from the environment.
    fn session_file_for_id(&self, _cwd: &Path, _session_id: &str) -> Option<PathBuf> {
        None
    }

    /// Classify a plain-text screen tail plus the optional window title.
    fn detect(&self, screen: &str, title: Option<&str>) -> Detection;
}

/// The built-in agent detectors, in dispatch priority order, each reading
/// `rules` for its own bundle.
fn detectors_with(rules: fn(&Bundle) -> Rules) -> Vec<Box<dyn AgentDetector>> {
    vec![
        Box::new(codex::CodexDetector {
            rules: rules(&codex::RULES),
        }),
        Box::new(claude::ClaudeDetector {
            rules: rules(&claude::RULES),
        }),
        Box::new(pi::PiDetector {
            rules: rules(&pi::RULES),
        }),
        Box::new(agy::AgyDetector {
            rules: rules(&agy::RULES),
        }),
        Box::new(opencode::OpencodeDetector {
            rules: rules(&opencode::RULES),
        }),
    ]
}

/// The built-in detectors with their bundled rules only.
#[cfg(test)]
pub(crate) fn default_detectors() -> Vec<Box<dyn AgentDetector>> {
    detectors_with(Rules::bundled)
}

/// Classify a pane the process tree has attributed to `detector`'s agent. The
/// agent is known to be there, so a screen no rule explains is that agent at
/// rest: Herdr's known-agent idle fallback.
pub(crate) fn detect_attributed(
    detector: &dyn AgentDetector,
    screen: &str,
    title: Option<&str>,
) -> Detection {
    match detector.detect(screen, title) {
        Detection::State(AgentState::Unknown) => Detection::State(AgentState::Idle),
        detection => detection,
    }
}

/// Observer which logs agent state transitions for native panes.
///
/// The server loop owns this and calls [`tick`](Self::tick) on the observation
/// cadence, so classification happens between the loop's other work rather than
/// on a thread of its own holding the server state lock.
pub struct AgentObserver {
    detectors: Vec<Box<dyn AgentDetector>>,
    source: Rc<dyn ProcessSource>,
    hub: Option<StatusHub>,
    panes: HashMap<PaneId, TrackedPane>,
}

impl AgentObserver {
    /// How long the loop should wait between observer sweeps. This is the gap
    /// from one sweep finishing to the next beginning, not a period the sweep
    /// runs inside: a sweep reads every visible process, and timing the next
    /// one from the start of the last would let a slow sweep schedule itself
    /// back to back.
    pub const INTERVAL: Duration = POLL_INTERVAL;

    /// Observe with the built-in detector registry, attributing panes to agents
    /// via the real OS process table. Idle is published only after two seconds
    /// of consistent evidence; other states reach `hub` immediately.
    pub fn new(hub: StatusHub) -> Self {
        Self::with(
            detectors_with(Rules::configured),
            Rc::new(SystemProcesses),
            Some(hub),
        )
    }

    /// Observe with an explicit detector registry, process source, and optional
    /// status hub (test seam). A `None` hub logs state transitions but publishes
    /// nowhere.
    pub(crate) fn with(
        detectors: Vec<Box<dyn AgentDetector>>,
        source: Rc<dyn ProcessSource>,
        hub: Option<StatusHub>,
    ) -> Self {
        Self {
            detectors,
            source,
            hub,
            panes: HashMap::new(),
        }
    }

    /// Classify every observable pane once.
    pub fn tick(&mut self, observability: &dyn ServerObservability) {
        poll(
            observability,
            &self.detectors,
            self.source.as_ref(),
            self.hub.as_ref(),
            &mut self.panes,
            Instant::now(),
            IDLE_DELAY,
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReportedStatus {
    agent: Option<&'static str>,
    state: AgentState,
    pid: Option<u32>,
    session_id: Option<CString>,
    model: Option<CString>,
}

struct TrackedPane {
    pane: Rc<dyn PaneObservability>,
    revision: Option<u64>,
    /// Last `(agent label, state, pid, session id)` tuple reported for this pane.
    /// Dedup keys on the whole tuple — not just the state — so that identifying
    /// the agent, replacing the attributed process, or discovering a late
    /// session id is published even when the classified state is unchanged.
    reported: Option<ReportedStatus>,
    /// Idle must remain observed for the grace period before it is published.
    pending_idle: Option<(Instant, ReportedStatus)>,
    /// Index into the detector registry of the last agent identified in this
    /// pane's process tree, if any.
    agent: Option<usize>,
    /// Last matched agent process id, if process attribution found one.
    agent_pid: Option<u32>,
    /// Session id resolved from the matched agent. Lookup retains the last known
    /// id when a poll's inspection is temporarily unavailable and replaces it
    /// when the agent's live session changes (a new Codex rollout, or a newer
    /// cwd-scoped transcript adopted after activity correlation — see
    /// [`should_adopt_transcript`]).
    agent_session_id: Option<CString>,
    /// Whether the attributed session was named outright by the agent's own
    /// environment stamp rather than guessed from the session directory. An
    /// exact identification is not displaced by a guess — see
    /// [`should_adopt_transcript`].
    session_stamped: bool,
    /// A newer transcript observed in the pane's session directory, held while
    /// activity correlation gathers evidence that it belongs to this pane
    /// rather than to another pane running in the same working directory.
    session_candidate: Option<SessionCandidate>,
    /// Consecutive polls in which the attributed session file did not grow,
    /// restarted whenever it grows or a different file is attributed (see
    /// [`TRANSCRIPT_SILENT_POLLS`]).
    silent_polls: u32,
    /// Incremental scanner over the resolved session file, keyed on its path so
    /// a session switch restarts the scan from the new file's beginning.
    model_scan: Option<ModelScan>,
    /// The model the session file most recently named, as published on the
    /// pane's status.
    agent_model: Option<CString>,
}

impl TrackedPane {
    /// Drop everything known about the pane's session, so the next attribution
    /// starts from scratch. Used wherever the attributed agent goes away or is
    /// replaced: nothing learned about the previous one survives it.
    fn forget_session(&mut self) {
        self.agent_session_id = None;
        self.session_stamped = false;
        self.session_candidate = None;
        self.silent_polls = 0;
        self.model_scan = None;
        self.agent_model = None;
    }
}

/// Correlated-evidence polls required before a cwd-attributed pane adopts a
/// newer transcript from its session directory: polls in which the pane's agent
/// is working, its attributed transcript has been silent for
/// [`TRANSCRIPT_SILENT_POLLS`], and the candidate is growing. Session
/// directories are keyed by working directory, so two panes running the same
/// agent in the same directory share one; newest-by-mtime alone would flip both
/// panes to whichever session wrote last. Correlation still follows a genuine
/// in-pane session switch (a new session started in the same process), since the
/// old transcript then goes permanently silent while the new one grows with the
/// pane's own turns.
const TRANSCRIPT_ADOPTION_POLLS: u32 = 5;

/// Polls the attributed transcript must have stayed silent before any evidence
/// for a replacement counts — roughly 30s at [`POLL_INTERVAL`].
///
/// A working agent writes its transcript in bursts, not continuously: a single
/// tool call leaves it untouched for a minute or more while the pane still shows
/// a working spinner. Without this floor, a neighbouring session streaming into
/// the shared directory satisfies the correlation in the length of one such gap,
/// which is how a pane running one model came to report a sibling pane's.
const TRANSCRIPT_SILENT_POLLS: u32 = 150;

/// Maximum descendants whose environment is read while looking for an agent's
/// session stamp. A tool subprocess sits within a step or two of the agent, so
/// this bounds the per-poll cost of a pane that has spawned a deep or wide tree.
const ENV_SCAN_LIMIT: usize = 32;

/// Evidence gathered about a possible replacement session file.
struct SessionCandidate {
    path: PathBuf,
    /// Last observed byte length of the candidate file.
    len: u64,
    /// Polls of correlated evidence accumulated so far (see
    /// [`TRANSCRIPT_ADOPTION_POLLS`]).
    correlated_polls: u32,
}

fn poll(
    observability: &dyn ServerObservability,
    detectors: &[Box<dyn AgentDetector>],
    source: &dyn ProcessSource,
    hub: Option<&StatusHub>,
    panes: &mut HashMap<PaneId, TrackedPane>,
    now: Instant,
    idle_delay: Duration,
) {
    let ids = match observability.pane_ids() {
        Ok(ids) => ids,
        Err(error) => {
            for tracked in panes.values_mut() {
                tracked.pending_idle = None;
                tracked.revision = None;
            }
            warn!(target: "tiri::integration", %error, "could not list observable panes");
            return;
        }
    };
    // Scan the whole process table once per poll and share the cached snapshot
    // (read-only) with every pane inspected this tick. Rebuilding the parent→child
    // index requires reading every visible process, which dominates
    // attribution cost; doing it here — the dedicated poller — rather than inside
    // each pane's tree walk means a busy server with many streaming agents scans
    // the process table once per poll cadence instead of once per pane per poll.
    let snapshot = ProcessSnapshot::capture(source);

    let current = ids.iter().copied().collect::<HashSet<_>>();
    panes.retain(|id, tracked| {
        if current.contains(id) {
            true
        } else {
            info!(
                target: "tiri::integration",
                pane_id = id.0,
                previous = ?tracked.reported,
                state = "removed",
                "agent integration state changed"
            );
            if let Some(hub) = hub {
                hub.remove(*id);
            }
            false
        }
    });

    for id in ids {
        if let std::collections::hash_map::Entry::Vacant(entry) = panes.entry(id) {
            match observability.resolve_pane(id) {
                Ok(Some(pane)) => {
                    entry.insert(TrackedPane {
                        pane,
                        revision: None,
                        reported: None,
                        pending_idle: None,
                        agent: None,
                        agent_pid: None,
                        agent_session_id: None,
                        session_stamped: false,
                        session_candidate: None,
                        silent_polls: 0,
                        model_scan: None,
                        agent_model: None,
                    });
                }
                Ok(None) => continue,
                Err(error) => {
                    warn!(
                        target: "tiri::integration",
                        pane_id = id.0,
                        %error,
                        "could not resolve observable pane"
                    );
                    continue;
                }
            }
        }

        if let Some(tracked) = panes.get_mut(&id) {
            inspect(id, tracked, detectors, &snapshot, hub, now, idle_delay);
        }
    }
}

/// The result of scanning a pane's process tree for a known agent.
#[derive(Debug)]
enum TreeScan {
    /// The process table is unavailable; the agent cannot be identified by process.
    NoProcessTable,
    /// A known agent was found; the value indexes the detector registry.
    Found {
        detector: usize,
        pid: u32,
        invocation_state: Option<AgentState>,
    },
    /// The process table is available and no known agent runs in the tree.
    NotFound,
}

fn inspect(
    id: PaneId,
    tracked: &mut TrackedPane,
    detectors: &[Box<dyn AgentDetector>],
    snapshot: &ProcessSnapshot,
    hub: Option<&StatusHub>,
    now: Instant,
    idle_delay: Duration,
) {
    // Only successful, consecutive observations may confirm pending idle.
    let pending_idle = tracked.pending_idle.take();
    if pending_idle.is_some() {
        tracked.revision = None;
    }
    let process = match tracked.pane.process() {
        Ok(process) => process,
        Err(error) => {
            warn!(
                target: "tiri::integration",
                pane_id = id.0,
                %error,
                "could not inspect pane process"
            );
            return;
        }
    };

    if process.exited {
        tracked.agent_pid = None;
        tracked.forget_session();
        publish(
            id,
            tracked,
            AgentState::Exited,
            PublishMetadata {
                revision: tracked.revision,
                child_pid: process.child_pid,
                agent_pid: None,
                agent: tracked.agent.map(|i| detectors[i].label()),
                hub,
            },
        );
        return;
    }

    // Identify which agent (if any) owns this pane from its process tree.
    let scan = match process.child_pid {
        Some(pid) => find_agent_in_tree(snapshot, pid, detectors),
        // A process-less fixture pane can't be attributed by process; fall back
        // to screen-only detection across the whole registry.
        None => TreeScan::NoProcessTable,
    };

    let revision = match tracked.pane.output_revision() {
        Ok(revision) => revision,
        Err(error) => {
            warn!(
                target: "tiri::integration",
                pane_id = id.0,
                %error,
                "could not inspect pane output revision"
            );
            return;
        }
    };

    // The process tree says no known agent runs here: this is not an agent pane.
    if let TreeScan::NotFound = scan {
        tracked.agent = None;
        tracked.agent_pid = None;
        tracked.forget_session();
        tracked.revision = Some(revision);
        publish(
            id,
            tracked,
            AgentState::Unknown,
            PublishMetadata {
                revision: Some(revision),
                child_pid: process.child_pid,
                agent_pid: None,
                agent: None,
                hub,
            },
        );
        return;
    }

    // A newly identified agent or replacement pid forces a re-scan even if output
    // hasn't advanced.
    let agent_started =
        matches!(scan, TreeScan::Found { detector, .. } if tracked.agent != Some(detector));
    let agent_pid_changed = match scan {
        TreeScan::Found { pid, .. } => tracked.agent_pid != Some(pid),
        TreeScan::NoProcessTable => tracked.agent_pid.is_some(),
        TreeScan::NotFound => false,
    };
    let previous_session_id = tracked.agent_session_id.clone();
    let previous_model = tracked.agent_model.clone();
    match scan {
        TreeScan::Found { detector, pid, .. } => {
            let agent_changed = tracked.agent != Some(detector) || tracked.agent_pid != Some(pid);
            if agent_changed {
                tracked.forget_session();
            }
            // Read any bytes the agent appended to its session file since the
            // last poll; the newest model named there replaces the published
            // one. No new mention retains the current value. Growth doubles as
            // the liveness signal for adoption correlation below.
            let session_file_grew = advance_model_scan(tracked, snapshot);
            tracked.silent_polls = if session_file_grew {
                0
            } else {
                tracked.silent_polls.saturating_add(1)
            };
            // An agent that stamps the processes it spawns names its session
            // outright, so that reading is preferred over every heuristic. It is
            // only available while a spawned process is alive, so a miss falls
            // through to the declared source rather than clearing anything.
            if let Some((session_id, session_file)) =
                find_descendant_env_session(snapshot, pid, detectors[detector].as_ref())
            {
                adopt_session(tracked, snapshot, session_id, session_file, true);
            } else if let Some(source) = detectors[detector].session_id_source() {
                // Both sources are rechecked every poll so switching threads or
                // sessions within one agent process replaces the cached id. Each
                // only overwrites on a positive read, so a transient inspection
                // failure retains the last known id rather than clearing it.
                let resolved = match source {
                    SessionIdSource::ProcessTreeOpenFiles => {
                        find_open_file_session_in_tree(snapshot, pid, detectors[detector].as_ref())
                    }
                    SessionIdSource::AgentCwdTranscript => {
                        find_cwd_transcript_session(snapshot, pid, detectors[detector].as_ref())
                    }
                };
                if let Some((session_id, session_file)) = resolved {
                    let adopt = match source {
                        // An open descriptor names its owning process; no
                        // correlation is needed.
                        SessionIdSource::ProcessTreeOpenFiles => true,
                        // The newest transcript in a cwd-keyed directory may
                        // belong to another pane in the same working directory.
                        SessionIdSource::AgentCwdTranscript => should_adopt_transcript(
                            tracked,
                            &session_file,
                            session_file_grew,
                            snapshot,
                        ),
                    };
                    if adopt {
                        adopt_session(tracked, snapshot, session_id, session_file, false);
                    }
                }
            }
            tracked.agent = Some(detector);
            tracked.agent_pid = Some(pid);
        }
        TreeScan::NoProcessTable => {
            tracked.agent_pid = None;
            tracked.forget_session();
        }
        TreeScan::NotFound => {}
    }
    let session_id_changed = tracked.agent_session_id != previous_session_id;
    let model_changed = tracked.agent_model != previous_model;
    if !agent_started
        && !agent_pid_changed
        && !session_id_changed
        && !model_changed
        && tracked.revision == Some(revision)
    {
        return;
    }

    if let TreeScan::Found {
        detector,
        pid,
        invocation_state: Some(state),
    } = scan
    {
        tracked.revision = Some(revision);
        publish(
            id,
            tracked,
            state,
            PublishMetadata {
                revision: Some(revision),
                child_pid: process.child_pid,
                agent_pid: Some(pid),
                agent: Some(detectors[detector].label()),
                hub,
            },
        );
        return;
    }

    // The window title is a strong status channel for some agents, but
    // best-effort: a read error must not suppress screen-based detection.
    let title = match tracked.pane.title() {
        Ok(title) => title,
        Err(error) => {
            warn!(
                target: "tiri::integration",
                pane_id = id.0,
                %error,
                "could not read pane title"
            );
            None
        }
    };

    let screen = match tracked.pane.screen(ScreenSource::Visible, SCREEN_LINES) {
        Ok(screen) => screen,
        Err(error) => {
            warn!(
                target: "tiri::integration",
                pane_id = id.0,
                %error,
                "could not read pane screen"
            );
            return;
        }
    };
    tracked.revision = Some(screen.revision);

    let screen_text = screen.text.to_string_lossy();
    let title_text = title.as_deref().map(CStr::to_string_lossy);
    let title_text = title_text.as_deref();
    let (detection, label, agent_pid) = match scan {
        TreeScan::Found { detector, pid, .. } => (
            detect_attributed(detectors[detector].as_ref(), &screen_text, title_text),
            Some(detectors[detector].label()),
            Some(pid),
        ),
        // Process attribution unavailable: run the whole registry and take the
        // first definite classification. Do not pass the terminal title here:
        // Codex uses any non-empty OSC title as a last-resort idle signal once
        // the process tree has already identified Codex, but ordinary shells on
        // macOS often have a static hostname title.
        TreeScan::NoProcessTable => {
            let (detection, label) = run_all(detectors, &screen_text, None);
            (detection, label, None)
        }
        TreeScan::NotFound => unreachable!("handled above"),
    };

    match detection {
        Detection::State(mut state) => {
            let candidate = ReportedStatus {
                agent: label,
                state,
                pid: agent_pid,
                session_id: tracked.agent_session_id.clone(),
                model: tracked.agent_model.clone(),
            };
            let same_agent = |previous: &ReportedStatus| {
                previous.agent == candidate.agent
                    && previous.pid == candidate.pid
                    && previous.session_id == candidate.session_id
            };
            let previous = tracked
                .reported
                .as_ref()
                .filter(|status| same_agent(status));
            if state == AgentState::Idle
                && !previous.is_some_and(|status| status.state == AgentState::Idle)
            {
                let since = pending_idle
                    .filter(|(_, status)| same_agent(status))
                    .map_or(now, |(since, _)| since);
                if now.duration_since(since) < idle_delay {
                    tracked.pending_idle = Some((since, candidate));
                    // Keep attribution/model updates flowing while withholding
                    // idle. A replacement agent must not inherit the old state.
                    state = previous.map_or(AgentState::Unknown, |status| status.state);
                }
            }
            publish(
                id,
                tracked,
                state,
                PublishMetadata {
                    revision: Some(screen.revision),
                    child_pid: process.child_pid,
                    agent_pid,
                    agent: label,
                    hub,
                },
            );
        }
        Detection::KeepPrevious => {}
    }
}

/// Read bytes appended to the pane's attributed session file since the last
/// poll, publishing any newer model named there. Returns whether the file grew.
fn advance_model_scan(tracked: &mut TrackedPane, snapshot: &ProcessSnapshot) -> bool {
    let Some(scan) = tracked.model_scan.as_mut() else {
        return false;
    };
    let before = scan.offset();
    if let Some(model) = scan.advance(snapshot.source) {
        tracked.agent_model = Some(model);
    }
    scan.offset() != before
}

/// Decide whether a cwd-attributed pane should adopt `candidate` — the newest
/// recognized transcript in its session directory — as its own live session.
///
/// The session directory is keyed by working directory, so every pane running
/// the same agent in the same directory resolves the same "newest" file, and
/// only one of them can be its writer. mtime order alone cannot say which, so a
/// pane with an attributed transcript adopts a different one only on correlated
/// evidence that its own attribution is dead: its transcript has gone silent for
/// [`TRANSCRIPT_SILENT_POLLS`], and from then on the pane's agent is working
/// while the candidate grows over [`TRANSCRIPT_ADOPTION_POLLS`]. Any growth of
/// the attributed transcript re-confirms it and discards the gathered evidence.
///
/// A transcript named outright by the agent's own environment stamp is exact,
/// and no amount of correlated evidence outweighs it: the guess never displaces
/// it. The stamp lives on tool subprocesses, so it is republished whenever the
/// pane runs a tool, and a genuine in-pane session switch is picked up there.
///
/// The remainder is deliberately biased toward keeping the current attribution:
/// a pane mid-turn whose transcript pauses (a long tool call) while a
/// same-directory sibling streams would otherwise accumulate spurious evidence.
/// Adopting wrongly is self-healing — the pane's real transcript becomes the
/// newest candidate as soon as it grows again — but it is wrong meanwhile, and
/// flipping on bare mtime was wrong immediately and stayed wrong.
fn should_adopt_transcript(
    tracked: &mut TrackedPane,
    candidate: &Path,
    session_file_grew: bool,
    snapshot: &ProcessSnapshot,
) -> bool {
    let (attributed, is_current) = match tracked.model_scan.as_ref().map(ModelScan::path) {
        Some(current) => (true, current == candidate),
        None => (false, false),
    };
    // Nothing attributed yet: first attribution stays best-effort newest.
    if !attributed {
        return true;
    }
    // The newest transcript is already ours; nothing to correlate.
    if is_current {
        tracked.session_candidate = None;
        return true;
    }
    // Our session was named by the agent itself; a directory guess cannot
    // overrule it.
    if tracked.session_stamped {
        tracked.session_candidate = None;
        return false;
    }
    // Our transcript is still being written: whatever else appeared in the
    // shared directory belongs to another pane.
    if session_file_grew {
        tracked.session_candidate = None;
        return false;
    }
    let len = snapshot.source.file_len(candidate);
    let silent = tracked.silent_polls >= TRANSCRIPT_SILENT_POLLS;
    let working = matches!(
        tracked.reported.as_ref().map(|reported| reported.state),
        Some(AgentState::Working)
    );
    match tracked.session_candidate.as_mut() {
        Some(evidence) if evidence.path == *candidate => {
            if silent && working && len.is_some_and(|len| len > evidence.len) {
                evidence.correlated_polls += 1;
            }
            if let Some(len) = len {
                evidence.len = len;
            }
            evidence.correlated_polls >= TRANSCRIPT_ADOPTION_POLLS
        }
        // A new candidate (or none yet): start gathering evidence from its
        // current size, so only growth observed from here on counts.
        _ => {
            tracked.session_candidate = Some(SessionCandidate {
                path: candidate.to_path_buf(),
                len: len.unwrap_or(0),
                correlated_polls: 0,
            });
            false
        }
    }
}

/// Run every detector and return the first definite (non-`Unknown`)
/// classification, falling back to any `KeepPrevious`, then `Unknown`.
fn run_all(
    detectors: &[Box<dyn AgentDetector>],
    screen: &str,
    title: Option<&str>,
) -> (Detection, Option<&'static str>) {
    let mut keep_previous = None;
    for detector in detectors {
        match detector.detect(screen, title) {
            Detection::State(AgentState::Unknown) => {}
            state @ Detection::State(_) => return (state, Some(detector.label())),
            Detection::KeepPrevious => {
                keep_previous.get_or_insert((Detection::KeepPrevious, Some(detector.label())));
            }
        }
    }
    keep_previous.unwrap_or((Detection::State(AgentState::Unknown), None))
}

struct PublishMetadata<'a> {
    revision: Option<u64>,
    child_pid: Option<u32>,
    agent_pid: Option<u32>,
    agent: Option<&'static str>,
    hub: Option<&'a StatusHub>,
}

fn publish(
    id: PaneId,
    tracked: &mut TrackedPane,
    state: AgentState,
    metadata: PublishMetadata<'_>,
) {
    let PublishMetadata {
        revision,
        child_pid,
        agent_pid,
        agent,
        hub,
    } = metadata;
    let reported = ReportedStatus {
        agent,
        state,
        pid: agent_pid,
        session_id: tracked.agent_session_id.clone(),
        model: tracked.agent_model.clone(),
    };
    if tracked.reported.as_ref() == Some(&reported) {
        return;
    }
    let previous = tracked.reported.clone();
    tracked.reported = Some(reported);
    info!(
        target: "tiri::integration",
        pane_id = id.0,
        agent = ?agent,
        ?child_pid,
        ?agent_pid,
        ?revision,
        ?previous,
        ?state,
        "agent integration state changed"
    );
    // Mirror the transition into the shared hub for format renderers. Only real
    // transitions reach here, so this bumps the hub revision once per change.
    if let Some(hub) = hub {
        hub.publish(
            id,
            AgentStatus {
                agent: agent.unwrap_or(""),
                pid: agent_pid,
                session_id: tracked.agent_session_id.clone(),
                model: tracked.agent_model.clone(),
                state,
            },
        );
    }
}

/// Read-only access to the OS process table, used to attribute a pane's process
/// subtree to a known agent.
///
/// This is abstracted behind a trait so the attribution logic can be exercised
/// against a synthetic tree in tests, and so the descent does not depend on any
/// single OS interface. In particular the real implementation reconstructs the
/// tree from each process's parent pid rather than relying on a kernel-provided
/// children list.
pub(crate) trait ProcessSource {
    /// A snapshot of `(pid, ppid)` for every visible process, or `None` when the
    /// process table is unavailable on this platform.
    fn process_table(&self) -> Option<Vec<(u32, u32)>>;

    /// Candidate OS-visible program names for `pid`, most specific first.
    /// Empty when the process cannot be read.
    fn programs(&self, pid: u32) -> Vec<OsString>;

    /// Full process argument vector, including `argv[0]`. Empty when unreadable.
    fn arguments(&self, _pid: u32) -> Vec<OsString> {
        Vec::new()
    }

    /// The working directory of `pid`, when readable — used to locate an agent's
    /// per-project session state. `None` when unsupported or unreadable.
    fn cwd(&self, _pid: u32) -> Option<PathBuf> {
        None
    }

    /// The regular files in `dir`, most recently modified first. Empty when the
    /// directory is unreadable or the platform does not support inspection.
    fn files_newest_first(&self, _dir: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Paths currently open by `pid`. Empty when unsupported or unreadable.
    fn open_files(&self, _pid: u32) -> Vec<PathBuf> {
        Vec::new()
    }

    /// The current byte length of the file at `path`, used to watch a candidate
    /// session file for growth. `None` when unreadable or unsupported.
    fn file_len(&self, _path: &Path) -> Option<u64> {
        None
    }

    /// When the file at `path` was last modified. `None` when unreadable or
    /// unsupported.
    fn file_modified(&self, _path: &Path) -> Option<SystemTime> {
        None
    }

    /// When `pid` began running, used to reject session state that was already
    /// stale before the agent existed. `None` when unreadable or unsupported.
    fn start_time(&self, _pid: u32) -> Option<SystemTime> {
        None
    }

    /// `pid`'s environment as `(name, value)` pairs. Empty when unreadable or
    /// unsupported.
    fn environ(&self, _pid: u32) -> Vec<(OsString, OsString)> {
        Vec::new()
    }

    /// Up to `max_len` bytes of the file at `path` starting at byte `offset`,
    /// used to scan agent session files incrementally. An empty vector means
    /// end of file; `None` means the file is unreadable or the platform does
    /// not support inspection.
    fn read_span(&self, path: &Path, offset: u64, max_len: usize) -> Option<Vec<u8>>;
}

/// The production [`ProcessSource`], backed by the host OS process table.
pub(crate) struct SystemProcesses;

impl ProcessSource for SystemProcesses {
    fn process_table(&self) -> Option<Vec<(u32, u32)>> {
        CurrentPlatform::process_table().map(|table| {
            table
                .into_iter()
                .map(|process| (process.pid, process.ppid))
                .collect()
        })
    }

    fn programs(&self, pid: u32) -> Vec<OsString> {
        CurrentPlatform::process_programs(pid)
    }

    fn arguments(&self, pid: u32) -> Vec<OsString> {
        CurrentPlatform::process_arguments(pid)
    }

    fn cwd(&self, pid: u32) -> Option<PathBuf> {
        CurrentPlatform::process_cwd(pid)
    }

    fn files_newest_first(&self, dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut files = entries
            .flatten()
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
            .collect::<Vec<_>>();
        files.sort_unstable_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        files.into_iter().map(|(_, path)| path).collect()
    }

    fn open_files(&self, pid: u32) -> Vec<PathBuf> {
        CurrentPlatform::process_open_files(pid)
    }

    fn file_len(&self, path: &Path) -> Option<u64> {
        std::fs::metadata(path).ok().map(|metadata| metadata.len())
    }

    fn file_modified(&self, path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }

    fn start_time(&self, pid: u32) -> Option<SystemTime> {
        CurrentPlatform::process_start_time(pid)
    }

    fn environ(&self, pid: u32) -> Vec<(OsString, OsString)> {
        CurrentPlatform::process_environ(pid)
    }

    fn read_span(&self, path: &Path, offset: u64, max_len: usize) -> Option<Vec<u8>> {
        use std::io::{ErrorKind, Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(path).ok()?;
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buffer = vec![0; max_len];
        let mut filled = 0;
        while filled < buffer.len() {
            match file.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return None,
            }
        }
        buffer.truncate(filled);
        Some(buffer)
    }
}

/// A single poll tick's view of the process table, captured once by the observer
/// poller and shared read-only across every pane inspected that tick.
///
/// Building the parent→child index requires reading every visible process,
/// which dominates attribution cost. Capturing it once per poll — rather than
/// re-scanning inside each pane's tree walk — is what keeps a busy server (many
/// agents all streaming output) scanning the table once per poll cadence instead
/// of once per pane per poll. The poller is the sole writer: it constructs the
/// snapshot and hands out only shared references, so readers cannot mutate the
/// cached table and cannot trigger a fresh scan on output churn.
pub(crate) struct ProcessSnapshot<'a> {
    source: &'a dyn ProcessSource,
    /// `ppid -> child pids`, reconstructed from the parent-pid edges, or `None`
    /// when the process table was unavailable this tick.
    ///
    /// Built on first use rather than at capture: reading every visible process
    /// dominates the poll, and a tick where no pane needs attribution — no
    /// panes, or every pane's child already gone — should not pay for it.
    children: OnceCell<Option<HashMap<u32, Vec<u32>>>>,
}

impl<'a> ProcessSnapshot<'a> {
    /// Take the process table for one poll; every pane inspected this tick
    /// reads from the same index.
    pub(crate) fn capture(source: &'a dyn ProcessSource) -> Self {
        Self {
            source,
            children: OnceCell::new(),
        }
    }

    fn children(&self) -> Option<&HashMap<u32, Vec<u32>>> {
        self.children
            .get_or_init(|| {
                self.source.process_table().map(|table| {
                    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
                    for (pid, ppid) in table {
                        children.entry(ppid).or_default().push(pid);
                    }
                    children
                })
            })
            .as_ref()
    }

    /// Whether the process table is available to this snapshot.
    fn has_process_table(&self) -> bool {
        self.children().is_some()
    }

    /// The direct children of `pid` in the captured table.
    fn children_of(&self, pid: u32) -> &[u32] {
        self.children()
            .and_then(|children| children.get(&pid))
            .map_or(&[][..], Vec::as_slice)
    }
}

/// Find a detector-recognized session file on an agent process or descendant,
/// returning the session id together with the file that named it. Launchers
/// named `codex` commonly keep the actual Codex runtime as a child, and that
/// runtime owns the open rollout descriptor.
fn find_open_file_session_in_tree(
    snapshot: &ProcessSnapshot,
    root: u32,
    detector: &dyn AgentDetector,
) -> Option<(CString, PathBuf)> {
    let mut pending = vec![root];
    let mut visited = HashSet::new();
    while let Some(pid) = pending.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if let Some(found) = snapshot.source.open_files(pid).iter().find_map(|path| {
            detector
                .session_id_from_open_file(path)
                .map(|session_id| (session_id, path.clone()))
        }) {
            return Some(found);
        }
        pending.extend(snapshot.children_of(pid).iter().copied());
    }
    None
}

/// Resolve a session id from the agent's working directory: the detector maps
/// its cwd to a per-project session directory, and the newest file whose name
/// the detector recognizes names the live session. Returns the id together with
/// the session file. Reading the cwd once from the matched process suffices — an
/// agent and any wrapper below it share the working directory they were
/// launched in.
///
/// The session directory is keyed by working directory alone, so every agent
/// sharing a project contributes to it. A file last written before this agent
/// started cannot be the one this agent is writing, so such files are skipped
/// rather than treated as candidates: an agent that has not yet created its own
/// session file would otherwise adopt a neighbour's, which reports the
/// neighbour's model and session id for the life of the process. A resumed
/// session is not lost by this — the transcript re-dates itself as soon as the
/// resumed agent appends to it.
fn find_cwd_transcript_session(
    snapshot: &ProcessSnapshot,
    pid: u32,
    detector: &dyn AgentDetector,
) -> Option<(CString, PathBuf)> {
    let cwd = snapshot.source.cwd(pid)?;
    let dir = detector.session_dir_for_cwd(&cwd)?;
    // Without a readable start time nothing can be dated, so every file stays a
    // candidate and attribution remains best-effort newest.
    let started = snapshot.source.start_time(pid);
    snapshot
        .source
        .files_newest_first(&dir)
        .iter()
        .filter(|path| written_since(snapshot, path, started))
        .find_map(|path| {
            path.file_name()
                .and_then(|name| detector.session_id_from_file_name(name))
                .map(|session_id| (session_id, path.clone()))
        })
}

/// Publish `session_id` as the pane's session and point the model scan at its
/// file. A new file restarts the scan from its beginning; the model belongs to
/// the session, so the value read from the previous file is dropped with it.
///
/// `stamped` records how the session was resolved: exactly, from the agent's own
/// environment stamp, or by guessing at its session directory. Only the former
/// is protected from later guesses (see [`should_adopt_transcript`]).
fn adopt_session(
    tracked: &mut TrackedPane,
    snapshot: &ProcessSnapshot,
    session_id: CString,
    session_file: PathBuf,
    stamped: bool,
) {
    tracked.agent_session_id = Some(session_id);
    tracked.session_stamped = stamped;
    tracked.session_candidate = None;
    if tracked.model_scan.as_ref().map(ModelScan::path) != Some(&*session_file) {
        tracked.model_scan = Some(ModelScan::new(session_file));
        tracked.agent_model = None;
        tracked.silent_polls = 0;
        advance_model_scan(tracked, snapshot);
    }
}

/// Resolve a session id from the environment an agent stamps onto processes it
/// spawns. This is exact where the cwd heuristic can only guess: the stamp names
/// the session directly, so no correlation over file growth is needed.
///
/// Only *descendants* are inspected, never the attributed process itself. The
/// environment is inherited, so an agent launched by another agent carries its
/// parent's session id — reading the agent's own environment would attribute the
/// launching session. The owner-pid variable is checked for the same reason:
/// only a stamp naming this agent identifies a process it spawned itself.
///
/// The stamp lives on tool subprocesses, which exist only while the agent is
/// running one, so this yields nothing for an idle agent and the caller falls
/// back to dated-transcript attribution.
fn find_descendant_env_session(
    snapshot: &ProcessSnapshot,
    agent_pid: u32,
    detector: &dyn AgentDetector,
) -> Option<(CString, PathBuf)> {
    let stamp = detector.session_env_stamp()?;
    let cwd = snapshot.source.cwd(agent_pid)?;
    let mut pending = snapshot.children_of(agent_pid).to_vec();
    let mut visited = HashSet::from([agent_pid]);
    let mut inspected = 0;
    while let Some(pid) = pending.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if inspected >= ENV_SCAN_LIMIT {
            break;
        }
        inspected += 1;
        let environ = snapshot.source.environ(pid);
        let value = |name: &str| {
            environ
                .iter()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.to_str())
        };
        let owned_by_agent =
            value(stamp.owner_pid).and_then(|owner| owner.parse::<u32>().ok()) == Some(agent_pid);
        if owned_by_agent
            && let Some(session_id) = value(stamp.session_id)
            && let Some(file) = detector.session_file_for_id(&cwd, session_id)
        {
            return CString::new(session_id.to_ascii_lowercase())
                .ok()
                .map(|session_id| (session_id, file));
        }
        pending.extend(snapshot.children_of(pid).iter().copied());
    }
    None
}

/// Whether `path` was last modified at or after `started`. An unreadable
/// timestamp on either side keeps the file eligible, so a platform that cannot
/// date processes or files behaves exactly as it did before dating existed.
fn written_since(snapshot: &ProcessSnapshot, path: &Path, started: Option<SystemTime>) -> bool {
    let (Some(started), Some(modified)) = (started, snapshot.source.file_modified(path)) else {
        return true;
    };
    modified >= started
}

/// Return the registry index of the first known agent found at `root` or one of
/// its descendants, or the appropriate [`TreeScan`] outcome when none is.
///
/// The subtree is reconstructed from the parent-pid edges captured in `snapshot`,
/// so descent works regardless of whether the kernel exposes a per-process
/// children list — and, because the table was scanned once by the poller, this
/// walk does no full process-table scan of its own. Per-pid `programs` and
/// `arguments` reads are bounded to the pids on the pane's own subtree path.
fn find_agent_in_tree(
    snapshot: &ProcessSnapshot,
    root: u32,
    detectors: &[Box<dyn AgentDetector>],
) -> TreeScan {
    if !snapshot.has_process_table() {
        return TreeScan::NoProcessTable;
    }

    let mut pending = vec![root];
    let mut visited = HashSet::new();
    while let Some(pid) = pending.pop() {
        if !visited.insert(pid) {
            continue;
        }
        for program in snapshot.source.programs(pid) {
            if let Some(index) = program_matches_any(&program, detectors) {
                let arguments = snapshot.source.arguments(pid);
                return TreeScan::Found {
                    detector: index,
                    pid,
                    invocation_state: detectors[index].invocation_state(&arguments),
                };
            }
        }
        let arguments = snapshot.source.arguments(pid);
        if let Some(index) = invocation_matches_any(&arguments, detectors) {
            return TreeScan::Found {
                detector: index,
                pid,
                invocation_state: detectors[index].invocation_state(&arguments),
            };
        }
        pending.extend(snapshot.children_of(pid).iter().copied());
    }
    TreeScan::NotFound
}

fn program_matches_any(program: &OsStr, detectors: &[Box<dyn AgentDetector>]) -> Option<usize> {
    detectors.iter().position(|d| d.matches_program(program))
}

fn invocation_matches_any(
    arguments: &[OsString],
    detectors: &[Box<dyn AgentDetector>],
) -> Option<usize> {
    detectors
        .iter()
        .position(|d| d.matches_invocation(arguments))
}
