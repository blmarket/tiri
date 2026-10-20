//! Authoritative prompt domains.

use super::abi::{size_t, u_int};
use super::command::cmd_find_state;
use super::display::screen_cursor_style;
use super::grid::{grid_cell, utf8_data};
use std::ffi::{CStr, CString};

/// The client, pane, or mode solely owns a prompt. Dispatch observes its identity.
pub type prompt_type = ::core::ffi::c_uint;
pub const PROMPT_TYPE_COMMAND: prompt_type = 0;
pub const PROMPT_TYPE_INVALID: prompt_type = 255;
pub const PROMPT_TYPE_SEARCH: prompt_type = 1;

pub type prompt_key_result = ::core::ffi::c_uint;
pub const PROMPT_KEY_NOT_HANDLED: prompt_key_result = 0;
pub const PROMPT_KEY_HANDLED: prompt_key_result = 1;
pub const PROMPT_KEY_CLOSE: prompt_key_result = 2;
pub const PROMPT_KEY_MOVE: prompt_key_result = 3;

pub const PROMPT_CLOSE: prompt_result = 1;
pub const PROMPT_CONTINUE: prompt_result = 0;
pub const PROMPT_SINGLE: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const PROMPT_NUMERIC: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const PROMPT_INCREMENTAL: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const PROMPT_KEY: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const PROMPT_BSPACE_EXIT: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const PROMPT_NOFREEZE: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const PROMPT_ISPANE: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;
pub const PROMPT_NTYPES: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const PROMPT_NOFORMAT: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const PROMPT_ACCEPT: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const PROMPT_ISMODE: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;
pub const PROMPT_QUOTENEXT: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const PROMPT_COMMANDMODE: ::core::ffi::c_int = 0x200 as ::core::ffi::c_int;
pub const PROMPT_EDITARROWS: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;

pub type prompt_result = ::core::ffi::c_uint;

pub type prompt_free_cb = Option<Box<dyn FnOnce()>>;
pub type prompt_input_cb =
    Option<Box<dyn FnMut(Option<&CStr>, prompt_key_result) -> prompt_result>>;

#[derive(Default)]
pub struct prompt_completion {
    pub names: Vec<CString>,
    pub display: Option<CString>,
}

#[derive(Default)]
pub struct prompt {
    pub string: CString,
    pub buffer: Vec<utf8_data>,
    pub state: cmd_find_state,
    pub last: Option<CString>,
    pub index: size_t,
    pub inputcb: prompt_input_cb,
    pub freecb: prompt_free_cb,
    pub message_format: CString,
    pub keys: ::core::ffi::c_int,
    pub word_separators: CString,
    pub style: grid_cell,
    pub command_style: grid_cell,
    pub style_str: CString,
    pub command_style_str: CString,
    pub cstyle: screen_cursor_style,
    pub command_cstyle: screen_cursor_style,
    pub ccolour: ::core::ffi::c_int,
    pub command_ccolour: ::core::ffi::c_int,
    pub cmode: ::core::ffi::c_int,
    pub command_cmode: ::core::ffi::c_int,
    pub type_0: prompt_type,
    pub flags: ::core::ffi::c_int,
    pub closed: ::core::ffi::c_int,
    pub hindex: [u_int; 2],
    pub copied: Option<Box<[utf8_data]>>,
    pub completion: prompt_completion,
}

/// Temporary inputs: labels and target state are borrowed during creation;
/// copied option strings and callbacks move into the heap-allocated prompt.
#[derive(Default)]
pub struct prompt_create_data<'a> {
    pub fs: Option<&'a cmd_find_state>,
    pub prompt: &'a CStr,
    pub input: Option<&'a CStr>,
    pub type_0: prompt_type,
    pub flags: ::core::ffi::c_int,
    pub style: grid_cell,
    pub command_style: grid_cell,
    pub style_str: CString,
    pub command_style_str: CString,
    pub cstyle: screen_cursor_style,
    pub command_cstyle: screen_cursor_style,
    pub ccolour: ::core::ffi::c_int,
    pub command_ccolour: ::core::ffi::c_int,
    pub cmode: ::core::ffi::c_int,
    pub command_cmode: ::core::ffi::c_int,
    pub message_format: CString,
    pub keys: ::core::ffi::c_int,
    pub word_separators: CString,
    pub inputcb: prompt_input_cb,
    pub freecb: prompt_free_cb,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct prompt_draw_data {
    pub area_x: u_int,
    pub area_width: u_int,
    pub prompt_line: u_int,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn prompt_domains_match_translated_c_baseline() {
        assert_eq!(size_of::<prompt_type>(), 4);
        assert_eq!(align_of::<prompt_type>(), 4);
        assert_eq!(size_of::<prompt_key_result>(), 4);
        assert_eq!(align_of::<prompt_key_result>(), 4);
        assert_eq!(PROMPT_TYPE_COMMAND, 0);
        assert_eq!(PROMPT_TYPE_SEARCH, 1);
        assert_eq!(PROMPT_TYPE_INVALID, 255);
        assert_eq!(PROMPT_KEY_NOT_HANDLED, 0);
        assert_eq!(PROMPT_KEY_MOVE, 3);
    }
}
