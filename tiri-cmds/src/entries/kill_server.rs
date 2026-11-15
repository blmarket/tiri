use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_entry;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::CMD_STARTSERVER;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::args_parse;
use tiri_core::shared::signal::SIGTERM;
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
fn cmd_kill_server_exec<'a, 'ctx>(
    self_0: &'a cmd,
    _inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    if std::ptr::eq(cmd_get_entry(self_0), &cmd_kill_server_entry) {
        let _ =
            tiri_core::tiri_sys::send_signal(std::process::id() as tiri_core::libc::pid_t, SIGTERM);
    }
    ExecOutcome::ok()
})
}
