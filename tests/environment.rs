//! Environment-owner compatibility and lifetime checks.
use hmux::src::environ::{environ, environ_create, ENVIRON_HIDDEN};
use hmux::src::format::bytes::write_cstr;
use std::ffi::CStr;

#[test]
fn owner_preserves_missing_valueless_flags_order_and_bytes() {
    let mut env = environ_create();
    env.set(b"b", 0, b"value").unwrap();
    env.set(b"a\xff", ENVIRON_HIDDEN, b"\xfe\xfd").unwrap();
    env.set(b"a", 0, b"ascii").unwrap();
    env.clear(b"empty").unwrap();
    env.set(b"\xff", 0x40, b"high-byte").unwrap();

    assert!(env.find_bytes(b"missing").unwrap().is_none());

    let empty = env.find_bytes(b"empty").unwrap().unwrap();
    assert_eq!(empty.name_bytes(), b"empty");
    assert_eq!(empty.value_bytes(), None);
    assert_eq!(empty.flags(), 0);

    let hidden = env.find_bytes(b"a\xff").unwrap().unwrap();
    assert_eq!(hidden.value_bytes(), Some(&b"\xfe\xfd"[..]));
    assert_eq!(hidden.flags(), ENVIRON_HIDDEN);

    let names: Vec<Vec<u8>> = env
        .entries()
        .map(|entry| entry.name_bytes().to_vec())
        .collect();
    assert_eq!(
        names,
        vec![
            b"a".to_vec(),
            b"a\xff".to_vec(),
            b"b".to_vec(),
            b"empty".to_vec(),
            b"\xff".to_vec()
        ]
    );

    assert!(env.set(b"bad\0name", 0, b"value").is_err());
    assert!(env.find_bytes(b"bad\0name").is_err());
}

#[test]
fn borrowed_copy_and_entry_views_follow_their_owner() {
    let mut source = environ_create();
    source.set(b"copied", 7, b"bytes\xff").unwrap();
    let mut destination = environ_create();
    destination.copy_from(&source);

    let entry = destination.find_bytes(b"copied").unwrap().unwrap();
    assert_eq!(entry.value_bytes(), Some(&b"bytes\xff"[..]));
    assert_eq!(entry.flags(), 7);
    assert_eq!(entry.name(), c"copied");
}

#[test]
fn moving_an_owner_keeps_the_box_and_its_entries_alive() {
    assert!(std::mem::needs_drop::<Box<environ>>());
    assert!(std::mem::needs_drop::<environ>());
    let mut original = environ_create();
    original.set(b"transferred", 0, b"owned").unwrap();
    let address = std::ptr::from_ref(&*original);
    let entry = std::ptr::from_ref(original.find(c"transferred").unwrap());
    let mut slot = Some(original);
    let adopted = slot.take().unwrap();
    assert!(slot.is_none());
    assert_eq!(std::ptr::from_ref(&*adopted), address);
    assert_eq!(
        std::ptr::from_ref(adopted.find(c"transferred").unwrap()),
        entry
    );
    assert_eq!(
        adopted.find(c"transferred").unwrap().value(),
        Some(c"owned")
    );
}

#[test]
fn updates_preserve_entry_addresses_and_iteration_supports_removal_by_snapshot() {
    let mut env = environ_create();
    env.set(b"middle", 7, b"old").unwrap();
    let middle = std::ptr::from_ref(env.find_bytes(b"middle").unwrap().unwrap());
    // Grow the index in reverse order to exercise map rebalancing.
    for i in (0..128).rev() {
        env.set(format!("key-{i:03}").as_bytes(), 0, b"value")
            .unwrap();
    }
    env.set(b"middle", ENVIRON_HIDDEN, b"new").unwrap();
    assert_eq!(
        std::ptr::from_ref(env.find_bytes(b"middle").unwrap().unwrap()),
        middle
    );
    env.clear(b"middle").unwrap();
    let cleared = env.find_bytes(b"middle").unwrap().unwrap();
    assert_eq!(std::ptr::from_ref(cleared), middle);
    assert_eq!(cleared.value_bytes(), None);
    assert_eq!(cleared.flags(), ENVIRON_HIDDEN);

    let names: Vec<_> = env.entries().map(|entry| entry.name().to_owned()).collect();
    assert_eq!(names.len(), 129);
    for name in names {
        env.unset_cstr(&name);
    }
    assert!(env.entries().next().is_none());
    env.set(b"reinserted", 0, b"ok").unwrap();
    assert_eq!(env.entries().count(), 1);
}

