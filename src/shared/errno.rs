//! errno helpers; the errno constants themselves come from `libc`.

use std::fmt;
use std::io;

/// strerror(3) text of an I/O error, as tmux prints it: std's Display would
/// append " (os error N)". An error without an OS code prints itself.
pub(crate) struct ErrorText<'a>(pub &'a io::Error);

impl fmt::Display for ErrorText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(code) = self.0.raw_os_error() else {
            return fmt::Display::fmt(self.0, f);
        };
        let Some(text) = tiri_rt::unix::error_description(code) else {
            return write!(f, "Unknown error {code}");
        };
        f.write_str(&text.to_string_lossy())
    }
}
