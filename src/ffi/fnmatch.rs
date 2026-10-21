//! Bounded calls into libc fnmatch.

use std::ffi::{c_int, CStr};

/// Whether `string` matches the shell wildcard `pattern` under `flags`.
pub fn fnmatch_matches(pattern: &CStr, string: &CStr, flags: c_int) -> bool {
    // SAFETY: fnmatch only reads both NUL-terminated strings.
    unsafe { libc::fnmatch(pattern.as_ptr(), string.as_ptr(), flags) == 0 }
}
