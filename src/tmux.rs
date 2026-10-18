#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_assignments,
    unused_mut
)]
use crate::cfg::{cfg_quiet, cfg_set_files};
use crate::client::client_main;
use crate::compat::fdforkpty::getptmfd;
use crate::compat::getopt_long::{BSDgetopt, BSDoptarg, BSDoptind};
use crate::compat::getprogname::getprogname;
use crate::environ::{environ_create, environ_find, environ_put, environ_set};
use crate::ffi::libc::{environ, err, errx, stderr, stdout, tzset};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::log::{log_add_level, log_cstr, log_debug};
use crate::options::options_table;
use crate::options::{options_set_number, options_set_string, OptionsScope};
use crate::options::{OPTIONS_TABLE_SERVER, OPTIONS_TABLE_SESSION, OPTIONS_TABLE_WINDOW};
use crate::shared::abi::*;
use crate::shared::abi::{__mode_t, uid_t};
use crate::shared::account::passwd;
use crate::shared::client::{
    CLIENT_CONTROL, CLIENT_CONTROLCONTROL, CLIENT_DEFAULTSOCKET, CLIENT_LOGIN, CLIENT_NOFORK,
    CLIENT_NOSTARTSERVER, CLIENT_UTF8,
};
use crate::shared::environment::{environ, environ_entry};
use crate::shared::key::{MODEKEY_EMACS, MODEKEY_VI};
use crate::shared::posix_io::stat;
use crate::shared::posix_io::{S_IRWXU, X_OK};
use crate::shared::time::{timespec, CLOCK_REALTIME};
use crate::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::text::utf8::{utf8_isvalid, utf8_stravis_cstring};
use crate::tty::tty_parse_features;
use ::libc::nl_item;
use libc::{
    access, clock_gettime, exit, fprintf, getcwd, getenv, getpwuid, getuid, lstat, mkdir,
    nl_langinfo, printf, realpath, setlocale, strcasecmp, strcasestr, strcmp, strrchr, strstr,
};
use std::ffi::{CStr, CString};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub use ::libc::{CLOCK_MONOTONIC, CODESET, EEXIST, LC_CTYPE, LC_TIME};

pub use ::libc::{S_IFDIR, S_IFMT};

pub const TMUX_SOCK_PERM: ::core::ffi::c_int = 7 as ::core::ffi::c_int;
pub static mut global_environ: Option<Box<environ>> = None;
pub static mut start_time: SystemTime = UNIX_EPOCH;
pub static mut socket_path: Option<CString> = None;
pub static mut ptm_fd: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
pub static mut shell_command: Option<CString> = None;
/// The socket path chosen at startup (or by systemd activation).
pub unsafe fn socket_path_cstr() -> &'static CStr {
    socket_path.as_deref().expect("socket path is set")
}
unsafe fn usage(mut status: ::core::ffi::c_int) -> ! {
    fprintf(
        if status != 0 { stderr } else { stdout },
        c"usage: %s [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n            [-S socket-path] [-T features] [command [flags]]\n".as_ptr(),
        getprogname().as_ptr(),
    );
    exit(status);
}
unsafe fn getshell() -> CString {
    let shell = nullable_cstr(getenv(c"SHELL".as_ptr()));
    if let Some(shell) = shell.filter(|shell| checkshell(Some(shell)) != 0) {
        return shell.to_owned();
    }
    let pw = getpwuid(getuid());
    if !pw.is_null() {
        if let Some(shell) =
            nullable_cstr((*pw).pw_shell).filter(|shell| checkshell(Some(shell)) != 0)
        {
            return shell.to_owned();
        }
    }
    c"/bin/sh".to_owned()
}
pub unsafe fn checkshell(shell: Option<&CStr>) -> ::core::ffi::c_int {
    let Some(shell) = shell else {
        return 0 as ::core::ffi::c_int;
    };
    if shell.to_bytes().first() != Some(&b'/') {
        return 0 as ::core::ffi::c_int;
    }
    if areshell(shell) != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if access(shell.as_ptr(), X_OK) != 0 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}
