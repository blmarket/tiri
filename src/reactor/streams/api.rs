//! Opaque stream operations. Destruction remains explicit, independent of Drop.
use super::*;
use crate::src::shared::event::{bufferevent_data_callback, bufferevent_event_callback};
use std::io;
use std::os::fd::RawFd;

/// Read/write interests, distinct from EOF/error notification flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Interests(c_short);

impl Interests {
    pub const READ: Self = Self(2);
    pub const WRITE: Self = Self(4);
}

impl std::ops::BitOr for Interests {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Read,
    Write,
}

#[derive(Debug)]
pub enum StreamEventCause {
    Eof,
    IoError(io::Error),
}

#[derive(Debug)]
pub struct StreamEvent {
    pub direction: Direction,
    pub cause: StreamEventCause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamError {
    Freed,
    InvalidRange,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Freed => "stream has been freed",
            Self::InvalidRange => "input offset exceeds buffered length",
        })
    }
}

impl std::error::Error for StreamError {}

pub type StreamResult<T> = Result<T, StreamError>;
pub type DataCallback = Box<dyn FnMut(StreamHandle)>;
pub type EventCallback = Box<dyn FnMut(StreamHandle, StreamEvent)>;

#[derive(Default)]
pub struct Callbacks {
    pub read: Option<DataCallback>,
    pub write: Option<DataCallback>,
    pub event: Option<EventCallback>,
}

/// Fixed stream thresholds. Defaults enable unlimited input and zero low marks.
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamOptions {
    pub read_low: usize,
    /// None means unlimited. A finite limit must be nonzero and >= read_low.
    pub read_high: Option<usize>,
    pub write_low: usize,
}

/// Statically dispatched access to a reactor-owned stream through an observer.
/// No buffer references escape these operations. Clones do not own the stream;
/// the logical owner must call free at its existing teardown point.
pub trait BufferEvent {
    fn is_alive(&self) -> bool;
    /// Consume this handle, invalidate all observers, and release the stream.
    /// An already invalidated handle is harmless. The external fd is not closed.
    fn free(self);
    /// Enabling WRITE also requests an empty-output write callback opportunity.
    fn enable(&self, interests: Interests) -> StreamResult<()>;
    fn disable(&self, interests: Interests) -> StreamResult<()>;
    fn input_len(&self) -> StreamResult<usize>;
    /// Copy from a relative input offset without consuming. Returns bytes copied.
    fn copy_input(&self, offset: usize, dst: &mut [u8]) -> StreamResult<usize>;
    /// Consume up to count bytes and recheck read readiness. Returns bytes drained.
    fn drain_input(&self, count: usize) -> StreamResult<usize>;
    /// Copy borrowed bytes into output. Success means queued, not delivered.
    fn write(&self, bytes: &[u8]) -> StreamResult<()>;
}

impl StreamHandle {
    fn access<R>(&self, f: impl FnOnce(&mut bufferevent) -> StreamResult<R>) -> StreamResult<R> {
        let owner = self.0.upgrade().ok_or(StreamError::Freed)?;
        if !owner.live.get() {
            return Err(StreamError::Freed);
        }
        let mut slot = owner.stream.borrow_mut();
        f(slot.as_deref_mut().ok_or(StreamError::Freed)?)
    }
}

impl BufferEvent for StreamHandle {
    fn is_alive(&self) -> bool {
        self.0.upgrade().is_some_and(|state| state.live.get())
    }

    fn free(self) {
        // Release the allocation-slot borrow before freeing, including callbacks.
        let stream = self.ptr();
        unsafe { bufferevent_free(stream) };
    }

    fn enable(&self, interests: Interests) -> StreamResult<()> {
        self.access(|stream| {
            unsafe { bufferevent_enable(stream, interests.0) };
            Ok(())
        })
    }

    fn disable(&self, interests: Interests) -> StreamResult<()> {
        self.access(|stream| {
            unsafe { bufferevent_disable(stream, interests.0) };
            Ok(())
        })
    }

    fn input_len(&self) -> StreamResult<usize> {
        self.access(|stream| Ok(stream.input.remaining()))
    }

    fn copy_input(&self, mut offset: usize, dst: &mut [u8]) -> StreamResult<usize> {
        self.access(|stream| {
            if offset > stream.input.remaining() {
                return Err(StreamError::InvalidRange);
            }
            let mut copied = 0;
            for chunk in tiri_buffer::Buffer::chunks(&*stream.input) {
                if offset >= chunk.len() {
                    offset -= chunk.len();
                    continue;
                }
                let chunk = &chunk[offset..];
                let count = chunk.len().min(dst.len() - copied);
                dst[copied..copied + count].copy_from_slice(&chunk[..count]);
                copied += count;
                offset = 0;
                if copied == dst.len() {
                    break;
                }
            }
            Ok(copied)
        })
    }

