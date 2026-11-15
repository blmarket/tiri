use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::CMD_FIND_WINDOW;
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_DEFAULT_MARKED};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_has, args_parse};
use tiri_core::resize::recalculate_sizes;
use tiri_core::server::marked_pane;
use tiri_core::server_fn::server_redraw_session_group;
use tiri_core::session::Session;
use tiri_core::session::session_group_synchronize_from;
use tiri_core::shared::abi::u_char;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::window::winlink;
use tiri_core::window::Window as _;
use tiri_core::winlink::{winlink_index, winlink_window};

pub static cmd_swap_window_entry: cmd_entry = {
    cmd_entry {
        name: c"swap-window",
        alias: Some(c"swapw"),
        args: args_parse {
            template: c"ds:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-d] [-s src-window] [-t dst-window]",
        source: cmd_entry_flag {
            flag: b's',
            type_0: CMD_FIND_WINDOW,
            flags: CMD_FIND_DEFAULT_MARKED,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_swap_window_exec),
    }
};
fn cmd_swap_window_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let source = inv.source().clone();
    let target = inv.target().clone();
    let mut src: Option<SessionRef> = source.session_handle();
    let mut dst: Option<SessionRef> = target.session_handle();
    let mut sg_src = tiri_core::refbox::Weak::new();
    let mut sg_dst = tiri_core::refbox::Weak::new();
    let mut wl_src: tiri_core::refbox::Weak<winlink> = source.winlink_handle();
    let mut wl_dst: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    sg_src = tiri_core::session::session_groups::for_session(
        &src.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    sg_dst = tiri_core::session::session_groups::for_session(
        &dst.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    if !tiri_core::shared::rc::same(src.as_ref(), dst.as_ref())
        && sg_src.is_alive()
        && sg_dst.is_alive()
        && sg_src == sg_dst
    {
        inv.error( |out| {
            out.write_all(b"can't move window, sessions are grouped")
        });
        return ExecOutcome::err();
    }
    if tiri_core::shared::rc::same(
        winlink_window(&wl_dst).as_ref(),
        winlink_window(&wl_src).as_ref(),
    ) {
        return ExecOutcome::ok();
    }
    winlink_window(&wl_dst)
        .expect("linked window")
        .remove_winlink(wl_dst.clone());
    winlink_window(&wl_src)
        .expect("linked window")
        .remove_winlink(wl_src.clone());
    if wl_dst != wl_src {
        std::mem::swap(
            &mut wl_dst.try_borrow_mut().expect("live winlink").window_owner,
            &mut wl_src.try_borrow_mut().expect("live winlink").window_owner,
        );
    }
    winlink_window(&wl_dst)
        .expect("linked window")
        .add_winlink(wl_dst.clone());
    winlink_window(&wl_src)
        .expect("linked window")
        .add_winlink(wl_src.clone());
    marked_pane.with_borrow_mut(|marked| {
        if marked.winlink_handle() == wl_src {
            marked.set_wl((wl_dst).clone());
        }
    });
    if args_has(args, 'd' as i32 as u_char) != 0 {
        (dst.as_ref().expect("live session")).select_index(winlink_index(&wl_dst));
        if !tiri_core::shared::rc::same(src.as_ref(), dst.as_ref()) {
            (src.as_ref().expect("live session")).select_index(winlink_index(&wl_src));
        }
    }
    session_group_synchronize_from(src.as_ref().expect("live session"));
    server_redraw_session_group(src.as_ref().expect("live session"));
    if !tiri_core::shared::rc::same(src.as_ref(), dst.as_ref()) {
        session_group_synchronize_from(dst.as_ref().expect("live session"));
        server_redraw_session_group(dst.as_ref().expect("live session"));
    }
    recalculate_sizes();
    ExecOutcome::ok()
})
}
