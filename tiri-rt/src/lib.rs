//! library entrypoint
#![feature(local_waker)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod mio;
pub mod stream;
pub mod unix;

use std::ffi::c_int;
use std::future::Future;
use std::io::{self, IoSlice};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::time::Duration;

/// Owns and drives the runtime. Polling borrows the owner through callback
/// execution, so the owner cannot be dropped until polling returns.
pub trait Runtime: Sized + 'static {
    /// The capability for creating work on this runtime instance.
    type Handle: Handle;

    /// Create an independent executor and poller on the calling thread.
    fn new() -> io::Result<Self>;

    /// Obtain a capability bound to this instance and process generation.
    fn handle(&self) -> Self::Handle;

    /// Poll tasks, waiting at most max_wait for readiness. Callbacks schedule
    /// through handles, without borrowing the runtime again. The owner remains
    /// exclusively borrowed until all callbacks in this turn have returned.
    fn poll(&mut self, max_wait: Option<Duration>) -> io::Result<()>;
}

/// A cloneable, thread-local capability to schedule work.
/// A handle does not keep its runtime owner alive.
pub trait Handle: Clone + 'static {
    /// Owns a spawned future; dropping it cancels the work.
    type Task: 'static;

    /// Schedule a local-waker future, without polling it inline. Ordinary Waker
    /// notifications are inert. The returned task
    /// owns the future and must be retained until completion or cancellation.
    fn spawn<F>(&self, future: F) -> io::Result<Self::Task>
    where
        F: Future<Output = ()> + 'static;
}

/// A listener whose operations return owned, nonblocking, close-on-exec sockets.
///
/// Only one pending accept is supported. Dropping a pending accept consumes no
/// connection. Dropping the listener or its runtime closes the listening socket;
/// already accepted sockets remain owned by their callers.
pub trait AsyncAccept {
    fn accept(&self) -> impl Future<Output = io::Result<OwnedFd>> + '_;
}

/// Bytes and an optional owned file descriptor from one read operation.
///
/// The first `bytes` bytes of the caller's buffer contain input. This is not an
/// application message: the FD need not belong to the first byte or message in
/// the buffer. The caller associates the FD with a message and preserves incomplete
/// input in receive order.
#[derive(Debug)]
pub struct Received {
    /// Number of bytes read; zero means EOF when the input buffer is nonempty.
    pub bytes: usize,
    /// Received descriptor with close-on-exec set. `None` for descriptors
    /// without FD-passing support. Drop closes the received FD.
    pub fd: Option<OwnedFd>,
}

/// A local async byte reader with optional FD passing.
pub trait AsyncRead {
    /// Read available bytes and at most one accompanying owned FD.
    ///
    /// An empty buffer consumes nothing and returns zero bytes and no FDs.
    /// Dropping a pending read consumes no input. Receiving multiple FDs or
    /// truncated ancillary data returns `InvalidData` and closes all FDs
    /// received by that operation. Input has been consumed in that case, so
    /// treat it as a fatal transport error.
    /// Only one pending reader is supported per registration. Avoid reading
    /// through other aliases, which could discard FDs.
    fn read<'a>(&'a self, buffer: &'a mut [u8]) -> impl Future<Output = io::Result<Received>> + 'a;
}

/// A local async byte writer with optional FD passing.
pub trait AsyncWrite {
    /// Write some bytes from the buffers in order, optionally attaching one FD.
    ///
    /// Unsupported FD passing must return `Unsupported` before writing bytes,
    /// even with empty buffers. An FD requires at least one byte. Empty buffers
    /// without an FD return zero.
    ///
    /// A positive result transfers the attached FD, even on a partial write.
    /// Retry remaining bytes with `None`. Errors, zero, and cancellation
    /// of a pending write transfer neither bytes nor the FD. The FD remains owned by
    /// the caller. Only one pending writer is supported per registration;
    /// serialize submissions to avoid interleaving partial writes.
    fn write<'a>(
        &'a self,
        buffers: &'a [IoSlice<'a>],
        fd: Option<BorrowedFd<'a>>,
    ) -> impl Future<Output = io::Result<usize>> + 'a;
}

/// A local signal subscription
pub trait Signals {
    /// Cancellation-safe wait: dropping it cannot consume an undelivered signal.
    type Recv<'a>: Future<Output = io::Result<c_int>> + 'a
    where
        Self: 'a;

    /// Wait for a subscribed signal. The mutable borrow permits one receiver.
    fn recv(&mut self) -> Self::Recv<'_>;
}
