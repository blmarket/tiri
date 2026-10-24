//! Terminal state owned by a client; only the tty module sees its fields.
use crate::shared::abi::{size_t, time_t, u_int};
use crate::shared::display::{screen_cursor_style, visible_ranges};
use crate::shared::event::Timer;
use crate::shared::grid::grid_cell;
use crate::shared::key::key_code;
use crate::shared::terminal::TerminalAttributes;
use tiri_buffer::{Buf as _, SegmentedBuf};

pub const TERM_256COLOURS: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const TERM_RGBCOLOURS: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub(super) const TTY_OPENED: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub(super) const TTY_STARTED: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub(super) const TTY_NOCURSOR: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub(super) const TTY_FREEZE: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub(super) const TTY_BLOCK: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub(super) const TERM_NOAM: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub(super) const TERM_DECSLRM: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub(super) const TERM_DECFRA: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub(super) const TERM_VT100LIKE: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub(super) const TTY_TIMER: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub(super) const TTY_NOBLOCK: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub(super) const TTY_OSC52QUERY: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub(super) const TTY_HAVEDA: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub(super) const TTY_HAVEXDA: ::core::ffi::c_int = 0x200 as ::core::ffi::c_int;
pub(super) const TTY_SYNCING: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;
pub(super) const TTY_HAVEDA2: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;
pub(super) const TTY_WINSIZEQUERY: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;
pub(super) const TTY_WAITFG: ::core::ffi::c_int = 0x2000 as ::core::ffi::c_int;
pub(super) const TTY_WAITBG: ::core::ffi::c_int = 0x4000 as ::core::ffi::c_int;
pub(super) const TTY_HAVESYNC: ::core::ffi::c_int = 0x10000 as ::core::ffi::c_int;
pub(super) const TTY_ALL_REQUEST_FLAGS: ::core::ffi::c_int =
    TTY_HAVEDA | TTY_HAVEDA2 | TTY_HAVEXDA | TTY_HAVESYNC;
pub(super) const TTY_BLOCK_INTERVAL: ::core::ffi::c_int = 100000 as ::core::ffi::c_int;
pub(super) const TTY_QUERY_TIMEOUT: ::core::ffi::c_int = 5 as ::core::ffi::c_int;
pub(super) const TTY_REQUEST_LIMIT: ::core::ffi::c_int = 30 as ::core::ffi::c_int;
pub(super) const TERM_SIXEL: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub(super) const TTY_BRACKETPASTE: ::core::ffi::c_int = 0x8000 as ::core::ffi::c_int;
pub(super) const TERM_INVALIDMS: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub(super) const TERM_NOREPLACE: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;

/// A ternary search tree node with exclusively owned children.
#[repr(C)]
pub struct tty_key {
    pub(super) ch: ::core::ffi::c_char,
    pub(super) key: key_code,
    pub(super) left: Option<Box<tty_key>>,
    pub(super) right: Option<Box<tty_key>>,
    pub(super) next: Option<Box<tty_key>>,
}

/// A terminal capability owns its payload, including any string allocation.
#[derive(Clone, Default)]
pub enum tty_code {
    #[default]
    None,
    String(std::ffi::CString),
    Number(::core::ffi::c_int),
    Flag(::core::ffi::c_int),
}

#[derive(Default)]
pub struct tty {
    pub(super) start_timer: Option<Timer>,
    pub(super) clipboard_timer: Option<Timer>,
    pub(super) last_requests: time_t,
    pub(super) sx: u_int,
    pub(super) sy: u_int,
    pub(super) xpixel: u_int,
    pub(super) ypixel: u_int,
    pub(super) cx: u_int,
    pub(super) cy: u_int,
    pub(super) cstyle: screen_cursor_style,
    pub(super) ccolour: ::core::ffi::c_int,
    pub(super) oflag: ::core::ffi::c_int,
    pub(super) oox: u_int,
    pub(super) ooy: u_int,
    pub(super) osx: u_int,
    pub(super) osy: u_int,
    pub(super) mode: ::core::ffi::c_int,
    pub(super) fg: ::core::ffi::c_int,
    pub(super) bg: ::core::ffi::c_int,
    pub(super) rlower: u_int,
    pub(super) rupper: u_int,
    pub(super) rleft: u_int,
    pub(super) rright: u_int,
    pub(super) read_task: Option<tiri_rt::mio::Task>,
    pub(super) in_0: Option<Box<super::input::TerminalInput>>,
    pub(super) write_task: Option<tiri_rt::mio::Task>,
    pub(super) io_fd: Option<std::os::fd::OwnedFd>,
    pub(super) out: Option<Box<SegmentedBuf>>,
    pub(super) timer: Option<Timer>,
    pub(super) discarded: size_t,
    pub(super) tio: TerminalAttributes,
    pub(super) r: visible_ranges,
    pub(super) cell: grid_cell,
    pub(super) last_cell: grid_cell,
    pub(super) flags: ::core::ffi::c_int,
    /// Box-owned terminal record while TTY_OPENED is set; invalidated by tty_close.
    pub(super) term: Option<Box<tty_term>>,
    pub(super) mouse_last_x: u_int,
    pub(super) mouse_last_y: u_int,
    pub(super) mouse_last_b: u_int,
    pub(super) key_timer: Option<Timer>,
    /// Exclusively owned terminal key tree; cleared on rebuild or tty_close.
    pub(super) key_tree: Option<Box<tty_key>>,
}

