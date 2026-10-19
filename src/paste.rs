use crate::events::events_fire;
use crate::events_payload::{event_payload_create, event_payload_set_string};
use crate::options::options_get_number;
use crate::options::OptionsScope;
use crate::shared::abi::*;
use crate::shared::paste::{paste_buffer, PasteBufferRef};
use crate::shared::tree::{RB_INF, RB_NEGINF};
use crate::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::text::utf8::utf8_strvis;
use crate::tmux::clean_name_cstring;
use refbox::RefBox;
use std::cell::RefCell;
use std::ffi::{CStr, CString};

fn paste_name_cause(cause: Option<&mut Option<CString>>, prefix: &[u8], name: &CStr) {
    let Some(cause) = cause else {
        return;
    };
    let mut message = prefix.to_vec();
    message.extend_from_slice(name.to_bytes());
    *cause = Some(CString::new(message).expect("paste diagnostic contains no NUL"));
}

#[derive(Default)]
pub struct paste_time_tree {
    entries: std::collections::BTreeMap<std::cmp::Reverse<u_int>, RefBox<paste_buffer>>,
}
#[derive(Default)]
pub struct paste_name_tree {
    entries: std::collections::BTreeMap<CString, PasteBufferRef>,
}

thread_local! {
    static paste_next_index: RefCell<u_int> = const { RefCell::new(0) };
    static paste_next_order: RefCell<u_int> = const { RefCell::new(0) };
    static paste_num_automatic: RefCell<u_int> = const { RefCell::new(0) };
    static paste_by_name: RefCell<paste_name_tree> = RefCell::new(paste_name_tree::default());
    static paste_by_time: RefCell<paste_time_tree> = RefCell::new(paste_time_tree::default());
}

fn paste_new_owned(name: CString) -> RefBox<paste_buffer> {
    RefBox::new(paste_buffer {
        name,
        ..paste_buffer::empty()
    })
}

fn paste_store_data(pb: &mut paste_buffer, data: Option<Box<[u8]>>) {
    pb.data = data;
}

fn paste_replace_name(pb: &mut paste_buffer, name: CString) -> CString {
    std::mem::replace(&mut pb.name, name)
}

fn paste_name_tree_find(head: &paste_name_tree, name: &CStr) -> Option<PasteBufferRef> {
    head.entries.get(name).cloned()
}

fn paste_name_tree_insert(
    head: &mut paste_name_tree,
    elm: &PasteBufferRef,
) -> Option<PasteBufferRef> {
    let key = elm.borrow().name.clone();
    match head.entries.entry(key) {
        std::collections::btree_map::Entry::Occupied(entry) => Some(entry.get().clone()),
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(elm.clone());
            None
        }
    }
}

fn paste_name_tree_remove(
    head: &mut paste_name_tree,
    elm: &paste_buffer,
) -> Option<PasteBufferRef> {
    head.entries.remove(elm.name.as_c_str())
}

fn paste_time_key(pb: &paste_buffer) -> std::cmp::Reverse<u_int> {
    std::cmp::Reverse(pb.order)
}

fn paste_time_tree_insert(
    head: &mut paste_time_tree,
    elm: RefBox<paste_buffer>,
) -> Option<PasteBufferRef> {
    let key = paste_time_key(&elm.try_borrow_mut().unwrap());
    match head.entries.entry(key) {
        std::collections::btree_map::Entry::Occupied(entry) => {
            Some(PasteBufferRef::observe(entry.get()))
        }
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(elm);
            None
        }
    }
}

fn paste_time_tree_minmax(head: &paste_time_tree, val: i32) -> Option<PasteBufferRef> {
    if val < 0 {
        head.entries.values().next()
    } else {
        head.entries.values().next_back()
    }
    .map(PasteBufferRef::observe)
}

fn paste_time_tree_next(head: &paste_time_tree, elm: &paste_buffer) -> Option<PasteBufferRef> {
    head.entries
        .range((
            std::ops::Bound::Excluded(paste_time_key(elm)),
            std::ops::Bound::Unbounded,
        ))
        .next()
        .map(|(_, entry)| PasteBufferRef::observe(entry))
}

