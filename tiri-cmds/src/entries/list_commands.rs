use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::{cmd_find, cmd_get_args, cmd_table};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{CMD_AFTERHOOK, CMD_STARTSERVER};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::CStr;
use tiri_core::arguments::{args_get, args_parse, args_string};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{format_add, format_create, format_defaults, format_expand_cstring};
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::format::FORMAT_NONE;
use tiri_core::shared::format::format_tree;

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
fn cmd_list_single_command(
    entry: &cmd_entry,
    ft: &mut format_tree,
    template: &CStr,
    inv: &Invocation<'_>,
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
    let line = format_expand_cstring(ft, template);
    if !line.is_empty() {
        inv.print( |out| write_cstr(out, &*line));
    }
}
fn cmd_list_commands<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let queue_client = inv.client();
    let args = cmd_get_args(self_0);
    let template: &CStr = args_get(args, 'F' as i32 as u_char).unwrap_or(LIST_COMMANDS_TEMPLATE);
    let cf = inv.format();
    let mut ft_owner = format_create(
        queue_client.as_ref(),
        Some(&cf),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(
        ft,
        None,
        None,
        (tiri_core::refbox::Weak::new()).clone(),
        None,
    );
    if let Some(command) = args_string(args, 0 as u_int) {
        match cmd_find(command) {
            Ok(found) => {
                cmd_list_single_command(found, ft, template, inv);
            }
            Err(cause) => {
                inv.error( |out| write_cstr(out, &*cause));
                drop(ft_owner);
                return ExecOutcome::err();
            }
        }
    } else {
        for &entry in cmd_table() {
            cmd_list_single_command(entry, ft, template, inv);
        }
    }
    drop(ft_owner);
    ExecOutcome::ok()
})
}
