use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::{CMD_AFTERHOOK, CMD_READONLY};
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{
    format_add, format_create, format_defaults, format_expand_cstring, format_true,
};
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::sort::sort_criteria;
use tiri_core::shared::sort::{SORT_ACTIVITY, SORT_END};
use tiri_core::sort::{sort_get_clients, sort_order_from_string};

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
            flag: b't',
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_READONLY | CMD_AFTERHOOK,
        exec: Some(cmd_list_clients_exec),
    }
};
fn cmd_list_clients_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let queue_client = inv.client();
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
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
    sort_crit.order = sort_order_from_string(args_get(args, 'O' as i32 as u_char));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        inv.error( |out| out.write_all(b"invalid sort order"));
        return ExecOutcome::err();
    }
    sort_crit.reversed = args_has(args, 'r' as i32 as u_char);
    let cf = inv.format();
    let clients_sorted = sort_get_clients(&sort_crit);
    i = 0 as u_int;
    while (i as usize) < clients_sorted.len() {
        let c = &clients_sorted[i as usize];
        let attached = c.attached_session().upgrade();
        if !(attached.is_none()
            || !s.is_none() && !tiri_core::shared::rc::same(s.as_ref(), attached.as_ref()))
        {
            let mut ft_owner = format_create(
                queue_client.as_ref(),
                Some(&cf),
                FORMAT_NONE,
                0 as ::core::ffi::c_int,
            );
            let ft = &mut *ft_owner;
            format_add(ft, c"line", |out| write!(out, "{}", (i) as u32));
            format_defaults(
                ft,
                Some(c),
                None,
                (tiri_core::refbox::Weak::new()).clone(),
                None,
            );
            if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
                let line = format_expand_cstring(ft, template);
                inv.print( |out| write_cstr(out, &*line));
            }
            drop(ft_owner);
        }
        i = i.wrapping_add(1);
    }
    ExecOutcome::ok()
})
}
