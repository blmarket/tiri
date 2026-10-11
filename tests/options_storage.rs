//! Behavior of the Rust-owned option-name index through the existing API.
use hmux::src::format::bytes::write_cstr;
use hmux::src::options::*;
use std::ffi::{CStr, CString};
use std::ptr::null_mut;

#[test]
fn ordered_names_survive_updates_and_removal() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        assert!(options_iter(&*oo).next().is_none());
        let mut names: Vec<Vec<u8>> = (0..128)
            .rev()
            .map(|i| format!("@key-{i:03}").into_bytes())
            .collect();
        names.extend([b"@\xff".to_vec(), b"@\x80".to_vec(), b"@".to_vec()]);
        let mut pointers = Vec::new();
        for name in &names {
            let name = CString::new(name.as_slice()).unwrap();
            let entry = options_set_string(oo, &name, 0, |out| out.write_all(b"value"));
            pointers.push(entry);
        }
        for (name, &entry) in names.iter().zip(&pointers) {
            let name = CString::new(name.as_slice()).unwrap();
            assert_eq!(
                hmux::src::options::options_get_only_mut(
                    &mut *(oo),
                    std::ffi::CStr::from_ptr(name.as_ptr())
                )
                .map_or(std::ptr::null_mut(), |entry| entry),
                entry
            );
            assert_eq!(
                options_read_entry(&*oo, &name, |found| found.id()),
                Some((*entry).id())
            );
            assert_eq!(
                options_set_string(oo, &name, 0, |out| { out.write_all(b"updated") }),
                entry
            );
        }
        names.sort();
        let keys: Vec<_> = options_iter(&*oo).map(|entry| entry.name.clone()).collect();
        for (name, key) in names.iter().zip(keys) {
            let entry = std::ptr::from_mut(options_get_only_mut(&mut *oo, &key).unwrap());
            assert!(!entry.is_null());
            assert_eq!(
                CStr::from_ptr(options_name(&*(entry)).as_ptr()).to_bytes(),
                name
            );
            assert_eq!(
                CStr::from_ptr(
                    (*entry)
                        .value
                        .string_ptr()
                        .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
                ),
                c"updated"
            );
            assert_eq!(options_remove_or_default(entry, None, null_mut()), 0);
        }
        assert!(options_iter(&*oo).next().is_none());
        assert!(hmux::src::options::options_get_only_mut(
            &mut *(oo),
            std::ffi::CStr::from_ptr(c"@key-000".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |entry| entry)
        .is_null());
        // Repopulate so destruction also exercises nonempty storage.
        options_set_string(oo, c"@again", 0, |out| out.write_all(b"value"));
        drop(oo_owner);
    }
}

#[test]
fn aliases_parent_fallback_and_shadowing() {
    unsafe {
        let mut parent_owner = options_create(None);
        let parent = &raw mut *parent_owner;
        let saved_global = std::mem::replace(&mut hmux::src::tmux::global_options, parent);
        let mut child_owner = options_create(Some(OptionsScope::GlobalServer));
        let child = &raw mut *child_owner;
        let inherited = options_set_string(parent, c"@shared", 0, |out| out.write_all(b"parent"));
        assert!(hmux::src::options::options_get_only_mut(
            &mut *(child),
            std::ffi::CStr::from_ptr(c"@shared".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |entry| entry)
        .is_null());
        assert_eq!(
            options_read_entry(&*child, c"@shared", |entry| entry.id()),
            Some((*inherited).id())
        );
        let local = options_set_string(child, c"@shared", 0, |out| out.write_all(b"child"));
        assert_ne!(local, inherited);
        assert_eq!(
            options_read_entry(&*child, c"@shared", |entry| entry.id()),
            Some((*local).id())
        );
        options_remove_or_default(local, None, null_mut());
        assert_eq!(
            options_read_entry(&*child, c"@shared", |entry| entry.id()),
            Some((*inherited).id())
        );

        // Use an actual option-table entry so alias lookup and value cleanup
        // follow the same path as built-in options.
        let table = &raw const hmux::src::options_table::options_table;
        let definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"display-panes-colour"))
            .unwrap();
        let canonical = options_default(parent, definition);
        assert_eq!(
            hmux::src::options::options_get_only_mut(
                &mut *(parent),
                std::ffi::CStr::from_ptr(c"display-panes-color".as_ptr())
            )
            .map_or(std::ptr::null_mut(), |entry| entry),
            canonical
        );
        assert_eq!(
            options_read_entry(&*child, c"display-panes-color", |entry| entry.id()),
            Some((*canonical).id())
        );
        let numeric_definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"pane-border-lines"))
            .unwrap();
        options_default(parent, numeric_definition);
        options_set_number(parent, c"pane-border-lines", 2);
        assert_eq!(options_get_number_ref(&*child, c"pane-border-lines"), 2);
        let numeric = options_set_number(child, c"pane-border-lines", 1);
        assert_eq!(options_get_number_ref(&*child, c"pane-border-lines"), 1);
        assert_eq!(options_get_number_ref(&*parent, c"pane-border-lines"), 2);
        options_remove_or_default(numeric, None, null_mut());
        assert_eq!(options_get_number_ref(&*child, c"pane-border-lines"), 2);
        options_set_string(child, c"display-panes-color", 0, |out| {
            out.write_all(b"red")
        });
        assert_eq!(
            options_get_string(child, c"display-panes-colour").as_c_str(),
            c"red"
        );
        drop(child_owner);
        assert_eq!(
            hmux::src::options::options_get_only_mut(
                &mut *(parent),
                std::ffi::CStr::from_ptr(c"@shared".as_ptr())
            )
            .map_or(std::ptr::null_mut(), |entry| entry),
            inherited
        );
        hmux::src::tmux::global_options = saved_global;
        drop(parent_owner);
    }
}

