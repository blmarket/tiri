//! Persistent option identities. Model component borrows retain their model.
use super::types::{options, options_entry};
use crate::src::session::Session;
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::{SessionRef, SessionWeak};
use crate::src::shared::window::{WindowRef, WindowWeak};
use crate::src::tmux::{global_options, global_s_options, global_w_options};
use crate::src::window::Window;
use crate::src::window_pane::WindowPane;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::{cell::UnsafeCell, ffi::CStr};

/// A table's identity, independent of how its owning model stores state.
/// It does not prolong model lifetime; existing logical lifetime guarantees apply.
#[derive(Clone)]
pub enum OptionsScope {
    GlobalServer,
    GlobalSession,
    GlobalWindow,
    Session(SessionWeak),
    Window(WindowWeak),
    Pane(Weak<UnsafeCell<window_pane>>),
}

impl PartialEq for OptionsScope {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::GlobalServer, Self::GlobalServer)
            | (Self::GlobalSession, Self::GlobalSession)
            | (Self::GlobalWindow, Self::GlobalWindow) => true,
            (Self::Session(a), Self::Session(b)) => a.ptr_eq(b),
            (Self::Window(a), Self::Window(b)) => a.ptr_eq(b),
            (Self::Pane(a), Self::Pane(b)) => a.ptr_eq(b),
            _ => false,
        }
    }
}
impl Eq for OptionsScope {}

impl OptionsScope {
    pub fn is_global(&self) -> bool {
        matches!(
            self,
            Self::GlobalServer | Self::GlobalSession | Self::GlobalWindow
        )
    }

    /// Borrow this table. The guard retains its model, so the table stays
    /// allocated while it lives; it must still not be held across callbacks,
    /// format/parse commands or monitor release, and must not alias another
    /// borrow of the same table (scopes are cloneable and `resolve` can yield
    /// the same table).
    pub unsafe fn local(&self) -> OptionsBorrow<options> {
        let (table, owner) = match self {
            Self::GlobalServer => (NonNull::from(global_options()), ScopeOwner::Global),
            Self::GlobalSession => (NonNull::from(global_s_options()), ScopeOwner::Global),
            Self::GlobalWindow => (NonNull::from(global_w_options()), ScopeOwner::Global),
            Self::Session(observer) => {
                let owner = observer.upgrade().expect("live option session");
                let table = NonNull::from(owner.borrow_options_mut());
                (table, ScopeOwner::Session(owner))
            }
            Self::Window(observer) => {
                let owner = observer.upgrade().expect("live option window");
                let table = NonNull::from(owner.borrow_options_mut());
                (table, ScopeOwner::Window(owner))
            }
            Self::Pane(observer) => {
                let owner = observer.upgrade().expect("live option pane");
                let table = NonNull::from(owner.borrow_options_mut());
                (table, ScopeOwner::Pane(owner))
            }
        };
        OptionsBorrow {
            value: table,
            _owner: owner,
        }
    }

    pub unsafe fn parent(&self) -> Option<Self> {
        self.local().parent.clone()
    }

    /// Resolve once, then retain the resulting identity across callbacks. A new
    /// override or changed parent must not redirect an in-flight entry operation.
    pub unsafe fn resolve(&self, name: &CStr, local_only: bool) -> Option<Self> {
        let mut scope = self.clone();
        loop {
            if scope.entry(name).is_some() {
                return Some(scope);
            }
            if local_only {
                return None;
            }
            scope = scope.parent()?;
        }
    }

    /// Borrow a local entry, without resolving inheritance again. The same
    /// contract as `local` applies to this borrow.
    pub unsafe fn entry(&self, name: &CStr) -> Option<OptionsBorrow<options_entry>> {
        self.local()
            .map(|table| table.tree.get_mut(name).map(Box::as_mut))
    }
}

/// Model retention backing an `OptionsBorrow`.
enum ScopeOwner {
    Global,
    Session(SessionRef),
    Window(WindowRef),
    Pane(Rc<UnsafeCell<window_pane>>),
}

/// A borrow of an option table, or a component of one, that retains the
/// owning model until dropped. See `OptionsScope::local` for the contract.
pub struct OptionsBorrow<T> {
    value: NonNull<T>,
    _owner: ScopeOwner,
}

impl<T> OptionsBorrow<T> {
    /// Narrow the borrow to a component, keeping the same retention.
    pub fn map<U>(mut self, f: impl FnOnce(&mut T) -> Option<&mut U>) -> Option<OptionsBorrow<U>> {
        let value = NonNull::from(f(&mut *self)?);
        Some(OptionsBorrow {
            value,
            _owner: self._owner,
        })
    }
}

impl<T> Deref for OptionsBorrow<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.value.as_ref() }
    }
}

impl<T> DerefMut for OptionsBorrow<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.value.as_mut() }
    }
}
