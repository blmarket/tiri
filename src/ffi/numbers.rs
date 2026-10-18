//! Borrowed string interfaces to the C numeric conversion rules.

use crate::compat::strtonum::strtonum;
use crate::format::bytes::cstr_suffix;
use std::ffi::CStr;

pub(crate) fn decimal_in_range(input: &CStr, min: i64, max: i64) -> Option<i64> {
    strtonum(input, min, max).ok()
}

/// An integer prefix read by the strtol(3) family's rules.
pub(crate) struct IntegerPrefix {
    pub negative: bool,
    /// The digits' magnitude, or `None` when it overflows `u64`.
    pub magnitude: Option<u64>,
    /// Bytes consumed, or 0 when no digits were found.
    pub consumed: usize,
}

/// Read optional C-locale whitespace, an optional sign, a `0x` prefix for
/// base 16, and digits, the way strtoll and strtoull parse their input.
pub(crate) fn integer_prefix(input: &[u8], radix: u32) -> IntegerPrefix {
    let mut index = input
        .iter()
        .position(|&byte| !matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(input.len());
    let negative = input.get(index) == Some(&b'-');
    if matches!(input.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    if radix == 16
        && input.get(index) == Some(&b'0')
        && matches!(input.get(index + 1), Some(b'x' | b'X'))
        && input.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
    {
        index += 2;
    }
    let mut magnitude = Some(0u64);
    let start = index;
    while let Some(digit) = input
        .get(index)
        .and_then(|&byte| (byte as char).to_digit(radix))
    {
        magnitude = magnitude
            .and_then(|value| value.checked_mul(radix as u64))
            .and_then(|value| value.checked_add(digit as u64));
        index += 1;
    }
    IntegerPrefix {
        negative,
        magnitude,
        consumed: if index == start { 0 } else { index },
    }
}

/// Keep strtoull's optional whitespace, sign, and 0x prefix semantics, and
/// return the unconsumed suffix as a borrow of the original C string.
pub(crate) fn hexadecimal_prefix(input: &CStr) -> Option<(u64, &CStr)> {
    let prefix = integer_prefix(input.to_bytes(), 16);
    let magnitude = prefix.magnitude?;
    let value = if prefix.negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    };
    Some((value, cstr_suffix(input, prefix.consumed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &CStr) -> Option<(u64, &[u8])> {
        hexadecimal_prefix(input).map(|(value, rest)| (value, rest.to_bytes()))
    }

    #[test]
    fn hexadecimal_prefix_follows_strtoull() {
        assert_eq!(parse(c"1F600"), Some((0x1f600, &b""[..])));
        assert_eq!(parse(c" \t0x1fz"), Some((0x1f, &b"z"[..])));
        assert_eq!(parse(c"0xg"), Some((0, &b"xg"[..])));
        assert_eq!(parse(c"-1"), Some((u64::MAX, &b""[..])));
        assert_eq!(parse(c"zz"), Some((0, &b"zz"[..])));
        assert_eq!(parse(c"ffffffffffffffff"), Some((u64::MAX, &b""[..])));
        assert_eq!(parse(c"10000000000000000"), None);
    }

    #[test]
    fn strtonum_reports_strtoll_ranges() {
        assert_eq!(strtonum(c" +42", 0, 100), Ok(42));
        assert_eq!(strtonum(c"-9223372036854775808", i64::MIN, 0), Ok(i64::MIN));
        assert_eq!(strtonum(c"9223372036854775807", 0, i64::MAX), Ok(i64::MAX));
        assert_eq!(
            strtonum(c"-9223372036854775809", i64::MIN, 0),
            Err(c"too small")
        );
        assert_eq!(
            strtonum(c"9223372036854775808", 0, i64::MAX),
            Err(c"too large")
        );
        assert_eq!(strtonum(c"101", 0, 100), Err(c"too large"));
        assert_eq!(strtonum(c"-1", 0, 100), Err(c"too small"));
        assert_eq!(strtonum(c"", 0, 100), Err(c"invalid"));
        assert_eq!(strtonum(c"-", 0, 100), Err(c"invalid"));
        assert_eq!(strtonum(c"4 ", 0, 100), Err(c"invalid"));
        assert_eq!(strtonum(c"4", 5, 1), Err(c"invalid"));
    }
}
