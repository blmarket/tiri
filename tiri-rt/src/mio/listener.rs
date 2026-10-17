use std::io;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixListener;

use super::Io;

/// An owned Unix stream listener. Readiness and retries are internal to accept.
pub struct Listener {
    source: Io,
}

impl Listener {
    /// Take ownership of a nonblocking Unix stream listener on the current
    /// runtime. No descriptor flags are changed; construction errors close it.
    ///
    /// # Panics
    /// Panics if no runtime is initialized on this thread.
    #[track_caller]
    pub fn new(listener: UnixListener) -> io::Result<Self> {
        Ok(Self {
            source: Io::new(listener.into())?,
        })
    }
}

impl crate::AsyncAccept for Listener {
    async fn accept(&self) -> io::Result<OwnedFd> {
        self.source
            .read_with(|| {
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
                let raw = unsafe {
                    libc::accept4(
                        self.source.state.raw(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                    )
                };
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd"
                )))]
                let raw = unsafe {
                    libc::accept(
                        self.source.state.raw(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if raw < 0 {
                    return Err(io::Error::last_os_error());
                }
                let fd = unsafe { OwnedFd::from_raw_fd(raw) };
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd"
                )))]
                {
                    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
                    if flags < 0
                        || unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
                        || unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) } < 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(fd)
            })
            .await
    }
}
