use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use std::ffi::CStr;
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::format::{format_create, format_defaults, format_expand_cstring, format_true};
use tiri_core::server_client::Client as _;
use tiri_core::shared::client::ClientRef;
use tiri_core::winlink::winlink_window;

use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::server_fn::{server_kill_pane, server_redraw_window, server_unzoom_window};
use tiri_core::shared::abi::u_char;
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::pane::window_pane;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::window::winlink;
use tiri_core::window::Window;
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
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_kill_pane_exec),
    }
};
fn cmd_kill_pane_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
    let pane_owner = target.wp.upgrade();
    let mut filter: Option<&CStr> = args_get(args, 'f' as i32 as u_char);
    if filter.is_some() && args_has(args, 'a' as i32 as u_char) == 0 {
        inv.error( |out| out.write_all(b"-f only valid with -a"));
        return ExecOutcome::err();
    }
    if args_has(args, 'a' as i32 as u_char) != 0 {
        return cmd_kill_pane_all(inv, filter);
    }
    let Some(pane_owner) = pane_owner else {
        inv.error( |out| out.write_all(b"no active pane to kill"));
        return ExecOutcome::err();
    };
    server_kill_pane(&pane_owner);
    ExecOutcome::ok()
})
}
fn cmd_kill_pane_all(inv: &Invocation<'_>, filter: Option<&CStr>) -> ExecOutcome {
    let target = inv.target().clone();
    let mut s: Option<SessionRef> = target.session_handle();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let target_pane = target.pane_handle().expect("kill target pane");
    server_unzoom_window((winlink_window(&wl).as_ref()).expect("live window"));
    let panes = winlink_window(&wl)
        .map(|window| window.pane_snapshot())
        .unwrap_or_default();
    for pane_owner in panes.iter().filter_map(std::rc::Weak::upgrade) {
        if !std::rc::Rc::ptr_eq(&pane_owner, &target_pane)
            && cmd_kill_pane_filter(
                inv,
                s.as_ref().expect("live session"),
                wl.clone(),
                &pane_owner,
                filter,
            ) != 0
        {
            ClientRef::forget_pane(&pane_owner);
            winlink_window(&wl)
                .expect("live window")
                .remove_pane(&pane_owner);
        }
    }
    server_redraw_window((winlink_window(&wl).as_ref()).expect("live window"));
    ExecOutcome::ok()
}
fn cmd_kill_pane_filter(
    inv: &Invocation<'_>,
    s_owner: &SessionRef,
    mut wl: tiri_core::refbox::Weak<winlink>,
    pane_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {

    let queue_client = inv.client();
    let Some(filter) = filter else {
        return 1 as ::core::ffi::c_int;
    };
    let cf = inv.format();
    let mut ft_owner = format_create(
        queue_client.as_ref(),
        Some(&cf),
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
