//! Synchronous Unix descriptor setup and control.
//!
//! These operations do not expose readiness. Bootstrap operations can be used
//! before creating a runtime (in particular, before daemonization). Byte I/O
//! belongs to AsyncRead/AsyncWrite; creating/opening files can still block.
use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

mod notification;
pub mod regex;
pub use notification::{Notification, Notify};

fn check(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

/// Set O_NONBLOCK, preserving other status flags; return its previous value.
/// Status flags are shared with duplicates of this open file description.
pub fn set_nonblocking(fd: BorrowedFd<'_>, enabled: bool) -> io::Result<bool> {
    let flags = check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) })?;
    let next = if enabled {
        flags | libc::O_NONBLOCK
    } else {
        flags & !libc::O_NONBLOCK
    };
    check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, next) })?;
    Ok(flags & libc::O_NONBLOCK != 0)
}

/// Open a close-on-exec descriptor without registering it with a runtime.
pub fn open(path: &CStr, flags: libc::c_int, mode: libc::mode_t) -> io::Result<OwnedFd> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    let access = flags & libc::O_ACCMODE;
    let file = std::fs::OpenOptions::new()
        .read(access != libc::O_WRONLY)
        .write(access != libc::O_RDONLY)
        .custom_flags(flags & !libc::O_ACCMODE)
        .mode(mode.into())
        .open(std::ffi::OsStr::from_bytes(path.to_bytes()))?;
    Ok(file.into())
}

/// Mark every descriptor at or above `lowest` close-on-exec. Nothing is closed
/// before exec, so their owners stay valid if exec fails.
pub fn close_on_exec_from(lowest: RawFd) {
    let lowest = lowest.max(0);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let marked = unsafe {
            libc::syscall(
                libc::SYS_close_range,
                lowest as libc::c_uint,
                libc::c_uint::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            )
        };
        if marked == 0 {
            return;
        }
    }
    let end = unsafe { libc::getdtablesize() };
    for fd in lowest..end {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags >= 0 && flags & libc::FD_CLOEXEC == 0 {
            unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
        }
    }
}