#[test]
fn formatted_updates_use_an_owned_snapshot_of_the_previous_value() {
    use hmux::src::environ::environ_set;
    let mut env = environ_create();
    env.set(b"VAR", ENVIRON_HIDDEN, b"old\xff").unwrap();
    let entry = std::ptr::from_ref(env.find(c"VAR").unwrap());
    let previous = env.find(c"VAR").unwrap().value.clone().unwrap();
    unsafe {
        environ_set(&mut env, c"VAR", 0x40, |out| {
            write_cstr(out, &*previous)?;
            out.write_all(b"-new")
        });
    }
    assert_eq!(std::ptr::from_ref(env.find(c"VAR").unwrap()), entry);
    assert_eq!(
        env.find(c"VAR").unwrap().value_bytes(),
        Some(b"old\xff-new".as_slice())
    );
    assert_eq!(env.find(c"VAR").unwrap().flags(), 0x40);
    env.clear(b"VAR").unwrap();
    let cleared = env.find(c"VAR").unwrap();
    assert_eq!(std::ptr::from_ref(cleared), entry);
    assert_eq!(cleared.value(), None);
    assert_eq!(cleared.flags(), 0x40);
    env.set(b"VAR", 0, b"").unwrap();
    assert_eq!(env.find(c"VAR").unwrap().value(), Some(c""));
}

#[test]
fn put_splits_first_equals_and_preserves_c_string_bytes() {
    use hmux::src::environ::environ_put;

    let mut env = environ_create();
    unsafe {
        environ_put(&mut env, c"plain=one=two", 7);
        environ_put(&mut env, c"\xff=\xfe", 8);
        environ_put(&mut env, c"=empty-name", 9);
        environ_put(&mut env, c"no-equals", 10);
        environ_put(
            &mut env,
            CStr::from_bytes_until_nul(b"first=visible\0later=hidden\0").unwrap(),
            11,
        );
    }
    assert_eq!(
        env.find_bytes(b"plain").unwrap().unwrap().value_bytes(),
        Some(&b"one=two"[..])
    );
    assert_eq!(
        env.find_bytes(b"\xff").unwrap().unwrap().value_bytes(),
        Some(&b"\xfe"[..])
    );
    assert_eq!(
        env.find_bytes(b"").unwrap().unwrap().value_bytes(),
        Some(&b"empty-name"[..])
    );
    assert!(env.find_bytes(b"no-equals").unwrap().is_none());
    assert_eq!(
        env.find_bytes(b"first").unwrap().unwrap().value_bytes(),
        Some(&b"visible"[..])
    );
    assert!(env.find_bytes(b"later").unwrap().is_none());
}

#[test]
fn iteration_allows_nested_reads_and_copy_from_an_owned_snapshot() {
    use hmux::src::environ::environ_copy;
    let mut env = environ_create();
    env.set(b"a", 7, b"one").unwrap();
    env.clear(b"b").unwrap();
    let snapshot = env.clone();
    environ_copy(&snapshot, &mut env);
    for entry in env.entries() {
        assert_eq!(env.find(entry.name()).unwrap().value(), entry.value());
        assert_eq!(env.entries().count(), 2);
    }
    assert_eq!(env.find_bytes(b"a").unwrap().unwrap().flags(), 7);
    assert_eq!(env.find_bytes(b"b").unwrap().unwrap().value(), None);
}

#[test]
fn update_borrows_sources_and_accepts_owned_snapshots() {
    use hmux::src::environ::environ_update;
    use hmux::src::options::{options_array_set, options_create, options_empty};
    use std::ptr::null_mut;
    unsafe {
        let mut options_owner = options_create(None);
        let options = &raw mut *options_owner;
        let table = &raw const hmux::src::options_table::options_table;
        let definition = (*table)
            .iter()
            .find(|entry| entry.name == Some(c"update-environment"))
            .unwrap();
        let array = options_empty(options, definition);
        for (index, pattern) in [(c"0", c"a*"), (c"1", c"missing")] {
            assert_eq!(
                options_array_set(array, index, Some(pattern), 0, null_mut()),
                0
            );
        }
        let mut source = environ_create();
        source.set(b"alpha", ENVIRON_HIDDEN, b"value").unwrap();
        let mut destination = environ_create();
        destination.set(b"missing", 0, b"old").unwrap();
        environ_update(options, &source, &mut destination);
        assert_eq!(
            destination
                .find_bytes(b"alpha")
                .unwrap()
                .unwrap()
                .value_bytes(),
            Some(b"value".as_slice())
        );
        assert_eq!(
            destination.find_bytes(b"missing").unwrap().unwrap().value(),
            None
        );
        let snapshot = source.clone();
        environ_update(options, &snapshot, &mut source);
        assert_eq!(source.find_bytes(b"alpha").unwrap().unwrap().flags(), 0);
        assert_eq!(
            source.find_bytes(b"alpha").unwrap().unwrap().value_bytes(),
            Some(b"value".as_slice())
        );
        assert_eq!(
            source.find_bytes(b"missing").unwrap().unwrap().value(),
            None
        );
        drop(options_owner);
    }
}

