use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::io;
use std::os::fd::RawFd;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{ContextBuilder, LocalWake, LocalWaker, Poll, Waker};
use std::time::{Duration, Instant};

use super::readiness::IoState;
use super::signals::SignalState;

// Bound each turn so a self-waking task cannot monopolize the host thread.
const MAX_POLLS_PER_TURN: usize = 128;

type TaskFuture = Pin<Box<dyn Future<Output = ()>>>;

enum Ready {
    Task(Weak<TaskState>),
    Wake(LocalWaker),
}

impl Ready {
    fn is_runnable(&self) -> bool {
        match self {
            Self::Task(task) => task.upgrade().is_some_and(|task| !task.cancelled.get()),
            Self::Wake(_) => true,
        }
    }
}

pub(crate) fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "runtime is no longer usable in this process",
    )
}

/// Scheduling state accessible independently of the runtime owner.
pub(crate) struct Core {
    pub(crate) pid: u32,
    pub(crate) alive: Cell<bool>,
    poisoned: Cell<bool>,
    next_id: Cell<usize>,
    pub(crate) registry: RefCell<Option<mio::Registry>>,
    // Only a count for idle detection and queue compaction, never task ownership.
    pending_tasks: Cell<usize>,
    // Task entries observe their owners; the queue never owns task futures.
    ready: RefCell<VecDeque<Ready>>,
    pub(crate) timers: RefCell<BTreeMap<(Instant, usize), LocalWaker>>,
    pub(crate) io: RefCell<HashMap<usize, Weak<IoState>>>,
    pub(crate) fds: RefCell<HashMap<RawFd, usize>>,
    pub(crate) signals: RefCell<HashMap<usize, Weak<SignalState>>>,
    pub(crate) error: RefCell<Option<io::Error>>,
}

impl Core {
    pub(crate) fn check(&self) -> io::Result<()> {
        if !self.alive.get() || self.pid != std::process::id() {
            return Err(invalid());
        }
        if self.poisoned.get() {
            return Err(io::Error::other("runtime poisoned by a task panic"));
        }
        Ok(())
    }

    pub(crate) fn allocate(&self) -> io::Result<usize> {
        self.check()?;
        let id = self.next_id.get();
        self.next_id.set(
            id.checked_add(1)
                .ok_or_else(|| io::Error::other("identity space exhausted"))?,
        );
        Ok(id)
    }

    pub(crate) fn record(&self, error: io::Error) {
        let mut pending = self.error.borrow_mut();
        if pending.is_none() {
            *pending = Some(error);
        }
    }

    pub(crate) fn queue_wake(&self, wake: LocalWaker) {
        self.ready.borrow_mut().push_back(Ready::Wake(wake));
    }

    fn compact_ready(&self) {
        let threshold = self
            .pending_tasks
            .get()
            .saturating_mul(2)
            .saturating_add(64);
        let mut ready = self.ready.borrow_mut();
        if ready.len() > threshold {
            ready.retain(Ready::is_runnable);
        }
    }

    fn runnable(&self) -> bool {
        let mut ready = self.ready.borrow_mut();
        while ready.front().is_some_and(|entry| !entry.is_runnable()) {
            ready.pop_front();
        }
        !ready.is_empty()
    }

    fn close(&self) {
        if !self.alive.replace(false) {
            return;
        }
        let normal = self.pid == std::process::id();
        let signals = self
            .signals
            .borrow_mut()
            .drain()
            .filter_map(|(_, s)| s.upgrade())
            .collect::<Vec<_>>();
        for signal in signals {
            signal.close();
        }
        let sources = self
            .io
            .borrow_mut()
            .drain()
            .filter_map(|(_, s)| s.upgrade())
            .collect::<Vec<_>>();
        for source in sources {
            source.close(normal);
        }
        self.registry.borrow_mut().take();
        self.fds.borrow_mut().clear();
        self.timers.borrow_mut().clear();
        // Release queued wakers without holding the queue borrow during Drop.
        let ready = std::mem::take(&mut *self.ready.borrow_mut());
        drop(ready);
    }
}

struct TaskState {
    core: Weak<Core>,
    future: RefCell<Option<TaskFuture>>,
    cancelled: Cell<bool>,
    queued: Cell<bool>,
    waker: LocalWaker,
}

