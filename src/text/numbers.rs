//! Borrowed string interfaces to the C numeric conversion rules.

use crate::compat::strtod::strtod;
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

/// strtoul's result: `ULONG_MAX` on overflow, the magnitude negated
/// modulo 2^64 after a minus sign.
pub(crate) fn unsigned_long(prefix: &IntegerPrefix) -> u64 {
    match prefix.magnitude {
        None => u64::MAX,
        Some(magnitude) if prefix.negative => magnitude.wrapping_neg(),
        Some(magnitude) => magnitude,
    }
}

/// glibc `%u`: optional whitespace and sign, at least one digit, read
/// as `unsigned long` and stored truncated to 32 bits, so a number past
/// `u64` saturates to `u32::MAX` and anything shorter wraps modulo
/// 2^32. Returns the value and the bytes consumed.
pub(crate) fn scan_u32(field: &[u8]) -> Option<(u32, usize)> {
    let prefix = integer_prefix(field, 10);
    (prefix.consumed != 0).then(|| (unsigned_long(&prefix) as u32, prefix.consumed))
}

/// `sscanf(text, "<literal>%u<separator>%u")` returning 2; bytes after
/// the second number are ignored.
pub(crate) fn scan_u32_pair(text: &[u8], literal: &[u8], separator: &[u8]) -> Option<(u32, u32)> {
    let mut text = Scan::new(text);
    text.literal(literal)?;
    let first = text.conversion(scan_u32)?;
    text.literal(separator)?;
    let second = text.conversion(scan_u32)?;
    Some((first, second))
}

/// `strtol(text, &end, 10)` cast to `int`, with the bytes consumed (0
/// when there are no digits, where `end` is `text`). The `long` result
/// saturates first, so `LONG_MAX` becomes -1 and `LONG_MIN` becomes 0.
pub(crate) fn strtol_prefix(text: &[u8]) -> (i32, usize) {
    let prefix = integer_prefix(text, 10);
    let long = match prefix.magnitude {
        Some(magnitude) if prefix.negative => {
            0i64.checked_sub_unsigned(magnitude).unwrap_or(i64::MIN)
        }
        Some(magnitude) => i64::try_from(magnitude).unwrap_or(i64::MAX),
        None if prefix.negative => i64::MIN,
        None => i64::MAX,
    };
    (long as i32, prefix.consumed)
}

/// glibc `%d`: optional whitespace and sign, at least one digit, read as
/// `long` by strtol and stored truncated to `int`. Returns the value and
/// the bytes consumed.
pub(crate) fn scan_i32(field: &[u8]) -> Option<(i32, usize)> {
    let (value, consumed) = strtol_prefix(field);
    (consumed != 0).then_some((value, consumed))
}

fn c_space_count(input: &[u8]) -> usize {
    input
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count()
}

/// glibc `%x`: optional whitespace, then an optional sign, `0x` prefix and
/// hexadecimal digits. A `0x` prefix needs a digit after it. The value is
/// strtoul's, stored truncated to `unsigned int`. Returns the value and the
/// bytes consumed.
pub(crate) fn scan_hex_u32(field: &[u8]) -> Option<(u32, usize)> {
    scan_hex(field, None)
}

/// glibc `%<width>x`: as `%x`, but the sign, prefix and digits together
/// take at most `width` bytes; the whitespace before them does not count.
pub(crate) fn scan_hex_u32_width(field: &[u8], width: usize) -> Option<(u32, usize)> {
    scan_hex(field, Some(width))
}

fn scan_hex(field: &[u8], mut width: Option<usize>) -> Option<(u32, usize)> {
    let mut take = |index: &mut usize| match &mut width {
        Some(0) => false,
        Some(width) => {
            *width -= 1;
            *index += 1;
            true
        }
        None => {
            *index += 1;
            true
        }
    };
    let mut index = c_space_count(field);
    let negative = field.get(index) == Some(&b'-');
    if matches!(field.get(index), Some(b'+' | b'-')) {
        take(&mut index);
    }
    let mut found = false;
    if field.get(index) == Some(&b'0') && take(&mut index) {
        found = true;
        if matches!(field.get(index), Some(b'x' | b'X')) && take(&mut index) {
            found = false;
        }
    }
    let mut magnitude = Some(0u64);
    while let Some(digit) = field
        .get(index)
        .and_then(|&byte| (byte as char).to_digit(16))
    {
        if !take(&mut index) {
            break;
        }
        magnitude = magnitude
            .and_then(|value| value.checked_mul(16))
            .and_then(|value| value.checked_add(digit as u64));
        found = true;
    }
    let prefix = IntegerPrefix {
        negative,
        magnitude,
        consumed: index,
    };
    found.then(|| (unsigned_long(&prefix) as u32, index))
}

