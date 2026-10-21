use crate::shared::session::session;
use crate::window::*;
use refbox::BorrowError;

#[test]
fn moved_map_and_reindexed_owner_keep_identity_through_growth() {
    unsafe {
        let mut head = Default::default();
        let first = winlink_add(&head, 0);
        let weak = head.try_borrow_mut().unwrap().get(&0).unwrap().downgrade();
        let mut old = std::mem::take(&mut head);
        let replacement = winlink_add(&head, 0);
        winlinks_reindex(&old, (first).clone(), 5);
        assert_eq!(winlink_find_by_index(&old, 5), first);
        assert!(!winlink_find_by_index(&old, 0).is_alive());
        assert_eq!(weak.try_access_mut(|link| link.idx).unwrap(), 5);
        for idx in 6..134 {
            assert!(winlink_add(&old, idx).is_alive());
        }
        assert_eq!(weak, first);
        assert_eq!(
            winlinks_next(&first.try_borrow().expect("live winlink")),
            winlink_find_by_index(&old, 6)
        );
        assert!(!winlinks_next(&replacement.try_borrow().expect("live winlink")).is_alive());
        while !crate::window::winlinks_is_empty(&old) {
            let node = winlinks_minmax(&old, -1);
            winlink_remove(&old, (node).clone());
        }
        assert_eq!(weak.try_borrow_mut().err(), Some(BorrowError::Dropped));
        assert_eq!(winlink_count(&head), 1);
        winlink_remove(&head, (replacement).clone());
        assert!(crate::window::winlinks_is_empty(&head));
    }
}

#[test]
fn shuffle_moves_owners_without_losing_history_and_removal_clears_observers() {
    unsafe {
        let session_owner = session::new();
        let session = &mut *session_owner.as_ptr();
        let mut nodes = Vec::new();
        for idx in 1..=3 {
            let mut node = winlink_add(&session.windows, idx);
            node.try_borrow_mut().expect("live winlink").session =
                std::rc::Rc::downgrade(&session_owner);
            nodes.push(node);
        }
        winlink_stack_push(&mut session.lastw, (nodes[0]).clone());
        winlink_stack_push(&mut session.lastw, (nodes[1]).clone());
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
            winlink_remove(&session.windows, (node).clone());
        }
        assert!(winlink_stack_indices(&session.lastw).is_empty());
        assert_eq!(weak.try_borrow_mut().err(), Some(BorrowError::Dropped));
        assert!(crate::window::winlinks_is_empty(&session.windows));
    }
}

#[test]
fn detached_winlink_does_not_keep_session_alive_and_can_be_removed_after_expiry() {
    unsafe {
        let owner = session::new();
        let observer = std::rc::Rc::downgrade(&owner);
        let mut links = Default::default();
        let mut link = winlink_add(&links, 1);
        link.try_borrow_mut().expect("live winlink").session = observer.clone();
        assert!(std::rc::Rc::ptr_eq(
            &link
                .try_borrow()
                .expect("live winlink")
                .session
                .upgrade()
                .unwrap(),
            &owner
        ));
        drop(owner);
        assert!(link
            .try_borrow()
            .expect("live winlink")
            .session
            .upgrade()
            .is_none());
        winlink_remove(&links, link.clone());
        assert!(crate::window::winlinks_is_empty(&links));
    }
}
