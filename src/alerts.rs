//! Alert dispatch owns queued references; Window owns flags and its timer.
use crate::src::log::{log_debug, log_hex};
use crate::src::session::Session;
use crate::src::shared::session::SessionRef;
use crate::src::shared::tree::RB_NEGINF;
use crate::src::shared::window::WindowRef;
use crate::src::window::{windows, winlinks_minmax, winlinks_next, Window};
use std::collections::VecDeque;

static mut alerts_task: Option<hmux_rt::mio::Task> = None::<hmux_rt::mio::Task>;
static mut alerts_list: VecDeque<WindowRef> = VecDeque::new();

fn alerts_pop_front<T>(queue: &mut VecDeque<T>) -> Option<(T, bool)> {
    let item = queue.pop_front()?;
    // Leave appends made while processing an empty tail for the next callback.
    Some((item, !queue.is_empty()))
}

fn alerts_enqueue<T>(queue: &mut VecDeque<T>, queued: &mut ::core::ffi::c_int, item: T) -> bool {
    if *queued != 0 {
        return false;
    }
    *queued = 1;
    queue.push_back(item);
    true
}

unsafe fn alerts_callback() {
    loop {
        let Some((owner, has_next)) = alerts_pop_front(&mut alerts_list) else {
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
        cursor = owner.next_window();
        drop(owner);
    }
}

unsafe fn schedule(id: u32) {
    if alerts_task.is_none() {
        log_debug(format_args!("alerts check queued (by @{})", id));
        crate::src::reactor::task_start(&mut alerts_task, || {
            Ok(async {
                loop {
                    alerts_callback();
                    if alerts_list.is_empty() {
                        drop(alerts_task.take());
                        break;
                    }
                    crate::src::reactor::yield_now().await;
                }
            })
        })
        .expect("start alert dispatch");
    }
}

pub unsafe fn alerts_queue(owner: &WindowRef, flags: i32) {
    if let Some(newly_queued) = owner.queue_alerts(flags) {
        if newly_queued {
            alerts_list.push_back(owner.clone());
        }
        schedule(owner.id());
    }
}

#[cfg(test)]
mod alerts_list_tests {
    use super::{alerts_enqueue, alerts_pop_front};
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

    #[test]
    fn queue_flag_prevents_duplicate_membership_until_cleared() {
        let mut queue = VecDeque::new();
        let mut queued = 0;

        assert!(alerts_enqueue(&mut queue, &mut queued, 1));
        assert!(!alerts_enqueue(&mut queue, &mut queued, 2));
        assert_eq!(queue, VecDeque::from([1]));
        assert_eq!(queued, 1);

        let (item, has_next) = alerts_pop_front(&mut queue).unwrap();
        assert_eq!(item, 1);
        assert!(!has_next);
        assert!(!alerts_enqueue(&mut queue, &mut queued, 2));
        assert!(queue.is_empty());

        queued = 0;
        assert!(alerts_enqueue(&mut queue, &mut queued, 3));
        assert_eq!(queue, VecDeque::from([3]));
    }
}
