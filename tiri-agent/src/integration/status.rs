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

use std::cell::{Cell, RefCell, RefMut};
use std::collections::{BTreeMap, HashMap};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::rc::{Rc, Weak};

use crate::observability::v1::PaneId;
use crate::platform::{CurrentPlatform, OutputWakeup, Platform};

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
    /// Per-consumer pollable wakeups for attached status renderers.
    subscribers: RefCell<BTreeMap<u64, Weak<StatusEvent>>>,
    next_subscriber: Cell<u64>,
}

struct StatusEvent {
    wakeup: <CurrentPlatform as Platform>::OutputWakeup,
}

/// A coalescing, pollable notification for agent-status revision changes.
/// Each attached client owns one so draining cannot consume another client's
/// wakeup.
pub struct StatusSubscription {
    hub: Weak<Inner>,
    key: u64,
    event: Rc<StatusEvent>,
}

impl Drop for StatusSubscription {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            let mut subscribers = hub.subscribers.borrow_mut();
            if subscribers
                .get(&self.key)
                .is_some_and(|event| event.ptr_eq(&Rc::downgrade(&self.event)))
            {
                subscribers.remove(&self.key);
            }
        }
    }
}

impl StatusSubscription {
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.event.wakeup.as_fd()
    }

    pub fn as_raw_fd(&self) -> RawFd {
        self.as_fd().as_raw_fd()
    }

    pub fn drain(&self) {
        let _ = self.event.wakeup.clear();
    }
}

impl StatusHub {
    /// Build an empty hub at revision 1.
    pub fn new() -> Self {
        StatusHub(Rc::new(Inner {
            snapshot: RefCell::new(StatusSnapshot {
                revision: 1,
                panes: PaneAgents::new(),
            }),
            subscribers: RefCell::new(BTreeMap::new()),
            next_subscriber: Cell::new(0),
        }))
    }

    /// Upsert a pane's status. Bumps the revision and wakes waiters only when the
    /// stored value actually changes, so a heartbeat re-publish of an unchanged
    /// status doesn't churn `revision`.
    pub fn publish(&self, pane: PaneId, status: AgentStatus) {
        let mut snap = self.lock();
        if snap.panes.get(&pane) == Some(&status) {
            return;
        }
        snap.panes.insert(pane, status);
        snap.revision += 1;
        drop(snap);
        self.notify_changed();
    }

    /// Drop a pane that no longer exists. Bumps the revision and wakes waiters
    /// only when a pane was actually present.
    pub fn remove(&self, pane: PaneId) {
        let mut snap = self.lock();
        if snap.panes.remove(&pane).is_none() {
            return;
        }
        snap.revision += 1;
        drop(snap);
        self.notify_changed();
    }

    /// Clone the current snapshot under the lock.
    pub fn snapshot(&self) -> StatusSnapshot {
        self.lock().clone()
    }

    /// Subscribe to revision changes with a pollable, initially signalled
    /// descriptor. Initial readiness closes the race between registration and
    /// the subscriber's first snapshot.
    pub fn subscribe(&self) -> io::Result<StatusSubscription> {
        let event = Rc::new(StatusEvent {
            wakeup: CurrentPlatform::new_output_wakeup()?,
        });
        let key = self.0.next_subscriber.get();
        self.0.next_subscriber.set(
            key.checked_add(1)
                .expect("subscription identities exhausted"),
        );
        self.0
            .subscribers
            .borrow_mut()
            .insert(key, Rc::downgrade(&event));
        Ok(StatusSubscription {
            hub: Rc::downgrade(&self.0),
            key,
            event,
        })
    }

    fn lock(&self) -> RefMut<'_, StatusSnapshot> {
        self.0.snapshot.borrow_mut()
    }

    fn notify_changed(&self) {
        for subscriber in self.0.subscribers.borrow().values() {
            if let Some(event) = subscriber.upgrade() {
                let _ = event.wakeup.wake();
            }
        }
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
    use std::os::fd::RawFd;

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

    fn is_readable(fd: RawFd) -> bool {
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut pollfd, 1, 0) == 1 }
    }

    #[test]
    fn subscription_wakes_only_for_observable_changes() {
        let hub = StatusHub::new();
        let subscription = hub.subscribe().expect("subscribe");
        assert!(is_readable(subscription.as_raw_fd()));
        subscription.drain();
        assert!(!is_readable(subscription.as_raw_fd()));

        hub.publish(PaneId(1), status("codex", AgentState::Working));
        assert!(is_readable(subscription.as_raw_fd()));
        subscription.drain();

        hub.publish(PaneId(1), status("codex", AgentState::Working));
        assert!(!is_readable(subscription.as_raw_fd()));

        hub.publish(PaneId(1), status("codex", AgentState::Blocked));
        assert!(is_readable(subscription.as_raw_fd()));
    }
    #[test]
    fn dropped_subscribers_do_not_accumulate_without_publication() {
        let hub = StatusHub::new();
        for _ in 0..1000 {
            let subscription = hub.subscribe().unwrap();
            assert_eq!(hub.0.subscribers.borrow().len(), 1);
            drop(subscription);
            assert!(hub.0.subscribers.borrow().is_empty());
        }
    }

    #[test]
    fn subscriptions_drain_independently_and_survive_hub_destruction() {
        let hub = StatusHub::new();
        let first = hub.subscribe().unwrap();
        let middle = hub.subscribe().unwrap();
        let last = hub.subscribe().unwrap();
        first.drain();
        last.drain();
        drop(middle);
        assert_eq!(hub.0.subscribers.borrow().len(), 2);
        hub.publish(PaneId(1), status("codex", AgentState::Working));
        hub.publish(PaneId(1), status("codex", AgentState::Idle));
        first.drain();
        assert!(!is_readable(first.as_raw_fd()));
        assert!(is_readable(last.as_raw_fd()));
        last.drain();
        assert!(!is_readable(last.as_raw_fd()));
        let weak = Rc::downgrade(&hub.0);
        drop(hub);
        assert!(weak.upgrade().is_none());
        drop(first);
        drop(last);
    }

    #[test]
    fn stale_subscription_cannot_remove_a_replacement_registration() {
        let hub = StatusHub::new();
        let old = hub.subscribe().unwrap();
        let new = hub.subscribe().unwrap();
        hub.0
            .subscribers
            .borrow_mut()
            .insert(old.key, Rc::downgrade(&new.event));
        drop(old);
        assert_eq!(hub.0.subscribers.borrow().len(), 2);
    }
}
