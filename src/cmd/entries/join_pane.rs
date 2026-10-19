use crate::arguments::{args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_state_owned};
use crate::events::events_fire_window;
use crate::resize::recalculate_sizes;
use crate::server_client::Client as _;
use crate::server_fn::{
    server_kill_window, server_redraw_session, server_redraw_window, server_status_session,
};
use crate::window::Window as _;

use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_FIND_DEFAULT_MARKED;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::session::SessionRef;
use crate::shared::window::WindowRef;

use crate::shared::window::winlink;
use crate::window_pane::WindowPane as _;
pub static cmd_join_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"join-pane",
        alias: Some(c"joinp"),
        args: args_parse {
            template: c"bds:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-bd] [-s src-pane] [-t dst-pane]",
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
        exec: Some(cmd_join_pane_exec),
    }
};
unsafe fn cmd_join_pane_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let current = cmdq_get_state_owned(&*(item));
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut source_state = crate::cmd::queue::cmdq_get_source(&item).clone();
    let mut source: *mut cmd_find_state = &mut source_state;
    let mut dst_s: Option<SessionRef> = None;
    let mut src_wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut dst_wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut dst_idx: ::core::ffi::c_int = 0;
    dst_s = (*target).session_handle();
    dst_wl = (*target).winlink_handle();
    let dst_pane_owner = (*target).pane_handle().expect("join target pane");
    let dst_window = dst_wl
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("live window");
    let mut src_window = None;
    let result = (|| {
        dst_idx = dst_wl.get_unchecked().idx;
        src_wl = (*source).winlink_handle();
        let src_pane_owner = (*source).pane_handle().expect("join source pane");
        src_window = src_wl.get_unchecked().window_handle().cloned();
        let src_owner = src_window.as_ref().expect("live source window");
        if std::rc::Rc::ptr_eq(&src_pane_owner, &dst_pane_owner) {
            cmdq_error(item_handle, |out| {
                out.write_all(b"source and target panes must be different")
            });
            return CmdOutcome::Error;
        }
        let before = args_has(args, 'b' as i32 as u_char) != 0;
        let same_window = WindowRef::ptr_eq(src_owner, &dst_window);
        let mut order = dst_window
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|pane| !std::rc::Rc::ptr_eq(pane, &src_pane_owner))
            .collect::<Vec<_>>();
        let target = order
            .iter()
            .position(|pane| std::rc::Rc::ptr_eq(pane, &dst_pane_owner))
            .expect("join target belongs to window");
        order.insert(target + usize::from(!before), src_pane_owner.clone());
        if !same_window {
            src_pane_owner.reparent(&dst_window);
        }
        if let Err(reason) = dst_window.rearrange_panes(&order) {
            src_pane_owner.reparent(src_owner);
            cmdq_error(item_handle, |out| out.write_all(reason.to_bytes()));
            return CmdOutcome::Error;
        }
        ClientRef::forget_pane(&src_pane_owner);
        if !same_window {
            let rest = src_owner
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
                .filter(|pane| !std::rc::Rc::ptr_eq(pane, &src_pane_owner))
                .collect::<Vec<_>>();
            src_owner
                .rearrange_panes(&rest)
                .expect("a removal is never refused");
            dst_window.refit();
        }
        src_pane_owner.refresh_palette();
        recalculate_sizes();
        server_redraw_window(src_owner);
        server_redraw_window(&dst_window);
        if args_has(args, 'd' as i32 as u_char) == 0 {
            dst_window.select_pane(&src_pane_owner, true);
            (dst_s.as_ref().expect("live session")).select_index(dst_idx);
            *current.current.borrow_mut() = cmd_find_state::find_from_session(
                dst_s.as_ref().expect("live session"),
                0 as ::core::ffi::c_int,
            );
            server_redraw_session(dst_s.as_ref().expect("live session"));
        } else {
            server_status_session(dst_s.as_ref().expect("live session"));
        }
        src_pane_owner.notify_moved(src_owner, src_wl.get_unchecked().idx, &dst_window, dst_idx);
        cmd_join_pane_finish(src_window.take().expect("live source window"), &dst_window);
        CmdOutcome::Normal
    })();
    if let Some(window) = src_window {
        drop(window);
    }
    drop(dst_window);
    result
}

unsafe fn cmd_join_pane_finish(source: WindowRef, destination: &WindowRef) {
    if source.pane_snapshot().is_empty() {
        server_kill_window(source, 1);
    } else {
        events_fire_window(c"window-layout-changed", source);
    }
    events_fire_window(c"window-layout-changed", WindowRef::clone(destination));
}
