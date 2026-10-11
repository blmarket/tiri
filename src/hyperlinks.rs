use crate::src::shared::abi::u_int;
use crate::src::shared::hyperlinks::{hyperlinks, hyperlinks_uri, HyperlinkKey};
use crate::src::shared::vis::{VIS_CSTYLE, VIS_OCTAL};
use crate::src::text::utf8::utf8_stravis_cstring;
use std::{
    cell::{Ref, RefCell},
    collections::VecDeque,
    ffi::{CStr, CString},
    rc::{Rc, Weak},
};

/// A shared owner of a heap-allocated hyperlink table.
#[derive(Clone)]
pub struct HyperlinksRef(Rc<RefCell<hyperlinks>>);

impl HyperlinksRef {
    pub fn new() -> Self {
        hyperlinks_init()
    }

    pub fn len(&self) -> usize {
        self.0.borrow().by_inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for HyperlinksRef {
    fn default() -> Self {
        Self::new()
    }
}

pub const MAX_HYPERLINKS: i32 = 5000;
pub const MAX_HYPERLINK_URI: i32 = 1024;

struct QueuedHyperlink {
    table: Weak<RefCell<hyperlinks>>,
    inner: u_int,
}

struct HyperlinkHistory {
    next_external_id: i64,
    entries: VecDeque<QueuedHyperlink>,
}

thread_local! {
    // Hyperlink operations run on the server event loop, as does style parsing.
    static HYPERLINK_HISTORY: RefCell<HyperlinkHistory> = const {
        RefCell::new(HyperlinkHistory {
            next_external_id: 1,
            entries: VecDeque::new(),
        })
    };
}

fn hyperlink_key(internal_id: &CStr, uri: &CStr, inner: u_int) -> HyperlinkKey {
    if internal_id.is_empty() {
        (true, Vec::new(), Vec::new(), inner)
    } else {
        (
            false,
            internal_id.to_bytes().to_vec(),
            uri.to_bytes().to_vec(),
            0,
        )
    }
}

fn hyperlinks_remove(entry: QueuedHyperlink) {
    let Some(owner) = entry.table.upgrade() else {
        return;
    };
    let mut table = owner.borrow_mut();
    let node = table
        .by_inner
        .remove(&entry.inner)
        .expect("queued hyperlink is indexed");
    let key = hyperlink_key(&node.internal_id, &node.uri, node.inner);
    table.by_uri.remove(&key);
}

pub fn hyperlinks_put(hl: &HyperlinksRef, uri_in: &CStr, internal_id_in: Option<&CStr>) -> u_int {
    let uri = utf8_stravis_cstring(uri_in, VIS_OCTAL | VIS_CSTYLE);
    if uri.as_bytes().len() > MAX_HYPERLINK_URI as usize {
        return 0;
    }
    let internal_id = utf8_stravis_cstring(internal_id_in.unwrap_or(c""), VIS_OCTAL | VIS_CSTYLE);
    if !internal_id.is_empty() {
        let key = hyperlink_key(&internal_id, &uri, 0);
        if let Some(&inner) = hl.0.borrow().by_uri.get(&key) {
            return inner;
        }
    }
    let external = HYPERLINK_HISTORY.with(|history| {
        let mut history = history.borrow_mut();
        let id = history.next_external_id;
        history.next_external_id += 1;
        id
    });
    let inner = {
        let mut table = hl.0.borrow_mut();
        let inner = table.next_inner;
        table.next_inner = table.next_inner.wrapping_add(1);
        let key = hyperlink_key(&internal_id, &uri, inner);
        let node = Box::new(hyperlinks_uri {
            inner,
            internal_id,
            external_id: CString::new(format!("tmux{:X}", external as u64))
                .expect("generated hyperlink ID contains no NUL"),
            uri,
        });
        table.by_uri.insert(key, inner);
        table.by_inner.insert(inner, node);
        inner
    };
    let oldest = HYPERLINK_HISTORY.with(|history| {
        let mut history = history.borrow_mut();
        history.entries.push_back(QueuedHyperlink {
            table: Rc::downgrade(&hl.0),
            inner,
        });
        // tmux evicts when the count reaches the limit, leaving 4999 records.
        if history.entries.len() == MAX_HYPERLINKS as usize {
            history.entries.pop_front()
        } else {
            None
        }
    });
    if let Some(oldest) = oldest {
        hyperlinks_remove(oldest);
    }
    inner
}

/// Borrow an entry from its owning table. End the borrow before mutating any
/// table: insertion can evict entries from another table through the global FIFO.
pub fn hyperlinks_get(hl: &HyperlinksRef, inner: u_int) -> Option<Ref<'_, hyperlinks_uri>> {
    Ref::filter_map(hl.0.borrow(), |table| {
        table.by_inner.get(&inner).map(Box::as_ref)
    })
    .ok()
}

pub fn hyperlinks_init() -> HyperlinksRef {
    let mut value = hyperlinks::empty();
    value.next_inner = 1;
    HyperlinksRef(Rc::new(RefCell::new(value)))
}

pub fn hyperlinks_copy(hl: &HyperlinksRef) -> HyperlinksRef {
    hl.clone()
}

pub fn hyperlinks_reset(hl: &HyperlinksRef) {
    let table = Rc::downgrade(&hl.0);
    HYPERLINK_HISTORY.with(|history| {
        history
            .borrow_mut()
            .entries
            .retain(|entry| !entry.table.ptr_eq(&table));
    });
    let mut table = hl.0.borrow_mut();
    table.by_inner.clear();
    table.by_uri.clear();
}

impl Drop for hyperlinks {
    fn drop(&mut self) {
        if !self.by_inner.is_empty() {
            // The last strong table reference is already gone. Do not access
            // the history after its thread-local destructor has run.
            let _ = HYPERLINK_HISTORY.try_with(|history| {
                history
                    .borrow_mut()
                    .entries
                    .retain(|entry| entry.table.strong_count() != 0);
            });
        }
    }
}

#[cfg(test)]
mod hyperlink_owner_tests {
    use super::*;

    #[test]
    fn dropping_last_table_owner_removes_its_history() {
        let table = hyperlinks_init();
        let weak = Rc::downgrade(&table.0);
        let id = hyperlinks_put(&table, c"https://example.test/a", Some(c"alpha"));
        let node = hyperlinks_get(&table, id).unwrap().clone();
        let shared = table.clone();
        drop(table);
        assert!(weak.upgrade().is_some());
        drop(shared);
        assert!(weak.upgrade().is_none());
        assert_eq!(node.uri.as_c_str(), c"https://example.test/a");
        HYPERLINK_HISTORY.with(|history| assert!(history.borrow().entries.is_empty()));
    }
}
