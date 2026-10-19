//! Printing and escaping arguments.

use super::store::args;
use super::value::{args_value, ParseValue};
use crate::cmd::cmd_list_print_cstring;
use crate::shared::vis::{VIS_CSTYLE, VIS_DQ, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::text::utf8::utf8_strvis;
use std::borrow::Cow;
use std::ffi::{CStr, CString};

pub(super) fn args_value_for_log<'a>(value: ParseValue<'a>) -> Cow<'a, CStr> {
    match value {
        ParseValue::String(text) => Cow::Borrowed(text),
        ParseValue::Commands(commands) => Cow::Owned(cmd_list_print_cstring(&commands.borrow(), 0)),
    }
}

fn args_print_add_value(buf: &mut Vec<u8>, value: &args_value) {
    if !buf.is_empty() {
        buf.push(b' ');
    }
    match value {
        args_value::Commands(commands, _) => {
            let expanded = cmd_list_print_cstring(&commands.borrow(), 0);
            buf.extend_from_slice(b"{ ");
            buf.extend_from_slice(expanded.as_bytes());
            buf.extend_from_slice(b" }");
        }
        args_value::String(text) => {
            let expanded = args_escape_cstring(text);
            buf.extend_from_slice(expanded.as_bytes());
        }
        args_value::None => {}
    }
}
pub(crate) fn args_print_cstring(args: &args) -> CString {
    let mut buf = Vec::new();
    for (&flag, entry) in &args.tree {
        if !entry.optional && entry.values.is_empty() {
            if buf.is_empty() {
                buf.push(b'-');
            }
            for _ in 0..entry.count {
                if flag != 0 {
                    buf.push(flag);
                }
            }
        }
    }
    let mut last_optional = false;
    for (&flag, entry) in &args.tree {
        if entry.optional {
            if !buf.is_empty() {
                buf.push(b' ');
            }
            buf.push(b'-');
            if flag != 0 {
                buf.push(flag);
            }
            last_optional = true;
        } else if !entry.values.is_empty() {
            for value in &entry.values {
                if !buf.is_empty() {
                    buf.push(b' ');
                }
                buf.push(b'-');
                if flag != 0 {
                    buf.push(flag);
                }
                args_print_add_value(&mut buf, value);
            }
            last_optional = false;
        }
    }
    if last_optional {
        buf.extend_from_slice(b" --");
    }
    for value in &args.values {
        args_print_add_value(&mut buf, value);
    }
    CString::new(buf).expect("printed arguments contain no interior NUL")
}
pub(crate) fn args_escape_cstring(s: &CStr) -> CString {
    let source = s.to_bytes();
    if source.is_empty() {
        return CString::new(b"''".to_vec()).expect("literal has no NUL");
    }

    let quotes = if source.iter().any(|byte| b" #';${}%".contains(byte)) {
        b'"'
    } else if source.iter().any(|byte| b" \"".contains(byte)) {
        b'\''
    } else {
        0
    };
    if source.len() == 1 && source[0] != b' ' && (quotes != 0 || source[0] == b'~') {
        return CString::new(vec![b'\\', source[0]]).expect("source has no NUL");
    }

    let mut flags = VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL;
    if quotes == b'"' {
        flags |= VIS_DQ;
    }
    let mut escaped = vec![
        0;
        source
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(1))
            .expect("escaped argument too long")
    ];
    let length = utf8_strvis(&mut escaped, source, flags);
    escaped.truncate(length);

    let mut result = Vec::with_capacity(escaped.len() + 3);
    if quotes == b'\'' {
        result.push(b'\'');
        result.extend_from_slice(&escaped);
        result.push(b'\'');
    } else if quotes == b'"' {
        result.push(b'"');
        if escaped.first() == Some(&b'~') {
            result.push(b'\\');
        }
        result.extend_from_slice(&escaped);
        result.push(b'"');
    } else {
        if escaped.first() == Some(&b'~') {
            result.push(b'\\');
        }
        result.extend_from_slice(&escaped);
    }
    CString::new(result).expect("utf8_strvis output has no interior NUL")
}
