use crate::src::shared::posix_io::glob_t;
use libc::{glob, globfree};
use std::ffi::{c_char, c_int, CStr};

/// Owns the storage allocated by libc glob, including partial results on error.
pub(crate) struct GlobResult(glob_t);

impl GlobResult {
    pub(crate) unsafe fn run(pattern: &CStr) -> (Self, c_int) {
        let mut result = Self(::core::mem::zeroed());
        let status = glob(pattern.as_ptr(), 0, None, &mut result.0);
        (result, status)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.gl_pathc
    }

    /// The returned path is borrowed until this result is dropped.
    pub(crate) unsafe fn path(&self, index: usize) -> &std::ffi::CStr {
        debug_assert!(index < self.len());
        std::ffi::CStr::from_ptr(*self.0.gl_pathv.add(index))
    }
}

impl Drop for GlobResult {
    fn drop(&mut self) {
        unsafe { globfree(&mut self.0) }
    }
}
