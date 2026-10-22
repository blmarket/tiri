use crate::paste::paste_walk;
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::session::sessions;
use crate::winlink::winlink_window;
use std::ffi::CStr;

use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::{CLIENT_ATTACHED, CLIENT_UNATTACHEDFLAGS};
use crate::shared::key::*;
use crate::shared::key::{key_binding, key_table};
use crate::shared::pane::window_pane;
use crate::shared::paste::{paste_buffer, PasteBufferRef};
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::winlink;
use crate::shared::window::WindowRef;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
use std::cmp::Ordering;

use crate::window::{winlink_next, winlinks_minmax};

pub(super) fn sort_ordering(result: ::core::ffi::c_int, reversed: ::core::ffi::c_int) -> Ordering {
    let ordering = result.cmp(&0);
    if reversed != 0 {
        ordering.reverse()
    } else {
        ordering
    }
}

pub(super) fn sort_by_criteria<T>(
    values: &mut [T],
    sort_crit: &sort_criteria,
    mut compare: impl FnMut(&T, &T, &sort_criteria) -> Ordering,
) {
    if values.len() < 2 || sort_crit.order == SORT_END {
        return;
    }
    if sort_crit.order == SORT_ORDER {
        if sort_crit.reversed != 0 {
            values.reverse();
        }
        return;
    }
    values.sort_by(|a, b| compare(a, b, sort_crit));
}

