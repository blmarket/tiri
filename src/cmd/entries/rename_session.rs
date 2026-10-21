use crate::arguments::{args_parse, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::format::format_single_from_target_cstring;
use crate::shared::command::*;

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
    item: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    use crate::session::Session;
    let cmd_ref = command.try_borrow().expect("live command");
    let arguments = cmd_get_args(&cmd_ref);
    let name = args_string(arguments, 0).expect("rename-session argument");
    let expanded = format_single_from_target_cstring(item, name);
    let target = crate::cmd::queue::cmdq_get_target(&item.borrow()).clone();
    let owner = target.session_handle().expect("rename-session target");
    match owner.rename(&expanded) {
        Ok(()) => CmdOutcome::Normal,
        Err(cause) => {
            cmdq_error(item, |out| out.write_all(cause.as_bytes()));
            CmdOutcome::Error
        }
    }
}
