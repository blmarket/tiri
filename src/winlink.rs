//! Session-owned links and their Window associations.
use crate::src::log::fatalx;
use crate::src::session::Session;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::WindowRef;
use crate::src::window::*;

use crate::src::server_fn::server_status_session;

use crate::src::shared::abi::*;
use crate::src::shared::limits::INT_MAX;
use crate::src::shared::tree::{RB_INF, RB_NEGINF};
pub use crate::src::shared::window::{winlink, winlink_stack, winlinks};
use crate::src::shared::window::{WINLINK_ALERTFLAGS, WINLINK_VISITED};

pub fn winlinks_minmax(head: &winlinks, direction: ::core::ffi::c_int) -> refbox::Weak<winlink> {
    let owner = head;
    let map = owner
        .try_borrow_mut()
        .expect("winlink index already borrowed");
    let pair = if direction < 0 {
        map.first_key_value()
    } else {
        map.last_key_value()
    };
    pair.map_or(refbox::Weak::new(), |(_, owner)| owner.downgrade())
}

pub unsafe fn winlinks_next(elm: &winlink) -> refbox::Weak<winlink> {
    let owner = &elm.owner;
    let map = match owner.try_borrow_mut() {
        Ok(map) => map,
        Err(refbox::BorrowError::Dropped) => return refbox::Weak::new(),
        Err(refbox::BorrowError::Borrowed) => panic!("winlink index already borrowed"),
    };
    let key = elm.idx;
    map.range((std::ops::Bound::Excluded(&key), std::ops::Bound::Unbounded))
        .next()
        .map_or(refbox::Weak::new(), |(_, owner)| owner.downgrade())
}

pub unsafe fn winlinks_prev(elm: &winlink) -> refbox::Weak<winlink> {
    let owner = &elm.owner;
    let map = match owner.try_borrow_mut() {
        Ok(map) => map,
        Err(refbox::BorrowError::Dropped) => return refbox::Weak::new(),
        Err(refbox::BorrowError::Borrowed) => panic!("winlink index already borrowed"),
    };
    let key = elm.idx;
    map.range((std::ops::Bound::Unbounded, std::ops::Bound::Excluded(&key)))
        .next_back()
        .map_or(refbox::Weak::new(), |(_, owner)| owner.downgrade())
}

pub unsafe fn winlink_find_by_window(wwl: &winlinks, w_owner: &WindowRef) -> refbox::Weak<winlink> {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = winlinks_minmax(wwl, RB_NEGINF);
    while wl.is_alive() {
        if wl
            .get_unchecked()
            .window_handle()
            .is_some_and(|window| WindowRef::ptr_eq(window, w_owner))
        {
            return wl;
        }
        wl = winlinks_next(wl.get_unchecked());
    }
    refbox::Weak::new()
}

pub unsafe fn winlink_find_by_index(
    wwl: &winlinks,
    idx: ::core::ffi::c_int,
) -> refbox::Weak<winlink> {
    if idx < 0 as ::core::ffi::c_int {
        fatalx(|out| out.write_all(b"bad index"));
    }
    let map = wwl
        .try_borrow_mut()
        .expect("winlink index already borrowed");
    map.get(&idx)
        .map_or(refbox::Weak::new(), |owner| owner.downgrade())
}

pub unsafe fn winlink_find_by_window_id(wwl: &winlinks, mut id: u_int) -> refbox::Weak<winlink> {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = winlinks_minmax(wwl, RB_NEGINF);
    while wl.is_alive() {
        if wl
            .get_unchecked()
            .window_handle()
            .expect("indexed window")
            .id()
            == id
        {
            return wl;
        }
        wl = winlinks_next(wl.get_unchecked());
    }
    refbox::Weak::new()
}

unsafe fn winlink_next_index(
    mut wwl: *mut winlinks,
    mut idx: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut i: ::core::ffi::c_int = 0;
    i = idx;
    loop {
        if !winlink_find_by_index(&*wwl, i).is_alive() {
            return i;
        }
        if i == INT_MAX {
            i = 0 as ::core::ffi::c_int;
        } else {
            i += 1;
        }
        if !(i != idx) {
            break;
        }
    }
    -(1 as ::core::ffi::c_int)
}

pub fn winlink_count(wwl: &winlinks) -> u_int {
    wwl.try_borrow_mut()
        .expect("winlink index already borrowed")
        .len() as u_int
}

pub fn winlinks_is_empty(wwl: &winlinks) -> bool {
    wwl.try_borrow_mut()
        .expect("winlink index already borrowed")
        .is_empty()
}

