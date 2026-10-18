use crate::src::arguments::{args, args_get, args_has, args_parse, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::format::bytes::write_cstr;
use crate::src::key_bindings::{
    key_bindings_get_table, key_bindings_remove, key_bindings_remove_table,
};
use crate::src::key_string::key_string_parse_cstr;
use crate::src::shared::abi::*;
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::key::*;
use std::ffi::CStr;
pub static cmd_unbind_key_entry: cmd_entry = {
    cmd_entry {
        name: c"unbind-key",
        alias: Some(c"unbind"),
        args: args_parse {
            template: c"anqT:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-anq] [-T key-table] key",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_unbind_key_exec),
    }
};
unsafe fn cmd_unbind_key_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut key: key_code = 0;
    let mut tablename: &CStr = c"";
    let mut keystr: Option<&CStr> = args_string(&mut *(args), 0 as u_int);
    let mut quiet: ::core::ffi::c_int = args_has(&*args, 'q' as i32 as u_char);
    if args_has(&*args, 'a' as i32 as u_char) != 0 {
        if keystr.is_some() {
            if quiet == 0 {
                cmdq_error(item_handle, |out| out.write_all(b"key given with -a"));
            }
            return CmdOutcome::Error;
        }
        tablename = match args_get(&*(args), 'T' as i32 as u_char) {
            Some(tablename) => tablename,
            None if args_has(&*args, 'n' as i32 as u_char) != 0 => c"root",
            None => c"prefix",
        };
        if key_bindings_get_table(tablename, 0 as ::core::ffi::c_int).is_none() {
            if quiet == 0 {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"table ")?;
                    write_cstr(out, tablename)?;
                    out.write_all(b" doesn't exist")
                });
            }
            return CmdOutcome::Error;
        }
        key_bindings_remove_table(tablename);
        return CmdOutcome::Normal;
    }
    let Some(keystr) = keystr else {
        if quiet == 0 {
            cmdq_error(item_handle, |out| out.write_all(b"missing key"));
        }
        return CmdOutcome::Error;
    };
    key = key_string_parse_cstr(keystr).unwrap_or(KEYC_UNKNOWN);
    if key == KEYC_NONE as ::core::ffi::c_ulong as key_code
        || key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
    {
        if quiet == 0 {
            cmdq_error(item_handle, |out| {
                out.write_all(b"unknown key: ")?;
                write_cstr(out, keystr)
            });
        }
        return CmdOutcome::Error;
    }
    if args_has(&*args, 'T' as i32 as u_char) != 0 {
        tablename = args_get(&*(args), 'T' as i32 as u_char).expect("-T has a value");
        if key_bindings_get_table(tablename, 0 as ::core::ffi::c_int).is_none() {
            if quiet == 0 {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"table ")?;
                    write_cstr(out, tablename)?;
                    out.write_all(b" doesn't exist")
                });
            }
            return CmdOutcome::Error;
        }
    } else if args_has(&*args, 'n' as i32 as u_char) != 0 {
        tablename = c"root";
    } else {
        tablename = c"prefix";
    }
    key_bindings_remove(tablename, key);
    CmdOutcome::Normal
}
