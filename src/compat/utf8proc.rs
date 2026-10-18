use crate::ffi::utf8proc::{
    utf8proc_category, utf8proc_charwidth, utf8proc_codepoint_valid, utf8proc_encode_char,
};
use crate::ffi::utf8proc::{
    utf8proc_category_t, utf8proc_int32_t, utf8proc_ssize_t, utf8proc_uint8_t,
};
use crate::shared::abi::*;
use crate::shared::utf8::wchar_t;

pub const UTF8PROC_CATEGORY_CO: utf8proc_category_t = 29;
pub unsafe fn utf8proc_wcwidth(mut wc: wchar_t) -> ::core::ffi::c_int {
    let mut cat: ::core::ffi::c_int = 0;
    cat = utf8proc_category(wc as utf8proc_int32_t) as ::core::ffi::c_int;
    if cat == UTF8PROC_CATEGORY_CO as ::core::ffi::c_int {
        return 1 as ::core::ffi::c_int;
    }
    utf8proc_charwidth(wc as utf8proc_int32_t)
}
pub unsafe fn utf8proc_wctomb(
    mut s: *mut ::core::ffi::c_char,
    mut wc: wchar_t,
) -> ::core::ffi::c_int {
    if s.is_null() {
        return 0 as ::core::ffi::c_int;
    }
    if !utf8proc_codepoint_valid(wc as utf8proc_int32_t) {
        return -(1 as ::core::ffi::c_int);
    }
    utf8proc_encode_char(wc as utf8proc_int32_t, s as *mut utf8proc_uint8_t) as ::core::ffi::c_int
}
