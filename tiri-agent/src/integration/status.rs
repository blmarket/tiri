//! Shared server-side registry of per-pane agent status.
//!
//! The [`AgentObserver`](super::AgentObserver) writes each pane's classified
//! [`AgentStatus`] here; readers — the format-variable layer (`#{pane_agent*}`)
//! and attached client renderers — read it back. A monotonic `revision` mirrors
//! the pane `output_revision` pattern and makes snapshot changes cheap to detect.
//!
//! This hub is a **sibling** of the native server's `ServerState`, not a part of
//! it, so neither `ServerState`'s shape nor the `observability::v1` traits change.
//! It is engine-agnostic state that the native runtime happens to own. Like the
//! rest of observability it lives on the server's single thread: the observer
//! writes and the renderers read between turns of the same loop.

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use crate::observability::v1::PaneId;

use super::AgentState;

/// Per-pane agent attribution: which agent (if any) runs in a pane and the
/// lifecycle state last classified for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentStatus {
    /// Detector label: `"codex"`, `"claude"`, `"pi"`, `"agy"`, `"opencode"`,
    /// or `""` when no agent is present.
    pub agent: &'static str,
    /// OS pid of the matched agent process, when process attribution found one.
    pub pid: Option<u32>,
    /// The agent's session id, resolved from its process metadata when the
    /// detector defines a source (for example a cwd-scoped Claude/Pi transcript
    /// or an active Codex rollout). `None` when unknown.
    pub session_id: Option<CString>,
    /// The model the agent most recently recorded in its session file (for
    /// example `claude-opus-5` or `gpt-5-codex`). `None` when the session file
    /// is unknown or names no model yet.
    pub model: Option<CString>,
    /// The pane's classified lifecycle state.
    pub state: AgentState,
}

/// A map of pane id → agent status, as carried in a [`StatusSnapshot`] and used
/// by the format layer to expand `#{pane_agent*}`.
pub type PaneAgents = HashMap<PaneId, AgentStatus>;

/// A point-in-time copy of the hub: the `revision` it represents plus every
/// pane's status at that revision.
#[derive(Clone, Debug)]
pub struct StatusSnapshot {
    /// Monotonic counter, bumped on every observable change.
    pub revision: u64,
    /// Per-pane agent status at `revision`.
    pub panes: PaneAgents,
}

/// The shared registry. Cloneable; every clone refers to the same underlying
/// state (observer and connection handlers all hold clones).
#[derive(Clone)]
pub struct StatusHub(Rc<Inner>);

struct Inner {
    snapshot: RefCell<StatusSnapshot>,
}

impl StatusHub {
    /// Build an empty hub at revision 1.
    pub fn new() -> Self {
        StatusHub(Rc::new(Inner {
            snapshot: RefCell::new(StatusSnapshot {
                revision: 1,
                panes: PaneAgents::new(),
            }),
        }))
    }

    /// Upsert a pane's status. Bumps the revision only when the stored value
    /// actually changes, so a heartbeat re-publish of an unchanged
    /// status doesn't churn `revision`.
    pub fn publish(&self, pane: PaneId, status: AgentStatus) {
        let mut snap = self.lock();
        if snap.panes.get(&pane) == Some(&status) {
            return;
        }
        snap.panes.insert(pane, status);
        snap.revision += 1;
    }

    /// Drop a pane that no longer exists. Bumps the revision only when a pane
    /// was actually present.
    pub fn remove(&self, pane: PaneId) {
        let mut snap = self.lock();
        if snap.panes.remove(&pane).is_none() {
            return;
        }
        snap.revision += 1;
    }

    /// Clone the current snapshot under the lock.
    pub fn snapshot(&self) -> StatusSnapshot {
        self.lock().clone()
    }

    fn lock(&self) -> RefMut<'_, StatusSnapshot> {
        self.0.snapshot.borrow_mut()
    }
}

impl Default for StatusHub {
    fn default() -> Self {
        StatusHub::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(agent: &'static str, state: AgentState) -> AgentStatus {
        AgentStatus {
            agent,
            pid: None,
            session_id: None,
            model: None,
            state,
        }
    }

    #[test]
    fn publish_and_remove_bump_revision() {
        let hub = StatusHub::new();
        assert_eq!(hub.snapshot().revision, 1);

        hub.publish(PaneId(0), status("claude", AgentState::Working));
        let snap = hub.snapshot();
        assert_eq!(snap.revision, 2);
        assert_eq!(
            snap.panes[&PaneId(0)],
            status("claude", AgentState::Working)
        );

        // Re-publishing the identical status is a no-op for the revision.
        hub.publish(PaneId(0), status("claude", AgentState::Working));
        assert_eq!(hub.snapshot().revision, 2);

        // A real change bumps it.
        hub.publish(PaneId(0), status("claude", AgentState::Idle));
        assert_eq!(hub.snapshot().revision, 3);

        hub.remove(PaneId(0));
        let snap = hub.snapshot();
        assert_eq!(snap.revision, 4);
        assert!(snap.panes.is_empty());

        // Removing an absent pane is a no-op.
        hub.remove(PaneId(0));
        assert_eq!(hub.snapshot().revision, 4);
    }
}
