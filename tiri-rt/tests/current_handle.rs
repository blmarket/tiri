#![feature(local_waker)]

use std::cell::Cell;
use std::future::Future;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{ContextBuilder, LocalWake, LocalWaker, Waker};
use std::time::{Duration, Instant};
use tiri_rt::{Handle, Io, Runtime, Sleep};

#[test]
fn constructors_work_while_the_runtime_is_borrowed() {
    let mut runtime = Runtime::new().unwrap();
    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    writer.write_all(b"x").unwrap();
    runtime
        .block_on(async {
            let source = Io::new(reader.into()).unwrap();
            let mut bytes = [0; 1];
            assert_eq!(source.read(&mut bytes).await.unwrap().bytes, 1);
            assert_eq!(bytes, *b"x");
            Sleep::new(Instant::now() + Duration::from_millis(5))
                .await
                .unwrap();
        })
        .unwrap();
}

#[test]
fn entering_an_independent_runtime_binds_resources_and_restores_the_default() {
    let mut runtime = Runtime::new().unwrap();
    let mut independent = Runtime::new().unwrap();
    let (reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let (source, wait) = independent.enter(|| {
        (
            Io::new(reader.into()).unwrap(),
            Sleep::new(Instant::now() + Duration::from_millis(5)),
        )
    });
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let _task = Handle::current()
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
    let mut runtime = Runtime::new().unwrap();
    let mut independent = Runtime::new().unwrap();
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let _task = independent.enter(|| {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.enter(|| panic!("scope panic"));
        }));
        assert!(result.is_err());
        Handle::current()
            .spawn(async move { observed.set(true) })
            .unwrap()
    });
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(!called.get());
    independent.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());

    drop(independent);
    called.set(false);
    let observed = called.clone();
    let _task = Handle::current()
        .spawn(async move { observed.set(true) })
        .unwrap();
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
    drop(runtime);
    assert!(!Runtime::is_initialized());
}

#[test]
#[should_panic(expected = "runtime initialized")]
fn current_handle_panics_before_initialization() {
    Handle::current();
}

#[test]
#[should_panic(expected = "runtime initialized")]
fn current_handle_panics_after_shutdown() {
    let runtime = Runtime::new().unwrap();
    drop(runtime);
    Handle::current();
}

#[test]
fn initialization_registers_the_current_handle_on_its_thread() {
    assert!(!Runtime::is_initialized());
    let mut runtime = Runtime::new().unwrap();
    assert!(Runtime::is_initialized());

    std::thread::spawn(|| {
        assert!(!Runtime::is_initialized());
        let runtime = Runtime::new().unwrap();
        assert!(Runtime::is_initialized());
        drop(runtime);
        assert!(!Runtime::is_initialized());
    })
    .join()
    .unwrap();

    let mut independent = Runtime::new().unwrap();
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let task = independent
        .block_on(async {
            Handle::current()
                .spawn(async move { observed.set(true) })
                .unwrap()
        })
        .unwrap();
    independent.poll(Some(Duration::ZERO)).unwrap();
    assert!(!called.get());
    drop(independent);
    assert!(Runtime::is_initialized());

    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
    drop(task);
    drop(runtime);
    assert!(!Runtime::is_initialized());
}

#[test]
fn dropping_an_auxiliary_runtime_preserves_a_new_registration() {
    let old = Runtime::new().unwrap();
    let auxiliary = Runtime::new().unwrap();
    drop(old);
    assert!(!Runtime::is_initialized());

    let mut runtime = Runtime::new().unwrap();
    drop(auxiliary);

    let handle = Handle::current();
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let _task = handle.spawn(async move { observed.set(true) }).unwrap();
    runtime.poll(Some(Duration::ZERO)).unwrap();
    assert!(called.get());
    drop(runtime);
    assert!(!Runtime::is_initialized());
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
            assert!(Runtime::is_initialized());
            let handle = Handle::current();
            assert!(matches!(
                handle.spawn(async {}),
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe
            ));
            self.0.set(true);
        }
    }

    let runtime = Runtime::new().unwrap();
    let dropped = Rc::new(Cell::new(false));
    let mut sleep = Sleep::new(Instant::now() + Duration::from_secs(3600));
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
    assert!(!Runtime::is_initialized());
}
