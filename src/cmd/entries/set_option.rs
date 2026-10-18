use crate::src::arguments::{
    args, args_count, args_get, args_has, args_parse, args_parse_type, args_string, ArgsParseError,
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING,
};
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::events::events_fire;
use crate::src::events_payload::{
    event_payload_create, event_payload_set_client, event_payload_set_int, event_payload_set_pane,
    event_payload_set_session, event_payload_set_target, event_payload_set_window,
};
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::hooks::{hooks_add_event, hooks_monitor_add, hooks_monitor_remove, hooks_run};
use crate::src::monitor::monitor_parse_owned;
use crate::src::options::{
    options_match_owned, options_push_changes, options_scope_from_name, OptionMatchFailure,
    OptionsScope,
};
use crate::src::options::{OPTIONS_TABLE_IS_ARRAY, OPTIONS_TABLE_WINDOW};
use crate::src::session::sessions;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_FIND_CANFAIL};
use crate::src::shared::monitor::{MONITOR_NOTIFY_TRUE, MONITOR_SESSION};
use crate::src::shared::window::WindowRef;
use crate::src::window::{Window, WindowPane};
use std::ffi::{CStr, CString};
pub static cmd_set_option_entry: cmd_entry = {
    cmd_entry {
        name: c"set-option",
        alias: Some(c"set"),
        args: args_parse {
            template: c"aFgopqst:uUw",
            lower: 1 as ::core::ffi::c_int,
            upper: 2 as ::core::ffi::c_int,
            cb: Some(cmd_set_option_args_parse),
        },
        usage: c"[-aFgopqsuUw] [-t target-pane] option [value]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_set_option_exec),
    }
};
pub static cmd_set_window_option_entry: cmd_entry = {
    cmd_entry {
        name: c"set-window-option",
        alias: Some(c"setw"),
        args: args_parse {
            template: c"aFgoqt:u",
            lower: 1 as ::core::ffi::c_int,
            upper: 2 as ::core::ffi::c_int,
            cb: Some(cmd_set_option_args_parse),
        },
        usage: c"[-aFgoqu] [-t target-window] option [value]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_set_option_exec),
    }
};
pub static cmd_set_hook_entry: cmd_entry = {
    cmd_entry {
        name: c"set-hook",
        alias: None,
        args: args_parse {
            template: c"agpERTt:uB:w",
            lower: 0 as ::core::ffi::c_int,
            upper: 2 as ::core::ffi::c_int,
            cb: Some(cmd_set_option_args_parse),
        },
        usage: c"[-agpERTuw] [-B name:what:format] [-t target-pane] [hook] [command]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_set_option_exec),
    }
};
fn cmd_set_option_args_parse(
    args: &mut args,
    idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    if unsafe { args_has(args as *mut args, 'B' as i32 as u_char) } != 0 {
        return Ok(ARGS_PARSE_COMMANDS_OR_STRING);
    }
    if idx == 1 as u_int {
        return Ok(ARGS_PARSE_COMMANDS_OR_STRING);
    }
    Ok(ARGS_PARSE_STRING)
}
unsafe fn cmd_set_hook_event_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut c: Option<ClientRef> = None;
    if args_count(args) == 0 as u_int {
        cmdq_error(item_handle, |out| out.write_all(b"missing argument"));
        return CMD_RETURN_ERROR;
    }
    if args_count(args) != 1 as u_int {
        cmdq_error(item_handle, |out| out.write_all(b"too many arguments"));
        return CMD_RETURN_ERROR;
    }
    let argument = format_single_from_target_cstring(
        item_handle,
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
    );
    if *argument.as_ptr() as ::core::ffi::c_int != '@' as i32 {
        cmdq_error(item_handle, |out| {
            out.write_all(b"event name must start with @")
        });
        return CMD_RETURN_ERROR;
    }
    let mut ep = event_payload_create();
    event_payload_set_target(&mut ep, &*target);
    let c_owner = cmdq_get_client((item).as_ref());
    c = c_owner.clone();
    if !c.is_none() {
        event_payload_set_client(&mut ep, c.clone().expect("live client"));
    }
    if !(*target).session_handle().is_none() {
        event_payload_set_session(
            &mut ep,
            c"session",
            (*target).session_handle().expect("live session"),
        );
    }
    if !(*target).window_handle().is_none() {
        event_payload_set_window(
            &mut ep,
            c"window",
            WindowRef::clone(((*target).window_handle().as_ref()).expect("live window")),
        );
    }
    if (*target).winlink_handle().is_alive() {
        event_payload_set_int(
            &mut ep,
            c"window_index",
            ((*target).winlink_handle()).get_unchecked().idx,
        );
    } else if (*target).idx != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"window_index", (*target).idx);
    }
    if !(*target).pane_handle().is_none() {
        event_payload_set_pane(
            &mut ep,
            c"pane",
            (*target).pane_handle().expect("live pane"),
        );
    }
    events_fire(&argument, ep);
    CMD_RETURN_NORMAL
}
unsafe fn cmd_set_hook_monitor_exec(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    args: *mut args,
    window: i32,
) -> cmd_retval {
    let target = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item_handle.get());
    if args_count(args) > 1 {
        cmdq_error(item_handle, |out| out.write_all(b"too many arguments"));
        return CMD_RETURN_ERROR;
    }
    let argument = args_get(&*args, b'B').expect("monitor argument").to_owned();
    let unsubscribe = args_has(args, b'u') != 0;
    let parsed = monitor_parse_owned(&argument);
    let (name, kind, id, format) = if unsubscribe {
        match parsed {
            Some(parsed) => (parsed.name, parsed.type_0, parsed.id, None),
            None => (argument.clone(), MONITOR_SESSION, 0, None),
        }
    } else {
        let Some(parsed) = parsed else {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid subscription: ")?;
                write_cstr(out, &*argument)
            });
            return CMD_RETURN_ERROR;
        };
        (parsed.name, parsed.type_0, parsed.id, Some(parsed.format))
    };
    if !name.as_bytes().starts_with(b"@") {
        cmdq_error(item_handle, |out| {
            out.write_all(b"monitor hook name must start with @")
        });
        return CMD_RETURN_ERROR;
    }
    let selected = match options_scope_from_name(args, window, &name, &*target) {
        Ok((_, selected)) => selected,
        Err(cause) => return set_option_error(item_handle, &cause),
    };
    let mut fs = cmd_find_state::default();
    fs.copy_from(target);
    if unsubscribe {
        hooks_monitor_remove(&selected, &name);
        return CMD_RETURN_NORMAL;
    }
    if args_count(args) != 0 {
        let mut value = args_string(&mut *args, 0)
            .expect("monitor command")
            .to_owned();
        if args_has(args, b'F') != 0 {
            value = format_single_from_target_cstring(item_handle, value.as_ptr());
        }
        let exists = selected.with_entry(&name, |_| ()).is_some();
        if args_has(args, b'o') == 0 || !exists {
            selected
                .set_from_string(None, &name, Some(&value), args_has(args, b'a') != 0)
                .expect("monitor name is a user string option");
            options_push_changes(&name);
        }
    }
    let session = if selected.is_global() {
        None
    } else {
        (*target).session_handle()
    };
    let flags = if args_has(args, b'T') != 0 {
        MONITOR_NOTIFY_TRUE
    } else {
        0
    };
    hooks_monitor_add(
        &selected,
        &name,
        kind,
        id,
        CStr::from_ptr(format.as_deref().expect("monitor format").as_ptr()),
        flags,
        &mut fs,
        session.as_ref(),
    );
    if let Some(session) = session {
        (session).release(c"set-hook monitor");
    }
    CMD_RETURN_NORMAL
}

