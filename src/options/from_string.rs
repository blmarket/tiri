//! Typed option values parsed from text.
use super::store::*;
use super::types::*;
use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::compat::strtonum::strtonum;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::grid::grid_default_cell;
use crate::src::key_string::key_string_parse_cstr;
use libc::{fnmatch, strcasecmp, strcmp, strstr};

use crate::src::style::colour::colour_parse_cstr;
use crate::src::style::{style_parse, style_parse_colour};
use crate::src::tmux::checkshell;

use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::key::*;
use crate::src::shared::style::*;
use std::ffi::{CStr, CString};

/// Legacy detached/global-table adapter. Model callers must use the scoped
/// evaluator so no component borrow is held through format expansion.
pub unsafe fn options_string_to_style(
    oo: &mut options,
    name: &CStr,
    ft: *mut format_tree,
) -> Option<style> {
    crate::src::style::style_resolve_with_options(name, ft.as_mut(), |visit| visit(oo))
}
unsafe fn options_from_string_check(
    oe: &options_table_entry,
    value: &CStr,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let mut sy: style = style {
        gc: grid_cell {
            data: utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            },
            attr: 0,
            flags: 0,
            fg: 0,
            bg: 0,
            us: 0,
            link: 0,
        },
        ignore: 0,
        dim: 0,
        fill: 0,
        align: STYLE_ALIGN_DEFAULT,
        list: STYLE_LIST_OFF,
        range_type: STYLE_RANGE_NONE,
        range_argument: 0,
        range_string: [0; 16],
        width: 0,
        width_percentage: 0,
        pad: 0,
        default_type: STYLE_DEFAULT_BASE,
        link: 0,
    };
    if oe.name == Some(c"default-shell") && checkshell(nullable_cstr(value.as_ptr())) == 0 {
        format_options_cause!(cause, c"not a suitable shell: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    if oe.pattern.is_some_and(|pattern| {
        fnmatch(pattern.as_ptr(), value.as_ptr(), 0 as ::core::ffi::c_int)
            != 0 as ::core::ffi::c_int
    }) {
        format_options_cause!(cause, c"value is invalid: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    if oe.flags & OPTIONS_TABLE_IS_STYLE != 0
        && strstr(value.as_ptr(), c"#{".as_ptr()).is_null()
        && style_parse(&raw mut sy, &grid_default_cell, value) != 0 as ::core::ffi::c_int
    {
        format_options_cause!(cause, c"invalid style: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    if oe.flags & OPTIONS_TABLE_IS_COLOUR != 0
        && strstr(value.as_ptr(), c"#{".as_ptr()).is_null()
        && style_parse_colour(&raw mut sy, &grid_default_cell, value) != 0 as ::core::ffi::c_int
    {
        format_options_cause!(cause, c"invalid colour: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    0 as ::core::ffi::c_int
}
unsafe fn options_from_string_flag(
    oo: &mut options,
    name: &CStr,
    value: Option<&CStr>,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut flag: ::core::ffi::c_int = 0;
    if value.is_null() || *value as ::core::ffi::c_int == '\0' as i32 {
        flag = (options_get_number(oo, name) == 0) as ::core::ffi::c_int;
    } else if strcmp(value, c"1".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"on".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"yes".as_ptr()) == 0 as ::core::ffi::c_int
    {
        flag = 1 as ::core::ffi::c_int;
    } else if strcmp(value, c"0".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"off".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(value, c"no".as_ptr()) == 0 as ::core::ffi::c_int
    {
        flag = 0 as ::core::ffi::c_int;
    } else {
        format_options_cause!(cause, c"bad value: %s", CStr::from_ptr(value),);
        return -(1 as ::core::ffi::c_int);
    }
    options_set_number(oo, name, flag as ::core::ffi::c_longlong);
    0 as ::core::ffi::c_int
}
pub unsafe fn options_find_choice(
    oe: &options_table_entry,
    value: &CStr,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let mut choice: ::core::ffi::c_int = -1;
    for (n, candidate) in oe.choices.iter().enumerate() {
        if *candidate == value {
            choice = n as ::core::ffi::c_int;
        }
    }
    if choice == -(1 as ::core::ffi::c_int) {
        format_options_cause!(cause, c"unknown value: %s", value,);
        return -(1 as ::core::ffi::c_int);
    }
    choice
}
unsafe fn options_from_string_choice(
    oe: &options_table_entry,
    oo: &mut options,
    name: &CStr,
    value: Option<&CStr>,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut choice: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    if value.is_null() {
        choice = options_get_number(oo, name) as ::core::ffi::c_int;
        if choice < 2 as ::core::ffi::c_int {
            choice = (choice == 0) as ::core::ffi::c_int;
        }
    } else {
        choice = options_find_choice(oe, CStr::from_ptr(value), cause);
        if choice < 0 as ::core::ffi::c_int {
            return -(1 as ::core::ffi::c_int);
        }
    }
    options_set_number(oo, name, choice as ::core::ffi::c_longlong);
    0 as ::core::ffi::c_int
}
pub unsafe fn options_from_string(
    oo: &mut options,
    oe: &options_table_entry,
    name: &CStr,
    value: Option<&CStr>,
    mut append: ::core::ffi::c_int,
    mut cause: *mut Option<CString>,
) -> ::core::ffi::c_int {
    let value: *const ::core::ffi::c_char = value.map_or(std::ptr::null(), CStr::as_ptr);
    let mut type_0: options_table_type = OPTIONS_TABLE_STRING;
    let mut number: ::core::ffi::c_longlong = 0;
    let _new: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut key: key_code = 0;
    let mut pr: cmd_parse_result = cmd_parse_result::empty();
    if value.is_null() && oe.type_0 != OPTIONS_TABLE_FLAG && oe.type_0 != OPTIONS_TABLE_CHOICE {
        format_options_cause!(cause, c"empty value",);
        return -(1 as ::core::ffi::c_int);
    }
    type_0 = oe.type_0;
    match type_0 as ::core::ffi::c_uint {
        0 => {
            let old = options_get_string(oo, name);
            options_set_string(oo, name, append, |out| {
                write_cstr(out, nullable_cstr(value))
            });
            let new = options_get_string(oo, name);
            if options_from_string_check(oe, &new, cause) != 0 as ::core::ffi::c_int {
                options_set_string(oo, name, 0 as ::core::ffi::c_int, |out| {
                    write_cstr(out, &*old)
                });
                return -(1 as ::core::ffi::c_int);
            }
            return 0 as ::core::ffi::c_int;
        }
        1 => {
            let parsed = strtonum(
                CStr::from_ptr(value),
                oe.minimum as ::core::ffi::c_longlong,
                oe.maximum as ::core::ffi::c_longlong,
            );
            number = parsed.unwrap_or(0);
            if let Err(errstr) = parsed {
                format_options_cause!(cause, c"value is %s: %s", errstr, CStr::from_ptr(value),);
                return -(1 as ::core::ffi::c_int);
            }
            options_set_number(oo, name, number);
            return 0 as ::core::ffi::c_int;
        }
        2 => {
            key = key_string_parse_cstr(std::ffi::CStr::from_ptr(value)).unwrap_or(KEYC_UNKNOWN);
            if key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code {
                format_options_cause!(cause, c"bad key: %s", CStr::from_ptr(value),);
                return -(1 as ::core::ffi::c_int);
            }
            options_set_number(oo, name, key as ::core::ffi::c_longlong);
            return 0 as ::core::ffi::c_int;
        }
        3 => {
            number = colour_parse_cstr(std::ffi::CStr::from_ptr(value)).unwrap_or(-1)
                as ::core::ffi::c_longlong;
            if number == -(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong {
                format_options_cause!(cause, c"bad colour: %s", CStr::from_ptr(value),);
                return -(1 as ::core::ffi::c_int);
            }
            options_set_number(oo, name, number);
            return 0 as ::core::ffi::c_int;
        }
        4 => return options_from_string_flag(oo, name, nullable_cstr(value), cause),
        5 => return options_from_string_choice(oe, oo, name, nullable_cstr(value), cause),
        6 => {
            pr = cmd_parse_from_string(
                CStr::from_ptr(value),
                ::core::ptr::null_mut::<cmd_parse_input>(),
            );
            match pr.status as ::core::ffi::c_uint {
                0 => {
                    if !cause.is_null() {
                        *cause = pr.error;
                    }
                    return -(1 as ::core::ffi::c_int);
                }
                1 => {
                    options_set_command(oo, name, pr.take_cmdlist());
                    return 0 as ::core::ffi::c_int;
                }
                _ => {}
            }
        }
        _ => {}
    }
    -(1 as ::core::ffi::c_int)
}
