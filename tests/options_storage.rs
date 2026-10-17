//! Behavior of the Rust-owned option-name index through the existing API.
use std::ffi::{CStr, CString};
use tiri::src::format::bytes::write_cstr;
use tiri::src::options::options_entry;
use tiri::src::options::*;

#[test]
fn scalar_string_replacement_append_and_default_keep_stable_entry() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        let entry: *mut options_entry =
            options_set_string(&mut *oo, c"@bytes", 0, |out| write_cstr(out, c"\xff"));
        let name = c"@bytes";
        let previous = (*entry)
            .value
            .string_ptr()
            .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut());
        assert_eq!(
            options_set_string(&mut *oo, name, 1, |out| {
                write_cstr(out, CStr::from_ptr(previous))
            }) as *mut options_entry,
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
            options_set_string(&mut *oo, name, 0, |out| {
                write_cstr(out, CStr::from_ptr(previous))
            }) as *mut options_entry,
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
        options_set_string(&mut *oo, name, 0, |out| out.write_all(b""));
        assert_eq!(
            CStr::from_ptr(
                (*entry)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            ),
            c""
        );

        let table = &raw const tiri::src::options::options_table;
        let definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"status-left"))
            .unwrap();
        let default: *mut options_entry = options_default(&mut *oo, definition);
        assert_eq!(
            CStr::from_ptr(
                (*default)
                    .value
                    .string_ptr()
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            ),
            definition.default_str.unwrap()
        );
        match tiri::src::options::options_local_entry(&mut *(oo), definition.name.unwrap()) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                assert_eq!(&mut **entry.into_mut() as *mut _, default)
            }
            std::collections::btree_map::Entry::Vacant(_) => panic!("default entry missing"),
        }

        let empty_definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"status-right"))
            .unwrap();
        let empty: *mut options_entry = options_empty(&mut *oo, empty_definition);
        assert!((*empty)
            .value
            .string_ptr()
            .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            .is_null());
        options_set_string(&mut *oo, empty_definition.name.unwrap(), 1, |out| {
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
        let table = &raw const tiri::src::options::options_table;
        let definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"update-environment"))
            .unwrap();
        let strings: *mut options_entry = options_empty(&mut *oo, definition);
        let mut input = b"  ONE,\xff TWO,,\0ignored".to_vec();
        assert_eq!(
            (*strings)
                .value
                .array_mut()
                .unwrap()
                .assign(definition, CStr::from_bytes_until_nul(&input).unwrap()),
            Ok(())
        );
        input.fill(b'x');
        for (index, expected) in [b"ONE".as_slice(), b"\xff", b"TWO"].iter().enumerate() {
            let key = CString::new(index.to_string()).unwrap();
            let value = (*strings)
                .value
                .array_mut()
                .unwrap()
                .get_mut(std::ffi::CStr::from_ptr(key.as_ptr()))
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
        assert!((*strings)
            .value
            .array_mut()
            .unwrap()
            .get_mut(std::ffi::CStr::from_ptr(c"3".as_ptr()))
            .map_or(std::ptr::null_mut(), |value| value)
            .is_null());

        let colour_definition = (*table)
            .iter()
            .find(|oe| oe.name == Some(c"pane-colours"))
            .unwrap();
        let colours: *mut options_entry = options_empty(&mut *oo, colour_definition);
        let cause = (*colours)
            .value
            .array_mut()
            .unwrap()
            .assign(colour_definition, c"red,,invalid-colour")
            .unwrap_err();
        assert!(!(*colours)
            .value
            .array_mut()
            .unwrap()
            .get_mut(std::ffi::CStr::from_ptr(c"0".as_ptr()))
            .map_or(std::ptr::null_mut(), |value| value)
            .is_null());
        assert!((*colours)
            .value
            .array_mut()
            .unwrap()
            .get_mut(std::ffi::CStr::from_ptr(c"1".as_ptr()))
            .map_or(std::ptr::null_mut(), |value| value)
            .is_null());
        assert_eq!(cause.as_c_str(), c"bad colour: invalid-colour");
        drop(oo_owner);
    }
}

fn table_entry(name: &CStr) -> &'static tiri::src::options::options_table_entry {
    tiri::src::options::options_table
        .iter()
        .find(|oe| oe.name == Some(name))
        .unwrap()
}

#[test]
fn command_arrays_parse_items_and_assign_unseparated_values() {
    unsafe {
        let mut oo_owner = options_create(None);
        let oo = &raw mut *oo_owner;
        let definition = table_entry(c"after-new-window");
        let hook: *mut options_entry = options_empty(&mut *oo, definition);

        assert_eq!(
            (*hook).value.array_mut().unwrap().set(
                definition,
                c"3",
                Some(c"display-message first"),
                false
            ),
            Ok(())
        );
        assert_eq!(
            options_to_cstring(&*hook, Some(c"3"), 0).as_bytes(),
            b"display-message first"
        );
        assert_eq!(
            (*hook).value.array_mut().unwrap().set(
                definition,
                c"3",
                Some(c"display-message second"),
                false
            ),
            Ok(())
        );
        assert_eq!(
            options_to_cstring(&*hook, Some(c"3"), 0).as_bytes(),
            b"display-message second"
        );
        assert_eq!((*hook).value.array().unwrap().iter().count(), 1);

        let cause = (*hook)
            .value
            .array_mut()
            .unwrap()
            .set(definition, c"4", Some(c"no-such-command"), false)
            .unwrap_err();
        assert_eq!(cause.to_bytes(), b"unknown command: no-such-command");
        assert!((*hook).value.array().unwrap().get(c"4").is_none());

        assert_eq!(
            (*hook)
                .value
                .array_mut()
                .unwrap()
                .assign(definition, c"display-message a ; display-message b"),
            Ok(())
        );
        assert_eq!(
            options_to_cstring(&*hook, Some(c"0"), 0).as_bytes(),
            b"display-message a ; display-message b"
        );
        assert_eq!((*hook).value.array().unwrap().iter().count(), 2);
        assert_eq!(
            (*hook).value.array_mut().unwrap().assign(definition, c""),
            Ok(())
        );
        assert_eq!((*hook).value.array().unwrap().iter().count(), 2);
        drop(oo_owner);
    }
}
