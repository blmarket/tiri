//! Authoritative redraw model declarations.

use super::abi::{u_int, uint64_t};
use super::pane::window_pane;
use crate::src::shared::client::ClientWeak;
use crate::src::shared::window::WindowWeak;
use std::{cell::UnsafeCell, rc::Weak};

#[repr(C)]
pub struct redraw_scene {
    pub c: ClientWeak,
    pub w: WindowWeak,
    /// Stable boxed row slice owned by this scene, with length `sy`.
    /// Dropping the rows also drops their spans.
    pub lines: Box<[redraw_line]>,
    pub generation: uint64_t,
    pub sx: u_int,
    pub sy: u_int,
    pub ox: u_int,
    pub oy: u_int,
}

pub type redraw_line = [redraw_spans; 7];

/// Preserve stable heap allocation while drawing helpers borrow spans.
pub type redraw_spans = Vec<Box<redraw_span>>;

#[repr(C)]
pub struct redraw_span {
    pub x: u_int,
    pub width: u_int,
    pub data: redraw_span_data,
}

/// The active payload owns any lifetime handles stored in a cached span.
#[derive(Clone)]
pub enum redraw_span_data {
    Pane(RedrawPaneSpan),
    Outside,
    Empty,
    Status(RedrawStatusSpan),
    Border(RedrawBorderSpan),
    Scrollbar(RedrawScrollbarSpan),
    Menu(RedrawMenuSpan),
}

impl Default for redraw_span_data {
    fn default() -> Self {
        Self::Pane(RedrawPaneSpan::default())
    }
}

impl redraw_span_data {
    pub fn kind(&self) -> redraw_span_type {
        match self {
            Self::Pane(_) => 0,
            Self::Outside => 1,
            Self::Empty => 2,
            Self::Status(_) => 3,
            Self::Border(_) => 4,
            Self::Scrollbar(_) => 5,
            Self::Menu(_) => 6,
        }
    }
    pub fn pane(&self) -> &RedrawPaneSpan {
        let Self::Pane(data) = self else {
            panic!("pane span expected")
        };
        data
    }
    pub fn pane_mut(&mut self) -> &mut RedrawPaneSpan {
        let Self::Pane(data) = self else {
            panic!("pane span expected")
        };
        data
    }
    pub fn border(&self) -> &RedrawBorderSpan {
        let Self::Border(data) = self else {
            panic!("border span expected")
        };
        data
    }
    pub fn border_mut(&mut self) -> &mut RedrawBorderSpan {
        let Self::Border(data) = self else {
            panic!("border span expected")
        };
        data
    }
    pub fn status(&self) -> &RedrawStatusSpan {
        let Self::Status(data) = self else {
            panic!("status span expected")
        };
        data
    }
    pub fn status_mut(&mut self) -> &mut RedrawStatusSpan {
        let Self::Status(data) = self else {
            panic!("status span expected")
        };
        data
    }
    pub fn scrollbar(&self) -> &RedrawScrollbarSpan {
        let Self::Scrollbar(data) = self else {
            panic!("scrollbar span expected")
        };
        data
    }
    pub fn scrollbar_mut(&mut self) -> &mut RedrawScrollbarSpan {
        let Self::Scrollbar(data) = self else {
            panic!("scrollbar span expected")
        };
        data
    }
    pub fn menu(&self) -> &RedrawMenuSpan {
        let Self::Menu(data) = self else {
            panic!("menu span expected")
        };
        data
    }
    pub fn menu_mut(&mut self) -> &mut RedrawMenuSpan {
        let Self::Menu(data) = self else {
            panic!("menu span expected")
        };
        data
    }
}

#[derive(Clone, Default)]
pub struct RedrawPaneSpan {
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub px: u_int,
    pub py: u_int,
}

/// A separator cell: the pane before it (on its left, or above a horizontal
/// separator) and the pane after it, either missing at an edge, and the
/// line it draws, a `CELL_*` kind from the separators it joins.
#[derive(Clone, Default)]
pub struct RedrawBorderSpan {
    pub left_wp: Weak<UnsafeCell<window_pane>>,
    pub right_wp: Weak<UnsafeCell<window_pane>>,
    pub style_wp: Weak<UnsafeCell<window_pane>>,
    pub flags: ::core::ffi::c_int,
    pub cell_type: ::core::ffi::c_int,
}

/// A cell of a pane's status row: `offset` columns into its status line,
/// over a separator cell that draws `cell_type`.
#[derive(Clone, Default)]
pub struct RedrawStatusSpan {
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub offset: u_int,
    pub cell_type: ::core::ffi::c_int,
}

#[derive(Clone, Default)]
pub struct RedrawScrollbarSpan {
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub y: u_int,
    pub height: u_int,
    pub flags: ::core::ffi::c_int,
}

impl PartialEq for RedrawBorderSpan {
    fn eq(&self, other: &Self) -> bool {
        self.left_wp.ptr_eq(&other.left_wp)
            && self.right_wp.ptr_eq(&other.right_wp)
            && self.style_wp.ptr_eq(&other.style_wp)
            && self.flags == other.flags
            && self.cell_type == other.cell_type
    }
}
impl Eq for RedrawBorderSpan {}

impl PartialEq for RedrawPaneSpan {
    fn eq(&self, other: &Self) -> bool {
        self.wp.ptr_eq(&other.wp) && self.px == other.px && self.py == other.py
    }
}
impl Eq for RedrawPaneSpan {}

impl PartialEq for RedrawStatusSpan {
    fn eq(&self, other: &Self) -> bool {
        self.wp.ptr_eq(&other.wp)
            && self.offset == other.offset
            && self.cell_type == other.cell_type
    }
}
impl Eq for RedrawStatusSpan {}

impl PartialEq for RedrawScrollbarSpan {
    fn eq(&self, other: &Self) -> bool {
        self.wp.ptr_eq(&other.wp)
            && self.y == other.y
            && self.height == other.height
            && self.flags == other.flags
    }
}
impl Eq for RedrawScrollbarSpan {}

#[derive(Clone)]
pub struct RedrawMenuSpan {
    pub md: refbox::Weak<crate::src::shared::menu::menu_data>,
    pub px: u_int,
    pub py: u_int,
}
pub type redraw_span_type = ::core::ffi::c_uint;

#[cfg(test)]
mod tests {
    use super::*;

    fn span(x: u_int) -> redraw_span {
        redraw_span {
            x,
            width: 1,
            data: Default::default(),
        }
    }

    #[test]
    fn span_addresses_and_order_survive_vector_growth() {
        let mut spans = redraw_spans::default();
        spans.push(Box::new(span(10)));
        let first = spans[0].as_ref() as *const redraw_span;

        for x in 11..128 {
            spans.push(Box::new(span(x)));
        }

        assert_eq!(spans[0].as_ref() as *const redraw_span, first);
        assert_eq!(unsafe { (*first).x }, 10);
        assert_eq!(spans.first().unwrap().x, 10);
        assert_eq!(spans.last().unwrap().x, 127);
        let xs = spans.iter().map(|span| span.x).collect::<Vec<_>>();
        assert_eq!(xs.len(), 118);
        assert_eq!(xs.first(), Some(&10));
        assert_eq!(xs.last(), Some(&127));
    }
}
