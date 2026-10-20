//! Foreign declarations supplied by utf8proc.

use crate::shared::abi::__int32_t;
pub type int32_t = __int32_t;

pub type utf8proc_int32_t = int32_t;

pub type utf8proc_category_t = ::core::ffi::c_uint;

extern "C" {
    pub fn utf8proc_category(codepoint: utf8proc_int32_t) -> utf8proc_category_t;
    pub fn utf8proc_charwidth(codepoint: utf8proc_int32_t) -> ::core::ffi::c_int;
    pub fn utf8proc_version() -> *const ::core::ffi::c_char;
}

/// Terminal width of a codepoint; utf8proc only looks it up in static tables.
pub(crate) fn codepoint_width(codepoint: crate::shared::utf8::wchar_t) -> i32 {
    unsafe { crate::compat::utf8proc::utf8proc_wcwidth(codepoint) }
}
