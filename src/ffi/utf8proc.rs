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

const UTF8PROC_CATEGORY_CO: utf8proc_category_t = 29;

/// Terminal width of a codepoint, with private-use codepoints one cell wide
/// as tmux's `utf8proc_wcwidth` has them.
pub(crate) fn codepoint_width(codepoint: crate::shared::utf8::wchar_t) -> i32 {
    // SAFETY: utf8proc only looks the codepoint up in static tables.
    unsafe {
        if utf8proc_category(codepoint as utf8proc_int32_t) == UTF8PROC_CATEGORY_CO {
            return 1;
        }
        utf8proc_charwidth(codepoint as utf8proc_int32_t)
    }
}
