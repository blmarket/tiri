//! Authoritative errno declarations, supplied by the platform libc.

use std::ffi::CStr;
use std::fmt;
use std::io;

pub use ::libc::{
    E2BIG, EAGAIN, EBADMSG, ECHILD, EINTR, EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, ERANGE,
};

/// strerror(3) text of an I/O error, as tmux prints it: std's Display would
/// append " (os error N)". An error without an OS code prints itself.
pub(crate) struct ErrorText<'a>(pub &'a io::Error);

impl fmt::Display for ErrorText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(code) = self.0.raw_os_error() else {
            return fmt::Display::fmt(self.0, f);
        };
        let mut text = [0 as ::core::ffi::c_char; 128];
        // SAFETY: the XSI strerror_r writes a NUL-terminated string of at
        // most `text.len()` bytes into `text` when it succeeds.
        if unsafe { libc::strerror_r(code, text.as_mut_ptr(), text.len()) } != 0 {
            return write!(f, "Unknown error {code}");
        }
        let text = unsafe { CStr::from_ptr(text.as_ptr()) };
        f.write_str(&text.to_string_lossy())
    }
}
