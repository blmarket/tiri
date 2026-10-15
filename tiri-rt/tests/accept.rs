use std::future::Future;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tiri_rt::{AsyncAccept, AsyncRead, Runtime, mio};

struct SocketPath(PathBuf);

impl SocketPath {
    fn bind() -> (Self, UnixListener) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = Self(std::env::temp_dir().join(format!(
            "tiri-rt-accept-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        )));
        let listener = UnixListener::bind(&path.0).unwrap();
        (path, listener)
    }
}

impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn complete<T>(runtime: &mut mio::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    for _ in 0..100 {
        runtime.poll(Some(Duration::ZERO)).unwrap();
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("operation did not complete");
}

#[test]
fn cancelled_accept_preserves_connections_and_accepts_configure_owned_sockets() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (path, socket) = SocketPath::bind();
    socket.set_nonblocking(true).unwrap();
    let listener = mio::Listener::new(socket).unwrap();
    let mut pending = Box::pin(listener.accept());
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    let mut peers: Vec<_> = (0..2)
        .map(|_| UnixStream::connect(&path.0).unwrap())
        .collect();
    runtime.poll(Some(Duration::ZERO)).unwrap();
    drop(pending);
    for (index, peer) in peers.iter_mut().enumerate() {
        peer.write_all(&[index as u8]).unwrap();
        let fd: OwnedFd = complete(&mut runtime, listener.accept()).unwrap();
        // SAFETY: these calls only query the newly accepted, live descriptor.
        assert_ne!(
            unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
            0
        );
        assert_ne!(
            unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        let source = mio::Io::new(fd).unwrap();
        let mut byte = [0];
        assert_eq!(
            complete(&mut runtime, source.read(&mut byte))
                .unwrap()
                .bytes,
            1
        );
        assert_eq!(byte, [index as u8]);
    }
}

#[test]
fn accepts_have_one_waiter_and_runtime_shutdown_closes_the_listener() {
    let runtime = mio::Runtime::new().unwrap();
    let (_path, socket) = SocketPath::bind();
    socket.set_nonblocking(true).unwrap();
    let raw = socket.as_raw_fd();
    let listener = mio::Listener::new(socket).unwrap();
    let mut first = Box::pin(listener.accept());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(matches!(pin!(listener.accept()).poll(&mut cx),
        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::AlreadyExists));
    drop(runtime);
    // SAFETY: F_GETFD only queries this descriptor number.
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    assert!(matches!(first.as_mut().poll(&mut cx),
        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe));
}

#[test]
fn blocking_listener_is_rejected_and_closed() {
    let _runtime = mio::Runtime::new().unwrap();
    let (_path, socket) = SocketPath::bind();
    let raw = socket.as_raw_fd();
    assert!(matches!(mio::Listener::new(socket),
        Err(e) if e.kind() == io::ErrorKind::InvalidInput));
    // SAFETY: F_GETFD only queries this descriptor number.
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
}
