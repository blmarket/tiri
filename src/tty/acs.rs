use super::model::tty;
use crate::shared::abi::*;
use crate::shared::grid::*;
use crate::shared::tty::*;
use crate::tty::term::tty_term_owner_ptr;
use crate::tty::term::{tty_term_has, tty_term_number};
use std::ffi::CStr;

#[derive(Copy, Clone)]
#[repr(C)]
pub(super) struct tty_acs_entry {
    pub key: u_char,
    pub string: &'static ::std::ffi::CStr,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub(super) struct tty_acs_reverse_entry {
    pub string: &'static ::std::ffi::CStr,
    pub key: u_char,
}

static tty_acs_table: [tty_acs_entry; 36] = [
    tty_acs_entry {
        key: '+' as i32 as u_char,
        string: c"\xE2\x86\x92",
    },
    tty_acs_entry {
        key: ',' as i32 as u_char,
        string: c"\xE2\x86\x90",
    },
    tty_acs_entry {
        key: '-' as i32 as u_char,
        string: c"\xE2\x86\x91",
    },
    tty_acs_entry {
        key: '.' as i32 as u_char,
        string: c"\xE2\x86\x93",
    },
    tty_acs_entry {
        key: '0' as i32 as u_char,
        string: c"\xE2\x96\xAE",
    },
    tty_acs_entry {
        key: '`' as i32 as u_char,
        string: c"\xE2\x97\x86",
    },
    tty_acs_entry {
        key: 'a' as i32 as u_char,
        string: c"\xE2\x96\x92",
    },
    tty_acs_entry {
        key: 'b' as i32 as u_char,
        string: c"\xE2\x90\x89",
    },
    tty_acs_entry {
        key: 'c' as i32 as u_char,
        string: c"\xE2\x90\x8C",
    },
    tty_acs_entry {
        key: 'd' as i32 as u_char,
        string: c"\xE2\x90\x8D",
    },
    tty_acs_entry {
        key: 'e' as i32 as u_char,
        string: c"\xE2\x90\x8A",
    },
    tty_acs_entry {
        key: 'f' as i32 as u_char,
        string: c"\xC2\xB0",
    },
    tty_acs_entry {
        key: 'g' as i32 as u_char,
        string: c"\xC2\xB1",
    },
    tty_acs_entry {
        key: 'h' as i32 as u_char,
        string: c"\xE2\x90\xA4",
    },
    tty_acs_entry {
        key: 'i' as i32 as u_char,
        string: c"\xE2\x90\x8B",
    },
    tty_acs_entry {
        key: 'j' as i32 as u_char,
        string: c"\xE2\x94\x98",
    },
    tty_acs_entry {
        key: 'k' as i32 as u_char,
        string: c"\xE2\x94\x90",
    },
    tty_acs_entry {
        key: 'l' as i32 as u_char,
        string: c"\xE2\x94\x8C",
    },
    tty_acs_entry {
        key: 'm' as i32 as u_char,
        string: c"\xE2\x94\x94",
    },
    tty_acs_entry {
        key: 'n' as i32 as u_char,
        string: c"\xE2\x94\xBC",
    },
    tty_acs_entry {
        key: 'o' as i32 as u_char,
        string: c"\xE2\x8E\xBA",
    },
    tty_acs_entry {
        key: 'p' as i32 as u_char,
        string: c"\xE2\x8E\xBB",
    },
    tty_acs_entry {
        key: 'q' as i32 as u_char,
        string: c"\xE2\x94\x80",
    },
    tty_acs_entry {
        key: 'r' as i32 as u_char,
        string: c"\xE2\x8E\xBC",
    },
    tty_acs_entry {
        key: 's' as i32 as u_char,
        string: c"\xE2\x8E\xBD",
    },
    tty_acs_entry {
        key: 't' as i32 as u_char,
        string: c"\xE2\x94\x9C",
    },
    tty_acs_entry {
        key: 'u' as i32 as u_char,
        string: c"\xE2\x94\xA4",
    },
    tty_acs_entry {
        key: 'v' as i32 as u_char,
        string: c"\xE2\x94\xB4",
    },
    tty_acs_entry {
        key: 'w' as i32 as u_char,
        string: c"\xE2\x94\xAC",
    },
    tty_acs_entry {
        key: 'x' as i32 as u_char,
        string: c"\xE2\x94\x82",
    },
    tty_acs_entry {
        key: 'y' as i32 as u_char,
        string: c"\xE2\x89\xA4",
    },
    tty_acs_entry {
        key: 'z' as i32 as u_char,
        string: c"\xE2\x89\xA5",
    },
    tty_acs_entry {
        key: '{' as i32 as u_char,
        string: c"\xCF\x80",
    },
    tty_acs_entry {
        key: '|' as i32 as u_char,
        string: c"\xE2\x89\xA0",
    },
    tty_acs_entry {
        key: '}' as i32 as u_char,
        string: c"\xC2\xA3",
    },
    tty_acs_entry {
        key: '~' as i32 as u_char,
        string: c"\xC2\xB7",
    },
];
static tty_acs_reverse2: [tty_acs_reverse_entry; 1] = [tty_acs_reverse_entry {
    string: c"\xC2\xB7",
    key: '~' as i32 as u_char,
}];
static tty_acs_reverse3: [tty_acs_reverse_entry; 32] = [
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x80",
        key: 'q' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x81",
        key: 'q' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x82",
        key: 'x' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x83",
        key: 'x' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x8C",
        key: 'l' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x8F",
        key: 'k' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x90",
        key: 'k' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x93",
        key: 'l' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x94",
        key: 'm' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x97",
        key: 'm' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x98",
        key: 'j' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x9B",
        key: 'j' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\x9C",
        key: 't' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xA3",
        key: 't' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xA4",
        key: 'u' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xAB",
        key: 'u' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xB3",
        key: 'w' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xB4",
        key: 'v' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xBB",
        key: 'v' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x94\xBC",
        key: 'n' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x8B",
        key: 'n' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x90",
        key: 'q' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x91",
        key: 'x' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x94",
        key: 'l' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x97",
        key: 'k' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x9A",
        key: 'm' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\x9D",
        key: 'j' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\xA0",
        key: 't' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\xA3",
        key: 'u' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\xA6",
        key: 'w' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\xA9",
        key: 'v' as i32 as u_char,
    },
    tty_acs_reverse_entry {
        string: c"\xE2\x95\xAC",
        key: 'n' as i32 as u_char,
    },
];
static tty_acs_double_borders_list: [utf8_data; 13] = {
    [
        utf8_data {
            data: *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 0 as u_char,
            width: 0 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x91\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x90\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x94\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x97\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x9A\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x9D\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xA6\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xA9\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xA0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xA3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xAC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xC2\xB7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 2 as u_char,
            width: 1 as u_char,
        },
    ]
};
static tty_acs_heavy_borders_list: [utf8_data; 13] = {
    [
        utf8_data {
            data: *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 0 as u_char,
            width: 0 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x83\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x81\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x8F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x93\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x97\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x9B\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xBB\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xA3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xAB\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x8B\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xC2\xB7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 2 as u_char,
            width: 1 as u_char,
        },
    ]
};
static tty_acs_rounded_borders_list: [utf8_data; 13] = {
    [
        utf8_data {
            data: *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 0 as u_char,
            width: 0 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x82\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x80\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xAD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xAE\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xB0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\xAF\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xB3\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xBB\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\x9C\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x94\xA4\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xE2\x95\x8B\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        },
        utf8_data {
            data: *b"\xC2\xB7\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 0 as u_char,
            size: 2 as u_char,
            width: 1 as u_char,
        },
    ]
};
pub fn tty_acs_double_borders(cell_type: ::core::ffi::c_int) -> &'static utf8_data {
    &tty_acs_double_borders_list[cell_type as usize]
}
pub fn tty_acs_heavy_borders(cell_type: ::core::ffi::c_int) -> &'static utf8_data {
    &tty_acs_heavy_borders_list[cell_type as usize]
}
pub fn tty_acs_rounded_borders(cell_type: ::core::ffi::c_int) -> &'static utf8_data {
    &tty_acs_rounded_borders_list[cell_type as usize]
}
pub(super) fn tty_acs_needed(terminal: Option<&tty>, utf8: bool) -> ::core::ffi::c_int {
    let Some(terminal) = terminal else {
        return 0;
    };
    if tty_term_has(
        tty_term_owner_ptr(&terminal.term).expect("client has a terminal"),
        TTYC_U8,
    ) != 0
        && tty_term_number(
            tty_term_owner_ptr(&terminal.term).expect("client has a terminal"),
            TTYC_U8,
        ) == 0
    {
        return 1;
    }
    i32::from(!utf8)
}