unsafe fn areshell(shell: &CStr) -> ::core::ffi::c_int {
    let mut progname: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut ptr: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    ptr = strrchr(shell.as_ptr(), '/' as i32);
    if !ptr.is_null() {
        ptr = ptr.offset(1);
    } else {
        ptr = shell.as_ptr();
    }
    progname = getprogname().as_ptr();
    if *progname as ::core::ffi::c_int == '-' as i32 {
        progname = progname.offset(1);
    }
    if strcmp(ptr, progname) == 0 as ::core::ffi::c_int {
        return 1 as ::core::ffi::c_int;
    }
    0 as ::core::ffi::c_int
}
unsafe fn expand_path(path: &CStr, home: Option<&CStr>) -> Option<CString> {
    let mut value: Option<&environ_entry> = None;
    let path_bytes = path.to_bytes();
    if path_bytes.starts_with(b"~/") {
        let mut expanded = home?.to_bytes().to_vec();
        expanded.extend_from_slice(&path_bytes[1..]);
        return Some(CString::new(expanded).expect("C strings contain no interior NUL"));
    }
    if path_bytes.first() == Some(&b'$') {
        let slash = path_bytes.iter().position(|byte| *byte == b'/');
        let name_end = slash.unwrap_or(path_bytes.len());
        let name =
            CString::new(&path_bytes[1..name_end]).expect("variable name comes from a C string");
        value = environ_find(global_environ.as_deref().expect("environment"), &name);
        value?;
        let mut expanded = if value.unwrap().value.is_none() {
            b"(null)".to_vec()
        } else {
            (value.unwrap().value)
                .as_deref()
                .expect("string is present")
                .to_bytes()
                .to_vec()
        };
        if let Some(slash) = slash {
            expanded.extend_from_slice(&path_bytes[slash..]);
        }
        return Some(CString::new(expanded).expect("C strings contain no interior NUL"));
    }
    Some(path.to_owned())
}
unsafe fn expand_paths(s: &CStr, no_realpath: bool) -> Vec<CString> {
    let home = find_home_cstr();
    let mut resolved = [0u8; 4096];
    let mut paths = Vec::new();
    for next in s.to_bytes().split(|&b| b == b':') {
        let next = CString::new(next).expect("a path from a C string contains no NUL");
        if let Some(expanded) = expand_path(&next, home) {
            let path = if no_realpath {
                Some(expanded)
            } else {
                let path = if realpath(expanded.as_ptr(), resolved.as_mut_ptr().cast()).is_null() {
                    log_debug(format_args!(
                        "{}: realpath(\"{}\") failed: {}",
                        "expand_paths",
                        log_cstr(&expanded),
                        errno::errno()
                    ));
                    None
                } else {
                    Some(
                        CStr::from_bytes_until_nul(&resolved)
                            .expect("realpath NUL-terminates")
                            .to_owned(),
                    )
                };
                drop(expanded);
                path
            };
            if let Some(path) = path {
                if paths.iter().any(|existing| existing == &path) {
                    log_debug(format_args!(
                        "{}: duplicate path: {}",
                        "expand_paths",
                        log_cstr(&path)
                    ));
                } else {
                    paths.push(path);
                }
            }
        } else {
            log_debug(format_args!(
                "{}: invalid path: {}",
                "expand_paths",
                log_cstr(&next)
            ));
        }
    }
    paths
}
fn make_label_cause(parts: &[&[u8]]) -> CString {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(part);
    }
    CString::new(bytes).expect("socket label cause contains no NUL")
}

