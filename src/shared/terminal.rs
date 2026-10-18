//! Authoritative terminal ABI declarations.

pub use ::libc::termios;

/// Terminal attributes owned by a model record. libc's termios has no
/// Default; the zeroed record is the translated model's initial value.
#[derive(Copy, Clone)]
pub struct TerminalAttributes(pub termios);

impl Default for TerminalAttributes {
    fn default() -> Self {
        Self(unsafe { ::core::mem::zeroed() })
    }
}

/// Borrow a raw descriptor that may be negative; a negative one reports EBADF
/// the way the termios calls would.
unsafe fn borrow_terminal(fd: i32) -> std::io::Result<std::os::fd::BorrowedFd<'static>> {
    if fd < 0 {
        return Err(std::io::Error::from_raw_os_error(libc::EBADF));
    }
    Ok(std::os::fd::BorrowedFd::borrow_raw(fd))
}

pub(crate) unsafe fn read_attributes(fd: i32) -> std::io::Result<termios> {
    tiri_rt::unix::terminal_attributes(borrow_terminal(fd)?)
}

pub(crate) unsafe fn set_attributes(
    fd: i32,
    action: i32,
    attributes: &termios,
) -> std::io::Result<()> {
    tiri_rt::unix::set_terminal_attributes(borrow_terminal(fd)?, action, attributes)
}

pub(crate) unsafe fn read_size(fd: i32) -> std::io::Result<super::posix_terminal::winsize> {
    tiri_rt::unix::terminal_size(borrow_terminal(fd)?)
}

pub(crate) unsafe fn set_size(
    fd: i32,
    size: &super::posix_terminal::winsize,
) -> std::io::Result<()> {
    tiri_rt::unix::set_terminal_size(borrow_terminal(fd)?, size)
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