fn sort_buffer_cmp(pa: &paste_buffer, pb: &paste_buffer, sort_crit: &sort_criteria) -> Ordering {
    let order = match sort_crit.order {
        SORT_NAME => pa.name.cmp(&pb.name),
        SORT_CREATION => pb.order.cmp(&pa.order),
        SORT_SIZE => pa.size().cmp(&pb.size()),
        _ => Ordering::Equal,
    }
    .then_with(|| pa.name.cmp(&pb.name));
    if sort_crit.reversed != 0 {
        order.reverse()
    } else {
        order
    }
}
unsafe fn sort_client_cmp(ca: &ClientRef, cb: &ClientRef, sort_crit: &sort_criteria) -> Ordering {
    let order = match sort_crit.order {
        SORT_SIZE => {
            let (a, b) = (ca.terminal_size(), cb.terminal_size());
            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
        }
        SORT_CREATION => {
            let (a, b) = (ca.creation_time(), cb.creation_time());
            a.cmp(&b)
        }
        SORT_ACTIVITY => {
            let (a, b) = (ca.activity_time(), cb.activity_time());
            b.cmp(&a)
        }
        _ => Ordering::Equal,
    }
    .then_with(|| {
        ca.name()
            .expect("client name")
            .cmp(&cb.name().expect("client name"))
    });
    if sort_crit.reversed != 0 {
        order.reverse()
    } else {
        order
    }
}
unsafe fn sort_winlink_cmp(
    wla: refbox::Weak<winlink>,
    wlb: refbox::Weak<winlink>,
    sort_crit: &sort_criteria,
) -> Ordering {
    let a = winlink_window(&wla).expect("linked window");
    let b = winlink_window(&wlb).expect("linked window");
    let mut result = match sort_crit.order {
        2 => {
            wla.try_borrow().expect("live winlink").idx
                - wlb.try_borrow().expect("live winlink").idx
        }
        1 => {
            let (a, b) = (a.creation_time(), b.creation_time());
            a.cmp(&b) as i32
        }
        0 => {
            let (a, b) = (a.activity_time(), b.activity_time());
            b.cmp(&a) as i32
        }
        4 => a.name().cmp(&b.name()) as i32,
        6 => {
            let (ax, ay) = a.size();
            let (bx, by) = b.size();
            ax.wrapping_mul(ay).cmp(&bx.wrapping_mul(by)) as i32
        }
        _ => 0,
    };
    if result == 0 {
        result = a.name().cmp(&b.name()) as i32;
    }
    sort_ordering(result, sort_crit.reversed)
}
/// Compare table names as strcasecmp(3) does in the UTF-8 locale: ASCII letters fold.
fn sort_key_binding_table_cmp(a: &key_binding, b: &key_binding) -> ::core::ffi::c_int {
    let a = a.tablename.as_deref().expect("sorted binding table");
    let b = b.tablename.as_deref().expect("sorted binding table");
    let (a, b) = (a.to_bytes().iter(), b.to_bytes().iter());
    a.map(u8::to_ascii_lowercase)
        .cmp(b.map(u8::to_ascii_lowercase)) as ::core::ffi::c_int
}
unsafe fn sort_key_binding_cmp(
    a: &key_binding,
    b: &key_binding,
    sort_crit: &sort_criteria,
) -> Ordering {
    let mut result: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    match sort_crit.order as ::core::ffi::c_uint {
        2 => {
            result = a.key.cmp(&b.key) as ::core::ffi::c_int;
        }
        3 => {
            let ak = a.key as ::core::ffi::c_ulonglong & KEYC_MASK_MODIFIERS;
            let bk = b.key as ::core::ffi::c_ulonglong & KEYC_MASK_MODIFIERS;
            result = ak.cmp(&bk) as ::core::ffi::c_int;
        }
        4 => {
            result = sort_key_binding_table_cmp(a, b);
        }
        _ => {}
    }
    if result == 0 as ::core::ffi::c_int {
        result = sort_key_binding_table_cmp(a, b);
    }
    if result == 0 as ::core::ffi::c_int {
        result = a.key.cmp(&b.key) as ::core::ffi::c_int;
    }
    sort_ordering(result, sort_crit.reversed)
}
pub fn sort_next_order(criteria: &mut sort_criteria) {
    let sequence = criteria.order_seq;
    if sequence.is_empty() {
        return;
    }
    let next = sequence
        .iter()
        .position(|&order| order == criteria.order)
        .map_or(0, |index| (index + 1) % sequence.len());
    criteria.order = sequence[next];
}
pub fn sort_order_from_string(order: Option<&CStr>) -> sort_order {
    let Some(order) = order else {
        return SORT_END;
    };
    let order = order.to_bytes();
    let is = |name: &[u8]| order.eq_ignore_ascii_case(name);
    if is(b"activity") {
        SORT_ACTIVITY
    } else if is(b"creation") {
        SORT_CREATION
    } else if is(b"index") || is(b"key") {
        SORT_INDEX
    } else if is(b"modifier") {
        SORT_MODIFIER
    } else if is(b"name") || is(b"title") {
        SORT_NAME
    } else if is(b"order") {
        SORT_ORDER
    } else if is(b"size") {
        SORT_SIZE
    } else {
        SORT_END
    }
}
pub fn sort_order_to_string(order: sort_order) -> Option<&'static CStr> {
    if order as ::core::ffi::c_uint == SORT_ACTIVITY as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"activity");
    }
    if order as ::core::ffi::c_uint == SORT_CREATION as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"creation");
    }
    if order as ::core::ffi::c_uint == SORT_INDEX as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"index");
    }
    if order as ::core::ffi::c_uint == SORT_MODIFIER as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"modifier");
    }
    if order as ::core::ffi::c_uint == SORT_NAME as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"name");
    }
    if order as ::core::ffi::c_uint == SORT_ORDER as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"order");
    }
    if order as ::core::ffi::c_uint == SORT_SIZE as ::core::ffi::c_int as ::core::ffi::c_uint {
        return Some(c"size");
    }
    None
}
pub unsafe fn sort_would_window_tree_swap(
    sort_crit: &sort_criteria,
    mut wla: refbox::Weak<winlink>,
    mut wlb: refbox::Weak<winlink>,
) -> ::core::ffi::c_int {
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_INDEX as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return 0 as ::core::ffi::c_int;
    }
    (sort_winlink_cmp((wla).clone(), (wlb).clone(), sort_crit) != Ordering::Equal)
        as ::core::ffi::c_int
}
pub fn sort_get_buffers(sort_crit: &sort_criteria) -> Vec<PasteBufferRef> {
    let mut buffers = Vec::new();
    let mut next = paste_walk(None);
    while let Some(pb) = next {
        next = paste_walk(Some(&pb));
        buffers.push(pb);
    }
    sort_by_criteria(&mut buffers, sort_crit, |a, b, criteria| {
        if a == b {
            Ordering::Equal
        } else {
            sort_buffer_cmp(&a.borrow(), &b.borrow(), criteria)
        }
    });
    buffers
}
pub unsafe fn sort_get_clients(sort_crit: &sort_criteria) -> Vec<ClientRef> {
    let mut clients_sorted = Vec::new();
    let mut current = ClientRegistry::first();
    while let Some(owner) = current {
        current = ClientRegistry::next(&owner);
        let client = &owner;
        if client.flags() & CLIENT_UNATTACHEDFLAGS as uint64_t == 0
            && client.flags() & CLIENT_ATTACHED as uint64_t != 0
        {
            clients_sorted.push(owner);
        }
    }
    sort_by_criteria(&mut clients_sorted, sort_crit, |a, b, criteria| unsafe {
        sort_client_cmp(a, b, criteria)
    });
    clients_sorted
}
pub unsafe fn sort_get_sessions(criteria: &sort_criteria) -> Vec<SessionRef> {
    SessionRef::sorted(criteria)
}
pub unsafe fn sort_get_panes_window(
    w: &WindowRef,
    sort_crit: &sort_criteria,
) -> Vec<std::rc::Rc<std::cell::RefCell<window_pane>>> {
    let mut panes = w
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .collect::<Vec<_>>();
    sort_by_criteria(&mut panes, sort_crit, |a, b, criteria| unsafe {
        a.compare_for_sort(b, criteria)
    });
    panes
}
pub unsafe fn sort_get_winlinks(sort_crit: &sort_criteria) -> Vec<refbox::Weak<winlink>> {
    let mut links = Vec::new();
    let mut s_owner = sessions::first();
    while s_owner.is_some() {
        let mut wl = s_owner
            .as_ref()
            .expect("session traversal owner")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            links.push(wl.clone());
            wl = winlink_next(&wl);
        }
        s_owner = sessions::after(s_owner.as_ref().expect("live session").name().as_c_str());
    }
    sort_by_criteria(&mut links, sort_crit, |a, b, criteria| unsafe {
        sort_winlink_cmp((*a).clone(), (*b).clone(), criteria)
    });
    links
}
pub unsafe fn sort_get_winlinks_session(
    s_owner: &SessionRef,
    sort_crit: &sort_criteria,
) -> Vec<refbox::Weak<winlink>> {
    let mut l = Vec::new();
    let mut wl = s_owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while wl.is_alive() {
        l.push(wl.clone());
        wl = winlink_next(&wl);
    }
    sort_by_criteria(&mut l, sort_crit, |a, b, criteria| unsafe {
        sort_winlink_cmp((*a).clone(), (*b).clone(), criteria)
    });
    l
}
pub fn sort_get_key_bindings<'a>(
    tables: &'a [std::cell::Ref<'_, key_table>],
    sort_crit: &sort_criteria,
) -> Vec<&'a key_binding> {
    let mut bindings: Vec<_> = tables
        .iter()
        .flat_map(|table| table.key_bindings.iter())
        .collect();
    sort_by_criteria(&mut bindings, sort_crit, |a, b, criteria| unsafe {
        sort_key_binding_cmp(a, b, criteria)
    });
    bindings
}

