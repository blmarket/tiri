mod api;
use super::io;
use crate::control::CONTROL_BUFFER_LOW;
use crate::shared::event::{bufferevent, bufferevent_data_cb, bufferevent_event_cb};
pub use api::*;
use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_short};
use std::future::{poll_fn, Future};
use std::pin::pin;
use std::rc::{Rc, Weak};
use std::task::{LocalWaker, Poll};
use tiri_buffer::{Buf, BufMut, SegmentedBuf};
use tiri_rt::Handle as _;
use tiri_rt::{AsyncRead as _, AsyncWrite as _};
pub(crate) struct StreamState {
    /// Present until free; observers treat None as released.
    stream: RefCell<Option<Box<bufferevent>>>,
    fd: c_int,
    was_nonblocking: bool,
    pid: u32,
    generation: Cell<u64>,
    write_requested: Cell<bool>,
    wake: RefCell<Option<LocalWaker>>,
    task: RefCell<Option<tiri_rt::mio::Task>>,
}

/// Observe a runtime-owned stream without extending its allocation lifetime.
#[derive(Clone, Default)]
pub struct StreamHandle(Weak<StreamState>);

impl PartialEq for StreamHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl StreamHandle {
    /// Borrow the live stream for one synchronous operation that does not
    /// reenter this stream.
    pub fn with_stream<R>(&self, access: impl FnOnce(&mut bufferevent) -> R) -> Option<R> {
        let owner = self.0.upgrade()?;
        let mut slot = owner.stream.borrow_mut();
        Some(access(slot.as_deref_mut()?))
    }
}
thread_local! {
    static LIVE_STREAMS: RefCell<Vec<Weak<StreamState>>> = const { RefCell::new(Vec::new()) };
}
fn live_states() -> Vec<Rc<StreamState>> {
    LIVE_STREAMS.with(|streams| {
        let mut streams = streams.borrow_mut();
        streams.retain(|stream| stream.strong_count() != 0);
        streams
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|s| s.stream.borrow().is_some())
            .collect()
    })
}
impl StreamState {
    fn wake(&self) {
        self.generation.set(
            self.generation
                .get()
                .checked_add(1)
                .expect("stream generation exhausted"),
        );
        let wake = self.wake.borrow().clone();
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}
pub(super) fn clear() {
    for state in live_states() {
        free(&state);
    }
    LIVE_STREAMS.with(|streams| streams.borrow_mut().clear());
}

fn start(state: &Rc<StreamState>) -> std::io::Result<()> {
    if state.fd == -1 {
        return Ok(());
    }
    let source = io(unsafe { std::os::fd::BorrowedFd::borrow_raw(state.fd) })?;
    let s = state.clone();
    let handle = tiri_rt::mio::Handle::current();
    let task = handle.spawn(async move {
        enum Completion {
            Changed,
            Read(std::io::Result<tiri_rt::Received>),
            Write(std::io::Result<usize>),
        }
        let mut prefer_write = false;
        loop {
            let (count, output, write, generation) = {
                let mut slot = s.stream.borrow_mut();
                let Some(b) = slot.as_deref_mut() else { break };
                let count = if b.enabled & 2 == 0 {
                    0
                } else if b.wm_read.high == 0 {
                    65536
                } else {
                    b.wm_read
                        .high
                        .saturating_sub(b.input.remaining())
                        .min(65536)
                };
                let write =
                    b.enabled & 4 != 0 && (b.output.has_remaining() || s.write_requested.get());
                let output = if write {
                    super::buffer_prefix(&b.output, 65536)
                } else {
                    Vec::new()
                };
                (count, output, write, s.generation.get())
            };
            let mut input = vec![0; count];
            let buffers = [std::io::IoSlice::new(&output)];
            let completion = {
                let mut reader = pin!(source.read(&mut input));
                let mut writer = pin!(source.write(&buffers, None));
                poll_fn(|cx| {
                    *s.wake.borrow_mut() = Some(cx.local_waker().clone());
                    if s.stream.borrow().is_none() || s.generation.get() != generation {
                        return Poll::Ready(Completion::Changed);
                    }
                    for writing in [prefer_write, !prefer_write] {
                        if writing && write {
                            if let Poll::Ready(result) = writer.as_mut().poll(cx) {
                                return Poll::Ready(Completion::Write(result));
                            }
                        } else if !writing && count > 0 {
                            if let Poll::Ready(result) = reader.as_mut().poll(cx) {
                                return Poll::Ready(Completion::Read(result));
                            }
                        }
                    }
                    Poll::Pending
                })
                .await
            };
            let (data, error) = {
                let mut slot = s.stream.borrow_mut();
                let Some(b) = slot.as_deref_mut() else { break };
                let (data, error) = match completion {
                    Completion::Changed => continue,
                    Completion::Read(Ok(received)) if received.bytes > 0 => {
                        input.truncate(received.bytes);
                        b.input.put(SegmentedBuf::from(input));
                        prefer_write = true;
                        let cb = (b.input.remaining() >= b.wm_read.low)
                            .then(|| b.readcb.clone())
                            .flatten();
                        (cb, None)
                    }
                    Completion::Read(result) => {
                        b.enabled &= !2;
                        let event = match result {
                            Err(error) => (1 | 0x20, Some(error)),
                            Ok(_) => (1 | 0x10, None),
                        };
                        (None, Some((b.errorcb.clone(), event)))
                    }
                    Completion::Write(Ok(n)) if n > 0 || output.is_empty() => {
                        b.output.advance(n);
                        s.write_requested.set(false);
                        prefer_write = false;
                        let cb = (b.output.remaining() <= b.wm_write.low)
                            .then(|| b.writecb.clone())
                            .flatten();
                        (cb, None)
                    }
                    Completion::Write(result) => {
                        b.enabled &= !4;
                        let error = result
                            .err()
                            .unwrap_or_else(|| std::io::Error::from_raw_os_error(libc::EIO));
                        (None, Some((b.errorcb.clone(), (2 | 0x20, Some(error)))))
                    }
                };
                (data, error)
            };
            // Callbacks may reenter the stream, so the slot is no longer borrowed.
            if let Some(cb) = data {
                (*cb.borrow_mut())();
            } else if let Some((Some(cb), (flags, error))) = error {
                (*cb.borrow_mut())(flags, error.as_ref());
            }
            super::yield_now().await;
        }
    })?;
    *state.task.borrow_mut() = Some(task);
    Ok(())
}
pub unsafe fn bufferevent_new(
    fd: Option<std::os::fd::BorrowedFd<'_>>,
    readcb: bufferevent_data_cb,
    writecb: bufferevent_data_cb,
    errorcb: bufferevent_event_cb,
) -> std::io::Result<StreamHandle> {
    let was_nonblocking = match fd {
        None => true,
        Some(fd) => tiri_rt::unix::set_nonblocking(fd, true)?,
    };
    let fd = fd.map_or(-1, |fd| std::os::fd::AsRawFd::as_raw_fd(&fd));
    let s = Rc::new(StreamState {
        stream: RefCell::new(None),
        fd,
        was_nonblocking,
        pid: std::process::id(),
        generation: Cell::new(0),
        write_requested: Cell::new(false),
        wake: RefCell::new(None),
        task: RefCell::new(None),
    });
    // The stream owns its state until free breaks this cycle.
    let owner = Box::new(bufferevent {
        state: s.clone(),
        input: Default::default(),
        output: Default::default(),
        wm_read: Default::default(),
        wm_write: Default::default(),
        readcb,
        writecb,
        errorcb,
        enabled: 4,
    });
    *s.stream.borrow_mut() = Some(owner);
    LIVE_STREAMS.with(|streams| {
        let mut streams = streams.borrow_mut();
        streams.retain(|stream| stream.strong_count() != 0);
        streams.push(Rc::downgrade(&s));
    });
    if let Err(error) = start(&s) {
        free(&s);
        // File peers receive the raw errno; report an unsupported
        // descriptor as EOPNOTSUPP.
        if error.kind() == std::io::ErrorKind::Unsupported {
            return Err(std::io::Error::from_raw_os_error(libc::EOPNOTSUPP));
        }
        return Err(error);
    }
    Ok(StreamHandle(Rc::downgrade(&s)))
}
/// Invalidate observers and release the stream. A released stream is ignored.
/// The caller's reference keeps the state alive once the stream is dropped.
fn free(s: &StreamState) {
    let Some(stream) = s.stream.borrow_mut().take() else {
        return;
    };
    drop(s.task.take());
    if s.fd != -1 && s.pid == std::process::id() && !s.was_nonblocking {
        // The fd is owned by the caller and outlives the stream.
        let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(s.fd) };
        let _ = tiri_rt::unix::set_nonblocking(fd, false);
    }
    drop(stream);
}

