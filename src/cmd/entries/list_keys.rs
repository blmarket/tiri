use crate::arguments::{args, args_get, args_has, args_parse, args_string};
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_target_client, cmdq_print};
use crate::cmd::{cmd_get_args, cmd_list_print_cstring};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{format_add, format_create, format_defaults, format_expand_cstring};
use crate::key_bindings::{key_bindings_get_table, key_bindings_has_repeat, key_tables};
use crate::key_string::{key_string_format, key_string_parse_cstr};
use crate::options::options_get_number;
use crate::options::OptionsScope;
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_CONTROL;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{
    CMD_AFTERHOOK, CMD_LIST_PRINT_ESCAPED, CMD_LIST_PRINT_NO_GROUPS, CMD_STARTSERVER,
};
use crate::shared::format::format_tree;
use crate::shared::format::FORMAT_NONE;
use crate::shared::key::KEY_BINDING_REPEAT;
use crate::shared::key::*;
use crate::shared::key::{key_binding, key_table};
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::sort::{sort_get_key_bindings, sort_get_key_bindings_table, sort_order_from_string};
use crate::status::status_message_set;
use crate::text::utf8::utf8_cstrwidth;
use std::ffi::{CStr, CString};

pub const LIST_KEYS_TEMPLATE: &std::ffi::CStr = c"#{?notes_only,#{key_prefix} #{p|#{key_string_width}:key_string} #{?key_note,#{key_note},#{key_command}},bind-key #{?key_has_repeat,#{?key_repeat,-r,  },} -T #{p|#{key_table_width}:key_table} #{p|#{key_string_width}:#{q|a:key_string}} #{key_command}}";
pub static cmd_list_keys_entry: cmd_entry = {
    cmd_entry {
        name: c"list-keys",
        alias: Some(c"lsk"),
        args: args_parse {
            template: c"1aF:NO:P:rT:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-1aNr] [-F format] [-O order] [-P prefix-string][-T key-table] [key]",
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
        exec: Some(cmd_list_keys_exec),
    }
};
unsafe fn cmd_list_keys_get_prefix(args: &args) -> CString {
    let mut prefix: key_code = 0;
    if args_has(args, 'P' as i32 as u_char) != 0 {
        return args_get(args, 'P' as i32 as u_char)
            .expect("argument is present")
            .to_owned();
    }
    prefix =
        OptionsScope::GlobalSession.with_local(|oo| options_get_number(oo, c"prefix")) as key_code;
    if prefix == KEYC_NONE as ::core::ffi::c_ulong as key_code {
        return CString::default();
    }
    key_string_format(prefix, false)
}
fn cmd_list_keys_get_width(bindings: &[&key_binding]) -> u_int {
    bindings
        .iter()
        .map(|&bd| {
            let key_string = key_string_format(bd.key, false);
            utf8_cstrwidth(&key_string)
        })
        .max()
        .unwrap_or(0)
}
fn cmd_list_keys_get_table_width(bindings: &[&key_binding]) -> u_int {
    bindings
        .iter()
        .map(|&bd| bd.tablename.as_ref().map_or(0, |s| utf8_cstrwidth(s)))
        .max()
        .unwrap_or(0)
}
fn cmd_list_keys_get_root_and_prefix<'a>(
    tables: &'a [std::cell::Ref<'_, key_table>],
    sort_crit: &sort_criteria,
) -> Vec<&'a key_binding> {
    let mut bindings = Vec::new();
    for name in [c"prefix", c"root"] {
        let table = tables
            .iter()
            .map(|table| &**table)
            .find(|table| table.name.as_c_str() == name);
        bindings.extend(sort_get_key_bindings_table(table, sort_crit));
    }
    bindings
}
fn cmd_list_keys_filter_key_list(
    filter_notes: ::core::ffi::c_int,
    filter_key: ::core::ffi::c_int,
    only: key_code,
    bindings: &mut Vec<&key_binding>,
) {
    bindings.retain(|&bd| {
        let key = (bd.key as ::core::ffi::c_ulonglong & (KEYC_MASK_KEY | KEYC_MASK_MODIFIERS))
            as key_code;
        (filter_key == 0 || only == key) && (filter_notes == 0 || !bd.note.is_none())
    });
}
unsafe fn cmd_list_keys_format_add_key_binding(
    ft: &mut format_tree,
    bd: &key_binding,
    prefix: &CStr,
) {
    if bd.flags & KEY_BINDING_REPEAT != 0 {
        format_add(ft, c"key_repeat", |out| out.write_all(b"1"));
    } else {
        format_add(ft, c"key_repeat", |out| out.write_all(b"0"));
    }
    if bd.note.is_some() {
        format_add(ft, c"key_note", |out| write_cstr(out, bd.note.as_deref()));
    } else {
        format_add(ft, c"key_note", |out| write_cstr(out, c""));
    }
    let key_string = key_string_format(bd.key, false);
    format_add(ft, c"key_prefix", |out| {
        write_cstr(out, nullable_cstr(prefix.as_ptr()))
    });
    format_add(ft, c"key_table", |out| {
        write_cstr(out, bd.tablename.as_deref())
    });
    format_add(ft, c"key_string", |out| write_cstr(out, &*key_string));
    let command = cmd_list_print_cstring(
        &bd.cmdlist().borrow(),
        CMD_LIST_PRINT_ESCAPED | CMD_LIST_PRINT_NO_GROUPS,
    );
    format_add(ft, c"key_command", |out| write_cstr(out, &*command));
}
unsafe fn cmd_list_keys_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let args = cmd_get_args(self_0.get_unchecked());
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut table = None;
    let mut only: key_code = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
    let mut single: ::core::ffi::c_int = 0;
    let mut notes_only: ::core::ffi::c_int = 0;
    let mut filter_notes: ::core::ffi::c_int = 0;
    let mut filter_key: ::core::ffi::c_int = 0;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let keystr: Option<&CStr> = args_string(args, 0 as u_int);
    if let Some(keystr) = keystr {
        only = key_string_parse_cstr(keystr).unwrap_or(KEYC_UNKNOWN);
        if only == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid key: ")?;
                write_cstr(out, keystr)
            });
            return CmdOutcome::Error;
        }
        only &= KEYC_MASK_KEY | KEYC_MASK_MODIFIERS;
    }
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
    if let Some(tablename) = args_get(args, 'T' as i32 as u_char) {
        table = key_bindings_get_table(tablename, 0 as ::core::ffi::c_int);
        if table.is_none() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"table ")?;
                write_cstr(out, tablename)?;
                out.write_all(b" doesn't exist")
            });
            return CmdOutcome::Error;
        }
    }
    let prefix = cmd_list_keys_get_prefix(args);
    single = args_has(args, '1' as i32 as u_char);
    notes_only = args_has(args, 'N' as i32 as u_char);
    let template = args_get(args, 'F' as i32 as u_char).unwrap_or(LIST_KEYS_TEMPLATE);
    let table_owners = key_tables::snapshot();
    let tables: Vec<_> = table_owners.iter().map(|table| table.borrow()).collect();
    let table_borrow = table.as_ref().map(|table| table.borrow());
    let mut bindings = if let Some(table) = table_borrow.as_ref() {
        sort_get_key_bindings_table(Some(table), &sort_crit)
    } else if notes_only != 0 {
        cmd_list_keys_get_root_and_prefix(&tables, &sort_crit)
    } else {
        sort_get_key_bindings(&tables, &sort_crit)
    };
    filter_notes =
        (notes_only != 0 && args_has(args, 'a' as i32 as u_char) == 0) as ::core::ffi::c_int;
    filter_key = (only != KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code) as ::core::ffi::c_int;
    if filter_notes != 0 || filter_key != 0 {
        cmd_list_keys_filter_key_list(filter_notes, filter_key, only, &mut bindings);
    }
    if filter_key != 0 && bindings.is_empty() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"unknown key: ")?;
            write_cstr(out, keystr)
        });
        return CmdOutcome::Error;
    }
    if single != 0 {
        bindings.truncate(1);
    }
    let mut ft_owner = format_create(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, tc.as_ref(), None, (refbox::Weak::new()).clone(), None);
    format_add(ft, c"notes_only", |out| {
        write!(out, "{}", (notes_only) as i32)
    });
    format_add(ft, c"key_has_repeat", |out| {
        write!(out, "{}", (key_bindings_has_repeat(&bindings)) as i32)
    });
    format_add(ft, c"key_string_width", |out| {
        write!(out, "{}", (cmd_list_keys_get_width(&bindings)) as u32)
    });
    format_add(ft, c"key_table_width", |out| {
        write!(out, "{}", (cmd_list_keys_get_table_width(&bindings)) as u32)
    });
    for &bd in &bindings {
        cmd_list_keys_format_add_key_binding(ft, bd, &prefix);
        let line = format_expand_cstring(ft, template);
        if single != 0
            && !tc.is_none()
            && !tc.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
        {
            status_message_set(
                tc.as_ref(),
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| write_cstr(out, &*line),
            );
        } else if !line.is_empty() {
            cmdq_print(item_handle, |out| write_cstr(out, &*line));
        }
        if single != 0 {
            break;
        }
    }
    drop(ft_owner);
    CmdOutcome::Normal
}
