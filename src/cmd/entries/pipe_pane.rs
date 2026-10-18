use crate::arguments::args_parse;
use crate::shared::command::*;
use crate::window_pane::WindowPane;

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
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_pipe_pane_exec),
    }
};
unsafe fn cmd_pipe_pane_exec(
    command: refbox::Weak<cmd>,
    item: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let target = crate::cmd::queue::cmdq_get_target(&item.borrow()).clone();
    let pane = target.wp.upgrade().expect("live pipe target pane");
    pane.pipe(command, item)
}
