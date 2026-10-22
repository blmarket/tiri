use crate::shared::session::session;
use crate::window::{
    winlink_add, winlink_remove, winlink_stack_first, winlink_stack_indices, winlink_stack_next,
    winlink_stack_push, winlink_stack_remove, winlinks_reindex,
};
use refbox::BorrowError;

#[test]
fn visit_order_uses_weak_links_and_survives_index_change() {
    let mut links = Default::default();
    let mut stack = Default::default();
    let first = winlink_add(&links, 1);
    let second = winlink_add(&links, 2);
    let old_second = links.try_borrow_mut().unwrap().get(&2).unwrap().downgrade();

    winlink_stack_push(&mut stack, (first).clone());
    winlink_stack_push(&mut stack, (second).clone());
    assert_eq!(winlink_stack_indices(&stack), [2, 1]);
    assert_eq!(winlink_stack_first(&stack), second);
    assert_eq!(winlink_stack_next(&stack, (second).clone()), first);

    winlink_stack_remove(&mut stack, (second).clone());
    winlink_remove((second).clone());
    assert_eq!(
        old_second.try_borrow_mut().err(),
        Some(BorrowError::Dropped)
    );
    assert_eq!(winlink_stack_first(&stack), first);

    winlinks_reindex(&links, (first).clone(), 3);
    assert_eq!(winlink_stack_indices(&stack), [3]);
    assert_eq!(winlink_stack_first(&stack), first);

    winlink_stack_remove(&mut stack, (first).clone());
    winlink_remove((first).clone());
    assert!(crate::window::winlinks_is_empty(&links));
}

#[test]
fn history_entry_is_removed_before_its_owner() {
    let mut links = Default::default();
    let mut stack = Default::default();
    let link = winlink_add(&links, 7);
    winlink_stack_push(&mut stack, link.clone());
    winlink_stack_remove(&mut stack, link.clone());
    winlink_remove(link.clone());

    assert!(!winlink_stack_first(&stack).is_alive());
    assert!(winlink_stack_indices(&stack).is_empty());
}

#[test]
fn boxed_session_drops_its_winlink_owner() {
    let mut owner = Box::new(session::empty());
    let link = winlink_add(&owner.windows, 9);
    let weak = owner
        .windows
        .try_borrow_mut()
        .unwrap()
        .get(&9)
        .unwrap()
        .downgrade();
    winlink_stack_push(&mut owner.lastw, link.clone());
    drop(owner);
    assert_eq!(weak.try_borrow_mut().err(), Some(BorrowError::Dropped));
}

#[test]
#[should_panic(expected = "visited winlink owner was dropped before observer teardown")]
fn expired_history_observer_is_an_invariant_violation() {
    let mut links = Default::default();
    let mut stack = Default::default();
    let link = winlink_add(&links, 1);
    winlink_stack_push(&mut stack, link.clone());
    drop(links);
    winlink_stack_indices(&stack);
}
