//! Rust formatting into owned bytes and bounded C-string byte slices
//!
//! ```
//! use tiri::src::format::bytes::format_bytes;
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

use std::ffi::{c_char, c_int, CStr, CString, NulError};
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
    try_format_message_with(write).unwrap_or_else(|| unsafe {
        crate::src::log::fatalx(|out| out.write_all(b"message formatting failed"))
    })
}

/// Write an unescaped C string. A missing string keeps the legacy `(null)` text.
pub fn write_cstr<'a>(out: &mut dyn Write, value: impl Into<Option<&'a CStr>>) -> io::Result<()> {
    out.write_all(value.into().map_or(b"(null)".as_slice(), CStr::to_bytes))
}

/// Borrow a C string pointer from code that has not moved to `&CStr` yet.
///
/// # Safety
/// A non-null pointer must reference a readable NUL-terminated string that
/// outlives the returned borrow.
pub unsafe fn nullable_cstr<'a>(ptr: *const c_char) -> Option<&'a CStr> {
    (!ptr.is_null()).then(|| CStr::from_ptr(ptr))
}

struct CStrWriter<'a> {
    dst: &'a mut [u8],
    written: usize,
}

impl Write for CStrWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.dst.len() - 1 - self.written {
            return Err(io::ErrorKind::WriteZero.into());
        }
        let end = self.written + bytes.len();
        self.dst[self.written..end].copy_from_slice(bytes);
        self.written = end;
        self.dst[end] = 0;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        fmt::write(self, args).map_err(|_| io::ErrorKind::Other.into())
    }
}

impl fmt::Write for CStrWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.write_all(text.as_bytes()).map_err(|_| fmt::Error)
    }
}

/// Write raw bytes and formatted text into a bounded C-string byte slice without
/// allocating.
///
/// The slice includes room for the trailing NUL. Returns the payload byte count,
/// excluding that terminator. Embedded NUL bytes are preserved and counted.
/// Empty destinations, insufficient capacity, and formatter failures return an
/// error. On failure, a nonempty destination contains a NUL-terminated prefix;
/// bytes beyond the terminator are unspecified. Limits count bytes, not columns.
/// Use `write_all` for arbitrary C-string bytes and single-byte characters, and
/// `write!` for Rust formatting. Propagate write errors from the closure.
pub fn format_cstr_with(
    dst: &mut [u8],
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<usize> {
    *dst.first_mut().ok_or(io::ErrorKind::WriteZero)? = 0;
    let mut writer = CStrWriter { dst, written: 0 };
    write(&mut writer)?;
    Ok(writer.written)
}

/// Format into a bounded C-string byte slice, exiting on overflow or format errors.
pub fn xformat(dst: &mut [u8], args: fmt::Arguments<'_>) -> c_int {
    xformat_with(dst, |out| out.write_fmt(args))
}

/// Write bytes and formatted text, exiting on overflow or write errors.
/// Returns the payload byte count, excluding the trailing NUL.
pub fn xformat_with(dst: &mut [u8], write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> c_int {
    if dst.len() > c_int::MAX as usize {
        unsafe { crate::src::log::fatalx(|out| out.write_all(b"xformat: len > INT_MAX")) };
    }
    match format_cstr_with(dst, write) {
        Ok(written) => written as c_int,
        Err(_) => unsafe {
            crate::src::log::fatalx(|out| out.write_all(b"xformat: formatting failed or overflow"))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    byte as c_int,
                    u32::MAX,
                )
            };
            let mut actual = [42u8; 64];
            let written = xformat_with(&mut actual, |out| {
                write!(out, "{:4}: ", 7u32)?;
                out.write_all(value.as_bytes())?;
                out.write_all(&[b':', byte, b':'])?;
                write!(out, "{}", u32::MAX)
            });
            assert_eq!(written, count);
            assert_eq!(&actual[..=count as usize], &expected[..=count as usize]);
        }
    }

    #[test]
    fn mixed_output_checks_capacity_and_propagates_write_errors() {
        let mut dst = [42u8; 6];
        let n = format_cstr_with(&mut dst[..3], |out| out.write_all(b"\xffx")).unwrap();
        assert_eq!(n, 2);
        assert_eq!(dst[2], 0);
        assert_eq!(dst[3], 42);
        let n = format_cstr_with(&mut dst[n..], |out| {
            out.write_all(&[0])?;
            write!(out, "{}", 12)
        })
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(dst, *b"\xffx\x0012\0");

        let error = format_cstr_with(&mut dst[..3], |out| {
            out.write_all(b"a")?;
            out.write_all(b"bc")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(&dst[..2], b"a\0");
        let error = format_cstr_with(&mut dst, |out| {
            out.write_all(b"ok")?;
            Err(io::ErrorKind::InvalidData.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(dst[2], 0);
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
