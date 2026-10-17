//! Persistent option identities. Model component borrows never outlive a visit.
use super::options_local_entry;
use crate::src::session::Session;
use crate::src::shared::options::{options, options_entry};
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionWeak;
use crate::src::shared::window::WindowWeak;
use crate::src::tmux::{global_options, global_s_options, global_w_options};
use crate::src::window::Window;
use crate::src::window_pane::WindowPane;
use std::collections::btree_map::{self, Entry};
use std::ffi::CString;
use std::{cell::UnsafeCell, ffi::CStr, rc::Weak};

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

    /// The visitor may read/edit this table only. It must not dispatch callbacks,
    /// format/parse commands, release monitors, or return component observers.
    /// Temporary model retention ends after its component borrow is returned.
    pub unsafe fn with_local<R>(&self, visit: impl FnOnce(&mut options) -> R) -> R {
        match self {
            Self::GlobalServer => visit(global_options.as_mut().expect("server options")),
            Self::GlobalSession => {
                visit(global_s_options.as_mut().expect("global session options"))
            }
            Self::GlobalWindow => visit(global_w_options.as_mut().expect("global window options")),
            Self::Session(observer) => {
                let owner = observer.upgrade().expect("live option session");
                let result = owner.with_options_mut(visit);
                drop(owner);
                result
            }
            Self::Window(observer) => {
                let owner = observer.upgrade().expect("live option window");
                let result = owner.with_options_mut(visit);
                drop(owner);
                result
            }
            Self::Pane(observer) => {
                let owner = observer.upgrade().expect("live option pane");
                let result = owner.with_options_mut(visit);
                owner.release(c"options scope");
                result
            }
        }
    }

    pub unsafe fn parent(&self) -> Option<Self> {
        self.with_local(|table| table.parent.clone())
    }

    /// Resolve once, then retain the resulting identity across callbacks. A new
    /// override or changed parent must not redirect an in-flight entry operation.
    pub unsafe fn resolve(&self, name: &CStr, local_only: bool) -> Option<Self> {
        let mut scope = self.clone();
        loop {
            let (found, parent) = scope.with_local(|table| {
                (
                    matches!(
                        options_local_entry(table, name),
                        Entry::<'_, CString, Box<options_entry>>::Occupied(_)
                    ),
                    table.parent.clone(),
                )
            });
            if found {
                return Some(scope);
            }
            if local_only {
                return None;
            }
            scope = parent?;
        }
    }

    /// Visit a local entry, without resolving inheritance again. The same
    /// non-reentry/no-escape contract as `with_local` applies to this visitor.
    pub unsafe fn with_entry<R>(
        &self,
        name: &CStr,
        visit: impl FnOnce(&mut options_entry) -> R,
    ) -> Option<R> {
        self.with_local(|table| match options_local_entry(table, name) {
            btree_map::Entry::Occupied(entry) => Some(visit(entry.into_mut())),
            btree_map::Entry::Vacant(_) => None,
        })
    }
}
