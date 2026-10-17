//! Arguments for the escaped debug-log format.
//!
//! Escape byte inputs here, before interpolation. The debug sink must not run
//! `strvis` again: doing so would double the backslashes in these arguments.
use crate::src::compat::vis::vis;
use crate::src::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use std::ffi::{c_char, c_int, CStr};
use std::fmt;

/// Display bytes using the logger's existing escaping, stopping at the first NUL.
pub fn log_bytes(bytes: &[u8]) -> impl fmt::Display + '_ {
    LogBytes(bytes)
}

struct LogBytes<'a>(&'a [u8]);

impl fmt::Display for LogBytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in self.0.iter().take_while(|&&byte| byte != 0) {
            escaped_byte(byte, f)?;
        }
        Ok(())
    }
}

fn escaped_byte(byte: u8, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut escaped = [0 as c_char; 5];
    let end = unsafe {
        vis(
            escaped.as_mut_ptr(),
            byte.into(),
            VIS_CSTYLE | VIS_NL | VIS_OCTAL | VIS_TAB,
            0,
        )
    };
    let len = unsafe { end.offset_from(escaped.as_ptr()) as usize };
    let bytes = unsafe { std::slice::from_raw_parts(escaped.as_ptr().cast(), len) };
    f.write_str(std::str::from_utf8(bytes).expect("vis produces ASCII"))
}

/// Display a C string using the logger's existing escaping.
pub fn log_cstr(s: &CStr) -> impl fmt::Display + '_ {
    LogBytes(s.to_bytes())
}

/// Bytes of a C string, padded like printf's `%*s` before escaping.
pub struct LogCStr<'a> {
    bytes: &'a [u8],
    width: c_int,
}

/// Display a byte buffer up to its first NUL, like printf's `%.*s` with the
/// buffer length as the precision.
pub fn log_cstr_n(bytes: &[u8]) -> LogCStr<'_> {
    let len = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    LogCStr {
        bytes: &bytes[..len],
        width: 0,
    }
}

/// Display a C string, padding to a byte width before escaping. Negative
/// widths select left alignment, as with printf's `%*s`.
pub fn log_cstr_width(s: &CStr, width: c_int) -> LogCStr<'_> {
    LogCStr {
        bytes: s.to_bytes(),
        width,
    }
}

impl fmt::Display for LogCStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.bytes;
        let padding = (self.width.unsigned_abs() as usize).saturating_sub(bytes.len());
        if self.width >= 0 {
            for _ in 0..padding {
                f.write_str(" ")?;
            }
        }
        fmt::Display::fmt(&LogBytes(bytes), f)?;
        if self.width < 0 {
            for _ in 0..padding {
                f.write_str(" ")?;
            }
        }
        Ok(())
    }
}

/// Escape one printf-style byte. NUL terminates the assembled log message.
pub fn log_byte(byte: u8) -> impl fmt::Display {
    struct Byte(u8);
    impl fmt::Display for Byte {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0 == 0 {
                f.write_str("\0")
            } else {
                escaped_byte(self.0, f)
            }
        }
    }
    Byte(byte)
}

/// Preserve libc's `(nil)` spelling for a null pointer.
pub fn log_pointer<T: ?Sized>(ptr: *const T) -> impl fmt::Display {
    struct Pointer(usize);
    impl fmt::Display for Pointer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0 == 0 {
                f.write_str("(nil)")
            } else {
                write!(f, "{:#x}", self.0)
            }
        }
    }
    Pointer(ptr.addr())
}

/// Preserve printf's `%#x`/`%#llx` spelling: zero has no `0x` prefix.
pub fn log_hex(value: u64) -> impl fmt::Display {
    struct Hex(u64);
    impl fmt::Display for Hex {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.0 == 0 {
                f.write_str("0")
            } else {
                write!(f, "{:#x}", self.0)
            }
        }
    }
    Hex(value)
}