impl tty {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn size(&self) -> (u_int, u_int) {
        (self.sx, self.sy)
    }
    pub fn pixel_size(&self) -> (u_int, u_int) {
        (self.xpixel, self.ypixel)
    }
    pub fn mode(&self) -> ::core::ffi::c_int {
        self.mode
    }
    /// Foreground and background colours reported by the terminal, -1 if unknown.
    pub fn reported_colours(&self) -> (::core::ffi::c_int, ::core::ffi::c_int) {
        (self.fg, self.bg)
    }
    pub fn is_opened(&self) -> bool {
        self.flags & TTY_OPENED != 0
    }
    pub fn is_started(&self) -> bool {
        self.flags & TTY_STARTED != 0
    }
    #[cfg(test)]
    pub(crate) fn set_started_for_test(&mut self, started: bool) {
        if started {
            self.flags |= TTY_STARTED;
        } else {
            self.flags &= !TTY_STARTED;
        }
    }
    pub fn is_blocked(&self) -> bool {
        self.flags & TTY_BLOCK != 0
    }
    pub fn pending_output(&self) -> usize {
        self.out.as_deref().expect("open TTY buffer").remaining()
    }
    /// A frozen terminal ignores pane output until unfrozen.
    pub fn set_frozen(&mut self, frozen: bool) {
        if frozen {
            self.flags |= TTY_FREEZE;
        } else {
            self.flags &= !TTY_FREEZE;
        }
    }
    pub fn set_cursor_hidden(&mut self, hidden: bool) {
        if hidden {
            self.flags |= TTY_NOCURSOR;
        } else {
            self.flags &= !TTY_NOCURSOR;
        }
    }
    /// Clear the blocked state for a forced draw; pass the result to `resume_block`.
    pub fn suspend_block(&mut self) -> bool {
        let blocked = self.is_blocked();
        self.flags &= !TTY_BLOCK;
        blocked
    }
    pub fn resume_block(&mut self, blocked: bool) {
        if blocked {
            self.flags |= TTY_BLOCK;
        }
    }
    /// Unblock, unfreeze and hide the cursor while a full redraw is written.
    pub fn begin_redraw(&mut self) -> TtyRedrawState {
        let saved = self.flags & (TTY_BLOCK | TTY_FREEZE | TTY_NOCURSOR);
        self.flags = self.flags & !(TTY_BLOCK | TTY_FREEZE) | TTY_NOCURSOR;
        TtyRedrawState(saved)
    }
    /// Restore the cursor visibility saved by `begin_redraw` before updating modes.
    pub fn restore_redraw_cursor(&mut self, saved: &TtyRedrawState) {
        self.flags = self.flags & !TTY_NOCURSOR | saved.0 & TTY_NOCURSOR;
    }
    pub fn end_redraw(&mut self, saved: TtyRedrawState) {
        self.flags = self.flags & !(TTY_BLOCK | TTY_FREEZE | TTY_NOCURSOR) | saved.0;
    }
}

/// Terminal state saved across a redraw by `tty::begin_redraw`.
pub struct TtyRedrawState(::core::ffi::c_int);

#[repr(C)]
pub struct tty_term {
    pub(super) name: std::ffi::CString,
    pub(super) applied_features: ::core::ffi::c_int,
    pub(super) acs: [[u8; 2]; 256],
    pub(super) codes: Box<[tty_code]>,
    pub(super) flags: ::core::ffi::c_int,
    pub(super) identity: std::rc::Rc<()>,
    pub(super) registered: bool,
}

pub(super) type tty_code_type = ::core::ffi::c_uint;

macro_rules! terminal_value {
    ($owner:expr, $field:ident $(.$member:ident)*) => {{
        let terminal = $owner.borrow_terminal();
        terminal.$field$(.$member)*
    }};
}
macro_rules! terminal_set {
    ($owner:expr, $field:ident $(.$member:ident)*, $operator:tt, $value:expr) => {{
        let value = $value;
        let mut terminal = $owner.borrow_terminal_mut();
        terminal.$field$(.$member)* $operator value;
    }};
}
pub(super) use {terminal_set, terminal_value};
