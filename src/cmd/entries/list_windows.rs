use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create, format_defaults, format_expand_cstring, format_true,
};
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::format::FORMAT_NONE;
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::window::winlink;
use crate::sort::{sort_get_winlinks, sort_get_winlinks_session, sort_order_from_string};

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
unsafe fn cmd_list_windows_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s: Option<SessionRef> = None;
    let mut i: u_int = 0;
    let mut n: u_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let filter = args_get(args, 'f' as i32 as u_char);
    sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(args, 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CmdOutcome::Error;
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
            Some(item_handle),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_add(ft, c"line", |out| write!(out, "{}", (n) as u32));
        format_defaults(ft, c.as_ref(), s.as_ref(), wl.clone(), None);
        if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
            let line = format_expand_cstring(ft, template);
            cmdq_print(item_handle, |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
        i = i.wrapping_add(1);
    }
    CmdOutcome::Normal
}