fn paste_time_tree_prev(head: &paste_time_tree, elm: &paste_buffer) -> Option<PasteBufferRef> {
    head.entries
        .range((
            std::ops::Bound::Unbounded,
            std::ops::Bound::Excluded(paste_time_key(elm)),
        ))
        .next_back()
        .map(|(_, entry)| PasteBufferRef::observe(entry))
}

fn paste_time_tree_remove(
    head: &mut paste_time_tree,
    elm: &paste_buffer,
) -> Option<RefBox<paste_buffer>> {
    head.entries.remove(&paste_time_key(elm))
}

fn paste_name_tree_find_local(name: &CStr) -> Option<PasteBufferRef> {
    paste_by_name.with(|head| paste_name_tree_find(&head.borrow(), name))
}
fn paste_name_tree_insert_local(elm: &PasteBufferRef) -> Option<PasteBufferRef> {
    paste_by_name.with(|head| paste_name_tree_insert(&mut head.borrow_mut(), elm))
}
fn paste_name_tree_remove_local(elm: &paste_buffer) -> Option<PasteBufferRef> {
    paste_by_name.with(|head| paste_name_tree_remove(&mut head.borrow_mut(), elm))
}
fn paste_time_tree_insert_local(elm: RefBox<paste_buffer>) -> Option<PasteBufferRef> {
    paste_by_time.with(|head| paste_time_tree_insert(&mut head.borrow_mut(), elm))
}
fn paste_time_tree_minmax_local(val: i32) -> Option<PasteBufferRef> {
    paste_by_time.with(|head| paste_time_tree_minmax(&head.borrow(), val))
}
fn paste_time_tree_next_local(elm: &paste_buffer) -> Option<PasteBufferRef> {
    paste_by_time.with(|head| paste_time_tree_next(&head.borrow(), elm))
}
fn paste_time_tree_prev_local(elm: &paste_buffer) -> Option<PasteBufferRef> {
    paste_by_time.with(|head| paste_time_tree_prev(&head.borrow(), elm))
}
fn paste_time_tree_remove_local(elm: &paste_buffer) -> Option<RefBox<paste_buffer>> {
    paste_by_time.with(|head| paste_time_tree_remove(&mut head.borrow_mut(), elm))
}
fn paste_time_tree_is_empty_local() -> bool {
    paste_by_time.with(|head| head.borrow().entries.is_empty())
}

fn paste_automatic_count() -> u_int {
    paste_num_automatic.with(|count| *count.borrow())
}

fn paste_automatic_count_increment() {
    paste_num_automatic.with(|count| {
        let mut count = count.borrow_mut();
        *count = (*count).wrapping_add(1);
    });
}

fn paste_automatic_count_decrement() {
    paste_num_automatic.with(|count| {
        let mut count = count.borrow_mut();
        *count = (*count).wrapping_sub(1);
    });
}

fn paste_next_index_take() -> u_int {
    paste_next_index.with(|index| {
        let mut index = index.borrow_mut();
        let current = *index;
        *index = (*index).wrapping_add(1);
        current
    })
}

fn paste_next_order_take() -> u_int {
    paste_next_order.with(|order| {
        let mut order = order.borrow_mut();
        let current = *order;
        *order = (*order).wrapping_add(1);
        current
    })
}

unsafe fn paste_fire_event(
    name: &CStr,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut ep = event_payload_create();
    event_payload_set_string(&mut ep, c"paste_buffer", write);
    events_fire(name, ep);
}

