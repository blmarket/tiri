//! Byte-stream adapter contracts over the local runtime.
use std::cell::RefCell;
use std::future::Future;
use std::io::{self, IoSlice, Write};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tiri_rt::stream::Reader;
use tiri_rt::{AsyncRead, AsyncWrite, Handle, Received, Runtime, mio};

fn tick(runtime: &mut mio::Runtime) {
    runtime.poll(Some(Duration::ZERO)).unwrap();
}

fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn chunks_drain_before_eof_and_wake_after_would_block() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (mut writer, socket) = UnixStream::pair().unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut reader = Reader::new(socket.into()).unwrap();
    let chunks = Rc::new(RefCell::new(Vec::new()));
    let output = chunks.clone();
    let _task = runtime
        .handle()
        .spawn(async move {
            loop {
                let chunk = reader.read_chunk(2).await.unwrap();
                let eof = chunk.is_none();
                output.borrow_mut().push(chunk.map(|(bytes, fd)| {
                    assert!(fd.is_none());
                    bytes
                }));
                if eof {
                    break;
                }
            }
        })
        .unwrap();
    tick(&mut runtime);
    assert!(chunks.borrow().is_empty());
    writer.write_all(b"abc").unwrap();
    tick(&mut runtime);
    assert_eq!(
        *chunks.borrow(),
        vec![Some(b"ab".to_vec()), Some(b"c".to_vec())]
    );
    writer.write_all(b"de").unwrap();
    drop(writer);
    tick(&mut runtime);
    assert_eq!(
        *chunks.borrow(),
        vec![
            Some(b"ab".to_vec()),
            Some(b"c".to_vec()),
            Some(b"de".to_vec()),
            None
        ]
    );
}

#[test]
fn cancelling_a_pending_read_leaves_bytes_for_the_next_read() {
    let mut runtime = mio::Runtime::new().unwrap();
    let (mut writer, socket) = UnixStream::pair().unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut reader = Reader::new(socket.into()).unwrap();
    let mut pending = Box::pin(reader.read_chunk(8));
    assert!(poll(pending.as_mut()).is_pending());
    writer.write_all(b"hello").unwrap();
    tick(&mut runtime);
    drop(pending);
    let mut next = Box::pin(reader.read_chunk(8));
    assert!(
        matches!(poll(next.as_mut()), Poll::Ready(Ok(Some((bytes, fd)))) if bytes == b"hello" && fd.is_none())
    );
}

#[test]
fn validates_sizes_and_propagates_runtime_shutdown() {
    let runtime = mio::Runtime::new().unwrap();
    let (_writer, socket) = UnixStream::pair().unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut reader = Reader::new(socket.into()).unwrap();
    assert!(
        matches!(poll(Box::pin(reader.read_chunk(0)).as_mut()), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(matches!(
        poll(Box::pin(reader.read(&mut [])).as_mut()),
        Poll::Ready(Ok(Received { bytes: 0, fd })) if fd.is_none()
    ));
    drop(runtime);
    assert!(
        matches!(poll(Box::pin(reader.read_chunk(8)).as_mut()), Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::BrokenPipe)
    );
}

#[test]
fn rejects_blocking_descriptors() {
    use std::io::Read;
    let _runtime = mio::Runtime::new().unwrap();
    let (mut peer, socket) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    assert!(
        matches!(Reader::new(socket.into()), Err(e) if e.kind() == io::ErrorKind::InvalidInput)
    );
    assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn direct_io_reads_and_writes_can_wait_independently() {
    use std::io::Read;
    let mut runtime = mio::Runtime::new().unwrap();
    let (mut peer, socket) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    socket.set_nonblocking(true).unwrap();
    let source = mio::Io::new(socket.into()).unwrap();
    let mut bytes = [0; 8];
    let mut read = Box::pin(source.read(&mut bytes));
    assert!(poll(read.as_mut()).is_pending());
    tick(&mut runtime);
    assert!(matches!(
        poll(Box::pin(source.write(&[IoSlice::new(b"out")], None)).as_mut()),
        Poll::Ready(Ok(3))
    ));
    let mut output = [0; 3];
    peer.read_exact(&mut output).unwrap();
    assert_eq!(&output, b"out");
    peer.write_all(b"in").unwrap();
    tick(&mut runtime);
    assert!(
        matches!(poll(read.as_mut()), Poll::Ready(Ok(Received { bytes: 2, fd })) if fd.is_none())
    );
    drop(read);
    assert_eq!(&bytes[..2], b"in");
    assert!(matches!(
        poll(Box::pin(source.write(&[IoSlice::new(&[])], None)).as_mut()),
        Poll::Ready(Ok(0))
    ));
}

#[test]
fn writer_owns_registration_and_closes_it_on_drop() {
    use std::io::Read;
    use tiri_rt::stream::Writer;
    let mut runtime = mio::Runtime::new().unwrap();
    for reuse in [true, false] {
        let (mut peer, socket) = UnixStream::pair().unwrap();
        peer.set_nonblocking(true).unwrap();
        socket.set_nonblocking(true).unwrap();
        let writer = if reuse {
            Writer::from_io(mio::Io::new(socket.into()).unwrap())
        } else {
            Writer::new(socket.into()).unwrap()
        };
        tick(&mut runtime);
        assert!(matches!(
            poll(Box::pin(writer.write(&[IoSlice::new(b"input")], None)).as_mut()),
            Poll::Ready(Ok(5))
        ));
        let mut bytes = [0; 5];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"input");
        drop(writer);
        assert_eq!(peer.read(&mut bytes).unwrap(), 0);
        tick(&mut runtime);
    }
}

#[test]
fn wrappers_accept_implementations_with_only_their_own_capability() {
    use tiri_rt::stream::Writer;
    struct ReadOnly;
    impl AsyncRead for ReadOnly {
        async fn read(&self, buffer: &mut [u8]) -> io::Result<Received> {
            if let Some(byte) = buffer.first_mut() {
                *byte = b'x';
                Ok(Received { bytes: 1, fd: None })
            } else {
                Ok(Received { bytes: 0, fd: None })
            }
        }
    }
    struct WriteOnly(Cell<usize>);
    impl AsyncWrite for WriteOnly {
        async fn write(
            &self,
            buffers: &[IoSlice<'_>],
            fd: Option<std::os::fd::BorrowedFd<'_>>,
        ) -> io::Result<usize> {
            if fd.is_some() {
                return Err(io::ErrorKind::Unsupported.into());
            }
            let len = buffers.iter().map(|buffer| buffer.len()).sum::<usize>();
            self.0.set(self.0.get() + len);
            Ok(len)
        }
    }
    use std::cell::Cell;
    let reader = Reader::from_io(ReadOnly);
    let mut byte = [0];
    assert!(matches!(
        poll(Box::pin(reader.read(&mut byte)).as_mut()),
        Poll::Ready(Ok(Received { bytes: 1, fd })) if fd.is_none()
    ));
    assert_eq!(&byte, b"x");
    let writer = Writer::from_io(WriteOnly(Cell::new(0)));
    assert!(matches!(
        poll(Box::pin(writer.write(&[IoSlice::new(b"hello")], None)).as_mut()),
        Poll::Ready(Ok(5))
    ));
}
