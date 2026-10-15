//! FD passing is part of ordinary I/O, including one-way adapters.
use std::future::{Future, poll_fn};
use std::io::{self, IoSlice, Read, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::pin::{Pin, pin};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use tiri_rt::stream::{Reader, Writer};
use tiri_rt::{AsyncRead, AsyncWrite, Runtime, mio};

fn tick(runtime: &mut mio::Runtime) {
    runtime.poll(Some(Duration::ZERO)).unwrap();
}

fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

fn ready<T>(runtime: &mut mio::Runtime, future: impl Future<Output = T>) -> T {
    let timeout = mio::Sleep::new(Instant::now() + Duration::from_secs(5));
    // PTY delivery can require kernel worker scheduling. Wait for readiness
    // with a real deadline instead of exhausting a fixed number of busy polls.
    runtime
        .block_on(async {
            let mut future = pin!(future);
            let mut timeout = pin!(timeout);
            poll_fn(|cx| {
                if let Poll::Ready(value) = future.as_mut().poll(cx) {
                    return Poll::Ready(value);
                }
                assert!(
                    timeout.as_mut().poll(cx).is_pending(),
                    "operation did not complete"
                );
                Poll::Pending
            })
            .await
        })
        .unwrap()
}

fn pair() -> (UnixStream, UnixStream) {
    let (left, right) = UnixStream::pair().unwrap();
    left.set_nonblocking(true).unwrap();
    right.set_nonblocking(true).unwrap();
    (left, right)
}

fn streams() -> (mio::Io, mio::Io) {
    let (left, right) = pair();
    (
        mio::Io::new(left.into()).unwrap(),
        mio::Io::new(right.into()).unwrap(),
    )
}

#[test]
fn vectored_transfer_returns_an_owned_fd_and_eof() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (sender, receiver) = streams();
    let sender = Writer::from_io(sender);
    let receiver = Reader::from_io(receiver);
    let (fd, mut peer) = pair();
    let buffers = [IoSlice::new(b"header"), IoSlice::new(b"payload")];
    assert_eq!(
        ready(&mut runtime, sender.write(&buffers, Some(fd.as_fd()))).unwrap(),
        13
    );
    drop(fd);

    let mut bytes = [0; 32];
    let received = ready(&mut runtime, receiver.read(&mut bytes)).unwrap();
    assert_eq!(&bytes[..received.bytes], b"headerpayload");
    let fd = received.fd.unwrap();
    // SAFETY: F_GETFD queries the live received descriptor.
    assert_ne!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    let mut transferred = UnixStream::from(fd);
    transferred.write_all(b"x").unwrap();
    let mut byte = [0];
    peer.read_exact(&mut byte).unwrap();
    assert_eq!(byte, *b"x");
    drop(transferred);
    assert_eq!(peer.read(&mut byte).unwrap(), 0);
    drop(sender);
    let eof = ready(&mut runtime, receiver.read(&mut bytes)).unwrap();
    assert_eq!(eof.bytes, 0);
    assert!(eof.fd.is_none());
}

#[test]
fn cancelled_read_preserves_fd_and_chunk_reader_returns_it() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (sender, receiver) = streams();
    let mut receiver = Reader::from_io(receiver);
    let mut pending = Box::pin(receiver.read_chunk(8));
    assert!(poll(pending.as_mut()).is_pending());
    let (fd, mut peer) = pair();
    ready(
        &mut runtime,
        sender.write(&[IoSlice::new(b"input")], Some(fd.as_fd())),
    )
    .unwrap();
    drop(fd);
    tick(&mut runtime);
    // Notification has arrived but the pending read has not consumed anything.
    drop(pending);
    let (bytes, fd) = ready(&mut runtime, receiver.read_chunk(8))
        .unwrap()
        .unwrap();
    assert_eq!(bytes, b"input");
    assert!(fd.is_some());
    drop(fd);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    drop(sender);
    assert!(
        ready(&mut runtime, receiver.read_chunk(8))
            .unwrap()
            .is_none()
    );
}

