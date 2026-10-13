//! Executor and readiness contracts; process-global behavior is tested separately.
#![feature(local_waker)]

use tiri_rt::mio;
use tiri_rt::{AsyncRead, AsyncWrite, Handle, Received, Runtime};
use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::io::{self, IoSlice, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

fn tick(runtime: &mut mio::Runtime) {
    runtime.poll(Some(Duration::ZERO)).unwrap()
}
fn poll<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
}
fn pair() -> (UnixStream, OwnedFd) {
    let (writer, reader) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    (writer, reader.into())
}

struct Dropped(Rc<Cell<usize>>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn tasks_release_completed_work_and_own_pending_work_after_runtime_shutdown() {
    let mut runtime = mio::Runtime::new().unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let task = runtime
        .handle()
        .spawn(async move { observed.set(observed.get() + 1) })
        .unwrap();
    assert_eq!(calls.get(), 0);
    tick(&mut runtime);
    assert_eq!(calls.get(), 1);
    assert_eq!(Rc::strong_count(&calls), 1);
    drop(task);

    let count = Rc::new(Cell::new(0));
    let spy = Dropped(count.clone());
    let pending = runtime
        .handle()
        .spawn(async move {
            let _spy = spy;
            std::future::pending::<()>().await;
        })
        .unwrap();
    drop(runtime);
    assert_eq!(count.get(), 0, "the task still owns its future");
    drop(pending);
    assert_eq!(count.get(), 1);
}

#[test]
fn readiness_wakers_can_schedule_while_the_owner_is_borrowed() {
    use std::task::{ContextBuilder, LocalWake, LocalWaker};

    struct Wake {
        handle: mio::Handle,
        calls: Rc<Cell<usize>>,
        tasks: Rc<RefCell<Vec<mio::Task>>>,
    }
    impl LocalWake for Wake {
        fn wake(self: Rc<Self>) {
            let calls = self.calls.clone();
            self.tasks.borrow_mut().push(
                self.handle
                    .spawn(async move { calls.set(calls.get() + 1) })
                    .unwrap(),
            );
        }
    }

    let runtime = Rc::new(RefCell::new(mio::Runtime::new().unwrap()));
    let calls = Rc::new(Cell::new(0));
    let wake = LocalWaker::from(Rc::new(Wake {
        handle: runtime.borrow().handle(),
        calls: calls.clone(),
        tasks: Rc::new(RefCell::new(Vec::new())),
    }));
    let mut context = ContextBuilder::from_waker(Waker::noop())
        .local_waker(&wake)
        .build();
    let (mut writer, fd) = pair();
    let source = mio::Io::new(fd).unwrap();
    let mut bytes = [0; 1];
    let mut read = std::pin::pin!(source.read(&mut bytes));
    assert!(read.as_mut().poll(&mut context).is_pending());
    writer.write_all(b"x").unwrap();
    runtime.borrow_mut().poll(Some(Duration::ZERO)).unwrap();
    assert_eq!(calls.get(), 1);
    assert!(matches!(
        read.as_mut().poll(&mut context),
        Poll::Ready(Ok(Received { bytes: 1, fd })) if fd.is_none()
    ));
}

#[test]
fn tasks_can_spawn_owned_work_while_runtime_is_borrowed() {
    let runtime = Rc::new(RefCell::new(Some(mio::Runtime::new().unwrap())));
    let handle = runtime.borrow().as_ref().unwrap().handle();
    let owner = runtime.clone();
    let tasks = Rc::new(RefCell::new(Vec::new()));
    let retained = tasks.clone();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let _parent = handle
        .spawn(async move {
            assert!(owner.try_borrow_mut().is_err());
            retained.borrow_mut().push(
                mio::Handle::current()
                    .spawn(async move {
                        observed.set(1);
                    })
                    .unwrap(),
            );
        })
        .unwrap();
    assert_eq!(calls.get(), 0);
    runtime
        .borrow_mut()
        .as_mut()
        .unwrap()
        .poll(Some(Duration::ZERO))
        .unwrap();
    assert_eq!(calls.get(), 1);
    let owner = runtime.borrow_mut().take();
    drop(owner);
    assert!(!mio::Runtime::is_initialized());
    assert!(handle.spawn(async {}).is_err());
}

#[test]
fn cancellation_drops_unpolled_and_parked_futures_without_driving() {
    let mut runtime = mio::Runtime::new().unwrap();
    let handle = runtime.handle();
    let count = Rc::new(Cell::new(0));
    let spy = Dropped(count.clone());
    let task = handle
        .spawn(async move {
            let _spy = spy;
            panic!("cancelled before polling");
        })
        .unwrap();
    drop(task);
    assert_eq!(count.get(), 1);
    tick(&mut runtime);

    let (mut writer, fd) = pair();
    writer.set_nonblocking(true).unwrap();
    let source = mio::Io::new(fd).unwrap();
    let spy = Dropped(count.clone());
    let task = handle
        .spawn(async move {
            let _spy = spy;
            source.read(&mut [0; 1]).await.unwrap();
        })
        .unwrap();
    tick(&mut runtime);
    drop(task);
    assert_eq!(count.get(), 2);
    assert_eq!(writer.read(&mut [0; 1]).unwrap(), 0);
    // Deregistration errors are reported by the next driver turn.
    tick(&mut runtime);
}

#[test]
fn dropping_io_deregisters_before_closing_the_fd() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (mut writer, fd) = pair();
    let raw = fd.as_raw_fd();
    // Keep the open file description alive so closing the registered fd alone
    // cannot remove its kernel registration on epoll.
    let retained = fd.try_clone().unwrap();
    let source = mio::Io::new(fd).unwrap();
    drop(source);
    // SAFETY: F_GETFD only queries the descriptor number.
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
    tick(&mut runtime);

    // Restore the same number and open file description. Registration would
    // fail with EEXIST on epoll if the previous registration were still present.
    // SAFETY: retained is live and fcntl creates a new owned descriptor.
    let duplicate = unsafe { libc::fcntl(retained.as_raw_fd(), libc::F_DUPFD_CLOEXEC, raw) };
    assert!(duplicate >= 0, "{}", io::Error::last_os_error());
    // SAFETY: fcntl returned a new descriptor with no other owner.
    let fd = unsafe { OwnedFd::from_raw_fd(duplicate) };
    assert_eq!(fd.as_raw_fd(), raw);
    let source = mio::Io::new(fd).unwrap();
    writer.write_all(b"new").unwrap();
    tick(&mut runtime);
    assert!(matches!(poll(&mut read_bytes(&source, 3)), Poll::Ready(Ok(bytes)) if bytes == b"new"));
}