pub unsafe fn winlink_add(
    mut wwl: *mut winlinks,
    mut idx: ::core::ffi::c_int,
) -> refbox::Weak<winlink> {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if idx < 0 as ::core::ffi::c_int {
        // Decode -(base_index + 1) without negating INT_MIN first.
        idx = winlink_next_index(wwl, -(idx + 1));
        if idx == -(1 as ::core::ffi::c_int) {
            return refbox::Weak::new();
        }
    } else if winlink_find_by_index(&*wwl, idx).is_alive() {
        return refbox::Weak::new();
    }
    let owner = refbox::RefBox::new(winlink {
        idx,
        session: std::rc::Weak::new(),
        window_owner: None,
        flags: 0,
        owner: refbox::Weak::new(),
    });
    wl = owner.downgrade();
    let storage = &*wwl;
    let observer = storage.downgrade();
    let mut map = storage
        .try_borrow_mut()
        .expect("winlink index already borrowed");
    match map.entry(idx) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(owner);
        }
        std::collections::btree_map::Entry::Occupied(_) => {
            unreachable!("winlink index was checked above")
        }
    }
    wl.get_mut_unchecked().owner = observer;
    wl
}

// Drop the reference in place: the field stays published through the close
// notification, so its callbacks can still inspect the winlink.
unsafe fn winlink_release_window(mut wl: refbox::Weak<winlink>) {
    let field = &raw mut wl.get_mut_unchecked().window_owner;
    assert!((*field).is_some(), "winlink window reference");
    std::ptr::drop_in_place(field);
    std::ptr::write(field, None);
}

pub unsafe fn winlink_set_window(mut wl: refbox::Weak<winlink>, w_owner: &WindowRef) {
    if let Some(previous) = wl.get_unchecked().window_handle() {
        previous.remove_winlink(wl.clone());
        winlink_release_window(wl.clone());
    }
    wl.get_mut_unchecked().window_owner = Some(w_owner.clone());
    w_owner.add_winlink(wl);
}

pub unsafe fn winlink_remove(mut wwl: *mut winlinks, mut wl: refbox::Weak<winlink>) {
    if let Some(session_owner) = wl.get_unchecked().session.upgrade() {
        session_owner.forget_winlink(wl.clone());
    }
    if let Some(window) = wl.get_unchecked().window_handle() {
        window.remove_winlink(wl.clone());
        winlink_release_window(wl.clone());
    }
    // Window teardown above may reenter; borrow the owning map only afterward.
    let idx = wl.get_unchecked().idx;
    let owner = {
        let storage = &*wwl;
        let mut map = storage
            .try_borrow_mut()
            .expect("winlink index already borrowed");
        assert_eq!(
            map.get(&idx).map(|owner| owner.downgrade()),
            Some(wl.clone()),
            "removed winlink must belong to this index"
        );
        let owner = map.remove(&idx).expect("winlink must have an owner");
        wl.get_mut_unchecked().owner = refbox::Weak::new();
        owner
    };
    drop(owner);
}

pub unsafe fn winlink_next(mut wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    winlinks_next(wl.get_unchecked())
}

pub unsafe fn winlink_previous(mut wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    winlinks_prev(wl.get_unchecked())
}

pub unsafe fn winlink_next_by_number(
    mut wl: refbox::Weak<winlink>,
    s_owner: &SessionRef,
    mut n: ::core::ffi::c_int,
) -> refbox::Weak<winlink> {
    let s = Some(s_owner.clone());
    while n > 0 as ::core::ffi::c_int {
        wl = winlinks_next(wl.get_unchecked());
        if !wl.is_alive() {
            wl = s
                .as_ref()
                .expect("live session")
                .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        }
        n -= 1;
    }
    wl
}

pub unsafe fn winlink_previous_by_number(
    mut wl: refbox::Weak<winlink>,
    s_owner: &SessionRef,
    mut n: ::core::ffi::c_int,
) -> refbox::Weak<winlink> {
    let s = Some(s_owner.clone());
    while n > 0 as ::core::ffi::c_int {
        wl = winlinks_prev(wl.get_unchecked());
        if !wl.is_alive() {
            wl = s
                .as_ref()
                .expect("live session")
                .with_winlinks(|links| winlinks_minmax(links, RB_INF));
        }
        n -= 1;
    }
    wl
}

/// Borrow the owner through the existing window index. The raw pointer remains
/// a compatibility view; neither it nor its address is a separate ownership key.
unsafe fn winlink_weak(wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    let owner = &wl.get_unchecked().owner;
    let map = owner
        .try_borrow_mut()
        .expect("winlink index already borrowed");
    let owner = map
        .get(&wl.get_unchecked().idx)
        .expect("visited winlink must have an owner");
    assert_eq!(
        owner.downgrade(),
        wl.clone(),
        "visited winlink must belong to its index"
    );
    owner.downgrade()
}