/// Schedule an I/O recheck before lending input for synchronous consumption.
/// The local task runs after the caller returns to the reactor, so it observes
/// the updated length and can resume reads paused at the high watermark.
pub fn bufferevent_get_input(stream: &mut bufferevent) -> &mut SegmentedBuf {
    stream.state.wake();
    &mut stream.input
}
/// Schedule an I/O recheck before lending output for synchronous mutation.
/// Callers must finish modifying the buffer before returning to the reactor.
pub fn bufferevent_get_output(stream: &mut bufferevent) -> &mut SegmentedBuf {
    stream.state.wake();
    &mut stream.output
}
pub fn bufferevent_enable(stream: &mut bufferevent, flags: c_short) -> c_int {
    let previous = stream.enabled;
    stream.enabled |= flags;
    if flags & 4 != 0 {
        stream.state.write_requested.set(true);
    }
    if previous != stream.enabled || flags & 4 != 0 {
        stream.state.wake();
    }
    0
}
pub fn bufferevent_disable(stream: &mut bufferevent, flags: c_short) -> c_int {
    let previous = stream.enabled;
    stream.enabled &= !flags;
    if previous != stream.enabled {
        stream.state.wake();
    }
    0
}
pub fn bufferevent_write(stream: &mut bufferevent, data: &[u8]) -> c_int {
    super::evbuffer_add(bufferevent_get_output(stream), data)
}
/// Move bytes into output. If the source belongs to another stream, obtain it
/// through bufferevent_get_input/output so that stream also rechecks its I/O.
pub fn bufferevent_write_buffer(stream: &mut bufferevent, buffer: &mut SegmentedBuf) -> c_int {
    stream.output.put(buffer);
    stream.state.wake();
    0
}
pub fn bufferevent_setwatermark(stream: &mut bufferevent) {
    stream.wm_write.low = CONTROL_BUFFER_LOW as usize;
    stream.wm_write.high = 0;

    stream.state.wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_buffer_access_wakes_and_explicit_free_invalidates_handles() {
        let handle = unsafe { bufferevent_new(None, None, None, None) }.unwrap();
        let observer = handle.clone();
        handle.with_stream(|stream| {
            let state = stream.state.clone();
            let generation = state.generation.get();
            bufferevent_write(stream, b"abc");
            assert!(state.generation.get() > generation);
            assert_eq!(stream.output.remaining(), 3);
            let generation = state.generation.get();
            super::super::evbuffer_add(&mut stream.input, b"x");
            assert_eq!(state.generation.get(), generation);
            super::super::evbuffer_drain(bufferevent_get_input(stream), 1);
            assert_eq!(state.generation.get(), generation + 1);
        });
        let state = handle.0.upgrade().unwrap();
        handle.free();
        assert!(!observer.is_alive());
        assert!(state.stream.borrow().is_none());
        drop(state);
        assert!(observer.0.upgrade().is_none());
    }

    #[test]
    fn runtime_clear_frees_streams_without_tasks() {
        let first = unsafe { bufferevent_new(None, None, None, None) }.unwrap();
        let second = unsafe { bufferevent_new(None, None, None, None) }.unwrap();
        assert!(first.is_alive());
        assert!(second.is_alive());
        clear();
        assert!(!first.is_alive());
        assert!(!second.is_alive());
        assert!(first.0.upgrade().is_none());
        assert!(second.0.upgrade().is_none());
    }
}
