//! Borrowed string interfaces to the existing C numeric conversions.

use crate::src::compat::strtonum::strtonum;
use crate::src::format::bytes::cstr_suffix;
use libc::strtoull;
use std::ffi::CStr;

pub(crate) fn decimal_in_range(input: &CStr, min: i64, max: i64) -> Option<i64> {
    unsafe { strtonum(input, min, max) }.ok()
}

/// Keep strtoull's optional whitespace, sign, and 0x prefix semantics, and
/// return the unconsumed suffix as a borrow of the original C string.
pub(crate) fn hexadecimal_prefix(input: &CStr) -> Option<(u64, &CStr)> {
    let mut end = std::ptr::null_mut();
    let value;
    let consumed;
    unsafe {
        errno::set_errno(errno::Errno(0));
        value = strtoull(input.as_ptr(), &mut end, 16);
        if value == u64::MAX && errno::errno().0 == libc::ERANGE {
            return None;
        }
        consumed = end.offset_from(input.as_ptr()) as usize;
    }
    let suffix = cstr_suffix(input, consumed);
    Some((value, suffix))
}