#[test]
fn scalar_string_replacement_append_and_default_keep_stable_entry() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        let entry = options_set_string(oo, c"@bytes", 0, |out| write_cstr(out, c"\xff"));
        let name = options_name(&*(entry));
        let previous = (*entry)
            .value
            .string_ptr()
            .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut());
        assert_eq!(
            options_set_string(oo, name, 1, |out| {
                write_cstr(out, CStr::from_ptr(previous))
            }),
            entry
        );
        assert_eq!(
            CStr::from_ptr(
                (*entry)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            )
            .to_bytes(),
            b"\xff\xff"
        );
        let previous = (*entry)
            .value
            .string_ptr()
            .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut());
        assert_eq!(
            options_set_string(oo, name, 0, |out| {
                write_cstr(out, CStr::from_ptr(previous))
            }),
            entry
        );
        assert_eq!(
            CStr::from_ptr(
                (*entry)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            )
            .to_bytes(),
            b"\xff\xff"
        );
        options_set_string(oo, name, 0, |out| out.write_all(b""));
        assert_eq!(
            CStr::from_ptr(
                (*entry)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            ),
            c""
        );

        let table = &raw const hmux::src::options_table::options_table;
        let definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"status-left"))
            .unwrap();
        let default = options_default(oo, definition);
        assert_eq!(
            CStr::from_ptr(
                (*default)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            ),
            definition.default_str.unwrap()
        );
        assert_eq!(
            hmux::src::options::options_get_only_mut(&mut *(oo), definition.name.unwrap())
                .map_or(std::ptr::null_mut(), |entry| entry),
            default
        );

        let empty_definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"status-right"))
            .unwrap();
        let empty = options_empty(oo, empty_definition);
        assert!((*empty)
            .value
            .string_ptr()
            .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            .is_null());
        options_set_string(oo, empty_definition.name.unwrap(), 1, |out| {
            write_cstr(out, c"tail")
        });
        assert_eq!(
            CStr::from_ptr(
                (*empty)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            ),
            c"(null)tail"
        );
        drop(oo_owner);
    }
}

