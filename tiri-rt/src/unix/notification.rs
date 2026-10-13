//! Coalescing notifications, distinct from byte-stream writes: saturation is
//! successful notification, and clearing an empty descriptor never waits.
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

pub trait Notify: AsFd + Send + Sync {
    fn wake(&self) -> io::Result<()>;
    fn clear(&self) -> io::Result<()>;
}

/// A thread-safe, initially signalled notification for foreign event consumers.
/// Linux uses eventfd; other Unix hosts use a nonblocking pipe.
pub struct Notification {
    read: OwnedFd,
    #[cfg(not(target_os = "linux"))]
    write: OwnedFd,
}

impl Notification {
    pub fn new() -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: eventfd has no pointer arguments and returns new ownership.
            let raw =
                super::check(unsafe { libc::eventfd(1, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) })?;
            Ok(Self {
                read: unsafe { OwnedFd::from_raw_fd(raw) },
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let mut fds = [-1; 2];
            // SAFETY: pipe initializes both descriptor slots on success.
            super::check(unsafe { libc::pipe(fds.as_mut_ptr()) })?;
            let result = Self {
                read: unsafe { OwnedFd::from_raw_fd(fds[0]) },
                write: unsafe { OwnedFd::from_raw_fd(fds[1]) },
            };
            for fd in [&result.read, &result.write] {
                super::set_nonblocking(fd.as_fd(), true)?;
                // SAFETY: both endpoints are owned and live.
                super::check(unsafe {
                    libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC)
                })?;
            }
            result.wake()?;
            Ok(result)
        }
    }
}

impl AsFd for Notification {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.read.as_fd()
    }
}

impl Notify for Notification {
    fn wake(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        let (fd, bytes) = (self.read.as_raw_fd(), 1u64.to_ne_bytes());
        #[cfg(not(target_os = "linux"))]
        let (fd, bytes) = (self.write.as_raw_fd(), [0u8]);
        loop {
            // SAFETY: fd is live and bytes remains readable for the syscall.
            let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
            if n == bytes.len() as isize {
                return Ok(());
            }
            if n >= 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                // Already signalled; no additional token is needed.
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(error),
            }
        }
    }

    fn clear(&self) -> io::Result<()> {
        let mut bytes = [0u8; 64];
        loop {
            // SAFETY: the owned, nonblocking endpoint and byte storage are live.
            let n = unsafe {
                libc::read(
                    self.read.as_raw_fd(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            if n == 0 {
                return Ok(());
            }
            if n > 0 {
                #[cfg(target_os = "linux")]
                return Ok(());
                #[cfg(not(target_os = "linux"))]
                continue;
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(error),
            }
        }
    }
}
