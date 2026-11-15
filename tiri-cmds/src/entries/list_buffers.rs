use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{
    format_create, format_defaults_paste_buffer, format_expand_cstring, format_true,
};
use tiri_core::shared::abi::u_char;
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::sort::sort_criteria;
use tiri_core::shared::sort::{SORT_ACTIVITY, SORT_END};
use tiri_core::sort::{sort_get_buffers, sort_order_from_string};

pub const LIST_BUFFERS_TEMPLATE: &std::ffi::CStr =
    c"#{buffer_name}: #{buffer_size} bytes: \"#{buffer_sample}\"";
pub static cmd_list_buffers_entry: cmd_entry = {
    cmd_entry {
        name: c"list-buffers",
        alias: Some(c"lsb"),
        args: args_parse {
            template: c"F:f:O:r",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-F format] [-f filter] [-O order]",
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
        exec: Some(cmd_list_buffers_exec),
    }
};
fn cmd_list_buffers_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let queue_client = inv.client();
    let args = cmd_get_args(self_0);
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let template = args_get(args, 'F' as i32 as u_char).unwrap_or(LIST_BUFFERS_TEMPLATE);
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
    let buffers = sort_get_buffers(&sort_crit);
    for pb in buffers {
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            Some(&cf),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_defaults_paste_buffer(ft, &pb);
        if filter.is_none_or(|filter| format_true(&format_expand_cstring(ft, filter))) {
            let line = format_expand_cstring(ft, template);
            inv.print( |out| write_cstr(out, &*line));
        }
        drop(ft_owner);
    }
    ExecOutcome::ok()
})
}
