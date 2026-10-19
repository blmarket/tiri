use crate::log::log_cstr;

use super::*;
use crate::shared::client::ClientRef;
use crate::shared::format::FormatEntryState;
use std::ffi::{CStr, CString};
use std::time::SystemTime;

pub(super) fn format_entry_tree_find<'a>(
    tree: &'a format_entry_tree,
    key: &CStr,
) -> Option<&'a format_entry> {
    tree.get(key).map(Box::as_ref)
}

/// Evaluate without retaining an entry borrow: callbacks may replace or remove
/// entries in this tree. As with the C API, callbacks must not destroy the tree.
unsafe fn format_entry_ensure_value(ft: &mut format_tree, key: &CStr) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_EVALUATION: AtomicU64 = AtomicU64::new(0);
    let (evaluation, callback) = {
        let Some(entry) = ft.tree.get_mut(key) else {
            return;
        };
        if !matches!(entry.state, FormatEntryState::Lazy(_)) {
            return;
        }
        let evaluation = NEXT_EVALUATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("format evaluation identity exhausted");
        (
            evaluation,
            std::mem::replace(&mut entry.state, FormatEntryState::Evaluating(evaluation)),
        )
    };
    let FormatEntryState::Lazy(mut callback) = callback else {
        unreachable!()
    };
    let value = callback(ft).unwrap_or_default();

    if let Some(entry) = ft.tree.get_mut(key) {
        if matches!(entry.state, FormatEntryState::Evaluating(id) if id == evaluation) {
            entry.state = FormatEntryState::Cached { value, callback };
        }
    }
}

/// Copy the resolved value after any lazy callback has finished mutating the tree.
pub(super) unsafe fn format_entry_get_value(
    ft: &mut format_tree,
    key: &CStr,
) -> Option<FormatValue> {
    let key = key.to_owned();
    format_entry_ensure_value(ft, &key);
    let entry = format_entry_tree_find(&ft.tree, &key)?;
    match &entry.state {
        FormatEntryState::Time(time) if *time != 0 => Some(FormatValue::Time(*time)),
        state => state
            .text()
            .map(|value| FormatValue::String(value.to_owned())),
    }
}

unsafe fn format_entry_set(ft: &mut format_tree, key: &CStr, state: FormatEntryState) {
    use std::collections::btree_map::Entry;

    let key = key.to_owned();
    let old = match ft.tree.entry(key.clone()) {
        Entry::Occupied(mut entry) => Some(std::mem::replace(&mut entry.get_mut().state, state)),
        Entry::Vacant(entry) => {
            entry.insert(Box::new(format_entry { key, state }));
            None
        }
    };
    drop(old);
}

pub unsafe fn format_merge(ft: &mut format_tree, from: &format_tree) {
    let values: Vec<_> = from
        .tree
        .values()
        .filter_map(|entry| {
            entry
                .state
                .text()
                .map(|value| (entry.key.clone(), value.to_owned()))
        })
        .collect();
    for (key, value) in values {
        format_add_value(ft, &key, value);
    }
}
pub fn format_get_pane(
    ft: &format_tree,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    ft.wp.upgrade()
}
pub(super) unsafe fn format_create_add_item(
    ft: &mut format_tree,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) {
    let item = item_handle.borrow();
    let mut event_snapshot = cmdq_get_event(&*(item));
    let event: *mut key_event = &mut event_snapshot;
    let mut m: *mut mouse_event = &raw mut (*event).m;
    cmdq_merge_formats(item_handle, ft);
    memcpy(
        &raw mut ft.m as *mut ::core::ffi::c_void,
        m as *const ::core::ffi::c_void,
        ::core::mem::size_of::<mouse_event>() as size_t,
    );
}
/// Construct the sole owner; callers may project pointers for legacy readers.
unsafe fn format_create_box(
    c: Option<ClientRef>,
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    tag: ::core::ffi::c_int,
    flags: ::core::ffi::c_int,
) -> Box<format_tree> {
    let mut owner = Box::new(format_tree::default());
    let ft = &mut *owner;
    ft.client = c;
    ft.item = item_handle.map(std::rc::Rc::downgrade).unwrap_or_default();
    ft.tag = tag as u_int;
    ft.flags = flags;
    if let Some(item_handle) = item_handle {
        format_create_add_item(ft, item_handle);
    }
    owner
}

pub unsafe fn format_create(
    c_owner: Option<&ClientRef>,
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    tag: ::core::ffi::c_int,
    flags: ::core::ffi::c_int,
) -> Box<format_tree> {
    format_create_box(c_owner.cloned(), item_handle, tag, flags)
}

