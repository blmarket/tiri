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
    stream: RefCell<Option<Box<bufferevent>>>,
    fd: c_int,
    was_nonblocking: bool,
    pid: u32,
    live: Cell<bool>,
    generation: Cell<u64>,
    write_requested: Cell<bool>,
    wake: RefCell<Option<LocalWaker>>,
    task: RefCell<Option<tiri_rt::mio::Task>>,
}

/// Observe a runtime-owned stream without extending its allocation lifetime.
#[derive(Clone, Default)]
pub struct StreamHandle(Weak<StreamState>);

impl StreamHandle {
    /// The pointer must be null or refer to a stream registered by bufferevent_new.
    pub unsafe fn from_ptr(stream: *mut bufferevent) -> Self {
        if stream.is_null() {
            return Self::default();
        }
        Self(Rc::downgrade(
            (*stream).state.as_ref().expect("registered stream"),
        ))
    }

    /// Keep the stream's allocation slot borrowed for one synchronous operation.
    pub fn with_ptr<R>(&self, access: impl FnOnce(*mut bufferevent) -> R) -> Option<R> {
        let owner = self.0.upgrade()?;
        if !owner.live.get() {
            return None;
        }
        let slot = owner.stream.borrow();
        let stream = slot.as_ref()?;
        Some(access((&**stream as *const bufferevent).cast_mut()))
    }