#[test]
fn excess_and_truncated_fds_are_rejected_and_closed() {
    const CONTROL_SIZE: usize =
        unsafe { libc::CMSG_SPACE((3 * size_of::<libc::c_int>()) as _) as usize };
    #[repr(C)]
    union Control {
        _align: libc::cmsghdr,
        bytes: [u8; CONTROL_SIZE],
    }

    let mut runtime = mio::Runtime::new().unwrap();
    // On 64-bit Linux, two FDs fit in the padding for one without MSG_CTRUNC;
    // three force truncation. Both cases must close every received descriptor.
    for count in [2, 3] {
        let (sender, receiver) = pair();
        let receiver = mio::Io::new(receiver.into()).unwrap();
        let (fd, mut peer) = pair();
        let mut control = Control {
            bytes: [0; CONTROL_SIZE],
        };
        let mut byte = b'x';
        let mut iov = libc::iovec {
            iov_base: (&mut byte as *mut u8).cast(),
            iov_len: 1,
        };
        // Bypass AsyncWrite to simulate a peer violating its single-FD contract.
        // SAFETY: zero initializes unused msghdr fields. The initialized iov
        // and aligned control buffer outlive sendmsg; the FD stays borrowed.
        unsafe {
            let mut msg: libc::msghdr = std::mem::zeroed();
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = (&mut control as *mut Control).cast();
            let bytes = (count * size_of::<libc::c_int>()) as _;
            msg.msg_controllen = libc::CMSG_SPACE(bytes) as _;
            let header = libc::CMSG_FIRSTHDR(&msg);
            (*header).cmsg_len = libc::CMSG_LEN(bytes) as _;
            (*header).cmsg_level = libc::SOL_SOCKET;
            (*header).cmsg_type = libc::SCM_RIGHTS;
            let data = libc::CMSG_DATA(header).cast::<libc::c_int>();
            for index in 0..count {
                data.add(index).write_unaligned(fd.as_raw_fd());
            }
            assert_eq!(libc::sendmsg(sender.as_raw_fd(), &msg, 0), 1);
        }
        drop(fd);
        let error = ready(&mut runtime, receiver.read(&mut [0; 8])).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        // Kernel-discarded and userspace-adopted FDs must all be closed.
        assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    }
}

#[test]
fn consecutive_single_fd_writes_remain_readable() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (sender, receiver) = streams();
    let (first, mut first_peer) = pair();
    let (second, mut second_peer) = pair();
    for (byte, fd) in [(b"a", &first), (b"b", &second)] {
        assert_eq!(
            ready(
                &mut runtime,
                sender.write(&[IoSlice::new(byte)], Some(fd.as_fd()))
            )
            .unwrap(),
            1
        );
    }
    drop((first, second));
    for (expected, peer) in [(b'a', &mut first_peer), (b'b', &mut second_peer)] {
        let mut bytes = [0; 16];
        let received = ready(&mut runtime, receiver.read(&mut bytes)).unwrap();
        assert_eq!(&bytes[..received.bytes], &[expected]);
        let mut transferred = UnixStream::from(received.fd.unwrap());
        transferred.write_all(&[expected]).unwrap();
        let mut byte = [0];
        peer.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [expected]);
        drop(transferred);
        assert_eq!(peer.read(&mut byte).unwrap(), 0);
    }
}

