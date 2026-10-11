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

pub static mut session_groups: session_groups = session_groups { storage: None };

pub fn session_groups_find(head: &session_groups, elm: &session_group) -> *mut session_group {
    let Some(owner) = head.storage.as_ref() else {
        return std::ptr::null_mut();
    };
    let map = owner
        .try_borrow_mut()
        .expect("session group index already borrowed");
    let key = elm.name.as_bytes();
    map.get(key)
        .map_or(std::ptr::null_mut(), |owner| owner.node_ptr())
}
impl session_group {
    pub fn new(name: &std::ffi::CStr) -> Box<Self> {
        Box::new(session_group {
            name: name.to_owned(),
            owner: refbox::Weak::new(),
            members: Vec::new(),
        })
    }

    pub fn node_ptr(&self) -> *mut session_group {
        (self as *const Self).cast_mut()
    }
}

/// Consumes the new owner. On a duplicate name, the old node is returned and
/// the incoming owner is dropped without entering the index.
pub unsafe fn session_groups_insert(
    head: *mut session_groups,
    owner: Box<session_group>,
) -> *mut session_group {
    let key = owner.name.to_bytes();
    let storage = (*head).storage.get_or_insert_with(refbox::RefBox::default);
    let observer = storage.downgrade();
    let mut map = storage
        .try_borrow_mut()
        .expect("session group index already borrowed");
    match map.entry(key.to_vec()) {
        std::collections::btree_map::Entry::Occupied(entry) => return entry.get().node_ptr(),
        std::collections::btree_map::Entry::Vacant(entry) => {
            let elm = owner.node_ptr();
            entry.insert(owner);
            (*elm).owner = observer;
        }
    }
    std::ptr::null_mut()
}
/// Removes and drops the indexed owner. A same-name node outside the index is
/// not adopted or removed.
pub unsafe fn session_groups_remove(head: *mut session_groups, elm: *mut session_group) -> bool {
    if elm.is_null() {
        return false;
    }
    let key = (*elm).name.as_bytes();
    let Some(storage) = (*head).storage.as_ref() else {
        return false;
    };
    // End the map borrow before dropping the group and its name.
    let (removed, empty) = {
        let mut map = storage
            .try_borrow_mut()
            .expect("session group index already borrowed");
        if map.get(key).map(|owner| owner.node_ptr()) != Some(elm) {
            return false;
        }
        (*elm).owner = refbox::Weak::new();
        (
            map.remove(key).expect("indexed session group disappeared"),
            map.is_empty(),
        )
    };
    if empty {
        (*head).storage = None;
    }
    drop(removed);
    true
}
pub fn session_groups_minmax(head: &session_groups) -> *mut session_group {
    let Some(storage) = head.storage.as_ref() else {
        return std::ptr::null_mut();
    };
    let map = storage
        .try_borrow_mut()
        .expect("session group index already borrowed");
    let pair = map.first_key_value();
    pair.map_or(std::ptr::null_mut(), |(_, owner)| owner.node_ptr())
}
pub unsafe fn session_groups_next(elm: &session_group) -> *mut session_group {
    let owner = &elm.owner;
    let map = match owner.try_borrow_mut() {
        Ok(map) => map,
        Err(refbox::BorrowError::Dropped) => return std::ptr::null_mut(),
        Err(refbox::BorrowError::Borrowed) => panic!("session group index already borrowed"),
    };
    let key = elm.name.as_bytes();
    map.range::<[u8], _>((std::ops::Bound::Excluded(key), std::ops::Bound::Unbounded))
        .next()
        .map_or(std::ptr::null_mut(), |(_, owner)| owner.node_ptr())
}
/// Membership compares nonowning identities without projecting Session storage.
pub unsafe fn session_group_for(target: &SessionWeak) -> *mut session_group {
    let mut group = session_groups_minmax(&session_groups);
    while let Some(value) = group.as_ref() {
        if value.members.iter().any(|member| member.ptr_eq(target)) {
            return group;
        }
        group = session_groups_next(value);
    }
    std::ptr::null_mut()
}

pub unsafe fn session_group_find(name: &CStr) -> *mut session_group {
    let key = session_group {
        name: name.to_owned(),
        ..session_group::empty()
    };
    session_groups_find(&session_groups, &key)
}

pub unsafe fn session_group_new(name: &CStr) -> *mut session_group {
    let existing = session_group_find(name);
    if !existing.is_null() {
        return existing;
    }
    let owner = session_group::new(name);
    let group = owner.node_ptr();
    assert!(session_groups_insert(&raw mut session_groups, owner).is_null());
    group
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
    if session_group_for(&Rc::downgrade(owner)).is_null() {
        (*group).members.push(Rc::downgrade(owner));
        session_group_fire(c"session-added-to-group", group, owner);
    }
}

pub(crate) unsafe fn session_group_remove(owner: &SessionRef) {
    let group = session_group_for(&Rc::downgrade(owner));
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
        assert!(session_groups_remove(&raw mut session_groups, group));
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
    let group = session_group_for(&Rc::downgrade(owner));
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
    let group = session_group_for(&Rc::downgrade(source));
    if group.is_null() {
        return;
    }
    for owner in session_group_members(group) {
        if !Rc::ptr_eq(&owner, source) {
            owner.synchronize_windows_from(source);
        }
    }
}
