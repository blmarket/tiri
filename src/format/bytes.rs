//! Rust formatting into owned bytes and C strings
//!
//! ```
//! use tiri::format::bytes::format_bytes;
//! use std::ffi::CString;
//! use std::io::Write;
//!
//! let name = CString::new(vec![0xff, b'x']).unwrap();
//! let mut bytes = format_bytes(format_args!("client {}: ", 7));
//! bytes.extend_from_slice(name.as_bytes());
//! write!(&mut bytes, " ({:04x})", 42).unwrap();
//! let value = CString::new(bytes).unwrap();
//! assert_eq!(value.as_bytes(), b"client 7: \xffx (002a)");
//! ```

use std::ffi::{c_char, CStr, CString, NulError};
use std::fmt;
use std::io::{self, Write};

/// Format into bytes. Embedded NUL bytes are preserved.
pub fn format_bytes(args: fmt::Arguments<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes
        .write_fmt(args)
        .expect("formatting into a byte vector failed");
    bytes
}

/// Format into a NUL-terminated string, rejecting embedded NUL bytes.
pub fn format_cstring(args: fmt::Arguments<'_>) -> Result<CString, NulError> {
    CString::new(format_bytes(args))
}

/// Assemble raw bytes and typed formatting, preserving embedded NUL bytes.
pub fn format_bytes_with(
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<Vec<u8>> {
    struct Writer(Vec<u8>);
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .try_reserve(bytes.len())
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
            fmt::write(self, args).map_err(|_| io::ErrorKind::Other.into())
        }
    }
    impl fmt::Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            self.write_all(text.as_bytes()).map_err(|_| fmt::Error)
        }
    }
    let mut out = Writer(Vec::new());
    write(&mut out)?;
    Ok(out.0)
}

/// Assemble a C-string message; as with C-string consumers, the first NUL ends it.
pub fn try_format_message_with(
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Option<CString> {
    let mut bytes = format_bytes_with(write).ok()?;
    if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
        bytes.truncate(end);
    }
    Some(CString::new(bytes).expect("message was truncated before its first NUL"))
}

pub fn format_message_with(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> CString {
    try_format_message_with(write)
        .unwrap_or_else(|| crate::log::fatalx(|out| out.write_all(b"message formatting failed")))
}

/// Write an unescaped C string. A missing string keeps the legacy `(null)` text.
pub fn write_cstr<'a>(out: &mut dyn Write, value: impl Into<Option<&'a CStr>>) -> io::Result<()> {
    out.write_all(value.into().map_or(b"(null)".as_slice(), CStr::to_bytes))
}

/// Borrow the tail of a C string starting at byte `offset`.
///
/// Panics if `offset` is past the terminating NUL.
pub fn cstr_suffix(s: &CStr, offset: usize) -> &CStr {
    CStr::from_bytes_with_nul(&s.to_bytes_with_nul()[offset..])
        .expect("a suffix of a C string is NUL-terminated")
}

/// Copy `src` into a fixed C buffer like `strlcpy`: at most `dst.len() - 1`
/// bytes and a NUL; bytes after the NUL are left alone. Returns the length of
/// `src`, so a result `>= dst.len()` means the copy was truncated.
pub fn cstr_copy_into(dst: &mut [u8], src: &CStr) -> usize {
    let src = src.to_bytes();
    if let Some(room) = dst.len().checked_sub(1) {
        let len = src.len().min(room);
        dst[..len].copy_from_slice(&src[..len]);
        dst[len] = 0;
    }
    src.len()
}

/// Append `src` to the C string in a fixed buffer like `strlcat`. Returns the
/// length of the string it tried to create.
pub fn cstr_append_into(dst: &mut [u8], src: &CStr) -> usize {
    match dst.iter().position(|&byte| byte == 0) {
        Some(len) => len + cstr_copy_into(&mut dst[len..], src),
        None => dst.len() + src.to_bytes().len(),
    }
}

/// POSIX `basename`: trailing slashes are ignored, an all-slash path is `/`
/// and an empty path is `.`.
pub fn cstr_basename(path: &CStr) -> CString {
    let path = path.to_bytes();
    let base: &[u8] = if path.is_empty() {
        b"."
    } else if let Some(last) = path.iter().rposition(|&byte| byte != b'/') {
        let path = &path[..=last];
        let start = path
            .iter()
            .rposition(|&byte| byte == b'/')
            .map_or(0, |i| i + 1);
        &path[start..]
    } else {
        b"/"
    };
    CString::new(base).expect("a part of a C string has no NUL")
}

