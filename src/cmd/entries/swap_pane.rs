use crate::src::arguments::{args, args_has, args_parse};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::events::events_fire_window;
use crate::src::server_client::Client as _;
use crate::src::shared::client::ClientRef;
use crate::src::window::Window as _;

use crate::src::server_fn::server_redraw_window;
use crate::src::shared::abi::*;
use crate::src::shared::command::CMD_FIND_DEFAULT_MARKED;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::window::WindowRef;
use crate::src::window_pane::WindowPane as _;
use std::rc::Rc;
pub static cmd_swap_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"swap-pane",
        alias: Some(c"swapp"),
        args: args_parse {
            template: c"dDs:t:U",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-dDU] [-s src-pane] [-t dst-pane]",
        source: cmd_entry_flag {
            flag: 's' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_DEFAULT_MARKED,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_swap_pane_exec),
    }
};
unsafe fn cmd_swap_pane_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut source: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_source_mut(&mut *item);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut src_idx: ::core::ffi::c_int = 0;
    let mut dst_idx: ::core::ffi::c_int = 0;
    let dst_window_owner = (*target).w.upgrade().expect("live swap target window");
    let dst_pane_owner = (*target).wp.upgrade().expect("live swap target pane");

    dst_idx = ((*target).winlink_handle()).get_unchecked().idx;
    let mut src_window_owner = (*source).w.upgrade().expect("live swap source window");
    let result = (|| {
        let mut src_pane_owner = (*source).wp.upgrade().expect("live swap source pane");

        src_idx = ((*source).winlink_handle()).get_unchecked().idx;
        if args_has(args, 'D' as i32 as u_char) != 0 {
            let previous = std::mem::replace(&mut src_window_owner, dst_window_owner.clone());
            drop(previous);

            src_pane_owner = dst_window_owner
                .step_pane(Some(&Rc::downgrade(&dst_pane_owner)), false)
                .upgrade()
                .unwrap_or_else(|| dst_pane_owner.clone());
        } else if args_has(args, 'U' as i32 as u_char) != 0 {
            let previous = std::mem::replace(&mut src_window_owner, dst_window_owner.clone());
            drop(previous);

            src_pane_owner = dst_window_owner
                .step_pane(Some(&Rc::downgrade(&dst_pane_owner)), true)
                .upgrade()
                .unwrap_or_else(|| dst_pane_owner.clone());
        }
        if !Rc::ptr_eq(&src_pane_owner, &dst_pane_owner) {
            ClientRef::forget_pane(&src_pane_owner);
            ClientRef::forget_pane(&dst_pane_owner);
            src_pane_owner.reparent(&dst_window_owner);
            dst_pane_owner.reparent(&src_window_owner);
            let swapped = |window: &WindowRef| {
                window
                    .pane_snapshot()
                    .iter()
                    .filter_map(std::rc::Weak::upgrade)
                    .map(|pane| {
                        if Rc::ptr_eq(&pane, &src_pane_owner) {
                            dst_pane_owner.clone()
                        } else if Rc::ptr_eq(&pane, &dst_pane_owner) {
                            src_pane_owner.clone()
                        } else {
                            pane
                        }
                    })
                    .collect::<Vec<_>>()
            };
            if WindowRef::ptr_eq(&src_window_owner, &dst_window_owner) {
                dst_window_owner
                    .rearrange_panes(&swapped(&dst_window_owner))
                    .expect("a reorder is never refused");
            } else {
                let dst_before = dst_window_owner
                    .pane_snapshot()
                    .iter()
                    .filter_map(std::rc::Weak::upgrade)
                    .collect::<Vec<_>>();
                let src_order = swapped(&src_window_owner);
                let refused = dst_window_owner
                    .rearrange_panes(&swapped(&dst_window_owner))
                    .err()
                    .or_else(|| {
                        let refused = src_window_owner.rearrange_panes(&src_order).err();
                        if refused.is_some() {
                            dst_pane_owner.reparent(&dst_window_owner);
                            dst_window_owner
                                .rearrange_panes(&dst_before)
                                .expect("the window took this list before");
                        }
                        refused
                    });
                if let Some(reason) = refused {
                    src_pane_owner.reparent(&src_window_owner);
                    dst_pane_owner.reparent(&dst_window_owner);
                    cmdq_error(item_handle, |out| out.write_all(reason.to_bytes()));
                    return CMD_RETURN_ERROR;
                }
            }
            if args_has(args, 'd' as i32 as u_char) == 0 {
                if !WindowRef::ptr_eq(&src_window_owner, &dst_window_owner) {
                    src_window_owner.select_pane(&dst_pane_owner, true);
                    WindowRef::clone(&(dst_window_owner)).select_pane(&src_pane_owner, true);
                } else {
                    src_window_owner.select_pane(&dst_pane_owner, true);
                }
            } else {
                if src_window_owner
                    .active_pane()
                    .ptr_eq(&Rc::downgrade(&src_pane_owner))
                {
                    src_window_owner.select_pane(&dst_pane_owner, true);
                }
                if dst_window_owner
                    .active_pane()
                    .ptr_eq(&Rc::downgrade(&dst_pane_owner))
                {
                    WindowRef::clone(&(dst_window_owner)).select_pane(&src_pane_owner, true);
                }
            }
            if !WindowRef::ptr_eq(&src_window_owner, &dst_window_owner) {
                src_window_owner
                    .borrow_pane_history_mut()
                    .remove(&Rc::downgrade(&src_pane_owner));
                dst_window_owner
                    .borrow_pane_history_mut()
                    .remove(&Rc::downgrade(&dst_pane_owner));
                src_pane_owner.refresh_palette();
                dst_pane_owner.refresh_palette();
                server_redraw_window(&src_window_owner);
            }
            server_redraw_window(&(dst_window_owner));
            if !WindowRef::ptr_eq(&src_window_owner, &dst_window_owner) {
                src_pane_owner.notify_moved(
                    &src_window_owner,
                    src_idx,
                    &WindowRef::clone(&(dst_window_owner)),
                    dst_idx,
                );
                dst_pane_owner.notify_moved(
                    &WindowRef::clone(&(dst_window_owner)),
                    dst_idx,
                    &src_window_owner,
                    src_idx,
                );
            }
            if WindowRef::ptr_eq(&src_window_owner, &dst_window_owner) {
                events_fire_window(
                    c"window-layout-changed",
                    WindowRef::clone(&src_window_owner),
                );
            }
        }
        CMD_RETURN_NORMAL
    })();
    drop(src_window_owner);
    drop(dst_window_owner);
    result
}
