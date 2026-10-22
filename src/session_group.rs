//! Group indexes and membership are independently owned. Session state is
//! accessed through its trait; no Session model storage belongs to this module.
use crate::events::events_fire;
use crate::events_payload::{
    event_payload_create, event_payload_set_session, event_payload_set_string,
    event_payload_set_target, event_payload_set_uint,
};
use crate::format::bytes::write_cstr;
use crate::session::Session;
use crate::shared::abi::u_int;
use crate::shared::command::cmd_find_state;
use crate::shared::session::{
    session_group, session_groups, SessionGroupWeak, SessionRef, SessionWeak,
};
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
    pub fn new(name: &std::ffi::CStr) -> refbox::RefBox<Self> {
        refbox::RefBox::new(session_group {
            name: name.to_owned(),
            members: Vec::new(),
        })
    }
}

/// Group access is short and never calls out, so a borrow cannot be active
/// when another one starts.
fn with_group<R>(
    group: &refbox::RefBox<session_group>,
    op: impl FnOnce(&mut session_group) -> R,
) -> R {
    group
        .try_access_mut(op)
        .expect("session group already borrowed")
}

/// The group index is thread-local; each call borrows it only for its own
/// duration. Returned handles stop being alive once `remove` drops the owner.
impl session_groups {
    pub fn find(name: &CStr) -> SessionGroupWeak {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .get(name)
                .map_or_else(SessionGroupWeak::new, refbox::RefBox::downgrade)
        })
    }
    /// Returns the group with this name, creating it when absent.
    pub fn find_or_create(name: &CStr) -> SessionGroupWeak {
        SESSION_GROUPS.with_borrow_mut(|groups| {
            groups
                .storage
                .entry(name.to_owned())
                .or_insert_with(|| session_group::new(name))
                .downgrade()
        })
    }
    /// Removes and drops the indexed owner. A handle outside the index is not
    /// removed.
    pub fn remove(elm: &SessionGroupWeak) -> bool {
        let removed = SESSION_GROUPS.with_borrow_mut(|groups| {
            let key = groups
                .storage
                .iter()
                .find(|(_, owner)| elm.is(owner))
                .map(|(key, _)| key.clone())?;
            groups.storage.remove(&key)
        });
        removed.is_some()
    }
    pub fn minmax() -> SessionGroupWeak {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .values()
                .next()
                .map_or_else(SessionGroupWeak::new, refbox::RefBox::downgrade)
        })
    }
    pub fn next(elm: &SessionGroupWeak) -> SessionGroupWeak {
        let name = session_group_name(elm);
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .range::<CStr, _>((
                    std::ops::Bound::Excluded(name.as_c_str()),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map_or_else(SessionGroupWeak::new, |(_, owner)| owner.downgrade())
        })
    }
    /// Membership compares nonowning identities without projecting Session storage.
    pub fn for_session(target: &SessionWeak) -> SessionGroupWeak {
        SESSION_GROUPS.with_borrow(|groups| {
            groups
                .storage
                .values()
                .find(|owner| {
                    with_group(owner, |group| {
                        group.members.iter().any(|member| member.ptr_eq(target))
                    })
                })
                .map_or_else(SessionGroupWeak::new, refbox::RefBox::downgrade)
        })
    }
}

/// Accesses a live group through its handle; see `with_group`.
fn access<R>(group: &SessionGroupWeak, op: impl FnOnce(&mut session_group) -> R) -> R {
    group
        .try_access_mut(op)
        .expect("session group is not alive or already borrowed")
}

pub fn session_group_name(group: &SessionGroupWeak) -> std::ffi::CString {
    access(group, |group| group.name.clone())
}

fn session_group_fire(name: &CStr, group: &SessionGroupWeak, owner: &SessionRef) {
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
    let group_name = session_group_name(group);
    event_payload_set_string(&mut payload, c"group", |out| {
        write_cstr(out, group_name.as_c_str())
    });
    event_payload_set_uint(&mut payload, c"group_size", session_group_count(group));
    events_fire(name, payload);
}

pub fn session_group_add(group: &SessionGroupWeak, owner: &SessionRef) {
    if !session_groups::for_session(&Rc::downgrade(owner)).is_alive() {
        access(group, |group| group.members.push(Rc::downgrade(owner)));
        session_group_fire(c"session-added-to-group", group, owner);
    }
}

pub(crate) fn session_group_remove(owner: &SessionRef) {
    let group = session_groups::for_session(&Rc::downgrade(owner));
    if !group.is_alive() {
        return;
    }
    session_group_fire(c"session-removed-from-group", &group, owner);
    let now_empty = access(&group, |group| {
        let members = &mut group.members;
        let index = members
            .iter()
            .position(|member| member.ptr_eq(&Rc::downgrade(owner)))
            .expect("session group membership disappeared");
        members.remove(index);
        members.is_empty()
    });
    if now_empty {
        assert!(session_groups::remove(&group));
    }
}

/// Retain live members in insertion order for the entire caller operation.
/// Callbacks may remove membership or destroy the group while this snapshot exists.
pub fn session_group_members(group: &SessionGroupWeak) -> Vec<SessionRef> {
    if !group.is_alive() {
        return Vec::new();
    }
    access(group, |group| {
        group
            .members
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .collect()
    })
}

pub fn session_group_count(group: &SessionGroupWeak) -> u_int {
    access(group, |group| {
        u_int::try_from(
            group
                .members
                .iter()
                .filter(|member| member.strong_count() != 0)
                .count(),
        )
        .expect("session group has too many members")
    })
}

pub fn session_group_attached_count(group: &SessionGroupWeak) -> u_int {
    session_group_members(group)
        .iter()
        .fold(0, |count, member| {
            count.wrapping_add(member.attached_count())
        })
}

pub fn session_group_synchronize_to(owner: &SessionRef) {
    let group = session_groups::for_session(&Rc::downgrade(owner));
    if !group.is_alive() {
        return;
    }
    let source = session_group_members(&group)
        .into_iter()
        .find(|member| !Rc::ptr_eq(member, owner));
    if let Some(source) = source {
        owner.synchronize_windows_from(&source);
    }
}

pub fn session_group_synchronize_from(source: &SessionRef) {
    let group = session_groups::for_session(&Rc::downgrade(source));
    if !group.is_alive() {
        return;
    }
    for owner in session_group_members(&group) {
        if !Rc::ptr_eq(&owner, source) {
            owner.synchronize_windows_from(source);
        }
    }
}
