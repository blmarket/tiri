//! Authoritative mouse declarations, shared by the C translation units.

use super::{abi::u_int, key::key_code};
#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct mouse_event {
    pub valid: ::core::ffi::c_int,
    pub ignore: ::core::ffi::c_int,
    pub key: key_code,
    pub statusat: ::core::ffi::c_int,
    pub statuslines: u_int,
    pub x: u_int,
    pub y: u_int,
    pub b: u_int,
    pub lx: u_int,
    pub ly: u_int,
    pub lb: u_int,
    pub ox: u_int,
    pub oy: u_int,
    pub s: ::core::ffi::c_int,
    pub w: ::core::ffi::c_int,
    pub wp: ::core::ffi::c_int,
    pub sgr_type: u_int,
    pub sgr_b: u_int,
}
pub const MOUSE_PARAM_MAX: ::core::ffi::c_int = 0xff as ::core::ffi::c_int;
pub const MOUSE_PARAM_UTF8_MAX: ::core::ffi::c_int = 0x7ff as ::core::ffi::c_int;
pub const MOUSE_PARAM_BTN_OFF: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const MOUSE_PARAM_POS_OFF: ::core::ffi::c_int = 0x21 as ::core::ffi::c_int;
pub const MOUSE_MASK_BUTTONS: ::core::ffi::c_int = 195 as ::core::ffi::c_int;
pub const MOUSE_MASK_DRAG: ::core::ffi::c_int = 32 as ::core::ffi::c_int;
pub const MOUSE_WHEEL_UP: ::core::ffi::c_int = 64 as ::core::ffi::c_int;
pub const MOUSE_WHEEL_DOWN: ::core::ffi::c_int = 65 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_1: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const MOUSE_MASK_SHIFT: ::core::ffi::c_int = 4 as ::core::ffi::c_int;
pub const MOUSE_MASK_META: ::core::ffi::c_int = 8 as ::core::ffi::c_int;
pub const MOUSE_MASK_CTRL: ::core::ffi::c_int = 16 as ::core::ffi::c_int;
pub const MOUSE_MASK_MODIFIERS: ::core::ffi::c_int =
    MOUSE_MASK_SHIFT | MOUSE_MASK_META | MOUSE_MASK_CTRL;
pub const MOUSE_BUTTON_3: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_2: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_6: ::core::ffi::c_int = 66 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_7: ::core::ffi::c_int = 67 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_8: ::core::ffi::c_int = 128 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_9: ::core::ffi::c_int = 129 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_10: ::core::ffi::c_int = 130 as ::core::ffi::c_int;
pub const MOUSE_BUTTON_11: ::core::ffi::c_int = 131 as ::core::ffi::c_int;
