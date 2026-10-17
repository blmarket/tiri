//! Persistent option identities. Model component borrows never outlive their use.
use crate::src::session::Session;
use crate::src::shared::options::{options, options_entry};
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionWeak;
use crate::src::shared::window::WindowWeak;
use crate::src::tmux::{global_options, global_s_options, global_w_options};
use crate::src::window::Window;
use crate::src::window_pane::WindowPane;
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

    /// Borrow this table. The borrow is valid only while its model keeps the
    /// table installed; it must not be held across callbacks, format/parse
    /// commands or monitor release, and must not alias another borrow of the
    /// same table (scopes are cloneable and `resolve` can yield the same table).
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn local(&self) -> &mut options {
        match self {
            Self::GlobalServer => global_options.as_mut().expect("server options"),
            Self::GlobalSession => global_s_options.as_mut().expect("global session options"),
            Self::GlobalWindow => global_w_options.as_mut().expect("global window options"),
            Self::Session(observer) => {
                let owner = observer.upgrade().expect("live option session");
                let table = owner.with_options_mut(|table| table as *mut options);
                drop(owner);
                &mut *table
            }
            Self::Window(observer) => {
                let owner = observer.upgrade().expect("live option window");
                let table = owner.with_options_mut(|table| table as *mut options);
                drop(owner);
                &mut *table
            }
            Self::Pane(observer) => {
                let owner = observer.upgrade().expect("live option pane");
                let table = owner.with_options_mut(|table| table as *mut options);
                owner.release(c"options scope");
                &mut *table
            }
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
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn entry(&self, name: &CStr) -> Option<&mut options_entry> {
        self.local().tree.get_mut(name).map(Box::as_mut)
    }
}