/// The number of threads in the calling process.
fn thread_count() -> io::Result<usize> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        Ok(std::fs::read_dir("/proc/self/task")?.count())
    }
    #[cfg(target_vendor = "apple")]
    {
        let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let written = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTASKINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if written != size {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init() }.pti_threadnum as usize)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    {
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Fail unless the calling process has a single thread, as a fork requires:
/// a forked child keeps only the calling thread, and anything another thread
/// was doing would be left half done in the child.
pub fn ensure_single_threaded() -> io::Result<()> {
    match thread_count()? {
        1 => Ok(()),
        _ => Err(io::Error::other("process has more than one thread")),
    }
}

/// Fork the calling process: the child's pid in the parent, `None` in the
/// child. Fails without forking when the process has more than one thread.
pub fn fork() -> io::Result<Option<libc::pid_t>> {
    ensure_single_threaded()?;
    match check(unsafe { libc::fork() })? {
        0 => Ok(None),
        pid => Ok(Some(pid)),
    }
}

/// Detach from the controlling terminal as `daemon(1, 0)` does: keep the
/// working directory and redirect stdio to `/dev/null`. Forks, so fails when
/// the process has more than one thread.
#[cfg_attr(target_vendor = "apple", allow(deprecated))]
pub fn daemonize() -> io::Result<()> {
    ensure_single_threaded()?;
    check(unsafe { libc::daemon(1, 0) }).map(|_| ())
}

/// Send EOF to a socket peer while preserving the receiving half.
/// The caller must finish queued writes before invoking this operation.
pub fn shutdown_write(fd: BorrowedFd<'_>) -> io::Result<()> {
    check(unsafe { libc::shutdown(fd.as_raw_fd(), libc::SHUT_WR) }).map(|_| ())
}

/// Synchronous bootstrap connection, before a process may fork. The returned
/// socket is nonblocking and close-on-exec, ready to import into a runtime.
pub fn connect(path: &Path) -> io::Result<OwnedFd> {
    let socket = UnixStream::connect(path)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

/// Create a nonblocking listener with an explicit backlog. Path permissions and
/// stale-path removal are application policy; inherited listeners can be imported
/// directly through [`crate::mio::Listener::new`] instead.
pub fn listen(path: &Path, backlog: libc::c_int) -> io::Result<UnixListener> {
    let listener = UnixListener::bind(path)?;
    check(unsafe { libc::listen(listener.as_raw_fd(), backlog) })?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// A blocking, close-on-exec socket pair for pre-fork setup. The parent sets its
/// endpoint nonblocking before importing it, while child stdio stays blocking.
pub fn socket_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let (first, second) = UnixStream::pair()?;
    Ok((first.into(), second.into()))
}

/// Pathname of a bound Unix socket. Unnamed and abstract sockets have no path.
pub fn socket_path(fd: BorrowedFd<'_>) -> io::Result<Option<CString>> {
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of_val(&address) as libc::socklen_t;
    check(unsafe {
        libc::getsockname(
            fd.as_raw_fd(),
            (&mut address as *mut libc::sockaddr_un).cast(),
            &mut size,
        )
    })?;
    if address.sun_family as libc::c_int != libc::AF_UNIX {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let count = (size as usize)
        .saturating_sub(std::mem::offset_of!(libc::sockaddr_un, sun_path))
        .min(address.sun_path.len());
    if count == 0 || address.sun_path[0] == 0 {
        return Ok(None);
    }
    let bytes: Vec<_> = address.sun_path[..count]
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte as u8)
        .collect();
    Ok(Some(CString::new(bytes).expect("path excludes NUL")))
}

/// Number of kernel-buffered input bytes, without consuming input.
pub fn bytes_available(fd: BorrowedFd<'_>) -> io::Result<usize> {
    let mut count: libc::c_int = 0;
    check(unsafe { libc::ioctl(fd.as_raw_fd(), libc::FIONREAD, &mut count) })?;
    Ok(count.max(0) as usize)
}

pub fn terminal_attributes(fd: BorrowedFd<'_>) -> io::Result<libc::termios> {
    let mut attributes = std::mem::MaybeUninit::uninit();
    check(unsafe { libc::tcgetattr(fd.as_raw_fd(), attributes.as_mut_ptr()) })?;
    Ok(unsafe { attributes.assume_init() })
}

pub fn set_terminal_attributes(
    fd: BorrowedFd<'_>,
    action: libc::c_int,
    attributes: &libc::termios,
) -> io::Result<()> {
    check(unsafe { libc::tcsetattr(fd.as_raw_fd(), action, attributes) }).map(|_| ())
}

/// All-zero terminal attributes, the record C code clears before filling in.
pub fn empty_terminal_attributes() -> libc::termios {
    unsafe { std::mem::zeroed() }
}

/// Set `attributes` to raw mode as `cfmakeraw` does.
pub fn make_raw(attributes: &mut libc::termios) {
    unsafe { libc::cfmakeraw(attributes) };
}

/// Copy the input and output speeds of `source` into `target`.
pub fn copy_speeds(source: &libc::termios, target: &mut libc::termios) -> io::Result<()> {
    check(unsafe { libc::cfsetispeed(target, libc::cfgetispeed(source)) })?;
    check(unsafe { libc::cfsetospeed(target, libc::cfgetospeed(source)) }).map(|_| ())
}

/// Discard terminal input/output as selected by TCIFLUSH/TCOFLUSH/TCIOFLUSH.
/// Open a pseudo-terminal pair as `(master, slave)`, both close-on-exec.
pub fn open_pty(size: Option<&libc::winsize>) -> io::Result<(OwnedFd, OwnedFd)> {
    let (mut master, mut slave) = (-1, -1);
    let mut size = size.copied();
    check(unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            size.as_mut()
                .map_or(std::ptr::null_mut(), |size| size as *mut _),
        )
    })?;
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
    }
    Ok((master, slave))
}

