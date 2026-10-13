//! Session state is private to its implementation. Other models use Session.
use crate::src::shared::abi::u_int;
use crate::src::shared::environment::environ;
use crate::src::shared::event::Timer;
use crate::src::shared::options::options;
use crate::src::shared::session::SessionRef;
use crate::src::shared::terminal::termios;
use crate::src::shared::window::{winlink, winlink_stack, winlinks};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
#[repr(C)]
pub struct sessions {
    pub(super) storage: std::collections::BTreeMap<Vec<u8>, SessionRef>,
}

#[repr(C)]
pub struct session {
    pub(super) id: u_int,
    pub(super) name: std::ffi::CString,
    pub(super) cwd: Option<std::ffi::CString>,
    pub(super) creation_time: SystemTime,
    pub(super) last_attached_time: SystemTime,
    pub(super) activity_time: SystemTime,
    pub(super) last_activity_time: SystemTime,
    pub(super) lock_timer: Option<Timer>,
    pub(super) curw: refbox::Weak<winlink>,
    pub(super) lastw: winlink_stack,
    pub(super) windows: winlinks,
    pub(super) statusat: ::core::ffi::c_int,
    pub(super) statuslines: u_int,
    pub(super) options: Option<Box<options>>,
    pub(super) flags: ::core::ffi::c_int,
    pub(super) attached: u_int,
    pub(super) tio: Option<Box<termios>>,
    pub(super) environ: Option<Box<environ>>,
}

impl session {
    /// Observe the current session-index winlink.
    pub(super) fn current_winlink(&self) -> refbox::Weak<winlink> {
        self.curw.clone()
    }

    /// The supplied winlink must be live in a session index.
    pub(super) fn set_curw(&mut self, wl: refbox::Weak<winlink>) {
        self.curw = wl;
    }

    pub(super) fn new() -> SessionRef {
        std::rc::Rc::new(std::cell::UnsafeCell::new(Self::empty()))
    }

    pub(super) fn empty() -> Self {
        Self {
            id: Default::default(),
            name: Default::default(),
            cwd: Default::default(),
            creation_time: UNIX_EPOCH,
            last_attached_time: UNIX_EPOCH,
            activity_time: UNIX_EPOCH,
            last_activity_time: UNIX_EPOCH,
            lock_timer: Default::default(),
            curw: Default::default(),
            lastw: Default::default(),
            windows: Default::default(),
            statusat: Default::default(),
            statuslines: Default::default(),
            options: Default::default(),
            flags: Default::default(),
            attached: Default::default(),
            tio: Default::default(),
            environ: Default::default(),
        }
    }
}

#[cfg(test)]
mod retained_session_tests {
    use super::*;
    use crate::src::shared::session::session_group;

    #[test]
    fn group_membership_is_weak_but_traversal_retains_live_sessions() {
        use crate::src::session::{
            session_group_attached_count, session_group_count, session_group_members,
        };
        use std::rc::Rc;

        unsafe {
            let first = session::new();
            let expired = session::new();
            let last = session::new();
            (*first.get()).attached = 2;
            (*last.get()).attached = 3;
            let first_weak = Rc::downgrade(&first);
            let last_weak = Rc::downgrade(&last);
            let mut group = session_group::empty();
            group.members = vec![
                first_weak.clone(),
                Rc::downgrade(&expired),
                last_weak.clone(),
            ];
            assert_eq!(Rc::strong_count(&first), 1);
            drop(expired);
            assert_eq!(session_group_count(&mut group), 2);
            assert_eq!(session_group_attached_count(&mut group), 5);
            let snapshot = session_group_members(&mut group);
            assert_eq!(snapshot.len(), 2);
            assert!(Rc::ptr_eq(&snapshot[0], &first));
            assert!(Rc::ptr_eq(&snapshot[1], &last));
            drop(first);
            drop(last);
            drop(group);
            assert!(first_weak.upgrade().is_some());
            assert!(last_weak.upgrade().is_some());
            drop(snapshot);
            assert!(first_weak.upgrade().is_none());
            assert!(last_weak.upgrade().is_none());
        }
    }

    #[test]
    fn removing_typed_reference_preserves_other_owners() {
        use std::rc::Rc;
        unsafe {
            let owner = session::new();
            let observer = Rc::downgrade(&owner);
            crate::src::session::session_remove_ref(owner.clone(), c"typed-owner-test");
            assert_eq!(Rc::strong_count(&owner), 1);
            drop(owner);
            assert!(observer.upgrade().is_none());
        }
    }

    #[test]
    fn final_reference_release_cleans_up_synchronously() {
        use std::rc::Rc;
        unsafe {
            let owner = session::new();
            let observer = Rc::downgrade(&owner);
            crate::src::session::session_remove_ref(owner, c"owner-test");
            assert!(observer.upgrade().is_none());
        }
    }
}
