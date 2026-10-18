use crate::src::arguments::args_parse;
use crate::src::cmd::cmd_get_entry;
use crate::src::cmd::queue::cmdq_get_target_client;
use crate::src::resize::recalculate_sizes;
use crate::src::server_fn::{server_lock, server_lock_client, server_lock_session};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{
    CMD_AFTERHOOK, CMD_CLIENT_TFLAG, CMD_TARGET_CLIENT_USAGE, CMD_TARGET_SESSION_USAGE,
};
pub static cmd_lock_server_entry: cmd_entry = {
    cmd_entry {
        name: c"lock-server",
        alias: Some(c"lock"),
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_lock_server_exec),
    }
};
pub static cmd_lock_session_entry: cmd_entry = {
    cmd_entry {
        name: c"lock-session",
        alias: Some(c"locks"),
        args: args_parse {
            template: c"t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: CMD_TARGET_SESSION_USAGE,
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
        exec: Some(cmd_lock_server_exec),
    }
};
pub static cmd_lock_client_entry: cmd_entry = {
    cmd_entry {
        name: c"lock-client",
        alias: Some(c"lockc"),
        args: args_parse {
            template: c"t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: CMD_TARGET_CLIENT_USAGE,
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
        flags: CMD_AFTERHOOK | CMD_CLIENT_TFLAG,
        exec: Some(cmd_lock_server_exec),
    }
};
unsafe fn cmd_lock_server_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut target_state = crate::src::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let tc_owner = cmdq_get_target_client(Some(&*item));
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_lock_server_entry,
    ) {
        server_lock();
    } else if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_lock_session_entry,
    ) {
        server_lock_session(&(*target).s.upgrade().expect("live target session"));
    } else {
        server_lock_client(tc_owner.as_ref().expect("lock target client"));
    }
    recalculate_sizes();
    CmdOutcome::Normal
}