unsafe fn make_label(label: Option<&CStr>) -> Result<CString, CString> {
    let mut sb: stat = unsafe { ::core::mem::zeroed() };
    let mut uid: uid_t = 0;
    uid = getuid() as uid_t;
    let paths = expand_paths(c"$TMUX_TMPDIR:/tmp/", false);
    if paths.is_empty() {
        return Err(c"no suitable socket path".to_owned());
    }
    let mut base_bytes = paths[0].to_bytes().to_vec();
    base_bytes.extend_from_slice(b"/tmux-");
    base_bytes.extend_from_slice((uid as ::core::ffi::c_long).to_string().as_bytes());
    let base = CString::new(base_bytes).expect("socket base contains no NUL");
    if mkdir(base.as_ptr(), S_IRWXU as __mode_t) != 0 as ::core::ffi::c_int
        && errno::errno().0 != EEXIST
    {
        let error = errno::errno().to_string();
        Err(make_label_cause(&[
            b"couldn't create directory ",
            base.to_bytes(),
            b" (",
            error.as_bytes(),
            b")",
        ]))
    } else if lstat(base.as_ptr(), &raw mut sb) != 0 as ::core::ffi::c_int {
        let error = errno::errno().to_string();
        Err(make_label_cause(&[
            b"couldn't read directory ",
            base.to_bytes(),
            b" (",
            error.as_bytes(),
            b")",
        ]))
    } else if sb.st_mode & S_IFMT != S_IFDIR {
        Err(make_label_cause(&[base.to_bytes(), b" is not a directory"]))
    } else if sb.st_uid != uid || sb.st_mode & TMUX_SOCK_PERM as __mode_t != 0 as __mode_t {
        Err(make_label_cause(&[
            b"directory ",
            base.to_bytes(),
            b" has unsafe permissions",
        ]))
    } else {
        let mut path = base.into_bytes();
        path.push(b'/');
        path.extend_from_slice(label.unwrap_or(c"default").to_bytes());
        Ok(CString::new(path).expect("socket label path contains no NUL"))
    }
}
pub(crate) fn shell_argv0_cstring(shell: &CStr, is_login: bool) -> CString {
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
pub unsafe fn setblocking(fd: ::core::ffi::c_int, state: ::core::ffi::c_int) {
    if fd >= 0 {
        let _ = tiri_rt::unix::set_nonblocking(std::os::fd::BorrowedFd::borrow_raw(fd), state == 0);
    }
}
pub unsafe fn get_timer() -> uint64_t {
    let mut ts: timespec = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if clock_gettime(CLOCK_MONOTONIC, &raw mut ts) != 0 as ::core::ffi::c_int {
        clock_gettime(CLOCK_REALTIME, &raw mut ts);
    }
    (ts.tv_sec as ::core::ffi::c_ulonglong)
        .wrapping_mul(1000 as ::core::ffi::c_ulonglong)
        .wrapping_add(
            (ts.tv_nsec as ::core::ffi::c_ulonglong)
                .wrapping_div(1000000 as ::core::ffi::c_ulonglong),
        ) as uint64_t
}
/// Escape a validated name once before moving it into its Rust owner.
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
    CString::new(signo.to_string()).expect("decimal number has no NUL")
}
pub unsafe fn find_cwd() -> Option<CString> {
    let mut resolved1: [::core::ffi::c_char; 4096] = [0; 4096];
    let mut resolved2: [::core::ffi::c_char; 4096] = [0; 4096];
    let mut buffer = [0u8; 4096];
    if getcwd(buffer.as_mut_ptr().cast(), buffer.len() as size_t).is_null() {
        return None;
    }
    let cwd = CStr::from_bytes_until_nul(&buffer)
        .expect("getcwd NUL-terminates")
        .to_owned();
    let Some(pwd) = nullable_cstr(getenv(c"PWD".as_ptr())) else {
        return Some(cwd);
    };
    if pwd.is_empty()
        || realpath(pwd.as_ptr(), resolved1.as_mut_ptr()).is_null()
        || realpath(cwd.as_ptr(), resolved2.as_mut_ptr()).is_null()
        || strcmp(resolved1.as_ptr(), resolved2.as_ptr()) != 0 as ::core::ffi::c_int
    {
        return Some(cwd);
    }
    Some(pwd.to_owned())
}
/// Return the cached home directory as a borrowed C string. A missing result
/// is retried on the next call, matching the old null-sentinel cache behavior.
pub(crate) unsafe fn find_home_cstr() -> Option<&'static CStr> {
    let mut pw: *mut passwd = ::core::ptr::null_mut::<passwd>();
    static HOME: OnceLock<CString> = OnceLock::new();
    if let Some(home) = HOME.get() {
        return Some(home.as_c_str());
    }
    let mut home = getenv(c"HOME".as_ptr());
    if home.is_null() || *home as ::core::ffi::c_int == '\0' as i32 {
        pw = getpwuid(getuid());
        if !pw.is_null() {
            home = (*pw).pw_dir;
        } else {
            return None;
        }
    }
    let owned = CStr::from_ptr(home).to_owned();
    let _ = HOME.set(owned);
    HOME.get().map(CString::as_c_str)
}

