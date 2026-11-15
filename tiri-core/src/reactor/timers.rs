//! Timer work is a future owned by the caller's task handle.
use std::io;
use std::time::{Duration, Instant};
use tiri_rt::Handle as _;

/// Owns one scheduled future. Dropping it cancels the work.
/// Owners that track active work clear their timer from the callback.
pub struct Timer {
    _task: tiri_rt::mio::Task,
}

impl Timer {
    /// Zero duration defers execution to a runtime turn.
    pub fn new(delay: Duration, callback: impl FnOnce() + 'static) -> io::Result<Self> {
        let now = Instant::now();
        let deadline = now.checked_add(delay).unwrap_or(now);
        let handle = tiri_rt::mio::Handle::current();
        let sleep = tiri_rt::mio::Sleep::new(deadline);
        let task = handle.spawn(async move {
            sleep.await.expect("timer wait failed");
            callback();
        })?;
        Ok(Self { _task: task })
    }
}

struct BackgroundTimers {
    entries: std::collections::BTreeMap<u64, Timer>,
    next_id: u64,
}

thread_local! {
    static BACKGROUND_TIMERS: std::cell::RefCell<BackgroundTimers> = const {
        std::cell::RefCell::new(BackgroundTimers {
            entries: std::collections::BTreeMap::new(),
            next_id: 0,
        })
    };
}

pub fn background_timer(delay: Duration, callback: impl FnOnce() + 'static) -> io::Result<()> {
    let id = BACKGROUND_TIMERS.with_borrow_mut(|timers| {
        let id = timers.next_id;
        timers.next_id = id.checked_add(1).expect("background timer ID overflow");
        id
    });
    let timer = Timer::new(delay, move || {
        let timer = BACKGROUND_TIMERS.with_borrow_mut(|timers| timers.entries.remove(&id));
        drop(timer);
        callback();
    })?;
    BACKGROUND_TIMERS.with_borrow_mut(|timers| timers.entries.insert(id, timer));
    Ok(())
}

pub fn cancel_background_timers() {
    let entries = BACKGROUND_TIMERS.with_borrow_mut(|timers| std::mem::take(&mut timers.entries));
    drop(entries);
}
