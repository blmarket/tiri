//! Group indexes and membership are independently owned. Session state is
//! accessed through its trait; no Session model storage belongs to this module.
use crate::src::events::events_fire;
use crate::src::events_payload::{
    event_payload_create, event_payload_set_session, event_payload_set_string,
    event_payload_set_target, event_payload_set_uint,
};
use crate::src::format::bytes::write_cstr;
use crate::src::session::Session;
use crate::src::shared::abi::u_int;
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::session::{session_group, session_groups, SessionRef, SessionWeak};
use std::ffi::CStr;
use std::rc::Rc;

thread_local! {
    static SESSION_GROUPS: std::cell::RefCell<session_groups> = const {
        std::cell::RefCell::new(session_groups {
            storage: std::collections::BTreeMap::new(),
        })
    };
}

impl session_group {
    pub fn new(name: &std::ffi::CStr) -> Box<Self> {
        Box::new(session_group {
            name: name.to_owned(),
            members: Vec::new(),
        })
    }

    pub fn node_ptr(&self) -> *mut session_group {
        (self as *const Self).cast_mut()
    }
}

/// The group index is thread-local; each call borrows it only for its own
/// duration. Returned node pointers stay valid until `remove` drops the owner.
impl session_groups {
    pub fn is_empty() -> bool {
        SESSION_GROUPS.with_borrow(|groups| groups.storage.is_empty())
    }
    pub fn find(name: &CStr) -> *mut session_group {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .get(name.to_bytes())
                .map_or(std::ptr::null_mut(), |owner| owner.node_ptr())
        })
    }
    /// Returns the group with this name, creating it when absent.
    pub fn find_or_create(name: &CStr) -> *mut session_group {
        SESSION_GROUPS.with_borrow_mut(|groups| {
            groups
                .storage
                .entry(name.to_bytes().to_vec())
                .or_insert_with(|| session_group::new(name))
                .node_ptr()
        })
    }
    /// Removes and drops the indexed owner. A node outside the index is not
    /// adopted or removed.
    pub fn remove(elm: *mut session_group) -> bool {
        let removed = SESSION_GROUPS.with_borrow_mut(|groups| {
            let key = groups
                .storage
                .iter()
                .find(|(_, owner)| owner.node_ptr() == elm)
                .map(|(key, _)| key.clone())?;
            groups.storage.remove(&key)
        });
        // The borrow ended above, so dropping the group cannot re-enter the index.
        removed.is_some()
    }
    pub fn minmax() -> *mut session_group {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .values()
                .next()
                .map_or(std::ptr::null_mut(), |owner| owner.node_ptr())
        })
    }
    pub fn next(elm: &session_group) -> *mut session_group {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .range::<[u8], _>((
                    std::ops::Bound::Excluded(elm.name.as_bytes()),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map_or(std::ptr::null_mut(), |(_, owner)| owner.node_ptr())
        })
    }
    /// Membership compares nonowning identities without projecting Session storage.
    pub fn for_session(target: &SessionWeak) -> *mut session_group {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .values()
                .find(|group| group.members.iter().any(|member| member.ptr_eq(target)))
                .map_or(std::ptr::null_mut(), |group| group.node_ptr())
        })
    }
}

unsafe fn session_group_fire(name: &CStr, group: *mut session_group, owner: &SessionRef) {
    let mut target = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut payload = event_payload_create();
    if owner.is_registered() {
        target = cmd_find_state::find_from_session(owner, 0);
        event_payload_set_target(&mut payload, &target);
    }
    event_payload_set_session(&mut payload, c"session", owner.clone());
    event_payload_set_string(&mut payload, c"group", |out| {
        write_cstr(out, &*((*group).name))
    });
    event_payload_set_uint(&mut payload, c"group_size", session_group_count(group));
    events_fire(name, payload);
}

pub unsafe fn session_group_add(group: *mut session_group, owner: &SessionRef) {
    if session_groups::for_session(&Rc::downgrade(owner)).is_null() {
        (*group).members.push(Rc::downgrade(owner));
        session_group_fire(c"session-added-to-group", group, owner);
    }
}

pub(crate) unsafe fn session_group_remove(owner: &SessionRef) {
    let group = session_groups::for_session(&Rc::downgrade(owner));
    if group.is_null() {
        return;
    }
    // Notify while membership is still published, before removing this identity.
    session_group_fire(c"session-removed-from-group", group, owner);
    let members = &mut (*group).members;
    let index = members
        .iter()
        .position(|member| member.ptr_eq(&Rc::downgrade(owner)))
        .expect("session group membership disappeared");
    members.remove(index);
    if members.is_empty() {
        assert!(session_groups::remove(group));
    }
}

/// Retain live members in insertion order for the entire caller operation.
/// Callbacks may remove membership or destroy the group while this snapshot exists.
pub unsafe fn session_group_members(group: *mut session_group) -> Vec<SessionRef> {
    let Some(group) = group.as_ref() else {
        return Vec::new();
    };
    group
        .members
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .collect()
}

pub unsafe fn session_group_count(group: *mut session_group) -> u_int {
    u_int::try_from(
        (*group)
            .members
            .iter()
            .filter(|member| member.strong_count() != 0)
            .count(),
    )
    .expect("session group has too many members")
}

pub unsafe fn session_group_attached_count(group: *mut session_group) -> u_int {
    session_group_members(group)
        .iter()
        .fold(0, |count, member| {
            count.wrapping_add(member.attached_count())
        })
}

pub unsafe fn session_group_synchronize_to(owner: &SessionRef) {
    let group = session_groups::for_session(&Rc::downgrade(owner));
    if group.is_null() {
        return;
    }
    let source = session_group_members(group)
        .into_iter()
        .find(|member| !Rc::ptr_eq(member, owner));
    if let Some(source) = source {
        owner.synchronize_windows_from(&source);
    }
}

pub unsafe fn session_group_synchronize_from(source: &SessionRef) {
    let group = session_groups::for_session(&Rc::downgrade(source));
    if group.is_null() {
        return;
    }
    for owner in session_group_members(group) {
        if !Rc::ptr_eq(&owner, source) {
            owner.synchronize_windows_from(source);
        }
    }
}