pub fn sort_get_key_bindings_table<'a>(
    table: Option<&'a key_table>,
    sort_crit: &sort_criteria,
) -> Vec<&'a key_binding> {
    let mut bindings: Vec<_> = table
        .into_iter()
        .flat_map(|table| table.key_bindings.iter())
        .collect();
    sort_by_criteria(&mut bindings, sort_crit, |a, b, criteria| unsafe {
        sort_key_binding_cmp(a, b, criteria)
    });
    bindings
}

#[cfg(test)]
mod sequence_tests {
    use super::*;

    #[test]
    fn equal_keys_preserve_input_order_in_both_directions() {
        for reversed in [0, 1] {
            let criteria = sort_criteria {
                order: SORT_NAME,
                reversed,
                order_seq: &[],
            };
            let mut values: Vec<_> = (0..30).map(|index| (index % 2, index)).collect();
            sort_by_criteria(&mut values, &criteria, |a, b, criteria| {
                sort_ordering(a.0 - b.0, criteria.reversed)
            });
            let first = reversed;
            let expected: Vec<_> = (first..30)
                .step_by(2)
                .chain((1 - first..30).step_by(2))
                .collect();
            assert_eq!(
                values.iter().map(|value| value.1).collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn sort_sequence_cycles_and_recovers_unknown_order() {
        let mut criteria = sort_criteria {
            order: SORT_NAME,
            reversed: 0,
            order_seq: &[SORT_NAME, SORT_SIZE],
        };
        sort_next_order(&mut criteria);
        assert_eq!(criteria.order, SORT_SIZE);
        sort_next_order(&mut criteria);
        assert_eq!(criteria.order, SORT_NAME);
        criteria.order = SORT_ACTIVITY;
        sort_next_order(&mut criteria);
        assert_eq!(criteria.order, SORT_NAME);
        criteria.order_seq = &[];
        sort_next_order(&mut criteria);
        assert_eq!(criteria.order, SORT_NAME);
        criteria.order_seq = &[SORT_SIZE];
        sort_next_order(&mut criteria);
        sort_next_order(&mut criteria);
        assert_eq!(criteria.order, SORT_SIZE);
    }

    #[test]
    fn sort_order_names_round_trip_and_end_has_no_name() {
        for order in [
            SORT_ACTIVITY,
            SORT_CREATION,
            SORT_INDEX,
            SORT_MODIFIER,
            SORT_NAME,
            SORT_ORDER,
            SORT_SIZE,
        ] {
            let name = sort_order_to_string(order).expect("sort order has a name");
            assert_eq!(sort_order_from_string(Some(name)), order);
        }
        assert_eq!(sort_order_to_string(SORT_END), None);
        assert_eq!(sort_order_from_string(None), SORT_END);
    }
}
