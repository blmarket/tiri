//! Foreign declarations supplied by resolv.

use crate::shared::abi::size_t;

extern "C" {
    #[cfg_attr(target_os = "linux", link_name = "__b64_ntop")]
    #[cfg_attr(target_vendor = "apple", link_name = "res_9_b64_ntop")]
    pub fn b64_ntop(
        _: *const ::core::ffi::c_uchar,
        _: size_t,
        _: *mut ::core::ffi::c_char,
        _: size_t,
    ) -> ::core::ffi::c_int;
    #[cfg_attr(target_os = "linux", link_name = "__b64_pton")]
    #[cfg_attr(target_vendor = "apple", link_name = "res_9_b64_pton")]
    pub fn b64_pton(
        _: *const ::core::ffi::c_char,
        _: *mut ::core::ffi::c_uchar,
        _: size_t,
    ) -> ::core::ffi::c_int;
}