/// Make `slave` the controlling terminal and standard streams of a new session
/// (`login_tty`), consuming it.
pub fn login_terminal(slave: OwnedFd) -> io::Result<()> {
    check(unsafe { libc::login_tty(slave.into_raw_fd()) }).map(|_| ())
}

pub fn discard_terminal(fd: BorrowedFd<'_>, queue: libc::c_int) -> io::Result<()> {
    check(unsafe { libc::tcflush(fd.as_raw_fd(), queue) }).map(|_| ())
}

pub fn terminal_size(fd: BorrowedFd<'_>) -> io::Result<libc::winsize> {
    let mut size = std::mem::MaybeUninit::uninit();
    check(unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCGWINSZ, size.as_mut_ptr()) })?;
    Ok(unsafe { size.assume_init() })
}

pub fn set_terminal_size(fd: BorrowedFd<'_>, size: &libc::winsize) -> io::Result<()> {
    check(unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCSWINSZ, size) }).map(|_| ())
}

pub fn terminal_session(fd: BorrowedFd<'_>) -> io::Result<libc::pid_t> {
    check(unsafe { libc::tcgetsid(fd.as_raw_fd()) })
}

pub fn terminal_foreground_group(fd: BorrowedFd<'_>) -> io::Result<libc::pid_t> {
    check(unsafe { libc::tcgetpgrp(fd.as_raw_fd()) })
}