#[test]
fn self_waking_work_returns_control_and_cancelled_wakes_are_inert() {
    let mut runtime = mio::Runtime::new().unwrap();
    let polls = Rc::new(Cell::new(0));
    let saved = Rc::new(RefCell::new(None));
    let count = polls.clone();
    let wake = saved.clone();
    let task = runtime
        .handle()
        .spawn(poll_fn(move |cx| {
            count.set(count.get() + 1);
            *wake.borrow_mut() = Some(cx.local_waker().clone());
            for _ in 0..20 {
                cx.local_waker().wake_by_ref();
            }
            Poll::Pending
        }))
        .unwrap();
    tick(&mut runtime);
    let polled = polls.get();
    assert!(polled > 0);
    drop(task);
    saved.borrow().as_ref().unwrap().wake_by_ref();
    tick(&mut runtime);
    assert_eq!(polls.get(), polled);
}

#[test]
fn ordinary_waker_is_deliberately_inert() {
    let mut runtime = mio::Runtime::new().unwrap();
    let polls = Rc::new(Cell::new(0));
    let count = polls.clone();
    let _task = runtime
        .handle()
        .spawn(poll_fn(move |cx| {
            count.set(count.get() + 1);
            cx.waker().wake_by_ref();
            Poll::Pending
        }))
        .unwrap();
    tick(&mut runtime);
    tick(&mut runtime);
    assert_eq!(polls.get(), 1);
}