    fn drain_input(&self, count: usize) -> StreamResult<usize> {
        self.access(|stream| {
            let count = count.min(stream.input.remaining());
            stream.input.advance(count);
            state(stream).wake();
            Ok(count)
        })
    }

    fn write(&self, bytes: &[u8]) -> StreamResult<()> {
        self.access(|stream| {
            stream.output.put_slice(bytes);
            state(stream).wake();
            Ok(())
        })
    }
}

/// Construct a stream with fixed thresholds and opaque callback handles.
///
/// # Safety
/// fd must be -1 (buffer-only), or an externally owned descriptor that remains
/// open until explicit free. Coordinate external descriptor I/O with the reactor.
pub unsafe fn new_buffer_event(
    fd: RawFd,
    options: StreamOptions,
    callbacks: Callbacks,
) -> io::Result<StreamHandle> {
    if options
        .read_high
        .is_some_and(|high| high == 0 || high < options.read_low)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid read watermarks",
        ));
    }
    fn data_callback(callback: Option<DataCallback>) -> bufferevent_data_cb {
        callback.and_then(|mut callback| {
            bufferevent_data_callback(move |stream| {
                callback(unsafe { StreamHandle::from_ptr(stream.as_ptr()) });
            })
        })
    }
    let errorcb = callbacks.event.and_then(|mut callback| {
        bufferevent_event_callback(move |stream, flags| {
            // Capture errno before any application code or handle manipulation.
            let cause = if flags & 0x10 != 0 {
                StreamEventCause::Eof
            } else {
                StreamEventCause::IoError(io::Error::last_os_error())
            };
            let direction = if flags & 1 != 0 {
                Direction::Read
            } else {
                Direction::Write
            };
            callback(
                unsafe { StreamHandle::from_ptr(stream.as_ptr()) },
                StreamEvent { direction, cause },
            );
        })
    });
    let stream = bufferevent_new(
        fd,
        data_callback(callbacks.read),
        data_callback(callbacks.write),
        errorcb,
    );
    if stream.is_null() {
        return Err(io::Error::last_os_error());
    }
    // Construction and configuration are synchronous; the task has not polled.
    (*stream).wm_read.low = options.read_low;
    (*stream).wm_read.high = options.read_high.unwrap_or(0);
    (*stream).wm_write.low = options.write_low;
    Ok(StreamHandle::from_ptr(stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    fn buffer_only() -> StreamHandle {
        unsafe { new_buffer_event(-1, StreamOptions::default(), Callbacks::default()).unwrap() }
    }

    #[test]
    fn input_copy_crosses_segments_without_consuming_and_drain_wakes() {
        let stream = buffer_only();
        stream
            .access(|inner| {
                inner
                    .input
                    .as_mut()
                    .put(SegmentedBuf::from(b"abc".to_vec()));
                inner
                    .input
                    .as_mut()
                    .put(SegmentedBuf::from(b"def".to_vec()));
                Ok(())
            })
            .unwrap();
        let state = stream.0.upgrade().unwrap();
        let generation = state.generation.get();
        let mut bytes = [0; 4];
        assert_eq!(stream.copy_input(2, &mut bytes), Ok(4));
        assert_eq!(&bytes, b"cdef");
        assert_eq!(stream.input_len(), Ok(6));
        assert_eq!(stream.copy_input(6, &mut bytes), Ok(0));
        assert_eq!(
            stream.copy_input(7, &mut bytes),
            Err(StreamError::InvalidRange)
        );
        assert_eq!(state.generation.get(), generation);
        assert_eq!(stream.drain_input(usize::MAX), Ok(6));
        assert_eq!(stream.input_len(), Ok(0));
        assert!(state.generation.get() > generation);
        stream.free();
        assert!(state.stream.borrow().is_none());
    }

    #[test]
    fn invalid_options_do_not_change_descriptor_flags() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
        for high in [0, 2] {
            let result = unsafe {
                new_buffer_event(
                    socket.as_raw_fd(),
                    StreamOptions {
                        read_low: 3,
                        read_high: Some(high),
                        ..Default::default()
                    },
                    Callbacks::default(),
                )
            };
            assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidInput));
            assert_eq!(
                unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) },
                flags
            );
        }
    }
}
