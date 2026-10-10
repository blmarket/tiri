use crate::src::arguments::args_has;
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::window::{LayoutAction, Window as _};
use crate::src::window_pane::WindowPane as _;

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
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let pane_owner = (*target).wp.upgrade().expect("live resize target pane");
    if args_has(args, b'Z') != 0 {
        let window_owner = (*target).w.upgrade().expect("live resize target window");
        let toggled = window_owner.layout_action(&pane_owner, LayoutAction::ToggleZoom);
        drop(window_owner);
        if let Err(cause) = toggled {
            cmdq_error(item_handle, |out| out.write_all(cause.to_bytes()));
            return CMD_RETURN_ERROR;
        }
    }
    if args_has(args, b'T') != 0 {
        pane_owner.trim_history();
    }
    CMD_RETURN_NORMAL
}
