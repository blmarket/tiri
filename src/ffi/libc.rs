//! Foreign declarations supplied by libc.
//! Only what the libc crate lacks is declared here; use the libc crate for the rest.

extern "C" {
    pub static mut environ: *mut *mut ::core::ffi::c_char;
    pub fn tzset();
}
