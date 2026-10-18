//! Scoped ownership for streams returned by libc's `fopen` and `fdopen`.

use crate::shared::stdio::FILE;
use libc::fclose;
use std::ptr::NonNull;

pub(crate) struct CFile(NonNull<FILE>);

impl CFile {
    /// Take ownership of a non-null C stream.
    ///
    /// # Safety
    ///
    /// `file` must be an open stream whose ownership is transferred to this
    /// value and which has not already been closed.
    pub(crate) unsafe fn from_raw(file: *mut FILE) -> Option<Self> {
        NonNull::new(file).map(Self)
    }

    pub(crate) fn as_ptr(&self) -> *mut FILE {
        self.0.as_ptr()
    }
}

impl Drop for CFile {
    fn drop(&mut self) {
        unsafe {
            fclose(self.0.as_ptr());
        }
    }
}
