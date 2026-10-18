//! Application-owned futures with explicit and drop cancellation.
use std::future::Future;
use std::io;
use tiri_rt::Handle as _;

/// Replace the owner's task. Dropping the runtime task cancels it.
/// Owners that use the slot to track active work must clear it on completion.
pub fn task_start<F>(
    task: &mut Option<tiri_rt::mio::Task>,
    initialize: impl FnOnce() -> io::Result<F>,
) -> io::Result<()>
where
    F: Future<Output = ()> + 'static,
{
    drop(task.take());
    *task = Some(tiri_rt::mio::Handle::current().spawn(initialize()?)?);
    Ok(())
}

/// Run release-only work on a later loop turn, outside the current callback.
/// After runtime shutdown the callback is dropped unrun, releasing its captures.
pub fn defer(callback: impl FnOnce() + 'static) {
    let _ = tiri_rt::mio::Handle::current().defer(callback);
}
