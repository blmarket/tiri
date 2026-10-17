//! Strong and weak Window identities.
//!
//! Every strong owner is a `WindowRef`. Dropping the last one fires
//! `window-closed` while the owner is still held, then performs the explicit
//! cleanup. Close callbacks only observe the window: nothing retains a strong
//! owner past the notification, so it fires exactly once.
use super::model::window;
use std::cell::UnsafeCell;
use std::rc::{Rc, Weak};

/// Retained Window identity.
pub struct WindowRef(Rc<UnsafeCell<window>>);

/// Nonowning Window identity, including callback and parent links.
#[derive(Clone, Default)]
pub struct WindowWeak(Weak<UnsafeCell<window>>);

impl WindowRef {
    pub(super) fn new(value: window) -> Self {
        Self(Rc::new(UnsafeCell::new(value)))
    }

    pub fn downgrade(this: &Self) -> WindowWeak {
        WindowWeak(Rc::downgrade(&this.0))
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        Rc::ptr_eq(&a.0, &b.0)
    }
}

impl crate::src::shared::rc::SameAllocation for WindowRef {
    fn same_allocation(a: &Self, b: &Self) -> bool {
        Self::ptr_eq(a, b)
    }
}

impl Clone for WindowRef {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl std::ops::Deref for WindowRef {
    type Target = UnsafeCell<window>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for WindowRef {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) != 1 {
            return;
        }
        unsafe {
            crate::src::events::events_fire_window(c"window-closed", self.clone());
            assert_eq!(
                Rc::strong_count(&self.0),
                1,
                "window-closed callbacks must not retain the window"
            );
            super::window_destroy(self);
        }
    }
}

impl WindowWeak {
    pub const fn new() -> Self {
        Self(Weak::new())
    }

    pub fn upgrade(&self) -> Option<WindowRef> {
        self.0.upgrade().map(WindowRef)
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }

    pub fn strong_count(&self) -> usize {
        self.0.strong_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::events::{events_add_sink, events_remove_sink};
    use std::cell::Cell;

    #[test]
    fn last_owner_fires_window_closed_once() {
        let closed = Rc::new(Cell::new(0));
        let observed = closed.clone();
        unsafe {
            let sink = events_add_sink(
                c"window-closed",
                Rc::new(move |_, _| observed.set(observed.get() + 1)),
            );
            let owner = window::new_ref();
            let weak = WindowRef::downgrade(&owner);
            drop(owner.clone());
            assert_eq!(closed.get(), 0);
            drop(owner);
            assert_eq!(closed.get(), 1);
            assert!(weak.upgrade().is_none());
            events_remove_sink(sink);
        }
    }
}