#[test]
fn array_keys_order_normalize_and_keep_stable_items() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        let table = &raw const hmux::src::options_table::options_table;
        let definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"update-environment"))
            .unwrap();
        let array = options_empty(oo, definition);
        assert!(options_array_iter_mut(&mut *(array))
            .next()
            .map_or(std::ptr::null_mut(), |item| item)
            .is_null());
        let keys: &[&[u8]] = &[
            b"\xff",
            b"10",
            b"alpha",
            b"002",
            b"0",
            b"4294967295",
            b"\x80",
            b"-1",
        ];
        for key in keys {
            let key = CString::new(*key).unwrap();
            assert_eq!(
                options_array_set(array, &*key, Some(c"value"), 0, null_mut()),
                0
            );
        }
        let first = options_array_iter_mut(&mut *(array))
            .next()
            .map_or(std::ptr::null_mut(), |item| item);
        let two = options_array_iter_mut(&mut *array)
            .nth(1)
            .map_or(std::ptr::null_mut(), |item| item);
        assert_eq!(
            CStr::from_ptr(options_array_item_key(&*(two)).as_ptr()),
            c"2"
        );
        assert_eq!(
            options_array_set(array, c"0002", Some(c"updated"), 0, null_mut()),
            0
        );
        assert_eq!(
            options_array_iter_mut(&mut *array)
                .nth(1)
                .map_or(std::ptr::null_mut(), |item| item),
            two
        );
        assert_eq!(
            hmux::src::options::options_array_get_mut(
                &mut *(array),
                std::ffi::CStr::from_ptr(c"02".as_ptr())
            )
            .map_or(std::ptr::null_mut(), |value| value),
            (hmux::src::options::options_array_item_value_mut(&mut *(two))
                as *mut hmux::src::shared::options::options_value)
        );
        assert_eq!(
            hmux::src::options::options_array_get_index_mut(&mut *(array), 2)
                .map_or(std::ptr::null_mut(), |value| value),
            (hmux::src::options::options_array_item_value_mut(&mut *(two))
                as *mut hmux::src::shared::options::options_value)
        );
        for (index, key) in [(0, c"0"), (u32::MAX, c"4294967295")] {
            let value = hmux::src::options::options_array_get_index_mut(&mut *(array), index)
                .map_or(std::ptr::null_mut(), |value| value);
            assert!(!value.is_null());
            assert_eq!(
                value,
                hmux::src::options::options_array_get_mut(
                    &mut *(array),
                    std::ffi::CStr::from_ptr(key.as_ptr())
                )
                .map_or(std::ptr::null_mut(), |value| value)
            );
        }
        assert!(
            hmux::src::options::options_array_get_index_mut(&mut *(array), 3)
                .map_or(std::ptr::null_mut(), |value| value)
                .is_null()
        );
        let non_utf8 = c"\xff";
        let non_utf8_value = hmux::src::options::options_array_get_mut(
            &mut *(array),
            std::ffi::CStr::from_ptr(non_utf8.as_ptr()),
        )
        .map_or(std::ptr::null_mut(), |value| value);
        assert!(!non_utf8_value.is_null());
        assert!(hmux::src::options::options_array_get_mut(
            &mut *(array),
            std::ffi::CStr::from_ptr(c"".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |value| value)
        .is_null());
        assert_eq!(
            CStr::from_ptr(
                (*(hmux::src::options::options_array_item_value_mut(&mut *(two))
                    as *mut hmux::src::shared::options::options_value))
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            ),
            c"updated"
        );
        for (key, expected) in [
            (c"0002".as_ptr(), b"updated".as_slice()),
            (non_utf8.as_ptr(), b"value".as_slice()),
            (c"4294967296".as_ptr(), b"".as_slice()),
        ] {
            let output = options_to_cstring(array, Some(CStr::from_ptr(key)), 0);
            assert_eq!(output.as_bytes(), expected);
        }
        for invalid in [c"", c"4294967296"] {
            assert_eq!(
                options_array_set(array, &*invalid, Some(c"bad"), 0, null_mut()),
                -1
            );
            assert!(hmux::src::options::options_array_get_mut(
                &mut *(array),
                std::ffi::CStr::from_ptr(invalid.as_ptr())
            )
            .map_or(std::ptr::null_mut(), |value| value)
            .is_null());
        }
        let expected: &[&[u8]] = &[
            b"0",
            b"2",
            b"10",
            b"4294967295",
            b"-1",
            b"alpha",
            b"\x80",
            b"\xff",
        ];
        let keys: Vec<_> = options_array_iter(&*array)
            .map(|item| item.key.clone())
            .collect();
        for (key, saved_key) in expected.iter().zip(keys) {
            let item = options_array_iter_mut(&mut *array)
                .find(|item| item.key == saved_key)
                .map_or(std::ptr::null_mut(), |item| item);
            assert!(!item.is_null());
            assert_eq!(
                CStr::from_ptr(options_array_item_key(&*(item)).as_ptr()).to_bytes(),
                *key
            );
            let key = CString::new(*key).unwrap();
            assert_eq!(options_array_set(array, &*key, None, 0, null_mut()), 0);
        }
        assert!(options_array_iter_mut(&mut *(array))
            .next()
            .map_or(std::ptr::null_mut(), |item| item)
            .is_null());
        // Clearing keeps the storage reusable; destroying a populated option
        // releases both the Rust index and C-allocated values.
        for _ in 0..3 {
            for index in (0..128).rev() {
                let key = CString::new(index.to_string()).unwrap();
                assert_eq!(
                    options_array_set(array, &*key, Some(c"again"), 0, null_mut()),
                    0
                );
            }
            options_array_clear(array);
            assert!(options_array_iter_mut(&mut *(array))
                .next()
                .map_or(std::ptr::null_mut(), |item| item)
                .is_null());
        }
        assert_eq!(
            options_array_set(array, c"7", Some(c"last"), 0, null_mut()),
            0
        );
        // Replacing the option also destroys populated array storage.
        let replacement = options_default(oo, definition);
        assert!(!options_array_iter_mut(&mut *(replacement))
            .next()
            .map_or(std::ptr::null_mut(), |item| item)
            .is_null());
        drop(oo_owner);
    }
}

