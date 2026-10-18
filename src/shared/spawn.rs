//! Authoritative spawn declarations, shared by the C translation units.
use crate::src::window_pane::WindowPane as _;

use super::abi::pid_t;
use super::command::cmdq_item;
use super::environment::environ;
use super::pane::window_pane;
use super::window::winlink;
use crate::src::shared::client::ClientWeak;
use crate::src::shared::session::SessionWeak;
pub const SPAWN_BEFORE: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const SPAWN_KILL: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const SPAWN_DETACHED: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const SPAWN_EMPTY: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const SPAWN_RESPAWN: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const SPAWN_NONOTIFY: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;

pub struct spawn_editor_state {
    pub id: EditorId,
    pub path: std::ffi::CString,
    pub pid: pid_t,
    pub cb: spawn_finish_edit_cb,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditorId(pub(crate) u64);

/// Observe one pane-owned editor without keeping its Box alive. The identity
/// prevents a delayed completion from clearing a replacement editor.
pub struct EditorHandle {
    pane: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    id: EditorId,
}

impl EditorHandle {
    /// The editor pointer must be live and installed in this pane.
    pub unsafe fn new(
        pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
        editor: std::ptr::NonNull<spawn_editor_state>,
    ) -> Self {
        assert_eq!(
            pane.editor_identity(),
            Some(editor.as_ref().id),
            "editor must belong to the pane",
        );
        Self {
            pane: std::rc::Rc::downgrade(pane),
            id: editor.as_ref().id,
        }
    }

    pub fn matches(&self, editor: &spawn_editor_state) -> bool {
        editor.id == self.id
    }

    pub fn cancel(&self) {
        let Some(pane) = self.pane.upgrade() else {
            return;
        };
        unsafe {
            pane.cancel_editor(self.id);
        }
    }

    pub fn pid(&self) -> pid_t {
        let Some(pane) = self.pane.upgrade() else {
            return -1;
        };
        unsafe { pane.editor_process_id(self.id) }
    }
}

/// Rust-only callback so the editor result can move as owned binary bytes.
pub type spawn_finish_edit_cb =
    Option<Box<dyn FnOnce(std::ptr::NonNull<spawn_editor_state>, Option<Vec<u8>>)>>;

#[repr(C)]
pub struct spawn_context {
    pub item: std::rc::Weak<std::cell::RefCell<cmdq_item>>,
    pub s: SessionWeak,
    pub wl: refbox::Weak<winlink>,
    /// Observe the target client; each spawn operation retains an upgrade while using it.
    pub tc: ClientWeak,
    pub wp0: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub name: Option<std::ffi::CString>,
    pub argv: Vec<std::ffi::CString>,
    pub environ: Option<Box<environ>>,
    pub idx: ::core::ffi::c_int,
    pub cwd: Option<std::ffi::CString>,
    pub flags: ::core::ffi::c_int,
}

impl spawn_context {
    /// Observe the link selected for spawning.
    pub fn winlink_handle(&self) -> refbox::Weak<winlink> {
        self.wl.clone()
    }

    /// The caller supplies a live winlink owned by a session index.
    pub fn set_wl(&mut self, wl: refbox::Weak<winlink>) {
        self.wl = wl;
    }
}