/// Borrow a C string pointer from code that has not moved to `&CStr` yet.
///
/// # Safety
/// A non-null pointer must reference a readable NUL-terminated string that
/// outlives the returned borrow.
pub unsafe fn nullable_cstr<'a>(ptr: *const c_char) -> Option<&'a CStr> {
    (!ptr.is_null()).then(|| CStr::from_ptr(ptr))
}

#[cfg(test)]
mod tests {
    use super::*;

    // (buffer, src, after copy, copy result, after append, append result)
    type CopyCase = (
        &'static [u8],
        &'static CStr,
        &'static [u8],
        usize,
        &'static [u8],
        usize,
    );

    #[test]
    fn copy_and_append_match_strlcpy_and_strlcat() {
        let cases: &[CopyCase] = &[
            (b"#", c"", b"\0", 0, b"#", 1),
            (b"########", c"abc", b"abc\0####", 3, b"########", 11),
            (b"####", c"abcdefgh", b"abc\0", 8, b"####", 12),
            (b"ab\0#####", c"cd", b"cd\0#####", 2, b"abcd\0###", 4),
            (b"abc\0##", c"defgh", b"defgh\0", 5, b"abcde\0", 8),
            (b"abcdef", c"gh", b"gh\0def", 2, b"abcdef", 8),
            (b"", c"gh", b"", 2, b"", 2),
        ];
        for &(initial, src, copied, copy_len, appended, append_len) in cases {
            let mut buffer = initial.to_vec();
            assert_eq!(cstr_copy_into(&mut buffer, src), copy_len);
            assert_eq!(buffer, copied);
            let mut buffer = initial.to_vec();
            assert_eq!(cstr_append_into(&mut buffer, src), append_len);
            assert_eq!(buffer, appended);
        }
    }

    #[test]
    fn basename_follows_posix() {
        for (path, base) in [
            (c"", c"."),
            (c"/", c"/"),
            (c"//", c"/"),
            (c"///", c"/"),
            (c"name", c"name"),
            (c"/usr/bin/vim", c"vim"),
            (c"/usr/bin/", c"bin"),
            (c"usr//", c"usr"),
            (c"a//b///", c"b"),
            (c"/\xffx", c"\xffx"),
        ] {
            assert_eq!(cstr_basename(path).as_c_str(), base, "{path:?}");
        }
    }

    #[test]
    fn byte_output_preserves_nul_while_cstring_reports_it() {
        assert_eq!(format_bytes(format_args!("a\0{}", 7)), b"a\x007");
        let error = format_cstring(format_args!("a\0{}", 7)).unwrap_err();
        assert_eq!(error.nul_position(), 1);
        assert_eq!(error.into_vec(), b"a\x007");
    }

    #[test]
    fn mixed_output_matches_c_bytes_characters_and_widths() {
        let value = CString::new(b"name\xff".as_slice()).unwrap();
        for byte in [0u8, b'M', 0x80, 0xff] {
            let mut expected = [0u8; 64];
            let count = unsafe {
                libc::snprintf(
                    expected.as_mut_ptr().cast(),
                    expected.len(),
                    c"%4u: %s:%c:%u".as_ptr(),
                    7u32,
                    value.as_ptr(),
                    byte as std::ffi::c_int,
                    u32::MAX,
                )
            };
            let actual = format_bytes_with(|out| {
                write!(out, "{:4}: ", 7u32)?;
                out.write_all(value.as_bytes())?;
                out.write_all(&[b':', byte, b':'])?;
                write!(out, "{}", u32::MAX)
            })
            .unwrap();
            assert_eq!(actual.len(), count as usize);
            assert_eq!(actual, &expected[..count as usize]);
        }
    }

    #[test]
    fn owned_messages_and_byte_buffers_keep_their_distinct_nul_contracts() {
        let bytes = format_bytes_with(|out| out.write_all(b"\xffbefore\0after")).unwrap();
        assert_eq!(bytes, b"\xffbefore\0after");
        let message = format_message_with(|out| out.write_all(&bytes));
        assert_eq!(message.as_bytes(), b"\xffbefore");
        assert!(try_format_message_with(|_| Err(io::ErrorKind::InvalidData.into())).is_none());
    }

    #[test]
    fn raw_c_strings_stop_at_nul_and_keep_legacy_null_text() {
        unsafe {
            let raw = [0xffu8, b'x', 0, b'y', 0];
            let actual =
                format_bytes_with(|out| write_cstr(out, nullable_cstr(raw.as_ptr().cast())))
                    .unwrap();
            assert_eq!(actual, b"\xffx");
            let actual =
                format_bytes_with(|out| write_cstr(out, nullable_cstr(std::ptr::null()))).unwrap();
            assert_eq!(actual, b"(null)");
        }
    }
}
