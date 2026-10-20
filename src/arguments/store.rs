//! Flag and positional argument storage.

use super::value::args_value;
use crate::cmd::{
    cmd_list_copy, cmd_list_print_cstring, cmd_log_argv, cmd_template_replace_cstring,
};
use crate::shared::abi::*;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};

pub struct args {
    pub(super) tree: args_tree,
    pub values: Vec<args_value>,
}

pub(super) type args_tree = BTreeMap<u_char, args_entry>;

/// Owned by the enclosing args tree, keyed by its flag.
pub(super) struct args_entry {
    pub(super) values: Vec<args_value>,
    pub(super) count: u_int,
    /// Parsed as `-f` without its optional value.
    pub(super) optional: bool,
}

/// Iterate flags in tmux's byte ordering. Zero remains the legacy end sentinel.
pub fn args_flags(args: &args) -> impl Iterator<Item = u_char> + '_ {
    args.tree.keys().copied().take_while(|flag| *flag != 0)
}

/// Borrow repeated flag values in the order they were supplied.
pub fn args_flag_values(args: &args, flag: u_char) -> impl Iterator<Item = &args_value> {
    args.tree
        .get(&flag)
        .into_iter()
        .flat_map(|entry| entry.values.iter())
}

pub(super) fn args_last_value(args: &args, flag: u_char) -> Option<&args_value> {
    args.tree.get(&flag)?.values.last()
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

fn args_copy_copy_value(from: &args_value, argv: &Vec<CString>) -> args_value {
    match from {
        args_value::String(source) => {
            if argv.is_empty() {
                return args_value::String(source.clone());
            }
            let mut expanded = cmd_template_replace_cstring(source, argv[0].as_c_str(), 1);
            for (i, argument) in argv.iter().enumerate().skip(1) {
                expanded = cmd_template_replace_cstring(
                    expanded.as_c_str(),
                    argument.as_c_str(),
                    (i + 1) as ::core::ffi::c_int,
                );
            }
            args_value::String(expanded)
        }
        args_value::Commands(commands, _) => {
            args_value::commands(cmd_list_copy(&commands.borrow(), argv))
        }
        args_value::None => args_value::None,
    }
}
pub fn args_copy(args: &args, argv: &Vec<CString>) -> Box<args> {
    cmd_log_argv(argv, c"args_copy");
    let mut new_args = args_create();
    for (&flag, entry) in &args.tree {
        if entry.values.is_empty() {
            for _ in 0..entry.count {
                args_set_flag(&mut new_args, flag, false);
            }
        } else {
            for value in &entry.values {
                args_set_value(
                    &mut new_args,
                    flag,
                    Some(args_copy_copy_value(value, argv)),
                    false,
                );
            }
        }
    }
    for value in &args.values {
        args_push_positional_owned(&mut new_args, args_copy_copy_value(value, argv));
    }
    new_args
}
pub fn args_to_vector(args: &args) -> Vec<CString> {
    let mut argv = Vec::new();
    for value in args.values.iter() {
        match value {
            args_value::String(text) => argv.push(text.clone()),
            args_value::Commands(commands, _) => {
                argv.push(cmd_list_print_cstring(&commands.borrow(), 0));
            }
            args_value::None => {}
        }
    }
    argv
}

pub fn args_has(args: &args, flag: u_char) -> ::core::ffi::c_int {
    args.tree
        .get(&flag)
        .map_or(0, |entry| entry.count as ::core::ffi::c_int)
}
pub(super) fn args_set_value(
    args: &mut args,
    flag: u_char,
    value: Option<args_value>,
    optional: bool,
) {
    let entry = args.tree.entry(flag).or_insert_with(|| args_entry {
        values: Vec::new(),
        count: 0,
        optional,
    });
    entry.count = entry.count.wrapping_add(1);
    if let Some(value) = value.filter(|value| !matches!(value, args_value::None)) {
        entry.values.push(value);
    }
}

pub fn args_set_string(args: &mut args, flag: u_char, value: CString) {
    args_set_value(args, flag, Some(args_value::String(value)), false);
}

/// Add an occurrence of a flag that has no associated value.
pub fn args_set_flag(args: &mut args, flag: u_char, optional: bool) {
    args_set_value(args, flag, None, optional);
}

pub fn args_get(args: &args, flag: u_char) -> Option<&CStr> {
    args_last_value(args, flag).and_then(args_value::as_string)
}

pub fn args_count(args: &args) -> u_int {
    args.values.len() as u_int
}
pub fn args_string(args: &args, idx: u_int) -> Option<&CStr> {
    match args.values.get(idx as usize)? {
        args_value::None => Some(c""),
        args_value::String(text) => Some(text),
        args_value::Commands(commands, cached) => {
            Some(cached.get_or_init(|| cmd_list_print_cstring(&commands.borrow(), 0)))
        }
    }
}