pub fn getversion() -> &'static CStr {
    c"3.8"
}
unsafe fn main_0(args: &[CString]) -> ::core::ffi::c_int {
    let mut argc = ::core::ffi::c_int::try_from(args.len()).expect("argv length exceeds c_int");
    let mut argv_view: Vec<_> = args.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
    argv_view.push(::core::ptr::null_mut());
    let mut argv = argv_view.as_mut_ptr();
    let mut path: Option<CString> = None;
    let mut label: Option<&CStr> = None;
    let mut var: *mut *mut ::core::ffi::c_char =
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut opt: ::core::ffi::c_int = 0;
    let mut keys: ::core::ffi::c_int = 0;
    let mut feat: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut fflag: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut flags: uint64_t = 0 as uint64_t;
    if setlocale(LC_CTYPE, c"en_US.UTF-8".as_ptr()).is_null()
        && setlocale(LC_CTYPE, c"C.UTF-8".as_ptr()).is_null()
    {
        if setlocale(LC_CTYPE, c"".as_ptr()).is_null() {
            errx(
                1 as ::core::ffi::c_int,
                c"invalid LC_ALL, LC_CTYPE or LANG".as_ptr(),
            );
        }
        s = nl_langinfo(CODESET as ::core::ffi::c_int as nl_item);
        if strcasecmp(s, c"UTF-8".as_ptr()) != 0 as ::core::ffi::c_int
            && strcasecmp(s, c"UTF8".as_ptr()) != 0 as ::core::ffi::c_int
        {
            errx(
                1 as ::core::ffi::c_int,
                c"need UTF-8 locale (LC_CTYPE) but have %s".as_ptr(),
                s,
            );
        }
    }
    setlocale(LC_TIME, c"".as_ptr());
    tzset();
    if args.first().and_then(|arg| arg.as_bytes().first()) == Some(&b'-') {
        flags = CLIENT_LOGIN as uint64_t;
    }
    global_environ = Some(environ_create());
    var = environ;
    while !(*var).is_null() {
        environ_put(
            global_environ.as_deref_mut().expect("environment"),
            CStr::from_ptr(*var),
            0 as ::core::ffi::c_int,
        );
        var = var.offset(1);
    }
    if let Some(cwd) = find_cwd() {
        environ_set(
            global_environ.as_deref_mut().expect("environment"),
            c"PWD",
            0 as ::core::ffi::c_int,
            |out| out.write_all(cwd.as_bytes()),
        );
    }
    let mut config_paths = expand_paths(TMUX_CONF, true);
    loop {
        opt = BSDgetopt(argc, argv, c"2c:CDdf:hlL:NqS:T:uUvV".as_ptr());
        if !(opt != -(1 as ::core::ffi::c_int)) {
            break;
        }
        match opt {
            50 => {
                tty_parse_features(
                    c"256",
                    c":,",
                    &raw mut feat,
                    ::core::ptr::null_mut::<::core::ffi::c_int>(),
                );
            }
            99 => {
                shell_command = Some(CStr::from_ptr(BSDoptarg).to_owned());
            }
            68 => {
                flags |= CLIENT_NOFORK as uint64_t;
            }
            67 => {
                if flags & CLIENT_CONTROL as uint64_t != 0 {
                    flags |= CLIENT_CONTROLCONTROL as uint64_t;
                } else {
                    flags |= CLIENT_CONTROL as uint64_t;
                }
            }
            102 => {
                if fflag == 0 {
                    fflag = 1 as ::core::ffi::c_int;
                    config_paths.clear();
                }
                config_paths.push(CStr::from_ptr(BSDoptarg).to_owned());
                cfg_quiet = 0 as ::core::ffi::c_int;
            }
            104 => {
                usage(0 as ::core::ffi::c_int);
            }
            86 => {
                printf(c"tmux %s\n".as_ptr(), getversion().as_ptr());
                exit(0 as ::core::ffi::c_int);
            }
            108 => {
                flags |= CLIENT_LOGIN as uint64_t;
            }
            76 => {
                label = Some(CStr::from_ptr(BSDoptarg));
            }
            78 => {
                flags |= CLIENT_NOSTARTSERVER as uint64_t;
            }
            113 => {}
            83 => {
                path = Some(CStr::from_ptr(BSDoptarg).to_owned());
            }
            84 => {
                tty_parse_features(
                    CStr::from_ptr(BSDoptarg),
                    c":,",
                    &raw mut feat,
                    ::core::ptr::null_mut::<::core::ffi::c_int>(),
                );
            }
            117 => {
                flags |= CLIENT_UTF8 as uint64_t;
            }
            118 => {
                log_add_level();
            }
            _ => {
                usage(1 as ::core::ffi::c_int);
            }
        }
    }
    cfg_set_files(config_paths);
    argc -= BSDoptind;
    argv = argv.offset(BSDoptind as isize);
    if shell_command.is_some() && argc != 0 as ::core::ffi::c_int {
        usage(1 as ::core::ffi::c_int);
    }
    if flags & CLIENT_NOFORK as uint64_t != 0 && argc != 0 as ::core::ffi::c_int {
        usage(1 as ::core::ffi::c_int);
    }
    ptm_fd = getptmfd();
    if ptm_fd == -(1 as ::core::ffi::c_int) {
        err(1 as ::core::ffi::c_int, c"getptmfd".as_ptr());
    }
    if 0 as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
        err(1 as ::core::ffi::c_int, c"pledge".as_ptr());
    }
    if !getenv(c"TMUX".as_ptr()).is_null() {
        flags |= CLIENT_UTF8 as uint64_t;
    } else {
        s = getenv(c"LC_ALL".as_ptr());
        if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 {
            s = getenv(c"LC_CTYPE".as_ptr());
        }
        if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 {
            s = getenv(c"LANG".as_ptr());
        }
        if s.is_null() || *s as ::core::ffi::c_int == '\0' as i32 {
            s = c"".as_ptr();
        }
        if !strcasestr(s, c"UTF-8".as_ptr()).is_null() || !strcasestr(s, c"UTF8".as_ptr()).is_null()
        {
            flags |= CLIENT_UTF8 as uint64_t;
        }
    }
    for oe in options_table
        .iter()
        .take_while(|entry| entry.name.is_some())
    {
        if oe.scope & OPTIONS_TABLE_SERVER != 0 {
            OptionsScope::GlobalServer.reset_default(oe);
        }
        if oe.scope & OPTIONS_TABLE_SESSION != 0 {
            OptionsScope::GlobalSession.reset_default(oe);
        }
        if oe.scope & OPTIONS_TABLE_WINDOW != 0 {
            OptionsScope::GlobalWindow.reset_default(oe);
        }
    }
    OptionsScope::GlobalSession.with_local(|oo| {
        options_set_string(oo, c"default-shell", 0 as ::core::ffi::c_int, |out| {
            out.write_all(getshell().as_bytes())
        });
    });
    s = getenv(c"VISUAL".as_ptr());
    if !s.is_null() || {
        s = getenv(c"EDITOR".as_ptr());
        !s.is_null()
    } {
        OptionsScope::GlobalServer.with_local(|oo| {
            options_set_string(oo, c"editor", 0 as ::core::ffi::c_int, |out| {
                write_cstr(out, nullable_cstr(s))
            });
        });
        if !strrchr(s, '/' as i32).is_null() {
            s = strrchr(s, '/' as i32).offset(1 as ::core::ffi::c_int as isize);
        }
        if !strstr(s, c"vi".as_ptr()).is_null() {
            keys = MODEKEY_VI;
        } else {
            keys = MODEKEY_EMACS;
        }
        OptionsScope::GlobalSession.with_local(|oo| {
            options_set_number(oo, c"status-keys", keys as ::core::ffi::c_longlong);
        });
        OptionsScope::GlobalWindow.with_local(|oo| {
            options_set_number(oo, c"mode-keys", keys as ::core::ffi::c_longlong);
        });
    }
    if path.is_none() && label.is_none() {
        s = getenv(c"TMUX".as_ptr());
        if !s.is_null()
            && *s as ::core::ffi::c_int != '\0' as i32
            && *s as ::core::ffi::c_int != ',' as i32
        {
            let tmux = CStr::from_ptr(s).to_bytes();
            let end = tmux
                .iter()
                .position(|byte| *byte == b',')
                .unwrap_or(tmux.len());
            path = Some(CString::new(&tmux[..end]).expect("TMUX socket path contains no NUL"));
        }
    }
    if path.is_none() {
        path = Some(match make_label(label) {
            Ok(path) => path,
            Err(cause) => {
                fprintf(stderr, c"%s\n".as_ptr(), cause.as_ptr());
                drop(cause);
                exit(1 as ::core::ffi::c_int);
            }
        });
        flags |= CLIENT_DEFAULTSOCKET as uint64_t;
    }
    socket_path = Some(path.expect("socket path was selected"));
    let command_argv: Vec<CString> = (0..argc)
        .map(|i| CStr::from_ptr(*argv.add(i as usize)).to_owned())
        .collect();
    exit(client_main(&command_argv, flags, feat));
}
pub const TMUX_CONF: &std::ffi::CStr =
    c"/etc/tmux.conf:~/.tmux.conf:$XDG_CONFIG_HOME/tmux/tmux.conf:~/.config/tmux/tmux.conf";
pub fn main() {
    let args_strings: Vec<CString> = ::std::env::args_os()
        .map(|arg| {
            CString::new(::std::os::unix::ffi::OsStrExt::as_bytes(arg.as_os_str()))
                .expect("Failed to convert argument into CString.")
        })
        .collect();
    unsafe { ::std::process::exit(main_0(&args_strings) as i32) }
}