unsafe fn set_option_error(
    item: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    cause: &CStr,
) -> cmd_retval {
    cmdq_error(item, |out| write_cstr(out, &*cause));
    CMD_RETURN_ERROR
}

/// Pane traversal remains live between removals; a monitor teardown may change
/// membership. Each successor is selected from the current pane's parent.
unsafe fn unset_window_panes(
    target: &cmd_find_state,
    name: &CStr,
    array_key: Option<&CStr>,
) -> Result<(), CString> {
    let Some(window) = target.window_handle() else {
        return Ok(());
    };
    let mut next = window.step_pane(None, false).upgrade();
    drop(window);
    while let Some(pane) = next.take() {
        let scope = OptionsScope::Pane(std::rc::Rc::downgrade(&pane));
        let result = scope.remove_or_default(name, array_key);
        if result.is_ok() {
            if let Some(parent) = pane.window_observer().upgrade() {
                next = parent
                    .step_pane(Some(&std::rc::Rc::downgrade(&pane)), false)
                    .upgrade();
                drop(parent);
            }
        }
        drop(pane);
        result?;
    }
    Ok(())
}

unsafe fn cmd_set_option_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let target = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let window = std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_set_window_option_entry,
    ) as i32;
    let set_hook = std::ptr::eq(cmd_get_entry(self_0.get_unchecked()), &cmd_set_hook_entry);
    if set_hook && args_has(args, b'E') != 0 {
        return cmd_set_hook_event_exec(self_0, item_handle);
    }
    if set_hook && args_has(args, b'B') != 0 {
        return cmd_set_hook_monitor_exec(item_handle, args, window);
    }
    if args_count(args) == 0 {
        return set_option_error(item_handle, c"missing argument");
    }
    let argument = format_single_from_target_cstring(
        item_handle,
        args_string(&mut *args, 0).map_or(std::ptr::null(), CStr::as_ptr),
    );
    if set_hook && args_has(args, b'R') != 0 {
        hooks_run(Some(item_handle), &argument);
        return CMD_RETURN_NORMAL;
    }
    let parsed = match options_match_owned(&argument) {
        Ok(parsed) => parsed,
        Err(failure) => {
            if args_has(args, b'q') != 0 {
                return CMD_RETURN_NORMAL;
            }
            let ambiguous = match failure {
                OptionMatchFailure::Ambiguous => true,
                OptionMatchFailure::Parse => !sessions::is_empty(),
                _ => false,
            };
            cmdq_error(item_handle, |out| {
                out.write_all(if ambiguous {
                    b"ambiguous option: "
                } else {
                    b"invalid option: "
                })?;
                write_cstr(out, &*argument)
            });
            return CMD_RETURN_ERROR;
        }
    };
    let mut value = if args_count(args) < 2 {
        None
    } else {
        args_string(&mut *args, 1).map(CStr::to_owned)
    };
    if args_has(args, b'F') != 0 {
        if let Some(old) = value.as_ref() {
            value = Some(format_single_from_target_cstring(item_handle, old.as_ptr()));
        }
    }
    let (scope, selected) = match options_scope_from_name(args, window, &parsed.name, &*target) {
        Ok(selected) => selected,
        Err(_) if args_has(args, b'q') != 0 => return CMD_RETURN_NORMAL,
        Err(cause) => return set_option_error(item_handle, &cause),
    };
    let name = parsed.name.as_c_str();
    let array_key = parsed.array_key.as_deref();
    let user = name.to_bytes().starts_with(b"@");
    let definition = selected
        .resolve(name, false)
        .and_then(|source| source.with_entry(name, |entry| entry.tableentry)?);
    let array = definition.is_some_and(|definition| definition.flags & OPTIONS_TABLE_IS_ARRAY != 0);
    if array_key.is_some() && (user || !array) {
        cmdq_error(item_handle, |out| {
            out.write_all(b"not an array: ")?;
            write_cstr(out, &*argument)
        });
        return CMD_RETURN_ERROR;
    }
    if args_has(args, b'u') == 0 && args_has(args, b'o') != 0 {
        let already = selected
            .with_entry(name, |entry| {
                array_key.is_none_or(|key| {
                    entry
                        .value
                        .array()
                        .and_then(|array| array.get(key))
                        .is_some()
                })
            })
            .unwrap_or(false);
        if already {
            if args_has(args, b'q') != 0 {
                return CMD_RETURN_NORMAL;
            }
            cmdq_error(item_handle, |out| {
                out.write_all(b"already set: ")?;
                write_cstr(out, &*argument)
            });
            return CMD_RETURN_ERROR;
        }
    }
    if args_has(args, b'U') != 0 && scope == OPTIONS_TABLE_WINDOW {
        if let Err(cause) = unset_window_panes(&*target, name, array_key) {
            return set_option_error(item_handle, &cause);
        }
    }
    let append = args_has(args, b'a') != 0;
    let result = if args_has(args, b'u') != 0 || args_has(args, b'U') != 0 {
        selected.remove_or_default(name, array_key)
    } else if user {
        selected
            .set_from_string(None, name, value.as_deref(), append)
            .map(|()| {
                if set_hook {
                    hooks_add_event(&*name);
                }
                true
            })
    } else if !array {
        let definition = definition.expect("built-in option definition");
        selected
            .set_from_string(
                Some(definition),
                definition.name.expect("built-in name"),
                value.as_deref(),
                append,
            )
            .map(|()| true)
    } else if let Some(value) = value.as_deref() {
        selected.ensure_array(definition.expect("array option definition"));
        if let Some(key) = array_key {
            selected
                .set_array_item(name, key, Some(value), append)
                .map(|()| true)
        } else {
            if !append {
                selected.clear_array(name);
            }
            selected.assign_array(name, value).map(|()| true)
        }
    } else {
        Err(c"empty value".to_owned())
    };
    match result {
        Err(cause) => set_option_error(item_handle, &cause),
        Ok(false) => CMD_RETURN_NORMAL,
        Ok(true) => {
            options_push_changes(name);
            CMD_RETURN_NORMAL
        }
    }
}