pub fn paste_buffer_name(pb: &paste_buffer) -> &CStr {
    &pb.name
}
pub fn paste_buffer_order(pb: &paste_buffer) -> u_int {
    pb.order
}
pub fn paste_buffer_created(pb: &paste_buffer) -> time_t {
    pb.created
}
pub fn paste_buffer_data(pb: &paste_buffer) -> Option<&[u8]> {
    pb.data.as_deref()
}
pub fn paste_walk(pb: Option<&PasteBufferRef>) -> Option<PasteBufferRef> {
    match pb {
        Some(pb) => paste_time_tree_next_local(&*pb.try_borrow()?),
        None => paste_time_tree_minmax_local(RB_NEGINF),
    }
}
pub fn paste_is_empty() -> i32 {
    paste_time_tree_is_empty_local() as i32
}
pub(crate) fn paste_get_top(name: Option<&mut Option<CString>>) -> Option<PasteBufferRef> {
    let mut next = paste_time_tree_minmax_local(RB_NEGINF);
    while let Some(pb) = next {
        if pb.borrow().automatic != 0 {
            if let Some(name) = name {
                *name = Some(pb.borrow().name.clone());
            }
            return Some(pb);
        }
        next = paste_time_tree_next_local(&pb.borrow());
    }
    None
}
pub fn paste_get_name(name: &CStr) -> Option<PasteBufferRef> {
    if name.is_empty() {
        None
    } else {
        paste_name_tree_find_local(name)
    }
}
fn paste_is_registered(pb: &PasteBufferRef) -> bool {
    pb.try_borrow()
        .is_some_and(|buffer| paste_get_name(&buffer.name).is_some_and(|current| current == *pb))
}
pub unsafe fn paste_free(pb: &PasteBufferRef) {
    if !paste_is_registered(pb) {
        return;
    }
    paste_fire_event(c"paste-buffer-deleted", |out| {
        out.write_all(pb.borrow().name.as_bytes())
    });
    if !paste_is_registered(pb) {
        return;
    }
    let buffer = pb.borrow();
    paste_name_tree_remove_local(&buffer);
    paste_time_tree_remove_local(&buffer);
    if buffer.automatic != 0 {
        paste_automatic_count_decrement();
    }
}
pub(crate) unsafe fn paste_add_owned(prefix: Option<CString>, data: Box<[u8]>) {
    if data.is_empty() {
        return;
    }
    let prefix_bytes = prefix
        .as_ref()
        .map_or(b"buffer".as_slice(), CString::as_bytes);
    let limit = OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"buffer-limit"))
        as u_int;
    let mut next = paste_time_tree_minmax_local(RB_INF);
    while let Some(pb) = next {
        if paste_automatic_count() < limit {
            break;
        }
        let Some(buffer) = pb.try_borrow() else {
            next = paste_time_tree_minmax_local(RB_INF);
            continue;
        };
        next = paste_time_tree_prev_local(&buffer);
        let automatic = buffer.automatic != 0;
        drop(buffer);
        if automatic {
            paste_free(&pb);
        }
    }
    let owner = loop {
        let mut bytes = Vec::with_capacity(prefix_bytes.len() + 10);
        bytes.extend_from_slice(prefix_bytes);
        bytes.extend_from_slice(paste_next_index_take().to_string().as_bytes());
        let name = CString::new(bytes).expect("generated buffer name has no NUL");
        if paste_get_name(&name).is_none() {
            break paste_new_owned(name);
        }
    };
    let pb = PasteBufferRef::observe(&owner);
    {
        let mut buffer = pb.borrow_mut();
        paste_store_data(&mut buffer, Some(data));
        buffer.automatic = 1;
        paste_automatic_count_increment();
        buffer.created = crate::shared::time::unix_seconds(std::time::SystemTime::now());
        buffer.order = paste_next_order_take();
    }
    paste_name_tree_insert_local(&pb);
    paste_time_tree_insert_local(owner);
    paste_fire_event(c"paste-buffer-changed", |out| {
        out.write_all(pb.borrow().name.as_bytes())
    });
}
pub unsafe fn paste_rename(
    oldname: Option<&CStr>,
    newname: Option<&CStr>,
    mut cause: Option<&mut Option<CString>>,
) -> i32 {
    if let Some(cause) = cause.as_deref_mut() {
        *cause = None;
    }
    let Some(oldname) = oldname.filter(|name| !name.is_empty()) else {
        paste_name_cause(cause, b"no buffer", c"");
        return -1;
    };
    let Some(newname) = newname.filter(|name| !name.is_empty()) else {
        paste_name_cause(cause, b"new name is empty", c"");
        return -1;
    };
    let Some(name) = clean_name_cstring(newname, 0) else {
        paste_name_cause(cause, b"invalid buffer name: ", newname);
        return -1;
    };
    let Some(pb) = paste_get_name(oldname) else {
        paste_name_cause(cause, b"no buffer ", oldname);
        return -1;
    };
    if let Some(replaced) = paste_get_name(&name) {
        if pb == replaced {
            return 0;
        }
        paste_free(&replaced);
        if !paste_is_registered(&pb) {
            paste_name_cause(cause, b"no buffer ", oldname);
            return -1;
        }
    }
    paste_name_tree_remove_local(&pb.borrow());
    let previous = {
        let mut buffer = pb.borrow_mut();
        let previous = paste_replace_name(&mut buffer, name);
        if buffer.automatic != 0 {
            paste_automatic_count_decrement();
        }
        buffer.automatic = 0;
        previous
    };
    paste_name_tree_insert_local(&pb);
    let changed_name = pb.borrow().name.clone();
    paste_fire_event(c"paste-buffer-deleted", |out| {
        out.write_all(previous.as_bytes())
    });
    paste_fire_event(c"paste-buffer-changed", |out| {
        out.write_all(changed_name.as_bytes())
    });
    0
}
pub(crate) unsafe fn paste_set_owned(
    data: Box<[u8]>,
    name: Option<&CStr>,
    mut cause: Option<&mut Option<CString>>,
) -> i32 {
    if let Some(cause) = cause.as_deref_mut() {
        *cause = None;
    }
    if data.is_empty() {
        return 0;
    }
    let Some(name) = name else {
        paste_add_owned(None, data);
        return 0;
    };
    if name.is_empty() {
        paste_name_cause(cause, b"empty buffer name", c"");
        return -1;
    }
    let Some(newname) = clean_name_cstring(name, 0) else {
        paste_name_cause(cause, b"invalid buffer name: ", name);
        return -1;
    };
    let owner = paste_new_owned(newname);
    let pb = PasteBufferRef::observe(&owner);
    {
        let mut buffer = pb.borrow_mut();
        paste_store_data(&mut buffer, Some(data));
        buffer.order = paste_next_order_take();
        buffer.created = crate::shared::time::unix_seconds(std::time::SystemTime::now());
    }
    let old = paste_get_name(&pb.borrow().name);
    if let Some(old) = old {
        paste_free(&old);
    }
    paste_name_tree_insert_local(&pb);
    paste_time_tree_insert_local(owner);
    paste_fire_event(c"paste-buffer-changed", |out| {
        out.write_all(pb.borrow().name.as_bytes())
    });
    0
}
/// Replace the bytes while preserving the buffer identity and creation order.
pub(crate) unsafe fn paste_replace_owned(pb: &PasteBufferRef, data: Box<[u8]>) {
    let Some(mut buffer) = pb.try_borrow() else {
        return;
    };
    paste_store_data(&mut buffer, Some(data));
    let name = buffer.name.clone();
    drop(buffer);
    paste_fire_event(c"paste-buffer-changed", |out| {
        out.write_all(name.as_bytes())
    });
}

