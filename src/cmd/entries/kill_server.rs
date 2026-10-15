use crate::src::cmd::cmd_get_entry;
use crate::src::shared::arguments::args_parse;
use crate::src::shared::command::CMD_STARTSERVER;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::signal::SIGTERM;
use libc::{getpid, kill};
pub static cmd_kill_server_entry: cmd_entry = {
    cmd_entry {
        name: c"kill-server",
        alias: None,
        args: args_parse {
            template: c"",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_kill_server_exec),
    }
};
pub static cmd_start_server_entry: cmd_entry = {
    cmd_entry {
        name: c"start-server",
        alias: Some(c"start"),
        args: args_parse {
            template: c"",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"",
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
        flags: CMD_STARTSERVER,
        exec: Some(cmd_kill_server_exec),
    }
};
unsafe fn cmd_kill_server_exec(
    mut self_0: refbox::Weak<cmd>,
    _item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_kill_server_entry,
    ) {
        kill(getpid(), SIGTERM);
    }
    CMD_RETURN_NORMAL
}