#[test]
fn array_assign_copies_split_tokens_and_keeps_partial_result_on_error() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        let table = &raw const hmux::src::options_table::options_table;
        let definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"update-environment"))
            .unwrap();
        let strings = options_empty(oo, definition);
        let mut input = b"  ONE,\xff TWO,,\0ignored".to_vec();
        assert_eq!(
            options_array_assign(
                strings,
                CStr::from_bytes_until_nul(&input).unwrap(),
                null_mut()
            ),
            0
        );
        input.fill(b'x');
        for (index, expected) in [b"ONE".as_slice(), b"\xff", b"TWO"].iter().enumerate() {
            let key = CString::new(index.to_string()).unwrap();
            let value = hmux::src::options::options_array_get_mut(
                &mut *(strings),
                std::ffi::CStr::from_ptr(key.as_ptr()),
            )
            .map_or(std::ptr::null_mut(), |value| value);
            assert!(!value.is_null());
            assert_eq!(
                CStr::from_ptr(
                    (*value)
                        .string_ptr()
                        .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
                )
                .to_bytes(),
                *expected
            );
        }
        assert!(hmux::src::options::options_array_get_mut(
            &mut *(strings),
            std::ffi::CStr::from_ptr(c"3".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |value| value)
        .is_null());

        let colour_definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"pane-colours"))
            .unwrap();
        let colours = options_empty(oo, colour_definition);
        let mut cause: Option<CString> = None;
        assert_eq!(
            options_array_assign(colours, c"red,,invalid-colour", &mut cause),
            -1
        );
        assert!(!hmux::src::options::options_array_get_mut(
            &mut *(colours),
            std::ffi::CStr::from_ptr(c"0".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |value| value)
        .is_null());
        assert!(hmux::src::options::options_array_get_mut(
            &mut *(colours),
            std::ffi::CStr::from_ptr(c"1".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |value| value)
        .is_null());
        assert_eq!(
            CStr::from_ptr(cause.as_ref().unwrap().as_ptr()),
            c"bad colour: invalid-colour"
        );
        drop(oo_owner);
    }
}

/// Run `body` with `global_options` pointing at a fresh, empty table so command
/// parsing (which consults `command-alias`) and parent fallback have a server
/// table to read, restoring the previous pointer afterwards.
unsafe fn with_global_options(body: impl FnOnce(*mut hmux::src::shared::options::options)) {
    let mut global_owner = options_create(None);
    let global = &raw mut *global_owner;
    let saved = hmux::src::tmux::global_options;
    hmux::src::tmux::global_options = global;
    body(global);
    hmux::src::tmux::global_options = saved;
    drop(global_owner);
}

fn table_entry(name: &CStr) -> &'static hmux::src::shared::options::options_table_entry {
    hmux::src::options_table::options_table
        .iter()
        .find(|oe| oe.name == Some(name))
        .unwrap()
}

