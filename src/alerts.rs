//! Alert dispatch owns queued references; Window owns flags and its timer.
use crate::src::log::{log_debug, log_hex};
use crate::src::session::Session;
use crate::src::shared::session::SessionRef;
use crate::src::shared::tree::RB_NEGINF;
use crate::src::shared::window::WindowRef;
use crate::src::window::{windows, winlinks_minmax, winlinks_next, Window};
use std::collections::VecDeque;

/// Windows waiting for alert delivery and the task that drains them.
struct alerts {
    list: VecDeque<WindowRef>,
    task: Option<tiri_rt::mio::Task>,
}

thread_local! {
    static ALERTS: std::cell::RefCell<alerts> = const {
        std::cell::RefCell::new(alerts {
            list: VecDeque::new(),
            task: None,
        })
    };
}

/// Each call borrows the queue only for its own duration; delivery, window
/// drops and task drops happen after the borrow ends, so they may requeue.
impl alerts {
    fn pop_front() -> Option<(WindowRef, bool)> {
        ALERTS.with_borrow_mut(|alerts| alerts_pop_front(&mut alerts.list))
    }

    fn push_back(owner: WindowRef) {
        ALERTS.with_borrow_mut(|alerts| alerts.list.push_back(owner));
    }

    fn is_scheduled() -> bool {
        ALERTS.with_borrow(|alerts| alerts.task.is_some())
    }

    fn set_task(task: Option<tiri_rt::mio::Task>) {
        let previous = ALERTS.with_borrow_mut(|alerts| std::mem::replace(&mut alerts.task, task));
        drop(previous);
    }

    /// Ends dispatch when nothing is queued; returns whether it ended.
    fn finish_if_idle() -> bool {
        let finished = ALERTS.with_borrow_mut(|alerts| {
            if alerts.list.is_empty() {
                Some(alerts.task.take())
            } else {
                None
            }
        });
        finished.map(drop).is_some()
    }
}

fn alerts_pop_front<T>(queue: &mut VecDeque<T>) -> Option<(T, bool)> {
    let item = queue.pop_front()?;
    // Leave appends made while processing an empty tail for the next callback.
    Some((item, !queue.is_empty()))
}

unsafe fn alerts_callback() {
    loop {
        let Some((owner, has_next)) = alerts::pop_front() else {
            break;
        };
        // Membership remains set throughout callbacks to suppress requeueing.
        let alerts = SessionRef::deliver_window_alerts(&owner);
        log_debug(format_args!(
            "@{} alerts check, alerts {}",
            owner.id(),
            log_hex(alerts as u32 as u64)
        ));
        owner.finish_alerts();
        drop(owner);
        if !has_next {
            break;
        }
    }
}

pub unsafe fn alerts_check_session(session: &SessionRef) {
    let mut link = session.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() {
        SessionRef::deliver_window_alerts(
            link.get_unchecked().window_handle().expect("linked window"),
        );
        link = winlinks_next(link.get_unchecked());
    }
}

pub unsafe fn alerts_reset_all() {
    let mut cursor = windows::first();
    while let Some(owner) = cursor {
        owner.reset_alert_timer();
        cursor = owner.next_window().upgrade();
        drop(owner);
    }
}

unsafe fn schedule(id: u32) {
    if !alerts::is_scheduled() {
        log_debug(format_args!("alerts check queued (by @{})", id));
        let mut task = None;
        crate::src::reactor::task_start(&mut task, || {
            Ok(async {
                loop {
                    alerts_callback();
                    if alerts::finish_if_idle() {
                        break;
                    }
                    crate::src::reactor::yield_now().await;
                }
            })
        })
        .expect("start alert dispatch");
        alerts::set_task(task);
    }
}

pub unsafe fn alerts_queue(owner: &WindowRef, flags: i32) {
    if let Some(newly_queued) = owner.queue_alerts(flags) {
        if newly_queued {
            alerts::push_back(owner.clone());
        }
        schedule(owner.id());
    }
}

#[cfg(test)]
mod alerts_list_tests {
    use super::alerts_pop_front;
    use std::collections::VecDeque;

    fn run_callback<T>(queue: &mut VecDeque<T>, mut process: impl FnMut(T, &mut VecDeque<T>)) {
        loop {
            let Some((item, has_next)) = alerts_pop_front(queue) else {
                break;
            };
            process(item, queue);
            if !has_next {
                break;
            }
        }
    }

    #[test]
    fn processes_items_appended_before_the_current_tail() {
        let mut queue = VecDeque::from([1, 2]);
        let mut seen = Vec::new();

        run_callback(&mut queue, |item, queue| {
            seen.push(item);
            if item == 1 {
                queue.push_back(3);
            }
        });

        assert_eq!(seen, [1, 2, 3]);
        assert!(queue.is_empty());
    }

    #[test]
    fn leaves_items_appended_while_processing_the_tail_for_next_callback() {
        let mut queue = VecDeque::from([1]);
        let mut seen = Vec::new();

        run_callback(&mut queue, |item, queue| {
            seen.push(item);
            if item == 1 {
                queue.push_back(2);
            }
        });

        assert_eq!(seen, [1]);
        assert_eq!(queue, VecDeque::from([2]));
    }
}
