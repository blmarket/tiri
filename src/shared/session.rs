//! Session indexes and groups; Session state belongs to its owner module.

pub use crate::src::session::{session, sessions};

/// Retained Session identity. Logical destruction and deferred releases remain explicit.
pub type SessionRef = std::rc::Rc<std::cell::UnsafeCell<session>>;
/// Nonowning Session identity for callbacks, targets and membership.
pub type SessionWeak = std::rc::Weak<std::cell::UnsafeCell<session>>;

#[repr(C)]
pub struct session_group {
    pub name: std::ffi::CString,
    /// Membership observes globally owned sessions without retaining them.
    pub(crate) members: Vec<SessionWeak>,
}

impl session_group {
    pub fn empty() -> Self {
        Self {
            name: Default::default(),
            members: Default::default(),
        }
    }
}

#[repr(C)]
pub struct session_groups {
    pub storage: std::collections::BTreeMap<Vec<u8>, Box<session_group>>,
}
