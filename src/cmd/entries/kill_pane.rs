use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::format::{format_create, format_defaults, format_expand_cstring, format_true};
use crate::server_client::Client as _;
use crate::shared::client::ClientRef;
use crate::shared::window::WindowRef;
use std::ffi::CStr;

use crate::server_fn::{server_kill_pane, server_redraw_window, server_unzoom_window};
use crate::shared::abi::*;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::format::FORMAT_NONE;
use crate::shared::pane::window_pane;
use crate::shared::session::SessionRef;
use crate::shared::window::winlink;
use crate::window::Window;
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
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let pane_owner = target.wp.upgrade();
    let mut filter: Option<&CStr> = args_get(args, 'f' as i32 as u_char);
    if filter.is_some() && args_has(args, 'a' as i32 as u_char) == 0 {
        cmdq_error(item_handle, |out| out.write_all(b"-f only valid with -a"));
        return CmdOutcome::Error;
    }
    if args_has(args, 'a' as i32 as u_char) != 0 {
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
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    filter: Option<&CStr>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let mut s: Option<SessionRef> = target.session_handle();
    let mut wl: refbox::Weak<winlink> = target.winlink_handle();
    let target_pane = target.pane_handle().expect("kill target pane");
    server_unzoom_window((wl.get_unchecked().window_handle().as_ref()).expect("live window"));
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
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let item = item_handle.borrow();

    let queue_client = cmdq_get_client(Some(&*item));
    let Some(filter) = filter else {
        return 1 as ::core::ffi::c_int;
    };
    let mut ft_owner = format_create(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, None, Some(s_owner), wl.clone(), Some(pane_owner));
    let expanded = format_expand_cstring(ft, filter);
    let flag = format_true(&expanded);
    drop(ft_owner);
    ::core::ffi::c_int::from(flag)
}
