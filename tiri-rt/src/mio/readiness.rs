use std::cell::{Cell, RefCell};
use std::future::Future;
use std::io::{self, IoSlice};
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, LocalWaker, Poll};

use mio::unix::SourceFd;

use crate::Received;

use super::fd_passing;
use super::runtime::{Core, Handle, invalid};

/// How the poller refuses devices that are always ready, such as /dev/null:
/// epoll reports EPERM, and kqueue EINVAL.
#[cfg(any(target_os = "linux", target_os = "android"))]
const UNPOLLABLE_DEVICE: libc::c_int = libc::EPERM;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const UNPOLLABLE_DEVICE: libc::c_int = libc::EINVAL;

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Read,
    Write,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Readiness {
    closed: bool,
    error: bool,
}

#[derive(Default)]
struct DirectionState {
    cached: Cell<Option<Readiness>>,
    generation: Cell<usize>,
    waiter: RefCell<Option<(usize, LocalWaker)>>,
}

pub(crate) struct IoState {
    core: Weak<Core>,
    id: usize,
    raw: RawFd,
    fd: RefCell<Option<OwnedFd>>,
    registered: bool,
    read: DirectionState,
    write: DirectionState,
}

impl IoState {
    fn direction(&self, direction: Direction) -> &DirectionState {
        match direction {
            Direction::Read => &self.read,
            Direction::Write => &self.write,
        }
    }

    pub(crate) fn raw(&self) -> RawFd {
        self.raw
    }

    pub(crate) fn observe(&self, event: &mio::event::Event) -> io::Result<()> {
        let core = self.core.upgrade().ok_or_else(invalid)?;
        for (direction, ready, closed) in [
            (Direction::Read, event.is_readable(), event.is_read_closed()),
            (
                Direction::Write,
                event.is_writable(),
                event.is_write_closed(),
            ),
        ] {
            if !ready && !closed && !event.is_error() {
                continue;
            }
            let state = self.direction(direction);
            let generation = state
                .generation
                .get()
                .checked_add(1)
                .ok_or_else(|| io::Error::other("readiness generation exhausted"))?;
            state.generation.set(generation);
            let previous = state.cached.get().unwrap_or_default();
            state.cached.set(Some(Readiness {
                closed: closed || previous.closed,
                error: event.is_error() || previous.error,
            }));
            let wake = state.waiter.borrow().as_ref().map(|(_, wake)| wake.clone());
            if let Some(wake) = wake {
                core.queue_wake(wake);
            }
        }
        Ok(())
    }

    pub(super) fn poll_ready(
        &self,
        direction: Direction,
        id: &mut Option<usize>,
        context: &mut Context<'_>,
    ) -> Poll<io::Result<(usize, Readiness)>> {
        let core = self.core.upgrade().ok_or_else(invalid)?;
        core.check()?;
        if self.fd.borrow().is_none() {
            return Poll::Ready(Err(invalid()));
        }
        let state = self.direction(direction);
        if state
            .waiter
            .borrow()
            .as_ref()
            .is_some_and(|(existing, _)| Some(*existing) != *id)
        {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "direction already has a waiter",
            )));
        }
        let id = match *id {
            Some(id) => id,
            None => {
                let next = core.allocate()?;
                *id = Some(next);
                next
            }
        };
        *state.waiter.borrow_mut() = Some((id, context.local_waker().clone()));
        if !self.registered {
            return Poll::Ready(Ok((state.generation.get(), Readiness::default())));
        }
        if let Some(ready) = state.cached.get() {
            return Poll::Ready(Ok((state.generation.get(), ready)));
        }
        Poll::Pending
    }

    pub(super) fn remove_waiter(&self, direction: Direction, id: Option<usize>) {
        let state = self.direction(direction);
        let mut waiter = state.waiter.borrow_mut();
        if waiter
            .as_ref()
            .is_some_and(|(existing, _)| Some(*existing) == id)
        {
            waiter.take();
        }
    }

    pub(super) fn clear(&self, direction: Direction, generation: usize) {
        let state = self.direction(direction);
        if state.generation.get() == generation {
            state.cached.set(None);
        }
    }

    pub(crate) fn close(&self, normal: bool) {
        let fd = self.fd.borrow_mut().take();
        if fd.is_none() {
            return;
        }
        if let Some(core) = self.core.upgrade() {
            if normal && self.registered {
                let error = core
                    .registry
                    .borrow()
                    .as_ref()
                    .and_then(|registry| registry.deregister(&mut SourceFd(&self.raw)).err());
                if let Some(error) = error {
                    core.record(error);
                }
            }
            core.io.borrow_mut().remove(&self.id);
            if core.fds.borrow().get(&self.raw) == Some(&self.id) {
                core.fds.borrow_mut().remove(&self.raw);
            }
        }
        self.read.waiter.borrow_mut().take();
        self.write.waiter.borrow_mut().take();
        drop(fd);
    }
}

impl Drop for IoState {
    fn drop(&mut self) {
        let normal = self
            .core
            .upgrade()
            .is_some_and(|core| core.pid == std::process::id());
        self.close(normal);
    }
}

