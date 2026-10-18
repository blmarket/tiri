//! Parsing raw command values into an argument set.

use super::print::{args_type_to_string, args_value_for_log};
use super::store::{
    args, args_create, args_push_positional_owned, args_set_value, ARGS_ENTRY_OPTIONAL_VALUE,
};
use super::value::{args_value, ArgumentValue, ARGS_COMMANDS, ARGS_STRING};
use crate::src::log::{log_byte, log_bytes, log_debug};
use crate::src::shared::abi::*;
use std::ffi::{CStr, CString};

pub type args_parse_type = ::core::ffi::c_uint;
pub(super) const ARGS_PARSE_COMMANDS: args_parse_type = 3;
pub const ARGS_PARSE_COMMANDS_OR_STRING: args_parse_type = 2;
pub const ARGS_PARSE_STRING: args_parse_type = 1;
pub(super) const ARGS_PARSE_INVALID: args_parse_type = 0;

#[derive(Debug)]
pub enum ArgsParseError {
    Usage,
    Message(std::ffi::CString),
}

type args_parse_cb = Option<fn(&mut args, u_int) -> Result<args_parse_type, ArgsParseError>>;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct args_parse {
    pub template: &'static CStr,
    pub lower: ::core::ffi::c_int,
    pub upper: ::core::ffi::c_int,
    pub cb: args_parse_cb,
}

fn parse_flag_error(prefix: &[u8], flag: u_char, suffix: &[u8]) -> CString {
    let mut bytes = Vec::with_capacity(prefix.len() + 1 + suffix.len());
    bytes.extend_from_slice(prefix);
    bytes.push(flag);
    bytes.extend_from_slice(suffix);
    CString::new(bytes).expect("argument flag diagnostics contain no NUL")
}

fn parse_number_error(message: String) -> CString {
    CString::new(message).expect("argument diagnostics contain no NUL")
}

fn args_copy_value(from: &ArgumentValue<'_>) -> args_value {
    from.to_owned()
}

