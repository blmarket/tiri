use crate::key_string::key_string_format;
use crate::log::{log_debug, log_hex};
use crate::server_client::ClientRegistry;

use crate::server_client::Client as _;
use crate::shared::client::ClientRef;

use crate::actions::ProgramRef;
use crate::shared::key::KEY_BINDING_REPEAT;
use crate::shared::key::*;
use crate::shared::key::{key_binding, key_table};
use std::ffi::CStr;

/// Key tables indexed by name.
type KeyTableMap =
    std::collections::BTreeMap<std::ffi::CString, std::rc::Rc<std::cell::RefCell<key_table>>>;

#[repr(C)]
pub struct key_tables {
    pub(crate) storage: KeyTableMap,
}

thread_local! {
    static KEY_TABLES: std::cell::RefCell<key_tables> = const {
        std::cell::RefCell::new(key_tables {
            storage: KeyTableMap::new(),
        })
    };
}

/// The key table index owns the tables. Each call borrows it only for its own
/// duration and returns owned `Rc`s; removed tables are dropped by the caller.
impl key_tables {
    pub fn find(name: &CStr) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
        KEY_TABLES.with_borrow(|index| index.storage.get(name).cloned())
    }

    /// An existing table with the same name wins and is returned.
    pub fn insert(
        table: std::rc::Rc<std::cell::RefCell<key_table>>,
    ) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
        let key = table.borrow().name.clone();
        KEY_TABLES.with_borrow_mut(|index| match index.storage.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => Some(entry.get().clone()),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(table);
                None
            }
        })
    }

    /// Removes and returns the indexed owner only if it is `table`.
    pub fn remove(
        table: &std::rc::Rc<std::cell::RefCell<key_table>>,
    ) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
        let key = table.borrow().name.clone();
        KEY_TABLES.with_borrow_mut(|index| {
            if !index
                .storage
                .get(&key)
                .is_some_and(|candidate| std::rc::Rc::ptr_eq(candidate, table))
            {
                return None;
            }
            index.storage.remove(&key)
        })
    }

    /// Retain the tables, in name order, while callers borrow their bindings.
    pub fn snapshot() -> Vec<std::rc::Rc<std::cell::RefCell<key_table>>> {
        KEY_TABLES.with_borrow(|index| index.storage.values().cloned().collect())
    }
}

pub fn key_bindings_set_note(bd: &mut key_binding, note: Option<&CStr>) {
    bd.note = note.map(CStr::to_owned);
}

pub fn key_bindings_get_table(
    name: &CStr,
    create: i32,
) -> Option<std::rc::Rc<std::cell::RefCell<key_table>>> {
    if let Some(table) = key_tables::find(name) {
        return Some(table);
    }
    if create == 0 {
        return None;
    }
    let mut value = key_table::empty();
    value.name = name.to_owned();
    let table = std::rc::Rc::new(std::cell::RefCell::new(value));
    key_tables::insert(table.clone());
    Some(table)
}
pub fn key_bindings_get(table: &key_table, key: key_code) -> Option<&key_binding> {
    table.key_bindings.get(key)
}

pub fn key_bindings_get_default(table: &key_table, key: key_code) -> Option<&key_binding> {
    table.default_key_bindings.get(key)
}

pub fn key_bindings_add(
    name: &CStr,
    key: key_code,
    note: Option<&CStr>,
    repeat: ::core::ffi::c_int,
    cmdlist: Option<ProgramRef>,
) {
    let owner = key_bindings_get_table(name, 1).expect("created key table");
    let mut table = owner.borrow_mut();
    let key = key & !KEYC_MASK_FLAGS;
    let Some(cmdlist) = cmdlist else {
        if let Some(bd) = table.key_bindings.get_mut(key) {
            if let Some(note) = note {
                key_bindings_set_note(bd, Some(note));
            }
            if repeat != 0 {
                bd.flags |= KEY_BINDING_REPEAT;
            }
        }
        return;
    };
    drop(table.key_bindings.remove(key));
    let bd = Box::new(key_binding {
        key,
        commands: cmdlist,
        note: note.map(CStr::to_owned),
        tablename: Some(table.name.clone()),
        flags: if repeat != 0 { KEY_BINDING_REPEAT } else { 0 },
    });
    let s = (bd.cmdlist()).text(0);
    let key_string = key_string_format(key, true);
    table.key_bindings.insert(bd);
    log_debug(format_args!(
        "key_bindings_add: {} {} = {}",
        log_hex(key),
        crate::log::log_bytes(key_string.as_bytes()),
        crate::log::log_bytes(s.as_bytes())
    ));
}

pub fn key_bindings_remove(name: &CStr, key: key_code) {
    let Some(owner) = key_bindings_get_table(name, 0) else {
        return;
    };
    let mut table = owner.borrow_mut();
    let Some(bd) = table.key_bindings.remove(key & !KEYC_MASK_FLAGS) else {
        return;
    };
    let key_string = key_string_format(bd.key, true);
    log_debug(format_args!(
        "key_bindings_remove: {} {}",
        log_hex(bd.key),
        crate::log::log_bytes(key_string.as_bytes())
    ));
    if table.key_bindings.storage.is_empty() && table.default_key_bindings.storage.is_empty() {
        drop(table);
        drop(key_tables::remove(&owner));
    }
}

pub fn key_bindings_reset(name: &CStr, key: key_code) {
    let Some(owner) = key_bindings_get_table(name, 0) else {
        return;
    };
    let mut table = owner.borrow_mut();
    let key = key & !KEYC_MASK_FLAGS;
    let table_ref = &mut *table;
    let Some(bd) = table_ref.key_bindings.get_mut(key) else {
        return;
    };
    let Some(dd) = table_ref.default_key_bindings.get(key) else {
        drop(table);
        key_bindings_remove(name, key);
        return;
    };
    bd.commands = std::rc::Rc::clone(&dd.commands);
    bd.note = dd.note.clone();
    bd.flags = dd.flags;
}

pub fn key_bindings_remove_table(name: &CStr) {
    let mut c: Option<ClientRef> = None;
    if let Some(owner) = key_bindings_get_table(name, 0) {
        let detached = key_tables::remove(&owner);
        let mut registry_c_owner = ClientRegistry::first();
        c = registry_c_owner.clone();
        while !c.is_none() {
            if c.as_ref().expect("live client").uses_key_table(&owner) {
                c.clone().expect("live client").set_key_table(None);
            }
            registry_c_owner =
                ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
            c = registry_c_owner.clone();
        }
        drop(detached);
    }
}

/// Snapshot startup defaults with their own command-list reference and note.
pub fn key_bindings_has_repeat(bindings: &[&key_binding]) -> bool {
    bindings.iter().any(|bd| bd.flags & KEY_BINDING_REPEAT != 0)
}
