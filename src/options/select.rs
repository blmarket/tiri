//! Option scope selection from command arguments.
use super::diagnostic::options_string_cause;
use super::scope::OptionsScope;
use super::table::options_table;
use super::types::*;
use crate::src::arguments::{args, args_get, args_has};
use crate::src::shared::window::WindowRef;

use crate::src::shared::command::cmd_find_state;
use std::ffi::{CStr, CString};

/// Select the options scope for `name`, returning the table kind and scope.
pub unsafe fn options_scope_from_name(
    args: *mut args,
    window: i32,
    name: &CStr,
    fs: &cmd_find_state,
) -> Result<(i32, OptionsScope), CString> {
    if name.to_bytes().starts_with(b"@") {
        return options_scope_from_flags(args, window, fs);
    }
    let Some(definition) = options_table.iter().find(|entry| entry.name == Some(name)) else {
        return Err(options_string_cause(c"unknown option: %s", &[name]));
    };
    let kind = if definition.scope == OPTIONS_TABLE_WINDOW | OPTIONS_TABLE_PANE {
        if args_has(args, b'p') != 0 {
            OPTIONS_TABLE_PANE
        } else {
            OPTIONS_TABLE_WINDOW
        }
    } else {
        definition.scope
    };
    options_select_scope(args, fs, kind)
}
pub unsafe fn options_scope_from_flags(
    args: *mut args,
    window: i32,
    fs: &cmd_find_state,
) -> Result<(i32, OptionsScope), CString> {
    let kind = if args_has(args, b's') != 0 {
        OPTIONS_TABLE_SERVER
    } else if args_has(args, b'p') != 0 {
        OPTIONS_TABLE_PANE
    } else if window != 0 || args_has(args, b'w') != 0 {
        OPTIONS_TABLE_WINDOW
    } else {
        OPTIONS_TABLE_SESSION
    };
    options_select_scope(args, fs, kind)
}
unsafe fn options_select_scope(
    args: *mut args,
    fs: &cmd_find_state,
    kind: i32,
) -> Result<(i32, OptionsScope), CString> {
    let global = args_has(args, b'g') != 0;
    let (scope, noun) = match kind {
        OPTIONS_TABLE_SERVER => (Some(OptionsScope::GlobalServer), c"server"),
        OPTIONS_TABLE_SESSION if global => (Some(OptionsScope::GlobalSession), c"session"),
        OPTIONS_TABLE_SESSION => (
            (fs.s.strong_count() != 0).then(|| OptionsScope::Session(fs.s.clone())),
            c"session",
        ),
        OPTIONS_TABLE_WINDOW if global => (Some(OptionsScope::GlobalWindow), c"window"),
        OPTIONS_TABLE_WINDOW => {
            let scope = if fs.wl.is_alive() {
                Some(OptionsScope::Window(WindowRef::downgrade(
                    fs.wl
                        .get_unchecked()
                        .window_handle()
                        .expect("option window"),
                )))
            } else {
                None
            };
            (scope, c"window")
        }
        OPTIONS_TABLE_PANE => (
            (fs.wp.strong_count() != 0).then(|| OptionsScope::Pane(fs.wp.clone())),
            c"pane",
        ),
        _ => unreachable!("unknown option scope kind"),
    };
    if let Some(scope) = scope {
        return Ok((kind, scope));
    }
    let target = args_get(&*args, b't');
    let diagnostic = if let Some(target) = target {
        let mut bytes = b"no such ".to_vec();
        bytes.extend_from_slice(noun.to_bytes());
        bytes.extend_from_slice(b": ");
        bytes.extend_from_slice(target.to_bytes());
        CString::new(bytes).unwrap()
    } else {
        let mut bytes = b"no current ".to_vec();
        bytes.extend_from_slice(noun.to_bytes());
        CString::new(bytes).unwrap()
    };
    Err(diagnostic)
}