impl TaskState {
    fn cancel(&self) {
        if self.cancelled.replace(true) {
            return;
        }
        self.queued.set(false);
        if let Some(core) = self.core.upgrade() {
            core.pending_tasks.set(core.pending_tasks.get() - 1);
            core.compact_ready();
        }
        // Never run a user destructor while borrowing the future slot.
        let future = self.future.borrow_mut().take();
        drop(future);
    }

    fn enqueue(self: &Rc<Self>) {
        let Some(core) = self.core.upgrade() else {
            return;
        };
        if core.check().is_err() || self.cancelled.get() || self.queued.replace(true) {
            return;
        }
        core.ready
            .borrow_mut()
            .push_back(Ready::Task(Rc::downgrade(self)));
    }
}

struct TaskWake(Weak<TaskState>);
impl LocalWake for TaskWake {
    fn wake(self: Rc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Rc<Self>) {
        if let Some(task) = self.0.upgrade() {
            task.enqueue();
        }
    }
}

/// Owns a local future. Dropping it cancels queued or waiting work immediately;
/// an executing future is released when its current poll returns.
#[must_use = "dropping the task cancels its future"]
pub struct Task {
    state: Rc<TaskState>,
}

impl Drop for Task {
    fn drop(&mut self) {
        self.state.cancel();
    }
}

/// Cloneable, thread-local capability bound to one runtime generation.
#[derive(Clone)]
pub struct Handle {
    pub(crate) core: Rc<Core>,
}

