use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashSet};
use std::ffi::c_int;
use std::future::Future;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Context, Poll};

use super::Io;
use super::readiness::Direction;
use super::runtime::{Core, Handle, invalid};

static CLAIMED: LazyLock<Mutex<HashSet<c_int>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

struct Subscription {
    number: c_int,
    flag: Arc<AtomicBool>,
    registration: signal_hook::SigId,
}

struct Resources {
    io: Io,
    subscriptions: Vec<Subscription>,
}

impl Drop for Resources {
    fn drop(&mut self) {
        let mut claimed = CLAIMED.lock().unwrap_or_else(|poison| poison.into_inner());
        for subscription in &self.subscriptions {
            signal_hook::low_level::unregister(subscription.registration);
            claimed.remove(&subscription.number);
        }
    }
}

pub(crate) struct SignalState {
    core: Weak<Core>,
    id: usize,
    resources: RefCell<Option<Resources>>,
    next: Cell<usize>,
}

impl SignalState {
    pub(crate) fn close(&self) {
        let resources = self.resources.borrow_mut().take();
        drop(resources);
        if let Some(core) = self.core.upgrade() {
            core.signals.borrow_mut().remove(&self.id);
        }
    }
}

impl Drop for SignalState {
    fn drop(&mut self) {
        self.close();
    }
}

/// Coalescing Unix signal subscription with signal identity preserved.
pub struct Signals {
    state: Rc<SignalState>,
}

impl Signals {
    /// Subscribe to a nonempty set of distinct, valid, catchable signal numbers
    /// on the current runtime.
    ///
    /// # Panics
    /// Panics if no runtime is initialized on this thread.
    #[track_caller]
    pub fn new(set: &[c_int]) -> io::Result<Self> {
        let handle = Handle::current();
        let core = &handle.core;
        core.check()?;
        let unique = set.iter().copied().collect::<BTreeSet<_>>();
        if set.is_empty()
            || unique.len() != set.len()
            || set.iter().any(|signal| {
                if *signal <= 0 || signal_hook::consts::FORBIDDEN.contains(signal) {
                    return true;
                }
                // SAFETY: querying a signal disposition without changing it.
                unsafe { libc::sigaction(*signal, std::ptr::null(), std::ptr::null_mut()) != 0 }
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid signal set",
            ));
        }
        let id = core.allocate()?;
        let (reader, writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        let source = Io::new(reader.into())?;
        let writer = Arc::new(OwnedFd::from(writer));
        let mut resources = Resources {
            io: source,
            subscriptions: Vec::new(),
        };
        {
            let mut claimed = CLAIMED.lock().unwrap_or_else(|poison| poison.into_inner());
            if set.iter().any(|signal| claimed.contains(signal)) {
                // Resources::drop also locks CLAIMED.
                drop(claimed);
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "signal already subscribed",
                ));
            }
            for &number in set {
                let flag = Arc::new(AtomicBool::new(false));
                let pending = flag.clone();
                let output = writer.clone();
                // SAFETY: callback only stores an atomic flag and performs a
                // nonblocking, SIGPIPE-suppressed send on its owned socket.
                let registration = unsafe {
                    signal_hook::low_level::register(number, move || {
                        if pending.swap(true, Ordering::SeqCst) {
                            return;
                        }
                        loop {
                            let sent = libc::send(
                                output.as_raw_fd(),
                                b"x".as_ptr().cast(),
                                1,
                                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
                            );
                            if sent >= 0 || errno::errno().0 != libc::EINTR {
                                break;
                            }
                        }
                    })
                };
                match registration {
                    Ok(registration) => {
                        claimed.insert(number);
                        resources.subscriptions.push(Subscription {
                            number,
                            flag,
                            registration,
                        });
                    }
                    Err(error) => {
                        drop(claimed);
                        return Err(error);
                    }
                }
            }
        }
        let state = Rc::new(SignalState {
            core: Rc::downgrade(core),
            id,
            resources: RefCell::new(Some(resources)),
            next: Cell::new(0),
        });
        core.signals.borrow_mut().insert(id, Rc::downgrade(&state));
        Ok(Self { state })
    }
}

impl crate::Signals for Signals {
    type Recv<'a> = Recv<'a>;
    fn recv(&mut self) -> Recv<'_> {
        Recv {
            signals: self,
            waiter: None,
        }
    }
}

/// A cancellation-safe signal receive using the runtime's local waker.
pub struct Recv<'a> {
    signals: &'a mut Signals,
    waiter: Option<usize>,
}

impl Future for Recv<'_> {
    type Output = io::Result<c_int>;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let state = &this.signals.state;
        let core = state.core.upgrade().ok_or_else(invalid)?;
        core.check()?;
        let resources = state.resources.borrow();
        let resources = resources.as_ref().ok_or_else(invalid)?;
        for offset in 0..resources.subscriptions.len() {
            let index = (state.next.get() + offset) % resources.subscriptions.len();
            let subscription = &resources.subscriptions[index];
            if subscription.flag.swap(false, Ordering::SeqCst) {
                state.next.set((index + 1) % resources.subscriptions.len());
                resources
                    .io
                    .state
                    .remove_waiter(Direction::Read, this.waiter.take());
                return Poll::Ready(Ok(subscription.number));
            }
        }
        let generation =
            match resources
                .io
                .state
                .poll_ready(Direction::Read, &mut this.waiter, context)
            {
                Poll::Ready(result) => result?.0,
                Poll::Pending => return Poll::Pending,
            };
        let mut bytes = [0u8; 4096];
        // SAFETY: the resource owns the live fd and bytes is a valid writable buffer.
        let count = unsafe {
            libc::read(
                resources.io.state.raw(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if count < 0 {
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::WouldBlock => {
                    resources.io.state.clear(Direction::Read, generation);
                    return resources
                        .io
                        .state
                        .poll_ready(Direction::Read, &mut this.waiter, context)
                        .map(|result| result.map(|_| unreachable!("just cleared readiness")));
                }
                io::ErrorKind::Interrupted => {}
                _ => return Poll::Ready(Err(error)),
            }
        } else if count == 0 {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "signal socket closed",
            )));
        }
        // At most one read per poll; stale wake bytes cannot monopolize a turn.
        context.local_waker().wake_by_ref();
        Poll::Pending
    }
}

impl Drop for Recv<'_> {
    fn drop(&mut self) {
        if let Some(resources) = self.signals.state.resources.borrow().as_ref() {
            resources
                .io
                .state
                .remove_waiter(Direction::Read, self.waiter.take());
        }
    }
}
