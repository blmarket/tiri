use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use std::ffi::CStr;
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::format::{format_create, format_defaults, format_expand_cstring, format_true};
use tiri_core::resize::recalculate_sizes;
use tiri_core::server_fn::{server_kill_window, server_renumber_all, server_unlink_window};
use tiri_core::winlink::winlink_window;

use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_WINDOW};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::session::Session;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::tree::RB_NEGINF;
use tiri_core::shared::window::winlink;
use tiri_core::window::{winlink_next, winlinks_minmax, winlinks_next, winlinks_prev};
pub static cmd_kill_window_entry: cmd_entry = {
    cmd_entry {
        name: c"kill-window",
        alias: Some(c"killw"),
        args: args_parse {
            template: c"af:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-a] [-f filter] [-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_kill_window_exec),
    }
};
pub static cmd_unlink_window_entry: cmd_entry = {
    cmd_entry {
        name: c"unlink-window",
        alias: Some(c"unlinkw"),
        args: args_parse {
            template: c"kt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-k] [-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_kill_window_exec),
    }
};
fn cmd_kill_window_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let mut s: Option<SessionRef> = target.session_handle();
    let mut filter: Option<&CStr> = args_get(args, 'f' as i32 as u_char);
    if filter.is_some() && args_has(args, 'a' as i32 as u_char) == 0 {
        inv.error( |out| out.write_all(b"-f only valid with -a"));
        return ExecOutcome::err();
    }
    if std::ptr::eq(cmd_get_entry(self_0), &cmd_unlink_window_entry) {
        if args_has(args, 'k' as i32 as u_char) == 0
            && (tiri_core::shared::session::SessionRef::window_linked_outside_group(
                s.as_ref(),
                &winlink_window(&wl).expect("live window"),
            ) as i32)
                == 0
        {
            inv.error( |out| {
                out.write_all(b"window only linked to one session")
            });
            return ExecOutcome::err();
        }
        server_unlink_window(s.as_ref().expect("live session"), wl.clone());
        recalculate_sizes();
        return ExecOutcome::ok();
    }
    if args_has(args, 'a' as i32 as u_char) != 0 {
        return cmd_kill_window_all(inv, filter);
    }
    server_kill_window(
        winlink_window(&wl).expect("winlink window"),
        1 as ::core::ffi::c_int,
    );
    ExecOutcome::ok()
})
}
fn cmd_kill_window_all(inv: &Invocation<'_>, filter: Option<&CStr>) -> ExecOutcome {
    let target = inv.target().clone();
    let mut s: Option<SessionRef> = target.session_handle();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let mut loop_0: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    let mut found: u_int = 0;
    let mut kill_current: u_int = 0;
    if !winlinks_prev(&wl.try_borrow().expect("live winlink")).is_alive()
        && !winlinks_next(&wl.try_borrow().expect("live winlink")).is_alive()
    {
        return ExecOutcome::ok();
    }
    loop {
        found = 0 as u_int;
        loop_0 = s
            .as_ref()
            .expect("live session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while loop_0.is_alive() {
            if !tiri_core::shared::rc::same(
                winlink_window(&loop_0).as_ref(),
                winlink_window(&wl).as_ref(),
            ) && cmd_kill_window_filter(
                inv,
                s.as_ref().expect("live session"),
                (loop_0).clone(),
                filter,
            ) != 0
            {
                server_kill_window(
                    winlink_window(&loop_0).expect("winlink window"),
                    0 as ::core::ffi::c_int,
                );
                found = found.wrapping_add(1);
                break;
            } else {
                loop_0 = winlink_next(&loop_0);
            }
        }
        if !(found != 0 as u_int) {
            break;
        }
    }
    kill_current = 0 as u_int;
    found = kill_current;
    loop_0 = s
        .as_ref()
        .expect("live session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while loop_0.is_alive() {
        if tiri_core::shared::rc::same(
            winlink_window(&loop_0).as_ref(),
            winlink_window(&wl).as_ref(),
        ) {
            found = found.wrapping_add(1);
            if cmd_kill_window_filter(
                inv,
                s.as_ref().expect("live session"),
                (loop_0).clone(),
                filter,
            ) != 0
            {
                kill_current = 1 as u_int;
            }
        }
        loop_0 = winlink_next(&loop_0);
    }
    if kill_current != 0 && found > 1 as u_int {
        server_kill_window(
            winlink_window(&wl).expect("winlink window"),
            0 as ::core::ffi::c_int,
        );
    }
    server_renumber_all();
    ExecOutcome::ok()
}
fn cmd_kill_window_filter(
    inv: &Invocation<'_>,
    s_owner: &SessionRef,
    mut wl: tiri_core::refbox::Weak<winlink>,
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
    format_defaults(ft, None, Some(s_owner), wl.clone(), None);
    let expanded = format_expand_cstring(ft, filter);
    let flag = format_true(&expanded);
    drop(ft_owner);
    ::core::ffi::c_int::from(flag)
}
