//! Authoritative input declarations.
use crate::src::window_pane::WindowPane as _;

use crate::src::shared::client::ClientWeak;
use tiri_buffer::SegmentedBuf;
use std::collections::VecDeque;
use std::ffi::CString;

use super::abi::{size_t, u_char, u_int, uint64_t};
use super::colour::colour_palette;
use super::event::Timer;
use super::grid::{grid_cell, utf8_data};
use super::pane::window_pane;
use super::screen_write::screen_write_ctx;
pub const INPUT_REQUEST_QUEUE: input_request_type = 2;

pub const INPUT_REQUEST_CLIPBOARD: input_request_type = 1;

pub const INPUT_REQUEST_PALETTE: input_request_type = 0;

pub const INPUT_BUF_DEFAULT_SIZE: ::core::ffi::c_int = 1048576 as ::core::ffi::c_int;

#[derive(Default)]
pub enum InputPalette {
    #[default]
    None,
    Pane(std::rc::Weak<std::cell::UnsafeCell<window_pane>>),
    Popup(refbox::Weak<colour_palette>),
}

impl InputPalette {
    /// Borrow the current palette for one operation; no guard crosses parser callbacks.
    pub fn with_mut<R>(&self, access: impl FnOnce(&mut colour_palette) -> R) -> Option<R> {
        match self {
            Self::None => None,
            Self::Pane(observer) => {
                let owner = observer.upgrade()?;
                let result = {
                    let mut palette = unsafe { owner.borrow_palette_mut() };
                    access(palette)
                };
                unsafe { owner.release(c"input palette") };
                Some(result)
            }
            Self::Popup(observer) => {
                let mut palette = match observer.try_borrow_mut() {
                    Ok(palette) => palette,
                    Err(refbox::BorrowError::Dropped) => return None,
                    Err(refbox::BorrowError::Borrowed) => panic!("popup palette already borrowed"),
                };
                Some(access(&mut palette))
            }
        }
    }
}

pub type input_request_type = ::core::ffi::c_uint;

#[repr(C)]
pub struct input_ctx {
    /// Pane lifetime is owned by the pane index and retained event handlers.
    pub wp: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub event: crate::src::reactor::StreamHandle,
    pub ctx: screen_write_ctx,
    pub palette: InputPalette,
    /// Client observation; request/context ownership does not retain the client.
    pub c: ClientWeak,
    pub cell: input_cell,
    pub old_cell: input_cell,
    pub old_cx: u_int,
    pub old_cy: u_int,
    pub old_mode: ::core::ffi::c_int,
    pub interm_buf: [u_char; 4],
    pub interm_len: size_t,
    pub param_buf: [u_char; 64],
    pub param_len: size_t,
    pub input_buf: Vec<u_char>,
    pub input_len: size_t,
    pub input_end: input_end_type,
    pub param_list: [input_param; 24],
    pub param_list_len: u_int,
    pub utf8data: utf8_data,
    pub utf8started: ::core::ffi::c_int,
    pub ch: ::core::ffi::c_int,
    pub last: utf8_data,
    pub state: &'static input_state,
    pub flags: ::core::ffi::c_int,
    pub(crate) requests: VecDeque<Box<input_request>>,
    pub request_timer: Option<Timer>,
    pub since_ground: Box<SegmentedBuf>,
    pub ground_timer: Option<Timer>,
}

#[repr(C)]
pub struct input_request {
    /// Client observation; request/context ownership does not retain the client.
    pub c: ClientWeak,
    pub ictx: *mut input_ctx,
    pub type_0: input_request_type,
    pub t: uint64_t,
    pub end: input_end_type,
    pub idx: ::core::ffi::c_int,
    /// Formatted reply owned by a queued request; absent for terminal queries.
    pub data: Option<CString>,
}

pub type input_end_type = ::core::ffi::c_uint;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct input_state {
    pub name: &'static ::std::ffi::CStr,
    pub enter: Option<unsafe fn(*mut input_ctx)>,
    pub exit: Option<unsafe fn(*mut input_ctx)>,
    pub transitions: &'static [input_transition],
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct input_transition {
    pub first: ::core::ffi::c_int,
    pub last: ::core::ffi::c_int,
    pub handler: Option<unsafe fn(*mut input_ctx) -> ::core::ffi::c_int>,
    pub state: Option<&'static input_state>,
}

// Rust representation of tmux's tagged parameter union. Each string is owned
// by its parameter; resetting or dropping the parameter releases it.
#[derive(Default)]
pub enum input_param {
    #[default]
    Missing,
    Number(::core::ffi::c_int),
    String(CString),
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
