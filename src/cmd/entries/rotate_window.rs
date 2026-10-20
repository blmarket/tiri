use crate::arguments::{args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_get_state_owned;
use crate::server_fn::server_redraw_window;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::window::winlink;
use crate::shared::window::WindowRef;
use crate::window::Window as _;
pub static cmd_rotate_window_entry: cmd_entry = {
    cmd_entry {
        name: c"rotate-window",
        alias: Some(c"rotatew"),
        args: args_parse {
            template: c"Dt:UZ",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-DUZ] [-t target-window]",
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
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let current = cmdq_get_state_owned(&*(item));
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let mut wl: refbox::Weak<winlink> = target.winlink_handle();
    let window_owner = target.w.upgrade().expect("live rotation window");
    let result = {
        let down = args_has(args, b'D') != 0;
        window_owner.push_zoom(false, args_has(args, b'Z') != 0);
        let mut order = window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .collect::<Vec<_>>();
        if down {
            order.rotate_right(1);
        } else {
            order.rotate_left(1);
        }
        window_owner
            .rearrange_panes(&order)
            .expect("a reorder is never refused");
        let selected_pane = window_owner
            .step_pane(Some(&window_owner.active_pane()), down)
            .upgrade()
            .or_else(|| window_owner.step_pane(None, down).upgrade())
            .expect("rotation window has an active candidate");
        WindowRef::clone(&(window_owner)).select_pane(&selected_pane, true);
        *current.current.borrow_mut() = cmd_find_state::find_from_winlink_pane(
            wl.clone(),
            &selected_pane,
            0 as ::core::ffi::c_int,
        );
        window_owner.pop_zoom();
        server_redraw_window(&(window_owner));
        CmdOutcome::Normal
    };
    drop(window_owner);
    result
}
