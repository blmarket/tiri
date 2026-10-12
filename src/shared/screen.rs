//! Authoritative screen declarations, shared by the C translation units.

use super::{
    abi::{bitstr_t, u_int},
    display::{progress_bar, screen_cursor_style},
    grid::{grid, grid_cell},
    screen_write::screen_write_cline,
};
use std::collections::VecDeque;
use std::ffi::CString;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct screen_sel {
    pub hidden: ::core::ffi::c_int,
    pub rectangle: ::core::ffi::c_int,
    pub modekeys: ::core::ffi::c_int,
    pub sx: u_int,
    pub sy: u_int,
    pub ex: u_int,
    pub ey: u_int,
    pub clipx: u_int,
    pub cell: grid_cell,
}
#[derive(Default)]
#[repr(C)]
pub struct screen {
    pub title: CString,
    pub path: Option<CString>,
    pub titles: VecDeque<CString>,
    pub grid: Option<Box<grid>>,
    pub cx: u_int,
    pub cy: u_int,
    pub cstyle: screen_cursor_style,
    pub default_cstyle: screen_cursor_style,
    pub ccolour: ::core::ffi::c_int,
    pub default_ccolour: ::core::ffi::c_int,
    pub rupper: u_int,
    pub rlower: u_int,
    pub mode: ::core::ffi::c_int,
    pub default_mode: ::core::ffi::c_int,
    pub saved_cx: u_int,
    pub saved_cy: u_int,
    pub saved_grid: Option<Box<grid>>,
    pub saved_cell: grid_cell,
    pub saved_flags: ::core::ffi::c_int,
    pub tabs: Vec<bitstr_t>,
    pub sel: Option<Box<screen_sel>>,
    pub write_list: Option<Box<[screen_write_cline]>>,
    pub hyperlinks: Option<crate::src::hyperlinks::HyperlinksRef>,
    pub progress_bar: progress_bar,
}

/// Owned display state used after releasing the screen's owner or component borrow.
/// In particular, overlay callbacks must not return pointers into their screen.
#[derive(Clone, Copy)]
pub struct ScreenMode {
    pub cx: u_int,
    pub cy: u_int,
    pub mode: ::core::ffi::c_int,
    pub default_mode: ::core::ffi::c_int,
    pub cstyle: screen_cursor_style,
    pub default_cstyle: screen_cursor_style,
    pub ccolour: ::core::ffi::c_int,
    pub default_ccolour: ::core::ffi::c_int,
}

impl From<&screen> for ScreenMode {
    fn from(screen: &screen) -> Self {
        Self {
            cx: screen.cx,
            cy: screen.cy,
            mode: screen.mode,
            default_mode: screen.default_mode,
            cstyle: screen.cstyle,
            default_cstyle: screen.default_cstyle,
            ccolour: screen.ccolour,
            default_ccolour: screen.default_ccolour,
        }
    }
}

impl screen {
    pub(crate) fn write_rows(&self) -> &[screen_write_cline] {
        self.write_list
            .as_deref()
            .expect("screen write rows are initialized")
    }

    pub(crate) fn write_rows_mut(&mut self) -> &mut [screen_write_cline] {
        self.write_list
            .as_deref_mut()
            .expect("screen write rows are initialized")
    }

    pub fn grid(&self) -> &grid {
        self.grid.as_deref().expect("screen is initialized")
    }

    pub fn grid_mut(&mut self) -> &mut grid {
        self.grid.as_deref_mut().expect("screen is initialized")
    }

    /// A valid empty record for paths that initialize a screen in place.
    /// All scalar and pointer fields retain their translated zero state while
    /// the drop-bearing owners are initialized with their Rust invariants.
    pub fn empty() -> Self {
        Self::default()
    }
}

pub const MODE_BRACKETPASTE: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;
pub const MODE_CURSOR: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const MODE_INSERT: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const MODE_KCURSOR: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const MODE_KKEYPAD: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const MODE_WRAP: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const MODE_MOUSE_STANDARD: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const MODE_MOUSE_BUTTON: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const MODE_CURSOR_BLINKING: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const MODE_MOUSE_UTF8: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const MODE_MOUSE_SGR: ::core::ffi::c_int = 0x200 as ::core::ffi::c_int;
pub const MODE_FOCUSON: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;
pub const MODE_MOUSE_ALL: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;
pub const MODE_ORIGIN: ::core::ffi::c_int = 0x2000 as ::core::ffi::c_int;
pub const MODE_KEYS_EXTENDED: ::core::ffi::c_int = 32768;
pub const MODE_CURSOR_VERY_VISIBLE: ::core::ffi::c_int = 0x10000 as ::core::ffi::c_int;
pub const MODE_CURSOR_BLINKING_SET: ::core::ffi::c_int = 0x20000 as ::core::ffi::c_int;
pub const MODE_KEYS_EXTENDED_2: ::core::ffi::c_int = 262144;
pub const MODE_THEME_UPDATES: ::core::ffi::c_int = 0x80000 as ::core::ffi::c_int;
pub const MODE_SYNC: ::core::ffi::c_int = 0x100000 as ::core::ffi::c_int;
pub const ALL_MOUSE_MODES: ::core::ffi::c_int =
    MODE_MOUSE_STANDARD | MODE_MOUSE_BUTTON | MODE_MOUSE_ALL;
pub const EXTENDED_KEY_MODES: ::core::ffi::c_int = MODE_KEYS_EXTENDED | MODE_KEYS_EXTENDED_2;
pub const MODE_CRLF: ::core::ffi::c_int = 0x4000 as ::core::ffi::c_int;
pub const ALL_MODES: ::core::ffi::c_int = 0xffffff as ::core::ffi::c_int;
pub const CURSOR_MODES: ::core::ffi::c_int =
    MODE_CURSOR | MODE_CURSOR_BLINKING | MODE_CURSOR_VERY_VISIBLE;
