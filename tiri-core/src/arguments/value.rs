//! Argument values: parser input borrows, stored arguments own.

use std::cell::OnceCell;
use std::ffi::{CStr, CString};

/// Parser input; `args_parse` copies what it keeps into `args_value`.
#[derive(Copy, Clone)]
pub enum ParseValue<'a> {
    String(&'a CStr),
    Commands(&'a dyn crate::actions::ProgramSource),
}

impl<'a> ParseValue<'a> {
    pub fn as_string(&self) -> Option<&'a CStr> {
        match *self {
            ParseValue::String(value) => Some(value),
            ParseValue::Commands(_) => None,
        }
    }

    pub(super) fn type_name(&self) -> &'static CStr {
        match self {
            ParseValue::String(_) => c"STRING",
            ParseValue::Commands(_) => c"COMMANDS",
        }
    }

    /// Copy into stored arguments without carrying any parser-input borrow.
    pub(super) fn to_args_value(self) -> args_value {
        match self {
            ParseValue::String(text) => args_value::String(text.to_owned()),
            ParseValue::Commands(commands) => args_value::commands(commands.program_ref()),
        }
    }
}

/// Stored argument value. A `{ commands }` value carries the lazily printed
/// text that `args_string` hands out.
pub enum args_value {
    None,
    String(CString),
    Commands(crate::actions::ProgramRef, OnceCell<CString>),
}

impl args_value {
    pub fn commands(value: crate::actions::ProgramRef) -> Self {
        args_value::Commands(value, OnceCell::new())
    }

    pub fn as_string(&self) -> Option<&CStr> {
        match self {
            args_value::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_commands(&self) -> Option<&crate::actions::ProgramRef> {
        match self {
            args_value::Commands(value, _) => Some(value),
            _ => None,
        }
    }

    /// Borrow a stored value as input for another parse.
    pub fn as_parse_value(&self) -> ParseValue<'_> {
        match self {
            args_value::String(value) => ParseValue::String(value),
            args_value::Commands(value, _) => ParseValue::Commands(value),
            args_value::None => unreachable!("stored argument without a value"),
        }
    }
}
