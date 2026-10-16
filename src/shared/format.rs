//! Authoritative format declarations.
use crate::src::shared::client::{ClientRef, ClientWeak};
use crate::src::shared::session::SessionWeak;
use crate::src::shared::window::WindowWeak;
use std::cell::UnsafeCell;

use super::abi::{time_t, u_int};
use super::command::cmdq_item;
use super::job::job;
use super::mouse::mouse_event;
use super::pane::window_pane;
use super::paste::PasteBufferRef;
use super::window::winlink;
use std::collections::BTreeMap;
pub const FORMAT_VERBOSE: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;

pub const FORMAT_NONE: ::core::ffi::c_int = 0 as ::core::ffi::c_int;

pub const FORMAT_NOJOBS: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;

pub const FORMAT_STATUS: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

pub const FORMAT_FORCE: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;

pub const FORMAT_PANE: ::core::ffi::c_uint = 0x80000000 as ::core::ffi::c_uint;

pub const FORMAT_WINDOW: ::core::ffi::c_uint = 0x40000000 as ::core::ffi::c_uint;

pub const FORMAT_MAX_WIDTH: ::core::ffi::c_int = 10000 as ::core::ffi::c_int;

pub const FORMAT_MAX_REPEAT: ::core::ffi::c_int = 10000 as ::core::ffi::c_int;

/// Maximum repeat result size in bytes.
pub const FORMAT_MAX_REPEAT_SIZE: usize = 65536;

pub const FORMAT_MAX_PRECISION: ::core::ffi::c_int = 100 as ::core::ffi::c_int;

pub const FORMAT_TIMESTRING: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

pub const FORMAT_BASENAME: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;

pub const FORMAT_DIRNAME: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;

pub const FORMAT_QUOTE_SHELL: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;

pub const FORMAT_LITERAL: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;

pub const FORMAT_EXPAND: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;

pub const FORMAT_EXPANDTIME: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;

pub const FORMAT_SESSIONS: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;

pub const FORMAT_WINDOWS: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;

pub const FORMAT_PANES: ::core::ffi::c_int = 0x200 as ::core::ffi::c_int;

pub const FORMAT_PRETTY: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;

pub const FORMAT_LENGTH: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;

pub const FORMAT_WIDTH: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;

pub const FORMAT_QUOTE_STYLE: ::core::ffi::c_int = 0x2000 as ::core::ffi::c_int;

pub const FORMAT_WINDOW_NAME: ::core::ffi::c_int = 0x4000 as ::core::ffi::c_int;

pub const FORMAT_SESSION_NAME: ::core::ffi::c_int = 0x8000 as ::core::ffi::c_int;

pub const FORMAT_CHARACTER: ::core::ffi::c_int = 0x10000 as ::core::ffi::c_int;

pub const FORMAT_COLOUR: ::core::ffi::c_int = 0x20000 as ::core::ffi::c_int;

pub const FORMAT_CLIENTS: ::core::ffi::c_int = 0x40000 as ::core::ffi::c_int;

pub const FORMAT_NOT: ::core::ffi::c_int = 0x80000 as ::core::ffi::c_int;

pub const FORMAT_NOT_NOT: ::core::ffi::c_int = 0x100000 as ::core::ffi::c_int;

pub const FORMAT_REPEAT: ::core::ffi::c_int = 0x200000 as ::core::ffi::c_int;

pub const FORMAT_QUOTE_ARGUMENTS: ::core::ffi::c_int = 0x400000 as ::core::ffi::c_int;

pub const FORMAT_RELATIVE: ::core::ffi::c_int = 0x800000 as ::core::ffi::c_int;

pub const FORMAT_CLIENT_TERMCAP: ::core::ffi::c_int = 0x1000000 as ::core::ffi::c_int;

pub const FORMAT_CLIENT_TERMFEAT: ::core::ffi::c_int = 0x2000000 as ::core::ffi::c_int;

pub const FORMAT_CLIENT_ENVIRON: ::core::ffi::c_int = 0x4000000 as ::core::ffi::c_int;

pub const FORMAT_COLOUR_ESC_FG: ::core::ffi::c_int = 0x8000000 as ::core::ffi::c_int;

pub const FORMAT_COLOUR_ESC_BG: ::core::ffi::c_int = 0x10000000 as ::core::ffi::c_int;

