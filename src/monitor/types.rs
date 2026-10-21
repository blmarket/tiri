//! Authoritative monitor declarations, shared by the C translation units.

use crate::shared::abi::{time_t, u_int};
use crate::shared::client::ClientWeak;
use crate::shared::pane::window_pane;
use crate::shared::session::SessionWeak;
use crate::shared::window::winlink;
use std::cell::RefCell;
use std::rc::Weak;
pub type monitor_type = ::core::ffi::c_uint;
pub const MONITOR_ALL_WINDOWS: monitor_type = 4;
pub const MONITOR_WINDOW: monitor_type = 3;
pub const MONITOR_ALL_PANES: monitor_type = 2;
pub const MONITOR_PANE: monitor_type = 1;
pub const MONITOR_SESSION: monitor_type = 0;
pub const MONITOR_NOTIFY_TRUE: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const MONITOR_NOTIFY_INITIAL: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

#[derive(Clone)]
#[repr(C)]
pub struct monitor_change<'a> {
    pub name: &'a std::ffi::CStr,
    pub value: &'a std::ffi::CStr,
    pub last: Option<&'a std::ffi::CStr>,
    pub c: ClientWeak,
    pub s: SessionWeak,
    pub wl: refbox::Weak<winlink>,
    pub wp: Weak<RefCell<window_pane>>,
}

pub type monitor_cb = std::rc::Rc<dyn Fn(&monitor_change)>;

pub fn monitor_callback(callback: impl Fn(&monitor_change) + 'static) -> monitor_cb {
    std::rc::Rc::new(callback)
}

pub type monitor_items = std::collections::BTreeMap<std::ffi::CString, Box<monitor_item>>;

#[repr(C)]
pub struct monitor_item {
    pub name: std::ffi::CString,
    pub format: std::ffi::CString,
    pub type_0: monitor_type,
    pub id: u_int,
    pub flags: ::core::ffi::c_int,
    pub last: Option<std::ffi::CString>,
    pub panes: monitor_panes,
    pub windows: monitor_windows,
    pub fire_count: u_int,
    pub fire_time: time_t,
}

impl monitor_item {
    pub fn empty() -> Self {
        Self {
            name: Default::default(),
            format: Default::default(),
            type_0: Default::default(),
            id: Default::default(),
            flags: Default::default(),
            last: Default::default(),
            panes: Default::default(),
            windows: Default::default(),
            fire_count: Default::default(),
            fire_time: Default::default(),
        }
    }
}

pub type monitor_windows = std::collections::BTreeMap<(u32, u32), Box<monitor_window>>;

#[repr(C)]
pub struct monitor_window {
    pub window: u_int,
    pub idx: u_int,
    pub last: Option<std::ffi::CString>,
    pub generation: u_int,
}

impl monitor_window {
    pub fn empty() -> Self {
        Self {
            window: Default::default(),
            idx: Default::default(),
            last: Default::default(),
            generation: Default::default(),
        }
    }
}

pub type monitor_panes = std::collections::BTreeMap<(u32, u32), Box<monitor_pane>>;

#[repr(C)]
pub struct monitor_pane {
    pub pane: u_int,
    pub idx: u_int,
    pub last: Option<std::ffi::CString>,
    pub generation: u_int,
}

impl monitor_pane {
    pub fn empty() -> Self {
        Self {
            pane: Default::default(),
            idx: Default::default(),
            last: Default::default(),
            generation: Default::default(),
        }
    }
}
