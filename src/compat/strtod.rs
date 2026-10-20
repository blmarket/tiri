use crate::format::bytes::cstr_suffix;
use crate::text::numbers::integer_prefix;
use std::ffi::CStr;

/// glibc strtod(3) in the C locale: convert the longest prefix of `nptr` and
/// return the value with the rest, as `*endptr` would point to it. The prefix
/// is optional white space, an optional sign, then a decimal or hexadecimal
/// number, `inf`, `infinity` or `nan` with an optional `(n-char-sequence)`.
/// Without a conversion the value is zero and the rest is all of `nptr`.
///
/// Results are correctly rounded to nearest, overflow is infinity and
/// underflow is zero or subnormal; strtod's `ERANGE` is not reported.
pub fn strtod(nptr: &CStr) -> (f64, &CStr) {
    let s = nptr.to_bytes();
    let mut i = s
        .iter()
        .take_while(|&&byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count();
    let negative = s.get(i) == Some(&b'-');
    if matches!(s.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let rest = &s[i..];
    let starts_with =
        |word: &[u8]| rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word);
    let (value, len) = if starts_with(b"infinity") {
        (f64::INFINITY, 8)
    } else if starts_with(b"inf") {
        (f64::INFINITY, 3)
    } else if starts_with(b"nan") {
        nan(&rest[3..])
    } else if rest.len() >= 2 && rest[0] == b'0' && rest[1].eq_ignore_ascii_case(&b'x') {
        hexadecimal(&rest[2..])
    } else {
        match decimal(rest) {
            Some(parsed) => parsed,
            None => return (0.0, nptr),
        }
    };
    let value = if negative { -value } else { value };
    (value, cstr_suffix(nptr, i + len))
}

fn digits(s: &[u8], radix: u32) -> usize {
    s.iter()
        .take_while(|&&byte| (byte as char).is_digit(radix))
        .count()
}

/// After `nan`, an `(n-char-sequence)` that reads whole as a base-0
/// strtoull(3) number puts its low 51 bits in the quiet NaN's payload. A
/// sequence without the closing parenthesis is not consumed.
fn nan(after: &[u8]) -> (f64, usize) {
    let quiet = f64::NAN.to_bits();
    if after.first() != Some(&b'(') {
        return (f64::from_bits(quiet), 3);
    }
    let chars = &after[1..];
    let count = chars
        .iter()
        .take_while(|&&byte| byte.is_ascii_alphanumeric() || byte == b'_')
        .count();
    if chars.get(count) != Some(&b')') {
        return (f64::from_bits(quiet), 3);
    }
    let chars = &chars[..count];
    let radix = if chars.len() > 2
        && chars[0] == b'0'
        && chars[1].eq_ignore_ascii_case(&b'x')
        && chars[2].is_ascii_hexdigit()
    {
        16
    } else if chars.first() == Some(&b'0') {
        8
    } else {
        10
    };
    let prefix = integer_prefix(chars, radix);
    let mut bits = quiet;
    if prefix.consumed == chars.len() {
        bits |= prefix.magnitude.unwrap_or(u64::MAX) & ((1 << 51) - 1);
    }
    (f64::from_bits(bits), 3 + 1 + count + 1)
}

/// The hexadecimal number after `0x`: the value and the length consumed
/// counting the `0x`. Without digits only the `0` converts.
fn hexadecimal(hex: &[u8]) -> (f64, usize) {
    let whole = digits(hex, 16);
    let has_point = hex.get(whole) == Some(&b'.');
    let fraction = if has_point {
        digits(&hex[whole + 1..], 16)
    } else {
        0
    };
    if whole + fraction == 0 {
        return (0.0, 1);
    }
    // The first 16 significant digits fill `mantissa`; later ones only set
    // `sticky`, so the value is `mantissa * 2^exponent` plus a nonzero tail.
    let mut mantissa = 0u64;
    let mut significant = 0;
    let mut sticky = false;
    let mut exponent = 0i64;
    let fraction_digits = if has_point {
        &hex[whole + 1..whole + 1 + fraction]
    } else {
        &[]
    };
    for (index, &byte) in hex[..whole].iter().chain(fraction_digits).enumerate() {
        let digit = (byte as char).to_digit(16).expect("hex digit") as u64;
        let in_fraction = index >= whole;
        if significant == 0 && digit == 0 {
            if in_fraction {
                exponent -= 4;
            }
        } else if significant < 16 {
            mantissa = mantissa << 4 | digit;
            significant += 1;
            if in_fraction {
                exponent -= 4;
            }
        } else {
            sticky |= digit != 0;
            if !in_fraction {
                exponent += 4;
            }
        }
    }
    let mut len = whole + usize::from(has_point) + fraction;
    if matches!(hex.get(len), Some(b'p' | b'P')) {
        let mut j = len + 1;
        let exponent_negative = hex.get(j) == Some(&b'-');
        if matches!(hex.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        let exponent_digits = digits(&hex[j..], 10);
        if exponent_digits > 0 {
            // Far past any double's range, and far from overflowing `i64`.
            let binary = hex[j..j + exponent_digits]
                .iter()
                .fold(0i64, |value, &byte| {
                    (value * 10 + i64::from(byte - b'0')).min(1 << 32)
                });
            exponent += if exponent_negative { -binary } else { binary };
            len = j + exponent_digits;
        }
    }
    (round(mantissa, sticky, exponent), 2 + len)
}

/// Round `mantissa * 2^exponent`, with `sticky` for a nonzero tail below the
/// mantissa, to the nearest double, ties to even.
fn round(mantissa: u64, sticky: bool, exponent: i64) -> f64 {
    if mantissa == 0 {
        return 0.0;
    }
    let zeros = mantissa.leading_zeros();
    let mantissa = mantissa << zeros;
    // The value lies in [2^top, 2^(top + 1)).
    let top = exponent - i64::from(zeros) + 63;
    if top > 1023 {
        return f64::INFINITY;
    }
    // Bits kept: 53 for a normal double, fewer as a subnormal loses range.
    let kept = 53 - (-1022 - top).max(0);
    if kept < 0 {
        // Below half the smallest subnormal.
        return 0.0;
    }
    let shift = (64 - kept) as u32;
    let wide = u128::from(mantissa);
    let mut kept_bits = (wide >> shift) as u64;
    let below = wide & ((1 << shift) - 1);
    let half = 1 << (shift - 1);
    if below > half || (below == half && (sticky || kept_bits & 1 == 1)) {
        kept_bits += 1;
    }
    if kept < 53 {
        // A subnormal's bits are its mantissa, and one carried out of it
        // becomes the smallest normal's exponent.
        return f64::from_bits(kept_bits);
    }
    let mut top = top;
    if kept_bits == 1 << 53 {
        kept_bits >>= 1;
        top += 1;
        if top > 1023 {
            return f64::INFINITY;
        }
    }
    f64::from_bits(((top + 1023) as u64) << 52 | (kept_bits & ((1 << 52) - 1)))
}

/// A decimal number: digits with an optional point, then an exponent only
/// when it has digits. `None` without a digit.
fn decimal(rest: &[u8]) -> Option<(f64, usize)> {
    let whole = digits(rest, 10);
    let fraction = if rest.get(whole) == Some(&b'.') {
        digits(&rest[whole + 1..], 10)
    } else {
        0
    };
    if whole + fraction == 0 {
        return None;
    }
    let mut len = whole;
    if rest.get(whole) == Some(&b'.') {
        len += 1 + fraction;
    }
    if matches!(rest.get(len), Some(b'e' | b'E')) {
        let mut j = len + 1;
        if matches!(rest.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        let exponent_digits = digits(&rest[j..], 10);
        if exponent_digits > 0 {
            len = j + exponent_digits;
        }
    }
    // Rust's parser rounds correctly for any length, as glibc does.
    let value = std::str::from_utf8(&rest[..len])
        .expect("decimal digits are ASCII")
        .parse::<f64>()
        .expect("a decimal prefix parses");
    Some((value, len))
}

#[cfg(test)]
mod tests {
    use super::strtod;
    use std::ffi::CString;

    /// Check `input` converts to the bits and leaves the rest libc's does.
    fn assert_matches_libc(input: &str) {
        let owned = CString::new(input).unwrap();
        let mut end = std::ptr::null_mut();
        let expected = unsafe { libc::strtod(owned.as_ptr(), &mut end) };
        let consumed = unsafe { end.offset_from(owned.as_ptr()) } as usize;
        let (value, rest) = strtod(&owned);
        assert_eq!(rest.to_bytes(), &input.as_bytes()[consumed..], "{input:?}");
        assert_eq!(value.to_bits(), expected.to_bits(), "{input:?}: {value}");
    }

    #[test]
    fn decimal_and_special_values_match_libc() {
        for input in [
            "",
            " ",
            "-",
            " 2",
            "\t-2.5",
            "1.5x",
            "1e3",
            "1E-2",
            "007",
            "12345678901234567890",
            "3.14159265358979323846",
            " \t\n+1",
            "-0",
            "+.5",
            "5.",
            ".",
            "-.",
            "1e",
            "1e+",
            "1e-2x",
            "1E400",
            "-1e400",
            "1e-400",
            "4.9406564584124654e-324",
            "2.4703282292062328e-324",
            "0.1000000000000000055511151231257827021181583404541015625",
            "123456789012345678901234567890e-30",
            "inf",
            "-INFINITY",
            "infinit",
            "infx",
            "nan",
            "-NaN",
            "nan(",
            "nan()",
            "NaN(abc)",
            "nan(abc_1)",
            "nan(abc-1)",
            "nan(123)",
            "-nan(0x7ff)",
            "nan(0xffffffffffffffffff)",
            "nan(017)",
            "nan(08)",
            "nan(0x)",
            "nan(0xg)",
            "x",
            "+",
            "1,5",
            "2 ",
        ] {
            assert_matches_libc(input);
        }
    }

    #[test]
    fn hexadecimal_values_match_libc() {
        for input in [
            "0x",
            "0x10",
            "0x1.8p1",
            "-0x",
            "0x.",
            "0xg",
            "0x.p1",
            "0x1.",
            "0x.8",
            "0X1P-2",
            "0x1p",
            "0x1p+",
            "0x1p-2x",
            "-0x0p5",
            "0x1p1024",
            "0x1.fffffffffffffp1023",
            "0x1.fffffffffffff8p1023",
            "0x1p-1074",
            "0x1p-1075",
            "0x1.0000000000001p-1075",
            "0x1.8p-1074",
            "0x1.fffffffffffffp-1023",
            "0x1.ffffffffffffffp-1023",
            "0x1p99999999999999999999",
            "0x1p-99999999999999999999",
        ] {
            assert_matches_libc(input);
        }
    }

    /// A hexadecimal mantissa wider than a double rounds once, as libc rounds
    /// it, not once per digit; nor does a long one overflow or underflow
    /// before its binary exponent scales it back into range.
    #[test]
    fn wide_hexadecimal_values_round_like_libc() {
        let zeros = "0".repeat(256);
        let places = "0".repeat(300);
        for input in [
            "0x200000000000011".to_owned(),
            "0x20000000000001000000000000000001".to_owned(),
            "0x20000000000001000000000000000000".to_owned(),
            "0x20000000000003000000000000000000".to_owned(),
            "0x1.000000000000081".to_owned(),
            "-0x6899e5422e751aa8".to_owned(),
            "0x.66f78f868fd4e22f69".to_owned(),
            format!("0x1{zeros}p-1024"),
            format!("0x0.{places}1p1300"),
            format!("0x0.{places}8p-50"),
        ] {
            assert_matches_libc(&input);
        }
    }
}
