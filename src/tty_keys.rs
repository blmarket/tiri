use crate::src::bracketed_paste::{
    match_bracketed_paste_boundary, BracketedPasteBoundary, BracketedPasteBoundaryMatch,
};
use crate::src::events::events_fire_client;
use crate::src::ffi::libc::{strlcpy, strsep};
use crate::src::ffi::resolv::b64_pton;
use crate::src::input::{input_request_reply, InputRequestReply};
use crate::src::key_string::key_string_format;
use crate::src::log::{log_cstr, log_cstr_n, log_debug, log_get_level, log_hex};
use crate::src::options::options_get_number;
use crate::src::paste::paste_add_owned;
use crate::src::reactor::Timer;
use crate::src::server_client::Client as _;
use crate::src::window::Window as _;
use libc::{sscanf, strcspn, strncmp, strtol, strtoul};

use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_FOCUSED;
use crate::src::shared::control_character::{C0_CR, C0_ESC, C0_HT, C0_NUL};
use crate::src::shared::grid::*;
use crate::src::shared::input::{input_request_clipboard_data, input_request_palette_data};
use crate::src::shared::key::key_event;
use crate::src::shared::key::*;
use crate::src::shared::mouse::{
    mouse_event, MOUSE_MASK_BUTTONS, MOUSE_PARAM_BTN_OFF, MOUSE_PARAM_POS_OFF, MOUSE_WHEEL_DOWN,
    MOUSE_WHEEL_UP,
};
use crate::src::shared::posix_terminal::VERASE;
use crate::src::shared::tty::*;
use crate::src::shared::tty::{tty, tty_key};
use crate::src::shared::tty::{
    TTY_ALL_REQUEST_FLAGS, TTY_BRACKETPASTE, TTY_HAVEDA, TTY_HAVEDA2, TTY_HAVESYNC, TTY_HAVEXDA,
    TTY_OSC52QUERY, TTY_TIMER, TTY_WAITBG, TTY_WAITFG, TTY_WINSIZEQUERY,
};
use crate::src::shared::utf8::wchar_t;
use crate::src::shared::utf8::*;
use crate::src::style::colour::{colour_format, colour_parse_x11_logged};
use crate::src::text::utf8::{utf8_append, utf8_from_data, utf8_fromwc, utf8_open};
use crate::src::tmux::global_options;
use crate::src::tty::{tty_invalidate, tty_update_features};
use crate::src::tty_features::{tty_default_features, tty_parse_client_features};
use crate::src::tty_term::tty_term_owner_ptr;
use crate::src::tty_term::tty_term_string;