#[test]
fn copying_cleared_entries_preserves_existing_destination_flags() {
    let mut source = environ_create();
    source.set(b"kept", ENVIRON_HIDDEN, b"hidden").unwrap();
    source.clear(b"kept").unwrap();
    source.clear(b"new").unwrap();
    let mut destination = environ_create();
    destination.set(b"kept", 0x40, b"old").unwrap();
    let address = std::ptr::from_ref(destination.find(c"kept").unwrap());
    destination.copy_from(&source);
    assert_eq!(
        std::ptr::from_ref(destination.find(c"kept").unwrap()),
        address
    );
    assert_eq!(destination.find(c"kept").unwrap().flags(), 0x40);
    assert_eq!(destination.find(c"kept").unwrap().value(), None);
    assert_eq!(destination.find(c"new").unwrap().flags(), 0);
}

#[test]
fn entry_allocation_survives_updates_and_clears() {
    let mut env = environ_create();
    env.set_cstr(c"NAME", 0, c"first");
    let snapshot = env.find(c"NAME").unwrap().clone();
    let id = env.find(c"NAME").unwrap().id();
    assert!(id > 0 && id < (1 << 61));

    env.set_cstr(c"NAME", ENVIRON_HIDDEN, c"second");
    assert_eq!(env.find(c"NAME").unwrap().id(), id);
    env.clear_cstr(c"NAME");
    assert_eq!(env.find(c"NAME").unwrap().id(), id);
    assert_eq!(env.find(c"NAME").unwrap().flags(), ENVIRON_HIDDEN);
    env.set_cstr(c"NAME", 0, c"restored");
    assert_eq!(env.find(c"NAME").unwrap().id(), id);

    env.unset_cstr(c"NAME");
    env.clear_cstr(c"NAME");
    let recreated = env.find(c"NAME").unwrap().id();
    env.set_cstr(c"NAME", 0, c"replacement");
    assert_eq!(env.find(c"NAME").unwrap().id(), recreated);
    assert_eq!(snapshot.value(), Some(c"first"));
}

#[test]
fn independent_environment_clones_get_new_entry_ids_and_preserve_cleared_flags() {
    let mut original = environ_create();
    original.set_cstr(c"set", 0x40, c"value");
    original.set_cstr(c"cleared", ENVIRON_HIDDEN, c"old");
    original.clear_cstr(c"cleared");
    let cloned = original.clone();

    for entry in original.entries() {
        let copy = cloned.find(entry.name()).unwrap();
        assert_ne!(copy.id(), entry.id(), "a cloned owner has new records");
        assert_eq!(copy.value(), entry.value());
        assert_eq!(copy.flags(), entry.flags());
    }
    assert_eq!(cloned.find(c"cleared").unwrap().flags(), ENVIRON_HIDDEN);
    assert_eq!(cloned.find(c"cleared").unwrap().value(), None);
}

#[test]
fn environment_copy_preserves_destination_identity_without_importing_source_ids() {
    let mut source = environ_create();
    source.set_cstr(c"existing", 7, c"new value");
    source.set_cstr(c"added", 0, c"new record");
    source.clear_cstr(c"cleared");
    let mut destination = environ_create();
    destination.set_cstr(c"existing", ENVIRON_HIDDEN, c"old");
    let retained_id = destination.find(c"existing").unwrap().id();

    destination.copy_from(&source);
    assert_eq!(destination.find(c"existing").unwrap().id(), retained_id);
    for entry in source.entries() {
        assert_ne!(destination.find(entry.name()).unwrap().id(), entry.id());
    }
    let ids: Vec<_> = destination.entries().map(|entry| entry.id()).collect();
    destination.copy_from(&source);
    assert_eq!(
        destination
            .entries()
            .map(|entry| entry.id())
            .collect::<Vec<_>>(),
        ids,
        "copying values again does not replace destination records"
    );
}