unsafe fn args_parse_flag_argument(
    values: &[ArgumentValue<'_>],
    args: &mut args,
    i: &mut usize,
    suffix: &[u8],
    flag: u_char,
    optional_argument: bool,
) -> Result<(), CString> {
    let new = if !suffix.is_empty() {
        args_value::string(CString::new(suffix).expect("flag suffix contains no NUL"))
    } else {
        let argument = values.get(*i);
        if argument.is_some_and(|value| value.type_0() != ARGS_STRING) {
            return Err(parse_flag_error(b"-", flag, b" argument must be a string"));
        }
        let skip_optional = optional_argument
            && argument.is_none_or(|value| {
                let bytes = value.as_string().expect("string argument").to_bytes();
                bytes.first() == Some(&b'-')
                    && bytes.get(1).is_some_and(|next| {
                        *next == b'-' || ::libc::isalpha(*next as ::core::ffi::c_int) != 0
                    })
            });
        if skip_optional {
            log_debug(format_args!(
                "args_parse_flag_argument: -{} (optional)",
                log_byte(flag)
            ));
            args_set_value(args, flag, None, ARGS_ENTRY_OPTIONAL_VALUE);
            return Ok(());
        }
        let argument =
            argument.ok_or_else(|| parse_flag_error(b"-", flag, b" expects an argument"))?;
        *i += 1;
        args_copy_value(argument)
    };
    let printed = args_value_for_log(&new);
    log_debug(format_args!(
        "args_parse_flag_argument: -{} = {}",
        log_byte(flag),
        log_bytes(printed.to_bytes())
    ));
    args_set_value(args, flag, Some(new), 0);
    Ok(())
}

/// Return true when flags end, leaving the first positional value unconsumed.
unsafe fn args_parse_flags(
    parse: &args_parse,
    values: &[ArgumentValue<'_>],
    args: &mut args,
    i: &mut usize,
) -> Result<bool, ArgsParseError> {
    let value = &values[*i];
    if value.type_0() != ARGS_STRING {
        return Ok(true);
    }
    let string = value.as_string().expect("string argument").to_bytes();
    log_debug(format_args!("args_parse_flags: next {}", log_bytes(string)));
    if string.first() != Some(&b'-') || string.len() == 1 {
        return Ok(true);
    }
    *i += 1;
    if string == b"--" {
        return Ok(true);
    }
    let template = parse.template.to_bytes();
    for (offset, &flag) in string.iter().enumerate().skip(1) {
        if flag == b'?' {
            return Err(ArgsParseError::Usage);
        }
        if ::libc::isalnum(flag as ::core::ffi::c_int) == 0 {
            return Err(ArgsParseError::Message(parse_flag_error(
                b"invalid flag -",
                flag,
                b"",
            )));
        }
        let found = template
            .iter()
            .position(|byte| *byte == flag)
            .ok_or_else(|| {
                ArgsParseError::Message(parse_flag_error(b"unknown flag -", flag, b""))
            })?;
        if template.get(found + 1) != Some(&b':') {
            log_debug(format_args!("args_parse_flags: -{}", log_byte(flag)));
            args_set_value(args, flag, None, 0);
        } else {
            return args_parse_flag_argument(
                values,
                args,
                i,
                &string[offset + 1..],
                flag,
                template.get(found + 2) == Some(&b':'),
            )
            .map(|()| false)
            .map_err(ArgsParseError::Message);
        }
    }
    Ok(false)
}

pub unsafe fn args_parse(
    parse: &args_parse,
    values: &[ArgumentValue<'_>],
) -> Result<Box<args>, ArgsParseError> {
    let mut args = args_create();
    if values.is_empty() {
        return Ok(args);
    }
    let mut i = 1;
    while i < values.len() {
        if args_parse_flags(parse, values, &mut args, &mut i)? {
            break;
        }
    }
    log_debug(format_args!(
        "args_parse: flags end at {} of {}",
        i,
        values.len()
    ));
    for (index, value) in values.iter().enumerate().skip(i) {
        let printed = args_value_for_log(value);
        log_debug(format_args!(
            "args_parse: {} = {} (type {})",
            index,
            log_bytes(printed.to_bytes()),
            log_bytes(args_type_to_string(value.type_0()).to_bytes())
        ));
        let kind = if let Some(callback) = parse.cb {
            let index = args.values.len() as u_int;
            callback(&mut args, index)?
        } else {
            ARGS_PARSE_STRING
        };
        match kind {
            ARGS_PARSE_INVALID => return Err(ArgsParseError::Usage),
            ARGS_PARSE_STRING if value.type_0() != ARGS_STRING => {
                return Err(ArgsParseError::Message(parse_number_error(format!(
                    "argument {} must be \"string\"",
                    (args.values.len() as u_int).wrapping_add(1)
                ))));
            }
            ARGS_PARSE_COMMANDS if value.type_0() != ARGS_COMMANDS => {
                return Err(ArgsParseError::Message(parse_number_error(format!(
                    "argument {} must be {{ commands }}",
                    (args.values.len() as u_int).wrapping_add(1)
                ))));
            }
            _ => {}
        }
        let copied = match kind {
            ARGS_PARSE_STRING | ARGS_PARSE_COMMANDS_OR_STRING | ARGS_PARSE_COMMANDS => {
                args_copy_value(value)
            }
            _ => args_value::empty(),
        };
        args_push_positional_owned(&mut args, copied);
    }
    if parse.lower != -1 && (args.values.len() as u_int) < parse.lower as u_int {
        return Err(ArgsParseError::Message(parse_number_error(format!(
            "too few arguments (need at least {})",
            parse.lower as u_int
        ))));
    }
    if parse.upper != -1 && (args.values.len() as u_int) > parse.upper as u_int {
        return Err(ArgsParseError::Message(parse_number_error(format!(
            "too many arguments (need at most {})",
            parse.upper as u_int
        ))));
    }
    Ok(args)
}
