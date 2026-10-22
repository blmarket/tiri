//! Borrowed model views for ordinary Rc owners.
//!
//! Ownership is kept in Rc values. Callback observers use Weak handles derived
//! from those owners; borrowed raw addresses never represent strong references.

use std::rc::Rc;

/// Retained handles compared by allocation identity.
pub trait SameAllocation {
    fn same_allocation(a: &Self, b: &Self) -> bool;
}

impl<T> SameAllocation for Rc<T> {
    fn same_allocation(a: &Self, b: &Self) -> bool {
        Rc::ptr_eq(a, b)
    }
}

/// Compare optional retained handles by allocation identity, including absent targets.
pub fn same<P: SameAllocation>(a: Option<&P>, b: Option<&P>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => P::same_allocation(a, b),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct Value(Rc<Cell<usize>>);

    impl Drop for Value {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn final_release_cleans_once_and_expires_weak_observers() {
        let calls = Rc::new(Cell::new(0));
        let owner = Rc::new(RefCell::new(Value(calls.clone())));
        let weak = Rc::downgrade(&owner);
        let retained = owner.clone();
        drop(owner);
        assert_eq!(calls.get(), 0);
        assert!(Rc::ptr_eq(&weak.upgrade().unwrap(), &retained));
        drop(retained);
        assert_eq!(calls.get(), 1);
        assert!(weak.upgrade().is_none());
    }
}