/// Own the terminal name instead of exposing libc's shared ttyname storage.
pub fn terminal_name(fd: BorrowedFd<'_>) -> io::Result<CString> {
    let mut bytes = vec![0u8; 128];
    loop {
        let result =
            unsafe { libc::ttyname_r(fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
        if result == 0 {
            return CStr::from_bytes_until_nul(&bytes)
                .map(CStr::to_owned)
                .map_err(|_| io::ErrorKind::InvalidData.into());
        }
        if result != libc::ERANGE {
            return Err(io::Error::from_raw_os_error(result));
        }
        let size = bytes
            .len()
            .checked_mul(2)
            .ok_or(io::ErrorKind::OutOfMemory)?;
        bytes.resize(size, 0);
    }
}

/// Create and open a unique file from `template`, a NUL-terminated path
/// ending in `XXXXXX`, which is rewritten to the name created.
pub fn make_temporary(template: &mut [u8]) -> io::Result<OwnedFd> {
    if !template.ends_with(b"XXXXXX\0") {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    use std::hash::{BuildHasher, Hasher};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let suffix = template.len() - 7..template.len() - 1;
    let random = std::hash::RandomState::new();
    for attempt in 0u64..10_000 {
        let mut hasher = random.build_hasher();
        hasher.write_u64(attempt);
        let mut bits = hasher.finish();
        for byte in &mut template[suffix.clone()] {
            *byte = LETTERS[(bits % LETTERS.len() as u64) as usize];
            bits /= LETTERS.len() as u64;
        }
        let path = std::ffi::OsStr::from_bytes(&template[..template.len() - 1]);
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(file) => return Ok(file.into()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}

/// The host name, truncated to the 64 bytes tmux keeps.
pub fn host_name() -> io::Result<CString> {
    let mut bytes = [0u8; 65];
    check(unsafe { libc::gethostname(bytes.as_mut_ptr().cast(), bytes.len()) })?;
    CStr::from_bytes_until_nul(&bytes)
        .map(CStr::to_owned)
        .map_err(|_| io::ErrorKind::InvalidData.into())
}

/// Release what a standard stream (0, 1 or 2) refers to by pointing it at
/// `/dev/null`, so peers see EOF without the descriptor number being reused.
pub fn release_standard_stream(target: libc::c_int) -> io::Result<()> {
    if !(0..=2).contains(&target) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let null = open(c"/dev/null", libc::O_RDWR, 0)?;
    check(unsafe { libc::dup2(null.as_raw_fd(), target) }).map(|_| ())
}

/// Acquire/release an advisory file lock. Blocking lock modes may block the
/// calling thread and are intended for bootstrap, before the runtime is driven.
pub fn lock(fd: BorrowedFd<'_>, operation: libc::c_int) -> io::Result<()> {
    check(unsafe { libc::flock(fd.as_raw_fd(), operation) }).map(|_| ())
}

/// Effective user and group IDs reported by a connected Unix socket's peer.
pub fn peer_credentials(fd: BorrowedFd<'_>) -> io::Result<(libc::uid_t, libc::gid_t)> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let mut credentials = std::mem::MaybeUninit::<libc::ucred>::uninit();
        let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        check(unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                credentials.as_mut_ptr().cast(),
                &mut size,
            )
        })?;
        let credentials = unsafe { credentials.assume_init() };
        Ok((credentials.uid, credentials.gid))
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let (mut uid, mut gid) = (0, 0);
        check(unsafe { libc::getpeereid(fd.as_raw_fd(), &mut uid, &mut gid) })?;
        Ok((uid, gid))
    }
}

/// A thread signal mask saved by [`block_signals`].
pub struct SignalMask(libc::sigset_t);

/// Block every signal on the calling thread and return the previous mask.
pub fn block_signals() -> SignalMask {
    let mut set = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
    let mut old = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
    unsafe {
        libc::sigfillset(set.as_mut_ptr());
        libc::sigprocmask(libc::SIG_BLOCK, set.as_ptr(), old.as_mut_ptr());
        SignalMask(old.assume_init())
    }
}

/// Install `mask` as the calling thread's signal mask.
pub fn set_signal_mask(mask: &SignalMask) {
    unsafe { libc::sigprocmask(libc::SIG_SETMASK, &mask.0, std::ptr::null_mut()) };
}

/// A disposition that involves no Rust signal handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalDisposition {
    Default,
    Ignore,
}

/// Install `SIG_DFL` or `SIG_IGN` for `signal` with an empty mask and
/// `SA_RESTART`.
pub fn set_signal_disposition(
    signal: libc::c_int,
    disposition: SignalDisposition,
) -> io::Result<()> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    action.sa_flags = libc::SA_RESTART;
    action.sa_sigaction = match disposition {
        SignalDisposition::Default => libc::SIG_DFL,
        SignalDisposition::Ignore => libc::SIG_IGN,
    };
    check(unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) }).map(|_| ())
}

/// Whether `string` matches the shell wildcard `pattern` under `flags`
/// (`fnmatch`).
pub fn fnmatch(pattern: &CStr, string: &CStr, flags: libc::c_int) -> bool {
    unsafe { libc::fnmatch(pattern.as_ptr(), string.as_ptr(), flags) == 0 }
}

unsafe extern "C" {
    fn tzset();
}

/// Reload the time zone from `TZ` (`tzset`). It races with other threads
/// reading the time zone or environment, so it fails when the process has
/// more than one thread.
pub fn reset_timezone() -> io::Result<()> {
    ensure_single_threaded()?;
    unsafe { tzset() };
    Ok(())
}

/// The C library's description of the errno value `code` (`strerror_r`).
pub fn error_description(code: libc::c_int) -> Option<CString> {
    let mut text = [0 as libc::c_char; 128];
    if unsafe { libc::strerror_r(code, text.as_mut_ptr(), text.len()) } != 0 {
        return None;
    }
    Some(unsafe { CStr::from_ptr(text.as_ptr()) }.to_owned())
}

/// Set the calling thread's name, which the kernel cuts to 15 bytes
/// (`PR_SET_NAME`).
#[cfg(target_os = "linux")]
pub fn set_thread_name(name: &CStr) -> io::Result<()> {
    check(unsafe { libc::prctl(libc::PR_SET_NAME, name.as_ptr()) }).map(|_| ())
}

