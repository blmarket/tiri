use crate::src::log::{fatal, fatalx};
use crate::src::shared::abi::*;
use libc::calloc;
pub unsafe fn xcalloc(size: size_t) -> *mut ::core::ffi::c_void {
    if size == 0 {
        fatalx(|out| out.write_all(b"xcalloc: zero size"));
    }
    let ptr = calloc(1, size);
    if ptr.is_null() {
        fatal(|out| write!(out, "xcalloc: allocating {} bytes", { size }));
    }
    ptr
}
