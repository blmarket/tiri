use crate::arguments::{args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::window::{LayoutAction, Window as _};
use crate::window_pane::WindowPane as _;

/// Panes take their size from the layout; -Z asks it to give a pane the whole
/// window (in the strip, full width) or take it back, and -T trims history.
pub static cmd_resize_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"resize-pane",
        alias: Some(c"resizep"),
        args: args_parse {
            template: c"TZt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-TZ] [-t target-pane]",
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
        exec: Some(cmd_resize_pane_exec),
    }
};
unsafe fn cmd_resize_pane_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let pane_owner = target.wp.upgrade().expect("live resize target pane");
    if args_has(args, b'Z') != 0 {
        let window_owner = target.w.upgrade().expect("live resize target window");
        let toggled = window_owner.layout_action(&pane_owner, LayoutAction::ToggleZoom);
        drop(window_owner);
        if let Err(cause) = toggled {
            cmdq_error(item_handle, |out| out.write_all(cause.to_bytes()));
            return CmdOutcome::Error;
        }
    }
    if args_has(args, b'T') != 0 {
        pane_owner.trim_history();
    }
    CmdOutcome::Normal
}
