//! Authoritative argument storage, parsing callbacks, and scalar domains.

use super::abi::{u_char, u_int};
use super::command::{cmd_list, cmd_parse_input};
use crate::src::shared::client::ClientRef;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::rc::Rc;
pub type args_type = ::core::ffi::c_uint;
pub const ARGS_COMMANDS: args_type = 2;
pub const ARGS_STRING: args_type = 1;
pub const ARGS_NONE: args_type = 0;

pub type args_parse_type = ::core::ffi::c_uint;
pub const ARGS_PARSE_COMMANDS: args_parse_type = 3;
pub const ARGS_PARSE_COMMANDS_OR_STRING: args_parse_type = 2;
pub const ARGS_PARSE_STRING: args_parse_type = 1;
pub const ARGS_PARSE_INVALID: args_parse_type = 0;

#[derive(Debug)]
pub enum ArgsParseError {
    Usage,
    Message(std::ffi::CString),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn borrowed_payloads_copy_to_owners_without_retaining_input_storage() {
        let text = CString::new(b"borrowed\xff".as_slice()).unwrap();
        let commands = Rc::new(std::cell::RefCell::new(cmd_list {
            group: 0,
            list: Vec::new(),
        }));
        let observer = Rc::downgrade(&commands);
        let inputs = [
            ArgumentValue::borrowed_string(&text),
            ArgumentValue::borrowed_commands(&commands),
        ];
        assert_eq!(inputs[0].as_string().unwrap().as_ptr(), text.as_ptr());
        assert_eq!(Rc::strong_count(&commands), 1);
        let stored: Vec<args_value> = inputs.iter().map(ArgumentValue::to_owned).collect();
        assert_ne!(stored[0].as_string().unwrap().as_ptr(), text.as_ptr());
        assert!(Rc::ptr_eq(stored[1].as_commands().unwrap(), &commands));
        assert_eq!(Rc::strong_count(&commands), 2);
        drop(inputs);
        drop(text);
        drop(commands);
        assert_eq!(stored[0].as_string().unwrap().to_bytes(), b"borrowed\xff");
        assert_eq!(observer.strong_count(), 1);
        drop(stored);
        assert!(observer.upgrade().is_none());
    }

    #[test]
    fn argument_domains_match_translated_c_baseline() {
        assert_eq!(size_of::<args_type>(), 4);
        assert_eq!(align_of::<args_type>(), 4);
        assert_eq!(size_of::<args_parse_type>(), 4);
        assert_eq!(align_of::<args_parse_type>(), 4);
        assert_eq!(ARGS_NONE, 0);
        assert_eq!(ARGS_COMMANDS, 2);
        assert_eq!(ARGS_PARSE_INVALID, 0);
        assert_eq!(ARGS_PARSE_COMMANDS, 3);
    }
}

#[repr(C)]
pub struct args {
    pub tree: args_tree,
    pub values: Vec<args_value>,
}

pub enum ArgsPayload<'a> {
    None,
    String(CString),
    Command(Rc<std::cell::RefCell<cmd_list>>),
    BorrowedString(&'a CStr),
    BorrowedCommand(&'a Rc<std::cell::RefCell<cmd_list>>),
}

/// Parser inputs can borrow payloads; stored arguments contain owned copies.
pub struct ArgumentValue<'a> {
    pub payload: ArgsPayload<'a>,
    pub cached: Option<CString>,
}

pub type args_value = ArgumentValue<'static>;

impl<'a> ArgumentValue<'a> {
    pub fn new(payload: ArgsPayload<'a>) -> Self {
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
    pub fn to_owned(&self) -> args_value {
        if let Some(text) = self.as_string() {
            args_value::string(text.to_owned())
        } else if let Some(commands) = self.as_commands() {
            args_value::commands(Rc::clone(commands))
        } else {
            args_value::empty()
        }
    }
}

pub type args_tree = BTreeMap<u_char, Box<args_entry>>;

#[repr(C)]
/// Box-owned by the enclosing args tree; dropping it also drops its values.
pub struct args_entry {
    pub flag: u_char,
    pub values: args_values,
    pub count: u_int,
    pub flags: ::core::ffi::c_int,
}

/// Owns flag-value storage directly. Iteration borrows
/// the boxed records and cannot outlive the containing argument set.
pub type args_values = Vec<Box<args_value>>;

pub type args_parse_cb = Option<fn(&mut args, u_int) -> Result<args_parse_type, ArgsParseError>>;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct args_parse {
    pub template: &'static CStr,
    pub lower: ::core::ffi::c_int,
    pub upper: ::core::ffi::c_int,
    pub cb: args_parse_cb,
}

/// Box-owned prepared command; retained model references have typed owners.
pub struct args_command_state {
    pub cmdlist: Option<Rc<std::cell::RefCell<cmd_list>>>,
    pub(crate) client: Option<ClientRef>,
    pub cmd: Option<std::ffi::CString>,
    pub pi: cmd_parse_input,
    pub(crate) file: Option<std::ffi::CString>,
}

impl args_command_state {
    pub fn empty() -> Self {
        Self {
            cmdlist: None,
            client: None,
            cmd: Default::default(),
            pi: Default::default(),
            file: Default::default(),
        }
    }
}
