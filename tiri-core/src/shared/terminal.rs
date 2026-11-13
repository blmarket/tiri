//! Authoritative terminal ABI declarations.

pub use ::libc::termios;
use std::os::fd::BorrowedFd;

/// Terminal attributes owned by a model record. libc's termios has no
/// Default; the all-zero record is the translated model's initial value.
#[derive(Copy, Clone)]
pub struct TerminalAttributes(pub termios);

impl Default for TerminalAttributes {
    fn default() -> Self {
        Self(tiri_sys::empty_terminal_attributes())
    }
}

pub(crate) fn read_attributes(fd: BorrowedFd<'_>) -> std::io::Result<termios> {
    tiri_sys::terminal_attributes(fd)
}

pub(crate) fn set_attributes(
    fd: BorrowedFd<'_>,
    action: i32,
    attributes: &termios,
) -> std::io::Result<()> {
    tiri_sys::set_terminal_attributes(fd, action, attributes)
}

pub(crate) fn read_size(fd: BorrowedFd<'_>) -> std::io::Result<super::posix_terminal::winsize> {
    tiri_sys::terminal_size(fd)
}

pub(crate) fn set_size(
    fd: BorrowedFd<'_>,
    size: &super::posix_terminal::winsize,
) -> std::io::Result<()> {
    tiri_sys::set_terminal_size(fd, size)
}