/// Construct a format tree retaining the supplied client handle.
/// Clone the handle before merging command-item formats.
///
/// # Safety
/// A non-null `item` must point to a live command-queue item.
pub unsafe fn format_create_with_client(
    c: Option<&ClientRef>,
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    tag: ::core::ffi::c_int,
    flags: ::core::ffi::c_int,
) -> Box<format_tree> {
    format_create_box(c.cloned(), item_handle, tag, flags)
}

pub unsafe fn format_create_owned(
    c: Option<&ClientRef>,
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    tag: ::core::ffi::c_int,
    flags: ::core::ffi::c_int,
) -> Box<format_tree> {
    format_create_box(c.cloned(), item_handle, tag, flags)
}

impl Drop for format_tree {
    fn drop(&mut self) {
        while let Some((_, entry)) = self.tree.pop_first() {
            drop(entry);
        }
        if let Some(client) = self.client.take() {
            drop(client);
        }
    }
}

pub unsafe fn format_log_debug(ft: &mut format_tree, prefix: &CStr) {
    if log_get_level() == 0 as ::core::ffi::c_int {
        return;
    }
    format_each(ft, |key, value| {
        log_debug(format_args!(
            "{}: {}={}",
            log_cstr(prefix),
            log_cstr(key),
            log_cstr(value)
        ));
    });
}
pub unsafe fn format_each(ft: &mut format_tree, mut cb: impl FnMut(&CStr, &CStr)) {
    for entry in &FORMAT_TABLE {
        let Some(value) = entry.get(ft) else { continue };
        let value = match value {
            FormatValue::String(value) => value,
            FormatValue::Time(value) => {
                CString::new(value.to_string()).expect("timestamp contains no NUL")
            }
        };
        cb(entry.key, value.as_c_str());
    }
    for (key, value) in crate::plugin::each(format_plugin_pane(ft)) {
        if format_entry_tree_find(&ft.tree, &key).is_none() {
            cb(&key, &value);
        }
    }
    let keys: Vec<_> = ft.tree.values().map(|entry| entry.key.clone()).collect();
    let mut values = Vec::new();
    for key in keys {
        let Some(value) = format_entry_get_value(ft, &key) else {
            continue;
        };
        let value = match value {
            FormatValue::String(value) => value,
            FormatValue::Time(value) => {
                CString::new(value.to_string()).expect("timestamp contains no NUL")
            }
        };
        values.push((key, value));
    }
    for (key, value) in values {
        cb(&key, &value);
    }
}
pub unsafe fn format_add(
    ft: &mut format_tree,
    key: &CStr,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let value = format_message_with(write);
    format_add_value(ft, key, value);
}

/// Insert literal text, copying the value into the format tree.
pub unsafe fn format_add_cstr(ft: &mut format_tree, key: &CStr, value: &CStr) {
    format_add_value(ft, key, value.to_owned());
}

unsafe fn format_add_value(ft: &mut format_tree, key: &CStr, value: CString) {
    format_entry_set(ft, key, FormatEntryState::Text(value));
}

pub unsafe fn format_add_time(ft: &mut format_tree, key: &CStr, time: SystemTime) {
    format_entry_set(
        ft,
        key,
        FormatEntryState::Time(crate::shared::time::unix_seconds(time)),
    );
}

