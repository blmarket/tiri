//! Byte-oriented attribute text APIs. Case matching follows the active C locale.
use crate::src::shared::grid::*;
use std::ffi::{CStr, CString};

const NAMES: &[(&[u8], i32)] = &[
    (b"acs", GRID_ATTR_CHARSET),
    (b"bright", GRID_ATTR_BRIGHT),
    (b"dim", GRID_ATTR_DIM),
    (b"underscore", GRID_ATTR_UNDERSCORE),
    (b"blink", GRID_ATTR_BLINK),
    (b"reverse", GRID_ATTR_REVERSE),
    (b"hidden", GRID_ATTR_HIDDEN),
    (b"italics", GRID_ATTR_ITALICS),
    (b"strikethrough", GRID_ATTR_STRIKETHROUGH),
    (b"double-underscore", GRID_ATTR_UNDERSCORE_2),
    (b"curly-underscore", GRID_ATTR_UNDERSCORE_3),
    (b"dotted-underscore", GRID_ATTR_UNDERSCORE_4),
    (b"dashed-underscore", GRID_ATTR_UNDERSCORE_5),
    (b"overline", GRID_ATTR_OVERLINE),
    (b"noattr", GRID_ATTR_NOATTR),
];

/// Parse an entire byte slice. Embedded NUL and invalid attribute lists return None.
pub fn attributes_parse(input: &[u8]) -> Option<i32> {
    fn delimiter(b: &u8) -> bool {
        b" ,|".contains(b)
    }
    if input.is_empty()
        || input.contains(&0)
        || delimiter(input.first()?)
        || delimiter(input.last()?)
    {
        return None;
    }
    if equal(input, b"none") || equal(input, b"default") {
        return Some(0);
    }
    let mut attr = 0;
    for token in input.split(delimiter).filter(|token| !token.is_empty()) {
        if equal(token, b"bold") {
            attr |= GRID_ATTR_BRIGHT;
            continue;
        }
        let (_, bit) = NAMES
            .iter()
            .find(|(name, bit)| *bit != GRID_ATTR_NOATTR && equal(token, name))?;
        attr |= bit;
    }
    Some(attr)
}

fn equal(a: &[u8], b: &[u8]) -> bool {
    // Both slices have this many readable bytes; strncasecmp reads at most n.
    // Keeping libc here preserves locale-sensitive case matching.
    a.len() == b.len()
        && unsafe { libc::strncasecmp(a.as_ptr().cast(), b.as_ptr().cast(), a.len()) == 0 }
}

/// Parse a NUL-terminated byte string without retaining its pointer.
/// Invalid UTF-8 is accepted and compared according to the active C locale.
pub fn attributes_parse_cstr(input: &CStr) -> Option<i32> {
    attributes_parse(input.to_bytes())
}

fn attributes_format_len(attr: i32) -> usize {
    if attr == 0 {
        return b"none".len();
    }
    NAMES
        .iter()
        .filter(|(_, bit)| attr & bit != 0)
        .map(|(name, _)| name.len())
        .sum::<usize>()
        + NAMES
            .iter()
            .filter(|(_, bit)| attr & bit != 0)
            .count()
            .saturating_sub(1)
}

/// Format canonical attribute text into a caller-owned NUL-terminated buffer.
///
/// The returned length excludes the trailing NUL. If `output` is too small,
/// this function returns `None` without modifying it. The output order is the
/// same as [`attributes_format`].
pub fn attributes_format_into(attr: i32, output: &mut [u8]) -> Option<usize> {
    let required = attributes_format_len(attr);
    if output.len() <= required {
        return None;
    }

    let mut written = 0;
    if attr == 0 {
        output[..b"none".len()].copy_from_slice(b"none");
        written = b"none".len();
    } else {
        for &(name, bit) in NAMES {
            if attr & bit == 0 {
                continue;
            }
            if written != 0 {
                output[written] = b',';
                written += 1;
            }
            output[written..written + name.len()].copy_from_slice(name);
            written += name.len();
        }
    }
    output[written] = 0;
    Some(written)
}

/// Owned canonical text; later formatting calls cannot invalidate this result.
pub fn attributes_format(attr: i32) -> CString {
    let mut bytes = vec![0; attributes_format_len(attr) + 1];
    let written = attributes_format_into(attr, &mut bytes).expect("sized attribute buffer");
    bytes.truncate(written + 1);
    CString::from_vec_with_nul(bytes).expect("attribute names contain no NUL")
}