pub const FORMAT_QUOTE_SHELL_SQ: ::core::ffi::c_int = 0x20000000 as ::core::ffi::c_int;

pub const FORMAT_OPTIONS: ::core::ffi::c_int = 0x40000000 as ::core::ffi::c_int;

pub const FORMAT_ENVIRON: ::core::ffi::c_ulonglong = 0x80000000 as ::core::ffi::c_ulonglong;

pub const FORMAT_DIFFERENCE: ::core::ffi::c_ulonglong = 0x100000000 as ::core::ffi::c_ulonglong;

pub const FORMAT_CYCLE: ::core::ffi::c_ulonglong = 0x200000000 as ::core::ffi::c_ulonglong;

pub const FORMAT_LOOP_LIMIT: ::core::ffi::c_int = 100 as ::core::ffi::c_int;

pub const FORMAT_TIME_LIMIT: ::core::ffi::c_int = 100 as ::core::ffi::c_int;

pub const FORMAT_TIME_LOOP_CHECK: ::core::ffi::c_int = 10000 as ::core::ffi::c_int;

pub const FORMAT_CYCLE_PERIOD: ::core::ffi::c_int = 100 as ::core::ffi::c_int;

pub const FORMAT_EXPAND_TIME: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

pub const FORMAT_EXPAND_NOJOBS: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;

pub const FORMAT_EXPAND_NOCYCLE: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;

/// Box-owned by format_create; borrowed pointers are invalid after it is dropped.
#[derive(Default)]
#[repr(C)]
pub struct format_tree {
    pub type_0: format_type,
    pub c: ClientWeak,
    pub s: SessionWeak,
    pub wl: refbox::Weak<winlink>,
    pub w: WindowWeak,
    pub wp: std::rc::Weak<UnsafeCell<window_pane>>,
    pub pb: Option<PasteBufferRef>,
    /// Queue item used for verbose output, observed without retaining it.
    pub item: std::rc::Weak<UnsafeCell<cmdq_item>>,
    pub client: Option<ClientRef>,
    pub flags: ::core::ffi::c_int,
    pub tag: u_int,
    pub m: mouse_event,
    pub tree: format_entry_tree,
}

impl format_tree {
    /// Observe the format context link without extending its lifetime.
    pub fn winlink_handle(&self) -> refbox::Weak<winlink> {
        self.wl.clone()
    }
}

// Jobs remain boxed; callbacks observe cache identity and reacquire an entry.
// Keys own the original command bytes, ordered exactly like tag followed by strcmp.

pub type format_job_tree = std::collections::BTreeMap<(u_int, Vec<u8>), Box<format_job>>;

/// Ordered owners for a format tree's heap-allocated entries.
/// Keys retain the original C string bytes so ordering matches `strcmp`.
pub type format_entry_tree = BTreeMap<std::ffi::CString, Box<format_entry>>;

pub struct format_entry {
    pub key: std::ffi::CString,
    pub(crate) state: FormatEntryState,
}

pub(crate) type FormatEntryCallback =
    Box<dyn FnMut(std::ptr::NonNull<format_tree>) -> Option<std::ffi::CString>>;

pub(crate) enum FormatEntryState {
    Text(std::ffi::CString),
    Time(time_t),
    Lazy(FormatEntryCallback),
    /// The callback owns its capture while running without an entry borrow.
    Evaluating(u64),
    Cached {
        value: std::ffi::CString,
        // Keep the capture alive until this entry is replaced or dropped.
        callback: FormatEntryCallback,
    },
}

impl FormatEntryState {
    pub(crate) fn text(&self) -> Option<&std::ffi::CStr> {
        match self {
            Self::Text(value) | Self::Cached { value, .. } => Some(value),
            _ => None,
        }
    }
}

pub type format_type = ::core::ffi::c_uint;

#[repr(C)]
pub struct format_job {
    /// Allocation-independent identity rejects callbacks for a retired entry
    /// when a replacement uses the same command key. It owns no resources.
    pub(crate) identity: std::rc::Rc<()>,
    /// The client owns its job cache; callbacks only observe it.
    pub client: ClientWeak,
    pub tag: u_int,
    pub cmd: std::ffi::CString,
    pub expanded: Option<std::ffi::CString>,
    pub last: time_t,
    pub out: Option<std::ffi::CString>,
    pub updated: ::core::ffi::c_int,
    pub job: refbox::Weak<job>,
    pub status: ::core::ffi::c_int,
}
