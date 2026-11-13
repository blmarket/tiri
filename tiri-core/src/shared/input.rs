//! Authoritative input declarations.
use crate::window_pane::WindowPane as _;

use crate::shared::client::ClientWeak;
use std::collections::VecDeque;
use std::ffi::CString;

use super::abi::{u_int, uint64_t};
use super::colour::colour_palette;
use super::event::Timer;
use super::grid::{grid_cell, utf8_data};
use super::pane::window_pane;
pub const INPUT_REQUEST_QUEUE: input_request_type = 2;

pub const INPUT_REQUEST_CLIPBOARD: input_request_type = 1;

pub const INPUT_REQUEST_PALETTE: input_request_type = 0;

pub use tiri_vt::InputEndType as input_end_type;
pub use tiri_vt::INPUT_BUF_DEFAULT_SIZE;

#[derive(Default)]
pub enum InputPalette {
    #[default]
    None,
    Pane(std::rc::Weak<std::cell::RefCell<window_pane>>),
}

impl InputPalette {
    /// Borrow the current palette for one operation; no guard crosses parser callbacks.
    pub fn with_mut<R>(&self, access: impl FnOnce(&mut colour_palette) -> R) -> Option<R> {
        match self {
            Self::None => None,
            Self::Pane(observer) => {
                let owner = observer.upgrade()?;
                let mut palette = owner.borrow_palette_mut();
                Some(access(&mut palette))
            }
        }
    }
}

pub type input_request_type = ::core::ffi::c_uint;

#[repr(C)]
pub struct input_ctx {
    /// Pane lifetime is owned by the pane index and retained event handlers.
    pub wp: std::rc::Weak<std::cell::RefCell<window_pane>>,
    pub event: crate::reactor::StreamHandle,
    pub palette: InputPalette,
    /// Client observation; request/context ownership does not retain the client.
    pub c: ClientWeak,
    pub cell: input_cell,
    pub old_cell: input_cell,
    pub old_cx: u_int,
    pub old_cy: u_int,
    pub old_mode: ::core::ffi::c_int,
    pub utf8data: utf8_data,
    pub utf8started: ::core::ffi::c_int,
    pub last: utf8_data,
    pub vt: tiri_vt::Parser,
    pub(crate) this: refbox::Weak<input_ctx>,
    pub(crate) requests: VecDeque<refbox::RefBox<input_request>>,
    pub request_timer: Option<Timer>,
    pub ground_timer: Option<Timer>,
}

#[repr(C)]
pub struct input_request {
    /// Client observation; request/context ownership does not retain the client.
    pub c: ClientWeak,
    pub ictx: refbox::Weak<input_ctx>,
    pub type_0: input_request_type,
    pub t: uint64_t,
    pub end: input_end_type,
    pub idx: ::core::ffi::c_int,
    /// Formatted reply owned by a queued request; absent for terminal queries.
    pub data: Option<CString>,
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct input_cell {
    pub cell: grid_cell,
    pub set: ::core::ffi::c_int,
    pub g0set: ::core::ffi::c_int,
    pub g1set: ::core::ffi::c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct input_request_palette_data {
    pub idx: ::core::ffi::c_int,
    pub c: ::core::ffi::c_int,
}

pub struct input_request_clipboard_data {
    /// Decoded bytes owned through synchronous reply dispatch and optional paste storage.
    pub data: Vec<u8>,
    pub clip: ::core::ffi::c_char,
}
