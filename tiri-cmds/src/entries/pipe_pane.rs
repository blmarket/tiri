use crate::cmd::queue::cmdq_item;
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_PANE, cmd, cmd_entry, cmd_entry_flag};
use tiri_core::actions::TaskOutcome as CmdOutcome;
use tiri_core::arguments::args_parse;
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
fn cmd_pipe_pane_exec(command: &cmd, item: &std::rc::Rc<cmdq_item>) -> CmdOutcome {
    let target = tiri_core::actions::queue::target(&item.borrow()).clone();
    let pane = target.wp.upgrade().expect("live pipe target pane");
    pane.pipe(crate::cmd::cmd_get_args(command), item)
}