/// The input a sscanf(3) format has yet to match. Each directive returns
/// `None` where sscanf would stop; sscanf's count is the conversions that
/// succeeded before that.
pub(crate) struct Scan<'a>(&'a [u8]);

impl<'a> Scan<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self(input)
    }

    /// Ordinary format characters, matched byte for byte.
    pub(crate) fn literal(&mut self, literal: &[u8]) -> Option<()> {
        self.0 = self.0.strip_prefix(literal)?;
        Some(())
    }

    /// `%%`: optional whitespace, then `%`.
    pub(crate) fn percent(&mut self) -> Option<()> {
        self.0 = self.0[c_space_count(self.0)..].strip_prefix(b"%")?;
        Some(())
    }

    pub(crate) fn rest(&self) -> &'a [u8] {
        self.0
    }

    /// A conversion such as `scan_u32`, given the rest of the input.
    pub(crate) fn conversion<T>(
        &mut self,
        scan: impl FnOnce(&[u8]) -> Option<(T, usize)>,
    ) -> Option<T> {
        let (value, consumed) = scan(self.0)?;
        self.0 = &self.0[consumed..];
        Some(value)
    }
}

/// glibc `%lf` in the C locale: optional whitespace, then the bytes that
/// can still continue a number, `nan` with an optional `(n-char-sequence)`,
/// `inf` or `infinity`, read without backtracking and converted by strtod.
/// An exponent needs a digit. Returns the value and the bytes
/// consumed.
pub(crate) fn scan_f64(field: &[u8]) -> Option<(f64, usize)> {
    let start = c_space_count(field);
    let mut index = start;
    let mut signed = false;
    if matches!(field.get(index), Some(b'+' | b'-')) {
        index += 1;
        signed = true;
    }
    let lower = |index: usize| field.get(index).map(u8::to_ascii_lowercase);
    let word = |index: usize, word: &[u8]| {
        field
            .get(index..index + word.len())
            .is_some_and(|bytes| bytes.eq_ignore_ascii_case(word))
    };
    match lower(index) {
        Some(b'n') => {
            if !word(index, b"nan") {
                return None;
            }
            index += 3;
            if field.get(index) == Some(&b'(') {
                let chars = field[index + 1..]
                    .iter()
                    .take_while(|&&byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    .count();
                if field.get(index + 1 + chars) != Some(&b')') {
                    return None;
                }
                index += chars + 2;
            }
        }
        Some(b'i') => {
            if !word(index, b"inf") {
                return None;
            }
            index += 3;
            if lower(index) == Some(b'i') {
                if !word(index, b"inity") {
                    return None;
                }
                index += 5;
            }
        }
        _ => {
            let mut hex = false;
            let mut exponent = b'e';
            let (mut digit, mut dot, mut exp) = (false, false, false);
            if field.get(index) == Some(&b'0') {
                index += 1;
                if lower(index) == Some(b'x') {
                    index += 1;
                    hex = true;
                    exponent = b'p';
                } else {
                    digit = true;
                }
            }
            while let Some(&byte) = field.get(index) {
                if byte.is_ascii_digit() || (hex && !exp && byte.is_ascii_hexdigit()) {
                    digit = true;
                } else if digit && !exp && byte.to_ascii_lowercase() == exponent {
                    exp = true;
                    dot = true;
                } else if !exp && !dot && byte == b'.' {
                    dot = true;
                } else if !(exp
                    && field[index - 1].to_ascii_lowercase() == exponent
                    && matches!(byte, b'+' | b'-'))
                {
                    break;
                }
                index += 1;
            }
            let prefix_len = signed as usize + if hex { 2 } else { 0 };
            if index - start == prefix_len || (exp && !field[index - 1].is_ascii_digit()) {
                return None;
            }
        }
    }
    let token = std::ffi::CString::new(&field[start..index]).ok()?;
    let (value, rest) = strtod(&token);
    (rest.to_bytes().len() != token.as_bytes().len()).then_some((value, index))
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