    /// Legacy pointer view. The caller must keep the stream registered through use.
    pub fn ptr(&self) -> *mut bufferevent {
        self.with_ptr(|stream| stream)
            .unwrap_or(std::ptr::null_mut())
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
            .filter(|s| s.live.get())
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
fn state(stream: &bufferevent) -> Rc<StreamState> {
    stream.state.as_ref().expect("live stream").clone()
}
pub(super) fn clear() {
    for state in live_states() {
        let stream = state
            .stream
            .borrow()
            .as_ref()
            .map(|s| (&**s as *const bufferevent).cast_mut());
        if let Some(stream) = stream {
            unsafe { bufferevent_free(stream) };
        }
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
            if !s.live.get() {
                break;
            }
            let (stream, count, output, write, generation) = {
                let mut slot = s.stream.borrow_mut();
                let b = slot.as_deref_mut().expect("live stream");
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
                (
                    b as *mut bufferevent,
                    count,
                    output,
                    write,
                    s.generation.get(),
                )
            };
            let mut input = vec![0; count];
            let buffers = [std::io::IoSlice::new(&output)];
            let completion = {
                let mut reader = pin!(source.read(&mut input));
                let mut writer = pin!(source.write(&buffers, None));
                poll_fn(|cx| {
                    *s.wake.borrow_mut() = Some(cx.local_waker().clone());
                    if !s.live.get() || s.generation.get() != generation {
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
            if !s.live.get() {
                break;
            }
            unsafe {
                let (data, error) = match completion {
                    Completion::Changed => continue,
                    Completion::Read(Ok(received)) if received.bytes > 0 => {
                        input.truncate(received.bytes);
                        (*stream).input.put(SegmentedBuf::from(input));
                        prefer_write = true;
                        let cb = ((*stream).input.remaining() >= (*stream).wm_read.low)
                            .then(|| (*stream).readcb.clone())
                            .flatten();
                        (cb, None)
                    }
                    Completion::Read(result) => {
                        (*stream).enabled &= !2;
                        let flags = if let Err(error) = result {
                            errno::set_errno(errno::Errno(
                                error.raw_os_error().unwrap_or(libc::EIO),
                            ));
                            1 | 0x20
                        } else {
                            1 | 0x10
                        };
                        (None, Some(flags))
                    }
                    Completion::Write(Ok(n)) if n > 0 || output.is_empty() => {
                        (*stream).output.advance(n);
                        s.write_requested.set(false);
                        prefer_write = false;
                        let cb = ((*stream).output.remaining() <= (*stream).wm_write.low)
                            .then(|| (*stream).writecb.clone())
                            .flatten();
                        (cb, None)
                    }
                    Completion::Write(result) => {
                        (*stream).enabled &= !4;
                        errno::set_errno(errno::Errno(
                            result
                                .err()
                                .and_then(|error| error.raw_os_error())
                                .unwrap_or(libc::EIO),
                        ));
                        (None, Some(2 | 0x20))
                    }
                };
                if let Some(cb) = data {
                    (*cb.borrow_mut())(std::ptr::NonNull::new(stream).expect("live stream"));
                } else if let Some(flags) = error {
                    let cb = (*stream).errorcb.clone();
                    if let Some(cb) = cb {
                        (*cb.borrow_mut())(
                            std::ptr::NonNull::new(stream).expect("live stream"),
                            flags,
                        );
                    }
                }
            }
            super::yield_now().await;
        }
    })?;
    *state.task.borrow_mut() = Some(task);
    Ok(())
}
pub unsafe fn bufferevent_new(
    fd: c_int,
    readcb: bufferevent_data_cb,
    writecb: bufferevent_data_cb,
    errorcb: bufferevent_event_cb,
) -> *mut bufferevent {
    let was_nonblocking = if fd == -1 {
        true
    } else {
        match tiri_rt::unix::set_nonblocking(std::os::fd::BorrowedFd::borrow_raw(fd), true) {
            Ok(previous) => previous,
            Err(error) => {
                super::io_status(Err(error));
                return std::ptr::null_mut();
            }
        }
    };
    let mut owner = Box::new(bufferevent {
        readcb,
        writecb,
        errorcb,
        enabled: 4,
        ..Default::default()
    });
    let stream = &raw mut *owner;
    let s = Rc::new(StreamState {
        stream: RefCell::new(Some(owner)),
        fd,
        was_nonblocking,
        pid: std::process::id(),
        live: Cell::new(true),
        generation: Cell::new(0),
        write_requested: Cell::new(false),
        wake: RefCell::new(None),
        task: RefCell::new(None),
    });
    (*stream).state = Some(s.clone());
    LIVE_STREAMS.with(|streams| {
        let mut streams = streams.borrow_mut();
        streams.retain(|stream| stream.strong_count() != 0);
        streams.push(Rc::downgrade(&s));
    });
    if let Err(error) = start(&s) {
        bufferevent_free(stream);
        errno::set_errno(errno::Errno(
            if error.kind() == std::io::ErrorKind::Unsupported {
                libc::EOPNOTSUPP
            } else {
                error.raw_os_error().unwrap_or(libc::EIO)
            },
        ));
        return std::ptr::null_mut();
    }
    stream
}
pub unsafe fn bufferevent_free(stream: *mut bufferevent) {
    if stream.is_null() {
        return;
    }
    let s = (*stream).state.take();
    if let Some(s) = s {
        s.live.set(false);
        drop(s.task.take());
        if s.fd != -1 && s.pid == std::process::id() && !s.was_nonblocking {
            let _ =
                tiri_rt::unix::set_nonblocking(std::os::fd::BorrowedFd::borrow_raw(s.fd), false);
        }
        drop(s.stream.take());
    }
}

/// Schedule an I/O recheck before lending input for synchronous consumption.
/// The local task runs after the caller returns to the reactor, so it observes
/// the updated length and can resume reads paused at the high watermark.
pub fn bufferevent_get_input(stream: &mut bufferevent) -> &mut SegmentedBuf {
    state(stream).wake();
    &mut stream.input
}
/// Schedule an I/O recheck before lending output for synchronous mutation.
/// Callers must finish modifying the buffer before returning to the reactor.
pub fn bufferevent_get_output(stream: &mut bufferevent) -> &mut SegmentedBuf {
    state(stream).wake();
    &mut stream.output
}
pub unsafe fn bufferevent_enable(stream: *mut bufferevent, flags: c_short) -> c_int {
    let previous = (*stream).enabled;
    (*stream).enabled |= flags;
    let s = state(&*stream);
    if flags & 4 != 0 {
        s.write_requested.set(true);
    }
    if previous != (*stream).enabled || flags & 4 != 0 {
        s.wake();
    }
    0
}
pub unsafe fn bufferevent_disable(stream: *mut bufferevent, flags: c_short) -> c_int {
    let previous = (*stream).enabled;
    (*stream).enabled &= !flags;
    if previous != (*stream).enabled {
        state(&*stream).wake();
    }
    0
}
pub unsafe fn bufferevent_write(stream: *mut bufferevent, data: &[u8]) -> c_int {
    super::evbuffer_add(bufferevent_get_output(&mut *stream), data)
}
/// Move bytes into output. If the source belongs to another stream, obtain it
/// through bufferevent_get_input/output so that stream also rechecks its I/O.
pub unsafe fn bufferevent_write_buffer(
    stream: *mut bufferevent,
    buffer: &mut SegmentedBuf,
) -> c_int {
    if std::ptr::eq(buffer, &raw const *(*stream).output) {
        return -1;
    }
    (*(*stream).output).put(buffer);
    state(&*stream).wake();
    0
}
pub unsafe fn bufferevent_setwatermark(stream: *mut bufferevent) {
    (*stream).wm_write.low = CONTROL_BUFFER_LOW as usize;
    (*stream).wm_write.high = 0;

    state(&*stream).wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_buffer_access_wakes_and_explicit_free_invalidates_handles() {
        unsafe {
            let stream = bufferevent_new(-1, None, None, None);
            let mut handle = StreamHandle::from_ptr(stream);
            let observer = handle.clone();
            let state = state(&*stream);
            let generation = state.generation.get();
            bufferevent_write(stream, b"abc");
            assert!(state.generation.get() > generation);
            assert_eq!((*stream).output.remaining(), 3);
            let generation = state.generation.get();
            super::super::evbuffer_add(&mut (*stream).input, b"x");
            assert_eq!(state.generation.get(), generation);
            super::super::evbuffer_drain(bufferevent_get_input(&mut *stream), 1);
            assert_eq!(state.generation.get(), generation + 1);
            handle.free();
            assert!(!observer.is_alive());
            assert!(state.stream.borrow().is_none());
            assert!(!state.live.get());
            drop(state);
            assert!(observer.0.upgrade().is_none());
        }
    }

    #[test]
    fn runtime_clear_frees_streams_without_tasks() {
        unsafe {
            let first = StreamHandle::from_ptr(bufferevent_new(-1, None, None, None));
            let second = StreamHandle::from_ptr(bufferevent_new(-1, None, None, None));
            assert!(first.is_alive());
            assert!(second.is_alive());
            clear();
            assert!(!first.is_alive());
            assert!(!second.is_alive());
            assert!(first.0.upgrade().is_none());
            assert!(second.0.upgrade().is_none());
        }
    }
}