#[test]
fn command_arrays_parse_items_and_assign_unseparated_values() {
    unsafe {
        with_global_options(|_| {
            let mut oo_owner = options_create(None);
            let oo = &raw mut *oo_owner;
            // Hooks are command arrays with an empty separator.
            let hook = options_empty(oo, table_entry(c"after-new-window"));

            // A new key publishes a parsed command list; the same key again
            // replaces it in place.
            assert_eq!(
                options_array_set(hook, c"3", Some(c"display-message first"), 0, null_mut()),
                0
            );
            assert_eq!(
                options_to_cstring(hook, Some(c"3"), 0).as_bytes(),
                b"display-message first"
            );
            assert_eq!(
                options_array_set(hook, c"3", Some(c"display-message second"), 0, null_mut()),
                0
            );
            assert_eq!(
                options_to_cstring(hook, Some(c"3"), 0).as_bytes(),
                b"display-message second"
            );
            assert_eq!(options_array_iter(&*hook).count(), 1);

            // A command that does not parse leaves no item and reports why.
            let mut cause: Option<CString> = None;
            assert_eq!(
                options_array_set(hook, c"4", Some(c"no-such-command"), 0, &mut cause),
                -1
            );
            assert_eq!(
                cause.as_deref().unwrap().to_bytes(),
                b"unknown command: no-such-command"
            );
            assert!(options_array_get(&*hook, c"4").is_none());

            // With no separator the whole value becomes one item at the first
            // free index, so a command list containing spaces is not split.
            assert_eq!(
                options_array_assign(hook, c"display-message a ; display-message b", null_mut()),
                0
            );
            assert_eq!(
                options_to_cstring(hook, Some(c"0"), 0).as_bytes(),
                b"display-message a ; display-message b"
            );
            assert_eq!(options_array_iter(&*hook).count(), 2);
            // An empty value assigns nothing.
            assert_eq!(options_array_assign(hook, c"", null_mut()), 0);
            assert_eq!(options_array_iter(&*hook).count(), 2);
            drop(oo_owner);
        });
    }
}

#[test]
fn set_command_publishes_a_local_default_before_replacing_it() {
    unsafe {
        with_global_options(|global| {
            let definition = table_entry(c"default-client-command");
            let inherited = options_default(global, definition);
            assert_eq!(
                options_to_cstring(inherited, None, 0).as_bytes(),
                b"new-session"
            );
            let mut child_owner = options_create(Some(OptionsScope::GlobalServer));
            let child = &raw mut *child_owner;
            assert!(options_get_only(&*child, c"default-client-command").is_none());

            let mut parsed =
                hmux::src::cmd::parse::cmd_parse_from_string(c"list-sessions -F x", null_mut());
            let local =
                options_set_command(child, c"default-client-command", parsed.take_cmdlist());
            assert!(!local.is_null());
            assert_ne!(local, inherited);
            assert_eq!(
                options_get_only(&*child, c"default-client-command").map(|entry| entry.id()),
                Some((*local).id())
            );
            assert_eq!(
                options_to_cstring(local, None, 0).as_bytes(),
                b"list-sessions -F x"
            );
            // The parent keeps its own default.
            assert_eq!(
                options_to_cstring(inherited, None, 0).as_bytes(),
                b"new-session"
            );
            drop(child_owner);
        });
    }
}

#[test]
fn remove_or_default_resets_global_tables_and_removes_array_items() {
    unsafe {
        with_global_options(|global| {
            // In a global table a built-in is reset to its default rather than
            // removed, so lookups keep finding it.
            let definition = table_entry(c"status-left");
            let entry = options_default(global, definition);
            options_set_string(global, c"status-left", 0, |out| out.write_all(b"changed"));
            assert_eq!(
                options_get_string(global, c"status-left").as_bytes(),
                b"changed"
            );
            assert_eq!(options_remove_or_default(entry, None, null_mut()), 0);
            assert_eq!(
                options_get_string(global, c"status-left").as_c_str(),
                definition.default_str.unwrap()
            );

            // A key removes only that item of an array.
            let array = options_empty(global, table_entry(c"update-environment"));
            for (key, value) in [(c"0", c"FIRST"), (c"1", c"SECOND")] {
                assert_eq!(options_array_set(array, key, Some(value), 0, null_mut()), 0);
            }
            assert_eq!(options_remove_or_default(array, Some(c"1"), null_mut()), 0);
            assert!(options_array_get(&*array, c"1").is_none());
            assert_eq!(
                options_to_cstring(array, Some(c"0"), 0).as_bytes(),
                b"FIRST"
            );

            // An unusable key is reported and leaves the array alone.
            let mut cause: Option<CString> = None;
            assert_eq!(
                options_remove_or_default(array, Some(c"4294967296"), &mut cause),
                -1
            );
            assert_eq!(
                cause.as_deref().unwrap().to_bytes(),
                b"bad array key: 4294967296"
            );
            assert_eq!(options_array_iter(&*array).count(), 1);
        });
    }
}