use std::ffi::CStr;
use std::time::Duration;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct tty_default_key_code {
    pub code: tty_code_code,
    pub key: key_code,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct tty_default_key_raw {
    pub string: &'static ::std::ffi::CStr,
    pub key: key_code,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct tty_default_key_xterm {
    pub template: &'static ::std::ffi::CStr,
    pub key: key_code,
}
pub use ::libc::_POSIX_VDISABLE;

static mut tty_default_raw_keys: [tty_default_key_raw; 102] = [
    tty_default_key_raw {
        string: c"\x1BO[",
        key: '\u{1b}' as i32 as key_code,
    },
    tty_default_key_raw {
        string: c"\x1BOo",
        key: KEYC_KP_SLASH as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOj",
        key: KEYC_KP_STAR as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOm",
        key: KEYC_KP_MINUS as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOw",
        key: KEYC_KP_SEVEN as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOx",
        key: KEYC_KP_EIGHT as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOy",
        key: KEYC_KP_NINE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOk",
        key: KEYC_KP_PLUS as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOt",
        key: KEYC_KP_FOUR as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOu",
        key: KEYC_KP_FIVE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOv",
        key: KEYC_KP_SIX as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOq",
        key: KEYC_KP_ONE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOr",
        key: KEYC_KP_TWO as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOs",
        key: KEYC_KP_THREE as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOM",
        key: KEYC_KP_ENTER as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOp",
        key: KEYC_KP_ZERO as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOn",
        key: KEYC_KP_PERIOD as ::core::ffi::c_ulong as key_code | KEYC_KEYPAD,
    },
    tty_default_key_raw {
        string: c"\x1BOA",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1BOB",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1BOC",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1BOD",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1B[A",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1B[B",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1B[C",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1B[D",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_raw {
        string: c"\x1B\x1BOA",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1BOB",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1BOC",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1BOD",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1B[A",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1B[B",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1B[C",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1B[D",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR | KEYC_META,
    },
    tty_default_key_raw {
        string: c"\x1BOH",
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1BOF",
        key: KEYC_END as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B\x1BOH",
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1BOF",
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_raw {
        string: c"\x1B[H",
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[F",
        key: KEYC_END as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B\x1B[H",
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_raw {
        string: c"\x1B\x1B[F",
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_raw {
        string: c"\x1BOa",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1BOb",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1BOc",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1BOd",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[a",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[b",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[c",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[d",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[11~",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[12~",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[13~",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[14~",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[15~",
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[17~",
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[18~",
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[19~",
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[20~",
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[21~",
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[23~",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[24~",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[25~",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[26~",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[28~",
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[29~",
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[31~",
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[32~",
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[33~",
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[34~",
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[23$",
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[24$",
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[11^",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[12^",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[13^",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[14^",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[15^",
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[17^",
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[18^",
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[19^",
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[20^",
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[21^",
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[23^",
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[24^",
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_raw {
        string: c"\x1B[11@",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[12@",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[13@",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[14@",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[15@",
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[17@",
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[18@",
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[19@",
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[20@",
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[21@",
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[23@",
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[24@",
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[I",
        key: KEYC_FOCUS_IN as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[O",
        key: KEYC_FOCUS_OUT as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[200~",
        key: KEYC_PASTE_START as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    tty_default_key_raw {
        string: c"\x1B[201~",
        key: KEYC_PASTE_END as ::core::ffi::c_ulong as key_code | KEYC_IMPLIED_META,
    },
    tty_default_key_raw {
        string: c"\x1B[1;5Z",
        key: '\t' as i32 as key_code | KEYC_CTRL | KEYC_SHIFT,
    },
    tty_default_key_raw {
        string: c"\x1B[?997;1n",
        key: KEYC_REPORT_DARK_THEME as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_raw {
        string: c"\x1B[?997;2n",
        key: KEYC_REPORT_LIGHT_THEME as ::core::ffi::c_ulong as key_code,
    },
];
static mut tty_default_xterm_keys: [tty_default_key_xterm; 30] = [
    tty_default_key_xterm {
        template: c"\x1B[1;_P",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO1;_P",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO_P",
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_Q",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO1;_Q",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO_Q",
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_R",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO1;_R",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO_R",
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_S",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO1;_S",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1BO_S",
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[15;_~",
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[17;_~",
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[18;_~",
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[19;_~",
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[20;_~",
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[21;_~",
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[23;_~",
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[24;_~",
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_A",
        key: KEYC_UP as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_B",
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_C",
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_D",
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_H",
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[1;_F",
        key: KEYC_END as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[5;_~",
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[6;_~",
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[2;_~",
        key: KEYC_IC as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_xterm {
        template: c"\x1B[3;_~",
        key: KEYC_DC as ::core::ffi::c_ulong as key_code,
    },
];
static mut tty_default_xterm_modifiers: [key_code; 10] = [
    0 as ::core::ffi::c_int as key_code,
    0 as ::core::ffi::c_int as key_code,
    KEYC_SHIFT,
    KEYC_META | KEYC_IMPLIED_META,
    KEYC_SHIFT | KEYC_META | KEYC_IMPLIED_META,
    KEYC_CTRL,
    KEYC_SHIFT | KEYC_CTRL,
    KEYC_META | KEYC_IMPLIED_META | KEYC_CTRL,
    KEYC_SHIFT | KEYC_META | KEYC_IMPLIED_META | KEYC_CTRL,
    KEYC_META | KEYC_IMPLIED_META,
];
static mut tty_default_code_keys: [tty_default_key_code; 136] = [
    tty_default_key_code {
        code: TTYC_KF1,
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF2,
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF3,
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF4,
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF5,
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF6,
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF7,
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF8,
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF9,
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF10,
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF11,
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF12,
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KF13,
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF14,
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF15,
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF16,
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF17,
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF18,
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF19,
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF20,
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF21,
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF22,
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF23,
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF24,
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF25,
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF26,
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF27,
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF28,
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF29,
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF30,
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF31,
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF32,
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF33,
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF34,
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF35,
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF36,
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF37,
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF38,
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF39,
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF40,
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF41,
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF42,
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF43,
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF44,
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF45,
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF46,
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF47,
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF48,
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KF49,
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF50,
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF51,
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF52,
        key: KEYC_F4 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF53,
        key: KEYC_F5 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF54,
        key: KEYC_F6 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF55,
        key: KEYC_F7 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF56,
        key: KEYC_F8 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF57,
        key: KEYC_F9 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF58,
        key: KEYC_F10 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF59,
        key: KEYC_F11 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF60,
        key: KEYC_F12 as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KF61,
        key: KEYC_F1 as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF62,
        key: KEYC_F2 as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KF63,
        key: KEYC_F3 as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KICH1,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KDCH1,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KHOME,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KEND,
        key: KEYC_END as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KNP,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KPP,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KCBT,
        key: KEYC_BTAB as ::core::ffi::c_ulong as key_code,
    },
    tty_default_key_code {
        code: TTYC_KCUU1,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_code {
        code: TTYC_KCUD1,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_code {
        code: TTYC_KCUB1,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_code {
        code: TTYC_KCUF1,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CURSOR,
    },
    tty_default_key_code {
        code: TTYC_KDC2,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KDC3,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KDC4,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KDC5,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KDC6,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KDC7,
        key: KEYC_DC as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KIND,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KDN2,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KDN3,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KDN4,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KDN5,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KDN6,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KDN7,
        key: KEYC_DOWN as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KEND2,
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KEND3,
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KEND4,
        key: KEYC_END as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KEND5,
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KEND6,
        key: KEYC_END as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KEND7,
        key: KEYC_END as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KHOM2,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KHOM3,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KHOM4,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KHOM5,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KHOM6,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KHOM7,
        key: KEYC_HOME as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KIC2,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KIC3,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KIC4,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KIC5,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KIC6,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KIC7,
        key: KEYC_IC as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KLFT2,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KLFT3,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KLFT4,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KLFT5,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KLFT6,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KLFT7,
        key: KEYC_LEFT as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KNXT2,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KNXT3,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KNXT4,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KNXT5,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KNXT6,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KNXT7,
        key: KEYC_NPAGE as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KPRV2,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KPRV3,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KPRV4,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KPRV5,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KPRV6,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KPRV7,
        key: KEYC_PPAGE as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KRIT2,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KRIT3,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KRIT4,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KRIT5,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KRIT6,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KRIT7,
        key: KEYC_RIGHT as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KRI,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KUP2,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_SHIFT,
    },
    tty_default_key_code {
        code: TTYC_KUP3,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_META | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KUP4,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code
            | KEYC_SHIFT
            | KEYC_META
            | KEYC_IMPLIED_META,
    },
    tty_default_key_code {
        code: TTYC_KUP5,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KUP6,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code | KEYC_SHIFT | KEYC_CTRL,
    },
    tty_default_key_code {
        code: TTYC_KUP7,
        key: KEYC_UP as ::core::ffi::c_ulong as key_code
            | KEYC_META
            | KEYC_IMPLIED_META
            | KEYC_CTRL,
    },
];
unsafe fn tty_keys_add(tty: *mut tty, s: &CStr, key: key_code) {
    let bytes = s.to_bytes();
    let key_string = key_string_format(key, true);
    let mut size = 0;
    if let Some(tk) = tty_keys_find_mut((*tty).key_tree.as_deref_mut(), bytes, &mut size) {
        log_debug(format_args!(
            "replacing key {}: 0x{:x} ({})",
            log_cstr(s),
            { key },
            crate::src::log::log_bytes(key_string.as_bytes())
        ));
        tk.key = key;
    } else {
        log_debug(format_args!(
            "new key {}: 0x{:x} ({})",
            log_cstr(s),
            { key },
            crate::src::log::log_bytes(key_string.as_bytes())
        ));
        tty_keys_add1(&mut (*tty).key_tree, bytes, key);
    }
}

fn tty_keys_add1(tree: &mut Option<Box<tty_key>>, bytes: &[u8], key: key_code) {
    let Some((&ch, rest)) = bytes.split_first() else {
        return;
    };
    let ch = ch as ::core::ffi::c_char;
    let tk = tree.get_or_insert_with(|| {
        Box::new(tty_key {
            ch,
            key: KEYC_UNKNOWN as key_code,
            left: None,
            right: None,
            next: None,
        })
    });
    match ch.cmp(&tk.ch) {
        std::cmp::Ordering::Less => tty_keys_add1(&mut tk.left, bytes, key),
        std::cmp::Ordering::Greater => tty_keys_add1(&mut tk.right, bytes, key),
        std::cmp::Ordering::Equal if rest.is_empty() => tk.key = key,
        std::cmp::Ordering::Equal => tty_keys_add1(&mut tk.next, rest, key),
    }
}

pub unsafe fn tty_keys_build(mut tty: *mut tty) {
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut copy: [::core::ffi::c_char; 16] = [0; 16];
    let mut key: key_code = 0;
    tty_keys_free(tty);
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[tty_default_key_xterm; 30]>() as usize)
            .wrapping_div(::core::mem::size_of::<tty_default_key_xterm>() as usize)
    {
        let tdkx = &tty_default_xterm_keys[i as usize];
        j = 2 as u_int;
        while (j as usize)
            < (::core::mem::size_of::<[key_code; 10]>() as usize)
                .wrapping_div(::core::mem::size_of::<key_code>() as usize)
        {
            strlcpy(
                &raw mut copy as *mut ::core::ffi::c_char,
                tdkx.template.as_ptr(),
                ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t,
            );
            copy[strcspn(&raw mut copy as *mut ::core::ffi::c_char, c"_".as_ptr()) as usize] =
                ('0' as i32 as u_int).wrapping_add(j) as ::core::ffi::c_char;
            key = tdkx.key | tty_default_xterm_modifiers[j as usize];
            tty_keys_add(
                tty,
                CStr::from_ptr(&raw mut copy as *mut ::core::ffi::c_char),
                key,
            );
            j = j.wrapping_add(1);
        }
        i = i.wrapping_add(1);
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[tty_default_key_raw; 102]>() as usize)
            .wrapping_div(::core::mem::size_of::<tty_default_key_raw>() as usize)
    {
        let tdkr = &tty_default_raw_keys[i as usize];
        s = tdkr.string.as_ptr();
        if *s as ::core::ffi::c_int != '\0' as i32 {
            tty_keys_add(tty, CStr::from_ptr(s), tdkr.key);
        }
        i = i.wrapping_add(1);
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[tty_default_key_code; 136]>() as usize)
            .wrapping_div(::core::mem::size_of::<tty_default_key_code>() as usize)
    {
        let tdkc = &tty_default_code_keys[i as usize];
        s = tty_term_string(
            tty_term_owner_ptr(&(*tty).term).expect("client has a terminal"),
            tdkc.code,
        )
        .as_ptr();
        if *s as ::core::ffi::c_int != '\0' as i32 {
            tty_keys_add(tty, CStr::from_ptr(s), tdkc.key);
        }
        i = i.wrapping_add(1);
    }
    let keys = crate::src::options::options_read_entry(global_options(), c"user-keys", |entry| {
        (0..=KEYC_NUSER as u_int)
            .filter_map(|index| {
                let key = std::ffi::CString::new(index.to_string()).unwrap();
                crate::src::options::options_array_get(entry, &key)
                    .and_then(|value| value.string_ptr())
                    .map(|value| (index, value.to_owned()))
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    for (index, value) in keys {
        tty_keys_add(
            tty,
            &value,
            (KEYC_USER as ::core::ffi::c_ulong).wrapping_add(index as ::core::ffi::c_ulong)
                as key_code,
        );
    }
}
pub unsafe fn tty_keys_free(tty: *mut tty) {
    (*tty).key_tree = None;
}

fn tty_keys_find1<'a>(
    mut tree: Option<&'a tty_key>,
    mut bytes: &[u8],
    size: &mut size_t,
) -> Option<&'a tty_key> {
    while let Some(tk) = tree {
        let (&ch, rest) = bytes.split_first()?;
        match (ch as ::core::ffi::c_char).cmp(&tk.ch) {
            std::cmp::Ordering::Less => tree = tk.left.as_deref(),
            std::cmp::Ordering::Greater => tree = tk.right.as_deref(),
            std::cmp::Ordering::Equal => {
                *size += 1;
                if rest.is_empty() || (tk.next.is_none() && tk.key != KEYC_UNKNOWN as key_code) {
                    return Some(tk);
                }
                bytes = rest;
                tree = tk.next.as_deref();
            }
        }
    }
    None
}

fn tty_keys_find_mut<'a>(
    mut tree: Option<&'a mut tty_key>,
    mut bytes: &[u8],
    size: &mut size_t,
) -> Option<&'a mut tty_key> {
    while let Some(tk) = tree {
        let (&ch, rest) = bytes.split_first()?;
        match (ch as ::core::ffi::c_char).cmp(&tk.ch) {
            std::cmp::Ordering::Less => tree = tk.left.as_deref_mut(),
            std::cmp::Ordering::Greater => tree = tk.right.as_deref_mut(),
            std::cmp::Ordering::Equal => {
                *size += 1;
                if rest.is_empty() || (tk.next.is_none() && tk.key != KEYC_UNKNOWN as key_code) {
                    return Some(tk);
                }
                bytes = rest;
                tree = tk.next.as_deref_mut();
            }
        }
    }
    None
}

unsafe fn tty_keys_next1(
    tty: &tty,
    diagnostic_name: Option<&CStr>,
    bytes: &[u8],
    mut key: *mut key_code,
    mut size: *mut size_t,
    mut expired: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    let mut uc: utf8_char = 0;
    let mut i: u_int = 0;
    log_debug(format_args!(
        "{}: next key is {} ({}) (expired={})",
        log_cstr(diagnostic_name.unwrap_or(c"(null)")),
        { len },
        log_cstr_n(&bytes[..len as usize]),
        { expired }
    ));
    *size = 0;
    let bytes = if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(buf.cast::<u8>(), len)
    };
    if let Some(tk) = tty_keys_find1(tty.key_tree.as_deref(), bytes, &mut *size)
        .filter(|tk| tk.key != KEYC_UNKNOWN as key_code)
    {
        let mut current = Some(tk);
        while let Some(node) = current {
            log_debug(format_args!(
                "{}: keys in list: {}",
                log_cstr(diagnostic_name.unwrap_or(c"(null)")),
                log_hex(node.key)
            ));
            current = node.next.as_deref();
        }
        if tk.next.is_some() && expired == 0 {
            return 1;
        }
        *key = tk.key;
        return 0;
    }
    more = utf8_open(&mut ud, *buf as u_char);
    if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint {
        *size = ud.size as size_t;
        if len < ud.size as size_t {
            if expired == 0 {
                return 1 as ::core::ffi::c_int;
            }
            return -(1 as ::core::ffi::c_int);
        }
        i = 1 as u_int;
        while i < ud.size as u_int {
            more = utf8_append(&mut ud, *buf.offset(i as isize) as u_char);
            i = i.wrapping_add(1);
        }
        if more as ::core::ffi::c_uint != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint {
            return -(1 as ::core::ffi::c_int);
        }
        if utf8_from_data(&ud, &mut uc) as ::core::ffi::c_uint
            != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            return -(1 as ::core::ffi::c_int);
        }
        *key = uc as key_code;
        log_debug(format_args!(
            "{}: UTF-8 key {} {}",
            log_cstr(diagnostic_name.unwrap_or(c"(null)")),
            log_cstr_n(&ud.data[..ud.size as usize]),
            log_hex(*key)
        ));
        return 0 as ::core::ffi::c_int;
    }
    -(1 as ::core::ffi::c_int)
}
unsafe fn tty_keys_winsz(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    mut size: *mut size_t,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut end: size_t = 0;
    let mut tmp: [::core::ffi::c_char; 64] = [0; 64];
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut xpixel: u_int = 0;
    let mut ypixel: u_int = 0;
    let _char_x: u_int = 0;
    let _char_y: u_int = 0;
    *size = 0 as size_t;
    if terminal_client_owner.borrow_terminal().flags & TTY_WINSIZEQUERY == 0 {
        return -(1 as ::core::ffi::c_int);
    }
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    end = 2 as size_t;
    while end < len && end != ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as usize {
        if *buf.add(end) as ::core::ffi::c_int == 't' as i32 {
            break;
        }
        if ::libc::isdigit(*buf.add(end) as u_char as ::core::ffi::c_int) == 0
            && *buf.add(end) as ::core::ffi::c_int != ';' as i32
        {
            break;
        }
        end = end.wrapping_add(1);
    }
    if end == len {
        return 1 as ::core::ffi::c_int;
    }
    if end == ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as usize
        || *buf.add(end) as ::core::ffi::c_int != 't' as i32
    {
        return -(1 as ::core::ffi::c_int);
    }
    for (dst, &src) in tmp.iter_mut().zip(&bytes[2..end]) {
        *dst = src as ::core::ffi::c_char;
    }
    tmp[end.wrapping_sub(2 as size_t) as usize] = '\0' as i32 as ::core::ffi::c_char;
    if sscanf(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"8;%u;%u".as_ptr(),
        &raw mut sy,
        &raw mut sx,
    ) == 2 as ::core::ffi::c_int
    {
        {
            let mut terminal = terminal_client_owner.borrow_terminal_mut();
            terminal.sx = sx;
            terminal.sy = sy;
        }
        *size = end.wrapping_add(1 as size_t);
        return 0 as ::core::ffi::c_int;
    } else if sscanf(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"4;%u;%u".as_ptr(),
        &raw mut ypixel,
        &raw mut xpixel,
    ) == 2 as ::core::ffi::c_int
    {
        {
            let mut terminal = terminal_client_owner.borrow_terminal_mut();
            terminal.xpixel = if xpixel != 0 && terminal.sx != 0 {
                xpixel / terminal.sx
            } else {
                0
            };
            terminal.ypixel = if ypixel != 0 && terminal.sy != 0 {
                ypixel / terminal.sy
            } else {
                0
            };
        }
        tty_invalidate(terminal_client_owner);
        terminal_client_owner.borrow_terminal_mut().flags &= !TTY_WINSIZEQUERY;
        *size = end.wrapping_add(1 as size_t);
        return 0 as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: unrecognized window size sequence: {}",
        log_cstr(
            (terminal_client_owner.name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        log_cstr(CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char))
    ));
    -(1 as ::core::ffi::c_int)
}
pub unsafe fn tty_keys_next(terminal_client_owner: &ClientRef) -> ::core::ffi::c_int {
    let mut current_block: u64;
    let diagnostic_name = terminal_client_owner.name();
    let mut buf: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut len: size_t = 0;
    let mut size: size_t = 0;
    let mut bspace: cc_t = 0;
    let mut delay: ::core::ffi::c_int = 0;
    let mut expired: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut n: ::core::ffi::c_int = 0;
    let mut bg: ::core::ffi::c_int = terminal_client_owner.borrow_terminal().bg;
    let mut key: key_code = 0;
    let mut onlykey: key_code = 0;
    let mut clipboard_reply = None;
    let mut palette_reply = None;
    let mut m: mouse_event = mouse_event {
        valid: 0 as ::core::ffi::c_int,
        ignore: 0,
        key: 0,
        statusat: 0,
        statuslines: 0,
        x: 0,
        y: 0,
        b: 0,
        lx: 0,
        ly: 0,
        lb: 0,
        ox: 0,
        oy: 0,
        s: 0,
        w: 0,
        wp: 0,
        sgr_type: 0,
        sgr_b: 0,
    };
    (buf, len) = {
        let mut terminal = terminal_client_owner.borrow_terminal_mut();
        let bytes = terminal
            .in_0
            .as_deref_mut()
            .expect("open TTY buffer")
            .bytes();
        (bytes.as_ptr().cast(), bytes.len())
    };
    if len == 0 as size_t {
        return 0 as ::core::ffi::c_int;
    }
    let bytes: &[u8] = ::core::slice::from_raw_parts(buf.cast(), len);
    log_debug(format_args!(
        "{}: keys are {} ({})",
        log_cstr(
            (terminal_client_owner.name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        (len) as usize,
        log_cstr_n(&bytes[..len as usize])
    ));
    if terminal_client_owner.attached_session().upgrade().is_none() {
        log_debug(format_args!(
            "{}: discard keys with no session",
            log_cstr(diagnostic_name.as_deref().unwrap_or(c"(null)"))
        ));
        let mut terminal = terminal_client_owner.borrow_terminal_mut();
        drop(terminal.key_timer.take());
        terminal.flags &= !TTY_TIMER;
        terminal
            .in_0
            .as_deref_mut()
            .expect("open TTY buffer")
            .drain(len);
        return 1 as ::core::ffi::c_int;
    }
    match tty_keys_clipboard(bytes, &raw mut size, &mut clipboard_reply) {
        0 => {
            key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
            current_block = 5025795842197473417;
        }
        -1 => {
            current_block = 1917311967535052937;
        }
        1 => {
            current_block = 16977559109335092698;
        }
        _ => {
            current_block = 1917311967535052937;
        }
    }
    if current_block == 1917311967535052937 {
        match tty_keys_sync(terminal_client_owner, bytes, &raw mut size) {
            0 => {
                key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                current_block = 5025795842197473417;
            }
            -1 => {
                current_block = 4166486009154926805;
            }
            1 => {
                current_block = 16977559109335092698;
            }
            _ => {
                current_block = 4166486009154926805;
            }
        }
        match current_block {
            5025795842197473417 => {}
            16977559109335092698 => {}
            _ => {
                match tty_keys_device_attributes(terminal_client_owner, bytes, &raw mut size) {
                    0 => {
                        key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                        current_block = 5025795842197473417;
                    }
                    -1 => {
                        current_block = 15652330335145281839;
                    }
                    1 => {
                        current_block = 16977559109335092698;
                    }
                    _ => {
                        current_block = 15652330335145281839;
                    }
                }
                match current_block {
                    5025795842197473417 => {}
                    16977559109335092698 => {}
                    _ => {
                        match tty_keys_device_attributes2(
                            terminal_client_owner,
                            bytes,
                            &raw mut size,
                        ) {
                            0 => {
                                key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                                current_block = 5025795842197473417;
                            }
                            -1 => {
                                current_block = 224731115979188411;
                            }
                            1 => {
                                current_block = 16977559109335092698;
                            }
                            _ => {
                                current_block = 224731115979188411;
                            }
                        }
                        match current_block {
                            5025795842197473417 => {}
                            16977559109335092698 => {}
                            _ => {
                                match tty_keys_extended_device_attributes(
                                    terminal_client_owner,
                                    bytes,
                                    &raw mut size,
                                ) {
                                    0 => {
                                        key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                                        current_block = 5025795842197473417;
                                    }
                                    -1 => {
                                        current_block = 17478428563724192186;
                                    }
                                    1 => {
                                        current_block = 16977559109335092698;
                                    }
                                    _ => {
                                        current_block = 17478428563724192186;
                                    }
                                }
                                match current_block {
                                    5025795842197473417 => {}
                                    16977559109335092698 => {}
                                    _ => {
                                        let res = {
                                            let mut terminal =
                                                terminal_client_owner.borrow_terminal_mut();
                                            let tty { flags, fg, bg, .. } = &mut *terminal;
                                            tty_keys_colours(
                                                flags,
                                                Some(
                                                    diagnostic_name.as_deref().unwrap_or(c"(null)"),
                                                ),
                                                bytes,
                                                &raw mut size,
                                                fg,
                                                bg,
                                            )
                                        };
                                        match res {
                                            0 => {
                                                key = KEYC_UNKNOWN as ::core::ffi::c_ulong
                                                    as key_code;
                                                if terminal_client_owner.borrow_terminal().bg != bg
                                                {
                                                    if let Some(client) =
                                                        Some(&terminal_client_owner)
                                                    {
                                                        client.update_theme_colours();
                                                    };
                                                }
                                                if let Some(session) = terminal_client_owner
                                                    .attached_session()
                                                    .upgrade()
                                                {
                                                    session.theme_changed();
                                                }
                                                current_block = 5025795842197473417;
                                            }
                                            -1 => {
                                                current_block = 1538046216550696469;
                                            }
                                            1 => {
                                                if terminal_client_owner.borrow_terminal().bg != bg
                                                {
                                                    if let Some(client) =
                                                        Some(&terminal_client_owner)
                                                    {
                                                        client.update_theme_colours();
                                                    };
                                                }
                                                if let Some(session) = terminal_client_owner
                                                    .attached_session()
                                                    .upgrade()
                                                {
                                                    session.theme_changed();
                                                }
                                                current_block = 16977559109335092698;
                                            }
                                            _ => {
                                                current_block = 1538046216550696469;
                                            }
                                        }
                                        match current_block {
                                            16977559109335092698 => {}
                                            5025795842197473417 => {}
                                            _ => {
                                                match tty_keys_palette(
                                                    bytes,
                                                    &raw mut size,
                                                    &mut palette_reply,
                                                ) {
                                                    0 => {
                                                        key = KEYC_UNKNOWN as ::core::ffi::c_ulong
                                                            as key_code;
                                                        current_block = 5025795842197473417;
                                                    }
                                                    -1 => {
                                                        current_block = 1836292691772056875;
                                                    }
                                                    1 => {
                                                        current_block = 16977559109335092698;
                                                    }
                                                    _ => {
                                                        current_block = 1836292691772056875;
                                                    }
                                                }
                                                match current_block {
                                                    5025795842197473417 => {}
                                                    16977559109335092698 => {}
                                                    _ => {
                                                        let res = {
                                                            let mut terminal =
                                                                terminal_client_owner
                                                                    .borrow_terminal_mut();
                                                            tty_keys_mouse(
                                                                &mut *terminal,
                                                                diagnostic_name.as_deref(),
                                                                bytes,
                                                                &raw mut size,
                                                                &raw mut m,
                                                            )
                                                        };
                                                        match res {
                                                            0 => {
                                                                key = KEYC_MOUSE
                                                                    as ::core::ffi::c_ulong
                                                                    as key_code;
                                                                current_block = 5025795842197473417;
                                                            }
                                                            -1 => {
                                                                current_block =
                                                                    17784502470059252271;
                                                            }
                                                            -2 => {
                                                                key = KEYC_MOUSE
                                                                    as ::core::ffi::c_ulong
                                                                    as key_code;
                                                                log_debug(format_args!(
                                                                    "{}: discard key {} {}",
                                                                    log_cstr(
                                                                        (terminal_client_owner
                                                                            .name())
                                                                        .as_deref()
                                                                        .unwrap_or(c"(null)")
                                                                    ),
                                                                    log_cstr_n(
                                                                        &bytes[..size as usize]
                                                                    ),
                                                                    log_hex((key) as u64)
                                                                ));
                                                                terminal_client_owner
                                                                    .borrow_terminal_mut()
                                                                    .in_0
                                                                    .as_deref_mut()
                                                                    .expect("open TTY buffer")
                                                                    .drain(size);
                                                                return 1 as ::core::ffi::c_int;
                                                            }
                                                            1 => {
                                                                current_block =
                                                                    16977559109335092698;
                                                            }
                                                            _ => {
                                                                current_block =
                                                                    17784502470059252271;
                                                            }
                                                        }
                                                        match current_block {
                                                            5025795842197473417 => {}
                                                            16977559109335092698 => {}
                                                            _ => {
                                                                let res = {
                                                                    let terminal =
                                                                        terminal_client_owner
                                                                            .borrow_terminal();
                                                                    tty_keys_extended_key(
                                                                        terminal,
                                                                        diagnostic_name.as_deref(),
                                                                        bytes,
                                                                        &raw mut size,
                                                                        &raw mut key,
                                                                    )
                                                                };
                                                                match res {
                                                                    0 => {
                                                                        current_block =
                                                                            5025795842197473417;
                                                                    }
                                                                    -1 => {
                                                                        current_block =
                                                                            3938820862080741272;
                                                                    }
                                                                    1 => {
                                                                        current_block =
                                                                            16977559109335092698;
                                                                    }
                                                                    _ => {
                                                                        current_block =
                                                                            3938820862080741272;
                                                                    }
                                                                }
                                                                match current_block {
                                                                    5025795842197473417 => {}
                                                                    16977559109335092698 => {}
                                                                    _ => {
                                                                        match tty_keys_winsz(
                                                                            terminal_client_owner,
                                                                            bytes,
                                                                            &raw mut size,
                                                                        ) {
                                                                            0 => {
                                                                                current_block = 1414802762261447502;
                                                                                match current_block {
                                                                                    14661562966503102838 => {
                                                                                        current_block = 16977559109335092698;
                                                                                    }
                                                                                    _ => {
                                                                                        key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                                                                                        current_block = 5025795842197473417;
                                                                                    }
                                                                                }
                                                                            }
                                                                            1 => {
                                                                                current_block = 14661562966503102838;
                                                                                match current_block {
                                                                                    14661562966503102838 => {
                                                                                        current_block = 16977559109335092698;
                                                                                    }
                                                                                    _ => {
                                                                                        key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                                                                                        current_block = 5025795842197473417;
                                                                                    }
                                                                                }
                                                                            }
                                                                            _ => {
                                                                                current_block = 12077302897653652224;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    loop {
        match current_block {
            12077302897653652224 => {
                n = tty_keys_next1(
                    terminal_client_owner.borrow_terminal(),
                    diagnostic_name.as_deref(),
                    bytes,
                    &raw mut key,
                    &raw mut size,
                    expired,
                );
                if n == 0 as ::core::ffi::c_int {
                    current_block = 5025795842197473417;
                    continue;
                }
                if n == 1 as ::core::ffi::c_int {
                    current_block = 16977559109335092698;
                    continue;
                }
                if *buf as ::core::ffi::c_int == '\u{1b}' as i32 && len > 1 as size_t {
                    n = tty_keys_next1(
                        terminal_client_owner.borrow_terminal(),
                        diagnostic_name.as_deref(),
                        &bytes[1..],
                        &raw mut key,
                        &raw mut size,
                        expired,
                    );
                    if n == 0 as ::core::ffi::c_int {
                        if key as ::core::ffi::c_ulonglong & KEYC_IMPLIED_META != 0 {
                            key = '\u{1b}' as i32 as key_code;
                            size = 1 as size_t;
                            current_block = 5025795842197473417;
                            continue;
                        } else {
                            key |= KEYC_META;
                            size = size.wrapping_add(1);
                            current_block = 5025795842197473417;
                            continue;
                        }
                    } else if n == 1 as ::core::ffi::c_int {
                        current_block = 16977559109335092698;
                        continue;
                    }
                }
                if *buf as ::core::ffi::c_int == '\u{1b}' as i32 && len >= 2 as size_t {
                    key = (*buf.offset(1 as ::core::ffi::c_int as isize) as u_char
                        as ::core::ffi::c_ulonglong
                        | KEYC_META) as key_code;
                    size = 2 as size_t;
                } else {
                    key = *buf.offset(0 as ::core::ffi::c_int as isize) as u_char as key_code;
                    size = 1 as size_t;
                }
                if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                    == C0_NUL as ::core::ffi::c_int as ::core::ffi::c_ulonglong
                {
                    key = (' ' as i32 as ::core::ffi::c_ulonglong
                        | KEYC_CTRL
                        | key as ::core::ffi::c_ulonglong & KEYC_META)
                        as key_code;
                }
                bspace = terminal_client_owner.borrow_terminal().tio.0.c_cc[VERASE];
                if bspace != _POSIX_VDISABLE {
                    if key == bspace as key_code {
                        log_debug(format_args!(
                            "{}: key {} is BSpace",
                            log_cstr(
                                (terminal_client_owner.name())
                                    .as_deref()
                                    .unwrap_or(c"(null)")
                            ),
                            log_hex((key) as u64)
                        ));
                        key = KEYC_BSPACE as ::core::ffi::c_ulong as key_code;
                    }
                    if key == bspace as ::core::ffi::c_ulonglong | KEYC_META {
                        log_debug(format_args!(
                            "{}: key {} is M-BSpace",
                            log_cstr(
                                (terminal_client_owner.name())
                                    .as_deref()
                                    .unwrap_or(c"(null)")
                            ),
                            log_hex((key) as u64)
                        ));
                        key = (KEYC_BSPACE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                            | KEYC_META) as key_code;
                    }
                }
                onlykey = (key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY) as key_code;
                if onlykey < 0x20 as key_code
                    && onlykey != C0_HT as ::core::ffi::c_int as key_code
                    && onlykey != C0_CR as ::core::ffi::c_int as key_code
                    && onlykey != C0_ESC as ::core::ffi::c_int as key_code
                {
                    onlykey |= 0x40 as key_code;
                    if onlykey >= 'A' as i32 as key_code && onlykey <= 'Z' as i32 as key_code {
                        onlykey |= 0x20 as key_code;
                    }
                    key = (onlykey as ::core::ffi::c_ulonglong
                        | KEYC_CTRL
                        | key as ::core::ffi::c_ulonglong & KEYC_META)
                        as key_code;
                }
                current_block = 5025795842197473417;
            }
            5025795842197473417 => {
                log_debug(format_args!(
                    "{}: complete key {} {}",
                    log_cstr(
                        (terminal_client_owner.name())
                            .as_deref()
                            .unwrap_or(c"(null)")
                    ),
                    log_cstr_n(&bytes[..size as usize]),
                    log_hex((key) as u64)
                ));
                let event = if key != KEYC_UNKNOWN as key_code {
                    let bytes = bytes[..size].to_vec();
                    Some(key_event::new(key, m, Some(bytes)))
                } else {
                    None
                };
                if let Some(reply) = clipboard_reply.take() {
                    input_request_reply(
                        terminal_client_owner,
                        InputRequestReply::Clipboard(&reply),
                    );
                    let clipboard_query =
                        terminal_client_owner.borrow_terminal().flags & TTY_OSC52QUERY != 0;
                    if clipboard_query {
                        paste_add_owned(None, reply.data.into_boxed_slice());
                        let mut terminal = terminal_client_owner.borrow_terminal_mut();
                        drop(terminal.clipboard_timer.take());
                        terminal.flags &= !TTY_OSC52QUERY;
                    }
                }
                if let Some(reply) = palette_reply.take() {
                    input_request_reply(terminal_client_owner, InputRequestReply::Palette(&reply));
                }
                {
                    let mut terminal = terminal_client_owner.borrow_terminal_mut();
                    drop(terminal.key_timer.take());
                    terminal.flags &= !TTY_TIMER;
                    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                        == KEYC_PASTE_START as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                    {
                        terminal.flags |= TTY_BRACKETPASTE;
                    } else if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                        == KEYC_PASTE_END as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                    {
                        terminal.flags &= !TTY_BRACKETPASTE;
                    }
                }
                if key == KEYC_FOCUS_OUT as ::core::ffi::c_ulong as key_code {
                    terminal_client_owner.update_flags(0, CLIENT_FOCUSED as u64);
                    tty_keys_update_focus(terminal_client_owner);
                    events_fire_client(c"client-focus-out", terminal_client_owner.clone());
                } else if key == KEYC_FOCUS_IN as ::core::ffi::c_ulong as key_code {
                    terminal_client_owner.update_flags(CLIENT_FOCUSED as u64, 0);
                    events_fire_client(c"client-focus-in", terminal_client_owner.clone());
                    tty_keys_update_focus(terminal_client_owner);
                }
                if let Some(event) = event {
                    terminal_client_owner.handle_key(event);
                }
                terminal_client_owner
                    .borrow_terminal_mut()
                    .in_0
                    .as_deref_mut()
                    .expect("open TTY buffer")
                    .drain(size);
                return 1 as ::core::ffi::c_int;
            }
            _ => {
                log_debug(format_args!(
                    "{}: partial key {}",
                    log_cstr(
                        (terminal_client_owner.name())
                            .as_deref()
                            .unwrap_or(c"(null)")
                    ),
                    log_cstr_n(&bytes[..len as usize])
                ));
                let (timer_active, timer_expired) = {
                    let mut terminal = terminal_client_owner.borrow_terminal_mut();
                    (
                        terminal.flags & TTY_TIMER != 0,
                        terminal.flags & TTY_TIMER != 0 && terminal.key_timer.is_none(),
                    )
                };
                if timer_active {
                    if timer_expired {
                        expired = 1 as ::core::ffi::c_int;
                        current_block = 12077302897653652224;
                    } else {
                        return 0 as ::core::ffi::c_int;
                    }
                } else {
                    delay =
                        options_get_number(global_options(), c"escape-time") as ::core::ffi::c_int;
                    if delay == 0 as ::core::ffi::c_int {
                        delay = 1 as ::core::ffi::c_int;
                    }
                    let partial_paste_end = match match_bracketed_paste_boundary(bytes) {
                        BracketedPasteBoundaryMatch::Incomplete { boundary, consumed } => {
                            consumed != 0 && boundary != Some(BracketedPasteBoundary::Start)
                        }
                        BracketedPasteBoundaryMatch::Match { .. }
                        | BracketedPasteBoundaryMatch::NoMatch { .. } => false,
                    };
                    let terminal_flags = terminal_client_owner.borrow_terminal().flags;
                    if terminal_flags & TTY_BRACKETPASTE != 0 && partial_paste_end {
                        log_debug(format_args!(
                            "{}: increasing delay (partial paste end)",
                            log_cstr(
                                (terminal_client_owner.name())
                                    .as_deref()
                                    .unwrap_or(c"(null)")
                            )
                        ));
                        if delay < 500 as ::core::ffi::c_int {
                            delay = 500 as ::core::ffi::c_int;
                        }
                    }
                    if terminal_flags & (TTY_WAITFG | TTY_WAITBG) != 0
                        || terminal_flags & (TTY_OSC52QUERY | TTY_WINSIZEQUERY) != 0
                        || terminal_flags & TTY_ALL_REQUEST_FLAGS != TTY_ALL_REQUEST_FLAGS
                        || terminal_client_owner.has_input_requests()
                    {
                        log_debug(format_args!(
                            "{}: increasing delay (active query)",
                            log_cstr(
                                (terminal_client_owner.name())
                                    .as_deref()
                                    .unwrap_or(c"(null)")
                            )
                        ));
                        if delay < 500 as ::core::ffi::c_int {
                            delay = 500 as ::core::ffi::c_int;
                        }
                    }
                    let timeout = Duration::from_millis(delay as u64);
                    {
                        let mut terminal = terminal_client_owner.borrow_terminal_mut();
                        drop(terminal.key_timer.take());
                        terminal.key_timer = Some(
                            Timer::new(
                                timeout,
                                crate::src::tty::tty_client_timer_callback(
                                    terminal_client_owner,
                                    tty_keys_callback,
                                ),
                            )
                            .expect("arm timer"),
                        );
                        terminal.flags |= TTY_TIMER;
                    }
                    return 0 as ::core::ffi::c_int;
                }
            }
        }
    }
}
unsafe fn tty_keys_update_focus(client: &ClientRef) {
    let window = {
        let session = client.attached_session().upgrade().expect("live session");
        let link = session.current_winlink();
        let window = link.get_unchecked().window_handle().cloned();
        window
    };
    if let Some(window) = window {
        window.update_focus();
        drop(window);
    }
}

unsafe fn tty_keys_callback(owner: &ClientRef) {
    drop(owner.borrow_terminal_mut().key_timer.take());
    let timer_active = owner.borrow_terminal().flags & TTY_TIMER != 0;
    if timer_active {
        while tty_keys_next(owner) != 0 {}
    }
}

unsafe fn tty_keys_extended_key(
    tty: &tty,
    diagnostic_name: Option<&CStr>,
    bytes: &[u8],
    mut size: *mut size_t,
    mut key: *mut key_code,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut end: size_t = 0;
    let mut number: u_int = 0;
    let mut modifiers: u_int = 0;
    let mut tmp: [::core::ffi::c_char; 64] = [0; 64];
    let mut bspace: cc_t = 0;
    let mut nkey: key_code = 0;
    let mut onlykey: key_code = 0;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut uc: utf8_char = 0;
    *size = 0 as size_t;
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    end = 2 as size_t;
    while end < len && end != ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as usize {
        if *buf.add(end) as ::core::ffi::c_int == '~' as i32 {
            break;
        }
        if ::libc::isdigit(*buf.add(end) as u_char as ::core::ffi::c_int) == 0
            && *buf.add(end) as ::core::ffi::c_int != ';' as i32
        {
            break;
        }
        end = end.wrapping_add(1);
    }
    if end == len {
        return 1 as ::core::ffi::c_int;
    }
    if end == ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as usize
        || *buf.add(end) as ::core::ffi::c_int != '~' as i32
            && *buf.add(end) as ::core::ffi::c_int != 'u' as i32
    {
        return -(1 as ::core::ffi::c_int);
    }
    for (dst, &src) in tmp.iter_mut().zip(&bytes[2..end]) {
        *dst = src as ::core::ffi::c_char;
    }
    tmp[end.wrapping_sub(2 as size_t) as usize] = '\0' as i32 as ::core::ffi::c_char;
    if *buf.add(end) as ::core::ffi::c_int == '~' as i32 {
        if sscanf(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"27;%u;%u".as_ptr(),
            &raw mut modifiers,
            &raw mut number,
        ) != 2 as ::core::ffi::c_int
        {
            return -(1 as ::core::ffi::c_int);
        }
    } else if sscanf(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"%u;%u".as_ptr(),
        &raw mut number,
        &raw mut modifiers,
    ) != 2 as ::core::ffi::c_int
    {
        return -(1 as ::core::ffi::c_int);
    }
    *size = end.wrapping_add(1 as size_t);
    bspace = tty.tio.0.c_cc[VERASE];
    if bspace != _POSIX_VDISABLE && number == bspace as u_int {
        nkey = KEYC_BSPACE as ::core::ffi::c_ulong as key_code;
    } else {
        nkey = number as key_code;
    }
    if nkey != KEYC_BSPACE as ::core::ffi::c_ulong as key_code
        && nkey & !(0x7f as ::core::ffi::c_int) as key_code != 0
    {
        if utf8_fromwc(nkey as wchar_t, &mut ud) as ::core::ffi::c_uint
            == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
            && utf8_from_data(&ud, &mut uc) as ::core::ffi::c_uint
                == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            nkey = uc as key_code;
        } else {
            return -(1 as ::core::ffi::c_int);
        }
    }
    if modifiers > 0 as u_int {
        modifiers = modifiers.wrapping_sub(1);
        if modifiers & 1 as u_int != 0 {
            nkey |= KEYC_SHIFT;
        }
        if modifiers & 2 as u_int != 0 {
            nkey |= KEYC_META | KEYC_IMPLIED_META;
        }
        if modifiers & 4 as u_int != 0 {
            nkey |= KEYC_CTRL;
        }
        if modifiers & 8 as u_int != 0 {
            nkey |= KEYC_META | KEYC_IMPLIED_META;
        }
    }
    if nkey as ::core::ffi::c_ulonglong & KEYC_MASK_KEY == '\t' as i32 as ::core::ffi::c_ulonglong
        && nkey as ::core::ffi::c_ulonglong & KEYC_SHIFT != 0
    {
        nkey = (KEYC_BTAB as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
            | nkey as ::core::ffi::c_ulonglong & !KEYC_MASK_KEY & !KEYC_SHIFT)
            as key_code;
    }
    onlykey = (nkey as ::core::ffi::c_ulonglong & KEYC_MASK_KEY) as key_code;
    if (onlykey > 0x20 as key_code && onlykey < 0x7f as key_code
        || nkey as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            == (KEYC_TYPE_UNICODE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && nkey as ::core::ffi::c_ulonglong & KEYC_MASK_KEY > 0x7f as ::core::ffi::c_ulonglong)
        && nkey as ::core::ffi::c_ulonglong & KEYC_MASK_MODIFIERS == KEYC_SHIFT
    {
        nkey &= !KEYC_SHIFT;
    }
    if log_get_level() != 0 as ::core::ffi::c_int {
        let key_string = key_string_format(nkey, true);
        log_debug(format_args!(
            "{}: extended key {} is {:x} ({})",
            log_cstr(diagnostic_name.unwrap_or(c"(null)")),
            log_cstr_n(&bytes[..*size]),
            (nkey) as u64,
            crate::src::log::log_bytes(key_string.as_bytes())
        ));
    }
    *key = nkey;
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_mouse(
    tty: &mut tty,
    diagnostic_name: Option<&CStr>,
    bytes: &[u8],
    mut size: *mut size_t,
    mut m: *mut mouse_event,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut i: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut b: u_int = 0;
    let mut sgr_b: u_int = 0;
    let mut sgr_type: u_char = 0;
    let mut ch: u_char = 0;
    *size = 0 as size_t;
    sgr_b = 0 as u_int;
    b = sgr_b;
    y = b;
    x = y;
    sgr_type = ' ' as i32 as u_char;
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == 'M' as i32 {
        *size = 3 as size_t;
        i = 0 as u_int;
        while i < 3 as u_int {
            if len <= *size {
                return 1 as ::core::ffi::c_int;
            }
            let fresh0 = *size;
            *size = (*size).wrapping_add(1);
            ch = *buf.add(fresh0) as u_char;
            if i == 0 as u_int {
                b = ch as u_int;
            } else if i == 1 as u_int {
                x = ch as u_int;
            } else {
                y = ch as u_int;
            }
            i = i.wrapping_add(1);
        }
        log_debug(format_args!(
            "{}: mouse input: {}",
            log_cstr(diagnostic_name.unwrap_or(c"(null)")),
            log_cstr_n(&bytes[..*size])
        ));
        if b < MOUSE_PARAM_BTN_OFF as u_int
            || x < MOUSE_PARAM_POS_OFF as u_int
            || y < MOUSE_PARAM_POS_OFF as u_int
        {
            return -(2 as ::core::ffi::c_int);
        }
        b = b.wrapping_sub(MOUSE_PARAM_BTN_OFF as u_int);
        x = x.wrapping_sub(MOUSE_PARAM_POS_OFF as u_int);
        y = y.wrapping_sub(MOUSE_PARAM_POS_OFF as u_int);
    } else if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '<' as i32 {
        *size = 3 as size_t;
        loop {
            if len <= *size {
                return 1 as ::core::ffi::c_int;
            }
            let fresh1 = *size;
            *size = (*size).wrapping_add(1);
            ch = *buf.add(fresh1) as u_char;
            if ch as ::core::ffi::c_int == ';' as i32 {
                break;
            }
            if (ch as ::core::ffi::c_int) < '0' as i32 || ch as ::core::ffi::c_int > '9' as i32 {
                return -(1 as ::core::ffi::c_int);
            }
            sgr_b = (10 as u_int)
                .wrapping_mul(sgr_b)
                .wrapping_add((ch as ::core::ffi::c_int - '0' as i32) as u_int);
        }
        loop {
            if len <= *size {
                return 1 as ::core::ffi::c_int;
            }
            let fresh2 = *size;
            *size = (*size).wrapping_add(1);
            ch = *buf.add(fresh2) as u_char;
            if ch as ::core::ffi::c_int == ';' as i32 {
                break;
            }
            if (ch as ::core::ffi::c_int) < '0' as i32 || ch as ::core::ffi::c_int > '9' as i32 {
                return -(1 as ::core::ffi::c_int);
            }
            x = (10 as u_int)
                .wrapping_mul(x)
                .wrapping_add((ch as ::core::ffi::c_int - '0' as i32) as u_int);
        }
        loop {
            if len <= *size {
                return 1 as ::core::ffi::c_int;
            }
            let fresh3 = *size;
            *size = (*size).wrapping_add(1);
            ch = *buf.add(fresh3) as u_char;
            if ch as ::core::ffi::c_int == 'M' as i32 || ch as ::core::ffi::c_int == 'm' as i32 {
                break;
            }
            if (ch as ::core::ffi::c_int) < '0' as i32 || ch as ::core::ffi::c_int > '9' as i32 {
                return -(1 as ::core::ffi::c_int);
            }
            y = (10 as u_int)
                .wrapping_mul(y)
                .wrapping_add((ch as ::core::ffi::c_int - '0' as i32) as u_int);
        }
        log_debug(format_args!(
            "{}: mouse input (SGR): {}",
            log_cstr(diagnostic_name.unwrap_or(c"(null)")),
            log_cstr_n(&bytes[..*size])
        ));
        if x < 1 as u_int || y < 1 as u_int {
            return -(2 as ::core::ffi::c_int);
        }
        x = x.wrapping_sub(1);
        y = y.wrapping_sub(1);
        b = sgr_b;
        sgr_type = ch;
        if sgr_type as ::core::ffi::c_int == 'm' as i32 {
            b = 3 as u_int;
        }
        if sgr_type as ::core::ffi::c_int == 'm' as i32
            && (sgr_b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_UP as u_int
                || sgr_b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_DOWN as u_int)
        {
            return -(2 as ::core::ffi::c_int);
        }
    } else {
        return -(1 as ::core::ffi::c_int);
    }
    (*m).lx = tty.mouse_last_x;
    (*m).x = x;
    (*m).ly = tty.mouse_last_y;
    (*m).y = y;
    (*m).lb = tty.mouse_last_b;
    (*m).b = b;
    (*m).sgr_type = sgr_type as u_int;
    (*m).sgr_b = sgr_b;
    tty.mouse_last_x = x;
    tty.mouse_last_y = y;
    tty.mouse_last_b = b;
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_clipboard(
    bytes: &[u8],
    mut size: *mut size_t,
    reply: &mut Option<input_request_clipboard_data>,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut end: size_t = 0;
    let mut terminator: size_t = 0 as size_t;
    let mut needed: size_t = 0;
    let mut clip: ::core::ffi::c_char = 0 as ::core::ffi::c_char;
    let mut outlen: ::core::ffi::c_int = 0;
    *size = 0 as size_t;
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ']' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '5' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(3 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '2' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 4 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(4 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ';' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 5 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    end = 5 as size_t;
    while end < len {
        if *buf.add(end) as ::core::ffi::c_int == '\u{7}' as i32 {
            terminator = 1 as size_t;
            break;
        } else if end > 5 as size_t
            && *buf.add(end.wrapping_sub(1 as size_t)) as ::core::ffi::c_int == '\u{1b}' as i32
            && *buf.add(end) as ::core::ffi::c_int == '\\' as i32
        {
            terminator = 2 as size_t;
            break;
        } else {
            end = end.wrapping_add(1);
        }
    }
    if end == len {
        return 1 as ::core::ffi::c_int;
    }
    *size = end.wrapping_add(1 as size_t);
    buf = buf.offset(5 as ::core::ffi::c_int as isize);
    end = end.wrapping_sub(5 as size_t);
    end = end.wrapping_sub(terminator.wrapping_sub(1 as size_t));
    if end >= 2 as size_t
        && *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ';' as i32
        && *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == ';' as i32
    {
        clip = *buf.offset(0 as ::core::ffi::c_int as isize);
    }
    while end != 0 as size_t && *buf as ::core::ffi::c_int != ';' as i32 {
        buf = buf.offset(1);
        end = end.wrapping_sub(1);
    }
    if end == 0 as size_t || end == 1 as size_t {
        return 0 as ::core::ffi::c_int;
    }
    buf = buf.offset(1);
    end = end.wrapping_sub(1);
    let mut copy = ::core::slice::from_raw_parts(buf.cast::<u8>(), end).to_vec();
    copy.push(0);
    needed = end
        .wrapping_add(3 as size_t)
        .wrapping_div(4 as size_t)
        .wrapping_mul(3 as size_t);
    if needed == 0 as size_t {
        return 0 as ::core::ffi::c_int;
    }
    let mut out = Vec::<u8>::with_capacity(needed);
    outlen = b64_pton(
        copy.as_ptr().cast::<::core::ffi::c_char>(),
        out.as_mut_ptr(),
        needed,
    );
    if outlen == -(1 as ::core::ffi::c_int) {
        return 0 as ::core::ffi::c_int;
    }
    out.set_len(outlen as usize);
    drop(copy);
    log_debug(format_args!(
        "{}: {}",
        "tty_keys_clipboard",
        log_cstr_n(&out[..outlen as usize])
    ));
    *reply = Some(input_request_clipboard_data { data: out, clip });
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_device_attributes(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    mut size: *mut size_t,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut i: u_int = 0;
    let mut n: u_int = 0 as u_int;
    let mut tmp: [::core::ffi::c_char; 128] = [0; 128];
    let mut endptr: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut p: [::core::ffi::c_char; 32] = [
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let mut cp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut next: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    *size = 0 as size_t;
    if terminal_client_owner.borrow_terminal().flags & TTY_HAVEDA != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '?' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize) < ::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize {
        if (3 as u_int).wrapping_add(i) as size_t == len {
            return 1 as ::core::ffi::c_int;
        }
        if *buf.offset((3 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int >= 'a' as i32
            && *buf.offset((3 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
                <= 'z' as i32
        {
            break;
        }
        tmp[i as usize] = *buf.offset((3 as u_int).wrapping_add(i) as isize);
        i = i.wrapping_add(1);
    }
    if i as usize == ::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize {
        return -(1 as ::core::ffi::c_int);
    }
    if *buf.offset((3 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int != 'c' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    tmp[i as usize] = '\0' as i32 as ::core::ffi::c_char;
    *size = (4 as u_int).wrapping_add(i) as size_t;
    cp = &raw mut tmp as *mut ::core::ffi::c_char;
    loop {
        next = strsep(&raw mut cp, c";".as_ptr());
        if next.is_null() {
            break;
        }
        p[n as usize] =
            strtoul(next, &raw mut endptr, 10 as ::core::ffi::c_int) as ::core::ffi::c_char;
        if *endptr as ::core::ffi::c_int != '\0' as i32 {
            p[n as usize] = 0 as ::core::ffi::c_char;
        }
        n = n.wrapping_add(1);
        if n as usize
            == (::core::mem::size_of::<[::core::ffi::c_char; 32]>() as usize)
                .wrapping_div(::core::mem::size_of::<::core::ffi::c_char>() as usize)
        {
            break;
        }
    }
    if let 61..=65 = p[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int {
        i = 1 as u_int;
        while i < n {
            log_debug(format_args!(
                "{}: DA feature: {}",
                log_cstr(
                    (terminal_client_owner.name())
                        .as_deref()
                        .unwrap_or(c"(null)")
                ),
                p[i as usize] as ::core::ffi::c_int
            ));
            if p[i as usize] as ::core::ffi::c_int == 4 as ::core::ffi::c_int {
                tty_parse_client_features(terminal_client_owner, c"sixel", c",");
            }
            if p[i as usize] as ::core::ffi::c_int == 21 as ::core::ffi::c_int {
                tty_parse_client_features(terminal_client_owner, c"margins", c",");
            }
            if p[i as usize] as ::core::ffi::c_int == 28 as ::core::ffi::c_int {
                tty_parse_client_features(terminal_client_owner, c"rectfill", c",");
            }
            if p[i as usize] as ::core::ffi::c_int == 52 as ::core::ffi::c_int {
                tty_parse_client_features(terminal_client_owner, c"clipboard", c",");
            }
            i = i.wrapping_add(1);
        }
    }
    log_debug(format_args!(
        "{}: received primary DA {}",
        log_cstr(
            (terminal_client_owner.name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        log_cstr_n(&bytes[..*size])
    ));
    tty_update_features(terminal_client_owner);
    terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVEDA;
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_sync(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    mut size: *mut size_t,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let prefix = b"\x1B[?2026;\0";
    let mut i: size_t = 0;
    let mut status: ::core::ffi::c_int = 0;
    *size = 0 as size_t;
    if terminal_client_owner.borrow_terminal().flags & TTY_HAVESYNC != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    i = 0 as size_t;
    while i < (::core::mem::size_of::<[::core::ffi::c_char; 9]>() as usize).wrapping_sub(1_usize) {
        if i == len {
            return 1 as ::core::ffi::c_int;
        }
        if *buf.add(i) as ::core::ffi::c_int != prefix[i as usize] as ::core::ffi::c_int {
            return -(1 as ::core::ffi::c_int);
        }
        i = i.wrapping_add(1);
    }
    if i == len {
        return 1 as ::core::ffi::c_int;
    }
    if (*buf.add(i) as ::core::ffi::c_int) < '0' as i32
        || *buf.add(i) as ::core::ffi::c_int > '4' as i32
    {
        return -(1 as ::core::ffi::c_int);
    }
    let fresh4 = i;
    i = i.wrapping_add(1);
    status = *buf.add(fresh4) as ::core::ffi::c_int - '0' as i32;
    if i == len {
        return 1 as ::core::ffi::c_int;
    }
    let fresh5 = i;
    i = i.wrapping_add(1);
    if *buf.add(fresh5) as ::core::ffi::c_int != '$' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if i == len {
        return 1 as ::core::ffi::c_int;
    }
    let fresh6 = i;
    i = i.wrapping_add(1);
    if *buf.add(fresh6) as ::core::ffi::c_int != 'y' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    *size = i;
    if status == 1 as ::core::ffi::c_int
        || status == 2 as ::core::ffi::c_int
        || status == 3 as ::core::ffi::c_int
    {
        tty_parse_client_features(terminal_client_owner, c"sync", c",");
        tty_update_features(terminal_client_owner);
    }
    log_debug(format_args!(
        "{}: received DECRPM {}",
        log_cstr(
            (terminal_client_owner.name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        log_cstr_n(&bytes[..*size])
    ));
    terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVESYNC;
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_device_attributes2(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    mut size: *mut size_t,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut i: u_int = 0;
    let mut n: u_int = 0 as u_int;
    let mut tmp: [::core::ffi::c_char; 128] = [0; 128];
    let mut endptr: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut p: [::core::ffi::c_char; 32] = [
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let mut cp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut next: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    *size = 0 as size_t;
    if terminal_client_owner.borrow_terminal().flags & TTY_HAVEDA2 != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '>' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize) < ::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize {
        if (3 as u_int).wrapping_add(i) as size_t == len {
            return 1 as ::core::ffi::c_int;
        }
        if *buf.offset((3 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int >= 'a' as i32
            && *buf.offset((3 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
                <= 'z' as i32
        {
            break;
        }
        tmp[i as usize] = *buf.offset((3 as u_int).wrapping_add(i) as isize);
        i = i.wrapping_add(1);
    }
    if i as usize == ::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize {
        return -(1 as ::core::ffi::c_int);
    }
    if *buf.offset((3 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int != 'c' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    tmp[i as usize] = '\0' as i32 as ::core::ffi::c_char;
    *size = (4 as u_int).wrapping_add(i) as size_t;
    cp = &raw mut tmp as *mut ::core::ffi::c_char;
    loop {
        next = strsep(&raw mut cp, c";".as_ptr());
        if next.is_null() {
            break;
        }
        p[n as usize] =
            strtoul(next, &raw mut endptr, 10 as ::core::ffi::c_int) as ::core::ffi::c_char;
        if *endptr as ::core::ffi::c_int != '\0' as i32 {
            p[n as usize] = 0 as ::core::ffi::c_char;
        }
        n = n.wrapping_add(1);
        if n as usize
            == (::core::mem::size_of::<[::core::ffi::c_char; 32]>() as usize)
                .wrapping_div(::core::mem::size_of::<::core::ffi::c_char>() as usize)
        {
            break;
        }
    }
    match p[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int {
        77 => {
            tty_default_features(terminal_client_owner, c"mintty");
        }
        84 => {
            tty_default_features(terminal_client_owner, c"tmux");
        }
        85 => {
            tty_default_features(terminal_client_owner, c"rxvt-unicode");
        }
        _ => {}
    }
    log_debug(format_args!(
        "{}: received secondary DA {}",
        log_cstr(
            (terminal_client_owner.name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        log_cstr_n(&bytes[..*size])
    ));
    tty_update_features(terminal_client_owner);
    terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVEDA2;
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_extended_device_attributes(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    mut size: *mut size_t,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut i: u_int = 0;
    let mut tmp: [::core::ffi::c_char; 128] = [0; 128];
    *size = 0 as size_t;
    if terminal_client_owner.borrow_terminal().flags & TTY_HAVEXDA != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != 'P' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '>' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(3 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '|' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 4 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize).wrapping_sub(1_usize)
    {
        if (4 as u_int).wrapping_add(i) as size_t == len {
            return 1 as ::core::ffi::c_int;
        }
        if *buf.offset((4 as u_int).wrapping_add(i).wrapping_sub(1 as u_int) as isize)
            as ::core::ffi::c_int
            == '\u{1b}' as i32
            && *buf.offset((4 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
                == '\\' as i32
        {
            break;
        }
        tmp[i as usize] = *buf.offset((4 as u_int).wrapping_add(i) as isize);
        i = i.wrapping_add(1);
    }
    if i as usize
        == (::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize).wrapping_sub(1_usize)
    {
        return -(1 as ::core::ffi::c_int);
    }
    *size = (5 as u_int).wrapping_add(i) as size_t;
    if i == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    tmp[i.wrapping_sub(1 as u_int) as usize] = '\0' as i32 as ::core::ffi::c_char;
    if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"iTerm2 ".as_ptr(),
        7 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"iTerm2");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"tmux ".as_ptr(),
        5 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"tmux");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"XTerm(".as_ptr(),
        6 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"XTerm");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"mintty ".as_ptr(),
        7 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"mintty");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"foot(".as_ptr(),
        5 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"foot");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"WezTerm ".as_ptr(),
        7 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"WezTerm");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"ghostty ".as_ptr(),
        8 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"ghostty");
    } else if strncmp(
        &raw mut tmp as *mut ::core::ffi::c_char,
        c"Rio ".as_ptr(),
        4 as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        tty_default_features(terminal_client_owner, c"Rio");
    }
    log_debug(format_args!(
        "{}: received extended DA {}",
        log_cstr(
            (terminal_client_owner.name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        log_cstr_n(&bytes[..*size])
    ));
    terminal_client_owner.record_terminal_type(CStr::from_ptr(tmp.as_ptr()));
    tty_update_features(terminal_client_owner);
    terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVEXDA;
    0 as ::core::ffi::c_int
}
pub unsafe fn tty_keys_colours(
    flags: &mut i32,
    diagnostic_name: Option<&std::ffi::CStr>,
    bytes: &[u8],
    mut size: *mut size_t,
    fg: &mut i32,
    bg: &mut i32,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut i: u_int = 0;
    let mut tmp: [::core::ffi::c_char; 128] = [0; 128];
    let mut n: ::core::ffi::c_int = 0;
    *size = 0 as size_t;
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ']' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '1' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(3 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '0' as i32
        && *buf.offset(3 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '1' as i32
    {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 4 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(4 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ';' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 5 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize).wrapping_sub(1_usize)
    {
        if (5 as u_int).wrapping_add(i) as size_t == len {
            return 1 as ::core::ffi::c_int;
        }
        if *buf.offset((5 as u_int).wrapping_add(i).wrapping_sub(1 as u_int) as isize)
            as ::core::ffi::c_int
            == '\u{1b}' as i32
            && *buf.offset((5 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
                == '\\' as i32
        {
            break;
        }
        if *buf.offset((5 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
            == '\u{7}' as i32
        {
            break;
        }
        tmp[i as usize] = *buf.offset((5 as u_int).wrapping_add(i) as isize);
        i = i.wrapping_add(1);
    }
    if i as usize
        == (::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize).wrapping_sub(1_usize)
    {
        return -(1 as ::core::ffi::c_int);
    }
    *size = (6 as u_int).wrapping_add(i) as size_t;
    if i == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    if tmp[i.wrapping_sub(1 as u_int) as usize] as ::core::ffi::c_int == '\u{1b}' as i32 {
        tmp[i.wrapping_sub(1 as u_int) as usize] = '\0' as i32 as ::core::ffi::c_char;
    } else {
        tmp[i as usize] = '\0' as i32 as ::core::ffi::c_char;
    }
    n = colour_parse_x11_logged(std::ffi::CStr::from_ptr(
        &raw mut tmp as *mut ::core::ffi::c_char,
    ))
    .unwrap_or(-1);
    if n != -(1 as ::core::ffi::c_int)
        && *buf.offset(3 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '0' as i32
    {
        if let Some(name) = diagnostic_name {
            log_debug(format_args!(
                "{} fg is {}",
                log_cstr(name),
                crate::src::log::log_bytes(colour_format(n).as_bytes())
            ));
        } else {
            log_debug(format_args!(
                "fg is {}",
                crate::src::log::log_bytes(colour_format(n).as_bytes())
            ));
        }
        *fg = n;
        *flags &= !TTY_WAITFG;
    } else if n != -(1 as ::core::ffi::c_int) {
        if let Some(name) = diagnostic_name {
            log_debug(format_args!(
                "{} bg is {}",
                log_cstr(name),
                crate::src::log::log_bytes(colour_format(n).as_bytes())
            ));
        } else {
            log_debug(format_args!(
                "bg is {}",
                crate::src::log::log_bytes(colour_format(n).as_bytes())
            ));
        }
        *bg = n;
        *flags &= !TTY_WAITBG;
    }
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_palette(
    bytes: &[u8],
    mut size: *mut size_t,
    reply: &mut Option<input_request_palette_data>,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
    let mut buf: *const ::core::ffi::c_char = bytes.as_ptr().cast();
    let mut i: u_int = 0;
    let mut tmp: [::core::ffi::c_char; 128] = [0; 128];
    let mut endptr: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut idx: ::core::ffi::c_int = 0;
    let mut pd: input_request_palette_data = input_request_palette_data { idx: 0, c: 0 };
    *size = 0 as size_t;
    if *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ']' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '4' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if *buf.offset(3 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ';' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 4 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize).wrapping_sub(1_usize)
    {
        if (4 as u_int).wrapping_add(i) as size_t == len {
            return 1 as ::core::ffi::c_int;
        }
        if *buf.offset((4 as u_int).wrapping_add(i).wrapping_sub(1 as u_int) as isize)
            as ::core::ffi::c_int
            == '\u{1b}' as i32
            && *buf.offset((4 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
                == '\\' as i32
        {
            break;
        }
        if *buf.offset((4 as u_int).wrapping_add(i) as isize) as ::core::ffi::c_int
            == '\u{7}' as i32
        {
            break;
        }
        tmp[i as usize] = *buf.offset((4 as u_int).wrapping_add(i) as isize);
        i = i.wrapping_add(1);
    }
    if i as usize
        == (::core::mem::size_of::<[::core::ffi::c_char; 128]>() as usize).wrapping_sub(1_usize)
    {
        return -(1 as ::core::ffi::c_int);
    }
    *size = (5 as u_int).wrapping_add(i) as size_t;
    if i == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    if tmp[i.wrapping_sub(1 as u_int) as usize] as ::core::ffi::c_int == '\u{1b}' as i32 {
        tmp[i.wrapping_sub(1 as u_int) as usize] = '\0' as i32 as ::core::ffi::c_char;
    } else {
        tmp[i as usize] = '\0' as i32 as ::core::ffi::c_char;
    }
    idx = strtol(
        &raw mut tmp as *mut ::core::ffi::c_char,
        &raw mut endptr,
        10 as ::core::ffi::c_int,
    ) as ::core::ffi::c_int;
    if *endptr as ::core::ffi::c_int != ';' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if idx < 0 as ::core::ffi::c_int || idx > 255 as ::core::ffi::c_int {
        return -(1 as ::core::ffi::c_int);
    }
    pd.c = colour_parse_x11_logged(std::ffi::CStr::from_ptr(
        endptr.offset(1 as ::core::ffi::c_int as isize),
    ))
    .unwrap_or(-1);
    if pd.c == -(1 as ::core::ffi::c_int) {
        return 0 as ::core::ffi::c_int;
    }
    pd.idx = idx;
    *reply = Some(pd);
    0 as ::core::ffi::c_int
}

#[cfg(test)]
mod key_tree_tests {
    use super::*;

    fn lookup(tree: &Option<Box<tty_key>>, bytes: &[u8]) -> (Option<key_code>, size_t) {
        let mut size = 0;
        let key = tty_keys_find1(tree.as_deref(), bytes, &mut size).map(|node| node.key);
        (key, size)
    }

    #[test]
    fn branches_and_partial_sequences_preserve_matching() {
        let mut tree = None;
        for (bytes, key) in [
            (b"mb".as_slice(), 1),
            (b"a", 2),
            (b"z", 3),
            (b"ma", 4),
            (b"mc", 5),
            (b"\xff", 6),
        ] {
            tty_keys_add1(&mut tree, bytes, key);
        }
        for (bytes, expected) in [
            (b"mb".as_slice(), 1),
            (b"a", 2),
            (b"z", 3),
            (b"ma", 4),
            (b"mc", 5),
            (b"\xff", 6),
        ] {
            assert_eq!(lookup(&tree, bytes), (Some(expected), bytes.len()));
        }
        assert_eq!(lookup(&tree, b"m"), (Some(KEYC_UNKNOWN as key_code), 1));
        assert_eq!(lookup(&tree, b"md"), (None, 1));
        assert_eq!(lookup(&tree, b"mbextra"), (Some(1), 2));
        assert_eq!(lookup(&tree, b""), (None, 0));
        assert_eq!(lookup(&tree, b"q"), (None, 0));
    }

    #[test]
    fn replacing_keys_preserves_existing_prefix_behavior() {
        let mut terminal = tty::default();
        unsafe {
            tty_keys_add(&mut terminal, c"ab", 1);
            tty_keys_add(&mut terminal, c"ab", 2);
            assert_eq!(lookup(&terminal.key_tree, b"ab"), (Some(2), 2));
            tty_keys_add(&mut terminal, c"a", 3);
            assert_eq!(lookup(&terminal.key_tree, b"a"), (Some(3), 1));
            assert_eq!(lookup(&terminal.key_tree, b"ab"), (Some(2), 2));
            tty_keys_add(&mut terminal, c"abc", 4);
            assert_eq!(lookup(&terminal.key_tree, b"abc"), (Some(4), 2));
        }
    }

    #[test]
    fn ambiguous_key_waits_until_timeout_or_completion() {
        let mut terminal = tty::default();
        tty_keys_add1(&mut terminal.key_tree, b"ab", 1);
        tty_keys_add1(&mut terminal.key_tree, b"a", 2);
        unsafe {
            let mut key = 0;
            let mut size = 0;
            assert_eq!(
                tty_keys_next1(
                    &terminal,
                    Some(c"key-tree-test"),
                    b"a",
                    &mut key,
                    &mut size,
                    0
                ),
                1
            );
            assert_eq!(size, 1);
            assert_eq!(
                tty_keys_next1(
                    &terminal,
                    Some(c"key-tree-test"),
                    b"a",
                    &mut key,
                    &mut size,
                    1
                ),
                0
            );
            assert_eq!((key, size), (2, 1));
            assert_eq!(
                tty_keys_next1(
                    &terminal,
                    Some(c"key-tree-test"),
                    b"ab",
                    &mut key,
                    &mut size,
                    0
                ),
                0
            );
            assert_eq!((key, size), (1, 2));
        }
    }

    #[test]
    fn clearing_is_repeatable_and_allows_repopulation() {
        let mut terminal = tty::default();
        unsafe {
            tty_keys_free(&mut terminal);
            tty_keys_add(&mut terminal, c"old", 1);
            tty_keys_free(&mut terminal);
            assert!(terminal.key_tree.is_none());
            tty_keys_free(&mut terminal);
            tty_keys_add(&mut terminal, c"new", 2);
            assert_eq!(lookup(&terminal.key_tree, b"new"), (Some(2), 3));
            assert_eq!(lookup(&terminal.key_tree, b"old"), (None, 0));
        }
    }
}