#[test]
fn self_cancellation_finishes_current_poll_then_drops_resources() {
    let mut runtime = mio::Runtime::new().unwrap();
    let owner = Rc::new(RefCell::new(None));
    let count = Rc::new(Cell::new(0));
    let spy = Dropped(count.clone());
    let slot = owner.clone();
    let observed = count.clone();
    *owner.borrow_mut() = Some(
        runtime
            .handle()
            .spawn(poll_fn(move |cx| {
                let _keep = &spy;
                slot.borrow_mut().take();
                assert_eq!(observed.get(), 0);
                cx.local_waker().wake_by_ref();
                Poll::Pending
            }))
            .unwrap(),
    );
    tick(&mut runtime);
    assert_eq!(count.get(), 1);
    tick(&mut runtime);
}

#[test]
fn future_destructors_can_cancel_other_tasks_and_spawn() {
    struct Cascade {
        child: Option<mio::Task>,
        handle: mio::Handle,
    }
    impl Drop for Cascade {
        fn drop(&mut self) {
            self.child.take();
            let transient = self.handle.spawn(async {}).unwrap();
            drop(transient);
        }
    }
    let runtime = mio::Runtime::new().unwrap();
    let count = Rc::new(Cell::new(0));
    let spy = Dropped(count.clone());
    let child = runtime
        .handle()
        .spawn(async move {
            let _spy = spy;
        })
        .unwrap();
    let cascade = Cascade {
        child: Some(child),
        handle: runtime.handle(),
    };
    let parent = runtime
        .handle()
        .spawn(async move {
            let _cascade = cascade;
        })
        .unwrap();
    drop(parent);
    assert_eq!(count.get(), 1);
}

#[test]
fn timers_wake_in_deadline_and_insertion_order() {
    let mut runtime = mio::Runtime::new().unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let deadline = Instant::now() + Duration::from_millis(15);
    let mut tasks = Vec::new();
    for i in 0..4 {
        let wait = mio::Sleep::new(deadline);
        let seen = seen.clone();
        tasks.push(
            runtime
                .handle()
                .spawn(async move {
                    wait.await.unwrap();
                    seen.borrow_mut().push(i);
                })
                .unwrap(),
        );
    }
    tick(&mut runtime);
    runtime.poll(Some(Duration::from_millis(100))).unwrap();
    assert_eq!(*seen.borrow(), [0, 1, 2, 3]);
}

fn read_bytes(
    source: &impl AsyncRead,
    limit: usize,
) -> Pin<Box<impl Future<Output = io::Result<Vec<u8>>> + '_>> {
    Box::pin(async move {
        let mut bytes = vec![0; limit];
        let count = source.read(&mut bytes).await?;
        assert!(count.fd.is_none());
        bytes.truncate(count.bytes);
        Ok(bytes)
    })
}

#[test]
fn partial_reads_retain_readiness_across_a_pause() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (mut writer, fd) = pair();
    let source = mio::Io::new(fd).unwrap();
    let mut wait = read_bytes(&source, 2);
    assert!(poll(&mut wait).is_pending());
    writer.write_all(b"first").unwrap();
    tick(&mut runtime);
    assert!(matches!(poll(&mut wait), Poll::Ready(Ok(bytes)) if bytes == b"fi"));
    drop(wait);
    tick(&mut runtime);
    assert!(
        matches!(poll(&mut read_bytes(&source, 32)), Poll::Ready(Ok(bytes)) if bytes == b"rst")
    );
    let mut wait = read_bytes(&source, 32);
    assert!(poll(&mut wait).is_pending());
    writer.write_all(b"new").unwrap();
    tick(&mut runtime);
    assert!(matches!(poll(&mut wait), Poll::Ready(Ok(bytes)) if bytes == b"new"));
}

