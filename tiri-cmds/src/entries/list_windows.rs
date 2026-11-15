use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{
    format_add, format_create, format_defaults, format_expand_cstring, format_true,
};
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::sort::sort_criteria;
use tiri_core::shared::sort::{SORT_ACTIVITY, SORT_END};
use tiri_core::shared::window::winlink;
use tiri_core::sort::{sort_get_winlinks, sort_get_winlinks_session, sort_order_from_string};

pub const LIST_WINDOWS_WITH_SESSION_TEMPLATE: &std::ffi::CStr = c"#{session_name}:#{window_index}: #{window_name}#{window_raw_flags} (#{window_panes} panes) [#{window_width}x#{window_height}] ";
pub static cmd_list_windows_entry: cmd_entry = {
    cmd_entry {
        name: c"list-windows",
        alias: Some(c"lsw"),
        args: args_parse {
            template: c"aF:f:O:rt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-ar] [-F format] [-f filter] [-O order][-t target-session]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_list_windows_exec),
    }
};
fn cmd_list_windows_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let queue_client = inv.client();
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
    let c_owner = inv.client();
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut wl: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    let mut s: Option<SessionRef> = None;
    let mut i: u_int = 0;
    let mut n: u_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let filter = args_get(args, 'f' as i32 as u_char);
    sort_crit.order = sort_order_from_string(args_get(args, 'O' as i32 as u_char));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        inv.error( |out| out.write_all(b"invalid sort order"));
        return ExecOutcome::err();
    }
    sort_crit.reversed = args_has(args, 'r' as i32 as u_char);
    let (winlinks, default_template) = if args_has(args, 'a' as i32 as u_char) != 0 {
        (
            sort_get_winlinks(&sort_crit),
            LIST_WINDOWS_WITH_SESSION_TEMPLATE,
        )
    } else {
        (
            sort_get_winlinks_session(&target.session_handle().expect("live session"), &sort_crit),
            c"#{window_index}: #{window_name}#{window_raw_flags} (#{window_panes} panes) [#{window_width}x#{window_height}] [layout #{window_layout}] #{window_id}#{?window_active, (active),}",
        )
    };
    let cf = inv.format();
    let template = args_get(args, 'F' as i32 as u_char).unwrap_or(default_template);
    n = u_int::try_from(winlinks.len()).expect("too many winlinks to list");
    i = 0 as u_int;
    while i < n {
        wl = winlinks[i as usize].clone();
        let Some(session_owner) = wl.try_borrow().expect("live winlink").session.upgrade() else {
            i += 1;
            continue;
        };
        s = Some(session_owner.clone());
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            Some(&cf),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_add(ft, c"line", |out| write!(out, "{}", (n) as u32));
        format_defaults(ft, c.as_ref(), s.as_ref(), wl.clone(), None);
        if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
            let line = format_expand_cstring(ft, template);
            inv.print( |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
        i = i.wrapping_add(1);
    }
    ExecOutcome::ok()
})
}