/// The terminal and its capabilities must remain valid while the result is borrowed.
pub fn tty_acs_get(terminal: Option<&tty>, utf8: bool, ch: u_char) -> Option<&CStr> {
    if tty_acs_needed(terminal, utf8) != 0 {
        let term = terminal
            .expect("legacy ACS requires a terminal")
            .term
            .as_deref()
            .expect("terminal capabilities");
        let bytes = &term.acs[ch as usize];
        if bytes[0] == 0 {
            return None;
        }
        return Some(CStr::from_bytes_with_nul(bytes).expect("ACS entries are terminated"));
    }
    tty_acs_table
        .binary_search_by_key(&ch, |entry| entry.key)
        .ok()
        .map(|index| tty_acs_table[index].string)
}

pub(super) fn tty_acs_reverse_get(s: &CStr, slen: usize) -> Option<u_char> {
    let table = match slen {
        2 => &tty_acs_reverse2[..],
        3 => &tty_acs_reverse3[..],
        _ => return None,
    };
    table
        .binary_search_by(|entry| entry.string.to_bytes().cmp(s.to_bytes()))
        .ok()
        .map(|index| table[index].key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::borders::{CELL_LR, CELL_RD};

    #[test]
    fn border_lookup_and_reverse_translation_preserve_table_selection() {
        for (cell, expected) in [
            (tty_acs_double_borders(CELL_LR), "═"),
            (tty_acs_heavy_borders(CELL_LR), "━"),
            (tty_acs_rounded_borders(CELL_RD), "╭"),
        ] {
            assert_eq!(&cell.data[..cell.size as usize], expected.as_bytes());
            assert_eq!(cell.width, 1);
        }
        assert_eq!(tty_acs_reverse_get(c"─", 3), Some(b'q'));
        assert_eq!(tty_acs_reverse_get(c"│", 3), Some(b'x'));
        assert_eq!(tty_acs_reverse_get(c"·", 2), Some(b'~'));
        assert_eq!(tty_acs_reverse_get(c"─", 2), None);
        assert_eq!(tty_acs_reverse_get(c"─", 4), None);
        assert_eq!(tty_acs_reverse_get(c"─x", 3), None);
        assert_eq!(tty_acs_reverse_get(c"漢", 3), None);
    }
}