thread_local! {
    static HANDLE: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

impl Handle {
    /// Clone the current handle on this thread. Runtime initialization registers
    /// the default; [`Runtime::enter`] temporarily selects another runtime.
    /// Looking up a handle neither creates nor drives a runtime.
    ///
    /// # Panics
    /// Panics before initialization or after the registered runtime is dropped.
    #[track_caller]
    pub fn current() -> Self {
        HANDLE.with(|handle| {
            handle
                .borrow()
                .as_ref()
                .expect("runtime initialized")
                .clone()
        })
    }
}

impl crate::Handle for Handle {
    type Task = Task;

    fn spawn<F>(&self, future: F) -> io::Result<Task>
    where
        F: Future<Output = ()> + 'static,
    {
        self.core.check()?;
        let pending = self
            .core
            .pending_tasks
            .get()
            .checked_add(1)
            .ok_or_else(|| io::Error::other("too many pending tasks"))?;
        let state = Rc::new_cyclic(|weak| TaskState {
            core: Rc::downgrade(&self.core),
            future: RefCell::new(Some(Box::pin(future))),
            cancelled: Cell::new(false),
            queued: Cell::new(false),
            waker: LocalWaker::from(Rc::new(TaskWake(weak.clone()))),
        });
        self.core.pending_tasks.set(pending);
        state.enqueue();
        Ok(Task { state })
    }
}

/// Host-driven local-waker executor with a mio Unix readiness backend.
///
/// Construction registers the runtime for [`Handle::current`] if none is
/// registered. Additional runtimes leave that registration unchanged, so
/// independent drivers used for synchronous I/O do not replace it.
/// The registered runtime clears its handle after shutdown cleanup.
pub struct Runtime {
    core: Rc<Core>,
    poller: mio::Poll,
    events: mio::Events,
}

impl crate::Runtime for Runtime {
    type Handle = Handle;

    fn new() -> io::Result<Self> {
        let poller = mio::Poll::new()?;
        let registry = poller.registry().try_clone()?;
        let runtime = Self {
            core: Rc::new(Core {
                pid: std::process::id(),
                alive: Cell::new(true),
                poisoned: Cell::new(false),
                next_id: Cell::new(1),
                registry: RefCell::new(Some(registry)),
                pending_tasks: Cell::new(0),
                ready: RefCell::default(),
                timers: RefCell::default(),
                io: RefCell::default(),
                fds: RefCell::default(),
                signals: RefCell::default(),
                error: RefCell::default(),
            }),
            poller,
            events: mio::Events::with_capacity(1024),
        };
        HANDLE.with(|handle| {
            let mut handle = handle.borrow_mut();
            if handle.is_none() {
                *handle = Some(runtime.handle());
            }
        });
        Ok(runtime)
    }

    fn handle(&self) -> Handle {
        Handle {
            core: self.core.clone(),
        }
    }

    fn poll(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        self.prepare_poll(max_wait)?;
        self.core.dispatch()
    }
}

impl Runtime {
    /// Run a closure with this runtime's handle as [`Handle::current`].
    /// Use this to construct resources for an independent synchronous driver.
    /// The previous handle is restored when the closure returns or unwinds.
    pub fn enter<T>(&self, f: impl FnOnce() -> T) -> T {
        struct Restore(Option<Handle>);
        impl Drop for Restore {
            fn drop(&mut self) {
                HANDLE.with(|handle| {
                    *handle.borrow_mut() = self.0.take().filter(|old| old.core.alive.get());
                });
            }
        }
        let _restore = Restore(HANDLE.with(|handle| {
            handle.borrow_mut().replace(Handle {
                core: self.core.clone(),
            })
        }));
        f()
    }

    /// Whether a runtime has registered its handle on this thread.
    /// Remains true during shutdown cleanup, until that handle is cleared.
    pub fn is_initialized() -> bool {
        HANDLE
            .try_with(|handle| handle.borrow().is_some())
            .unwrap_or(false)
    }

    /// Drive a borrowed future to completion on this runtime.
    ///
    /// This is also useful for synchronous startup/teardown with an independent
    /// runtime. It must not reenter an already borrowed runtime. Local wakeups
    /// drive the future; ordinary Waker notifications remain inert.
    pub fn block_on<F: Future>(&mut self, future: F) -> io::Result<F::Output> {
        struct Wake(Cell<bool>);
        impl LocalWake for Wake {
            fn wake(self: Rc<Self>) {
                self.0.set(true);
            }
            fn wake_by_ref(self: &Rc<Self>) {
                self.0.set(true);
            }
        }
        struct Pending<'a>(&'a Core);
        impl Drop for Pending<'_> {
            fn drop(&mut self) {
                self.0.pending_tasks.set(self.0.pending_tasks.get() - 1);
            }
        }
        let core = self.core.clone();
        core.check()?;
        let count = core
            .pending_tasks
            .get()
            .checked_add(1)
            .ok_or_else(|| io::Error::other("too many pending tasks"))?;
        core.pending_tasks.set(count);
        let _pending = Pending(&core);
        let wake = Rc::new(Wake(Cell::new(true)));
        let local = LocalWaker::from(wake.clone());
        let mut context = ContextBuilder::from_waker(Waker::noop())
            .local_waker(&local)
            .build();
        let mut future = std::pin::pin!(future);
        loop {
            if wake.0.replace(false) {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    future.as_mut().poll(&mut context)
                })) {
                    Ok(Poll::Ready(value)) => return Ok(value),
                    Ok(Poll::Pending) => {}
                    Err(panic) => {
                        core.poisoned.set(true);
                        std::panic::resume_unwind(panic);
                    }
                }
            }
            crate::Runtime::poll(self, wake.0.get().then_some(Duration::ZERO))?;
        }
    }

    /// Collect readiness without invoking wakers, callbacks, or futures.
    fn prepare_poll(&mut self, max_wait: Option<Duration>) -> io::Result<()> {
        self.core.check()?;
        if let Some(error) = self.core.error.borrow_mut().take() {
            return Err(error);
        }
        self.queue_ready_timers();
        let timeout = if self.core.runnable() || self.core.pending_tasks.get() == 0 {
            Some(Duration::ZERO)
        } else {
            let timer = self
                .core
                .timers
                .borrow()
                .first_key_value()
                .map(|(key, _)| key.0.saturating_duration_since(Instant::now()));
            match (max_wait, timer) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, None) => a,
                (None, b) => b,
            }
        };
        let stop = timeout.and_then(|duration| Instant::now().checked_add(duration));
        loop {
            self.events.clear();
            let remaining = stop.map(|stop| stop.saturating_duration_since(Instant::now()));
            match self.poller.poll(&mut self.events, remaining) {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    if stop.is_some_and(|stop| stop <= Instant::now()) {
                        break;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        for event in &self.events {
            let source = self
                .core
                .io
                .borrow()
                .get(&event.token().0)
                .and_then(Weak::upgrade);
            if let Some(source) = source {
                source.observe(event)?;
            }
        }
        self.queue_ready_timers();
        Ok(())
    }

    fn queue_ready_timers(&self) {
        let now = Instant::now();
        loop {
            let wake = {
                let mut timers = self.core.timers.borrow_mut();
                if timers
                    .first_key_value()
                    .is_some_and(|(key, _)| key.0 <= now)
                {
                    timers.pop_first().map(|(_, wake)| wake)
                } else {
                    None
                }
            };
            match wake {
                Some(wake) => self.core.queue_wake(wake),
                None => break,
            }
        }
    }
}

impl Core {
    /// Run queued notifications and work, releasing queue borrows before user code.
    fn dispatch(&self) -> io::Result<()> {
        self.check()?;
        let mut polled = 0;
        while polled < MAX_POLLS_PER_TURN && self.alive.get() {
            let ready = self.ready.borrow_mut().pop_front();
            let Some(ready) = ready else {
                break;
            };
            let weak = match ready {
                Ready::Task(weak) => weak,
                Ready::Wake(wake) => {
                    polled += 1;
                    wake.wake();
                    continue;
                }
            };
            let Some(task) = weak.upgrade() else {
                continue;
            };
            if task.cancelled.get() {
                continue;
            }
            task.queued.set(false);
            let future = task.future.borrow_mut().take();
            let Some(mut future) = future else {
                continue;
            };
            polled += 1;
            let mut context = ContextBuilder::from_waker(Waker::noop())
                .local_waker(&task.waker)
                .build();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                future.as_mut().poll(&mut context)
            }));
            match result {
                Ok(result) if result.is_pending() && !task.cancelled.get() => {
                    *task.future.borrow_mut() = Some(future);
                }
                Ok(_) => {
                    task.cancel();
                    drop(future);
                }
                Err(panic) => {
                    self.poisoned.set(true);
                    task.cancel();
                    drop(future);
                    std::panic::resume_unwind(panic);
                }
            }
        }
        if let Some(error) = self.error.borrow_mut().take() {
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.core.close();
        // Keep the handle accessible while close releases user-owned wakers.
        // TLS may already have been destroyed during thread shutdown.
        let _ = HANDLE.try_with(|handle| {
            let mut handle = handle.borrow_mut();
            if handle
                .as_ref()
                .is_some_and(|handle| Rc::ptr_eq(&handle.core, &self.core))
            {
                handle.take()
            } else {
                None
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Handle as _, Runtime as _};
    use std::task::{Context, Poll};

    #[test]
    fn cancellation_storage_is_bounded_without_a_driver_turn() {
        let runtime = Runtime::new().unwrap();
        let handle = runtime.handle();
        for _ in 0..10_000 {
            drop(handle.spawn(async {}).unwrap());
            let mut sleep = crate::mio::Sleep::new(Instant::now() + Duration::from_secs(3600));
            assert!(
                Pin::new(&mut sleep)
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            drop(sleep);
        }
        assert_eq!(runtime.core.pending_tasks.get(), 0);
        assert!(runtime.core.timers.borrow().is_empty());
        assert!(runtime.core.ready.borrow().len() <= 64);
    }

    #[test]
    fn moving_a_pending_timer_refreshes_its_task_waker() {
        let mut runtime = Runtime::new().unwrap();
        let handle = runtime.handle();
        let deadline = Instant::now() + Duration::from_millis(10);
        let timer = Rc::new(RefCell::new(crate::mio::Sleep::new(deadline)));
        let first_timer = timer.clone();
        let first = handle
            .spawn(std::future::poll_fn(move |context| {
                Pin::new(&mut *first_timer.borrow_mut())
                    .poll(context)
                    .map(|result| result.unwrap())
            }))
            .unwrap();
        runtime.poll(Some(Duration::ZERO)).unwrap();
        drop(first);
        let done = Rc::new(Cell::new(false));
        let mark = done.clone();
        let _second = handle
            .spawn(std::future::poll_fn(move |context| {
                match Pin::new(&mut *timer.borrow_mut()).poll(context) {
                    Poll::Ready(result) => {
                        result.unwrap();
                        mark.set(true);
                        Poll::Ready(())
                    }
                    Poll::Pending => Poll::Pending,
                }
            }))
            .unwrap();
        runtime.poll(Some(Duration::ZERO)).unwrap();
        runtime.poll(Some(Duration::from_millis(100))).unwrap();
        assert!(done.get());
    }
}
