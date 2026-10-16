//! Opaque stream operations. Destruction remains explicit, independent of Drop.
use super::*;

/// Read/write interests, distinct from EOF/error notification flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Interests(c_short);

impl Interests {
    pub const READ: Self = Self(2);
}

impl std::ops::BitOr for Interests {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
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
