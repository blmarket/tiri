use refbox::{BorrowError, RefBox, Weak};

#[test]
fn empty_live_and_expired_have_distinct_identity() {
    let owner = RefBox::new(7);
    let empty = Weak::new();
    let weak = owner.downgrade();
    assert_ne!(empty, weak);
    assert!(!empty.is(&owner));
    assert!(weak.is(&owner));
    let mut slot = empty.clone();
    slot.clone_from(&weak);
    assert!(slot.is(&owner));
    slot.clone_from(&empty);
    assert!(slot.is_empty());
    drop(owner);
    assert!(!weak.is_empty());
    assert!(!weak.is_alive());
    let expired_clone = weak.clone();
    assert_eq!(weak, expired_clone);
    assert_ne!(weak, empty);
}

#[test]
fn empty_handle_sentinel_and_borrows() {
    let empty = Weak::<u64>::new();
    let sentinel = empty.as_ptr();
    assert!(!sentinel.is_null());
    assert!(empty.is_empty());
    assert!(!empty.is_alive());
    assert_eq!(empty, empty.clone());
    assert!(matches!(empty.try_borrow(), Err(BorrowError::Dropped)));
    assert!(matches!(empty.try_borrow_mut(), Err(BorrowError::Dropped)));
}
