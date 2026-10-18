use crate::log::{fatal, fatalx};
use crate::shared::abi::*;
use libc::calloc;
pub unsafe fn xcalloc(size: size_t) -> *mut ::core::ffi::c_void {
    if size == 0 {
        fatalx(|out| out.write_all(b"xcalloc: zero size"));
    }
    let ptr = calloc(1, size);
    if ptr.is_null() {
        fatal(&std::io::Error::last_os_error(), |out| {
            write!(out, "xcalloc: allocating {} bytes", { size })
        });
    }
    ptr
}
