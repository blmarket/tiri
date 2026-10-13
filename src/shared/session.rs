//! Session indexes and groups; Session state belongs to its owner module.

pub use crate::src::session::{session, sessions};

/// Retained Session identity. Logical destruction and deferred releases remain explicit.
pub type SessionRef = std::rc::Rc<std::cell::UnsafeCell<session>>;
/// Nonowning Session identity for callbacks, targets and membership.
pub type SessionWeak = std::rc::Weak<std::cell::UnsafeCell<session>>;
/// Nonowning group handle; the group index holds the only owner. An empty
/// handle stands for "no group".
pub type SessionGroupWeak = refbox::Weak<session_group>;

#[repr(C)]
pub struct session_group {
    pub name: std::ffi::CString,
    /// Membership observes globally owned sessions without retaining them.
    pub(crate) members: Vec<SessionWeak>,
}

#[repr(C)]
pub struct session_groups {
    pub(crate) storage: std::collections::BTreeMap<Vec<u8>, refbox::RefBox<session_group>>,
}
