//! Parse codepoint-width overrides without raw pointers or global decoder flags.
#![forbid(unsafe_code)]

use crate::ffi::numbers::{decimal_in_range, hexadecimal_prefix};
use crate::shared::utf8::wchar_t;
use crate::text::utf8_decode::{decode_utf8, DecodeResult};
use std::ffi::CStr;
use std::ops::RangeInclusive;

pub(super) struct WidthOverride {
    pub codepoints: RangeInclusive<wchar_t>,
    pub width: u32,
}

fn codepoint_prefix(input: &CStr) -> Option<(wchar_t, &CStr)> {
    let bytes = input.to_bytes_with_nul().strip_prefix(b"U+")?;
    let (codepoint, rest) = hexadecimal_prefix(CStr::from_bytes_with_nul(bytes).ok()?)?;
    if codepoint == 0 || codepoint > wchar_t::MAX as u64 {
        return None;
    }
    Some((codepoint as wchar_t, rest))
}

pub(super) fn parse_width_override(input: &CStr) -> Option<WidthOverride> {
    let mut copy = input.to_bytes_with_nul().to_vec();
    let separator = copy.iter().position(|&byte| byte == b'=')?;
    copy[separator] = 0;
    let (codepoint, width) = copy.split_at(separator + 1);
    let width = decimal_in_range(CStr::from_bytes_with_nul(width).ok()?, 0, 2)? as u32;
    let codepoint = CStr::from_bytes_with_nul(codepoint).ok()?;
    let codepoints = if codepoint.to_bytes().starts_with(b"U+") {
        let (start, rest) = codepoint_prefix(codepoint)?;
        let end = if rest.is_empty() {
            start
        } else {
            let rest = rest.to_bytes_with_nul().strip_prefix(b"-")?;
            let (end, rest) = codepoint_prefix(CStr::from_bytes_with_nul(rest).ok()?)?;
            if !rest.is_empty() || end < start {
                return None;
            }
            end
        };
        start..=end
    } else {
        let DecodeResult::Complete {
            codepoint: value,
            len,
        } = decode_utf8(codepoint.to_bytes())
        else {
            return None;
        };
        if len != codepoint.to_bytes().len() {
            return None;
        }
        value as wchar_t..=value as wchar_t
    };
    Some(WidthOverride { codepoints, width })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_numeric_prefixes_signs_and_width_syntax() {
        for (input, start, end, width) in [
            (c"U+E010=2", 0xe010, 0xe010, 2),
            (c"U+ +0xe010-U+E012=\t+01", 0xe010, 0xe012, 1),
            (c"U+-FFFFFFFFFFFFFFBF=-0", 0x41, 0x41, 0),
            (c"U+7FFFFFFF=0", 0x7fffffff, 0x7fffffff, 0),
            (c"漢=2", 0x6f22, 0x6f22, 2),
            (c"😀=0", 0x1f600, 0x1f600, 0),
            (c"\t=1", 9, 9, 1),
        ] {
            let parsed = parse_width_override(input).unwrap();
            assert_eq!(parsed.codepoints, start..=end, "{input:?}");
            assert_eq!(parsed.width, width, "{input:?}");
        }
    }

    #[test]
    fn rejects_partial_numbers_invalid_ranges_and_multiple_cells() {
        for input in [
            c"U+=1",
            c"U+0=1",
            c"U+80000000=1",
            c"U+10000000000000000=1",
            c"U+E010 =1",
            c"U+E010-U+=1",
            c"U+E010-U+E00F=1",
            c"U+E010-U+E011x=1",
            c"U+E010=1 ",
            c"U+E010=0x1",
            c"U+E010=-1",
            c"U+E010=3",
            c"=1",
            c"ab=1",
            c"é=1",
            c"éz=1",
            c"\xff=1",
            c"\xe2\x82=1",
        ] {
            assert!(parse_width_override(input).is_none(), "{input:?}");
        }
    }
}
