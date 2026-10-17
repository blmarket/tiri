//! Behavior of the Rust-owned option-name index through the existing API.
use std::ffi::{CStr, CString};
use std::ptr::null_mut;
use tiri::src::format::bytes::write_cstr;
use tiri::src::options::*;

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

        let table = &raw const tiri::src::options_table::options_table;
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
            tiri::src::options::options_get_only_mut(&mut *(oo), definition.name.unwrap())
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
fn array_assign_copies_split_tokens_and_keeps_partial_result_on_error() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        let table = &raw const tiri::src::options_table::options_table;
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
            let value = tiri::src::options::options_array_get_mut(
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
        assert!(tiri::src::options::options_array_get_mut(
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
        assert!(!tiri::src::options::options_array_get_mut(
            &mut *(colours),
            std::ffi::CStr::from_ptr(c"0".as_ptr())
        )
        .map_or(std::ptr::null_mut(), |value| value)
        .is_null());
        assert!(tiri::src::options::options_array_get_mut(
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
unsafe fn with_global_options(body: impl FnOnce(*mut tiri::src::shared::options::options)) {
    let mut global_owner = options_create(None);
    let global = &raw mut *global_owner;
    let saved = tiri::src::tmux::global_options;
    tiri::src::tmux::global_options = global;
    body(global);
    tiri::src::tmux::global_options = saved;
    drop(global_owner);
}

fn table_entry(name: &CStr) -> &'static tiri::src::shared::options::options_table_entry {
    tiri::src::options_table::options_table
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
            let hook = options_empty(oo, table_entry(c"after-new-window"));

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

            assert_eq!(
                options_array_assign(hook, c"display-message a ; display-message b", null_mut()),
                0
            );
            assert_eq!(
                options_to_cstring(hook, Some(c"0"), 0).as_bytes(),
                b"display-message a ; display-message b"
            );
            assert_eq!(options_array_iter(&*hook).count(), 2);
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
                tiri::src::cmd::parse::cmd_parse_from_string(c"list-sessions -F x", null_mut());
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
            assert_eq!(
                options_to_cstring(inherited, None, 0).as_bytes(),
                b"new-session"
            );
            drop(child_owner);
        });
    }
}
