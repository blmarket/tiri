use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{
    format_add, format_create, format_defaults, format_expand_cstring, format_true,
};
use tiri_core::session::sessions;
use tiri_core::winlink::winlink_window;

use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_WINDOW};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::session::Session;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::sort::sort_criteria;
use tiri_core::shared::sort::{SORT_ACTIVITY, SORT_END, sort_order};
use tiri_core::shared::tree::RB_NEGINF;
use tiri_core::shared::window::winlink;
use tiri_core::sort::{sort_get_panes_window, sort_order_from_string};
use tiri_core::window::{winlink_next, winlinks_minmax};
pub static cmd_list_panes_entry: cmd_entry = {
    cmd_entry {
        name: c"list-panes",
        alias: Some(c"lsp"),
        args: args_parse {
            template: c"aF:f:O:rst:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-asr] [-F format] [-f filter] [-O order][-t target-window]",
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_list_panes_exec),
    }
};
fn cmd_list_panes_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
    let mut s: Option<SessionRef> = target.session_handle();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let mut order: sort_order = SORT_ACTIVITY;
    order = sort_order_from_string(args_get(args, 'O' as i32 as u_char));
    if order as ::core::ffi::c_uint == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        inv.error( |out| out.write_all(b"invalid sort order"));
        return ExecOutcome::err();
    }
    if args_has(args, 'a' as i32 as u_char) != 0 {
        cmd_list_panes_server(self_0, inv);
    } else if args_has(args, 's' as i32 as u_char) != 0 {
        cmd_list_panes_session(
            self_0,
            s.as_ref().expect("live session"),
            inv,
            1 as ::core::ffi::c_int,
        );
    } else {
        cmd_list_panes_window(
            self_0,
            s.as_ref().expect("live session"),
            wl.clone(),
            inv,
            0 as ::core::ffi::c_int,
        );
    }
    ExecOutcome::ok()
})
}
fn cmd_list_panes_server(self_0: &cmd, inv: &Invocation<'_>) {
    let mut s: Option<SessionRef> = None;
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        cmd_list_panes_session(
            self_0,
            s.as_ref().expect("live session"),
            inv,
            2 as ::core::ffi::c_int,
        );
        s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
        s = s_owner.clone();
    }
}
fn cmd_list_panes_session(
    self_0: &cmd,
    s_owner: &SessionRef,
    inv: &Invocation<'_>,
    mut type_0: ::core::ffi::c_int,
) {
    let s = Some(s_owner.clone());
    let mut wl: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    wl = s
        .as_ref()
        .expect("live session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while wl.is_alive() {
        cmd_list_panes_window(self_0, s_owner, wl.clone(), inv, type_0);
        wl = winlink_next(&wl);
    }
}
fn cmd_list_panes_window(
    self_0: &cmd,
    s_owner: &SessionRef,
    mut wl: tiri_core::refbox::Weak<winlink>,
    inv: &Invocation<'_>,
    mut type_0: ::core::ffi::c_int,
) {

    let queue_client = inv.client();
    let args = cmd_get_args(self_0);
    let c_owner = inv.client();
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut i: u_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let template = args_get(args, 'F' as i32 as u_char).unwrap_or(match type_0 {
        0 => c"#{pane_index}: [#{pane_width}x#{pane_height}] [history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}#{?pane_active, (active),}#{?pane_dead, (dead),}",
        1 => c"#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}] [history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}#{?pane_active, (active),}#{?pane_dead, (dead),}",
        2 => c"#{session_name}:#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}] [history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}#{?pane_active, (active),}#{?pane_dead, (dead),}",
        _ => c"",
    });
    let filter = args_get(args, 'f' as i32 as u_char);
    sort_crit.order = sort_order_from_string(args_get(args, 'O' as i32 as u_char));
    sort_crit.reversed = args_has(args, 'r' as i32 as u_char);
    let cf = inv.format();
    let l = sort_get_panes_window(&winlink_window(&wl).expect("linked window"), &sort_crit);
    let n = u_int::try_from(l.len()).expect("too many panes to list");
    i = 0 as u_int;
    while i < n {
        let pane = &l[i as usize];
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            Some(&cf),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_add(ft, c"line", |out| write!(out, "{}", { n }));
        format_defaults(ft, c.as_ref(), Some(s_owner), wl.clone(), Some(pane));
        if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
            let line = format_expand_cstring(ft, template);
            inv.print( |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
        i = i.wrapping_add(1);
    }
}