#[test]
fn directional_waiter_conflicts_cancellation_and_wouldblock() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (mut writer, fd) = pair();
    let source = mio::Io::new(fd).unwrap();
    let mut first = read_bytes(&source, 1);
    assert!(poll(&mut first).is_pending());
    assert!(
        matches!(poll(&mut read_bytes(&source, 1)), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::AlreadyExists)
    );
    drop(first);
    let mut replacement = read_bytes(&source, 1);
    assert!(poll(&mut replacement).is_pending());
    tick(&mut runtime);
    assert!(matches!(
        poll(&mut Box::pin(source.write(&[IoSlice::new(b"w")], None))),
        Poll::Ready(Ok(1))
    ));
    writer.write_all(b"x").unwrap();
    tick(&mut runtime);
    assert!(matches!(poll(&mut replacement), Poll::Ready(Ok(bytes)) if bytes == b"x"));
    // Completed futures release the waiter even before they are dropped.
    let mut next = read_bytes(&source, 1);
    assert!(poll(&mut next).is_pending());
    writer.write_all(b"y").unwrap();
    tick(&mut runtime);
    assert!(matches!(poll(&mut next), Poll::Ready(Ok(bytes)) if bytes == b"y"));
}

#[test]
fn kernel_readiness_is_serviced_despite_a_self_waking_task() {
    let mut runtime = mio::Runtime::new().unwrap();
    let spinner = runtime
        .handle()
        .spawn(poll_fn(|cx| {
            cx.local_waker().wake_by_ref();
            Poll::Pending
        }))
        .unwrap();
    let (mut writer, fd) = pair();
    let source = mio::Io::new(fd).unwrap();
    let done = Rc::new(Cell::new(false));
    let mark = done.clone();
    let _reader = runtime
        .handle()
        .spawn(async move {
            source.read(&mut [0; 1]).await.unwrap();
            mark.set(true);
        })
        .unwrap();
    tick(&mut runtime);
    writer.write_all(b"x").unwrap();
    for _ in 0..4 {
        tick(&mut runtime);
    }
    assert!(done.get());
    drop(spinner);
}

#[test]
fn runtime_drop_releases_resources_and_invalidates_old_leaves() {
    let runtime = mio::Runtime::new().unwrap();
    let handle = runtime.handle();
    let (mut writer, fd) = pair();
    writer.set_nonblocking(true).unwrap();
    let source = mio::Io::new(fd).unwrap();
    let mut read = read_bytes(&source, 1);
    assert!(poll(&mut read).is_pending());
    let mut sleep = mio::Sleep::new(Instant::now());
    drop(runtime);
    assert_eq!(writer.read(&mut [0; 1]).unwrap(), 0);
    assert!(
        matches!(poll(&mut sleep), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe)
    );
    assert!(
        matches!(poll(&mut read), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe)
    );
    assert!(handle.spawn(async {}).is_err());
}

#[test]
fn panic_poisons_runtime_and_resets_drive_state() {
    let mut runtime = mio::Runtime::new().unwrap();
    let _task = runtime
        .handle()
        .spawn(async {
            panic!("probe");
        })
        .unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick(&mut runtime))).is_err());
    assert!(runtime.handle().spawn(async {}).is_err());
    assert!(runtime.poll(Some(Duration::ZERO)).is_err());
}

#[test]
fn descriptor_eof_is_explicit() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (writer, fd) = pair();
    let source = mio::Io::new(fd).unwrap();
    drop(writer);
    tick(&mut runtime);
    assert!(matches!(
        poll(&mut Box::pin(source.read(&mut [0; 1]))),
        Poll::Ready(Ok(Received { bytes: 0, fd })) if fd.is_none()
    ));
}
