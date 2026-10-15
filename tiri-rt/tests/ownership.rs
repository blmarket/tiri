//! Byte I/O owns registration, directional waits, and descriptor cleanup.
use std::cell::Cell;
use std::future::Future;
use std::io::{self, IoSlice, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tiri_rt::{AsyncRead as _, AsyncWrite as _, Handle as _, Received, Runtime as _, mio};

fn assert_closed(raw: RawFd) {
    // SAFETY: F_GETFD only queries the descriptor number.
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
}

#[test]
fn cancelled_reads_preserve_input_and_partial_reads_need_no_new_edge() {
    let mut rt = mio::Runtime::new().unwrap();
    let (mut peer, fd) = UnixStream::pair().unwrap();
    fd.set_nonblocking(true).unwrap();
    let source = Rc::new(mio::Io::new(fd.into()).unwrap());
    let s = source.clone();
    let cancelled = rt
        .handle()
        .spawn(async move {
            s.read(&mut [0; 1]).await.unwrap();
        })
        .unwrap();
    rt.poll(Some(Duration::ZERO)).unwrap();
    drop(cancelled);
    peer.write_all(b"still present").unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let task = rt
        .handle()
        .spawn(async move {
            for expected in b"st" {
                let mut byte = [0; 1];
                assert_eq!(source.read(&mut byte).await.unwrap().bytes, 1);
                assert_eq!(byte[0], *expected);
                observed.set(observed.get() + 1);
            }
        })
        .unwrap();
    rt.poll(Some(Duration::ZERO)).unwrap();
    assert_eq!(calls.get(), 2);
    drop(task);
}

#[test]
fn concurrent_reads_share_one_waiter_and_cancellation_releases_it() {
    let mut rt = mio::Runtime::new().unwrap();
    let (mut peer, fd) = UnixStream::pair().unwrap();
    fd.set_nonblocking(true).unwrap();
    let source = mio::Io::new(fd.into()).unwrap();
    let mut context = Context::from_waker(Waker::noop());
    let mut first_buffer = [0; 1];
    let mut wait = Box::pin(source.read(&mut first_buffer));
    assert!(wait.as_mut().poll(&mut context).is_pending());
    let mut byte = [0; 1];
    assert!(matches!(
        pin!(source.read(&mut byte)).as_mut().poll(&mut context),
        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::AlreadyExists
    ));
    drop(wait);
    peer.write_all(b"x").unwrap();
    rt.poll(Some(Duration::ZERO)).unwrap();
    assert!(matches!(
        pin!(source.read(&mut byte)).as_mut().poll(&mut context),
        Poll::Ready(Ok(Received { bytes: 1, fd })) if fd.is_none()
    ));
    assert_eq!(&byte, b"x");
}

#[test]
fn io_after_runtime_shutdown_panics_and_closes_fd() {
    let rt = mio::Runtime::new().unwrap();
    let (mut peer, fd) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    fd.set_nonblocking(true).unwrap();
    let raw = fd.as_raw_fd();
    drop(rt);
    assert!(std::panic::catch_unwind(|| mio::Io::new(fd.into())).is_err());
    assert_closed(raw);
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn registered_descriptor_closes_on_runtime_shutdown_and_invalidates_pending_waits() {
    let rt = mio::Runtime::new().unwrap();
    let (mut peer, fd) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    fd.set_nonblocking(true).unwrap();
    let source = mio::Io::new(fd.into()).unwrap();
    let mut bytes = [0; 1];
    let mut wait = pin!(source.read(&mut bytes));
    assert!(
        wait.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(rt);
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    assert!(matches!(
        wait.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe
    ));
}

#[test]
fn registered_descriptor_closes_on_drop() {
    let mut rt = mio::Runtime::new().unwrap();
    let (mut peer, fd) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    fd.set_nonblocking(true).unwrap();
    let source = mio::Io::new(fd.into()).unwrap();
    drop(source);
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
    // A deregistration attempted after close would report an error here.
    rt.poll(Some(Duration::ZERO)).unwrap();
}

#[test]
fn rejected_descriptor_is_closed() {
    let _rt = mio::Runtime::new().unwrap();
    let (mut peer, fd) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    assert!(matches!(mio::Io::new(fd.into()), Err(e) if e.kind() == io::ErrorKind::InvalidInput));
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn immediate_device_supports_io_and_closes() {
    use std::os::unix::fs::OpenOptionsExt;

    let mut rt = mio::Runtime::new().unwrap();
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/null")
        .unwrap();
    let raw = fd.as_raw_fd();
    let source = mio::Io::new(fd.into()).unwrap();
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(
        pin!(source.write(&[IoSlice::new(b"discard")], None)).poll(&mut context),
        Poll::Ready(Ok(7))
    ));
    assert!(matches!(
        pin!(source.read(&mut [0; 1])).poll(&mut context),
        Poll::Ready(Ok(Received { bytes: 0, fd })) if fd.is_none()
    ));
    drop(source);
    assert_closed(raw);
    rt.poll(Some(Duration::ZERO)).unwrap();
}

#[test]
fn regular_file_io_preserves_offsets_eof_and_runtime_ownership() {
    use std::io::{Seek, SeekFrom};
    use std::os::fd::AsFd;
    let path = std::env::temp_dir().join(format!("tiri-rt-file-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::remove_file(path).unwrap();
    file.write_all(b"input").unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut observer = file.try_clone().unwrap();
    let raw = file.as_raw_fd();
    let rt = mio::Runtime::new().unwrap();
    // Regular files do not need O_NONBLOCK, which cannot prevent disk waits.
    let source = mio::Io::new(file.into()).unwrap();
    let mut context = Context::from_waker(Waker::noop());
    let fd = std::fs::File::open("/dev/null").unwrap();
    for buffers in [&[][..], &[IoSlice::new(b"rejected")][..]] {
        assert!(matches!(
            pin!(source.write(buffers, Some(fd.as_fd()))).poll(&mut context),
            Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Unsupported
        ));
    }
    assert_eq!(observer.stream_position().unwrap(), 0);
    let mut bytes = [0; 5];
    drop(Box::pin(source.read(&mut bytes)));
    drop(Box::pin(source.write(&[IoSlice::new(b"cancelled")], None)));
    assert_eq!(observer.stream_position().unwrap(), 0);
    assert!(matches!(
        pin!(source.read(&mut bytes)).poll(&mut context),
        Poll::Ready(Ok(Received { bytes: 5, fd })) if fd.is_none()
    ));
    assert_eq!(&bytes, b"input");
    assert!(matches!(
        pin!(source.read(&mut bytes)).poll(&mut context),
        Poll::Ready(Ok(Received { bytes: 0, fd })) if fd.is_none()
    ));
    assert!(matches!(
        pin!(source.write(&[IoSlice::new(b"output")], None)).poll(&mut context),
        Poll::Ready(Ok(6))
    ));
    observer.seek(SeekFrom::Start(0)).unwrap();
    let mut contents = Vec::new();
    observer.read_to_end(&mut contents).unwrap();
    assert_eq!(contents, b"inputoutput");
    drop(rt);
    assert_closed(raw);
    assert!(
        matches!(pin!(source.read(&mut bytes)).poll(&mut context), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe)
    );
    assert!(
        matches!(pin!(source.write(&[IoSlice::new(b"closed")], None)).poll(&mut context), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe)
    );
}