/// Registers a lazy Rust callback that returns an owned cache value.
pub unsafe fn format_add_owned_cb(
    ft: &mut format_tree,
    key: &CStr,
    cb: impl FnMut(&mut format_tree) -> Option<CString> + 'static,
) {
    format_entry_set(ft, key, FormatEntryState::Lazy(Box::new(cb)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::{Duration, UNIX_EPOCH};

    unsafe fn tree() -> Box<format_tree> {
        format_create(None, None, 0, 0)
    }

    unsafe fn text_value(ft: &mut format_tree, key: &CStr) -> Option<CString> {
        match format_entry_get_value(ft, key) {
            Some(FormatValue::String(value)) => Some(value),
            _ => None,
        }
    }

    #[test]
    fn owned_callbacks_cache_values_and_absence_until_replaced() {
        unsafe {
            let mut ft_owner = tree();
            let ft = &mut *ft_owner;
            let calls = Rc::new(Cell::new(0));
            let observed = calls.clone();
            format_add_owned_cb(ft, c"owned", move |_| {
                observed.set(observed.get() + 1);
                Some(CString::new(b"cached\xff".to_vec()).unwrap())
            });
            for _ in 0..2 {
                assert_eq!(text_value(ft, c"owned").unwrap().as_bytes(), b"cached\xff");
            }
            assert_eq!(calls.get(), 1);
            let observed = calls.clone();
            format_add_owned_cb(ft, c"owned", move |_| {
                observed.set(observed.get() + 1);
                None
            });
            for _ in 0..2 {
                assert_eq!(text_value(ft, c"owned").unwrap().as_c_str(), c"");
            }
            assert_eq!(calls.get(), 2);
            format_add_cstr(ft, c"owned", c"literal");
            assert_eq!(text_value(ft, c"owned").unwrap().as_c_str(), c"literal");
            let time = UNIX_EPOCH + Duration::from_secs(123);
            format_add_time(ft, c"owned", time);
            assert!(matches!(
                format_entry_get_value(ft, c"owned"),
                Some(FormatValue::Time(123))
            ));
            assert_eq!(calls.get(), 2);
            drop(ft_owner);
        }
    }

    #[test]
    fn boxed_entries_preserve_identity_during_replacement() {
        unsafe {
            let mut ft_owner = tree();
            let ft = &mut *ft_owner;
            let key = CString::new(b"custom\xff".to_vec()).unwrap();
            let first = CString::new(b"first\xff".to_vec()).unwrap();
            format_add_cstr(ft, &key, &first);
            let address = format_entry_tree_find(&ft.tree, &key).unwrap() as *const _ as usize;
            for i in 0..128 {
                let key = CString::new(format!("extra-{i}")).unwrap();
                format_add_cstr(ft, &key, c"extra");
            }
            let previous = format_entry_tree_find(&ft.tree, &key)
                .unwrap()
                .state
                .text()
                .unwrap()
                .to_owned();
            format_add(ft, &key, |out| {
                out.write_all(previous.to_bytes())?;
                out.write_all(b"-next")
            });
            assert_eq!(text_value(ft, &key).unwrap().as_bytes(), b"first\xff-next");
            assert_eq!(
                format_entry_tree_find(&ft.tree, &key).unwrap() as *const _ as usize,
                address
            );
            let entry = ft.tree.remove(&key).unwrap();
            drop(ft_owner);
            assert_eq!(entry.key, key);
            assert_eq!(entry.state.text().unwrap().to_bytes(), b"first\xff-next");
        }
    }

    #[test]
    fn lazy_callbacks_can_replace_remove_and_reinsert_their_own_entry() {
        unsafe {
            let mut ft_owner = tree();
            let ft = &mut *ft_owner;
            format_add_owned_cb(ft, c"owned", |ft| {
                format_add_cstr(ft, c"owned", c"replacement");
                Some(c"stale".to_owned())
            });
            assert_eq!(text_value(ft, c"owned").unwrap().as_c_str(), c"replacement");

            format_add_owned_cb(ft, c"owned", |ft| {
                let removed = ft.tree.remove(c"owned");
                drop(removed);
                Some(c"stale".to_owned())
            });
            assert!(text_value(ft, c"owned").is_none());
            assert!(format_entry_tree_find(&ft.tree, c"owned").is_none());

            let replacement_calls = Rc::new(Cell::new(0));
            let observed = replacement_calls.clone();
            format_add_owned_cb(ft, c"owned", move |ft| {
                let removed = ft.tree.remove(c"owned");
                drop(removed);
                let observed = observed.clone();
                format_add_owned_cb(ft, c"owned", move |_| {
                    observed.set(observed.get() + 1);
                    Some(c"new callback".to_owned())
                });
                Some(c"stale".to_owned())
            });
            assert!(text_value(ft, c"owned").is_none());
            assert_eq!(replacement_calls.get(), 0);
            assert_eq!(
                text_value(ft, c"owned").unwrap().as_c_str(),
                c"new callback"
            );
            assert_eq!(replacement_calls.get(), 1);

            format_add_owned_cb(ft, c"owned", |ft| {
                assert!(text_value(ft, c"owned").is_none());
                for i in 0..128 {
                    let key = CString::new(format!("extra-{i}")).unwrap();
                    format_add_cstr(ft, &key, c"extra");
                }
                Some(c"recursive".to_owned())
            });
            assert_eq!(text_value(ft, c"owned").unwrap().as_c_str(), c"recursive");
            drop(ft_owner);
        }
    }

    #[test]
    fn stale_evaluation_cannot_cache_into_a_reinserted_entry_after_unwind() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        unsafe {
            let mut ft_owner = tree();
            let ft = &mut *ft_owner;
            format_add_owned_cb(ft, c"owned", |ft| {
                drop(ft.tree.remove(c"owned"));
                format_add_owned_cb(ft, c"owned", |_| panic!("replacement callback"));
                assert!(catch_unwind(AssertUnwindSafe(|| text_value(ft, c"owned"))).is_err());
                Some(c"stale".to_owned())
            });
            assert!(text_value(ft, c"owned").is_none());
            assert!(matches!(
                format_entry_tree_find(&ft.tree, c"owned").unwrap().state,
                FormatEntryState::Evaluating(_)
            ));
            format_add_cstr(ft, c"owned", c"recovered");
            assert_eq!(text_value(ft, c"owned").unwrap().as_c_str(), c"recovered");
            drop(ft_owner);
        }
    }

    #[test]
    fn expansion_resolves_the_entry_again_after_a_callback_removes_it() {
        unsafe {
            let mut ft_owner = tree();
            let ft = &mut *ft_owner;
            format_add_owned_cb(ft, c"custom_removed_entry", |ft| {
                drop(ft.tree.remove(c"custom_removed_entry"));
                Some(c"stale".to_owned())
            });
            assert_eq!(
                format_expand_cstring(ft, c"before#{custom_removed_entry}after".as_ptr())
                    .as_c_str(),
                c"beforeafter"
            );
            drop(ft_owner);
        }
    }

    struct RecordDrop(u8, Rc<RefCell<Vec<u8>>>);
    impl Drop for RecordDrop {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }

    #[test]
    fn cached_captures_drop_on_replacement_and_remaining_entries_drop_in_key_order() {
        unsafe {
            let mut ft_owner = tree();
            let ft = &mut *ft_owner;
            let drops = Rc::new(RefCell::new(Vec::new()));
            for (key, id) in [(c"beta", 2), (c"alpha", 1), (c"replace", 3)] {
                let record = RecordDrop(id, drops.clone());
                format_add_owned_cb(ft, key, move |_| {
                    let _keep_capture = &record;
                    Some(c"cached".to_owned())
                });
                assert_eq!(text_value(ft, key).unwrap().as_c_str(), c"cached");
            }
            assert!(drops.borrow().is_empty());
            format_add_cstr(ft, c"replace", c"literal");
            assert_eq!(*drops.borrow(), [3]);
            drop(ft_owner);
            assert_eq!(*drops.borrow(), [3, 1, 2]);
        }
    }

    #[test]
    fn callback_captures_drop_once_on_replacement_and_free() {
        struct CountDrop(Rc<Cell<usize>>);
        impl Drop for CountDrop {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        for owned in [false, true] {
            unsafe {
                let mut owner = if owned {
                    format_create_owned(None, None, FORMAT_NONE, 0)
                } else {
                    tree()
                };
                let ft = &mut *owner;
                let calls = Rc::new(Cell::new(0));
                for replace in [true, false] {
                    let capture = CountDrop(calls.clone());
                    format_add_owned_cb(ft, c"capture", move |_| {
                        let _keep_capture = &capture;
                        None
                    });
                    if replace {
                        format_add_cstr(ft, c"capture", c"replacement");
                        assert_eq!(calls.get(), 1);
                        assert_eq!(
                            text_value(ft, c"capture").unwrap().as_c_str(),
                            c"replacement"
                        );
                        format_add_cstr(ft, c"after-drop", c"created");
                        assert_eq!(
                            text_value(ft, c"after-drop").unwrap().as_c_str(),
                            c"created"
                        );
                    }
                }
                assert_eq!(calls.get(), 1);
                drop(owner);
                assert_eq!(calls.get(), 2);
            }
        }
    }
    #[test]
    fn merge_copies_only_materialized_values_and_overwrites_existing_entries() {
        unsafe {
            let mut source_owner = tree();
            let source = &mut *source_owner;
            let mut destination_owner = tree();
            let destination = &mut *destination_owner;
            format_add_cstr(source, c"literal", c"text");
            format_add_owned_cb(source, c"cached", |_| Some(c"value".to_owned()));
            assert_eq!(text_value(source, c"cached").unwrap().as_c_str(), c"value");
            format_add_owned_cb(source, c"lazy", |_| {
                panic!("merge must not evaluate callbacks")
            });
            let time = UNIX_EPOCH + Duration::from_secs(123);
            format_add_time(source, c"time", time);
            format_add_cstr(destination, c"literal", c"old");
            format_merge(destination, source);
            assert_eq!(
                text_value(destination, c"literal").unwrap().as_c_str(),
                c"text"
            );
            assert_eq!(
                text_value(destination, c"cached").unwrap().as_c_str(),
                c"value"
            );
            assert!(format_entry_tree_find(&destination.tree, c"lazy").is_none());
            assert!(format_entry_tree_find(&destination.tree, c"time").is_none());
            format_merge(destination, source);
            assert_eq!(
                text_value(destination, c"cached").unwrap().as_c_str(),
                c"value"
            );
            assert_eq!(destination.tree.len(), 2);
            drop(source_owner);
            drop(destination_owner);
        }
    }
}
