#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_assignments,
    unused_mut
)]
use crate::cfg::{cfg_report_missing, cfg_set_files};
use crate::client::client_main;
use crate::compat::fdforkpty::getptmfd;
use crate::compat::getopt::Getopt;
use crate::compat::getprogname::getprogname;
use crate::environ::{environ_create, environ_find, environ_put, environ_set, process_environment};
use crate::ffi::libc::tzset;
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::log::{log_add_level, log_cstr, log_debug};
use crate::options::options_table;
use crate::options::{options_set_number, options_set_string, OptionsScope};
use crate::options::{OPTIONS_TABLE_SERVER, OPTIONS_TABLE_SESSION, OPTIONS_TABLE_WINDOW};
use crate::shared::abi::uid_t;
use crate::shared::abi::*;
use crate::shared::account::passwd;
use crate::shared::client::{
    CLIENT_CONTROL, CLIENT_CONTROLCONTROL, CLIENT_DEFAULTSOCKET, CLIENT_LOGIN, CLIENT_NOFORK,
    CLIENT_NOSTARTSERVER, CLIENT_UTF8,
};
use crate::shared::environment::{environ, environ_entry};
use crate::shared::errno::ErrorText;
use crate::shared::key::{MODEKEY_EMACS, MODEKEY_VI};
use crate::shared::posix_io::X_OK;
use crate::shared::time::{timespec, CLOCK_REALTIME};
use crate::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::text::search::contains;
use crate::text::utf8::{utf8_isvalid, utf8_stravis_cstring};
use crate::tty::tty_parse_features;
use ::libc::nl_item;
use libc::{access, clock_gettime, exit, getenv, getpwuid, getuid, nl_langinfo, setlocale};
use std::ffi::{CStr, CString, OsStr};
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub use ::libc::{CLOCK_MONOTONIC, CODESET, LC_CTYPE, LC_TIME};

pub const TMUX_SOCK_PERM: ::core::ffi::c_int = 7 as ::core::ffi::c_int;
thread_local! {
    static SHELL_COMMAND: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
    static GLOBAL_ENVIRON: std::cell::RefCell<Option<Box<environ>>> = const {
        std::cell::RefCell::new(None)
    };
    static START_TIME: std::cell::Cell<SystemTime> = const { std::cell::Cell::new(UNIX_EPOCH) };
    static PTM_FD: std::cell::Cell<::core::ffi::c_int> = const { std::cell::Cell::new(-1) };
    static SOCKET_PATH: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
}
/// The socket path chosen at startup (or by systemd activation).
pub fn socket_path_cstr() -> CString {
    SOCKET_PATH
        .with_borrow(|path| path.clone())
        .expect("socket path is set")
}
pub(crate) fn set_socket_path(path: CString) {
    SOCKET_PATH.set(Some(path));
}
/// The command given with -c, if any.
pub fn shell_command() -> Option<CString> {
    SHELL_COMMAND.with_borrow(Clone::clone)
}
/// Install a global environment, returning the one it replaces.
pub fn replace_global_environ(environment: Option<Box<environ>>) -> Option<Box<environ>> {
    GLOBAL_ENVIRON.replace(environment)
}
/// Read the global environment; `read` must not reach it again.
pub fn global_environ<R>(read: impl FnOnce(&environ) -> R) -> R {
    GLOBAL_ENVIRON.with_borrow(|environment| read(environment.as_deref().expect("environment")))
}
/// Edit the global environment; `edit` must not reach it again.
pub fn global_environ_mut<R>(edit: impl FnOnce(&mut environ) -> R) -> R {
    GLOBAL_ENVIRON
        .with_borrow_mut(|environment| edit(environment.as_deref_mut().expect("environment")))
}
/// When the server started.
pub fn start_time() -> SystemTime {
    START_TIME.get()
}
pub(crate) fn set_start_time(time: SystemTime) {
    START_TIME.set(time);
}
/// The descriptor fdforkpty opens new terminals through, or -1.
pub fn ptm_fd() -> ::core::ffi::c_int {
    PTM_FD.get()
}
fn usage(status: i32) -> ! {
    let message = [
        b"usage: ".as_slice(),
        getprogname().to_bytes(),
        b" [-2CDhlNuVv] [-c shell-command] [-f file] [-L socket-name]\n            [-S socket-path] [-T features] [command [flags]]\n",
    ]
    .concat();
    let _ = if status != 0 {
        io::stderr().write_all(&message)
    } else {
        io::stdout().write_all(&message)
    };
    std::process::exit(status);
}

