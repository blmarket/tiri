use crate::shared::abi::*;
use crate::shared::posix_terminal::winsize;
use crate::shared::terminal::*;
use libc::forkpty;
pub unsafe fn getptmfd() -> ::core::ffi::c_int {
    2147483647 as ::core::ffi::c_int
}
pub unsafe fn fdforkpty(
    _ptmfd: ::core::ffi::c_int,
    mut master: *mut ::core::ffi::c_int,
    mut name: *mut ::core::ffi::c_char,
    mut tio: *mut termios,
    mut ws: *mut winsize,
) -> pid_t {
    forkpty(master, name, tio, ws) as pid_t
}
