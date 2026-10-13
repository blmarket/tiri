//! Authoritative terminal ABI declarations.

use crate::src::ffi::libc::__errno_location;

pub use ::libc::termios;

/// Terminal attributes owned by a model record. libc's termios has no
/// Default; the zeroed record is the translated model's initial value.
#[derive(Copy, Clone)]
pub struct TerminalAttributes(pub termios);

impl Default for TerminalAttributes {
    fn default() -> Self {
        // SAFETY: termios contains only integers and integer arrays.
        Self(unsafe { ::core::mem::zeroed() })
    }
}

// These functions retain the errno/return convention until the model is migrated.
pub(crate) unsafe fn read_attributes(fd: i32, attributes: &mut termios) -> i32 {
    if fd < 0 {
        *__errno_location() = libc::EBADF;
        return -1;
    }
    crate::src::reactor::io_status(
        tiri_rt::unix::terminal_attributes(std::os::fd::BorrowedFd::borrow_raw(fd))
            .map(|native| *attributes = native),
    )
}

pub(crate) unsafe fn set_attributes(fd: i32, action: i32, attributes: &termios) -> i32 {
    if fd < 0 {
        *__errno_location() = libc::EBADF;
        return -1;
    }
    crate::src::reactor::io_status(tiri_rt::unix::set_terminal_attributes(
        std::os::fd::BorrowedFd::borrow_raw(fd),
        action,
        attributes,
    ))
}

pub(crate) unsafe fn read_size(fd: i32, size: &mut super::posix_terminal::winsize) -> i32 {
    if fd < 0 {
        *__errno_location() = libc::EBADF;
        return -1;
    }
    crate::src::reactor::io_status(
        tiri_rt::unix::terminal_size(std::os::fd::BorrowedFd::borrow_raw(fd))
            .map(|native| *size = native),
    )
}

pub(crate) unsafe fn set_size(fd: i32, size: &super::posix_terminal::winsize) -> i32 {
    if fd < 0 {
        *__errno_location() = libc::EBADF;
        return -1;
    }
    crate::src::reactor::io_status(tiri_rt::unix::set_terminal_size(
        std::os::fd::BorrowedFd::borrow_raw(fd),
        size,
    ))
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
