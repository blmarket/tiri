//! Authoritative limits declarations from the translated Linux C ABI.

pub const SIZE_MAX: ::core::ffi::c_ulong = 18446744073709551615 as ::core::ffi::c_ulong;

pub const SHRT_MAX: ::core::ffi::c_int = __SHRT_MAX__;

pub const INT_MIN: ::core::ffi::c_int = -__INT_MAX__ - 1 as ::core::ffi::c_int;

pub const __SHRT_MAX__: ::core::ffi::c_int = 32767 as ::core::ffi::c_int;

pub const __INT_MAX__: ::core::ffi::c_int = 2147483647 as ::core::ffi::c_int;

pub const UINT_MAX: ::core::ffi::c_uint = (__INT_MAX__ as ::core::ffi::c_uint)
    .wrapping_mul(2 as ::core::ffi::c_uint)
    .wrapping_add(1 as ::core::ffi::c_uint);

pub const INT_MAX: ::core::ffi::c_int = __INT_MAX__;

pub const USHRT_MAX: ::core::ffi::c_int =
    __SHRT_MAX__ * 2 as ::core::ffi::c_int + 1 as ::core::ffi::c_int;

pub const __LONG_LONG_MAX__: ::core::ffi::c_longlong =
    9223372036854775807 as ::core::ffi::c_longlong;

pub const UCHAR_MAX: ::core::ffi::c_int =
    __SCHAR_MAX__ * 2 as ::core::ffi::c_int + 1 as ::core::ffi::c_int;

pub const __SCHAR_MAX__: ::core::ffi::c_int = 127 as ::core::ffi::c_int;
