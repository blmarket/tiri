//! Single-threaded local-waker async runtime for `tiri`.
#![feature(local_waker)]
#![deny(unsafe_op_in_unsafe_fn)]

mod fd_passing;
mod listener;
pub mod oneshot;
mod readiness;
mod runtime;
mod signals;
mod waits;

use std::os::fd::OwnedFd;

pub use listener::Listener;
pub use readiness::Io;
pub use runtime::{Handle, Runtime, Task};
pub use signals::Signals;
pub use waits::Sleep;

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
