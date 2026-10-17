//! Typed option values parsed from text.
use super::diagnostic::options_string_cause;
use super::store::*;
use super::types::*;
use crate::src::compat::strtonum::strtonum;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::grid::grid_default_cell;
use crate::src::key_string::key_string_parse_cstr;
use libc::{fnmatch, strstr};

use crate::src::style::colour::colour_parse_cstr;
use crate::src::style::{style_parse, style_parse_colour};
use crate::src::tmux::checkshell;

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
unsafe fn options_from_string_check(oe: &options_table_entry, value: &CStr) -> Result<(), CString> {
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
        return Err(options_string_cause(c"not a suitable shell: %s", &[value]));
    }
    if oe.pattern.is_some_and(|pattern| {
        fnmatch(pattern.as_ptr(), value.as_ptr(), 0 as ::core::ffi::c_int)
            != 0 as ::core::ffi::c_int
    }) {
        return Err(options_string_cause(c"value is invalid: %s", &[value]));
    }
    if oe.flags & OPTIONS_TABLE_IS_STYLE != 0
        && strstr(value.as_ptr(), c"#{".as_ptr()).is_null()
        && style_parse(&raw mut sy, &grid_default_cell, value) != 0 as ::core::ffi::c_int
    {
        return Err(options_string_cause(c"invalid style: %s", &[value]));
    }
    if oe.flags & OPTIONS_TABLE_IS_COLOUR != 0
        && strstr(value.as_ptr(), c"#{".as_ptr()).is_null()
        && style_parse_colour(&raw mut sy, &grid_default_cell, value) != 0 as ::core::ffi::c_int
    {
        return Err(options_string_cause(c"invalid colour: %s", &[value]));
    }
    Ok(())
}
unsafe fn options_from_string_flag(
    oo: &mut options,
    name: &CStr,
    value: Option<&CStr>,
) -> Result<(), CString> {
    let flag = match value.map(CStr::to_bytes) {
        None | Some(b"") => options_get_number(oo, name) == 0,
        Some(v) if v == b"1" || v.eq_ignore_ascii_case(b"on") || v.eq_ignore_ascii_case(b"yes") => {
            true
        }
        Some(v) if v == b"0" || v.eq_ignore_ascii_case(b"off") || v.eq_ignore_ascii_case(b"no") => {
            false
        }
        Some(_) => return Err(options_string_cause(c"bad value: %s", &[value.unwrap()])),
    };
    options_set_number(oo, name, flag as ::core::ffi::c_longlong);
    Ok(())
}
pub fn options_find_choice(
    oe: &options_table_entry,
    value: &CStr,
) -> Result<::core::ffi::c_int, CString> {
    oe.choices
        .iter()
        .rposition(|candidate| *candidate == value)
        .map(|n| n as ::core::ffi::c_int)
        .ok_or_else(|| options_string_cause(c"unknown value: %s", &[value]))
}
unsafe fn options_from_string_choice(
    oe: &options_table_entry,
    oo: &mut options,
    name: &CStr,
    value: Option<&CStr>,
) -> Result<(), CString> {
    let choice = match value {
        None => {
            let choice = options_get_number(oo, name) as ::core::ffi::c_int;
            if choice < 2 {
                (choice == 0) as ::core::ffi::c_int
            } else {
                choice
            }
        }
        Some(value) => options_find_choice(oe, value)?,
    };
    options_set_number(oo, name, choice as ::core::ffi::c_longlong);
    Ok(())
}
/// Command options are parsed by `OptionsScope`, never through here.
pub unsafe fn options_from_string(
    oo: &mut options,
    oe: &options_table_entry,
    name: &CStr,
    value: Option<&CStr>,
    append: ::core::ffi::c_int,
) -> Result<(), CString> {
    let required = || value.ok_or_else(|| c"empty value".to_owned());
    match oe.type_0 {
        OPTIONS_TABLE_STRING => {
            let value = required()?;
            let old = options_get_string(oo, name);
            options_set_string(oo, name, append, |out| write_cstr(out, value));
            let new = options_get_string(oo, name);
            if let Err(error) = options_from_string_check(oe, &new) {
                options_set_string(oo, name, 0, |out| write_cstr(out, &*old));
                return Err(error);
            }
        }
        OPTIONS_TABLE_NUMBER => {
            let value = required()?;
            let number = strtonum(
                value,
                oe.minimum as ::core::ffi::c_longlong,
                oe.maximum as ::core::ffi::c_longlong,
            )
            .map_err(|errstr| options_string_cause(c"value is %s: %s", &[errstr, value]))?;
            options_set_number(oo, name, number);
        }
        OPTIONS_TABLE_KEY => {
            let value = required()?;
            let key = key_string_parse_cstr(value).unwrap_or(KEYC_UNKNOWN);
            if key == KEYC_UNKNOWN {
                return Err(options_string_cause(c"bad key: %s", &[value]));
            }
            options_set_number(oo, name, key as ::core::ffi::c_longlong);
        }
        OPTIONS_TABLE_COLOUR => {
            let value = required()?;
            let number = colour_parse_cstr(value)
                .filter(|&colour| colour != -1)
                .ok_or_else(|| options_string_cause(c"bad colour: %s", &[value]))?;
            options_set_number(oo, name, number as ::core::ffi::c_longlong);
        }
        OPTIONS_TABLE_FLAG => options_from_string_flag(oo, name, value)?,
        OPTIONS_TABLE_CHOICE => options_from_string_choice(oe, oo, name, value)?,
        _ => unreachable!("command options are parsed by OptionsScope"),
    }
    Ok(())
}
