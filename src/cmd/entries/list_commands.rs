use crate::arguments::{args, args_get, args_parse, args_string};
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print};
use crate::cmd::{cmd_find, cmd_get_args_mut, cmd_table};
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create_with_client, format_defaults, format_expand_cstring,
};
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_STARTSERVER};
use crate::shared::format::format_tree;
use crate::shared::format::FORMAT_NONE;
use std::ffi::CStr;

pub const LIST_COMMANDS_TEMPLATE: &std::ffi::CStr =
    c"#{command_list_name}#{?command_list_alias, (#{command_list_alias}),} #{command_list_usage}";
pub static cmd_list_commands_entry: cmd_entry = {
    cmd_entry {
        name: c"list-commands",
        alias: Some(c"lscm"),
        args: args_parse {
            template: c"F:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-F format] [command]",
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
        flags: CMD_STARTSERVER | CMD_AFTERHOOK,
        exec: Some(cmd_list_commands),
    }
};
unsafe fn cmd_list_single_command(
    entry: &cmd_entry,
    mut ft: *mut format_tree,
    template: &CStr,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) {
    format_add(ft, c"command_list_name", |out| {
        write_cstr(out, &*entry.name)
    });
    format_add(ft, c"command_list_alias", |out| {
        write_cstr(out, entry.alias.unwrap_or(c""))
    });
    format_add(ft, c"command_list_usage", |out| {
        write_cstr(out, &*entry.usage)
    });
    let line = format_expand_cstring(ft, template.as_ptr());
    if !line.is_empty() {
        cmdq_print(item_handle, |out| write_cstr(out, &*line));
    }
}
unsafe fn cmd_list_commands(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let template: &CStr =
        args_get(&*(args), 'F' as i32 as u_char).unwrap_or(LIST_COMMANDS_TEMPLATE);
    let mut ft_owner = format_create_with_client(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    ft = &raw mut *ft_owner;
    format_defaults(ft, None, None, (refbox::Weak::new()).clone(), None);
    if let Some(command) = args_string(&mut *(args), 0 as u_int) {
        match cmd_find(command) {
            Ok(found) => {
                cmd_list_single_command(found, ft, template, item_handle);
            }
            Err(cause) => {
                cmdq_error(item_handle, |out| write_cstr(out, &*cause));
                drop(ft_owner);
                return CmdOutcome::Error;
            }
        }
    } else {
        for &entry in &cmd_table {
            cmd_list_single_command(entry, ft, template, item_handle);
        }
    }
    drop(ft_owner);
    CmdOutcome::Normal
}
