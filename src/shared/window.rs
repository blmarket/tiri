//! Window identities and shared link/mode component declarations.
pub use crate::window::{WindowRef, WindowWeak};

use super::abi::u_int;
use super::command::{cmd_find_state, cmdq_item};
use super::format::format_tree;
use super::key::key_code;
use super::mouse::mouse_event;
use super::pane::{window_pane, PANE_MINIMUM};
use super::screen::screen;
use crate::arguments::args;
use crate::shared::client::ClientRef;
use crate::shared::session::{SessionRef, SessionWeak};

pub const WINDOW_BELL: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const WINDOW_ACTIVITY: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const WINDOW_SILENCE: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const WINDOW_ALERTFLAGS: ::core::ffi::c_int = WINDOW_BELL | WINDOW_ACTIVITY | WINDOW_SILENCE;
pub const WINLINK_BELL: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const WINLINK_ACTIVITY: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const WINLINK_SILENCE: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const WINLINK_ALERTFLAGS: ::core::ffi::c_int =
    WINLINK_BELL | WINLINK_ACTIVITY | WINLINK_SILENCE;
pub const WINDOW_MINIMUM: ::core::ffi::c_int = PANE_MINIMUM;
pub const WINDOW_MAXIMUM: ::core::ffi::c_int = 10000 as ::core::ffi::c_int;
pub const WINDOW_SIZE_LARGEST: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const WINDOW_SIZE_MANUAL: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const WINDOW_PANE_NO_MODE: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const WINDOW_SIZE_LATEST: ::core::ffi::c_int = 3 as ::core::ffi::c_int;
pub const WINDOW_RESIZE: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const WINDOW_MODE_HIDE_PANE_STATUS: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const WINDOW_MODE_NO_STACK: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const WINDOW_MODE_HIDE_SCROLLBARS: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
/// The mode zooms its pane unless entered with -Z; other modes zoom only
/// with -Z.
pub const WINDOW_MODE_ZOOM: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;

/// The session owns the index allocation. The index itself owns the
/// `RefBox` allocations for winlinks created by `winlink_add`.
pub type winlinks =
    refbox::RefBox<std::collections::BTreeMap<::core::ffi::c_int, refbox::RefBox<winlink>>>;

#[derive(Default)]
#[repr(C)]
pub struct winlink {
    pub idx: ::core::ffi::c_int,
    pub session: SessionWeak,
    pub window_owner: Option<WindowRef>,
    pub flags: ::core::ffi::c_int,
    /// Weak traversal handle into the containing index.
    pub owner:
        refbox::Weak<std::collections::BTreeMap<::core::ffi::c_int, refbox::RefBox<winlink>>>,
}

/// The global index observes windows; winlinks and callbacks own them.
pub type WindowIndex = std::collections::BTreeMap<u_int, WindowWeak>;

/// Ordered non-owning handles; session BTreeMaps own the RefBox allocations.
pub type window_winlinks = Vec<refbox::Weak<winlink>>;

#[repr(C)]
pub struct window_mode_entry {
    /// The pane owns this entry; retaining it here would create a cycle.
    pub wp: std::rc::Weak<std::cell::RefCell<window_pane>>,
    /// Non-owning source; copy mode keeps an independent screen snapshot.
    pub swp: std::rc::Weak<std::cell::RefCell<window_pane>>,
    pub mode: &'static window_mode,
    /// Owns mode payloads that need a stable address but no shared ownership.
    pub boxed_data: Option<Box<dyn std::any::Any>>,
    /// Owns payloads shared with callbacks that can outlive their mode entry.
    pub data_owner: Option<std::rc::Rc<dyn std::any::Any>>,
    pub prefix: u_int,
    pub kill: ::core::ffi::c_int,
    /// The mode zoomed its unzoomed window with -Z, so leaving it unzooms
    /// the window.
    pub unzoom: bool,
}

impl window_mode_entry {
    /// Boxed payload behind a RefCell; callers take scoped borrows.
    pub fn boxed_data_cell<T: std::any::Any>(&self) -> Option<&std::cell::RefCell<T>> {
        self.boxed_data
            .as_ref()?
            .downcast_ref::<std::cell::RefCell<T>>()
    }

    /// Retain an Rc-backed payload across callbacks that may remove this entry.
    pub fn retained_data<T: std::any::Any>(&self) -> Option<std::rc::Rc<T>> {
        self.data_owner
            .as_ref()
            .map(|owner| owner.clone().downcast::<T>().expect("mode payload type"))
    }
}

/// Shared borrow of a mode entry that dereferences to its boxed payload, so
/// the payload cannot outlive the entry borrow.
pub struct ModeDataRef<'a, T> {
    entry: refbox::Ref<'a, window_mode_entry>,
    _payload: std::marker::PhantomData<T>,
}