fn calendar_time(
    t: libc::time_t,
    convert: unsafe extern "C" fn(*const libc::time_t, *mut libc::tm) -> *mut libc::tm,
) -> Option<libc::tm> {
    let mut result = std::mem::MaybeUninit::uninit();
    let converted = unsafe { convert(&t, result.as_mut_ptr()) };
    (!converted.is_null()).then(|| unsafe { result.assume_init() })
}

/// Local calendar time (`localtime_r`), or `None` when `t` cannot be
/// represented.
pub fn local_time(t: libc::time_t) -> Option<libc::tm> {
    calendar_time(t, libc::localtime_r)
}

/// UTC calendar time (`gmtime_r`), or `None` when `t` cannot be represented.
pub fn utc_time(t: libc::time_t) -> Option<libc::tm> {
    calendar_time(t, libc::gmtime_r)
}

/// Format `tm` into `buf` (`strftime`); 0 when the result does not fit.
pub fn format_time(buf: &mut [u8], format: &CStr, tm: &libc::tm) -> usize {
    unsafe { libc::strftime(buf.as_mut_ptr().cast(), buf.len(), format.as_ptr(), tm) }
}

/// The paths matching `pattern` (`glob`), or glob's error status.
pub fn glob(pattern: &CStr) -> Result<Vec<CString>, libc::c_int> {
    let mut matches = std::mem::MaybeUninit::<libc::glob_t>::zeroed();
    let status = unsafe { libc::glob(pattern.as_ptr(), 0, None, matches.as_mut_ptr()) };
    let mut matches = unsafe { matches.assume_init() };
    let paths = (status == 0).then(|| {
        (0..matches.gl_pathc as usize)
            .map(|index| unsafe { CStr::from_ptr(*matches.gl_pathv.add(index)) }.to_owned())
            .collect()
    });
    unsafe { libc::globfree(&mut matches) };
    paths.ok_or(status)
}

/// The C library's description of `signal`.
pub fn signal_description(signal: libc::c_int) -> CString {
    let description = unsafe { libc::strsignal(signal) };
    if description.is_null() {
        return CString::default();
    }
    unsafe { CStr::from_ptr(description) }.to_owned()
}

/// Send `signal` to the process `pid` (`kill`).
pub fn send_signal(pid: libc::pid_t, signal: libc::c_int) -> io::Result<()> {
    check(unsafe { libc::kill(pid, signal) }).map(|_| ())
}

/// Send `signal` to the process group `group` (`killpg`).
pub fn send_group_signal(group: libc::pid_t, signal: libc::c_int) -> io::Result<()> {
    check(unsafe { libc::killpg(group, signal) }).map(|_| ())
}

/// Reap a child selected by `pid` as `waitpid` does. `None` means `WNOHANG`
/// found no child with a status to report.
pub fn wait_child(
    pid: libc::pid_t,
    options: libc::c_int,
) -> io::Result<Option<(libc::pid_t, std::process::ExitStatus)>> {
    use std::os::unix::process::ExitStatusExt as _;
    let mut status = 0;
    match check(unsafe { libc::waitpid(pid, &mut status, options) })? {
        0 => Ok(None),
        pid => Ok(Some((pid, std::process::ExitStatus::from_raw(status)))),
    }
}

/// Set the file mode creation mask and return the previous one.
pub fn set_umask(mask: libc::mode_t) -> libc::mode_t {
    unsafe { libc::umask(mask) }
}

/// Return free heap memory to the system where the allocator supports it.
pub fn trim_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Run `command` with `/bin/sh -c` and wait for it, as `system` does.
pub fn system(command: &CStr) -> libc::c_int {
    unsafe { libc::system(command.as_ptr()) }
}

