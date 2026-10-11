use crate::src::arguments::args_has;
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_get_state_owned;
use crate::src::server_fn::server_redraw_window;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::window::winlink;
use crate::src::shared::window::WindowRef;
use crate::src::window::Window as _;
use std::rc::Rc;
pub static cmd_rotate_window_entry: cmd_entry = {
    cmd_entry {
        name: c"rotate-window",
        alias: Some(c"rotatew"),
        args: args_parse {
            template: c"Dt:U",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-DU] [-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_rotate_window_exec),
    }
};

unsafe fn cmd_rotate_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let current = cmdq_get_state_owned(&*(item));
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let window_owner = (*target).w.upgrade().expect("live rotation window");
    let result = {
        let down = args_has(args, b'D') != 0;
        let mut order = window_owner.pane_snapshot();
        // Down moves the last pane to the front; up the first to the end.
        if down {
            order.rotate_right(1);
        } else {
            order.rotate_left(1);
        }
        window_owner
            .rearrange_panes(&order)
            .expect("a reorder is never refused");
        // Selection follows the active pane's old position in the strip.
        let selected_pane = window_owner
            .active_pane()
            .and_then(|pane| window_owner.step_pane(Some(&Rc::downgrade(&pane)), down))
            .or_else(|| window_owner.step_pane(None, down))
            .expect("rotation window has an active candidate");
        WindowRef::clone(&(window_owner)).select_pane(&selected_pane, true);
        *current.current.borrow_mut() = cmd_find_state::find_from_winlink_pane(
            wl.clone(),
            &selected_pane,
            0 as ::core::ffi::c_int,
        );
        server_redraw_window(&(window_owner));
        CMD_RETURN_NORMAL
    };
    drop(window_owner);
    result
}
