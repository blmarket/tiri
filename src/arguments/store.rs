//! Flag and positional argument storage.

use super::value::{args_value, ARGS_COMMANDS, ARGS_NONE, ARGS_STRING};
use crate::src::cmd::{
    cmd_list_copy, cmd_list_print_cstring, cmd_log_argv, cmd_template_replace_cstring,
};
use crate::src::shared::abi::*;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};

#[repr(C)]
pub struct args {
    pub(super) tree: args_tree,
    pub values: Vec<args_value>,
}

pub(super) type args_tree = BTreeMap<u_char, Box<args_entry>>;

#[repr(C)]
/// Box-owned by the enclosing args tree; dropping it also drops its values.
pub(super) struct args_entry {
    pub(super) flag: u_char,
    pub(super) values: args_values,
    pub(super) count: u_int,
    pub(super) flags: ::core::ffi::c_int,
}

pub(super) const ARGS_ENTRY_OPTIONAL_VALUE: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

/// Owns flag-value storage directly. Iteration borrows
/// the boxed records and cannot outlive the containing argument set.
pub(super) type args_values = Vec<Box<args_value>>;

/// Iterate flags in tmux's byte ordering. Zero remains the legacy end sentinel.
pub fn args_flags(args: &args) -> impl Iterator<Item = u_char> + '_ {
    args.tree.keys().copied().take_while(|flag| *flag != 0)
}

/// Borrow repeated flag values in the order they were supplied.
pub fn args_flag_values(args: &args, flag: u_char) -> impl Iterator<Item = &args_value> {
    args.tree
        .get(&flag)
        .into_iter()
        .flat_map(|entry| entry.values.iter().map(Box::as_ref))
}

pub(super) fn args_last_value(args: &args, flag: u_char) -> Option<&args_value> {
    args.tree.get(&flag)?.values.last().map(Box::as_ref)
}

pub(super) fn args_last_string(args: &args, flag: u_char) -> Option<&CStr> {
    args_last_value(args, flag)?.as_string()
}

pub fn args_create() -> Box<args> {
    Box::new(args {
        tree: Default::default(),
        values: Vec::new(),
    })
}
pub(super) fn args_push_positional_owned(args: &mut args, value: args_value) {
    args.values.push(value);
}

unsafe fn args_copy_copy_value(from: &args_value, argv: &Vec<CString>) -> args_value {
    match from.type_0() as ::core::ffi::c_uint {
        1 => {
            let source = from.as_string().expect("string argument");
            if argv.is_empty() {
                return args_value::string(source.to_owned());
            }
            let mut expanded = cmd_template_replace_cstring(source, argv[0].as_c_str(), 1);
            for (i, argument) in argv.iter().enumerate().skip(1) {
                expanded = cmd_template_replace_cstring(
                    expanded.as_c_str(),
                    argument.as_c_str(),
                    (i + 1) as ::core::ffi::c_int,
                );
            }
            args_value::string(expanded)
        }
        2 => args_value::commands(cmd_list_copy(
            &from.as_commands().expect("command argument").borrow(),
            argv,
        )),
        _ => args_value::empty(),
    }
}
pub unsafe fn args_copy(args: &args, argv: &Vec<CString>) -> Box<args> {
    cmd_log_argv(argv, c"args_copy");
    let mut new_args = args_create();
    for entry in args.tree.values() {
        if entry.values.is_empty() {
            for _ in 0..entry.count {
                args_set_flag(&mut *new_args, entry.flag, 0);
            }
        } else {
            for value in &entry.values {
                args_set_value(
                    &mut new_args,
                    entry.flag,
                    Some(args_copy_copy_value(value, argv)),
                    0,
                );
            }
        }
    }
    for value in &args.values {
        args_push_positional_owned(&mut new_args, args_copy_copy_value(value, argv));
    }
    new_args
}
pub unsafe fn args_to_vector(args: &args) -> Vec<CString> {
    let mut argv = Vec::new();
    for value in args.values.iter() {
        match value.type_0() as ::core::ffi::c_uint {
            1 => {
                argv.push(value.as_string().expect("string argument").to_owned());
            }
            2 => {
                let printed = cmd_list_print_cstring(
                    &value.as_commands().expect("command argument").borrow(),
                    0 as ::core::ffi::c_int,
                );
                argv.push(printed);
            }
            _ => {}
        }
    }
    argv
}

pub unsafe fn args_has(args: *mut args, flag: u_char) -> ::core::ffi::c_int {
    (&*args)
        .tree
        .get(&flag)
        .map_or(0, |entry| entry.count as ::core::ffi::c_int)
}
pub(super) fn args_set_value(
    args: &mut args,
    flag: u_char,
    value: Option<args_value>,
    flags: ::core::ffi::c_int,
) {
    let entry = args.tree.entry(flag).or_insert_with(|| {
        Box::new(args_entry {
            flag,
            values: Default::default(),
            count: 0,
            flags,
        })
    });
    entry.count = entry.count.wrapping_add(1);
    if let Some(value) = value {
        if value.type_0() != ARGS_NONE {
            entry.values.push(Box::new(value));
        }
    }
}

pub unsafe fn args_set_owned_string(args: *mut args, value: CString) {
    let flag: u_char = 'f' as i32 as u_char;
    let flags: ::core::ffi::c_int = 0;
    args_set_value(&mut *args, flag, Some(args_value::string(value)), flags);
}

/// Add an occurrence of a flag that has no associated value.
unsafe fn args_set_flag(args: *mut args, flag: u_char, flags: ::core::ffi::c_int) {
    args_set_value(&mut *args, flag, None, flags);
}

pub fn args_get(args: &args, flag: u_char) -> Option<&CStr> {
    args_last_value(args, flag).and_then(args_value::as_string)
}

pub unsafe fn args_count(mut args: *mut args) -> u_int {
    (*args).values.len() as u_int
}
pub fn args_string(args: &mut args, idx: u_int) -> Option<&CStr> {
    if idx >= (args.values.len() as u_int) {
        return None;
    }
    let value = &mut args.values[idx as usize];
    match value.type_0() {
        ARGS_NONE => Some(c""),
        ARGS_STRING => value.as_string(),
        ARGS_COMMANDS => {
            if value.cached.is_none() {
                let printed = unsafe {
                    cmd_list_print_cstring(
                        &value.as_commands().expect("command argument").borrow(),
                        0,
                    )
                };
                value.cached = Some(printed);
            }
            value.cached.as_deref()
        }
        _ => unreachable!("unexpected argument type"),
    }
}
