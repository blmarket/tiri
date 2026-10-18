//! Application callback handles for the Rust reactor.
//!
//! Scheduling and registration ownership live in the reactor's Rust collections.
//! Timer handles schedule monotonic waits; application tasks await I/O and signals.
//! Streams own their buffers; these layouts are independent of libevent.
use super::abi::*;
pub use crate::reactor::Timer;
use tiri_buffer::SegmentedBuf;

#[derive(Default)]
#[repr(C)]
pub struct bufferevent {
    pub(crate) state: Option<std::rc::Rc<crate::reactor::StreamState>>,
    pub input: Box<SegmentedBuf>,
    pub output: Box<SegmentedBuf>,
    pub wm_read: event_watermark,
    pub wm_write: event_watermark,
    pub readcb: bufferevent_data_cb,
    pub writecb: bufferevent_data_cb,
    pub errorcb: bufferevent_event_cb,
    pub enabled: ::core::ffi::c_short,
}

pub type bufferevent_event_cb = Option<
    std::rc::Rc<
        std::cell::RefCell<Box<dyn FnMut(std::ptr::NonNull<bufferevent>, ::core::ffi::c_short)>>,
    >,
>;
pub type bufferevent_data_cb =
    Option<std::rc::Rc<std::cell::RefCell<Box<dyn FnMut(std::ptr::NonNull<bufferevent>)>>>>;

pub fn bufferevent_data_callback(
    callback: impl FnMut(std::ptr::NonNull<bufferevent>) + 'static,
) -> bufferevent_data_cb {
    Some(std::rc::Rc::new(std::cell::RefCell::new(Box::new(
        callback,
    ))))
}

pub fn bufferevent_event_callback(
    callback: impl FnMut(std::ptr::NonNull<bufferevent>, ::core::ffi::c_short) + 'static,
) -> bufferevent_event_cb {
    Some(std::rc::Rc::new(std::cell::RefCell::new(Box::new(
        callback,
    ))))
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct event_watermark {
    pub low: size_t,
    pub high: size_t,
}

pub const EV_READ: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const EV_WRITE: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
