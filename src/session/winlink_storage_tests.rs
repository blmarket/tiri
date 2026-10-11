use crate::src::shared::session::session;
use crate::src::window::*;
use refbox::BorrowError;

#[test]
fn owned_winlinks_preserve_duplicates_bounds_and_traversal() {
    unsafe {
        let mut head = Default::default();
        assert!(!winlinks_minmax(&head, -1).is_alive());
        let ids = [i32::MAX, 0, 42, 7];
        let nodes: Vec<_> = ids.iter().map(|&id| winlink_add(&mut head, id)).collect();
        // Entry-only traversal must survive a moved tree head.
        let mut head = Box::new(head);
        let existing = nodes[2].clone();
        let weak = head
            .as_ref()
            .try_borrow_mut()
            .unwrap()
            .get(&42)
            .unwrap()
            .downgrade();
        assert!(!winlink_add(&mut *head, 42).is_alive());
        assert_eq!(weak, existing);
        assert!(weak.is_alive());
        assert_eq!(winlink_find_by_index(&head, 42), existing);
        assert_eq!(winlinks_nfind(&head, 42), existing);
        assert!(!winlink_find_by_index(&head, 8).is_alive());
        assert_eq!(winlinks_nfind(&head, 8), existing);
        let mut node = winlinks_minmax(&head, -1);
        for id in [0, 7, 42, i32::MAX] {
            assert_eq!(node.get_unchecked().idx, id);
            node = winlinks_next(node.get_unchecked());
        }
        assert!(!node.is_alive());
        node = winlinks_minmax(&head, 1);
        for id in [i32::MAX, 42, 7, 0] {
            assert_eq!(node.get_unchecked().idx, id);
            node = winlinks_prev(node.get_unchecked());
        }
        assert!(!node.is_alive());
        node = winlinks_minmax(&head, -1);
        while node.is_alive() {
            let next = winlinks_next(node.get_unchecked());
            winlink_remove(&mut *head, (node).clone());
            node = next;
        }
        assert_eq!(weak.try_borrow_mut().err(), Some(BorrowError::Dropped));
        assert!(crate::src::window::winlinks_is_empty(&head));
        assert!(!winlinks_nfind(&head, 8).is_alive());
        let replacement = winlink_add(&mut *head, 42);
        assert!(replacement.is_alive());
        winlink_remove(&mut *head, (replacement).clone());
        assert!(crate::src::window::winlinks_is_empty(&head));
    }
}

#[test]
fn moved_map_and_reindexed_owner_keep_identity_through_growth() {
    unsafe {
        let mut head = Default::default();
        let first = winlink_add(&mut head, 0);
        let weak = head.try_borrow_mut().unwrap().get(&0).unwrap().downgrade();
        let mut old = std::mem::take(&mut head);
        let replacement = winlink_add(&mut head, 0);
        winlinks_reindex(&mut old, (first).clone(), 5);
        assert_eq!(winlink_find_by_index(&old, 5), first);
        assert!(!winlink_find_by_index(&old, 0).is_alive());
        assert_eq!(weak.try_access_mut(|link| link.idx).unwrap(), 5);
        for idx in 6..134 {
            assert!(winlink_add(&mut old, idx).is_alive());
        }
        assert_eq!(weak, first);
        assert_eq!(
            winlinks_next(first.get_unchecked()),
            winlink_find_by_index(&old, 6)
        );
        assert!(!winlinks_next(replacement.get_unchecked()).is_alive());
        while !crate::src::window::winlinks_is_empty(&old) {
            let node = winlinks_minmax(&old, -1);
            winlink_remove(&mut old, (node).clone());
        }
        assert_eq!(weak.try_borrow_mut().err(), Some(BorrowError::Dropped));
        assert_eq!(winlink_count(&head), 1);
        winlink_remove(&mut head, (replacement).clone());
        assert!(crate::src::window::winlinks_is_empty(&head));
    }
}

#[test]
fn shuffle_moves_owners_without_losing_history_and_removal_clears_observers() {
    unsafe {
        let session_owner = session::new();
        let session = &mut *session_owner.get();
        let mut nodes = Vec::new();
        for idx in 1..=3 {
            let mut node = winlink_add(&raw mut session.windows, idx);
            node.get_mut_unchecked().session = std::rc::Rc::downgrade(&session_owner);
            nodes.push(node);
        }
        winlink_stack_push(&raw mut session.lastw, (nodes[0]).clone());
        winlink_stack_push(&raw mut session.lastw, (nodes[1]).clone());
        let weak = session
            .windows
            .try_borrow_mut()
            .unwrap()
            .get(&1)
            .unwrap()
            .downgrade();
        assert_eq!(winlink_shuffle_up(&session_owner, (nodes[0]).clone(), 1), 1);
        assert!(!winlink_find_by_index(&session.windows, 1).is_alive());
        for (node, idx) in nodes.iter().zip(2..=4) {
            assert_eq!(winlink_find_by_index(&session.windows, idx), *node);
        }
        assert_eq!(winlink_stack_indices(&session.lastw), [3, 2]);
        assert_eq!(weak, nodes[0]);
        for node in nodes {
            winlink_remove(&raw mut session.windows, (node).clone());
        }
        assert!(winlink_stack_indices(&session.lastw).is_empty());
        assert_eq!(weak.try_borrow_mut().err(), Some(BorrowError::Dropped));
        assert!(crate::src::window::winlinks_is_empty(&session.windows));
    }
}

#[test]
fn detached_winlink_does_not_keep_session_alive_and_can_be_removed_after_expiry() {
    unsafe {
        let owner = session::new();
        let observer = std::rc::Rc::downgrade(&owner);
        let mut links = Default::default();
        let mut link = winlink_add(&mut links, 1);
        link.get_mut_unchecked().session = observer.clone();
        assert!(std::rc::Rc::ptr_eq(
            &link.get_unchecked().session.upgrade().unwrap(),
            &owner
        ));
        drop(owner);
        assert!(link.get_unchecked().session.upgrade().is_none());
        winlink_remove(&mut links, link.clone());
        assert!(crate::src::window::winlinks_is_empty(&links));
    }
}
