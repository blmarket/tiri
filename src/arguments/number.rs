//! Converting argument strings to bounded numbers.

use super::store::{args, args_last_string, args_last_value};
use super::value::ARGS_STRING;
use crate::src::format::format_single_from_target_cstring;
use crate::src::shared::abi::*;
use crate::src::shared::command::cmdq_item;
use std::ffi::{CStr, CString};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ArgumentValueError {
    Missing,
    Empty,
    Invalid,
    TooSmall,
    TooLarge,
}

impl ArgumentValueError {
    pub fn message(self) -> &'static CStr {
        match self {
            Self::Missing => c"missing",
            Self::Empty => c"empty",
            Self::Invalid => c"invalid",
            Self::TooSmall => c"too small",
            Self::TooLarge => c"too large",
        }
    }
}

/// Parse a decimal integer with `strtonum` semantics: leading C whitespace and
/// one sign are accepted, anything after the digits is invalid.
pub(super) fn parse_number(
    value: &CStr,
    minval: i64,
    maxval: i64,
) -> Result<i64, ArgumentValueError> {
    if minval > maxval {
        return Err(ArgumentValueError::Invalid);
    }
    let bytes = value.to_bytes();
    let start = bytes
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
        .unwrap_or(bytes.len());
    let (negative, digits) = match bytes[start..].split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, &bytes[start..]),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err(ArgumentValueError::Invalid);
    }
    let mut number: i64 = 0;
    for digit in digits {
        let digit = i64::from(digit - b'0');
        number = number
            .checked_mul(10)
            .and_then(|number| {
                if negative {
                    number.checked_sub(digit)
                } else {
                    number.checked_add(digit)
                }
            })
            .ok_or(if negative {
                ArgumentValueError::TooSmall
            } else {
                ArgumentValueError::TooLarge
            })?;
    }
    if number < minval {
        return Err(ArgumentValueError::TooSmall);
    }
    if number > maxval {
        return Err(ArgumentValueError::TooLarge);
    }
    Ok(number)
}

fn percentage_share(
    curval: i64,
    percentage: i64,
    minval: i64,
    maxval: i64,
) -> Result<i64, ArgumentValueError> {
    let value = (curval as i128 * percentage as i128) / 100;
    if value < minval as i128 {
        return Err(ArgumentValueError::TooSmall);
    }
    if value > maxval as i128 {
        return Err(ArgumentValueError::TooLarge);
    }
    Ok(value as i64)
}

fn parse_percentage(
    value: &CStr,
    minval: i64,
    maxval: i64,
    curval: i64,
) -> Result<i64, ArgumentValueError> {
    let bytes = value.to_bytes();
    if bytes.is_empty() {
        return Err(ArgumentValueError::Empty);
    }
    if let Some(percentage) = bytes.strip_suffix(b"%") {
        let percentage = CString::new(percentage).map_err(|_| ArgumentValueError::Invalid)?;
        let percentage = parse_number(percentage.as_c_str(), 0, 1000)?;
        return percentage_share(curval, percentage, minval, maxval);
    }
    parse_number(value, minval, maxval)
}

/// Converts the last string value stored for `flag` to a bounded integer.
pub fn args_strtonum_result(
    args: &args,
    flag: u_char,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
) -> Result<i64, ArgumentValueError> {
    let value = args_last_string(args, flag).ok_or(ArgumentValueError::Missing)?;
    parse_number(value, minval, maxval)
}

/// Converts the last string value after format expansion to a bounded integer.
///
/// # Safety
/// `item` must be valid for format expansion.
pub unsafe fn args_strtonum_and_expand_result(
    args: &args,
    flag: u_char,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
) -> Result<i64, ArgumentValueError> {
    let value = args_last_string(args, flag).ok_or(ArgumentValueError::Missing)?;
    let formatted = format_single_from_target_cstring(
        (item_handle).expect("command queue item"),
        value.as_ptr(),
    );
    parse_number(formatted.as_c_str(), minval, maxval)
}

/// Converts the last stored string as an integer or percentage.
pub fn args_percentage_result(
    args: &args,
    flag: u_char,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
    curval: ::core::ffi::c_longlong,
) -> Result<i64, ArgumentValueError> {
    if !args.tree.contains_key(&flag) {
        return Err(ArgumentValueError::Missing);
    }
    let value = args_last_value(args, flag).ok_or(ArgumentValueError::Empty)?;
    if value.type_0() as ::core::ffi::c_uint
        != ARGS_STRING as ::core::ffi::c_int as ::core::ffi::c_uint
        || value.as_string().is_none()
    {
        return Err(ArgumentValueError::Missing);
    }
    parse_percentage(
        value.as_string().expect("string argument"),
        minval,
        maxval,
        curval,
    )
}
