use crate::src::arguments::{args_parse, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::format::format_single_from_target_cstring;
use crate::src::shared::command::*;

pub static cmd_rename_session_entry: cmd_entry = {
    cmd_entry {
        name: c"rename-session",
        alias: Some(c"rename"),
        args: args_parse {
            template: c"t:",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-t target-session] new-name",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_rename_session_exec),
    }
};
unsafe fn cmd_rename_session_exec(
    mut command: refbox::Weak<cmd>,
    item: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    use crate::src::session::Session;
    let arguments = cmd_get_args_mut(command.get_mut_unchecked()).expect("command arguments");
    let name = args_string(arguments, 0).expect("rename-session argument");
    let expanded = format_single_from_target_cstring(item, name.as_ptr());
    let target = crate::src::cmd::queue::cmdq_get_target(&*item.get());
    let owner = (*target).session_handle().expect("rename-session target");
    match owner.rename(&expanded) {
        Ok(()) => CmdOutcome::Normal,
        Err(cause) => {
            cmdq_error(item, |out| out.write_all(cause.as_bytes()));
            CmdOutcome::Error
        }
    }
}
