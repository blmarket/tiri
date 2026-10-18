use crate::src::arguments::{args, args_get, args_has, args_parse};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::src::format::{
    format_create_with_client, format_defaults, format_expand_cstring, format_true,
};
use crate::src::server_client::Client as _;
use crate::src::shared::client::ClientRef;
use crate::src::shared::window::WindowRef;
use std::ffi::CStr;

use crate::src::server_fn::{server_kill_pane, server_redraw_window};
use crate::src::shared::abi::*;
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::winlink;
use crate::src::window::Window;
pub static cmd_kill_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"kill-pane",
        alias: Some(c"killp"),
        args: args_parse {
            template: c"af:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-a] [-f filter] [-t target-pane]",
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
        exec: Some(cmd_kill_pane_exec),
    }
};
unsafe fn cmd_kill_pane_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let pane_owner = (*target).wp.upgrade();
    let mut filter: Option<&CStr> = args_get(&*(args), 'f' as i32 as u_char);
    if filter.is_some() && args_has(&*args, 'a' as i32 as u_char) == 0 {
        cmdq_error(item_handle, |out| out.write_all(b"-f only valid with -a"));
        return CmdOutcome::Error;
    }
    if args_has(&*args, 'a' as i32 as u_char) != 0 {
        return cmd_kill_pane_all(item_handle, filter);
    }
    let Some(pane_owner) = pane_owner else {
        cmdq_error(item_handle, |out| out.write_all(b"no active pane to kill"));
        return CmdOutcome::Error;
    };
    server_kill_pane(&pane_owner);
    CmdOutcome::Normal
}
unsafe fn cmd_kill_pane_all(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    filter: Option<&CStr>,
) -> CmdOutcome {
    let item = item_handle.get();
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let target_pane = (*target).pane_handle().expect("kill target pane");
    let panes = wl
        .get_unchecked()
        .window_handle()
        .map(|window| window.pane_snapshot())
        .unwrap_or_default();
    for pane_owner in panes.iter().filter_map(std::rc::Weak::upgrade) {
        if !std::rc::Rc::ptr_eq(&pane_owner, &target_pane)
            && cmd_kill_pane_filter(
                item_handle,
                s.as_ref().expect("live session"),
                wl.clone(),
                &pane_owner,
                filter,
            ) != 0
        {
            ClientRef::forget_pane(&pane_owner);
            WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window"))
                .remove_pane(&pane_owner);
        }
    }
    server_redraw_window((wl.get_unchecked().window_handle().as_ref()).expect("live window"));
    CmdOutcome::Normal
}
unsafe fn cmd_kill_pane_filter(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let item = item_handle.get();

    let queue_client = cmdq_get_client((item).as_ref());
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut flag: ::core::ffi::c_int = 0;
    if filter.is_null() {
        return 1 as ::core::ffi::c_int;
    }
    let mut ft_owner = format_create_with_client(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    ft = &raw mut *ft_owner;
    format_defaults(ft, None, Some(s_owner), wl.clone(), Some(pane_owner));
    let expanded = format_expand_cstring(ft, filter);
    flag = format_true(expanded.as_ptr());
    drop(ft_owner);
    flag
}
