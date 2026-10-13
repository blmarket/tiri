#[cfg(target_os = "linux")]
pub unsafe fn getprogname() -> &'static std::ffi::CStr {
    std::ffi::CStr::from_ptr(crate::src::ffi::libc::program_invocation_short_name)
}
#[cfg(not(target_os = "linux"))]
pub unsafe fn getprogname() -> &'static std::ffi::CStr {
    std::ffi::CStr::from_ptr(::libc::getprogname())
}
