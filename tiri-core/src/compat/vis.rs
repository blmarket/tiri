//! OpenBSD vis(3) over bytes. Each source byte is escaped as `vis` escapes
//! it; the string forms stop at the first NUL, as the C loops did.

use crate::shared::vis::{
    VIS_ALL, VIS_CSTYLE, VIS_DQ, VIS_GLOB, VIS_NL, VIS_NOSLASH, VIS_OCTAL, VIS_SAFE, VIS_SP,
    VIS_TAB,
};

/// The at most four bytes `vis` writes for one source byte.
pub struct VisBytes {
    len: u8,
    bytes: [u8; 4],
}

impl VisBytes {
    fn push(&mut self, byte: u8) {
        self.bytes[self.len as usize] = byte;
        self.len += 1;
    }
}

impl AsRef<[u8]> for VisBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// OpenBSD's `isvisible`: the byte is copied, with a backslash prefix for
/// `"` under `VIS_DQ` and for `\` without `VIS_NOSLASH`.
fn is_visible(c: u8, flags: i32) -> bool {
    let glob = matches!(c, b'*' | b'?' | b'[' | b'#');
    (c == b'\\' || flags & VIS_ALL == 0)
        && (c.is_ascii_graphic() && (!glob || flags & VIS_GLOB == 0)
            || flags & VIS_SP == 0 && c == b' '
            || flags & VIS_TAB == 0 && c == b'\t'
            || flags & VIS_NL == 0 && c == b'\n'
            || flags & VIS_SAFE != 0 && (matches!(c, 0x08 | 0x07 | b'\r') || c.is_ascii_graphic()))
}

fn needs_backslash(c: u8, flags: i32) -> bool {
    c == b'"' && flags & VIS_DQ != 0 || c == b'\\' && flags & VIS_NOSLASH == 0
}

/// Escape `c`, followed in the source by `next`, as OpenBSD `vis(3)` does.
///
/// The C function takes an `int`; callers passed both sign-extended `char`s
/// and zero-extended bytes, which reach the same output here because no byte
/// above 0x7f is visible and every later test masks to the low eight bits.
pub fn vis_byte(c: u8, flags: i32, next: u8) -> VisBytes {
    let mut out = VisBytes {
        len: 0,
        bytes: [0; 4],
    };
    if is_visible(c, flags) {
        if needs_backslash(c, flags) {
            out.push(b'\\');
        }
        out.push(c);
        return out;
    }
    if flags & VIS_CSTYLE != 0 {
        let letter = match c {
            b'\n' => Some(b'n'),
            b'\r' => Some(b'r'),
            0x08 => Some(b'b'),
            0x07 => Some(b'a'),
            0x0b => Some(b'v'),
            b'\t' => Some(b't'),
            0x0c => Some(b'f'),
            b' ' => Some(b's'),
            0 => Some(b'0'),
            _ => None,
        };
        if let Some(letter) = letter {
            out.push(b'\\');
            out.push(letter);
            // `\0` would swallow a following octal digit when decoded.
            if c == 0 && (b'0'..=b'7').contains(&next) {
                out.push(b'0');
                out.push(b'0');
            }
            return out;
        }
    }
    if c & 0o177 == b' '
        || flags & VIS_OCTAL != 0
        || flags & VIS_GLOB != 0 && matches!(c, b'*' | b'?' | b'[' | b'#')
    {
        out.push(b'\\');
        out.push(b'0' + (c >> 6 & 0o7));
        out.push(b'0' + (c >> 3 & 0o7));
        out.push(b'0' + (c & 0o7));
        return out;
    }
    if flags & VIS_NOSLASH == 0 {
        out.push(b'\\');
    }
    let mut c = c;
    if c & 0o200 != 0 {
        c &= 0o177;
        out.push(b'M');
    }
    if c.is_ascii_control() {
        out.push(b'^');
        out.push(if c == 0o177 { b'?' } else { c + b'@' });
    } else {
        out.push(b'-');
        out.push(c);
    }
    out
}

fn until_nul(src: &[u8]) -> &[u8] {
    src.iter()
        .position(|&byte| byte == 0)
        .map_or(src, |nul| &src[..nul])
}

