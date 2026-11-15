use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use tiri_core::arguments::{args_has, args_parse};
use tiri_core::events::events_fire_window;
use tiri_core::resize::recalculate_sizes;
use tiri_core::server_client::Client as _;
use tiri_core::server_fn::{
    server_kill_window, server_redraw_session, server_redraw_window, server_status_session,
    server_unzoom_window,
};
use tiri_core::window::Window as _;
use tiri_core::winlink::winlink_window;

use crate::shared::command::CMD_FIND_DEFAULT_MARKED;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::session::Session;
use tiri_core::shared::abi::u_char;
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::shared::window::WindowRef;

use tiri_core::shared::window::winlink;
use tiri_core::window_pane::WindowPane as _;
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
            flag: b's',
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_DEFAULT_MARKED,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_join_pane_exec),
    }
};
fn cmd_join_pane_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
    let source = inv.source().clone();
    let mut dst_s: Option<SessionRef> = None;
    let mut src_wl: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    let mut dst_wl: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    let mut dst_idx: ::core::ffi::c_int = 0;
    dst_s = target.session_handle();
    dst_wl = target.winlink_handle();
    let dst_pane_owner = target.pane_handle().expect("join target pane");
    let dst_window = winlink_window(&dst_wl).expect("live window");
    let mut src_window = None;
    let result = (|| {
        dst_idx = dst_wl.try_borrow().expect("live winlink").idx;
        src_wl = source.winlink_handle();
        let src_pane_owner = source.pane_handle().expect("join source pane");
        src_window = winlink_window(&src_wl);
        let src_owner = src_window.as_ref().expect("live source window");
        server_unzoom_window(&dst_window);
        server_unzoom_window(src_owner);
        if std::rc::Rc::ptr_eq(&src_pane_owner, &dst_pane_owner) {
            inv.error( |out| {
                out.write_all(b"source and target panes must be different")
            });
            return ExecOutcome::err();
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
            inv.error( |out| out.write_all(reason.to_bytes()));
            return ExecOutcome::err();
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
            inv.context_mut().current = cmd_find_state::from_session(
                dst_s.as_ref().expect("live session"),
                0 as ::core::ffi::c_int,
            );
            server_redraw_session(dst_s.as_ref().expect("live session"));
        } else {
            server_status_session(dst_s.as_ref().expect("live session"));
        }
        let src_idx = src_wl.try_borrow().expect("live winlink").idx;
        src_pane_owner.notify_moved(src_owner, src_idx, &dst_window, dst_idx);
        cmd_join_pane_finish(src_window.take().expect("live source window"), &dst_window);
        ExecOutcome::ok()
    })();
    if let Some(window) = src_window {
        drop(window);
    }
    drop(dst_window);
    result
})
}

fn cmd_join_pane_finish(source: WindowRef, destination: &WindowRef) {
    if source.pane_snapshot().is_empty() {
        server_kill_window(source, 1);
    } else {
        events_fire_window(c"window-layout-changed", source);
    }
    events_fire_window(c"window-layout-changed", WindowRef::clone(destination));
}
