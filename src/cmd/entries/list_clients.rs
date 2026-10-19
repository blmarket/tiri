use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create, format_defaults, format_expand_cstring, format_true,
};
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_READONLY};
use crate::shared::format::FORMAT_NONE;
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::sort::{sort_get_clients, sort_order_from_string};

pub const LIST_CLIENTS_TEMPLATE: &std::ffi::CStr = c"#{client_name}: #{session_name} [#{client_width}x#{client_height} #{client_termname}] #{?#{!=:#{client_uid},#{uid}},[user #{?client_user,#{client_user},#{client_uid},}] ,}#{?client_flags,(,}#{client_flags}#{?client_flags,),}";
pub static cmd_list_clients_entry: cmd_entry = {
    cmd_entry {
        name: c"list-clients",
        alias: Some(c"lsc"),
        args: args_parse {
            template: c"F:f:O:rt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-F format] [-f filter] [-O order][-t target-session]",
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
        flags: CMD_READONLY | CMD_AFTERHOOK,
        exec: Some(cmd_list_clients_exec),
    }
};
unsafe fn cmd_list_clients_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let mut s: Option<SessionRef> = None;
    let mut i: u_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    if args_has(args, 't' as i32 as u_char) != 0 {
        s = target.session_handle();
    } else {
        s = None;
    }
    let template = args_get(args, 'F' as i32 as u_char).unwrap_or(LIST_CLIENTS_TEMPLATE);
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
    let clients_sorted = sort_get_clients(&sort_crit);
    i = 0 as u_int;
    while (i as usize) < clients_sorted.len() {
        let c = &clients_sorted[i as usize];
        let attached = c.attached_session().upgrade();
        if !(attached.is_none()
            || !s.is_none() && !crate::shared::rc::same(s.as_ref(), attached.as_ref()))
        {
            let mut ft_owner = format_create(
                queue_client.as_ref(),
                Some(item_handle),
                FORMAT_NONE,
                0 as ::core::ffi::c_int,
            );
            let ft = &mut *ft_owner;
            format_add(ft, c"line", |out| write!(out, "{}", (i) as u32));
            format_defaults(ft, Some(c), None, (refbox::Weak::new()).clone(), None);
            if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
                let line = format_expand_cstring(ft, template);
                cmdq_print(item_handle, |out| write_cstr(out, &*line));
            }
            drop(ft_owner);
        }
        i = i.wrapping_add(1);
    }
    CmdOutcome::Normal
}
