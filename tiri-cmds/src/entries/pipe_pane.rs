use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_PANE, cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::args_parse;
use tiri_core::format::bytes::write_cstr;
use tiri_core::window_pane::WindowPane;

pub static cmd_pipe_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"pipe-pane",
        alias: Some(c"pipep"),
        args: args_parse {
            template: c"IOot:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-IOo] [-t target-pane] [shell-command]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_pipe_pane_exec),
    }
};
fn cmd_pipe_pane_exec<'a, 'ctx>(
    command: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let cf = inv.format();
    let (target, tc) = {
        (
            inv.target().clone(),
            inv.target_client(),
        )
    };
    let pane = target.wp.upgrade().expect("live pipe target pane");
    match pane.pipe(crate::cmd::cmd_get_args(command), Some(&cf), tc.as_ref(), &target) {
        Ok(()) => ExecOutcome::ok(),
        Err(err) => {
            inv.error( |out| write_cstr(out, &*err));
            ExecOutcome::err()
        }
    }
})
}
