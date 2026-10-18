use crate::src::arguments::{args, args_count, args_get, args_has, args_parse, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::environ::{environ_clear, environ_set, environ_unset};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_FIND_CANFAIL};
use crate::src::shared::environment::environ;
use crate::src::shared::environment::ENVIRON_HIDDEN;
use crate::src::tmux::global_environ;
use libc::strchr;
use std::ffi::CStr;
pub static cmd_set_environment_entry: cmd_entry = {
    cmd_entry {
        name: c"set-environment",
        alias: Some(c"setenv"),
        args: args_parse {
            template: c"Fhgrt:u",
            lower: 1 as ::core::ffi::c_int,
            upper: 2 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-Fhgru] [-t target-session] variable [value]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_set_environment_exec),
    }
};
unsafe fn cmd_set_environment_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);

    let mut name: *const ::core::ffi::c_char =
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut expanded: Option<std::ffi::CString> = None;
    if *name as ::core::ffi::c_int == '\0' as i32 {
        cmdq_error(item_handle, |out| out.write_all(b"empty variable name"));
        return CMD_RETURN_ERROR;
    }
    if !strchr(name, '=' as i32).is_null() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"variable name contains =")
        });
        return CMD_RETURN_ERROR;
    }
    if args_count(&*args) < 2 as u_int {
        value = ::core::ptr::null::<::core::ffi::c_char>();
    } else {
        value =
            args_string(&mut *(args), 1 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    }
    if !value.is_null() && args_has(&*args, 'F' as i32 as u_char) != 0 {
        expanded = Some(format_single_from_target_cstring(item_handle, value));
        value = expanded.as_ref().expect("expanded value was set").as_ptr();
    }
    let global = args_has(&*args, b'g') != 0;
    let session = (*target).session_handle();
    if !global && session.is_none() {
        if let Some(target) = args_get(&*args, b't') {
            cmdq_error(item_handle, |out| {
                out.write_all(b"no such session: ")?;
                out.write_all(target.to_bytes())
            });
        } else {
            cmdq_error(item_handle, |out| out.write_all(b"no current session"));
        }
        return CMD_RETURN_ERROR;
    }
    let unset = args_has(&*args, b'u') != 0;
    let remove = args_has(&*args, b'r') != 0;
    let error = if unset && !value.is_null() {
        Some(b"can't specify a value with -u".as_slice())
    } else if !unset && remove && !value.is_null() {
        Some(b"can't specify a value with -r".as_slice())
    } else if !unset && !remove && value.is_null() {
        Some(b"no value specified".as_slice())
    } else {
        None
    };
    if let Some(error) = error {
        cmdq_error(item_handle, |out| out.write_all(error));
        return CMD_RETURN_ERROR;
    }
    let hidden = if args_has(&*args, b'h') != 0 {
        ENVIRON_HIDDEN
    } else {
        0
    };
    let edit = |env: &mut environ| {
        if unset {
            environ_unset(env, CStr::from_ptr(name));
        } else if remove {
            environ_clear(env, CStr::from_ptr(name));
        } else {
            environ_set(env, CStr::from_ptr(name), hidden, |out| {
                write_cstr(out, nullable_cstr(value))
            });
        }
    };
    if global {
        edit(global_environ.as_deref_mut().expect("environment"));
    } else {
        session
            .as_ref()
            .expect("target session")
            .with_environment_mut(edit);
    }
    CMD_RETURN_NORMAL
}
