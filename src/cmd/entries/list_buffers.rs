use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{
    format_create_with_client, format_defaults_paste_buffer, format_expand_cstring, format_true,
};
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::sort::sort_criteria;
use crate::src::shared::sort::*;
use crate::src::sort::{sort_get_buffers, sort_order_from_string};

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
unsafe fn cmd_list_buffers_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
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
    if template.is_null() {
        template = LIST_BUFFERS_TEMPLATE.as_ptr();
    }
    filter =
        args_get(&*(args), 'f' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(&*(args), 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CMD_RETURN_ERROR;
    }
    sort_crit.reversed = args_has(args, 'r' as i32 as u_char);
    let buffers = sort_get_buffers(&sort_crit);
    for pb in buffers {
        let mut ft_owner = format_create_with_client(
            queue_client.as_ref(),
            Some(item_handle),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        ft = &raw mut *ft_owner;
        format_defaults_paste_buffer(&mut *ft, &pb);
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
    }
    CMD_RETURN_NORMAL
}
