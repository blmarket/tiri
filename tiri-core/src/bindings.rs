use crate::actions::{Context, ProgramRef, REPEAT_STATE, Target};
use crate::reactor::Lane;
use crate::key_string::key_string_format;
use crate::log::{fatalx, log_cstr, log_debug, log_hex};
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::shared::abi::uint64_t;
use crate::shared::client::{CLIENT_READONLY, ClientRef};
use crate::shared::key::{
    KEY_BINDING_REPEAT, KEYC_MASK_FLAGS, KeyBindingCommand, key_binding, key_code, key_event,
    key_table,
};
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

static DEFAULT_BINDINGS: std::sync::OnceLock<&'static [&'static CStr]> =
    std::sync::OnceLock::new();

pub fn set_default_bindings(defaults: &'static [&'static CStr]) {
    let registered = DEFAULT_BINDINGS.get_or_init(|| defaults);
    assert!(
        std::ptr::eq(*registered, defaults),
        "default bindings already registered"
    );
}

fn key_bindings_init_done() {
    for owner in key_tables::snapshot() {
        let mut table = owner.borrow_mut();
        let table_ref = &mut *table;
        for bd in table_ref.key_bindings.iter() {
            table_ref.default_key_bindings.insert(Box::new(key_binding {
                key: bd.key,
                commands: std::rc::Rc::clone(&bd.commands),
                note: bd.note.clone(),
                tablename: None,
                flags: bd.flags,
            }));
        }
    }
}

pub fn key_bindings_init() {
    let defaults = DEFAULT_BINDINGS.get().copied().unwrap_or(&[]);
    for command in defaults {
        let commands = crate::actions::sources::compile(command).unwrap_or_else(|error| {
            log_debug(format_args!("{}", log_cstr(&error)));
            fatalx(|out| {
                out.write_all(b"bad default key: ")?;
                out.write_all(command.to_bytes())
            });
        });
        Lane::push(None, async move {
            let _ = crate::actions::run(
                commands,
                Context::new(std::rc::Weak::new(), None, None, 0),
            )
            .await;
        });
    }
    Lane::push(None, async {
        key_bindings_init_done();
    });
}

fn key_bindings_read_only(client: crate::shared::client::ClientWeak) {
    ClientRef::report_error(client.upgrade().as_ref(), None, |out| {
        out.write_all(b"client is read-only")
    });
}

