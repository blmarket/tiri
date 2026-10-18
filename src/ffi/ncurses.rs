//! Foreign declarations supplied by ncurses.

use crate::shared::terminal::term;
pub type TERMINAL = term;

extern "C" {
    pub static mut cur_term: *mut TERMINAL;
    pub fn del_curterm(_: *mut TERMINAL) -> ::core::ffi::c_int;
    pub fn setupterm(
        _: *const ::core::ffi::c_char,
        _: ::core::ffi::c_int,
        _: *mut ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
    pub fn tigetflag(_: *const ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn tigetnum(_: *const ::core::ffi::c_char) -> ::core::ffi::c_int;
    pub fn tigetstr(_: *const ::core::ffi::c_char) -> *mut ::core::ffi::c_char;
    pub fn tiparm_s(
        _: ::core::ffi::c_int,
        _: ::core::ffi::c_int,
        _: *const ::core::ffi::c_char,
        ...
    ) -> *mut ::core::ffi::c_char;
}
