use crate::src::arguments::{
    args, args_count, args_get, args_has, args_parse, args_parse_type, args_string, ArgsParseError,
    ARGS_PARSE_COMMANDS_OR_STRING,
};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::parse::{cmd_parse_from_arguments, cmd_parse_from_string};
use crate::src::cmd::queue::cmdq_error;
use crate::src::format::bytes::write_cstr;
use crate::src::key_bindings::key_bindings_add;
use crate::src::key_string::key_string_parse_cstr;
use crate::src::shared::abi::*;
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::key::*;
use std::ffi::CStr;
pub static cmd_bind_key_entry: cmd_entry = {
    cmd_entry {
        name: c"bind-key",
        alias: Some(c"bind"),
        args: args_parse {
            template: c"nrN:T:",
            lower: 1 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: Some(cmd_bind_key_args_parse),
        },
        usage: c"[-nr] [-T key-table] [-N note] key [command [argument ...]]",
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
        exec: Some(cmd_bind_key_exec),
    }
};
fn cmd_bind_key_args_parse(
    _args: &mut args,
    _idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    Ok(ARGS_PARSE_COMMANDS_OR_STRING)
}
unsafe fn cmd_bind_key_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut key: key_code = 0;
    let mut tablename: &CStr = c"";
    let mut note: Option<&CStr> = args_get(&*(args), 'N' as i32 as u_char);
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    let mut repeat: ::core::ffi::c_int = 0;
    let mut count: u_int = args_count(&*args);
    key =
        key_string_parse_cstr(args_string(&mut *(args), 0 as u_int).expect("argument is present"))
            .unwrap_or(KEYC_UNKNOWN);
    if key == KEYC_NONE as ::core::ffi::c_ulong as key_code
        || key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
    {
        cmdq_error(item_handle, |out| {
            out.write_all(b"unknown key: ")?;
            write_cstr(out, args_string(&mut *(args), 0 as u_int))
        });
        return CmdOutcome::Error;
    }
    if args_has(&*args, 'T' as i32 as u_char) != 0 {
        tablename = args_get(&*(args), 'T' as i32 as u_char).expect("-T has a value");
    } else if args_has(&*args, 'n' as i32 as u_char) != 0 {
        tablename = c"root";
    } else {
        tablename = c"prefix";
    }
    repeat = args_has(&*args, 'r' as i32 as u_char);
    if count == 1 as u_int {
        key_bindings_add(tablename, key, note, repeat, None);
        return CmdOutcome::Normal;
    }
    if count == 2 {
        if let Some(commands) = (&(*args).values)[1].as_commands() {
            key_bindings_add(tablename, key, note, repeat, Some(commands.clone()));
            return CmdOutcome::Normal;
        }
    }
    if count == 2 as u_int {
        pr = cmd_parse_from_string(
            args_string(&mut *(args), 1 as u_int).expect("argument is present"),
            ::core::ptr::null_mut::<cmd_parse_input>(),
        );
    } else {
        pr = cmd_parse_from_arguments(
            &(&(*args).values)[1..],
            ::core::ptr::null_mut::<cmd_parse_input>(),
        );
    }
    if pr.status as ::core::ffi::c_uint == 0 {
        cmdq_error(item_handle, |out| write_cstr(out, pr.error.as_deref()));
        return CmdOutcome::Error;
    }
    key_bindings_add(tablename, key, note, repeat, pr.take_cmdlist());
    CmdOutcome::Normal
}