pub(crate) fn paste_make_sample_cstring(pb: &paste_buffer) -> CString {
    let flags = VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL;
    let width = 200;
    let data = pb.data.as_deref().unwrap_or(&[]);
    let len = data.len().min(width);
    let mut buffer = vec![0u8; len * 8 + 4];
    let used = utf8_strvis(&mut buffer, &data[..len], flags);
    if data.len() > width || used > width {
        buffer[width..width + 4].copy_from_slice(b"...\0");
    }
    let length = CStr::from_bytes_until_nul(&buffer)
        .expect("sample contains a terminating NUL")
        .to_bytes_with_nul()
        .len();
    buffer.truncate(length);
    CString::from_vec_with_nul(buffer).expect("sample contains one terminating NUL")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::CString, rc::Rc};

    #[test]
    fn invalid_names_return_owned_diagnostics() {
        unsafe {
            let mut cause: Option<CString> = Some(c"stale".to_owned());
            assert_eq!(
                paste_set_owned(
                    b"payload".to_vec().into_boxed_slice(),
                    Some(c""),
                    Some(&mut cause),
                ),
                -1
            );
            assert_eq!(cause.as_ref().unwrap().as_c_str(), c"empty buffer name");

            cause = Some(c"stale".to_owned());
            let name = c"owned-cause-success";
            assert_eq!(
                paste_set_owned(
                    b"payload".to_vec().into_boxed_slice(),
                    Some(name),
                    Some(&mut cause),
                ),
                0
            );
            assert!(cause.is_none());
            paste_free(&paste_get_name(name).unwrap());

            cause = Some(c"stale".to_owned());
            assert_eq!(
                paste_set_owned(Box::<[u8]>::default(), None, Some(&mut cause),),
                0
            );
            assert!(cause.is_none(), "empty-data success clears the cause");

            assert_eq!(
                paste_set_owned(b"payload".to_vec().into_boxed_slice(), Some(c""), None,),
                -1,
                "failure can discard its diagnostic"
            );
            assert_eq!(
                paste_set_owned(
                    b"payload".to_vec().into_boxed_slice(),
                    Some(c"owned-cause-none"),
                    None,
                ),
                0,
                "success can discard its diagnostic"
            );
            paste_free(&paste_get_name(c"owned-cause-none").unwrap());

            cause = Some(c"stale".to_owned());
            assert_eq!(paste_rename(None, Some(c"renamed"), Some(&mut cause)), -1);
            assert_eq!(cause.as_ref().unwrap().as_c_str(), c"no buffer");
        }
    }

    fn named_buffer(name: &CString) -> RefBox<paste_buffer> {
        paste_new_owned(name.clone())
    }

    fn ordered_buffer(name: &CString, order: u_int) -> RefBox<paste_buffer> {
        let buffer = named_buffer(name);
        buffer.try_borrow_mut().unwrap().order = order;
        buffer
    }

    #[test]
    fn paste_name_tree_matches_strcmp_order_and_duplicate_semantics() {
        let names = [
            CString::new("z").unwrap(),
            CString::new("a").unwrap(),
            CString::new(vec![b'a', 0xff]).unwrap(),
            CString::new("a0").unwrap(),
        ];
        let owners = names.iter().map(named_buffer).collect::<Vec<_>>();
        let items = owners
            .iter()
            .map(PasteBufferRef::observe)
            .collect::<Vec<_>>();
        let [z, a, a_high, a0] = [&items[0], &items[1], &items[2], &items[3]];
        let duplicate = named_buffer(&CString::new("a").unwrap());
        let mut tree = paste_name_tree::default();
        for item in [z, a_high, a, a0] {
            assert!(paste_name_tree_insert(&mut tree, item).is_none());
        }
        assert!(PartialEq::eq(
            &paste_name_tree_insert(&mut tree, &PasteBufferRef::observe(&duplicate)).unwrap(),
            a
        ));
        assert_eq!(
            tree.entries
                .keys()
                .map(|key| key.to_bytes())
                .collect::<Vec<_>>(),
            vec![&b"a"[..], &b"a0"[..], &b"a\xff"[..], &b"z"[..]]
        );
        assert!(PartialEq::eq(
            &paste_name_tree_find(&tree, &names[2]).unwrap(),
            a_high
        ));
        assert!(paste_name_tree_find(&tree, c"missing").is_none());
        assert!(PartialEq::eq(
            &paste_name_tree_remove(&mut tree, &a_high.borrow()).unwrap(),
            a_high
        ));
        assert!(paste_name_tree_find(&tree, &names[2]).is_none());
    }

    #[test]
    fn paste_time_tree_matches_reverse_order_and_duplicate_semantics() {
        let oldest_owner = ordered_buffer(&CString::new("oldest").unwrap(), 4);
        let middle_owner = ordered_buffer(&CString::new("middle").unwrap(), 7);
        let newest_owner = ordered_buffer(&CString::new("newest").unwrap(), 12);
        let duplicate = ordered_buffer(&CString::new("duplicate").unwrap(), 7);
        let oldest = PasteBufferRef::observe(&oldest_owner);
        let middle = PasteBufferRef::observe(&middle_owner);
        let newest = PasteBufferRef::observe(&newest_owner);
        let mut tree = paste_time_tree::default();
        for item in [oldest_owner, newest_owner, middle_owner] {
            assert!(paste_time_tree_insert(&mut tree, item).is_none());
        }
        assert!(PartialEq::eq(
            &paste_time_tree_insert(&mut tree, duplicate).unwrap(),
            &middle
        ));
        assert_eq!(
            paste_time_tree_minmax(&tree, RB_NEGINF)
                .unwrap()
                .borrow()
                .order,
            12
        );
        assert_eq!(
            paste_time_tree_next(&tree, &newest.borrow())
                .unwrap()
                .borrow()
                .order,
            7
        );
        assert_eq!(
            paste_time_tree_next(&tree, &middle.borrow())
                .unwrap()
                .borrow()
                .order,
            4
        );
        assert!(paste_time_tree_next(&tree, &oldest.borrow()).is_none());
        assert_eq!(
            paste_time_tree_minmax(&tree, RB_INF)
                .unwrap()
                .borrow()
                .order,
            4
        );
        assert_eq!(
            paste_time_tree_prev(&tree, &oldest.borrow())
                .unwrap()
                .borrow()
                .order,
            7
        );
        assert_eq!(
            paste_time_tree_prev(&tree, &middle.borrow())
                .unwrap()
                .borrow()
                .order,
            12
        );
        assert!(paste_time_tree_prev(&tree, &newest.borrow()).is_none());
        assert!(PartialEq::eq(
            &PasteBufferRef::observe(&paste_time_tree_remove(&mut tree, &middle.borrow()).unwrap()),
            &middle
        ));
        assert_eq!(
            paste_time_tree_next(&tree, &newest.borrow())
                .unwrap()
                .borrow()
                .order,
            4
        );
    }

    #[test]
    fn paste_events_release_borrows_and_preserve_registry_visibility() {
        use crate::events::{events_add_sink, events_remove_sink};
        use crate::events_payload::event_payload_get_string;
        let seen = Rc::new(RefCell::new(Vec::new()));
        unsafe {
            let mut sinks = Vec::new();
            for event in [c"paste-buffer-changed", c"paste-buffer-deleted"] {
                let seen = seen.clone();
                sinks.push(events_add_sink(
                    event,
                    Rc::new(move |event, payload| {
                        let name = event_payload_get_string(payload).expect("paste buffer name");
                        let buffer = paste_get_name(name);
                        seen.borrow_mut().push((
                            event.to_owned(),
                            name.to_owned(),
                            buffer.is_some(),
                        ));
                        if let Some(buffer) = buffer {
                            buffer.borrow_mut().created = 17;
                        }
                        if event == c"paste-buffer-changed" && name == c"root" {
                            assert_eq!(
                                paste_set_owned(
                                    b"nested".to_vec().into_boxed_slice(),
                                    Some(c"side"),
                                    None
                                ),
                                0
                            );
                        }
                    }),
                ));
            }
            assert_eq!(
                paste_set_owned(b"first".to_vec().into_boxed_slice(), Some(c"root"), None),
                0
            );
            assert_eq!(paste_rename(Some(c"root"), Some(c"renamed"), None), 0);
            assert_eq!(
                paste_set_owned(
                    b"second".to_vec().into_boxed_slice(),
                    Some(c"renamed"),
                    None
                ),
                0
            );
            paste_free(&paste_get_name(c"renamed").unwrap());
            paste_free(&paste_get_name(c"side").unwrap());
            for sink in sinks {
                events_remove_sink(sink);
            }
        }
        let expected = [
            (c"paste-buffer-changed", c"root", true),
            (c"paste-buffer-changed", c"side", true),
            (c"paste-buffer-deleted", c"root", false),
            (c"paste-buffer-changed", c"renamed", true),
            (c"paste-buffer-deleted", c"renamed", true),
            (c"paste-buffer-changed", c"renamed", true),
            (c"paste-buffer-deleted", c"renamed", true),
            (c"paste-buffer-deleted", c"side", true),
        ];
        assert_eq!(
            seen.borrow()
                .iter()
                .map(|(event, name, visible)| (event.as_c_str(), name.as_c_str(), *visible))
                .collect::<Vec<_>>(),
            expected
        );
    }
}
