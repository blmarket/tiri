use crate::src::arguments::{args, args_has, args_parse};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::resize::recalculate_sizes;
use crate::src::server::marked_pane;
use crate::src::server_fn::server_redraw_session_group;
use crate::src::session::session_group_synchronize_from;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_FIND_DEFAULT_MARKED};
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::winlink;
use crate::src::window::Window as _;

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
            flag: 's' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: CMD_FIND_DEFAULT_MARKED,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_swap_window_exec),
    }
};
unsafe fn cmd_swap_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut source: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_source_mut(&mut *item);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut src: Option<SessionRef> = (*source).session_handle();
    let mut dst: Option<SessionRef> = (*target).session_handle();
    let mut sg_src = crate::src::shared::session::SessionGroupWeak::new();
    let mut sg_dst = crate::src::shared::session::SessionGroupWeak::new();
    let mut wl_src: refbox::Weak<winlink> = (*source).winlink_handle();
    let mut wl_dst: refbox::Weak<winlink> = (*target).winlink_handle();
    sg_src = crate::src::session::session_groups::for_session(
        &src.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    sg_dst = crate::src::session::session_groups::for_session(
        &dst.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    if !crate::src::shared::rc::same(src.as_ref(), dst.as_ref())
        && sg_src.is_alive()
        && sg_dst.is_alive()
        && sg_src == sg_dst
    {
        cmdq_error(item_handle, |out| {
            out.write_all(b"can't move window, sessions are grouped")
        });
        return CmdOutcome::Error;
    }
    if crate::src::shared::rc::same(
        wl_dst.get_unchecked().window_handle(),
        wl_src.get_unchecked().window_handle(),
    ) {
        return CmdOutcome::Normal;
    }
    wl_dst
        .get_unchecked()
        .window_handle()
        .expect("linked window")
        .remove_winlink(wl_dst.clone());
    wl_src
        .get_unchecked()
        .window_handle()
        .expect("linked window")
        .remove_winlink(wl_src.clone());
    if wl_dst != wl_src {
        std::mem::swap(
            &mut wl_dst.get_mut_unchecked().window_owner,
            &mut wl_src.get_mut_unchecked().window_owner,
        );
    }
    wl_dst
        .get_unchecked()
        .window_handle()
        .expect("linked window")
        .add_winlink(wl_dst.clone());
    wl_src
        .get_unchecked()
        .window_handle()
        .expect("linked window")
        .add_winlink(wl_src.clone());
    if marked_pane.winlink_handle() == wl_src {
        marked_pane.set_wl((wl_dst).clone());
    }
    if args_has(&*args, 'd' as i32 as u_char) != 0 {
        (dst.as_ref().expect("live session")).select_index(wl_dst.get_unchecked().idx);
        if !crate::src::shared::rc::same(src.as_ref(), dst.as_ref()) {
            (src.as_ref().expect("live session")).select_index(wl_src.get_unchecked().idx);
        }
    }
    session_group_synchronize_from(src.as_ref().expect("live session"));
    server_redraw_session_group(src.as_ref().expect("live session"));
    if !crate::src::shared::rc::same(src.as_ref(), dst.as_ref()) {
        session_group_synchronize_from(dst.as_ref().expect("live session"));
        server_redraw_session_group(dst.as_ref().expect("live session"));
    }
    recalculate_sizes();
    CmdOutcome::Normal
}
