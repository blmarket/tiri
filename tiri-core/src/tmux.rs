use crate::compat::getprogname::getprogname;
use crate::shared::abi::uint64_t;
use crate::shared::environment::environ;
use crate::shared::posix_io::X_OK;
use crate::shared::time::CLOCK_REALTIME;
use crate::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::text::utf8::{utf8_isvalid, utf8_stravis_cstring};
use libc::CLOCK_MONOTONIC;
use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStringExt;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static GLOBAL_ENVIRON: std::cell::RefCell<Option<Box<environ>>> = const {
        std::cell::RefCell::new(None)
    };
    static START_TIME: std::cell::Cell<SystemTime> = const { std::cell::Cell::new(UNIX_EPOCH) };
    static SOCKET_PATH: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
}

pub fn socket_path_cstr() -> CString {
    SOCKET_PATH
        .with_borrow(|path| path.clone())
        .expect("socket path is set")
}

pub fn set_socket_path(path: CString) {
    SOCKET_PATH.set(Some(path));
}

pub fn replace_global_environ(environment: Option<Box<environ>>) -> Option<Box<environ>> {
    GLOBAL_ENVIRON.replace(environment)
}

pub fn global_environ<R>(read: impl FnOnce(&environ) -> R) -> R {
    GLOBAL_ENVIRON.with_borrow(|environment| read(environment.as_deref().expect("environment")))
}

pub fn global_environ_mut<R>(edit: impl FnOnce(&mut environ) -> R) -> R {
    GLOBAL_ENVIRON
        .with_borrow_mut(|environment| edit(environment.as_deref_mut().expect("environment")))
}

pub fn start_time() -> SystemTime {
    START_TIME.get()
}

pub(crate) fn set_start_time(time: SystemTime) {
    START_TIME.set(time);
}

pub fn env_cstring(name: &str) -> Option<CString> {
    std::env::var_os(name).and_then(|value| CString::new(value.into_vec()).ok())
}

pub fn checkshell(shell: Option<&CStr>) -> ::core::ffi::c_int {
    let Some(shell) = shell else {
        return 0 as ::core::ffi::c_int;
    };
    if shell.to_bytes().first() != Some(&b'/') {
        return 0 as ::core::ffi::c_int;
    }
    if areshell(shell) != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if !tiri_sys::accessible(shell, X_OK) {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}

fn areshell(shell: &CStr) -> ::core::ffi::c_int {
    let name = shell
        .to_bytes()
        .rsplit(|&byte| byte == b'/')
        .next()
        .unwrap_or_default();
    let progname = getprogname().to_bytes();
    let progname = progname.strip_prefix(b"-").unwrap_or(progname);
    (name == progname) as ::core::ffi::c_int
}

pub fn shell_argv0_cstring(shell: &CStr, is_login: bool) -> CString {
    let shell = shell.to_bytes();
    let name = match shell.iter().rposition(|&byte| byte == b'/') {
        Some(slash) if slash + 1 < shell.len() => &shell[slash + 1..],
        _ => shell,
    };
    let mut argv0 = Vec::with_capacity(name.len() + usize::from(is_login));
    if is_login {
        argv0.push(b'-');
    }
    argv0.extend_from_slice(name);
    CString::new(argv0).expect("shell path contains no NUL")
}

pub fn setblocking(fd: std::os::fd::BorrowedFd<'_>, state: ::core::ffi::c_int) {
    let _ = tiri_sys::set_nonblocking(fd, state == 0);
}

pub fn get_timer() -> uint64_t {
    let time = tiri_sys::clock_time(CLOCK_MONOTONIC)
        .or_else(|_| tiri_sys::clock_time(CLOCK_REALTIME))
        .unwrap_or_default();
    time.as_millis() as uint64_t
}

pub fn clean_name_cstring(name: &CStr, untrusted: ::core::ffi::c_int) -> Option<CString> {
    if !utf8_isvalid(name) {
        return None;
    }
    let mut copy = name.to_bytes_with_nul().to_vec();
    if untrusted != 0 {
        for i in 0..copy.len() - 1 {
            if copy[i] == b'#' && copy[i + 1] == b'(' {
                copy[i] = b'_';
            }
        }
    }
    Some(utf8_stravis_cstring(
        CStr::from_bytes_with_nul(&copy).expect("copied name has one trailing NUL"),
        VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL,
    ))
}

pub fn check_name(name: &CStr) -> bool {
    utf8_isvalid(name)
}

pub fn sig2name(signo: ::core::ffi::c_int) -> CString {
    match tiri_sys::signal_name(signo) {
        Some(name) => name.to_owned(),
        None => CString::new(signo.to_string()).expect("decimal number has no NUL"),
    }
}

pub fn find_home_cstr() -> Option<&'static CStr> {
    static HOME: OnceLock<CString> = OnceLock::new();
    if let Some(home) = HOME.get() {
        return Some(home.as_c_str());
    }
    let owned = match env_cstring("HOME").filter(|home| !home.is_empty()) {
        Some(home) => home,
        None => tiri_sys::user_home(tiri_sys::user_id())?,
    };
    let _ = HOME.set(owned);
    HOME.get().map(CString::as_c_str)
}

pub fn getversion() -> &'static CStr {
    c"3.8"
}