/// `strvis(3)`: append the escaped form of `src`, without a trailing NUL.
pub fn strvis_append(out: &mut Vec<u8>, src: &[u8], flags: i32) {
    let src = until_nul(src);
    for (index, &c) in src.iter().enumerate() {
        let next = src.get(index + 1).copied().unwrap_or(0);
        out.extend_from_slice(vis_byte(c, flags, next).as_ref());
    }
}

/// `strnvis(3)` into a `siz`-byte buffer, returning the bytes before its
/// NUL. As in OpenBSD, `siz - 1` bytes are available, and the first byte
/// whose whole escape does not fit ends the output; the full length the C
/// function returned has no reader.
pub fn strnvis(src: &[u8], siz: usize, flags: i32) -> Vec<u8> {
    let src = until_nul(src);
    let end = siz.saturating_sub(1);
    let mut out = Vec::with_capacity(end);
    for (index, &c) in src.iter().enumerate() {
        let next = src.get(index + 1).copied().unwrap_or(0);
        let escaped = vis_byte(c, flags, next);
        if out.len() + escaped.as_ref().len() > end {
            break;
        }
        out.extend_from_slice(escaped.as_ref());
    }
    out
}

/// vis writes at most four encoded bytes plus its trailing NUL.
pub(crate) fn vis_into(output: &mut [u8], byte: u8, flags: i32, next: u8) -> usize {
    assert!(output.len() >= 5, "vis needs four bytes plus a terminator");
    let escaped = vis_byte(byte, flags, next);
    let escaped = escaped.as_ref();
    output[..escaped.len()].copy_from_slice(escaped);
    output[escaped.len()] = 0;
    escaped.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vis(c: u8, flags: i32, next: u8) -> Vec<u8> {
        vis_byte(c, flags, next).as_ref().to_vec()
    }

    #[test]
    fn cstyle_names_each_control_byte() {
        let cases: [(u8, &[u8]); 8] = [
            (b'\n', b"\\n"),
            (b'\r', b"\\r"),
            (0x08, b"\\b"),
            (0x07, b"\\a"),
            (0x0b, b"\\v"),
            (b'\t', b"\\t"),
            (0x0c, b"\\f"),
            (b' ', b"\\s"),
        ];
        let flags = VIS_CSTYLE | VIS_SP | VIS_TAB | VIS_NL;
        for (c, escaped) in cases {
            assert_eq!(vis(c, flags, 0), escaped, "{c:#x}");
            assert_eq!(vis(c, flags | VIS_OCTAL, 0), escaped, "{c:#x}");
        }
    }

    #[test]
    fn cstyle_nul_widens_before_an_octal_digit() {
        let flags = VIS_CSTYLE | VIS_OCTAL;
        assert_eq!(vis(0, flags, 0), b"\\0");
        assert_eq!(vis(0, flags, b'a'), b"\\0");
        assert_eq!(vis(0, flags, b'8'), b"\\0");
        assert_eq!(vis(0, flags, b'0'), b"\\000");
        assert_eq!(vis(0, flags, b'7'), b"\\000");
        assert_eq!(vis(0, VIS_OCTAL, b'7'), b"\\000");
        assert_eq!(vis(0, 0, 0), b"\\^@");
    }

    #[test]
    fn quotes_and_backslashes_follow_their_flags() {
        assert_eq!(vis(b'"', 0, 0), b"\"");
        assert_eq!(vis(b'"', VIS_DQ, 0), b"\\\"");
        assert_eq!(vis(b'\\', 0, 0), b"\\\\");
        assert_eq!(vis(b'\\', VIS_NOSLASH, 0), b"\\");
        assert_eq!(vis(b'\\', VIS_ALL, 0), b"\\\\");
        assert_eq!(vis(b'a', VIS_ALL, 0), b"\\-a");
        assert_eq!(vis(b'a', VIS_ALL | VIS_OCTAL, 0), b"\\141");
    }

    #[test]
    fn whitespace_is_kept_unless_its_flag_is_set() {
        assert_eq!(vis(b' ', 0, 0), b" ");
        assert_eq!(vis(b' ', VIS_SP, 0), b"\\040");
        assert_eq!(vis(b'\t', 0, 0), b"\t");
        assert_eq!(vis(b'\t', VIS_TAB, 0), b"\\^I");
        assert_eq!(vis(b'\t', VIS_TAB | VIS_OCTAL, 0), b"\\011");
        assert_eq!(vis(b'\n', 0, 0), b"\n");
        assert_eq!(vis(b'\n', VIS_NL, 0), b"\\^J");
        assert_eq!(vis(b'\n', VIS_NL | VIS_NOSLASH, 0), b"^J");
        assert_eq!(vis(b'\r', 0, 0), b"\\^M");
        assert_eq!(vis(b'\r', VIS_SAFE, 0), b"\r");
        assert_eq!(vis(0x07, VIS_SAFE, 0), b"\x07");
        assert_eq!(vis(0x08, VIS_SAFE, 0), b"\x08");
        assert_eq!(vis(0x01, VIS_SAFE, 0), b"\\^A");
    }

    #[test]
    fn meta_control_and_space_forms() {
        assert_eq!(vis(0x7f, 0, 0), b"\\^?");
        assert_eq!(vis(0x80, 0, 0), b"\\M^@");
        assert_eq!(vis(0xc1, 0, 0), b"\\M-A");
        assert_eq!(vis(0xc1, VIS_NOSLASH, 0), b"M-A");
        assert_eq!(vis(0xff, 0, 0), b"\\M^?");
        assert_eq!(vis(0xff, VIS_OCTAL, 0), b"\\377");
        // `(c & 0177) == ' '` always takes the octal form.
        assert_eq!(vis(0xa0, 0, 0), b"\\240");
        assert_eq!(vis(b' ', VIS_SP | VIS_NOSLASH, 0), b"\\040");
        assert_eq!(vis(0x80, VIS_CSTYLE, 0), b"\\M^@");
    }

    #[test]
    fn glob_characters_become_octal() {
        for (c, escaped) in [
            (b'*', b"\\052"),
            (b'?', b"\\077"),
            (b'[', b"\\133"),
            (b'#', b"\\043"),
        ] {
            assert_eq!(vis(c, 0, 0), [c]);
            assert_eq!(vis(c, VIS_GLOB, 0), escaped);
            assert_eq!(vis(c, VIS_GLOB | VIS_SAFE, 0), [c]);
        }
        assert_eq!(vis(b']', VIS_GLOB, 0), b"]");
    }

    #[test]
    fn strvis_appends_without_a_terminator() {
        let mut out = b"> ".to_vec();
        strvis_append(&mut out, b"a\tb\0c", VIS_OCTAL | VIS_CSTYLE | VIS_TAB);
        assert_eq!(out, b"> a\\tb");
        let mut out = Vec::new();
        strvis_append(&mut out, b"\x007\xe3\x81", VIS_OCTAL | VIS_CSTYLE);
        assert_eq!(out, b"");
        strvis_append(&mut out, b"\x1b7\xe3\x81", VIS_OCTAL | VIS_CSTYLE);
        assert_eq!(out, b"\\0337\\343\\201");
    }

    #[test]
    fn strnvis_stops_at_the_first_unit_that_does_not_fit() {
        let flags = VIS_OCTAL | VIS_CSTYLE;
        assert_eq!(strnvis(b"abc", 0, flags), b"");
        assert_eq!(strnvis(b"abc", 1, flags), b"");
        assert_eq!(strnvis(b"abc", 3, flags), b"ab");
        assert_eq!(strnvis(b"abc", 4, flags), b"abc");
        // A backslashed byte needs two cells.
        assert_eq!(strnvis(b"a\\b", 3, flags), b"a");
        assert_eq!(strnvis(b"a\\b", 4, flags), b"a\\\\");
        // A CSTYLE escape needs two, an octal one four.
        assert_eq!(strnvis(b"a\nb", 3, flags | VIS_NL), b"a");
        assert_eq!(strnvis(b"a\nb", 4, flags | VIS_NL), b"a\\n");
        assert_eq!(strnvis(b"a\xffb", 5, flags), b"a");
        assert_eq!(strnvis(b"a\xffb", 6, flags), b"a\\377");
        assert_eq!(strnvis(b"a\xffb", 7, flags), b"a\\377b");
        // The meta form without VIS_OCTAL is four cells too.
        assert_eq!(strnvis(b"\x81", 4, 0), b"");
        assert_eq!(strnvis(b"\x81", 5, 0), b"\\M^A");
        assert_eq!(strnvis(b"ab\0cd", 12, flags), b"ab");
    }
}
