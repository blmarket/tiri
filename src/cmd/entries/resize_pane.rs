use crate::arguments::{args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::server_fn::{server_redraw_window, server_unzoom_window};
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::window::{LayoutAction, Window as _};
use crate::window_pane::WindowPane as _;

/// Panes take their size from the layout; -Z zooms the window to a pane or
/// unzooms it, -W asks the layout to switch a pane to its other width (in the
/// strip, half or full), and -T trims history.
pub static cmd_resize_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"resize-pane",
        alias: Some(c"resizep"),
        args: args_parse {
            template: c"TWZt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-TWZ] [-t target-pane]",
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
        exec: Some(cmd_resize_pane_exec),
    }
};
fn cmd_resize_pane_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let pane_owner = target.wp.upgrade().expect("live resize target pane");
    if args_has(args, b'Z') != 0 && args_has(args, b'T') == 0 {
        let window_owner = target.w.upgrade().expect("live resize target window");
        if window_owner.zoomed_pane().strong_count() != 0 {
            window_owner.unzoom(true);
        } else {
            window_owner.zoom(&pane_owner);
        }
        server_redraw_window(&window_owner);
        return CmdOutcome::Normal;
    }
    if args_has(args, b'T') == 0 {
        server_unzoom_window(&target.w.upgrade().expect("live resize target window"));
    }
    if args_has(args, b'W') != 0 {
        let window_owner = target.w.upgrade().expect("live resize target window");
        let toggled = window_owner.layout_action(&pane_owner, LayoutAction::ToggleWidth);
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
