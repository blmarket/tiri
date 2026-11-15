use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::{
    CMD_AFTERHOOK, CMD_FIND_PANE, CMD_FIND_SESSION, cmd, cmd_entry, cmd_entry_flag,
};
use tiri_core::arguments::{args_parse, args_string};

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
            flag: b't',
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_rename_session_exec),
    }
};
fn cmd_rename_session_exec<'a, 'ctx>(
    command: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    use tiri_core::session::Session;
    let arguments = cmd_get_args(command);
    let name = args_string(arguments, 0).expect("rename-session argument");
    let expanded = inv.format_single( name);
    let target = inv.target().clone();
    let owner = target.session_handle().expect("rename-session target");
    match owner.rename(&expanded) {
        Ok(()) => ExecOutcome::ok(),
        Err(cause) => {
            inv.error( |out| out.write_all(cause.as_bytes()));
            ExecOutcome::err()
        }
    }
})
}
