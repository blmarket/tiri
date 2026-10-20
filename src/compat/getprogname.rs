use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::sync::OnceLock;

/// The final path component of argv[0], as glibc's short invocation name
/// and BSD getprogname report it.
pub fn getprogname() -> &'static CStr {
    static PROGNAME: OnceLock<Option<CString>> = OnceLock::new();
    PROGNAME
        .get_or_init(|| {
            let argv0 = std::env::args_os().next()?;
            let name = argv0.as_bytes().rsplit(|&byte| byte == b'/').next()?;
            CString::new(name).ok()
        })
        .as_deref()
        .unwrap_or(c"tiri")
}