impl<'a, T: std::any::Any> ModeDataRef<'a, T> {
    /// Returns `None` unless the entry's boxed payload is a `RefCell<T>`.
    pub fn new(entry: refbox::Ref<'a, window_mode_entry>) -> Option<Self> {
        entry.boxed_data_cell::<T>()?;
        Some(Self {
            entry,
            _payload: std::marker::PhantomData,
        })
    }
}

impl<T: std::any::Any> std::ops::Deref for ModeDataRef<'_, T> {
    type Target = std::cell::RefCell<T>;

    fn deref(&self) -> &Self::Target {
        self.entry
            .boxed_data_cell::<T>()
            .expect("payload type checked on borrow")
    }
}

/// Rc-backed mode payload; `dead` lives outside the RefCell so `free` can
/// mark the mode dead while a caller further up the stack holds a borrow.
pub struct ModeCell<T> {
    pub dead: std::cell::Cell<bool>,
    pub data: std::cell::RefCell<T>,
}

impl<T> ModeCell<T> {
    pub fn new(data: T) -> Self {
        Self {
            dead: std::cell::Cell::new(false),
            data: std::cell::RefCell::new(data),
        }
    }
}

/// Enters a mode for an entry, optionally on behalf of a queue item; returns
/// whether the mode now owns a screen to draw to.
pub type window_mode_init_cb = fn(
    refbox::Weak<window_mode_entry>,
    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    Option<&cmd_find_state>,
    Option<&args>,
) -> bool;
/// Visits a screen owned by the mode payload, calling the visitor once when
/// the payload has one. The borrow ends when the visitor returns.
pub type window_mode_screen_cb = fn(refbox::Weak<window_mode_entry>, &mut dyn FnMut(&mut screen));

/// Run `visit` on the screen `cb` exposes; `None` when the mode has no screen.
pub fn window_mode_visit_screen<R>(
    cb: window_mode_screen_cb,
    wme: refbox::Weak<window_mode_entry>,
    visit: impl FnOnce(&mut screen) -> R,
) -> Option<R> {
    let mut visit = Some(visit);
    let mut result = None;
    cb(wme, &mut |screen| {
        result = visit.take().map(|visit| visit(screen));
    });
    result
}
/// Resizes a mode's screen to the given width and height.
pub type window_mode_resize_cb = fn(refbox::Weak<window_mode_entry>, u_int, u_int);
/// Handles a key from a client in the mode's winlink.
pub type window_mode_key_cb = fn(
    refbox::Weak<window_mode_entry>,
    &ClientRef,
    refbox::Weak<winlink>,
    key_code,
    Option<&mut mouse_event>,
);
/// Runs a mode command (send-keys -X) with its arguments.
pub type window_mode_command_cb = fn(
    refbox::Weak<window_mode_entry>,
    Option<&ClientRef>,
    Option<&SessionRef>,
    refbox::Weak<winlink>,
    &args,
    Option<&mut mouse_event>,
);

#[derive(Copy, Clone)]
#[repr(C)]
pub struct window_mode {
    pub name: &'static ::std::ffi::CStr,
    pub default_format: Option<&'static std::ffi::CStr>,
    pub flags: ::core::ffi::c_int,
    pub init: Option<window_mode_init_cb>,
    pub free: Option<fn(refbox::Weak<window_mode_entry>)>,
    pub resize: Option<window_mode_resize_cb>,
    pub update: Option<fn(refbox::Weak<window_mode_entry>)>,
    pub style_changed: Option<fn(refbox::Weak<window_mode_entry>)>,
    pub key: Option<window_mode_key_cb>,
    pub key_table: Option<fn(refbox::Weak<window_mode_entry>) -> &'static std::ffi::CStr>,
    pub command: Option<window_mode_command_cb>,
    pub formats: Option<fn(refbox::Weak<window_mode_entry>, &mut format_tree)>,
    /// The mode's source screen, for `capture-pane -M`.
    pub get_screen: Option<window_mode_screen_cb>,
    /// The screen the pane shows while this mode is selected.
    pub display_screen: Option<window_mode_screen_cb>,
}

impl Default for window_mode {
    fn default() -> Self {
        Self {
            name: c"",
            default_format: None,
            flags: 0,
            init: None,
            free: None,
            resize: None,
            update: None,
            style_changed: None,
            key: None,
            key_table: None,
            command: None,
            formats: None,
            get_screen: None,
            display_screen: None,
        }
    }
}

/// Weak visit history; the session owns the deque and the ordered index
/// owns each link.
pub type winlink_stack = std::collections::VecDeque<refbox::Weak<winlink>>;

#[repr(C)]
pub struct windows {
    /// Weak id-ordered index; window records remain externally owned.
    pub(crate) storage: WindowIndex,
}

impl winlink {
    pub fn window_handle(&self) -> Option<&WindowRef> {
        self.window_owner.as_ref()
    }
}
