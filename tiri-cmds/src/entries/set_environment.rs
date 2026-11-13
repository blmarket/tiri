use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::cmd::queue::cmdq_item;
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_CANFAIL};
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::CStr;
use tiri_core::actions::TaskOutcome as CmdOutcome;
use tiri_core::arguments::{args_count, args_get, args_has, args_parse, args_string};
use tiri_core::environ::{environ_clear, environ_set, environ_unset};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::format_single_from_target_cstring;
use tiri_core::session::Session;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::environment::ENVIRON_HIDDEN;
use tiri_core::shared::environment::environ;
use tiri_core::tmux::global_environ_mut;
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
            flag: b't',
            type_0: CMD_FIND_SESSION,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_set_environment_exec),
    }
};
fn cmd_set_environment_exec(self_0: &cmd, item_handle: &std::rc::Rc<cmdq_item>) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0);
    let target = tiri_core::actions::queue::target(&item);

    let mut name: &CStr = args_string(args, 0 as u_int).expect("argument is present");
    let mut value: Option<&CStr> = None;
    let mut expanded: Option<std::ffi::CString> = None;
    if name.is_empty() {
        cmdq_error(item_handle, |out| out.write_all(b"empty variable name"));
        return CmdOutcome::Error;
    }
    if name.to_bytes().contains(&b'=') {
        cmdq_error(item_handle, |out| {
            out.write_all(b"variable name contains =")
        });
        return CmdOutcome::Error;
    }
    if args_count(args) < 2 as u_int {
        value = None;
    } else {
        value = args_string(args, 1 as u_int);
    }
    if let Some(unexpanded) = value.filter(|_| args_has(args, 'F' as i32 as u_char) != 0) {
        expanded = Some(format_single_from_target_cstring(item_handle, unexpanded));
        value = expanded.as_deref();
    }
    let global = args_has(args, b'g') != 0;
    let session = target.session_handle();
    if !global && session.is_none() {
        if let Some(target) = args_get(args, b't') {
            cmdq_error(item_handle, |out| {
                out.write_all(b"no such session: ")?;
                out.write_all(target.to_bytes())
            });
        } else {
            cmdq_error(item_handle, |out| out.write_all(b"no current session"));
        }
        return CmdOutcome::Error;
    }
    let unset = args_has(args, b'u') != 0;
    let remove = args_has(args, b'r') != 0;
    let error = if unset && value.is_some() {
        Some(b"can't specify a value with -u".as_slice())
    } else if !unset && remove && value.is_some() {
        Some(b"can't specify a value with -r".as_slice())
    } else if !unset && !remove && value.is_none() {
        Some(b"no value specified".as_slice())
    } else {
        None
    };
    if let Some(error) = error {
        cmdq_error(item_handle, |out| out.write_all(error));
        return CmdOutcome::Error;
    }
    let hidden = if args_has(args, b'h') != 0 {
        ENVIRON_HIDDEN
    } else {
        0
    };
    let edit = |env: &mut environ| {
        if unset {
            environ_unset(env, name);
        } else if remove {
            environ_clear(env, name);
        } else {
            environ_set(env, name, hidden, |out| write_cstr(out, value));
        }
    };
    if global {
        global_environ_mut(edit);
    } else {
        session
            .as_ref()
            .expect("target session")
            .with_environment_mut(edit)
            .expect("session environment");
    }
    CmdOutcome::Normal
}
