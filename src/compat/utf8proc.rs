use crate::ffi::utf8proc::{utf8proc_category, utf8proc_charwidth};
use crate::ffi::utf8proc::{utf8proc_category_t, utf8proc_int32_t};
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
