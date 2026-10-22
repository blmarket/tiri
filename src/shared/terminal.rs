//! Authoritative terminal ABI declarations.

pub use ::libc::termios;
use std::os::fd::BorrowedFd;

/// Terminal attributes owned by a model record. libc's termios has no
/// Default; the zeroed record is the translated model's initial value.
#[derive(Copy, Clone)]
pub struct TerminalAttributes(pub termios);

impl Default for TerminalAttributes {
    fn default() -> Self {
        Self(unsafe { ::core::mem::zeroed() })
    }
}

pub(crate) fn read_attributes(fd: BorrowedFd<'_>) -> std::io::Result<termios> {
    tiri_rt::unix::terminal_attributes(fd)
}

pub(crate) fn set_attributes(
    fd: BorrowedFd<'_>,
    action: i32,
    attributes: &termios,
) -> std::io::Result<()> {
    tiri_rt::unix::set_terminal_attributes(fd, action, attributes)
}

pub(crate) fn read_size(fd: BorrowedFd<'_>) -> std::io::Result<super::posix_terminal::winsize> {
    tiri_rt::unix::terminal_size(fd)
}

pub(crate) fn set_size(
    fd: BorrowedFd<'_>,
    size: &super::posix_terminal::winsize,
) -> std::io::Result<()> {
    tiri_rt::unix::set_terminal_size(fd, size)
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct termtype {
    pub term_names: *mut ::core::ffi::c_char,
    pub str_table: *mut ::core::ffi::c_char,
    pub Booleans: *mut ::core::ffi::c_char,
    pub Numbers: *mut ::core::ffi::c_short,
    pub Strings: *mut *mut ::core::ffi::c_char,
    pub ext_str_table: *mut ::core::ffi::c_char,
    pub ext_Names: *mut *mut ::core::ffi::c_char,
    pub num_Booleans: ::core::ffi::c_ushort,
    pub num_Numbers: ::core::ffi::c_ushort,
    pub num_Strings: ::core::ffi::c_ushort,
    pub ext_Booleans: ::core::ffi::c_ushort,
    pub ext_Numbers: ::core::ffi::c_ushort,
    pub ext_Strings: ::core::ffi::c_ushort,
}

pub type TERMTYPE = termtype;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct term {
    pub type_0: TERMTYPE,
}
