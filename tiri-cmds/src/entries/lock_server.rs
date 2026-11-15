use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_entry;
use crate::shared::command::{
    CMD_AFTERHOOK, CMD_CLIENT_TFLAG, CMD_TARGET_CLIENT_USAGE, CMD_TARGET_SESSION_USAGE,
};
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::args_parse;
use tiri_core::resize::recalculate_sizes;
use tiri_core::server_fn::{server_lock, server_lock_client, server_lock_session};
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
            flag: b't',
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
fn cmd_lock_server_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let target = inv.target().clone();
    let tc_owner = inv.target_client();
    if std::ptr::eq(cmd_get_entry(self_0), &cmd_lock_server_entry) {
        server_lock();
    } else if std::ptr::eq(cmd_get_entry(self_0), &cmd_lock_session_entry) {
        server_lock_session(&target.s.upgrade().expect("live target session"));
    } else {
        server_lock_client(tc_owner.as_ref().expect("lock target client"));
    }
    recalculate_sizes();
    ExecOutcome::ok()
})
}
