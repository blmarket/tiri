//! Foreign declarations supplied by libc.
//! Only what the libc crate lacks is declared here; use the libc crate for the rest.

use crate::shared::abi::size_t;
use crate::shared::stdio::FILE;
use crate::shared::utf8::wchar_t;

#[cfg(target_os = "linux")]
extern "C" {
    pub static mut program_invocation_short_name: *mut ::core::ffi::c_char;
}

extern "C" {
    /// POSIX basename, which glibc exports under its XPG name.
    #[cfg_attr(
        all(target_os = "linux", target_env = "gnu"),
        link_name = "__xpg_basename"
    )]
    pub fn basename(__path: *mut ::core::ffi::c_char) -> *mut ::core::ffi::c_char;
    #[cfg_attr(target_vendor = "apple", link_name = "__stderrp")]
    pub static mut stderr: *mut FILE;
    #[cfg_attr(target_vendor = "apple", link_name = "__stdoutp")]
    pub static mut stdout: *mut FILE;
    pub static mut environ: *mut *mut ::core::ffi::c_char;
    pub fn err(_: ::core::ffi::c_int, _: *const ::core::ffi::c_char, ...);
    pub fn errx(_: ::core::ffi::c_int, _: *const ::core::ffi::c_char, ...);
    pub fn getc(__stream: *mut FILE) -> ::core::ffi::c_int;
    pub fn llabs(__x: ::core::ffi::c_longlong) -> ::core::ffi::c_longlong;
    pub fn strlcat(
        __dest: *mut ::core::ffi::c_char,
        __src: *const ::core::ffi::c_char,
        __n: size_t,
    ) -> ::core::ffi::c_ulong;
    pub fn strlcpy(
        __dest: *mut ::core::ffi::c_char,
        __src: *const ::core::ffi::c_char,
        __n: size_t,
    ) -> ::core::ffi::c_ulong;
    pub fn strsep(
        __stringp: *mut *mut ::core::ffi::c_char,
        __delim: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    pub fn tzset();
    pub fn warnx(_: *const ::core::ffi::c_char, ...);
    pub fn wctomb(__s: *mut ::core::ffi::c_char, __wchar: wchar_t) -> ::core::ffi::c_int;
}
