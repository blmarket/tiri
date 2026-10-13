//! Foreign declarations supplied by utf8proc.

use crate::src::shared::abi::{__int32_t, uint8_t};
pub type int32_t = __int32_t;

pub type ptrdiff_t = isize;

pub type utf8proc_uint8_t = uint8_t;

pub type utf8proc_int32_t = int32_t;

pub type utf8proc_ssize_t = ptrdiff_t;

pub type utf8proc_bool = bool;

pub type utf8proc_category_t = ::core::ffi::c_uint;

extern "C" {
    pub fn utf8proc_category(codepoint: utf8proc_int32_t) -> utf8proc_category_t;
    pub fn utf8proc_charwidth(codepoint: utf8proc_int32_t) -> ::core::ffi::c_int;
    pub fn utf8proc_codepoint_valid(codepoint: utf8proc_int32_t) -> utf8proc_bool;
    pub fn utf8proc_encode_char(
        codepoint: utf8proc_int32_t,
        dst: *mut utf8proc_uint8_t,
    ) -> utf8proc_ssize_t;
    pub fn utf8proc_version() -> *const ::core::ffi::c_char;
}

/// Encode into the fixed storage used by a terminal cell. utf8proc writes at
/// most four bytes, and leaves the buffer untouched for invalid codepoints.
pub(crate) fn encode_cell(
    codepoint: crate::src::shared::utf8::wchar_t,
    output: &mut [u8; 32],
) -> Result<usize, i32> {
    let size = unsafe {
        crate::src::compat::utf8proc::utf8proc_wctomb(output.as_mut_ptr().cast(), codepoint)
    };
    if size < 0 {
        Err(unsafe { *super::libc::__errno_location() })
    } else {
        Ok(size as usize)
    }
}

pub(crate) fn reset_wctomb() {
    unsafe {
        super::libc::wctomb(std::ptr::null_mut(), 0);
    }
}
