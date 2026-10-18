//! Argument values: owned or borrowed strings and command lists.

use crate::src::shared::command::cmd_list;
use std::ffi::{CStr, CString};
use std::rc::Rc;

pub(super) type args_type = ::core::ffi::c_uint;
pub const ARGS_COMMANDS: args_type = 2;
pub const ARGS_STRING: args_type = 1;
pub(super) const ARGS_NONE: args_type = 0;

enum ArgsPayload<'a> {
    None,
    String(CString),
    Command(Rc<std::cell::RefCell<cmd_list>>),
    BorrowedString(&'a CStr),
    BorrowedCommand(&'a Rc<std::cell::RefCell<cmd_list>>),
}

/// Parser inputs can borrow payloads; stored arguments contain owned copies.
pub struct ArgumentValue<'a> {
    payload: ArgsPayload<'a>,
    pub(super) cached: Option<CString>,
}

pub type args_value = ArgumentValue<'static>;

impl<'a> ArgumentValue<'a> {
    fn new(payload: ArgsPayload<'a>) -> Self {
        Self {
            payload,
            cached: None,
        }
    }

    pub fn empty() -> Self {
        Self::new(ArgsPayload::None)
    }

    pub fn string(value: CString) -> Self {
        Self::new(ArgsPayload::String(value))
    }

    pub fn commands(value: Rc<std::cell::RefCell<cmd_list>>) -> Self {
        Self::new(ArgsPayload::Command(value))
    }

    pub fn borrowed_string(value: &'a CStr) -> Self {
        Self::new(ArgsPayload::BorrowedString(value))
    }

    pub fn borrowed_commands(value: &'a Rc<std::cell::RefCell<cmd_list>>) -> Self {
        Self::new(ArgsPayload::BorrowedCommand(value))
    }

    pub fn type_0(&self) -> args_type {
        match self.payload {
            ArgsPayload::None => ARGS_NONE,
            ArgsPayload::String(_) | ArgsPayload::BorrowedString(_) => ARGS_STRING,
            ArgsPayload::Command(_) | ArgsPayload::BorrowedCommand(_) => ARGS_COMMANDS,
        }
    }

    pub fn as_string(&self) -> Option<&CStr> {
        match &self.payload {
            ArgsPayload::String(value) => Some(value),
            ArgsPayload::BorrowedString(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_commands(&self) -> Option<&Rc<std::cell::RefCell<cmd_list>>> {
        match &self.payload {
            ArgsPayload::Command(value) => Some(value),
            ArgsPayload::BorrowedCommand(value) => Some(value),
            _ => None,
        }
    }

    /// Copy into stored arguments without carrying any parser-input borrow or cache.
    pub(super) fn to_owned(&self) -> args_value {
        if let Some(text) = self.as_string() {
            args_value::string(text.to_owned())
        } else if let Some(commands) = self.as_commands() {
            args_value::commands(Rc::clone(commands))
        } else {
            args_value::empty()
        }
    }
}
