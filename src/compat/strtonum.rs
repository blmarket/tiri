use crate::text::numbers::integer_prefix;
use std::ffi::CStr;

/// OpenBSD strtonum(3): the whole string must be one decimal integer within
/// `minval..=maxval`.
pub fn strtonum(numstr: &CStr, minval: i64, maxval: i64) -> Result<i64, &'static CStr> {
    if minval > maxval {
        return Err(c"invalid");
    }
    let bytes = numstr.to_bytes();
    let prefix = integer_prefix(bytes, 10);
    if prefix.consumed == 0 || prefix.consumed != bytes.len() {
        return Err(c"invalid");
    }
    let value = match (prefix.negative, prefix.magnitude) {
        (true, Some(magnitude)) if magnitude <= i64::MIN.unsigned_abs() => {
            0i64.wrapping_sub_unsigned(magnitude)
        }
        (true, _) => return Err(c"too small"),
        (false, Some(magnitude)) if magnitude <= i64::MAX as u64 => magnitude as i64,
        (false, _) => return Err(c"too large"),
    };
    if value < minval {
        Err(c"too small")
    } else if value > maxval {
        Err(c"too large")
    } else {
        Ok(value)
    }
}