#[test]
fn invalid_writes_and_empty_reads_do_not_consume_data_or_fds() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (sender, receiver) = streams();
    let (fd, mut peer) = pair();
    for buffers in [&[][..], &[IoSlice::new(b"")][..]] {
        let error = ready(&mut runtime, sender.write(buffers, Some(fd.as_fd()))).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
    assert_eq!(ready(&mut runtime, sender.write(&[], None)).unwrap(), 0);
    assert!(poll(pin!(receiver.read(&mut [0; 32]))).is_pending());
    ready(
        &mut runtime,
        sender.write(&[IoSlice::new(b"x")], Some(fd.as_fd())),
    )
    .unwrap();
    drop(fd);
    let empty = ready(&mut runtime, receiver.read(&mut [])).unwrap();
    assert_eq!(empty.bytes, 0);
    assert!(empty.fd.is_none());
    let mut byte = [0];
    let received = ready(&mut runtime, receiver.read(&mut byte)).unwrap();
    assert_eq!(received.bytes, 1);
    assert_eq!(byte, *b"x");
    assert!(received.fd.is_some());
    drop(received);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
}

#[test]
fn disconnected_socket_reports_error_and_shutdown_invalidates_pending_read() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (sender, receiver) = streams();
    drop(receiver);
    let (fd, mut peer) = pair();
    assert_eq!(
        ready(
            &mut runtime,
            sender.write(&[IoSlice::new(b"x")], Some(fd.as_fd()))
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::BrokenPipe
    );
    // The failed write neither closes nor transfers the borrowed FD.
    assert_eq!(
        peer.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(fd);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);

    let (_sender, receiver) = streams();
    let mut bytes = [0];
    let mut pending = pin!(receiver.read(&mut bytes));
    assert!(poll(pending.as_mut()).is_pending());
    drop(runtime);
    assert!(
        matches!(poll(pending.as_mut()), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe)
    );
}

fn nonblocking(fd: &OwnedFd) {
    // SAFETY: fd owns a live descriptor, and fcntl only adjusts status flags.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0);
    assert_eq!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
}

fn pipe() -> (OwnedFd, OwnedFd) {
    let mut fds = [-1; 2];
    // SAFETY: fds is storage for two output descriptors.
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: successful pipe returned two newly owned descriptors.
    let (reader, writer) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    nonblocking(&reader);
    nonblocking(&writer);
    (writer, reader)
}

fn pty() -> (OwnedFd, OwnedFd) {
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: valid descriptor outputs; null optional arguments request defaults.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: openpty returned two newly owned descriptors.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: slave is live and termios is valid output storage.
    assert_eq!(
        unsafe { libc::tcgetattr(slave.as_raw_fd(), termios.as_mut_ptr()) },
        0
    );
    // SAFETY: successful tcgetattr initialized termios.
    let mut termios = unsafe { termios.assume_init() };
    // SAFETY: initialized termios and a live PTY slave.
    unsafe {
        libc::cfmakeraw(&mut termios);
        assert_eq!(
            libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &termios),
            0
        );
    }
    nonblocking(&master);
    nonblocking(&slave);
    (master, slave)
}

#[test]
fn pipes_and_ptys_use_the_same_traits_and_reject_fds_before_writing() {
    let mut runtime = mio::Runtime::new().unwrap();
    let fd = std::fs::File::open("/dev/null").unwrap();
    for (writer, reader) in [pipe(), pty()] {
        let writer = Writer::new(writer).unwrap();
        let reader = Reader::new(reader).unwrap();
        for buffers in [&[][..], &[IoSlice::new(b"rejected")][..]] {
            // Rejection must not wait for writable readiness either.
            assert!(
                matches!(poll(pin!(writer.write(buffers, Some(fd.as_fd())))),
                Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Unsupported)
            );
        }
        let mut buffer = [0; 64];
        assert!(poll(pin!(reader.read(&mut buffer))).is_pending());
        assert_eq!(
            ready(
                &mut runtime,
                writer.write(&[IoSlice::new(b"hel"), IoSlice::new(b"lo")], None)
            )
            .unwrap(),
            5
        );
        let received = ready(&mut runtime, reader.read(&mut buffer)).unwrap();
        assert_eq!(&buffer[..received.bytes], b"hello");
        assert!(received.fd.is_none());
        assert!(poll(pin!(reader.read(&mut buffer))).is_pending());
    }
}