/// Move the owner between keys without invalidating the winlink or its observers.
/// The caller must supply a live member of `head` and an unused destination index.
pub unsafe fn winlinks_reindex(head: *mut winlinks, wl: refbox::Weak<winlink>, idx: i32) {
    let owner = &*head;
    let mut map = owner
        .try_borrow_mut()
        .expect("winlink index already borrowed");
    let old_idx = wl.get_unchecked().idx;
    assert_eq!(
        map.get(&old_idx).map(|owner| owner.downgrade()),
        Some(wl.clone()),
        "reindexed winlink must belong to this index"
    );
    if old_idx == idx {
        return;
    }
    assert!(
        !map.contains_key(&idx),
        "destination winlink index must be vacant"
    );
    let owner = map.remove(&old_idx).expect("winlink must have an owner");
    owner
        .try_access_mut(|link| link.idx = idx)
        .expect("reindexed winlink is already borrowed");
    map.insert(idx, owner);
}

pub unsafe fn winlink_stack_push(stack: *mut winlink_stack, mut wl: refbox::Weak<winlink>) {
    if !wl.is_alive() {
        return;
    }
    winlink_stack_remove(stack, wl.clone());
    let weak = winlink_weak(wl.clone());
    (*stack).push_front(weak);
    wl.get_mut_unchecked().flags |= WINLINK_VISITED;
}

pub unsafe fn winlink_stack_remove(stack: *mut winlink_stack, mut wl: refbox::Weak<winlink>) {
    if !wl.is_alive() {
        return;
    }
    (*stack).retain(|link| *link != wl);
    wl.get_mut_unchecked().flags &= !WINLINK_VISITED;
}

/// Append while rebuilding a session's saved visit order.
pub unsafe fn winlink_stack_append(stack: &mut winlink_stack, mut wl: refbox::Weak<winlink>) {
    let weak = winlink_weak(wl.clone());
    stack.push_back(weak);
    wl.get_mut_unchecked().flags |= WINLINK_VISITED;
}

pub fn winlink_stack_clear(stack: &mut winlink_stack) {
    *stack = Default::default();
}

pub fn winlink_stack_indices(stack: &winlink_stack) -> Vec<::core::ffi::c_int> {
    let storage = &stack;
    storage
        .iter()
        .map(|link| match link.try_access_mut(|node| node.idx) {
            Ok(idx) => idx,
            Err(refbox::BorrowError::Dropped) => {
                panic!("visited winlink owner was dropped before observer teardown")
            }
            Err(refbox::BorrowError::Borrowed) => panic!("visited winlink is already borrowed"),
        })
        .collect()
}

pub fn winlink_stack_first(stack: &winlink_stack) -> refbox::Weak<winlink> {
    stack.front().cloned().unwrap_or_default()
}

pub fn winlink_stack_next(
    stack: &winlink_stack,
    wl: refbox::Weak<winlink>,
) -> refbox::Weak<winlink> {
    let queue = &stack;
    let Some(position) = queue.iter().position(|link| *link == wl) else {
        return refbox::Weak::new();
    };
    queue.get(position + 1).cloned().unwrap_or_default()
}

pub unsafe fn winlink_clear_flags(mut wl: refbox::Weak<winlink>) {
    let window = wl.get_unchecked().window_handle().expect("linked window");
    window.clear_alert_flags();
    let mut cursor = window.next_winlink(None);
    while cursor.is_alive() {
        if cursor.get_unchecked().flags & WINLINK_ALERTFLAGS != 0 {
            cursor.get_mut_unchecked().flags &= !WINLINK_ALERTFLAGS;
            if let Some(session) = cursor.get_unchecked().session.upgrade() {
                server_status_session(&session);
            }
        }
        cursor = window.next_winlink(Some(cursor));
    }
}

pub unsafe fn winlink_shuffle_up(
    session: &SessionRef,
    link: refbox::Weak<winlink>,
    before: i32,
) -> i32 {
    session.shuffle_window(link, before != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn winlink_index_keeps_order_and_weak_observers_across_moves() {
        unsafe {
            let mut head = Default::default();
            let first = winlink_add(&mut head, 1);
            let second = winlink_add(&mut head, 3);
            assert!(first.is_alive() && second.is_alive());
            assert!(!winlink_add(&mut head, 1).is_alive());
            winlinks_reindex(&mut head, (second).clone(), 2);

            let index_observer = first.get_unchecked().owner.clone();
            let node_observer = winlink_weak((first).clone());
            let mut moved = head;
            assert_eq!(winlinks_minmax(&moved, RB_NEGINF), first);
            assert_eq!(winlinks_next(first.get_unchecked()), second);
            assert_eq!(second.get_unchecked().idx, 2);

            winlink_remove(&mut moved, (first).clone());
            assert!(matches!(
                node_observer.try_access_mut(|link| link.idx),
                Err(refbox::BorrowError::Dropped)
            ));
            assert!(index_observer.try_borrow_mut().is_ok());
            let second_observer = winlink_weak((second).clone());
            winlink_remove(&mut moved, (second).clone());
            drop(moved);
            assert!(matches!(
                second_observer.try_access_mut(|link| link.idx),
                Err(refbox::BorrowError::Dropped)
            ));
            assert!(matches!(
                index_observer.try_borrow_mut(),
                Err(refbox::BorrowError::Dropped)
            ));
        }
    }
}
