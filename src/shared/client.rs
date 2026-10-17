//! Authoritative client objects, file transfers, overlays, and scalar domains.
use std::cell::UnsafeCell;
use std::rc::Rc;

use super::abi::u_int;
use super::display::visible_ranges;
use super::key::key_event;
use super::screen::ScreenMode;

pub type client_exit_type = ::core::ffi::c_uint;

pub const CLIENT_EXIT_DETACH: client_exit_type = 2;
pub const CLIENT_EXIT_SHUTDOWN: client_exit_type = 1;

pub type client_exit_reason = ::core::ffi::c_uint;

pub const CLIENT_EXIT_MESSAGE_PROVIDED: client_exit_reason = 8;
pub const CLIENT_EXIT_SERVER_EXITED: client_exit_reason = 7;
pub const CLIENT_EXIT_EXITED: client_exit_reason = 6;
pub const CLIENT_EXIT_LOST_SERVER: client_exit_reason = 5;
pub const CLIENT_EXIT_TERMINATED: client_exit_reason = 4;
pub const CLIENT_EXIT_LOST_TTY: client_exit_reason = 3;
pub const CLIENT_EXIT_DETACHED_HUP: client_exit_reason = 2;
pub const CLIENT_EXIT_DETACHED: client_exit_reason = 1;
pub const CLIENT_EXIT_NONE: client_exit_reason = 0;

pub const CLIENT_CONTROL: ::core::ffi::c_int = 0x2000 as ::core::ffi::c_int;
pub const CLIENT_LOGIN: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CLIENT_NOSTARTSERVER: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;
pub const CLIENT_CONTROLCONTROL: ::core::ffi::c_int = 0x4000 as ::core::ffi::c_int;
pub const CLIENT_STARTSERVER: ::core::ffi::c_int = 0x10000000 as ::core::ffi::c_int;
pub const CLIENT_CONTROL_WAITEXIT: ::core::ffi::c_ulonglong =
    0x200000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_WRITE_ACK: ::core::ffi::c_ulonglong = 0x4000000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_ATTACHED: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const CLIENT_READONLY: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;
pub const CLIENT_IGNORESIZE: ::core::ffi::c_int = 0x20000 as ::core::ffi::c_int;
pub const CLIENT_DEAD: ::core::ffi::c_int = 0x200 as ::core::ffi::c_int;
pub const CLIENT_UTF8: ::core::ffi::c_int = 0x10000 as ::core::ffi::c_int;
pub const CLIENT_STATUSFORCE: ::core::ffi::c_int = 0x80000 as ::core::ffi::c_int;
pub const CLIENT_SIZECHANGED: ::core::ffi::c_int = 0x400000 as ::core::ffi::c_int;
pub const CLIENT_WINDOWSIZECHANGED: ::core::ffi::c_ulonglong =
    0x400000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_CONTROL_NEWLAYOUTS: ::core::ffi::c_ulonglong =
    0x800000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_REDRAWSTATUS: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const CLIENT_REDRAWBORDERS: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;
pub const CLIENT_EXIT: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const CLIENT_SUSPENDED: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const CLIENT_CONTROL_NOOUTPUT: ::core::ffi::c_int = 0x4000000 as ::core::ffi::c_int;
pub const CLIENT_CONTROL_PAUSEAFTER: ::core::ffi::c_ulonglong =
    0x100000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_CONTROL_DISCARD: ::core::ffi::c_ulonglong =
    0x10000000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_UNATTACHEDFLAGS: ::core::ffi::c_int = CLIENT_DEAD | CLIENT_SUSPENDED | CLIENT_EXIT;
