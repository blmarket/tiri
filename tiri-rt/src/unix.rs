//! Synchronous Unix descriptor setup and control.
//!
//! These operations do not expose readiness. Bootstrap operations can be used
//! before creating a runtime (in particular, before daemonization). Byte I/O
//! belongs to AsyncRead/AsyncWrite; creating/opening files can still block.
use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

mod notification;
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
    // SAFETY: fd is borrowed live; these calls query/change status flags only.
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
    // SAFETY: path is terminated, and mode is supplied even if not needed.
    let raw = check(unsafe {
        libc::open(
            path.as_ptr(),
            flags | libc::O_CLOEXEC,
            libc::c_uint::from(mode),
        )
    })?;
    // SAFETY: open transferred ownership of a new descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// Report close errors without retrying a descriptor number that may have been
/// released. Accepts inherited raw descriptors as well as released OwnedFds.
///
/// # Safety
/// Cancel descriptor I/O first and relinquish any Rust owner with into_raw_fd.
/// No observer may use or close the descriptor after this call, even on error.
pub unsafe fn close(fd: RawFd) -> io::Result<()> {
    // SAFETY: the caller relinquishes ownership exactly once.
    check(unsafe { libc::close(fd) }).map(|_| ())
}

/// Close inherited descriptors before exec without allocating or running drops.
///
/// # Safety
/// No object may subsequently use or drop a descriptor at or above `lowest`.
/// A post-fork caller must proceed to exec or _exit without unwinding.
pub unsafe fn close_from(lowest: RawFd) {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn closefrom(lowest: libc::c_int);
        }
        // SAFETY: the caller relinquishes all affected descriptors.
        unsafe { closefrom(lowest) };
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        // SAFETY: these libc calls need no allocation; the caller relinquishes
        // the descriptors. This also covers platforms without close_range.
        let end = unsafe { libc::getdtablesize() };
        for fd in lowest.max(0)..end {
            let _ = unsafe { close(fd) };
        }
    }
}

/// Send EOF to a socket peer while preserving the receiving half.
/// The caller must finish queued writes before invoking this operation.
pub fn shutdown_write(fd: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: shutdown operates on a live borrowed descriptor.
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
    // SAFETY: listener owns a valid listening socket.
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
    // SAFETY: the output buffer has the advertised capacity.
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
    // SAFETY: FIONREAD initializes count on success.
    check(unsafe { libc::ioctl(fd.as_raw_fd(), libc::FIONREAD, &mut count) })?;
    Ok(count.max(0) as usize)
}

pub fn terminal_attributes(fd: BorrowedFd<'_>) -> io::Result<libc::termios> {
    let mut attributes = std::mem::MaybeUninit::uninit();
    // SAFETY: tcgetattr initializes the output on success.
    check(unsafe { libc::tcgetattr(fd.as_raw_fd(), attributes.as_mut_ptr()) })?;
    Ok(unsafe { attributes.assume_init() })
}

pub fn set_terminal_attributes(
    fd: BorrowedFd<'_>,
    action: libc::c_int,
    attributes: &libc::termios,
) -> io::Result<()> {
    // SAFETY: attributes is a valid borrowed termios record.
    check(unsafe { libc::tcsetattr(fd.as_raw_fd(), action, attributes) }).map(|_| ())
}

/// Discard terminal input/output as selected by TCIFLUSH/TCOFLUSH/TCIOFLUSH.
pub fn discard_terminal(fd: BorrowedFd<'_>, queue: libc::c_int) -> io::Result<()> {
    // SAFETY: tcflush takes a descriptor and a checked-by-kernel selector.
    check(unsafe { libc::tcflush(fd.as_raw_fd(), queue) }).map(|_| ())
}

pub fn terminal_size(fd: BorrowedFd<'_>) -> io::Result<libc::winsize> {
    let mut size = std::mem::MaybeUninit::uninit();
    // SAFETY: TIOCGWINSZ initializes the output on success.
    check(unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCGWINSZ, size.as_mut_ptr()) })?;
    Ok(unsafe { size.assume_init() })
}

pub fn set_terminal_size(fd: BorrowedFd<'_>, size: &libc::winsize) -> io::Result<()> {
    // SAFETY: TIOCSWINSZ reads a valid winsize record.
    check(unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCSWINSZ, size) }).map(|_| ())
}

pub fn terminal_session(fd: BorrowedFd<'_>) -> io::Result<libc::pid_t> {
    // SAFETY: tcgetsid queries a live descriptor.
    check(unsafe { libc::tcgetsid(fd.as_raw_fd()) })
}

pub fn terminal_foreground_group(fd: BorrowedFd<'_>) -> io::Result<libc::pid_t> {
    // SAFETY: tcgetpgrp queries a live descriptor.
    check(unsafe { libc::tcgetpgrp(fd.as_raw_fd()) })
}

/// Own the terminal name instead of exposing libc's shared ttyname storage.
pub fn terminal_name(fd: BorrowedFd<'_>) -> io::Result<CString> {
    let mut bytes = vec![0u8; 128];
    loop {
        // SAFETY: the output buffer is writable for the advertised length.
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

/// Duplicate a live descriptor onto a stdio number in a pre-exec child.
///
/// # Safety
/// The target must not have a Rust owner that could subsequently close it.
pub unsafe fn redirect(fd: BorrowedFd<'_>, target: libc::c_int) -> io::Result<()> {
    if fd.as_raw_fd() == target {
        // dup2(fd, fd) does not clear close-on-exec; stdio must survive exec.
        let flags = check(unsafe { libc::fcntl(target, libc::F_GETFD) })?;
        return check(unsafe { libc::fcntl(target, libc::F_SETFD, flags & !libc::FD_CLOEXEC) })
            .map(|_| ());
    }
    // SAFETY: the caller controls the lifetime of the replaced target.
    check(unsafe { libc::dup2(fd.as_raw_fd(), target) }).map(|_| ())
}

/// Acquire/release an advisory file lock. Blocking lock modes may block the
/// calling thread and are intended for bootstrap, before the runtime is driven.
pub fn lock(fd: BorrowedFd<'_>, operation: libc::c_int) -> io::Result<()> {
    // SAFETY: flock operates on a live borrowed descriptor.
    check(unsafe { libc::flock(fd.as_raw_fd(), operation) }).map(|_| ())
}

/// Effective user and group IDs reported by a connected Unix socket's peer.
pub fn peer_credentials(fd: BorrowedFd<'_>) -> io::Result<(libc::uid_t, libc::gid_t)> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let mut credentials = std::mem::MaybeUninit::<libc::ucred>::uninit();
        let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: the output buffer and length have the required size.
        check(unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                credentials.as_mut_ptr().cast(),
                &mut size,
            )
        })?;
        // SAFETY: the kernel initialized this ucred on success.
        let credentials = unsafe { credentials.assume_init() };
        Ok((credentials.uid, credentials.gid))
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: both output pointers refer to valid initialized storage.
        check(unsafe { libc::getpeereid(fd.as_raw_fd(), &mut uid, &mut gid) })?;
        Ok((uid, gid))
    }
}
