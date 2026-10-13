use refbox::{BorrowError, RefBox, Weak, coerce_weak};

#[test]
fn empty_live_and_expired_have_distinct_identity() {
    let owner = RefBox::new(7);
    let empty = Weak::new();
    let weak = owner.downgrade();
    assert_ne!(empty, weak);
    assert_ne!(empty, owner);
    assert_ne!(owner, empty);
    assert!(!empty.is(&owner));
    let mut slot = empty.clone();
    slot.clone_from(&weak);
    assert_eq!(owner.weak_count(), 2);
    slot.clone_from(&empty);
    assert!(slot.is_empty());
    assert_eq!(owner.weak_count(), 1);
    drop(owner);
    assert!(!weak.is_empty());
    assert!(!weak.is_alive());
    let expired_clone = weak.try_clone().unwrap();
    assert_eq!(weak.weak_count(), 2);
    assert_eq!(weak, expired_clone);
    assert_ne!(weak, empty);
}

#[test]
fn raw_roundtrips_and_unsized_empty_handles() {
    let empty = Weak::<u64>::new();
    let sentinel = empty.as_ptr();
    assert!(!sentinel.is_null());
    let raw = empty.into_raw();
    let restored = unsafe { Weak::from_raw(raw) };
    assert!(restored.is_empty());
    assert_eq!(sentinel, restored.as_ptr());
    let cast = unsafe { restored.cast::<i64>() };
    assert!(cast.is_empty());

    let dynamic = coerce_weak!(Weak::<u64>::new() => dyn std::fmt::Debug);
    assert!(dynamic.is_empty());
    assert_eq!(dynamic.as_ptr().cast::<()>().addr(), sentinel.addr());
    assert_eq!(dynamic, dynamic.clone());
    assert!(matches!(
        dynamic.try_borrow_mut(),
        Err(BorrowError::Dropped)
    ));
    let dynamic = unsafe { Weak::from_raw(dynamic.into_raw()) };
    assert!(dynamic.try_clone().unwrap().is_empty());

    let slice = coerce_weak!(Weak::<[u8; 3]>::new() => [u8]);
    assert!(slice.is_empty());
    assert_eq!(slice.as_ptr().len(), 3);
    assert_eq!(slice, slice.clone());
    assert!(matches!(slice.try_borrow_mut(), Err(BorrowError::Dropped)));
    let slice = unsafe { Weak::from_raw(slice.into_raw()) };
    assert_eq!(slice.as_ptr().len(), 3);
}
