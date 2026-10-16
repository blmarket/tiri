//! Application compatibility over tiri-rt and tiri-buffer.
//!
//! Streams own their buffers. Task handles own their cancellable futures;
//! callbacks run without registry borrows. Descriptor leases are
//! owned by each task and close after an executing poll finishes.
//! The client and server create their runtimes and move them into the process
//! loop. Scheduling helpers use the current handle without creating a runtime.
#![allow(clippy::missing_safety_doc)]
mod buffer;
mod streams;
mod tasks;
mod timers;
pub use buffer::*;
use std::os::fd::BorrowedFd;
pub use streams::*;
pub use tasks::task_start;
pub use timers::Timer;

/// Create a task-owned registration; no raw descriptor lookup survives this call.
pub(crate) fn io(fd: BorrowedFd<'_>) -> std::io::Result<tiri_rt::mio::Io> {
    // Retain the open file description through callback cancellation.
    let lease = fd.try_clone_to_owned()?;
    tiri_rt::mio::Io::new(lease)
}
/// Translate an owned I/O error at the remaining C-style callback boundary.
pub(crate) fn io_status(result: std::io::Result<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            unsafe {
                *crate::src::ffi::libc::__errno_location() =
                    error.raw_os_error().unwrap_or(libc::EIO);
            }
            -1
        }
    }
}
/// Free compatibility streams before dropping the runtime that drives them.
/// Taking ownership keeps shutdown outside an active runtime poll.
pub fn shutdown_runtime(runtime: tiri_rt::mio::Runtime) {
    streams::clear();
    // The runtime clears its current handle after destructors run.
    drop(runtime);
}
pub(crate) async fn yield_now() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        cx.local_waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await
}
