#![feature(local_waker)]

use tiri_rt::{AsyncRead as _, Handle as _, Runtime as _, mio};
use std::cell::Cell;
use std::future::Future;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{ContextBuilder, LocalWake, LocalWaker, Waker};
use std::time::{Duration, Instant};

#[test]
fn constructors_work_while_the_runtime_is_borrowed() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    writer.write_all(b"x").unwrap();
    runtime
        .block_on(async {
            let source = mio::Io::new(reader.into()).unwrap();
            let mut bytes = [0; 1];
            assert_eq!(source.read(&mut bytes).await.unwrap().bytes, 1);
            assert_eq!(bytes, *b"x");
            mio::Sleep::new(Instant::now() + Duration::from_millis(5))
                .await
                .unwrap();
        })
        .unwrap();
}

#[test]
fn entering_an_independent_runtime_binds_resources_and_restores_the_default() {
    let mut runtime = mio::Runtime::new().unwrap();
    let mut independent = mio::Runtime::new().unwrap();
    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let (source, wait) = independent.enter(|| {
        (
            mio::Io::new(reader.into()).unwrap(),
            mio::Sleep::new(Instant::now() + Duration::from_millis(5)),
        )
    });
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let _task = mio::Handle::current()
        .spawn(async move { observed.set(true) })
        .unwrap();
    writer.write_all(b"x").unwrap();
    independent
        .block_on(async {
            let mut bytes = [0; 1];
            assert_eq!(source.read(&mut bytes).await.unwrap().bytes, 1);
            wait.await.unwrap();
        })
        .unwrap();
    assert!(!called.get());
    drop(independent);
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
}

#[test]
fn nested_runtime_scopes_restore_the_previous_handle_after_unwinding() {
    let mut runtime = mio::Runtime::new().unwrap();
    let mut independent = mio::Runtime::new().unwrap();
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let _task = independent.enter(|| {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.enter(|| panic!("scope panic"));
        }));
        assert!(result.is_err());
        mio::Handle::current()
            .spawn(async move { observed.set(true) })
            .unwrap()
    });
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(!called.get());
    independent.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());

    // The default is restored after leaving the outer scope as well.
    drop(independent);
    called.set(false);
    let observed = called.clone();
    let _task = mio::Handle::current()
        .spawn(async move { observed.set(true) })
        .unwrap();
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
    drop(runtime);
    assert!(!mio::Runtime::is_initialized());
}

#[test]
#[should_panic(expected = "runtime initialized")]
fn current_handle_panics_before_initialization() {
    mio::Handle::current();
}

#[test]
#[should_panic(expected = "runtime initialized")]
fn current_handle_panics_after_shutdown() {
    let runtime = mio::Runtime::new().unwrap();
    drop(runtime);
    mio::Handle::current();
}

#[test]
fn initialization_registers_the_current_handle_on_its_thread() {
    assert!(!mio::Runtime::is_initialized());
    let mut runtime = mio::Runtime::new().unwrap();
    assert!(mio::Runtime::is_initialized());

    std::thread::spawn(|| {
        assert!(!mio::Runtime::is_initialized());
        let runtime = mio::Runtime::new().unwrap();
        assert!(mio::Runtime::is_initialized());
        drop(runtime);
        assert!(!mio::Runtime::is_initialized());
    })
    .join()
    .unwrap();

    let mut independent = mio::Runtime::new().unwrap();
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let task = independent
        .block_on(async {
            mio::Handle::current()
                .spawn(async move { observed.set(true) })
                .unwrap()
        })
        .unwrap();
    independent.poll(Some(Duration::ZERO)).unwrap();
    assert!(!called.get());
    drop(independent);
    assert!(mio::Runtime::is_initialized());

    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
    drop(task);
    drop(runtime);
    assert!(!mio::Runtime::is_initialized());
}

#[test]
fn dropping_an_auxiliary_runtime_preserves_a_new_registration() {
    let old = mio::Runtime::new().unwrap();
    let auxiliary = mio::Runtime::new().unwrap();
    drop(old);
    assert!(!mio::Runtime::is_initialized());

    let mut runtime = mio::Runtime::new().unwrap();
    drop(auxiliary);

    let handle = mio::Handle::current();
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let _task = handle.spawn(async move { observed.set(true) }).unwrap();
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
    drop(runtime);
    assert!(!mio::Runtime::is_initialized());
    assert!(matches!(
        handle.spawn(async {}),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe
    ));
}

#[test]
fn current_handle_remains_accessible_during_runtime_cleanup() {
    struct Wake(Rc<Cell<bool>>);

    impl LocalWake for Wake {
        fn wake(self: Rc<Self>) {}
    }

    impl Drop for Wake {
        fn drop(&mut self) {
            // Shutdown has invalidated scheduling, but lookup must still work.
            assert!(mio::Runtime::is_initialized());
            let handle = mio::Handle::current();
            assert!(matches!(
                handle.spawn(async {}),
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe
            ));
            self.0.set(true);
        }
    }

    let runtime = mio::Runtime::new().unwrap();
    let dropped = Rc::new(Cell::new(false));
    let mut sleep = mio::Sleep::new(Instant::now() + Duration::from_secs(3600));
    {
        let wake = LocalWaker::from(Rc::new(Wake(dropped.clone())));
        let mut context = ContextBuilder::from_waker(Waker::noop())
            .local_waker(&wake)
            .build();
        assert!(Pin::new(&mut sleep).poll(&mut context).is_pending());
    }
    assert!(!dropped.get());
    drop(runtime);
    assert!(dropped.get());
    assert!(!mio::Runtime::is_initialized());
}
