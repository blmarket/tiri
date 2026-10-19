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
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::sort::{sort_get_sessions, sort_order_from_string};

pub const LIST_SESSIONS_TEMPLATE: &std::ffi::CStr = c"#{session_name}: #{session_windows} windows (created #{t:session_created})#{?session_grouped, (group ,}#{session_group}#{?session_grouped,),}#{?session_attached, (attached),}";
pub static cmd_list_sessions_entry: cmd_entry = {
    cmd_entry {
        name: c"list-sessions",
        alias: Some(c"ls"),
        args: args_parse {
            template: c"F:f:O:r",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-r] [-F format] [-f filter] [-O order]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_list_sessions_exec),
    }
};
unsafe fn cmd_list_sessions_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let args = cmd_get_args(self_0.get_unchecked());
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut i: u_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let template = args_get(args, 'F' as i32 as u_char).unwrap_or(LIST_SESSIONS_TEMPLATE);
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
    let l = sort_get_sessions(&sort_crit);
    let n = u_int::try_from(l.len()).expect("too many sessions to list");
    i = 0 as u_int;
    while i < n {
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            Some(item_handle),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_add(ft, c"line", |out| write!(out, "{}", (i) as u32));
        format_defaults(
            ft,
            c.as_ref(),
            Some(&l[i as usize]),
            (refbox::Weak::new()).clone(),
            None,
        );
        if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
            let line = format_expand_cstring(ft, template);
            cmdq_print(item_handle, |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
        i = i.wrapping_add(1);
    }
    CmdOutcome::Normal
}
