//! Authoritative control model declarations.

use super::abi::{size_t, u_int, uint64_t};
use super::pane::window_pane_offset;
use crate::monitor::monitor_set;
use std::collections::VecDeque;

#[repr(C)]
pub struct control_state {
    pub panes: control_panes,
    pub windows: control_windows,
    pub queued_reply_bytes: size_t,
    pub read_event: crate::reactor::StreamHandle,
    pub write_event: crate::reactor::StreamHandle,
    pub subs: Option<refbox::RefBox<monitor_set>>,
    pub guard_depth: ::core::ffi::c_int,
    pub(crate) deferred: VecDeque<std::ffi::CString>,
    pub(crate) all_blocks: VecDeque<refbox::RefBox<control_block>>,
    pub(crate) pending_panes: VecDeque<u_int>,
}

impl control_state {
    pub fn empty() -> Self {
        Self {
            panes: Default::default(),
            windows: Default::default(),
            queued_reply_bytes: Default::default(),
            read_event: Default::default(),
            write_event: Default::default(),
            subs: Default::default(),
            guard_depth: Default::default(),
            deferred: Default::default(),
            all_blocks: Default::default(),
            pending_panes: Default::default(),
        }
    }
}

#[repr(C)]
pub struct control_block {
    pub size: size_t,
    pub line: Option<std::ffi::CString>,
    pub t: uint64_t,
}

#[repr(C)]
/// Owned by the pane-ID index until `control_reset_offsets` removes it.
/// The pending queue stores pane IDs and resolves them through the index.
pub struct control_pane {
    pub pane: u_int,
    pub offset: window_pane_offset,
    pub queued: window_pane_offset,
    pub flags: ::core::ffi::c_int,
    pub pending_flag: ::core::ffi::c_int,
    /// Ordered non-owning handles into the state-owned block collection.
    pub blocks: VecDeque<refbox::Weak<control_block>>,
}

pub type control_windows = std::collections::BTreeMap<u32, Box<control_window>>;

#[repr(C)]
/// Owned by the control client's window-ID index.
pub struct control_window {
    pub window: u_int,
    pub sx: u_int,
    pub sy: u_int,
}

/// The index owns each stable pane box.
pub type control_panes = std::collections::BTreeMap<u32, Box<control_pane>>;