pub const CLIENT_REDRAWOVERLAY: ::core::ffi::c_int = 0x2000000 as ::core::ffi::c_int;
pub const CLIENT_STATUSOFF: ::core::ffi::c_int = 0x800000 as ::core::ffi::c_int;
pub const CLIENT_NOSIZEFLAGS: ::core::ffi::c_int = CLIENT_DEAD | CLIENT_SUSPENDED | CLIENT_EXIT;
pub const CLIENT_REDRAWWINDOW: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const CLIENT_REDRAWMENU: ::core::ffi::c_int = 0x20000000 as ::core::ffi::c_int;
pub const CLIENT_IDENTIFIED: ::core::ffi::c_int = 0x40000 as ::core::ffi::c_int;
pub const CLIENT_DEFAULTSOCKET: ::core::ffi::c_int = 0x8000000 as ::core::ffi::c_int;
pub const CLIENT_NOFORK: ::core::ffi::c_int = 0x40000000 as ::core::ffi::c_int;
pub const CLIENT_PASTE_TIME_LIMIT: ::core::ffi::c_int = 5 as ::core::ffi::c_int;
pub const CLIENT_TERMINAL: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CLIENT_REPEAT: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const CLIENT_EXITED: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const CLIENT_FOCUSED: ::core::ffi::c_int = 0x8000 as ::core::ffi::c_int;
pub const CLIENT_DOUBLECLICK: ::core::ffi::c_int = 0x100000 as ::core::ffi::c_int;
pub const CLIENT_TRIPLECLICK: ::core::ffi::c_int = 0x200000 as ::core::ffi::c_int;
pub const CLIENT_REDRAWSTATUSALWAYS: ::core::ffi::c_int = 0x1000000 as ::core::ffi::c_int;
pub const CLIENT_REDRAWSCROLLBARS: ::core::ffi::c_ulonglong =
    0x80000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_BRACKETPASTING: ::core::ffi::c_ulonglong =
    0x1000000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_ASSUMEPASTING: ::core::ffi::c_ulonglong = 0x2000000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_NO_DETACH_ON_DESTROY: ::core::ffi::c_ulonglong =
    0x8000000000 as ::core::ffi::c_ulonglong;
pub const CLIENT_ALLREDRAWFLAGS: ::core::ffi::c_int = CLIENT_REDRAWWINDOW
    | CLIENT_REDRAWSTATUS
    | CLIENT_REDRAWSTATUSALWAYS
    | CLIENT_REDRAWBORDERS
    | CLIENT_REDRAWOVERLAY
    | CLIENT_REDRAWMENU;
pub const CLIENT_NODETACHFLAGS: ::core::ffi::c_int = CLIENT_DEAD | CLIENT_EXIT;

pub use crate::src::server_client::client;

/// Retained Client identity. Consumers use Client operations and bounded
/// component borrows; the cell representation is selected here.
pub type ClientRef = Rc<UnsafeCell<client>>;
/// Nonowning Client identity retained by callbacks and back references.
pub type ClientWeak = std::rc::Weak<UnsafeCell<client>>;

pub use crate::src::file::{client_file, client_file_cb, client_files};

pub type overlay_resize_cb = Option<Box<dyn FnMut(&ClientRef)>>;

pub type overlay_key_cb = Option<Box<dyn FnMut(&ClientRef, &mut key_event) -> i32>>;

pub type overlay_draw_cb = Option<Box<dyn FnMut(&ClientRef)>>;

pub type overlay_mode_cb = Option<Box<dyn FnMut(&ClientRef) -> Option<(ScreenMode, u_int, u_int)>>>;

pub type overlay_check_cb =
    Option<Box<dyn FnMut(&ClientRef, u_int, u_int, u_int) -> visible_ranges>>;

pub fn client_handle(owner: &Option<ClientRef>) -> Option<&ClientRef> {
    owner.as_ref()
}

#[cfg(test)]
mod tests {
    use super::{
        client_exit_reason, client_exit_type, CLIENT_EXIT_DETACH, CLIENT_EXIT_DETACHED,
        CLIENT_EXIT_MESSAGE_PROVIDED, CLIENT_EXIT_NONE,
    };
    use ::core::mem::{align_of, size_of};

    #[test]
    fn client_exit_domains_match_the_translated_c_baseline() {
        assert_eq!(size_of::<client_exit_type>(), 4);
        assert_eq!(align_of::<client_exit_type>(), 4);
        assert_eq!(size_of::<client_exit_reason>(), 4);
        assert_eq!(align_of::<client_exit_reason>(), 4);
        assert_eq!(CLIENT_EXIT_DETACH, 2);
        assert_eq!(CLIENT_EXIT_MESSAGE_PROVIDED, 8);
        assert_eq!(CLIENT_EXIT_DETACHED, 1);
        assert_eq!(CLIENT_EXIT_NONE, 0);
    }
}
