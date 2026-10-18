//! Persistent option identities. Table access retains the owning model.
use super::global::{with_global_options, with_global_s_options, with_global_w_options};
use super::types::{options, options_entry};
use crate::session::Session;
use crate::shared::pane::window_pane;
use crate::shared::session::SessionWeak;
use crate::shared::window::WindowWeak;
use crate::window::Window;
use crate::window_pane::WindowPane;
use std::rc::Weak;
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

    /// Run `visit` on this table, retaining its model until it returns.
    /// Borrowing the same table again inside `visit` panics, so `visit` must
    /// not reenter callbacks, format/parse commands or monitor release.
    /// Unsafe only for model access: no `&mut` to the owning model may be live.
    pub unsafe fn with_local<R>(&self, visit: impl FnOnce(&mut options) -> R) -> R {
        match self {
            Self::GlobalServer => with_global_options(|oo| visit(oo)),
            Self::GlobalSession => with_global_s_options(|oo| visit(oo)),
            Self::GlobalWindow => with_global_w_options(|oo| visit(oo)),
            Self::Session(observer) => {
                let owner = observer.upgrade().expect("live option session");
                let mut table = owner.borrow_options_mut();
                visit(&mut table)
            }
            Self::Window(observer) => {
                let owner = observer.upgrade().expect("live option window");
                let mut table = owner.borrow_options_mut();
                visit(&mut table)
            }
            Self::Pane(observer) => {
                let owner = observer.upgrade().expect("live option pane");
                let mut table = owner.borrow_options_mut();
                visit(&mut table)
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
            if scope.with_entry(name, |_| ()).is_some() {
                return Some(scope);
            }
            if local_only {
                return None;
            }
            scope = scope.parent()?;
        }
    }

    /// Run `visit` on a local entry, without resolving inheritance again. The
    /// same contract as `with_local` applies.
    pub unsafe fn with_entry<R>(
        &self,
        name: &CStr,
        visit: impl FnOnce(&mut options_entry) -> R,
    ) -> Option<R> {
        self.with_local(|table| table.tree.get_mut(name).map(|entry| visit(entry)))
    }
}
