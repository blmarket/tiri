use crate::src::session::{session_next_session, session_previous_session, sessions};
use crate::src::shared::session::session;
use crate::src::shared::sort::{sort_criteria, SORT_NAME};
use crate::src::sort::sort_get_sessions;
use std::rc::Rc;

#[test]
fn sorted_and_adjacent_results_survive_session_removal() {
    unsafe {
        let first = session::new();
        let last = session::new();
        (*first.get()).name = c"a".to_owned();
        (*last.get()).name = c"z".to_owned();
        let first_weak = Rc::downgrade(&first);
        let last_weak = Rc::downgrade(&last);
        sessions::insert(last.clone());
        sessions::insert(first.clone());
        let criteria = sort_criteria {
            order: SORT_NAME,
            reversed: 0,
            order_seq: &[],
        };
        let sorted = sort_get_sessions(&criteria);
        assert!(Rc::ptr_eq(&sorted[0], &first));
        assert!(Rc::ptr_eq(&sorted[1], &last));
        let next = session_next_session(Some(&last), &criteria).unwrap();
        let previous = session_previous_session(Some(&first), &criteria).unwrap();
        assert!(Rc::ptr_eq(&next, &first));
        assert!(Rc::ptr_eq(&previous, &last));
        sessions::remove(&first);
        assert!(session_next_session(Some(&first), &criteria).is_none());
        assert!(Rc::ptr_eq(
            &session_previous_session(Some(&last), &criteria).unwrap(),
            &last,
        ));
        sessions::remove(&last);
        drop(first);
        drop(last);
        assert_eq!((*sorted[0].get()).name.as_c_str(), c"a");
        drop(sorted);
        assert!(first_weak.upgrade().is_some());
        assert!(last_weak.upgrade().is_some());
        drop(next);
        drop(previous);
        assert!(first_weak.upgrade().is_none());
        assert!(last_weak.upgrade().is_none());
        assert!(session_next_session(None, &criteria).is_none());
    }
}