/// Whether the calling process may access `path` with `mode` (`access`).
pub fn accessible(path: &CStr, mode: libc::c_int) -> bool {
    unsafe { libc::access(path.as_ptr(), mode) == 0 }
}

/// The time of `clock` (`clock_gettime`).
pub fn clock_time(clock: libc::clockid_t) -> io::Result<std::time::Duration> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    check(unsafe { libc::clock_gettime(clock, &mut time) })?;
    Ok(std::time::Duration::new(
        time.tv_sec as u64,
        time.tv_nsec as u32,
    ))
}

/// The real user ID of the calling process.
pub fn user_id() -> libc::uid_t {
    unsafe { libc::getuid() }
}

fn owned_field(field: *const libc::c_char) -> CString {
    if field.is_null() {
        return CString::default();
    }
    unsafe { CStr::from_ptr(field) }.to_owned()
}

/// Run a reentrant `get*_r` lookup, growing its scratch buffer on `ERANGE`, and
/// convert the record while the strings it points to are alive.
fn lookup<T, R>(
    mut call: impl FnMut(*mut T, *mut libc::c_char, usize, *mut *mut T) -> libc::c_int,
    convert: impl FnOnce(&T) -> R,
) -> Option<R> {
    let mut buffer = vec![0 as libc::c_char; 1024];
    loop {
        let mut record = std::mem::MaybeUninit::<T>::uninit();
        let mut result = std::ptr::null_mut();
        match call(
            record.as_mut_ptr(),
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        ) {
            0 if result.is_null() => return None,
            0 => return Some(convert(unsafe { record.assume_init_ref() })),
            libc::ERANGE if buffer.len() < 1 << 20 => {
                let size = buffer.len() * 2;
                buffer.resize(size, 0);
            }
            _ => return None,
        }
    }
}

fn user_by_id<R>(uid: libc::uid_t, read: impl FnOnce(&libc::passwd) -> R) -> Option<R> {
    lookup(
        |record, buffer, size, result| unsafe {
            libc::getpwuid_r(uid, record, buffer, size, result)
        },
        read,
    )
}

fn user_by_name<R>(name: &CStr, read: impl FnOnce(&libc::passwd) -> R) -> Option<R> {
    lookup(
        |record, buffer, size, result| unsafe {
            libc::getpwnam_r(name.as_ptr(), record, buffer, size, result)
        },
        read,
    )
}

fn group_by_id<R>(gid: libc::gid_t, read: impl FnOnce(&libc::group) -> R) -> Option<R> {
    lookup(
        |record, buffer, size, result| unsafe {
            libc::getgrgid_r(gid, record, buffer, size, result)
        },
        read,
    )
}

fn group_by_name<R>(name: &CStr, read: impl FnOnce(&libc::group) -> R) -> Option<R> {
    lookup(
        |record, buffer, size, result| unsafe {
            libc::getgrnam_r(name.as_ptr(), record, buffer, size, result)
        },
        read,
    )
}

/// The login name of user `uid`.
pub fn user_name(uid: libc::uid_t) -> Option<CString> {
    user_by_id(uid, |pw| owned_field(pw.pw_name))
}

/// The home directory of user `uid`.
pub fn user_home(uid: libc::uid_t) -> Option<CString> {
    user_by_id(uid, |pw| owned_field(pw.pw_dir))
}

/// The login shell of user `uid`.
pub fn user_shell(uid: libc::uid_t) -> Option<CString> {
    user_by_id(uid, |pw| owned_field(pw.pw_shell))
}

/// The home directory of the user called `name`.
pub fn user_home_by_name(name: &CStr) -> Option<CString> {
    user_by_name(name, |pw| owned_field(pw.pw_dir))
}

/// The ID of the user called `name`.
pub fn user_id_by_name(name: &CStr) -> Option<libc::uid_t> {
    user_by_name(name, |pw| pw.pw_uid)
}