/// Print `progname: message` to stderr and exit, as errx(3) does.
fn die(status: i32, message: fmt::Arguments<'_>) -> ! {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(getprogname().to_bytes());
    let _ = writeln!(stderr, ": {message}");
    std::process::exit(status);
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
    // SAFETY: access only reads the NUL-terminated path.
    if unsafe { access(shell.as_ptr(), X_OK) } != 0 as ::core::ffi::c_int {
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
fn expand_path(path: &CStr, home: Option<&CStr>) -> Option<CString> {
    let mut value: Option<environ_entry> = None;
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
        value = global_environ(|environment| environ_find(environment, &name).cloned());
        let value = value?;
        let mut expanded = if value.value.is_none() {
            b"(null)".to_vec()
        } else {
            (value.value)
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
fn cstr_path(path: &CStr) -> &Path {
    Path::new(OsStr::from_bytes(path.to_bytes()))
}
fn path_cstring(path: PathBuf) -> CString {
    CString::new(path.into_os_string().into_vec()).expect("a Unix path contains no NUL")
}
fn expand_paths(s: &CStr, no_realpath: bool) -> Vec<CString> {
    let home = find_home_cstr();
    let mut paths = Vec::new();
    for next in s.to_bytes().split(|&b| b == b':') {
        let next = CString::new(next).expect("a path from a C string contains no NUL");
        if let Some(expanded) = expand_path(&next, home) {
            let path = if no_realpath {
                Some(expanded)
            } else {
                let path = match fs::canonicalize(cstr_path(&expanded)) {
                    Ok(resolved) => Some(path_cstring(resolved)),
                    Err(error) => {
                        log_debug(format_args!(
                            "{}: realpath(\"{}\") failed: {}",
                            "expand_paths",
                            log_cstr(&expanded),
                            ErrorText(&error)
                        ));
                        None
                    }
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
    let mkdir_error = fs::DirBuilder::new()
        .mode(0o700)
        .create(cstr_path(&base))
        .err()
        .filter(|error| error.kind() != io::ErrorKind::AlreadyExists);
    if let Some(error) = &mkdir_error {
        let error = ErrorText(error).to_string();
        return Err(make_label_cause(&[
            b"couldn't create directory ",
            base.to_bytes(),
            b" (",
            error.as_bytes(),
            b")",
        ]));
    }
    let sb = match fs::symlink_metadata(cstr_path(&base)) {
        Ok(sb) => sb,
        Err(error) => {
            let error = ErrorText(&error).to_string();
            return Err(make_label_cause(&[
                b"couldn't read directory ",
                base.to_bytes(),
                b" (",
                error.as_bytes(),
                b")",
            ]));
        }
    };
    if !sb.file_type().is_dir() {
        return Err(make_label_cause(&[base.to_bytes(), b" is not a directory"]));
    }
    if sb.uid() != uid || sb.mode() & TMUX_SOCK_PERM as u32 != 0 {
        return Err(make_label_cause(&[
            b"directory ",
            base.to_bytes(),
            b" has unsafe permissions",
        ]));
    }
    let mut path = base.into_bytes();
    path.push(b'/');
    path.extend_from_slice(label.unwrap_or(c"default").to_bytes());
    Ok(CString::new(path).expect("socket label path contains no NUL"))
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
pub fn setblocking(fd: std::os::fd::BorrowedFd<'_>, state: ::core::ffi::c_int) {
    let _ = tiri_rt::unix::set_nonblocking(fd, state == 0);
}
pub fn get_timer() -> uint64_t {
    let mut ts: timespec = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        if clock_gettime(CLOCK_MONOTONIC, &raw mut ts) != 0 as ::core::ffi::c_int {
            clock_gettime(CLOCK_REALTIME, &raw mut ts);
        }
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
    // tmux reads the directory into a PATH_MAX buffer, so a longer one is
    // not found at all.
    let cwd = std::env::current_dir()
        .ok()
        .filter(|cwd| cwd.as_os_str().len() < libc::PATH_MAX as usize)?;
    let Some(pwd) = nullable_cstr(getenv(c"PWD".as_ptr())) else {
        return Some(path_cstring(cwd));
    };
    let same = !pwd.is_empty()
        && matches!(
            (fs::canonicalize(cstr_path(pwd)), fs::canonicalize(&cwd)),
            (Ok(resolved1), Ok(resolved2)) if resolved1 == resolved2
        );
    if !same {
        return Some(path_cstring(cwd));
    }
    Some(pwd.to_owned())
}
/// Return the cached home directory as a borrowed C string. A missing result
/// is retried on the next call, matching the old null-sentinel cache behavior.
pub(crate) fn find_home_cstr() -> Option<&'static CStr> {
    static HOME: OnceLock<CString> = OnceLock::new();
    if let Some(home) = HOME.get() {
        return Some(home.as_c_str());
    }
    let owned = unsafe {
        let mut home = getenv(c"HOME".as_ptr());
        if home.is_null() || *home as ::core::ffi::c_int == '\0' as i32 {
            let pw: *mut passwd = getpwuid(getuid());
            if pw.is_null() {
                return None;
            }
            home = (*pw).pw_dir;
        }
        CStr::from_ptr(home).to_owned()
    };
    let _ = HOME.set(owned);
    HOME.get().map(CString::as_c_str)
}

pub fn getversion() -> &'static CStr {
    c"3.8"
}
unsafe fn main_0(args: &[CString]) -> ::core::ffi::c_int {
    let mut path: Option<CString> = None;
    let mut label: Option<&CStr> = None;
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut keys: ::core::ffi::c_int = 0;
    let mut feat: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut fflag: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut flags: uint64_t = 0 as uint64_t;
    if setlocale(LC_CTYPE, c"en_US.UTF-8".as_ptr()).is_null()
        && setlocale(LC_CTYPE, c"C.UTF-8".as_ptr()).is_null()
    {
        if setlocale(LC_CTYPE, c"".as_ptr()).is_null() {
            die(1, format_args!("invalid LC_ALL, LC_CTYPE or LANG"));
        }
        s = nl_langinfo(CODESET as ::core::ffi::c_int as nl_item);
        let codeset = CStr::from_ptr(s).to_bytes();
        if !codeset.eq_ignore_ascii_case(b"UTF-8") && !codeset.eq_ignore_ascii_case(b"UTF8") {
            die(
                1,
                format_args!(
                    "need UTF-8 locale (LC_CTYPE) but have {}",
                    CStr::from_ptr(s).to_string_lossy()
                ),
            );
        }
    }
    setlocale(LC_TIME, c"".as_ptr());
    tzset();
    if args.first().and_then(|arg| arg.as_bytes().first()) == Some(&b'-') {
        flags = CLIENT_LOGIN as uint64_t;
    }
    drop(replace_global_environ(Some(environ_create())));
    global_environ_mut(|environment| {
        for var in process_environment() {
            environ_put(environment, &var, 0 as ::core::ffi::c_int);
        }
    });
    if let Some(cwd) = find_cwd() {
        global_environ_mut(|environment| {
            environ_set(environment, c"PWD", 0 as ::core::ffi::c_int, |out| {
                out.write_all(cwd.as_bytes())
            })
        });
    }
    let mut config_paths = expand_paths(TMUX_CONF, true);
    let mut getopt = Getopt::new(args, b"2c:CDdf:hlL:NqS:T:uUvV");
    for opt in getopt.by_ref() {
        let opt = match opt {
            Ok(opt) => opt,
            Err(error) => {
                let _ = io::stderr().write_all(
                    &[getprogname().to_bytes(), format!(": {error}\n").as_bytes()].concat(),
                );
                usage(1);
            }
        };
        let optarg = || opt.arg.expect("getopt spec gives this option an argument");
        match opt.flag {
            b'2' => {
                tty_parse_features(c"256", c":,", &mut feat, None);
            }
            b'c' => {
                SHELL_COMMAND.set(Some(optarg().to_owned()));
            }
            b'D' => {
                flags |= CLIENT_NOFORK as uint64_t;
            }
            b'C' => {
                if flags & CLIENT_CONTROL as uint64_t != 0 {
                    flags |= CLIENT_CONTROLCONTROL as uint64_t;
                } else {
                    flags |= CLIENT_CONTROL as uint64_t;
                }
            }
            b'f' => {
                if fflag == 0 {
                    fflag = 1 as ::core::ffi::c_int;
                    config_paths.clear();
                }
                config_paths.push(optarg().to_owned());
                cfg_report_missing();
            }
            b'h' => {
                usage(0);
            }
            b'V' => {
                let _ =
                    io::stdout().write_all(&[b"tmux ", getversion().to_bytes(), b"\n"].concat());
                std::process::exit(0);
            }
            b'l' => {
                flags |= CLIENT_LOGIN as uint64_t;
            }
            b'L' => {
                label = Some(optarg());
            }
            b'N' => {
                flags |= CLIENT_NOSTARTSERVER as uint64_t;
            }
            b'q' => {}
            b'S' => {
                path = Some(optarg().to_owned());
            }
            b'T' => {
                tty_parse_features(optarg(), c":,", &mut feat, None);
            }
            b'u' => {
                flags |= CLIENT_UTF8 as uint64_t;
            }
            b'v' => {
                log_add_level();
            }
            _ => {
                usage(1);
            }
        }
    }
    cfg_set_files(config_paths);
    let command_argv = getopt.remaining().to_vec();
    if shell_command().is_some() && !command_argv.is_empty() {
        usage(1);
    }
    if flags & CLIENT_NOFORK as uint64_t != 0 && !command_argv.is_empty() {
        usage(1);
    }
    PTM_FD.set(getptmfd());
    if ptm_fd() == -(1 as ::core::ffi::c_int) {
        let error = io::Error::last_os_error();
        die(1, format_args!("getptmfd: {}", ErrorText(&error)));
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
        let lang = CStr::from_ptr(s).to_bytes();
        if contains(lang, b"UTF-8", true) || contains(lang, b"UTF8", true) {
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
        let editor = CStr::from_ptr(s).to_bytes();
        let base = editor.rsplit(|&byte| byte == b'/').next().unwrap_or(editor);
        if contains(base, b"vi", false) {
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
                let _ = io::stderr().write_all(&[cause.as_bytes(), b"\n"].concat());
                drop(cause);
                exit(1 as ::core::ffi::c_int);
            }
        });
        flags |= CLIENT_DEFAULTSOCKET as uint64_t;
    }
    set_socket_path(path.expect("socket path was selected"));
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
