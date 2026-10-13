use tiri_rt::{AsyncRead, AsyncWrite, Handle, Runtime, mio, unix};
use std::io::{IoSlice, Read, Write};
use std::os::fd::{AsFd, AsRawFd, IntoRawFd};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn borrowed_driver_waits_for_io_and_timers_without_retaining_its_future() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (reader, writer) = unix::socket_pair().unwrap();
    unix::set_nonblocking(reader.as_fd(), true).unwrap();
    unix::set_nonblocking(writer.as_fd(), true).unwrap();
    let reader = mio::Io::new(reader).unwrap();
    let writer = mio::Io::new(writer).unwrap();
    let _task = runtime
        .handle()
        .spawn(async move {
            mio::Sleep::new(Instant::now() + Duration::from_millis(5))
                .await
                .unwrap();
            writer
                .write(&[IoSlice::new(b"borrowed")], None)
                .await
                .unwrap();
        })
        .unwrap();
    let mut bytes = [0; 8];
    assert_eq!(
        runtime
            .block_on(reader.read(&mut bytes))
            .unwrap()
            .unwrap()
            .bytes,
        8
    );
    assert_eq!(&bytes, b"borrowed");
    assert_eq!(
        runtime
            .block_on(reader.read(&mut bytes))
            .unwrap()
            .unwrap()
            .bytes,
        0
    );
    // No root future remains registered after completion.
    runtime.poll(Some(Duration::ZERO)).unwrap();
}

#[test]
fn shutdown_write_preserves_incoming_data_and_close_consumes_ownership() {
    let (socket, peer) = unix::socket_pair().unwrap();
    assert!(!unix::set_nonblocking(socket.as_fd(), true).unwrap());
    let mut peer = UnixStream::from(peer);
    let mut runtime = mio::Runtime::new().unwrap();
    let source = mio::Io::new(socket.as_fd().try_clone_to_owned().unwrap()).unwrap();
    runtime
        .block_on(source.write(&[IoSlice::new(b"input")], None))
        .unwrap()
        .unwrap();
    unix::shutdown_write(socket.as_fd()).unwrap();
    let mut sent = Vec::new();
    peer.read_to_end(&mut sent).unwrap();
    assert_eq!(sent, b"input");
    peer.write_all(b"reply").unwrap();
    let mut reply = [0; 5];
    assert_eq!(
        runtime
            .block_on(source.read(&mut reply))
            .unwrap()
            .unwrap()
            .bytes,
        5
    );
    assert_eq!(&reply, b"reply");
    drop(source);
    let raw = socket.into_raw_fd();
    // SAFETY: the registration was dropped and ownership has been relinquished.
    unsafe { unix::close(raw) }.unwrap();
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
}

#[test]
fn redirecting_onto_the_same_descriptor_clears_close_on_exec() {
    let fd = unix::open(c"/dev/null", libc::O_WRONLY, 0).unwrap();
    assert_ne!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    // SAFETY: same live descriptor; no other owner is replaced.
    unsafe { unix::redirect(fd.as_fd(), fd.as_raw_fd()) }.unwrap();
    assert_eq!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
}

#[test]
fn socket_paths_and_terminal_queries_retain_borrowed_descriptors() {
    use std::ffi::{CStr, CString};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    let path = std::env::temp_dir().join(format!("tiri-rt-path-{}", std::process::id()));
    let listener = unix::listen(&path, 8).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        unix::socket_path(listener.as_fd()).unwrap(),
        Some(CString::new(path.as_os_str().as_bytes()).unwrap())
    );
    let (socket, _peer) = unix::socket_pair().unwrap();
    assert_eq!(unix::socket_path(socket.as_fd()).unwrap(), None);
    assert!(unix::terminal_name(socket.as_fd()).is_err());
    assert!(unix::terminal_foreground_group(socket.as_fd()).is_err());

    let (mut master, mut slave) = (-1, -1);
    let mut name = [0; 1024];
    // SAFETY: openpty initializes both descriptors and its bounded PTY name.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                name.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: openpty transferred both descriptors on success.
    let (_master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    let owned = unix::terminal_name(slave.as_fd()).unwrap();
    assert_eq!(owned, name);
    assert!(unix::terminal_attributes(slave.as_fd()).is_ok());
    assert_eq!(unix::terminal_name(slave.as_fd()).unwrap(), owned);
}