/// An owned descriptor with [`crate::AsyncRead`]/[`crate::AsyncWrite`] byte-stream
/// operations. The runtime owns readiness, syscall retries, and cancellation.
/// Unix stream sockets pass one optional FD per operation and suppress SIGPIPE.
/// Files, pipes, and PTYs return no FDs and reject attached FDs before writing.
/// For other descriptors the host retains SIGPIPE policy.
/// Use [`super::Listener`] for accepting connections and [`crate::unix`] for
/// descriptor and terminal control. Regular files bypass the readiness poller
/// and perform synchronous I/O on the runtime thread. Devices rejected by the
/// poller can also perform immediate I/O, but cannot wait for a later event.
///
/// Readiness is not a public operation:
/// ```compile_fail
/// use tiri_rt::mio::Io;
/// fn readiness(io: &Io) {
///     let _ = io.ready(true, false);
/// }
/// ```
///
/// Custom operations are not part of the public byte-stream API:
/// ```compile_fail
/// use tiri_rt::mio::Io;
/// fn custom(io: &Io) {
///     let _ = io.read_with(|| Ok(()));
/// }
/// ```
/// ```compile_fail
/// use tiri_rt::mio::Io;
/// fn custom(io: &Io) {
///     let _ = io.write_with(|| Ok(()));
/// }
/// ```
pub struct Io {
    pub(crate) state: Rc<IoState>,
    fd_passing: bool,
}

impl Io {
    /// Take ownership of a byte-stream descriptor on the current runtime.
    /// Non-file descriptors must be nonblocking; no flags are changed.
    /// Regular files bypass the readiness poller and may block the runtime
    /// thread. Construction errors close the descriptor.
    ///
    /// # Panics
    /// Panics if no runtime is initialized on this thread.
    #[track_caller]
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        let handle = Handle::current();
        let core = &handle.core;
        core.check()?;
        let raw = fd.as_raw_fd();
        if core.fds.borrow().contains_key(&raw) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "descriptor already registered",
            ));
        }
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(raw, stat.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let kind = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT;
        let regular = kind == libc::S_IFREG;
        let fd_passing = kind == libc::S_IFSOCK && fd_passing::supported(raw)?;
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if !regular && flags & libc::O_NONBLOCK == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "descriptor must be nonblocking",
            ));
        }
        let id = core.allocate()?;
        let registered = if regular {
            false
        } else {
            let result = core
                .registry
                .borrow()
                .as_ref()
                .ok_or_else(invalid)?
                .register(
                    &mut SourceFd(&raw),
                    mio::Token(id),
                    mio::Interest::READABLE | mio::Interest::WRITABLE,
                );
            match result {
                Ok(()) => true,
                Err(error) if error.raw_os_error() == Some(UNPOLLABLE_DEVICE) => false,
                Err(error) => return Err(error),
            }
        };
        let state = Rc::new(IoState {
            core: Rc::downgrade(core),
            id,
            raw,
            fd: RefCell::new(Some(fd)),
            registered,
            read: DirectionState::default(),
            write: DirectionState::default(),
        });
        core.io.borrow_mut().insert(id, Rc::downgrade(&state));
        core.fds.borrow_mut().insert(raw, id);
        Ok(Self { state, fd_passing })
    }
}

impl crate::AsyncRead for Io {
    async fn read(&self, buffer: &mut [u8]) -> io::Result<Received> {
        if buffer.is_empty() {
            return Ok(Received { bytes: 0, fd: None });
        }
        self.read_with(|| {
            if self.fd_passing {
                return fd_passing::read(self.state.raw(), buffer);
            }
            let count = unsafe {
                libc::read(
                    self.state.raw(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len().min(isize::MAX as usize),
                )
            };
            Ok(Received {
                bytes: syscall_result(count)?,
                fd: None,
            })
        })
        .await
    }
}

impl crate::AsyncWrite for Io {
    async fn write(
        &self,
        buffers: &[IoSlice<'_>],
        fd: Option<BorrowedFd<'_>>,
    ) -> io::Result<usize> {
        if fd.is_some() && !self.fd_passing {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "descriptor cannot pass FDs",
            ));
        }
        let has_bytes = buffers.iter().any(|buffer| !buffer.is_empty());
        if !has_bytes && fd.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "FD passing requires at least one byte",
            ));
        }
        if !has_bytes {
            return Ok(0);
        }
        let mut iov: Vec<_> = buffers
            .iter()
            .map(|buffer| libc::iovec {
                iov_base: buffer.as_ptr().cast_mut().cast(),
                iov_len: buffer.len(),
            })
            .collect();
        let count = libc::c_int::try_from(iov.len())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        self.write_with(|| {
            if self.fd_passing {
                return fd_passing::write(self.state.raw(), &mut iov, fd);
            }
            syscall_result(unsafe { libc::writev(self.state.raw(), iov.as_ptr(), count) })
        })
        .await
    }
}

impl Io {
    pub(crate) fn read_with<T, F>(&self, operation: F) -> impl Future<Output = io::Result<T>>
    where
        F: FnMut() -> io::Result<T>,
    {
        self.operation(Direction::Read, operation)
    }

