use crate::arguments::{args, args_get, args_has, args_parse};
use crate::cmd::cmd_get_args_mut;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create_with_client, format_defaults, format_expand_cstring, format_true,
};
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::format::format_tree;
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
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_list_windows_exec),
    }
};
unsafe fn cmd_list_windows_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s: Option<SessionRef> = None;
    let mut i: u_int = 0;
    let mut n: u_int = 0;
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut template: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut filter: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut flag: ::core::ffi::c_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    template =
        args_get(&*(args), 'F' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    filter =
        args_get(&*(args), 'f' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(&*(args), 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(&*args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CmdOutcome::Error;
    }
    sort_crit.reversed = args_has(&*args, 'r' as i32 as u_char);
    let winlinks = if args_has(&*args, 'a' as i32 as u_char) != 0 {
        let links = sort_get_winlinks(&raw mut sort_crit);
        if template.is_null() {
            template = LIST_WINDOWS_WITH_SESSION_TEMPLATE.as_ptr();
        }
        links
    } else {
        let links = sort_get_winlinks_session(
            &(*target).session_handle().expect("live session"),
            &raw mut sort_crit,
        );
        if template.is_null() {
            template = c"#{window_index}: #{window_name}#{window_raw_flags} (#{window_panes} panes) [#{window_width}x#{window_height}] [layout #{window_layout}] #{window_id}#{?window_active, (active),}".as_ptr();
        }
        links
    };
    n = u_int::try_from(winlinks.len()).expect("too many winlinks to list");
    i = 0 as u_int;
    while i < n {
        wl = winlinks[i as usize].clone();
        let Some(session_owner) = wl.get_unchecked().session.upgrade() else {
            i += 1;
            continue;
        };
        s = Some(session_owner.clone());
        let mut ft_owner = format_create_with_client(
            queue_client.as_ref(),
            Some(item_handle),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        ft = &raw mut *ft_owner;
        format_add(ft, c"line", |out| write!(out, "{}", (n) as u32));
        format_defaults(ft, c.as_ref(), s.as_ref(), wl.clone(), None);
        if !filter.is_null() {
            let expanded = format_expand_cstring(ft, filter);
            flag = format_true(expanded.as_ptr());
        } else {
            flag = 1 as ::core::ffi::c_int;
        }
        if flag != 0 {
            let line = format_expand_cstring(ft, template);
            cmdq_print(item_handle, |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
        i = i.wrapping_add(1);
    }
    CmdOutcome::Normal
}