/// The name of group `gid`.
pub fn group_name(gid: libc::gid_t) -> Option<CString> {
    group_by_id(gid, |gr| owned_field(gr.gr_name))
}

/// The ID of the group called `name`.
pub fn group_id_by_name(name: &CStr) -> Option<libc::gid_t> {
    group_by_name(name, |gr| gr.gr_gid)
}

/// Set the locale for `category` (`setlocale`). Changing the locale races with
/// other threads using it, so this fails when the process has more than one
/// thread.
pub fn set_locale(category: libc::c_int, locale: &CStr) -> io::Result<()> {
    ensure_single_threaded()?;
    if unsafe { libc::setlocale(category, locale.as_ptr()) }.is_null() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

/// The character encoding name of the current locale (`nl_langinfo(CODESET)`).
pub fn locale_codeset() -> CString {
    unsafe { CStr::from_ptr(libc::nl_langinfo(libc::CODESET)) }.to_owned()
}

/// The kernel's `uname` identification.
pub fn uname() -> io::Result<libc::utsname> {
    let mut name = std::mem::MaybeUninit::uninit();
    check(unsafe { libc::uname(name.as_mut_ptr()) })?;
    Ok(unsafe { name.assume_init() })
}

/// A program to replace the process image with.
#[derive(Clone, Copy)]
pub enum Program<'a> {
    /// Run the file at `path` with `argv`.
    Path(&'a CStr, &'a [&'a CStr]),
    /// Run `argv[0]`, searched in `PATH` when it has no slash.
    Search(&'a [&'a CStr]),
}

/// Replace the process image. Returns only on failure.
pub fn exec(program: Program<'_>) -> io::Error {
    let argv = match program {
        Program::Path(_, argv) | Program::Search(argv) => argv,
    };
    if argv.is_empty() {
        return io::ErrorKind::InvalidInput.into();
    }
    let mut pointers: Vec<*const libc::c_char> = argv.iter().map(|arg| arg.as_ptr()).collect();
    pointers.push(std::ptr::null());
    match program {
        Program::Path(path, _) => unsafe { libc::execv(path.as_ptr(), pointers.as_ptr()) },
        Program::Search(_) => unsafe { libc::execvp(pointers[0], pointers.as_ptr()) },
    };
    io::Error::last_os_error()
}

/// Point stdin, stdout and stderr at `stdio` in order and replace the process
/// image; exit with status 1 if either fails. Nothing runs between replacing
/// the standard streams and exec, so no owner of a replaced descriptor can
/// observe it.
pub fn exec_with_stdio(stdio: [BorrowedFd<'_>; 3], program: Program<'_>) -> ! {
    for (target, fd) in stdio.iter().enumerate() {
        let target = target as libc::c_int;
        let result = if fd.as_raw_fd() == target {
            let flags = unsafe { libc::fcntl(target, libc::F_GETFD) };
            if flags < 0 {
                -1
            } else {
                unsafe { libc::fcntl(target, libc::F_SETFD, flags & !libc::FD_CLOEXEC) }
            }
        } else {
            unsafe { libc::dup2(fd.as_raw_fd(), target) }
        };
        if result < 0 {
            exit_now(1);
        }
    }
    exec(program);
    exit_now(1)
}

/// Terminate the process without running exit handlers or flushing stdio.
pub fn exit_now(status: libc::c_int) -> ! {
    unsafe { libc::_exit(status) }
}

/// The open file description's status flags (`F_GETFL`).
pub fn status_flags(fd: BorrowedFd<'_>) -> io::Result<libc::c_int> {
    check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) })
}

/// Whether `fd` names an open descriptor in this process.
pub fn is_open(fd: RawFd) -> bool {
    unsafe { libc::fcntl(fd, libc::F_GETFD) >= 0 }
}

/// Move the calling process into a new process group led by itself.
pub fn lead_process_group() -> io::Result<()> {
    check(unsafe { libc::setpgid(0, 0) }).map(|_| ())
}
