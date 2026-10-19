//! Foreign declarations supplied by libc.
//! Only what the libc crate lacks is declared here; use the libc crate for the rest.

use crate::shared::stdio::FILE;
use crate::shared::utf8::wchar_t;

#[cfg(target_os = "linux")]
extern "C" {
    pub static mut program_invocation_short_name: *mut ::core::ffi::c_char;
}

extern "C" {
    #[cfg_attr(target_vendor = "apple", link_name = "__stderrp")]
    pub static mut stderr: *mut FILE;
    #[cfg_attr(target_vendor = "apple", link_name = "__stdoutp")]
    pub static mut stdout: *mut FILE;
    pub static mut environ: *mut *mut ::core::ffi::c_char;
    pub fn err(_: ::core::ffi::c_int, _: *const ::core::ffi::c_char, ...);
    pub fn errx(_: ::core::ffi::c_int, _: *const ::core::ffi::c_char, ...);
    pub fn tzset();
    pub fn warnx(_: *const ::core::ffi::c_char, ...);
    pub fn wctomb(__s: *mut ::core::ffi::c_char, __wchar: wchar_t) -> ::core::ffi::c_int;
}
