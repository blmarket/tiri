use crate::shared::abi::*;
use crate::shared::posix_terminal::winsize;
use libc::forkpty;
pub fn getptmfd() -> ::core::ffi::c_int {
    2147483647 as ::core::ffi::c_int
}
/// Fork with a new pseudo-terminal using the default attributes. `master`
/// receives the parent's descriptor and `name` the NUL-terminated slave path.
pub unsafe fn fdforkpty(
    _ptmfd: ::core::ffi::c_int,
    master: &mut ::core::ffi::c_int,
    name: &mut [u8; 32],
    ws: &winsize,
) -> pid_t {
    forkpty(
        master,
        name.as_mut_ptr().cast(),
        ::core::ptr::null_mut(),
        (ws as *const winsize).cast_mut(),
    ) as pid_t
}