pub fn key_bindings_dispatch(
    bd: KeyBindingCommand,
    c_owner: Option<&ClientRef>,
    exec_client: crate::shared::client::ClientWeak,
    event: Option<&key_event>,
    fs: &Target,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
    let readonly = if c_owner.is_none_or(|c| !c.flags() & CLIENT_READONLY as uint64_t != 0) {
        true
    } else {
        bd.cmdlist().read_only()
    };
    if !readonly {
        let client = c_owner.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        Box::pin(async move {
            key_bindings_read_only(client);
        })
    } else {
        let mut flags = 0;
        if bd.flags & KEY_BINDING_REPEAT != 0 {
            flags |= REPEAT_STATE;
        }
        let context = Context::new(exec_client, Some(fs), event, flags);
        Box::pin(async move {
            let _ = crate::actions::run(bd.commands, context).await;
        })
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::{
        KEY_BINDING_REPEAT, ProgramRef, key_binding, key_bindings_add, key_bindings_get,
        key_bindings_get_table, key_bindings_init_done, key_bindings_remove_table,
        key_bindings_reset, key_code, key_table, key_tables,
    };
    use crate::shared::key::key_bindings;
    use std::ffi::CString;
    use std::rc::Rc;

    struct TestProgram;
    impl crate::actions::Program for TestProgram {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn text(&self, _: i32) -> CString {
            c"".to_owned()
        }
        fn substitute(&self, _: &Vec<CString>) -> ProgramRef {
            Rc::new(Self)
        }
        fn read_only(&self) -> bool {
            true
        }
        fn starts_server(&self) -> bool {
            false
        }
    }

    fn binding(key: key_code) -> Box<key_binding> {
        Box::new(key_binding {
            key,
            commands: Rc::new(TestProgram),
            note: None,
            tablename: None,
            flags: 0,
        })
    }

    #[test]
    fn table_index_returns_owners_and_removes_only_matching_allocations() {
        use std::cell::RefCell;
        let mut a = key_table::empty();
        a.name = c"a".to_owned();
        let a = Rc::new(RefCell::new(a));
        let mut b = key_table::empty();
        b.name = c"b".to_owned();
        let b = Rc::new(RefCell::new(b));
        let mut duplicate = key_table::empty();
        duplicate.name = c"a".to_owned();
        let duplicate = Rc::new(RefCell::new(duplicate));
        assert!(!Rc::ptr_eq(&a, &duplicate));
        assert!(key_tables::insert(a.clone()).is_none());
        assert!(key_tables::insert(b.clone()).is_none());
        assert!(Rc::ptr_eq(
            &key_tables::insert(duplicate.clone()).unwrap(),
            &a
        ));
        assert!(key_tables::remove(&duplicate).is_none());
        assert!(Rc::ptr_eq(&key_tables::snapshot()[0], &a));
        let observed = Rc::downgrade(&a);
        let found = key_tables::find(c"a").unwrap();
        drop(key_tables::remove(&a));
        drop(a);
        assert!(observed.upgrade().is_some());
        drop(found);
        assert!(observed.upgrade().is_none());
        drop(key_tables::remove(&b));
        assert!(key_tables::snapshot().is_empty());
    }

    #[test]
    fn index_owns_bindings_and_keeps_addresses_stable() {
        let mut index = key_bindings::default();
        let first = binding(8);
        let address = std::ptr::from_ref(&*first);
        let commands = Rc::downgrade(&first.commands);
        index.insert(first);
        index.insert(binding(2));
        index.insert(binding(13));
        assert_eq!(
            index.iter().map(|bd| bd.key).collect::<Vec<_>>(),
            [2, 8, 13]
        );
        assert_eq!(std::ptr::from_ref(index.get(8).unwrap()), address);
        let detached = index.remove(8).unwrap();
        assert_eq!(std::ptr::from_ref(&*detached), address);
        assert!(index.get(8).is_none());
        assert!(commands.upgrade().is_some());
        drop(detached);
        assert!(commands.upgrade().is_none());
        drop(index.remove(2));
        drop(index.remove(13));
        assert!(key_tables::snapshot().is_empty());
    }

    #[test]
    fn defaults_restore_notes_flags_and_command_ownership() {
        let name = c"binding-owner-defaults";
        let original: ProgramRef = Rc::new(TestProgram);
        let original_lifetime = Rc::downgrade(&original);
        key_bindings_add(name, 65, Some(c"original"), 1, Some(original));
        let table_owner = key_bindings_get_table(name, 0).unwrap();
        key_bindings_init_done();
        assert_eq!(original_lifetime.strong_count(), 2);
        let replacement: ProgramRef = Rc::new(TestProgram);
        let replacement_lifetime = Rc::downgrade(&replacement);
        key_bindings_add(name, 65, Some(c"replacement"), 0, Some(replacement));
        assert_eq!(original_lifetime.strong_count(), 1);
        key_bindings_reset(name, 65);
        assert!(replacement_lifetime.upgrade().is_none());
        let table_borrow = table_owner.borrow();
        let bd = key_bindings_get(&table_borrow, 65).unwrap();
        assert_eq!(bd.note.as_deref(), Some(c"original"));
        assert_eq!(bd.flags, KEY_BINDING_REPEAT);
        assert!(std::rc::Weak::ptr_eq(
            &Rc::downgrade(bd.cmdlist()),
            &original_lifetime
        ));
        assert_eq!(original_lifetime.strong_count(), 2);
        drop(table_borrow);
        key_bindings_add(name, 65, Some(c"note only"), 0, None);
        assert!(std::rc::Weak::ptr_eq(
            &Rc::downgrade(
                key_bindings_get(&table_owner.borrow(), 65)
                    .unwrap()
                    .cmdlist()
            ),
            &original_lifetime
        ));
        assert_eq!(original_lifetime.strong_count(), 2);
        key_bindings_add(name, 66, None, 0, Some(Rc::new(TestProgram)));
        key_bindings_reset(name, 66);
        assert!(key_bindings_get(&table_owner.borrow(), 66).is_none());
        drop(table_owner);
        key_bindings_remove_table(name);
        assert!(original_lifetime.upgrade().is_none());
    }

    #[test]
    fn dispatch_snapshot_survives_binding_replacement_and_table_removal() {
        let name = c"binding-owner-dispatch";
        let original: ProgramRef = Rc::new(TestProgram);
        let original_lifetime = Rc::downgrade(&original);
        key_bindings_add(name, 65, None, 1, Some(original));
        let table_owner = key_bindings_get_table(name, 0).unwrap();
        let table_lifetime = Rc::downgrade(&table_owner);
        let retained_table = table_lifetime.upgrade().unwrap();
        let command = key_bindings_get(&table_owner.borrow(), 65)
            .unwrap()
            .command();
        key_bindings_add(name, 65, None, 0, Some(Rc::new(TestProgram)));
        assert!(std::rc::Weak::ptr_eq(
            &Rc::downgrade(command.cmdlist()),
            &original_lifetime
        ));
        assert_eq!(command.key, 65);
        assert_eq!(command.flags, KEY_BINDING_REPEAT);
        assert_eq!(original_lifetime.strong_count(), 1);
        drop(table_owner);
        key_bindings_remove_table(name);
        assert!(key_bindings_get_table(name, 0).is_none());
        assert!(table_lifetime.upgrade().is_some());
        drop(retained_table);
        assert!(table_lifetime.upgrade().is_none());
        assert!(original_lifetime.upgrade().is_some());
        drop(command);
        assert!(original_lifetime.upgrade().is_none());
    }
}
