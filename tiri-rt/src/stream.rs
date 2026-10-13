//! One-way I/O capabilities backed by a single registration, preserving FDs.

use std::io::{self, IoSlice};
use std::os::fd::{BorrowedFd, OwnedFd};

use crate::{AsyncRead, AsyncWrite, Received, mio};

/// An async reader for a byte-stream descriptor, such as a socket, pipe, or
/// regular file. Datagram and message-oriented descriptors are not supported.
///
/// Reads run only while their future is polled. Dropping a pending read consumes
/// no bytes. Non-file descriptors must remain nonblocking. Regular-file I/O may
/// block the runtime thread. Avoid reading through other aliases. This adapter
/// uses the runtime's local wakers.
///
/// A reader cannot be passed to code requiring write access:
/// ```compile_fail
/// use tiri_rt::{AsyncWrite, mio, stream::Reader};
/// fn needs_writer<T: AsyncWrite>() {}
/// needs_writer::<Reader<mio::Io>>();
/// ```
pub struct Reader<I: AsyncRead> {
    source: I,
}

impl Reader<mio::Io> {
    /// Take ownership of a byte stream on the current runtime.
    ///
    /// The descriptor's flags are not changed. Registration errors close the
    /// descriptor and are returned to the caller.
    ///
    /// # Panics
    /// Panics if no runtime is initialized on this thread.
    #[track_caller]
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        let source = mio::Io::new(fd)?;
        Ok(Self { source })
    }
}

impl<I: AsyncRead> Reader<I> {
    /// Restrict an existing reader to read-only access without registering again.
    pub fn from_io(source: I) -> Self {
        Self { source }
    }

    /// Await at most `max_bytes` owned bytes and an optional FD, or `None` at EOF.
    ///
    /// `max_bytes` must be nonzero. Chunks reflect individual reads, not message
    /// boundaries. No background task or read-ahead buffer is created.
    pub async fn read_chunk(
        &mut self,
        max_bytes: usize,
    ) -> io::Result<Option<(Vec<u8>, Option<OwnedFd>)>> {
        if max_bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "chunk size must be nonzero",
            ));
        }
        let mut bytes = vec![0; max_bytes];
        let received = self.source.read(&mut bytes).await?;
        if received.bytes == 0 {
            return Ok(None);
        }
        bytes.truncate(received.bytes);
        Ok(Some((bytes, received.fd)))
    }
}

impl<I: AsyncRead> AsyncRead for Reader<I> {
    async fn read(&self, buffer: &mut [u8]) -> io::Result<Received> {
        self.source.read(buffer).await
    }
}

/// A write-only view of a byte stream, such as a child's stdin pipe or a file.
/// This wrapper implements [`AsyncWrite`] only and adds no buffering.
/// FD passing and SIGPIPE behavior follow the underlying writer.
///
/// A writer cannot be passed to code requiring read access:
/// ```compile_fail
/// use tiri_rt::{AsyncRead, mio, stream::Writer};
/// fn needs_reader<T: AsyncRead>() {}
/// needs_reader::<Writer<mio::Io>>();
/// ```
pub struct Writer<I: AsyncWrite> {
    source: I,
}

impl Writer<mio::Io> {
    /// Take ownership of a writable byte stream on the current runtime, without
    /// changing flags. Construction errors close the descriptor. Regular-file
    /// writes may block the runtime thread.
    ///
    /// # Panics
    /// Panics if no runtime is initialized on this thread.
    #[track_caller]
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        Ok(Self {
            source: mio::Io::new(fd)?,
        })
    }
}

impl<I: AsyncWrite> Writer<I> {
    /// Restrict an existing writer to write-only access without registering again.
    pub fn from_io(source: I) -> Self {
        Self { source }
    }
}

impl<I: AsyncWrite> AsyncWrite for Writer<I> {
    async fn write(
        &self,
        buffers: &[IoSlice<'_>],
        fd: Option<BorrowedFd<'_>>,
    ) -> io::Result<usize> {
        self.source.write(buffers, fd).await
    }
}
