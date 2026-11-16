//! Application-owned futures with explicit and drop cancellation.
use std::future::Future;
use std::io;

/// Spawn a task. Dropping the returned task cancels it.
/// Owners replacing a slot must drop the previous task before starting the next.
pub fn task_start<F>(initialize: impl FnOnce() -> io::Result<F>) -> io::Result<tiri_rt::Task>
where
    F: Future<Output = ()> + 'static,
{
    tiri_rt::Handle::current().spawn(initialize()?)
}

/// Run release-only work on a later loop turn, outside the current callback.
/// After runtime shutdown the callback is dropped unrun, releasing its captures.
pub fn defer(callback: impl FnOnce() + 'static) {
    let _ = tiri_rt::Handle::current().defer(callback);
}
