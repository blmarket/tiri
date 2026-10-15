use crate::src::compat::getprogname::getprogname;
use crate::src::shared::abi::*;
use libc::{snprintf, strrchr};

#[cfg(target_os = "linux")]
pub unsafe fn setproctitle(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
    let mut title: [::core::ffi::c_char; 16] = [0; 16];
    let mut name: [::core::ffi::c_char; 16] = [0; 16];
    let mut cp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut used: ::core::ffi::c_int = 0;
    let formatted = crate::src::format::bytes::format_message_with(write);
    for (dst, &byte) in title[..15].iter_mut().zip(formatted.as_bytes()) {
        *dst = byte as ::core::ffi::c_char;
    }
    used = snprintf(
        &raw mut name as *mut ::core::ffi::c_char,
        ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t,
        c"%s: %s".as_ptr(),
        getprogname().as_ptr(),
        &raw mut title as *mut ::core::ffi::c_char,
    );
    if used >= ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as ::core::ffi::c_int {
        cp = strrchr(&raw mut name as *mut ::core::ffi::c_char, ' ' as i32);
        if !cp.is_null() {
            *cp = '\0' as i32 as ::core::ffi::c_char;
        }
    }
    ::libc::prctl(
        ::libc::PR_SET_NAME,
        &raw mut name as *mut ::core::ffi::c_char,
    );
}
// Like tmux's compat fallback, platforms without PR_SET_NAME keep their title.
#[cfg(not(target_os = "linux"))]
pub unsafe fn setproctitle(_write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {}
