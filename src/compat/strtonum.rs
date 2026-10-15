use crate::src::ffi::libc::__errno_location;
use crate::src::shared::errno::{EINVAL, ERANGE};
use crate::src::shared::limits::__LONG_LONG_MAX__;
use libc::strtoll;
use std::ffi::CStr;

pub const LLONG_MAX: ::core::ffi::c_longlong = __LONG_LONG_MAX__;
pub const LLONG_MIN: ::core::ffi::c_longlong = -__LONG_LONG_MAX__ - 1 as ::core::ffi::c_longlong;
pub const INVALID: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const TOOSMALL: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const TOOLARGE: ::core::ffi::c_int = 3 as ::core::ffi::c_int;
pub unsafe fn strtonum(
    numstr: &CStr,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
) -> Result<i64, &'static CStr> {
    let mut ll: ::core::ffi::c_longlong = 0 as ::core::ffi::c_longlong;
    let mut ep: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut error: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let saved_errno = *__errno_location();
    *__errno_location() = 0 as ::core::ffi::c_int;
    if minval > maxval {
        error = INVALID;
    } else {
        ll = strtoll(numstr.as_ptr(), &raw mut ep, 10 as ::core::ffi::c_int);
        if std::ptr::eq(numstr.as_ptr(), ep) || *ep as ::core::ffi::c_int != '\0' as i32 {
            error = INVALID;
        } else if ll == LLONG_MIN && *__errno_location() == ERANGE || ll < minval {
            error = TOOSMALL;
        } else if ll == LLONG_MAX && *__errno_location() == ERANGE || ll > maxval {
            error = TOOLARGE;
        }
    }
    *__errno_location() = match error {
        INVALID => EINVAL,
        TOOSMALL | TOOLARGE => ERANGE,
        _ => saved_errno,
    };
    match error {
        INVALID => Err(c"invalid"),
        TOOSMALL => Err(c"too small"),
        TOOLARGE => Err(c"too large"),
        _ => Ok(ll),
    }
}