    pub(crate) fn write_with<T, F>(&self, operation: F) -> impl Future<Output = io::Result<T>>
    where
        F: FnMut() -> io::Result<T>,
    {
        self.operation(Direction::Write, operation)
    }
}

pub(super) fn syscall_result(count: isize) -> io::Result<usize> {
    if count < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(count as usize)
    }
}

impl Io {
    fn operation<F>(&self, direction: Direction, operation: F) -> Operation<'_, F> {
        Operation {
            source: self,
            direction,
            id: None,
            operation,
        }
    }
}

struct Operation<'a, F> {
    source: &'a Io,
    direction: Direction,
    id: Option<usize>,
    operation: F,
}

impl<F> Unpin for Operation<'_, F> {}

impl<T, F: FnMut() -> io::Result<T>> Future for Operation<'_, F> {
    type Output = io::Result<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let generation = match this
            .source
            .state
            .poll_ready(this.direction, &mut this.id, context)
        {
            Poll::Ready(result) => result?.0,
            Poll::Pending => return Poll::Pending,
        };
        match (this.operation)() {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if !this.source.state.registered {
                    this.source
                        .state
                        .remove_waiter(this.direction, this.id.take());
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "descriptor cannot wait for readiness",
                    )));
                }
                this.source.state.clear(this.direction, generation);
                match this
                    .source
                    .state
                    .poll_ready(this.direction, &mut this.id, context)
                {
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Ready(Ok(_)) => context.local_waker().wake_by_ref(),
                    Poll::Pending => {}
                }
                Poll::Pending
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                context.local_waker().wake_by_ref();
                Poll::Pending
            }
            result => {
                this.source
                    .state
                    .remove_waiter(this.direction, this.id.take());
                Poll::Ready(result)
            }
        }
    }
}

impl<F> Drop for Operation<'_, F> {
    fn drop(&mut self) {
        self.source
            .state
            .remove_waiter(self.direction, self.id.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime as _;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::task::Waker;
    use std::time::Duration;

    #[test]
    fn unregistered_operation_cannot_wait_and_releases_its_waiter() {
        let mut runtime = super::super::Runtime::new().unwrap();
        let file = std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        let source = super::Io::new(file.into()).unwrap();
        let mut operation = source.read_with(|| Err::<(), _>(io::ErrorKind::WouldBlock.into()));
        assert!(
            matches!(poll(&mut operation), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Unsupported)
        );
        assert!(matches!(
            poll(&mut source.read_with(|| Ok(42))),
            Poll::Ready(Ok(42))
        ));
        drop(operation);
        drop(source);
        tick(&mut runtime);
    }

    #[test]
    fn acknowledging_an_old_generation_preserves_a_new_notification() {
        let mut runtime = super::super::Runtime::new().unwrap();
        let (mut writer, reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let source = super::Io::new(reader.into()).unwrap();
        writer.write_all(b"first").unwrap();
        runtime.poll(Some(Duration::ZERO)).unwrap();
        let old = source.state.read.generation.get();
        let mut bytes = [0; 16];
        assert_eq!(
            unsafe { libc::read(source.state.raw(), bytes.as_mut_ptr().cast(), bytes.len()) },
            5
        );
        writer.write_all(b"new").unwrap();
        runtime.poll(Some(Duration::ZERO)).unwrap();
        assert!(source.state.read.generation.get() > old);
        source.state.clear(Direction::Read, old);
        let mut operation = source.read_with(|| Ok(42));
        assert!(matches!(
            Pin::new(&mut operation).poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok(42))
        ));
    }

    fn tick(runtime: &mut super::super::Runtime) {
        runtime.poll(Some(Duration::ZERO)).unwrap();
    }
    fn poll<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
        Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
    }
    fn pair() -> (UnixStream, OwnedFd) {
        let (writer, reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        (writer, reader.into())
    }
    #[test]
    fn interrupted_operations_yield_keep_the_waiter_and_return_errors() {
        let mut runtime = super::super::Runtime::new().unwrap();
        let (_writer, fd) = pair();
        let source = super::Io::new(fd).unwrap();
        tick(&mut runtime);
        let calls = Cell::new(0);
        let mut operation = source.write_with(|| {
            calls.set(calls.get() + 1);
            match calls.get() {
                1 => Err(io::Error::from(io::ErrorKind::Interrupted)),
                _ => Err::<(), _>(io::Error::from(io::ErrorKind::PermissionDenied)),
            }
        });
        assert_eq!(calls.get(), 0);
        assert!(poll(&mut operation).is_pending());
        assert_eq!(calls.get(), 1);
        assert!(
            matches!(poll(&mut source.write_with(|| Ok(()))), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::AlreadyExists)
        );
        assert!(
            matches!(poll(&mut operation), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::PermissionDenied)
        );
        assert_eq!(calls.get(), 2);
        assert!(matches!(
            poll(&mut source.write_with(|| Ok(42))),
            Poll::Ready(Ok(42))
        ));
    }
}
