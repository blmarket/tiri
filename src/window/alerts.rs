//! Window-owned alert flags, queue membership and silence timer.
use super::*;
use crate::src::shared::window::WindowRef;
use crate::src::shared::window::{WINDOW_ACTIVITY, WINDOW_ALERTFLAGS, WINDOW_BELL, WINDOW_SILENCE};
use std::time::Duration;

pub(super) unsafe fn reset_timer(owner: &WindowRef) {
    let w = owner.get();
    (*w).flags &= !WINDOW_SILENCE;
    drop((*w).alerts_timer.take());
    let timeout = Duration::from_secs(
        (owner.with_options_mut(|options| options_get_number(options, c"monitor-silence"))) as u64,
    );
    log_debug(format_args!(
        "@{} alerts timer reset {}",
        owner.id(),
        timeout.as_secs() as u32
    ));
    if timeout.as_secs() != 0 {
        let observer = WindowRef::downgrade(owner);
        (*w).alerts_timer = Some(
            Timer::new(timeout, move || {
                if let Some(owner) = observer.upgrade() {
                    log_debug(format_args!("@{} alerts timer expired", owner.id()));
                    crate::src::alerts::alerts_queue(&owner, WINDOW_SILENCE);
                }
            })
            .expect("arm timer"),
        );
    }
}

pub(super) unsafe fn queue(owner: &WindowRef, flags: i32) -> Option<bool> {
    assert_eq!(flags & !WINDOW_ALERTFLAGS, 0, "alert flags only");
    owner.reset_alert_timer();
    let w = owner.get();
    if (*w).flags & flags != flags {
        (*w).flags |= flags;
        log_debug(format_args!(
            "@{} alerts flags added {}",
            owner.id(),
            crate::src::log::log_hex(flags as u32 as u64)
        ));
    }
    let enabled = owner.with_options_mut(|options| {
        [
            (WINDOW_BELL, c"monitor-bell"),
            (WINDOW_ACTIVITY, c"monitor-activity"),
            (WINDOW_SILENCE, c"monitor-silence"),
        ]
        .into_iter()
        .any(|(flag, name)| flags & flag != 0 && options_get_number(options, name) != 0)
    });
    if !enabled {
        return None;
    }
    if (*w).alerts_queued != 0 {
        return Some(false);
    }
    (*w).alerts_queued = 1;
    Some(true)
}
