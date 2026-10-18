//! Authoritative menu declarations, shared by the C translation units.

use super::abi::u_int;
use super::command::cmd_find_state;
use super::grid::grid_cell;
use super::key::key_code;
use super::layout::box_lines;
use super::mouse::mouse_event;
use super::screen::screen;
use crate::shared::window::WindowWeak;
use std::ffi::CStr;

/// Windows own menus; redraw scenes only observe them.
/// A borrowed menu definition. An empty name denotes a separator.
#[derive(Copy, Clone)]
pub struct menu_item<'a> {
    pub name: &'a CStr,
    pub key: key_code,
    pub command: Option<&'a CStr>,
}
/// A runtime menu row owns its expanded text. Static definitions use `menu_item`.
#[derive(Default)]
pub struct MenuRow {
    pub name: Option<std::ffi::CString>,
    pub key: key_code,
    pub command: Option<std::ffi::CString>,
}

impl MenuRow {
    pub fn is_selectable(&self) -> bool {
        self.name
            .as_ref()
            .is_some_and(|name| !name.as_bytes().starts_with(b"-"))
    }
}

#[repr(C)]
pub struct menu {
    pub title: std::ffi::CString,
    pub items: Vec<MenuRow>,
    pub width: u_int,
}

pub const MENU_NOMOUSE: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const MENU_STAYOPEN: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const MENU_TAB: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;

#[repr(C)]
pub struct menu_data {
    pub w: WindowWeak,
    pub closed: bool,
    pub flags: ::core::ffi::c_int,
    pub style: Option<std::ffi::CString>,
    pub border_style: Option<std::ffi::CString>,
    pub selected_style: Option<std::ffi::CString>,
    pub style_gc: grid_cell,
    pub border_style_gc: grid_cell,
    pub selected_style_gc: grid_cell,
    pub border_lines: box_lines,
    pub fs: cmd_find_state,
    pub key: key_code,
    pub m: mouse_event,
    pub s: screen,
    pub px: u_int,
    pub py: u_int,
    pub menu: Box<menu>,
    pub choice: ::core::ffi::c_int,
    pub cb: menu_choice_cb,
}

impl menu_data {
    pub fn new(menu: Box<menu>) -> Self {
        Self {
            w: WindowWeak::new(),
            closed: false,
            flags: Default::default(),
            style: Default::default(),
            border_style: Default::default(),
            selected_style: Default::default(),
            style_gc: Default::default(),
            border_style_gc: Default::default(),
            selected_style_gc: Default::default(),
            border_lines: Default::default(),
            fs: Default::default(),
            key: Default::default(),
            m: Default::default(),
            s: screen::empty(),
            px: Default::default(),
            py: Default::default(),
            menu,
            choice: Default::default(),
            cb: Default::default(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MenuSelection {
    Selected { index: u_int, key: key_code },
    Cancelled,
}

pub type menu_choice_cb = Option<Box<dyn FnOnce(MenuSelection)>>;
