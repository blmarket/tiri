use crate::bracketed_paste::{
    match_bracketed_paste_boundary, BracketedPasteBoundary, BracketedPasteBoundaryMatch,
};
use crate::events::events_fire_client;
use crate::input::{input_request_reply, InputRequestReply};
use crate::key_string::key_string_format;
use crate::log::{log_bytes, log_cstr, log_cstr_n, log_debug, log_get_level, log_hex};
use crate::options::options_get_number;
use crate::options::OptionsScope;
use crate::paste::paste_add_owned;
use crate::reactor::Timer;
use crate::server_client::Client as _;
use crate::window::Window as _;
use crate::winlink::winlink_window;

use super::control::tty_update_features;
use super::model::*;
use super::output::tty_invalidate;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_FOCUSED;
use crate::shared::control_character::{C0_CR, C0_ESC, C0_HT, C0_NUL};
use crate::shared::grid::*;
use crate::shared::input::{input_request_clipboard_data, input_request_palette_data};
use crate::shared::key::key_event;
use crate::shared::key::*;
use crate::shared::mouse::{
    mouse_event, MOUSE_MASK_BUTTONS, MOUSE_PARAM_BTN_OFF, MOUSE_PARAM_POS_OFF, MOUSE_WHEEL_DOWN,
    MOUSE_WHEEL_UP,
};
use crate::shared::posix_terminal::VERASE;
use crate::shared::tty::*;
use crate::shared::utf8::wchar_t;
use crate::shared::utf8::*;
use crate::style::colour::{colour_format, colour_parse_x11_logged};
use crate::text::utf8::{utf8_append, utf8_from_data, utf8_fromwc, utf8_open};
use crate::tty::features::{tty_default_features, tty_parse_client_features};
use crate::tty::term::tty_term_owner_ptr;
use crate::tty::term::tty_term_string;

use std::ffi::{CStr, CString};
use std::time::Duration;

#[derive(Copy, Clone)]
#[repr(C)]
pub(super) struct tty_default_key_code {
    pub code: tty_code_code,
    pub key: key_code,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub(super) struct tty_default_key_raw {
    pub string: &'static ::std::ffi::CStr,
    pub key: key_code,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub(super) struct tty_default_key_xterm {
    pub template: &'static ::std::ffi::CStr,
    pub key: key_code,
}
pub use ::libc::_POSIX_VDISABLE;

static tty_default_raw_keys: [tty_default_key_raw; 102] = [
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
static tty_default_xterm_keys: [tty_default_key_xterm; 30] = [
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
static tty_default_xterm_modifiers: [key_code; 10] = [
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
static tty_default_code_keys: [tty_default_key_code; 136] = [
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
fn tty_keys_add(tty: &mut tty, bytes: &[u8], key: key_code) {
    let key_string = key_string_format(key, true);
    let mut size = 0;
    if let Some(tk) = tty_keys_find_mut(tty.key_tree.as_deref_mut(), bytes, &mut size) {
        log_debug(format_args!(
            "replacing key {}: 0x{:x} ({})",
            log_bytes(bytes),
            { key },
            crate::log::log_bytes(key_string.as_bytes())
        ));
        tk.key = key;
    } else {
        log_debug(format_args!(
            "new key {}: 0x{:x} ({})",
            log_bytes(bytes),
            { key },
            crate::log::log_bytes(key_string.as_bytes())
        ));
        tty_keys_add1(&mut tty.key_tree, bytes, key);
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

pub fn tty_keys_build(tty: &mut tty) {
    let mut i: u_int = 0;
    let mut j: u_int = 0;
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
            let mut copy = tdkx.template.to_bytes().to_vec();
            let modifier = copy
                .iter_mut()
                .find(|byte| **byte == b'_')
                .expect("xterm key templates have a modifier placeholder");
            *modifier = (b'0' as u_int).wrapping_add(j) as u8;
            key = tdkx.key | tty_default_xterm_modifiers[j as usize];
            tty_keys_add(tty, &copy, key);
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
        if !tdkr.string.is_empty() {
            tty_keys_add(tty, tdkr.string.to_bytes(), tdkr.key);
        }
        i = i.wrapping_add(1);
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[tty_default_key_code; 136]>() as usize)
            .wrapping_div(::core::mem::size_of::<tty_default_key_code>() as usize)
    {
        let tdkc = &tty_default_code_keys[i as usize];
        let s = tty_term_string(
            tty_term_owner_ptr(&tty.term).expect("client has a terminal"),
            tdkc.code,
        )
        .to_owned();
        if !s.is_empty() {
            tty_keys_add(tty, s.to_bytes(), tdkc.key);
        }
        i = i.wrapping_add(1);
    }
    let keys = OptionsScope::GlobalServer
        .with_entry(c"user-keys", |entry| {
            (0..=KEYC_NUSER as u_int)
                .filter_map(|index| {
                    let key = std::ffi::CString::new(index.to_string()).unwrap();
                    entry
                        .value
                        .array()
                        .and_then(|array| array.get(&key))
                        .and_then(|value| value.string_ptr())
                        .map(|value| (index, value.to_owned()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (index, value) in keys {
        tty_keys_add(
            tty,
            value.to_bytes(),
            (KEYC_USER as ::core::ffi::c_ulong).wrapping_add(index as ::core::ffi::c_ulong)
                as key_code,
        );
    }
}
pub(super) fn tty_keys_free(tty: &mut tty) {
    tty.key_tree = None;
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

/// `bytes` is never empty: `tty_keys_next` returns before parsing an empty
/// buffer and retries after an Escape only when more bytes follow it.
fn tty_keys_next1(
    tty: &tty,
    diagnostic_name: Option<&CStr>,
    bytes: &[u8],
    key: &mut key_code,
    size: &mut size_t,
    expired: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    let mut uc: utf8_char = 0;
    log_debug(format_args!(
        "{}: next key is {} ({}) (expired={})",
        log_cstr(diagnostic_name.unwrap_or(c"(null)")),
        { len },
        log_cstr_n(bytes),
        { expired }
    ));
    *size = 0;
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
    more = utf8_open(&mut ud, bytes[0]);
    if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint {
        *size = ud.size as size_t;
        if len < ud.size as size_t {
            if expired == 0 {
                return 1 as ::core::ffi::c_int;
            }
            return -(1 as ::core::ffi::c_int);
        }
        for &byte in &bytes[1..ud.size as usize] {
            more = utf8_append(&mut ud, byte);
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
/// Window size and extended key parameters end before this offset, where
/// the old fixed copy ran out.
const KEY_PARAMS_END: size_t = 64;

unsafe fn tty_keys_winsz(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    size: &mut size_t,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
    *size = 0 as size_t;
    if unsafe { terminal_client_owner.borrow_terminal() }.flags & TTY_WINSIZEQUERY == 0 {
        return -(1 as ::core::ffi::c_int);
    }
    if bytes[0] != b'\x1b' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[1] != b'[' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    let mut end: size_t = 2;
    while end < len && end != KEY_PARAMS_END {
        if bytes[end] == b't' {
            break;
        }
        if !bytes[end].is_ascii_digit() && bytes[end] != b';' {
            break;
        }
        end += 1;
    }
    if end == len {
        return 1 as ::core::ffi::c_int;
    }
    if end == KEY_PARAMS_END || bytes[end] != b't' {
        return -(1 as ::core::ffi::c_int);
    }
    let params = &bytes[2..end];
    if let Some((sy, sx)) = scan::scan_u32_pair(params, b"8;") {
        let terminal = unsafe { terminal_client_owner.borrow_terminal_mut() };
        terminal.sx = sx;
        terminal.sy = sy;
        *size = end + 1;
        return 0 as ::core::ffi::c_int;
    } else if let Some((ypixel, xpixel)) = scan::scan_u32_pair(params, b"4;") {
        {
            let terminal = unsafe { terminal_client_owner.borrow_terminal_mut() };
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
        unsafe {
            tty_invalidate(terminal_client_owner);
            terminal_client_owner.borrow_terminal_mut().flags &= !TTY_WINSIZEQUERY;
        }
        *size = end + 1;
        return 0 as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: unrecognized window size sequence: {}",
        log_cstr(terminal_client_owner.name().as_deref().unwrap_or(c"(null)")),
        log_bytes(params)
    ));
    -(1 as ::core::ffi::c_int)
}
pub(super) unsafe fn tty_keys_next(terminal_client_owner: &ClientRef) -> ::core::ffi::c_int {
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
    // SAFETY: `bytes` is the input buffer's storage, kept past the terminal
    // borrow that produced it while the code below takes the terminal again,
    // mutably too. The storage stays put while `bytes` is read: only the
    // read callback appends to the buffer and it does not run inside this
    // call, nothing here coalesces the buffer again, and each `drain` follows
    // the last read of `bytes` on its path (a complete key copies its bytes
    // into the event before replies and key dispatch run). The reply parsers
    // read `bytes` only before their first mutable terminal borrow, direct or
    // through `tty_update_features` and `tty_invalidate`.
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
    match tty_keys_clipboard(bytes, &mut size, &mut clipboard_reply) {
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
        match tty_keys_sync(terminal_client_owner, bytes, &mut size) {
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
                match tty_keys_device_attributes(terminal_client_owner, bytes, &mut size) {
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
                        match tty_keys_device_attributes2(terminal_client_owner, bytes, &mut size) {
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
                                    &mut size,
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
                                                &mut size,
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
                                                    &mut size,
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
                                                                &mut size,
                                                                &mut m,
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
                                                                        &mut size,
                                                                        &mut key,
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
                                                                            &mut size,
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
                    &mut key,
                    &mut size,
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
                if bytes[0] == b'\x1b' && len > 1 as size_t {
                    n = tty_keys_next1(
                        terminal_client_owner.borrow_terminal(),
                        diagnostic_name.as_deref(),
                        &bytes[1..],
                        &mut key,
                        &mut size,
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
                if bytes[0] == b'\x1b' && len >= 2 as size_t {
                    key = (bytes[1] as ::core::ffi::c_ulonglong | KEYC_META) as key_code;
                    size = 2 as size_t;
                } else {
                    key = bytes[0] as key_code;
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
                    delay = OptionsScope::GlobalServer
                        .with_local(|oo| options_get_number(oo, c"escape-time"))
                        as ::core::ffi::c_int;
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
                                super::control::tty_client_timer_callback(
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
        winlink_window(&session.current_winlink())
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

fn tty_keys_extended_key(
    tty: &tty,
    diagnostic_name: Option<&CStr>,
    bytes: &[u8],
    size: &mut size_t,
    key: &mut key_code,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
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
    if bytes[0] != b'\x1b' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[1] != b'[' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    let mut end: size_t = 2;
    while end < len && end != KEY_PARAMS_END {
        if bytes[end] == b'~' {
            break;
        }
        if !bytes[end].is_ascii_digit() && bytes[end] != b';' {
            break;
        }
        end += 1;
    }
    if end == len {
        return 1 as ::core::ffi::c_int;
    }
    if end == KEY_PARAMS_END || bytes[end] != b'~' && bytes[end] != b'u' {
        return -(1 as ::core::ffi::c_int);
    }
    let params = &bytes[2..end];
    let parsed = if bytes[end] == b'~' {
        scan::scan_u32_pair(params, b"27;").map(|(modifiers, number)| (number, modifiers))
    } else {
        scan::scan_u32_pair(params, b"")
    };
    let Some((number, mut modifiers)) = parsed else {
        return -(1 as ::core::ffi::c_int);
    };
    *size = end + 1;
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
            crate::log::log_bytes(key_string.as_bytes())
        ));
    }
    *key = nkey;
    0 as ::core::ffi::c_int
}
fn tty_keys_mouse(
    tty: &mut tty,
    diagnostic_name: Option<&CStr>,
    bytes: &[u8],
    size: &mut size_t,
    m: &mut mouse_event,
) -> ::core::ffi::c_int {
    let mut len: size_t = bytes.len();
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
    if bytes[0] as ::core::ffi::c_int != '\u{1b}' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[1] as ::core::ffi::c_int != '[' as i32 {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[2] as ::core::ffi::c_int == 'M' as i32 {
        *size = 3 as size_t;
        i = 0 as u_int;
        while i < 3 as u_int {
            if len <= *size {
                return 1 as ::core::ffi::c_int;
            }
            let fresh0 = *size;
            *size = (*size).wrapping_add(1);
            ch = bytes[fresh0];
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
    } else if bytes[2] as ::core::ffi::c_int == '<' as i32 {
        *size = 3 as size_t;
        loop {
            if len <= *size {
                return 1 as ::core::ffi::c_int;
            }
            let fresh1 = *size;
            *size = (*size).wrapping_add(1);
            ch = bytes[fresh1];
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
            ch = bytes[fresh2];
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
            ch = bytes[fresh3];
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
    m.lx = tty.mouse_last_x;
    m.x = x;
    m.ly = tty.mouse_last_y;
    m.y = y;
    m.lb = tty.mouse_last_b;
    m.b = b;
    m.sgr_type = sgr_type as u_int;
    m.sgr_b = sgr_b;
    tty.mouse_last_x = x;
    tty.mouse_last_y = y;
    tty.mouse_last_b = b;
    0 as ::core::ffi::c_int
}
fn tty_keys_clipboard(
    bytes: &[u8],
    size: &mut size_t,
    reply: &mut Option<input_request_clipboard_data>,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
    let mut terminator: size_t = 0 as size_t;
    let mut clip: ::core::ffi::c_char = 0 as ::core::ffi::c_char;
    *size = 0 as size_t;
    for (i, &expected) in b"\x1b]52;".iter().enumerate() {
        if bytes[i] != expected {
            return -(1 as ::core::ffi::c_int);
        }
        if len == i + 1 {
            return 1 as ::core::ffi::c_int;
        }
    }
    let mut end: size_t = 5;
    while end < len {
        if bytes[end] == b'\x07' {
            terminator = 1;
            break;
        } else if end > 5 && bytes[end - 1] == b'\x1b' && bytes[end] == b'\\' {
            terminator = 2;
            break;
        } else {
            end += 1;
        }
    }
    if end == len {
        return 1 as ::core::ffi::c_int;
    }
    *size = end + 1;
    let payload = &bytes[5..end + 1 - terminator];
    if payload.len() >= 2 && payload[0] != b';' && payload[1] == b';' {
        clip = payload[0] as ::core::ffi::c_char;
    }
    let Some(semicolon) = payload.iter().position(|&byte| byte == b';') else {
        return 0 as ::core::ffi::c_int;
    };
    let encoded = &payload[semicolon + 1..];
    if encoded.is_empty() {
        return 0 as ::core::ffi::c_int;
    }
    let Some(out) = crate::text::base64::decode(encoded) else {
        return 0 as ::core::ffi::c_int;
    };
    log_debug(format_args!(
        "{}: {}",
        "tty_keys_clipboard",
        log_cstr_n(&out)
    ));
    *reply = Some(input_request_clipboard_data { data: out, clip });
    0 as ::core::ffi::c_int
}
/// Primary and secondary DA parameters are shorter than this, the size of
/// the old fixed copy.
const DA_PARAMS_MAX: size_t = 128;

/// The DA parameters as numbers: at most 32 fields, each read by
/// `scan::strtoul_field`, stopping at a NUL as the C string split did.
fn tty_keys_da_fields(params: &[u8]) -> impl Iterator<Item = i8> + '_ {
    scan::until_nul(params)
        .split(|&byte| byte == b';')
        .take(32)
        .map(scan::strtoul_field)
}

/// Match `\e[<marker>...c` and return the parameters between them; `Err` is
/// the parser's answer when this is not a complete DA reply.
fn tty_keys_da_params(bytes: &[u8], marker: u8) -> Result<&[u8], ::core::ffi::c_int> {
    let len: size_t = bytes.len();
    for (i, &expected) in [b'\x1b', b'[', marker].iter().enumerate() {
        if bytes[i] != expected {
            return Err(-(1 as ::core::ffi::c_int));
        }
        if len == i + 1 {
            return Err(1 as ::core::ffi::c_int);
        }
    }
    let mut i: size_t = 0;
    while i < DA_PARAMS_MAX {
        if 3 + i == len {
            return Err(1 as ::core::ffi::c_int);
        }
        if bytes[3 + i].is_ascii_lowercase() {
            break;
        }
        i += 1;
    }
    if i == DA_PARAMS_MAX || bytes[3 + i] != b'c' {
        return Err(-(1 as ::core::ffi::c_int));
    }
    Ok(&bytes[3..3 + i])
}
unsafe fn tty_keys_device_attributes(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    size: &mut size_t,
) -> ::core::ffi::c_int {
    *size = 0 as size_t;
    if unsafe { terminal_client_owner.borrow_terminal() }.flags & TTY_HAVEDA != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    let params = match tty_keys_da_params(bytes, b'?') {
        Ok(params) => params,
        Err(n) => return n,
    };
    *size = params.len() + 4;
    let fields: Vec<i8> = tty_keys_da_fields(params).collect();
    if let [61..=65, features @ ..] = fields.as_slice() {
        for &p in features {
            log_debug(format_args!(
                "{}: DA feature: {}",
                log_cstr(terminal_client_owner.name().as_deref().unwrap_or(c"(null)")),
                p as ::core::ffi::c_int
            ));
            let feature = match p {
                4 => c"sixel",
                21 => c"margins",
                28 => c"rectfill",
                52 => c"clipboard",
                _ => continue,
            };
            unsafe { tty_parse_client_features(terminal_client_owner, feature, c",") };
        }
    }
    log_debug(format_args!(
        "{}: received primary DA {}",
        log_cstr(terminal_client_owner.name().as_deref().unwrap_or(c"(null)")),
        log_cstr_n(&bytes[..*size])
    ));
    unsafe {
        tty_update_features(terminal_client_owner);
        terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVEDA;
    }
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_sync(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    size: &mut size_t,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
    let prefix = b"\x1B[?2026;";
    *size = 0 as size_t;
    if unsafe { terminal_client_owner.borrow_terminal() }.flags & TTY_HAVESYNC != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    for (i, &expected) in prefix.iter().enumerate() {
        if i == len {
            return 1 as ::core::ffi::c_int;
        }
        if bytes[i] != expected {
            return -(1 as ::core::ffi::c_int);
        }
    }
    let mut i: size_t = prefix.len();
    if i == len {
        return 1 as ::core::ffi::c_int;
    }
    if !(b'0'..=b'4').contains(&bytes[i]) {
        return -(1 as ::core::ffi::c_int);
    }
    let status = bytes[i] - b'0';
    i += 1;
    for &expected in b"$y" {
        if i == len {
            return 1 as ::core::ffi::c_int;
        }
        if bytes[i] != expected {
            return -(1 as ::core::ffi::c_int);
        }
        i += 1;
    }
    *size = i;
    log_debug(format_args!(
        "{}: received DECRPM {}",
        log_cstr(terminal_client_owner.name().as_deref().unwrap_or(c"(null)")),
        log_cstr_n(&bytes[..*size])
    ));
    if let 1..=3 = status {
        unsafe {
            tty_parse_client_features(terminal_client_owner, c"sync", c",");
            tty_update_features(terminal_client_owner);
        }
    }
    unsafe { terminal_client_owner.borrow_terminal_mut() }.flags |= TTY_HAVESYNC;
    0 as ::core::ffi::c_int
}
unsafe fn tty_keys_device_attributes2(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    size: &mut size_t,
) -> ::core::ffi::c_int {
    *size = 0 as size_t;
    if unsafe { terminal_client_owner.borrow_terminal() }.flags & TTY_HAVEDA2 != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    let params = match tty_keys_da_params(bytes, b'>') {
        Ok(params) => params,
        Err(n) => return n,
    };
    *size = params.len() + 4;
    let terminal = match tty_keys_da_fields(params).next() {
        Some(77) => Some(c"mintty"),
        Some(84) => Some(c"tmux"),
        Some(85) => Some(c"rxvt-unicode"),
        _ => None,
    };
    if let Some(terminal) = terminal {
        unsafe { tty_default_features(terminal_client_owner, terminal) };
    }
    log_debug(format_args!(
        "{}: received secondary DA {}",
        log_cstr(terminal_client_owner.name().as_deref().unwrap_or(c"(null)")),
        log_cstr_n(&bytes[..*size])
    ));
    unsafe {
        tty_update_features(terminal_client_owner);
        terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVEDA2;
    }
    0 as ::core::ffi::c_int
}
/// Extended DA, colour and palette reply text is shorter than this, one
/// less than the old fixed copy.
const REPLY_TEXT_MAX: size_t = 127;

/// Find the end of `\e<header><text>` terminated by `\e\\`, or also by BEL
/// when `bel` is set: `Ok` is the length of the text including the Escape
/// of `\e\\`, `Err` the parser's answer when the reply is incomplete or too
/// long. The header has been matched already.
fn tty_keys_reply_text_len(
    bytes: &[u8],
    header: size_t,
    bel: bool,
) -> Result<size_t, ::core::ffi::c_int> {
    let mut i: size_t = 0;
    while i < REPLY_TEXT_MAX {
        if header + i == bytes.len() {
            return Err(1 as ::core::ffi::c_int);
        }
        // The header's last byte is never an Escape, so the text does not
        // end before it starts.
        if bytes[header + i - 1] == b'\x1b' && bytes[header + i] == b'\\' {
            break;
        }
        if bel && bytes[header + i] == b'\x07' {
            break;
        }
        i += 1;
    }
    if i == REPLY_TEXT_MAX {
        return Err(-(1 as ::core::ffi::c_int));
    }
    Ok(i)
}
unsafe fn tty_keys_extended_device_attributes(
    terminal_client_owner: &ClientRef,
    bytes: &[u8],
    size: &mut size_t,
) -> ::core::ffi::c_int {
    /// Text prefixes in the order they are tried, with the feature set each
    /// selects; a prefix is compared as `strncmp` compared it.
    const TERMINALS: [(&[u8], &CStr); 8] = [
        (b"iTerm2 ", c"iTerm2"),
        (b"tmux ", c"tmux"),
        (b"XTerm(", c"XTerm"),
        (b"mintty ", c"mintty"),
        (b"foot(", c"foot"),
        (b"WezTerm", c"WezTerm"),
        (b"ghostty ", c"ghostty"),
        (b"Rio ", c"Rio"),
    ];
    let len: size_t = bytes.len();
    *size = 0 as size_t;
    if unsafe { terminal_client_owner.borrow_terminal() }.flags & TTY_HAVEXDA != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    for (i, &expected) in b"\x1bP>|".iter().enumerate() {
        if bytes[i] != expected {
            return -(1 as ::core::ffi::c_int);
        }
        if len == i + 1 {
            return 1 as ::core::ffi::c_int;
        }
    }
    let i = match tty_keys_reply_text_len(bytes, 4, false) {
        Ok(i) => i,
        Err(n) => return n,
    };
    *size = 5 + i;
    if i == 0 {
        return 0 as ::core::ffi::c_int;
    }
    let text = scan::until_nul(&bytes[4..4 + i - 1]);
    let terminal_type = CString::new(text).expect("terminal type stops at a NUL");
    if let Some(&(_, name)) = TERMINALS
        .iter()
        .find(|(prefix, _)| text.starts_with(prefix))
    {
        unsafe { tty_default_features(terminal_client_owner, name) };
    }
    log_debug(format_args!(
        "{}: received extended DA {}",
        log_cstr(terminal_client_owner.name().as_deref().unwrap_or(c"(null)")),
        log_cstr_n(&bytes[..*size])
    ));
    unsafe {
        terminal_client_owner.record_terminal_type(&terminal_type);
        tty_update_features(terminal_client_owner);
        terminal_client_owner.borrow_terminal_mut().flags |= TTY_HAVEXDA;
    }
    0 as ::core::ffi::c_int
}
/// The text of a BEL or `\e\\` terminated reply as the colour parsers read
/// it: without a final Escape and up to the first NUL.
fn tty_keys_colour_text(text: &[u8]) -> &[u8] {
    scan::until_nul(text.strip_suffix(b"\x1b").unwrap_or(text))
}
pub(super) fn tty_keys_colours(
    flags: &mut i32,
    diagnostic_name: Option<&std::ffi::CStr>,
    bytes: &[u8],
    size: &mut size_t,
    fg: &mut i32,
    bg: &mut i32,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
    *size = 0 as size_t;
    if bytes[0] != b'\x1b' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 1 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[1] != b']' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 2 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[2] != b'1' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 3 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[3] != b'0' && bytes[3] != b'1' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 4 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    if bytes[4] != b';' {
        return -(1 as ::core::ffi::c_int);
    }
    if len == 5 as size_t {
        return 1 as ::core::ffi::c_int;
    }
    let i = match tty_keys_reply_text_len(bytes, 5, true) {
        Ok(i) => i,
        Err(n) => return n,
    };
    *size = 6 + i;
    if i == 0 {
        return 0 as ::core::ffi::c_int;
    }
    let text =
        CString::new(tty_keys_colour_text(&bytes[5..5 + i])).expect("colour reply stops at a NUL");
    let Some(n) = colour_parse_x11_logged(&text) else {
        return 0 as ::core::ffi::c_int;
    };
    if bytes[3] == b'0' {
        if let Some(name) = diagnostic_name {
            log_debug(format_args!(
                "{} fg is {}",
                log_cstr(name),
                crate::log::log_bytes(colour_format(n).as_bytes())
            ));
        } else {
            log_debug(format_args!(
                "fg is {}",
                crate::log::log_bytes(colour_format(n).as_bytes())
            ));
        }
        *fg = n;
        *flags &= !TTY_WAITFG;
    } else {
        if let Some(name) = diagnostic_name {
            log_debug(format_args!(
                "{} bg is {}",
                log_cstr(name),
                crate::log::log_bytes(colour_format(n).as_bytes())
            ));
        } else {
            log_debug(format_args!(
                "bg is {}",
                crate::log::log_bytes(colour_format(n).as_bytes())
            ));
        }
        *bg = n;
        *flags &= !TTY_WAITBG;
    }
    0 as ::core::ffi::c_int
}
fn tty_keys_palette(
    bytes: &[u8],
    size: &mut size_t,
    reply: &mut Option<input_request_palette_data>,
) -> ::core::ffi::c_int {
    let len: size_t = bytes.len();
    *size = 0 as size_t;
    for (i, &expected) in b"\x1b]4;".iter().enumerate() {
        if bytes[i] != expected {
            return -(1 as ::core::ffi::c_int);
        }
        if len == i + 1 {
            return 1 as ::core::ffi::c_int;
        }
    }
    let i = match tty_keys_reply_text_len(bytes, 4, true) {
        Ok(i) => i,
        Err(n) => return n,
    };
    *size = 5 + i;
    if i == 0 {
        return 0 as ::core::ffi::c_int;
    }
    let text = tty_keys_colour_text(&bytes[4..4 + i]);
    let (idx, consumed) = scan::strtol_prefix(text);
    if text.get(consumed) != Some(&b';') {
        return -(1 as ::core::ffi::c_int);
    }
    if !(0..=255).contains(&idx) {
        return -(1 as ::core::ffi::c_int);
    }
    let colour = CString::new(&text[consumed + 1..]).expect("palette reply stops at a NUL");
    let Some(c) = colour_parse_x11_logged(&colour) else {
        return 0 as ::core::ffi::c_int;
    };
    *reply = Some(input_request_palette_data { idx, c });
    0 as ::core::ffi::c_int
}

/// Byte-slice forms of the C conversions the reply parsers once ran on
/// NUL-terminated copies of the reply.
mod scan {
    use crate::text::numbers::{integer_prefix, IntegerPrefix};

    /// The bytes before the first NUL: what a C string function reading a
    /// copy of `text` saw.
    pub(super) fn until_nul(text: &[u8]) -> &[u8] {
        text.split(|&byte| byte == 0).next().unwrap_or(text)
    }

    /// strtoul's result: `ULONG_MAX` on overflow, the magnitude negated
    /// modulo 2^64 after a minus sign.
    fn unsigned_long(prefix: &IntegerPrefix) -> u64 {
        match prefix.magnitude {
            None => u64::MAX,
            Some(magnitude) if prefix.negative => magnitude.wrapping_neg(),
            Some(magnitude) => magnitude,
        }
    }

    /// glibc `%u`: optional whitespace and sign, at least one digit, read
    /// as `unsigned long` and stored truncated to 32 bits, so a number past
    /// `u64` saturates to `u32::MAX` and anything shorter wraps modulo
    /// 2^32. Returns the value and the bytes consumed.
    pub(super) fn scan_u32(field: &[u8]) -> Option<(u32, usize)> {
        let prefix = integer_prefix(field, 10);
        (prefix.consumed != 0).then(|| (unsigned_long(&prefix) as u32, prefix.consumed))
    }

    /// `sscanf(text, "<literal>%u;%u")` returning 2; bytes after the second
    /// number are ignored.
    pub(super) fn scan_u32_pair(text: &[u8], literal: &[u8]) -> Option<(u32, u32)> {
        let text = text.strip_prefix(literal)?;
        let (first, consumed) = scan_u32(text)?;
        let text = text[consumed..].strip_prefix(b";")?;
        let (second, _) = scan_u32(text)?;
        Some((first, second))
    }

    /// `strtoul(field, &end, 10)` cast to `char`, or 0 when `end` stops
    /// short of the end of the field. An empty field is 0.
    pub(super) fn strtoul_field(field: &[u8]) -> i8 {
        let prefix = integer_prefix(field, 10);
        if prefix.consumed != field.len() {
            return 0;
        }
        unsigned_long(&prefix) as i8
    }

    /// `strtol(text, &end, 10)` cast to `int`, with the bytes consumed (0
    /// when there are no digits, where `end` is `text`). The `long` result
    /// saturates first, so `LONG_MAX` becomes -1 and `LONG_MIN` becomes 0.
    pub(super) fn strtol_prefix(text: &[u8]) -> (i32, usize) {
        let prefix = integer_prefix(text, 10);
        let long = match prefix.magnitude {
            Some(magnitude) if prefix.negative => {
                0i64.checked_sub_unsigned(magnitude).unwrap_or(i64::MIN)
            }
            Some(magnitude) => i64::try_from(magnitude).unwrap_or(i64::MAX),
            None if prefix.negative => i64::MIN,
            None => i64::MAX,
        };
        (long as i32, prefix.consumed)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::ffi::CString;

        const INPUTS: &[&[u8]] = &[
            b"",
            b"0",
            b"7",
            b"007",
            b" 7",
            b"\t\n\x0b\x0c\r7",
            b"+7",
            b"-7",
            b"-0",
            b"+",
            b"-",
            b" ",
            b"7x",
            b"x7",
            b"7 ",
            b"7;8",
            b"255",
            b"256",
            b"260",
            b"-252",
            b"2147483647",
            b"2147483648",
            b"-2147483649",
            b"4294967295",
            b"4294967296",
            b"4294967303",
            b"9223372036854775807",
            b"9223372036854775808",
            b"-9223372036854775808",
            b"-9223372036854775809",
            b"18446744073709551615",
            b"18446744073709551616",
            b"-18446744073709551615",
            b"-18446744073709551616",
            b"99999999999999999999999999",
            b"-99999999999999999999999999",
        ];

        #[test]
        fn conversions_match_libc() {
            for &input in INPUTS {
                let text = CString::new(input).unwrap();
                unsafe {
                    let mut end = std::ptr::null_mut();
                    let value = libc::strtoul(text.as_ptr(), &mut end, 10);
                    let whole = *end == 0;
                    let expected = if whole {
                        value as ::core::ffi::c_char
                    } else {
                        0
                    };
                    assert_eq!(strtoul_field(input), expected as i8, "strtoul {input:?}");

                    let value = libc::strtol(text.as_ptr(), &mut end, 10);
                    let consumed = end.offset_from(text.as_ptr()) as usize;
                    assert_eq!(
                        strtol_prefix(input),
                        (value as ::core::ffi::c_int, consumed),
                        "strtol {input:?}"
                    );

                    let mut value: ::core::ffi::c_uint = 0;
                    let mut consumed: ::core::ffi::c_int = 0;
                    let n =
                        libc::sscanf(text.as_ptr(), c"%u%n".as_ptr(), &mut value, &mut consumed);
                    let expected = (n == 1).then_some((value, consumed as usize));
                    assert_eq!(scan_u32(input), expected, "%u {input:?}");
                }
            }
        }

        #[test]
        fn pairs_match_sscanf() {
            for &input in &[
                b"8;24;80".as_slice(),
                b"8;24;80;5",
                b"8;24;80x",
                b"8;24",
                b"8;;80",
                b"8; 24;+80",
                b"8;24 ;80",
                b"88;24;80",
                b"4;4294967297;99999999999999999999999",
            ] {
                let text = CString::new(input).unwrap();
                let (mut first, mut second): (::core::ffi::c_uint, ::core::ffi::c_uint) = (0, 0);
                let n = unsafe {
                    libc::sscanf(text.as_ptr(), c"8;%u;%u".as_ptr(), &mut first, &mut second)
                };
                let expected = (n == 2).then_some((first, second));
                assert_eq!(scan_u32_pair(input, b"8;"), expected, "{input:?}");
            }
        }

        #[test]
        fn text_stops_at_the_first_nul() {
            assert_eq!(until_nul(b"ab\0cd\0"), b"ab");
            assert_eq!(until_nul(b"\0ab"), b"");
            assert_eq!(until_nul(b"ab"), b"ab");
            assert_eq!(until_nul(b""), b"");
        }
    }
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
        tty_keys_add(&mut terminal, b"ab", 1);
        tty_keys_add(&mut terminal, b"ab", 2);
        assert_eq!(lookup(&terminal.key_tree, b"ab"), (Some(2), 2));
        tty_keys_add(&mut terminal, b"a", 3);
        assert_eq!(lookup(&terminal.key_tree, b"a"), (Some(3), 1));
        assert_eq!(lookup(&terminal.key_tree, b"ab"), (Some(2), 2));
        tty_keys_add(&mut terminal, b"abc", 4);
        assert_eq!(lookup(&terminal.key_tree, b"abc"), (Some(4), 2));
    }

    #[test]
    fn ambiguous_key_waits_until_timeout_or_completion() {
        let mut terminal = tty::default();
        tty_keys_add1(&mut terminal.key_tree, b"ab", 1);
        tty_keys_add1(&mut terminal.key_tree, b"a", 2);
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

    #[test]
    fn clearing_is_repeatable_and_allows_repopulation() {
        let mut terminal = tty::default();
        tty_keys_free(&mut terminal);
        tty_keys_add(&mut terminal, b"old", 1);
        tty_keys_free(&mut terminal);
        assert!(terminal.key_tree.is_none());
        tty_keys_free(&mut terminal);
        tty_keys_add(&mut terminal, b"new", 2);
        assert_eq!(lookup(&terminal.key_tree, b"new"), (Some(2), 3));
        assert_eq!(lookup(&terminal.key_tree, b"old"), (None, 0));
    }
}

#[cfg(test)]
mod reply_parser_tests {
    use super::*;
    use crate::format::format_create;
    use crate::server_client::queue_test_clients;

    /// A client with an open terminal that has no capabilities, so feature
    /// updates run without a real terminal behind them.
    unsafe fn client() -> ClientRef {
        for name in [c"extended-keys", c"focus-events"] {
            let definition = crate::options::options_table
                .iter()
                .find(|entry| entry.name == Some(name))
                .unwrap();
            OptionsScope::GlobalServer.reset_default(definition);
        }
        let owner = queue_test_clients::with_queue();
        let caps = [
            c"clear=\x1b[H\x1b[2J".to_owned(),
            c"cup=\x1b[%i%p1%d;%p2%dH".to_owned(),
        ];
        let term = super::super::term::tty_term_create(&owner, c"reply-test", &caps).unwrap();
        let terminal = owner.borrow_terminal_mut();
        terminal.term = Some(term);
        terminal.out = Some(Box::default());
        owner
    }

    unsafe fn features_of(name: &CStr) -> i32 {
        let owner = client();
        tty_default_features(&owner, name);
        owner.terminal_feature_mask()
    }

    /// The feature mask of a fresh client with `features` added to what
    /// its terminal already brings.
    unsafe fn features_with(features: &CStr) -> i32 {
        let owner = client();
        if !features.is_empty() {
            tty_parse_client_features(&owner, features, c",");
        }
        owner.terminal_feature_mask()
    }

    unsafe fn term_type(owner: &ClientRef) -> CString {
        let mut context = format_create(None, None, 0, 0);
        context.c = std::rc::Rc::downgrade(owner);
        let Some(crate::format::FormatValue::String(value)) =
            owner.format_value(c"client_termtype", &mut context)
        else {
            panic!("client_termtype");
        };
        value
    }

    /// Every strict prefix of `sequence` waits for more input.
    fn assert_prefixes_wait(
        mut parse: impl FnMut(&[u8], &mut size_t) -> ::core::ffi::c_int,
        sequence: &[u8],
    ) {
        for end in 1..sequence.len() {
            let mut size = 99;
            assert_eq!(
                parse(&sequence[..end], &mut size),
                1,
                "prefix {:?}",
                &sequence[..end]
            );
        }
    }

    /// Replacing any of the first `header` bytes rejects the sequence.
    fn assert_header_rejects(
        mut parse: impl FnMut(&[u8], &mut size_t) -> ::core::ffi::c_int,
        sequence: &[u8],
        header: usize,
    ) {
        for at in 0..header {
            let mut wrong = sequence.to_vec();
            wrong[at] = 0x01;
            let mut size = 99;
            assert_eq!(parse(&wrong, &mut size), -1, "byte {at} of {sequence:?}");
        }
    }

    unsafe fn winsz(owner: &ClientRef, bytes: &[u8]) -> (i32, size_t) {
        let mut size = 99;
        (tty_keys_winsz(owner, bytes, &mut size), size)
    }

    #[test]
    fn window_size_reports() {
        unsafe {
            let owner = client();
            assert_eq!(winsz(&owner, b"\x1b[8;24;80t"), (-1, 0));
            owner.borrow_terminal_mut().flags |= TTY_WINSIZEQUERY;
            assert_prefixes_wait(|b, s| tty_keys_winsz(&owner, b, s), b"\x1b[8;24;80t");
            assert_header_rejects(|b, s| tty_keys_winsz(&owner, b, s), b"\x1b[8;24;80t", 2);
            assert_eq!(winsz(&owner, b"\x1b[8;24;80t"), (0, 10));
            assert_eq!(
                (owner.borrow_terminal().sy, owner.borrow_terminal().sx),
                (24, 80)
            );
            assert_eq!(winsz(&owner, b"\x1b[8;24;80tx"), (0, 10));
            assert_eq!(winsz(&owner, b"\x1b[8;;80t"), (-1, 0));
            assert_eq!(winsz(&owner, b"\x1b[8;24t"), (-1, 0));
            assert_eq!(winsz(&owner, b"\x1b[8;24;80;5t"), (0, 12));
            assert_eq!(winsz(&owner, b"\x1b[8;24;80x"), (-1, 0));

            assert_eq!(winsz(&owner, b"\x1b[8;4294967297;80t"), (0, 18));
            assert_eq!(owner.borrow_terminal().sy, 1);
            assert_eq!(winsz(&owner, b"\x1b[8;999999999999999999999;80t"), (0, 29));
            assert_eq!(owner.borrow_terminal().sy, u32::MAX);

            owner.borrow_terminal_mut().sx = 80;
            owner.borrow_terminal_mut().sy = 24;
            assert_eq!(winsz(&owner, b"\x1b[4;480;800t"), (0, 12));
            {
                let terminal = owner.borrow_terminal();
                assert_eq!((terminal.xpixel, terminal.ypixel), (10, 20));
                assert_eq!(terminal.flags & TTY_WINSIZEQUERY, 0);
            }
            assert_eq!(winsz(&owner, b"\x1b[4;480;800t"), (-1, 0));
            owner.borrow_terminal_mut().flags |= TTY_WINSIZEQUERY;
            owner.borrow_terminal_mut().sx = 0;
            assert_eq!(winsz(&owner, b"\x1b[4;480;800t"), (0, 12));
            {
                let terminal = owner.borrow_terminal();
                assert_eq!((terminal.xpixel, terminal.ypixel), (0, 20));
            }
            owner.borrow_terminal_mut().flags |= TTY_WINSIZEQUERY;

            // The parameters end before byte 64.
            let mut report = b"\x1b[8;24;".to_vec();
            report.resize(61, b'0');
            report.extend_from_slice(b"80t");
            assert_eq!(winsz(&owner, &report), (0, 64));
            report.insert(7, b'0');
            assert_eq!(winsz(&owner, &report), (-1, 0));
            assert_eq!(winsz(&owner, &report[..64]), (1, 0));
            assert_eq!(winsz(&owner, &report[..65]), (-1, 0));
        }
    }

    fn extended_key(tty: &tty, bytes: &[u8]) -> (i32, size_t, key_code) {
        let mut size = 99;
        let mut key = 0;
        let n = tty_keys_extended_key(tty, None, bytes, &mut size, &mut key);
        (n, size, key)
    }

    #[test]
    fn extended_keys() {
        let mut terminal = tty::default();
        assert_prefixes_wait(
            |b, s| tty_keys_extended_key(&terminal, None, b, s, &mut 0),
            b"\x1b[27;5;9~",
        );
        assert_header_rejects(
            |b, s| tty_keys_extended_key(&terminal, None, b, s, &mut 0),
            b"\x1b[27;5;9~",
            2,
        );
        assert_eq!(
            extended_key(&terminal, b"\x1b[27;5;9~"),
            (0, 9, '\t' as key_code | KEYC_CTRL)
        );
        assert_eq!(
            extended_key(&terminal, b"\x1b[9;5u"),
            (0, 6, '\t' as key_code | KEYC_CTRL)
        );
        assert_eq!(extended_key(&terminal, b"\x1b[9;5x"), (-1, 0, 0));
        assert_eq!(extended_key(&terminal, b"\x1b[28;5;9~"), (-1, 0, 0));
        assert_eq!(extended_key(&terminal, b"\x1b[9u"), (-1, 0, 0));
        assert_eq!(extended_key(&terminal, b"\x1b[;5u"), (-1, 0, 0));
        assert_eq!(extended_key(&terminal, b"\x1b[13;1;7u"), (0, 9, 13));
        assert_eq!(extended_key(&terminal, b"\x1b[13;0u"), (0, 7, 13));
        assert_eq!(
            extended_key(&terminal, b"\x1b[13;2u"),
            (0, 7, 13 | KEYC_SHIFT)
        );
        let meta = KEYC_META | KEYC_IMPLIED_META;
        assert_eq!(extended_key(&terminal, b"\x1b[13;3u"), (0, 7, 13 | meta));
        assert_eq!(
            extended_key(&terminal, b"\x1b[13;5u"),
            (0, 7, 13 | KEYC_CTRL)
        );
        assert_eq!(extended_key(&terminal, b"\x1b[13;9u"), (0, 7, 13 | meta));
        assert_eq!(
            extended_key(&terminal, b"\x1b[9;2u"),
            (0, 6, KEYC_BTAB as key_code)
        );
        assert_eq!(
            extended_key(&terminal, b"\x1b[9;6u"),
            (0, 6, KEYC_BTAB as key_code | KEYC_CTRL)
        );
        assert_eq!(extended_key(&terminal, b"\x1b[65;2u"), (0, 7, 65));
        assert_eq!(
            extended_key(&terminal, b"\x1b[65;6u"),
            (0, 7, 65 | KEYC_SHIFT | KEYC_CTRL)
        );
        let (n, _, accented) = extended_key(&terminal, b"\x1b[233;2u");
        assert_eq!(n, 0);
        assert_eq!(accented & KEYC_SHIFT, 0);
        assert_eq!(extended_key(&terminal, b"\x1b[233;1u"), (0, 8, accented));
        assert_eq!(extended_key(&terminal, b"\x1b[1114112;1u"), (-1, 12, 0));
        assert_eq!(extended_key(&terminal, b"\x1b[127;1u"), (0, 8, 127));
        terminal.tio.0.c_cc[VERASE] = 0x7f;
        assert_eq!(
            extended_key(&terminal, b"\x1b[127;1u"),
            (0, 8, KEYC_BSPACE as key_code)
        );
        assert_eq!(
            extended_key(&terminal, b"\x1b[27;5;127~"),
            (0, 11, KEYC_BSPACE as key_code | KEYC_CTRL)
        );
        // A number past u32 wraps, one past u64 saturates.
        assert_eq!(extended_key(&terminal, b"\x1b[4294967309;1u"), (0, 15, 13));
        assert_eq!(
            extended_key(&terminal, b"\x1b[13;99999999999999999999999u").2,
            extended_key(&terminal, b"\x1b[13;4294967295u").2
        );

        let mut report = b"\x1b[".to_vec();
        report.resize(59, b'0');
        report.extend_from_slice(b"13;5u");
        assert_eq!(extended_key(&terminal, &report), (0, 64, 13 | KEYC_CTRL));
        report.insert(2, b'0');
        assert_eq!(extended_key(&terminal, &report), (-1, 0, 0));
        assert_eq!(extended_key(&terminal, &report[..64]), (1, 0, 0));
    }

    fn mouse(tty: &mut tty, bytes: &[u8]) -> (i32, size_t, mouse_event) {
        let mut size = 99;
        let mut m = mouse_event::default();
        let n = tty_keys_mouse(tty, None, bytes, &mut size, &mut m);
        (n, size, m)
    }

    #[test]
    fn mouse_reports() {
        let mut terminal = tty::default();
        for sequence in [b"\x1b[M !!".as_slice(), b"\x1b[<0;10;5M"] {
            assert_prefixes_wait(
                |b, s| tty_keys_mouse(&mut terminal, None, b, s, &mut Default::default()),
                sequence,
            );
            assert_header_rejects(
                |b, s| tty_keys_mouse(&mut terminal, None, b, s, &mut Default::default()),
                sequence,
                3,
            );
        }
        let (n, size, m) = mouse(&mut terminal, b"\x1b[M\x22\x2a\x25");
        assert_eq!((n, size), (0, 6));
        assert_eq!(
            (m.b, m.x, m.y, m.sgr_type, m.sgr_b),
            (2, 9, 4, b' ' as u32, 0)
        );
        assert_eq!((m.lb, m.lx, m.ly), (0, 0, 0));
        assert_eq!(
            (
                terminal.mouse_last_b,
                terminal.mouse_last_x,
                terminal.mouse_last_y
            ),
            (2, 9, 4)
        );
        assert_eq!(mouse(&mut terminal, b"\x1b[M\x1f!!").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[M  !").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[M ! ").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[M \xff\xff").1, 6);

        let (n, size, m) = mouse(&mut terminal, b"\x1b[<1;10;5Mx");
        assert_eq!((n, size), (0, 10));
        assert_eq!(
            (m.b, m.x, m.y, m.sgr_type, m.sgr_b),
            (1, 9, 4, b'M' as u32, 1)
        );
        assert_eq!((m.lb, m.lx, m.ly), (0, 222, 222));
        let (n, size, m) = mouse(&mut terminal, b"\x1b[<1;12;7m");
        assert_eq!((n, size), (0, 10));
        assert_eq!(
            (m.b, m.x, m.y, m.sgr_type, m.sgr_b),
            (3, 11, 6, b'm' as u32, 1)
        );
        assert_eq!(mouse(&mut terminal, b"\x1b[<64;1;1m").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[<65;1;1m").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[<64;1;1M").0, 0);
        assert_eq!(mouse(&mut terminal, b"\x1b[<0;0;1M").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[<0;1;0M").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[<0;1x;1M").0, -1);
        assert_eq!(mouse(&mut terminal, b"\x1b[<x;1;1M").0, -1);
        assert_eq!(mouse(&mut terminal, b"\x1b[<0;1;1;M").0, -1);
        assert_eq!(mouse(&mut terminal, b"\x1b[<;;1M").0, -2);
        assert_eq!(mouse(&mut terminal, b"\x1b[X").0, -1);
        assert_eq!(
            (
                terminal.mouse_last_b,
                terminal.mouse_last_x,
                terminal.mouse_last_y
            ),
            (64, 0, 0)
        );
    }

    fn clipboard(bytes: &[u8]) -> (i32, size_t, Option<(Vec<u8>, u8)>) {
        let mut size = 99;
        let mut reply = None;
        let n = tty_keys_clipboard(bytes, &mut size, &mut reply);
        (n, size, reply.map(|reply| (reply.data, reply.clip as u8)))
    }

    #[test]
    fn clipboard_replies() {
        let sequence = b"\x1b]52;c;aGk=\x07";
        assert_prefixes_wait(|b, s| tty_keys_clipboard(b, s, &mut None), sequence);
        assert_header_rejects(|b, s| tty_keys_clipboard(b, s, &mut None), sequence, 5);
        assert_eq!(clipboard(sequence), (0, 12, Some((b"hi".to_vec(), b'c'))));
        assert_eq!(
            clipboard(b"\x1b]52;c;aGk=\x1b\\x"),
            (0, 13, Some((b"hi".to_vec(), b'c')))
        );
        assert_prefixes_wait(
            |b, s| tty_keys_clipboard(b, s, &mut None),
            b"\x1b]52;c;aGk=\x1b\\",
        );
        assert_eq!(
            clipboard(b"\x1b]52;;aGk=\x07"),
            (0, 11, Some((b"hi".to_vec(), 0)))
        );
        assert_eq!(
            clipboard(b"\x1b]52;pc;aGk=\x07"),
            (0, 13, Some((b"hi".to_vec(), 0)))
        );
        assert_eq!(clipboard(b"\x1b]52;aGk=\x07"), (0, 10, None));
        assert_eq!(clipboard(b"\x1b]52;c;\x07"), (0, 8, None));
        assert_eq!(clipboard(b"\x1b]52;c;\x1b\\"), (0, 9, None));
        assert_eq!(clipboard(b"\x1b]52;\x1b\\"), (0, 7, None));
        assert_eq!(clipboard(b"\x1b]52;\x07"), (0, 6, None));
        assert_eq!(clipboard(b"\x1b]52;c;!!\x07"), (0, 10, None));
        assert_eq!(clipboard(b"\x1b]52;c;aG\0k=\x07"), (0, 13, None));
        assert_eq!(clipboard(b"\x1b]52;c;;aGk=\x07"), (0, 13, None));
    }

    fn da(owner: &ClientRef, bytes: &[u8]) -> (i32, size_t) {
        let mut size = 99;
        (
            unsafe { tty_keys_device_attributes(owner, bytes, &mut size) },
            size,
        )
    }

    unsafe fn da_features(params: &[u8]) -> i32 {
        let owner = client();
        let mut report = b"\x1b[?".to_vec();
        report.extend_from_slice(params);
        report.push(b'c');
        assert_eq!(da(&owner, &report), (0, report.len()));
        owner.terminal_feature_mask()
    }

    #[test]
    fn primary_device_attributes() {
        unsafe {
            let owner = client();
            let sequence = b"\x1b[?64;4;21;28;52c";
            assert_prefixes_wait(|b, s| tty_keys_device_attributes(&owner, b, s), sequence);
            assert_header_rejects(|b, s| tty_keys_device_attributes(&owner, b, s), sequence, 3);
            assert_eq!(da(&owner, b"\x1b[?64;4x"), (-1, 0));
            assert_eq!(da(&owner, b"\x1b[?64;4cX"), (0, 8));
            assert_eq!(owner.terminal_feature_mask(), features_with(c"sixel"));
            assert_eq!(owner.borrow_terminal().flags & TTY_HAVEDA, TTY_HAVEDA);
            assert_eq!(da(&owner, sequence), (-1, 0));

            let none = features_with(c"");
            let sixel = features_with(c"sixel");
            assert_eq!(
                da_features(b"64;4;21;28;52"),
                features_with(c"clipboard,margins,rectfill,sixel")
            );
            assert_eq!(da_features(b"62;4"), sixel);
            assert_eq!(da_features(b"60;4"), none);
            assert_eq!(da_features(b"66;4"), none);
            assert_eq!(da_features(b""), none);
            assert_eq!(da_features(b"64"), none);
            for (field, has_sixel) in [
                (b" 4".as_slice(), true),
                (b"+4", true),
                (b"\t4", true),
                (b"04", true),
                (b"260", true),
                (b"-252", true),
                (b"-1", false),
                (b"4 ", false),
                (b"4X", false),
                (b"", false),
                (b"+", false),
                (b"9999999999999999999999999", false),
                (b"18446744073709551620", false),
                (b"18446744073709551364", true),
            ] {
                let mut params = b"64;".to_vec();
                params.extend_from_slice(field);
                assert_eq!(da_features(&params) == sixel, has_sixel, "{field:?}");
            }
            assert_eq!(da_features(b" 64;4"), sixel);
            assert_eq!(da_features(b"64;1\0;4"), none);
            assert_eq!(da_features(b"64;1;4\0"), sixel);

            let mut params = b"64".to_vec();
            for _ in 0..30 {
                params.extend_from_slice(b";1");
            }
            params.extend_from_slice(b";4");
            assert_eq!(da_features(&params), sixel);
            params.insert(3, b'1');
            params.insert(3, b';');
            assert_eq!(da_features(&params), none);

            let owner = client();
            let mut report = b"\x1b[?".to_vec();
            report.resize(130, b'1');
            assert_eq!(da(&owner, &report), (1, 0));
            report.push(b'1');
            assert_eq!(da(&owner, &report), (-1, 0));
            report.truncate(130);
            report.push(b'c');
            assert_eq!(da(&owner, &report), (0, 131));
        }
    }

    fn da2(owner: &ClientRef, bytes: &[u8]) -> (i32, size_t) {
        let mut size = 99;
        (
            unsafe { tty_keys_device_attributes2(owner, bytes, &mut size) },
            size,
        )
    }

    #[test]
    fn secondary_device_attributes() {
        unsafe {
            let owner = client();
            let sequence = b"\x1b[>84;0;0c";
            assert_prefixes_wait(|b, s| tty_keys_device_attributes2(&owner, b, s), sequence);
            assert_header_rejects(
                |b, s| tty_keys_device_attributes2(&owner, b, s),
                sequence,
                3,
            );
            assert_eq!(da2(&owner, sequence), (0, 10));
            assert_eq!(owner.terminal_feature_mask(), features_of(c"tmux"));
            assert_eq!(da2(&owner, sequence), (-1, 0));
            for (report, name) in [
                (b"\x1b[>77;20005;0c".as_slice(), Some(c"mintty")),
                (b"\x1b[>85;95;0c", Some(c"rxvt-unicode")),
                (b"\x1b[> 85c", Some(c"rxvt-unicode")),
                (b"\x1b[>341;85c", Some(c"rxvt-unicode")),
                (b"\x1b[>1;2c", None),
                (b"\x1b[>c", None),
            ] {
                let owner = client();
                assert_eq!(da2(&owner, report), (0, report.len()), "{report:?}");
                let expected = name.map_or_else(|| features_with(c""), |name| features_of(name));
                assert_eq!(owner.terminal_feature_mask(), expected, "{report:?}");
                assert_eq!(owner.borrow_terminal().flags & TTY_HAVEDA2, TTY_HAVEDA2);
            }
            let owner = client();
            assert_eq!(da2(&owner, b"\x1b[>85x"), (-1, 0));
            assert_eq!(da2(&owner, b"\x1b[>85x;0c"), (-1, 0));
            let mut report = b"\x1b[>".to_vec();
            report.resize(130, b'1');
            assert_eq!(da2(&owner, &report), (1, 0));
            report.push(b'1');
            assert_eq!(da2(&owner, &report), (-1, 0));
        }
    }

    fn xda(owner: &ClientRef, bytes: &[u8]) -> (i32, size_t) {
        let mut size = 99;
        (
            unsafe { tty_keys_extended_device_attributes(owner, bytes, &mut size) },
            size,
        )
    }

    #[test]
    fn extended_device_attributes() {
        unsafe {
            let owner = client();
            let sequence = b"\x1bP>|foot(1.16.2)\x1b\\";
            assert_prefixes_wait(
                |b, s| tty_keys_extended_device_attributes(&owner, b, s),
                sequence,
            );
            assert_header_rejects(
                |b, s| tty_keys_extended_device_attributes(&owner, b, s),
                sequence,
                4,
            );
            assert_eq!(xda(&owner, sequence), (0, 18));
            assert_eq!(owner.terminal_feature_mask(), features_of(c"foot"));
            assert_eq!(term_type(&owner).as_c_str(), c"foot(1.16.2)");
            assert_eq!(xda(&owner, sequence), (-1, 0));
            for (text, name) in [
                (b"iTerm2 3.5".as_slice(), Some(c"iTerm2")),
                (b"tmux 3.5", Some(c"tmux")),
                (b"XTerm(390)", Some(c"XTerm")),
                (b"mintty 3.7", Some(c"mintty")),
                (b"foot(1.0)", Some(c"foot")),
                (b"WezTerm 2024", Some(c"WezTerm")),
                (b"WezTermx", Some(c"WezTerm")),
                (b"ghostty 1.0", Some(c"ghostty")),
                (b"Rio 0.2", Some(c"Rio")),
                (b"iTerm2", None),
                (b"tmux", None),
                (b"kitty(0.36)", None),
                (b"", None),
                (b"tmux \0x", Some(c"tmux")),
                (b"tm\0ux 3.5", None),
            ] {
                let owner = client();
                let mut report = b"\x1bP>|".to_vec();
                report.extend_from_slice(text);
                report.extend_from_slice(b"\x1b\\");
                assert_eq!(xda(&owner, &report), (0, report.len()), "{text:?}");
                let expected = name.map_or_else(|| features_with(c""), |name| features_of(name));
                assert_eq!(owner.terminal_feature_mask(), expected, "{text:?}");
                let shown = text.split(|&b| b == 0).next().unwrap();
                assert_eq!(term_type(&owner).as_bytes(), shown);
                assert_eq!(owner.borrow_terminal().flags & TTY_HAVEXDA, TTY_HAVEXDA);
            }
            let owner = client();
            assert_eq!(xda(&owner, b"\x1bP>|a\x07b\x1b\\"), (0, 9));
            assert_eq!(term_type(&owner).as_bytes(), b"a\x07b");
            let owner = client();
            let mut report = b"\x1bP>|".to_vec();
            report.resize(130, b'a');
            assert_eq!(xda(&owner, &report), (1, 0));
            report.push(b'a');
            assert_eq!(xda(&owner, &report), (-1, 0));
            report.truncate(129);
            report.extend_from_slice(b"\x1b\\");
            assert_eq!(xda(&owner, &report), (0, 131));
            report.truncate(129);
            report.extend_from_slice(b"a\x1b\\");
            assert_eq!(xda(&owner, &report), (-1, 0));
        }
    }

    fn sync(owner: &ClientRef, bytes: &[u8]) -> (i32, size_t) {
        let mut size = 99;
        (unsafe { tty_keys_sync(owner, bytes, &mut size) }, size)
    }

    #[test]
    fn synchronized_output_reports() {
        unsafe {
            let owner = client();
            let sequence = b"\x1b[?2026;2$y";
            assert_prefixes_wait(|b, s| tty_keys_sync(&owner, b, s), sequence);
            assert_header_rejects(|b, s| tty_keys_sync(&owner, b, s), sequence, 8);
            assert_eq!(sync(&owner, b"\x1b[?2026;5$y"), (-1, 0));
            assert_eq!(sync(&owner, b"\x1b[?2026;2xy"), (-1, 0));
            assert_eq!(sync(&owner, b"\x1b[?2026;2$x"), (-1, 0));
            for status in b'0'..=b'4' {
                let owner = client();
                let report = [b"\x1b[?2026;".as_slice(), &[status], b"$yz"].concat();
                assert_eq!(sync(&owner, &report), (0, 11));
                let enabled = matches!(status, b'1'..=b'3');
                assert_eq!(
                    owner.terminal_feature_mask() == features_with(c"sync"),
                    enabled,
                    "{status}"
                );
                assert_eq!(owner.borrow_terminal().flags & TTY_HAVESYNC, TTY_HAVESYNC);
                assert_eq!(sync(&owner, &report), (-1, 0));
            }
        }
    }

    fn colours(flags: &mut i32, bytes: &[u8]) -> (i32, size_t, i32, i32) {
        let mut size = 99;
        let (mut fg, mut bg) = (-5, -5);
        let n = tty_keys_colours(flags, None, bytes, &mut size, &mut fg, &mut bg);
        (n, size, fg, bg)
    }

    #[test]
    fn colour_reports() {
        let red = colour_parse_x11_logged(c"rgb:ff/00/00").unwrap();
        let waiting = TTY_WAITFG | TTY_WAITBG;
        let sequence = b"\x1b]10;rgb:ff/00/00\x07";
        assert_prefixes_wait(
            |b, s| tty_keys_colours(&mut 0, None, b, s, &mut 0, &mut 0),
            sequence,
        );
        assert_header_rejects(
            |b, s| tty_keys_colours(&mut 0, None, b, s, &mut 0, &mut 0),
            sequence,
            5,
        );
        let mut flags = waiting;
        assert_eq!(colours(&mut flags, sequence), (0, 18, red, -5));
        assert_eq!(flags, TTY_WAITBG);
        let mut flags = waiting;
        assert_eq!(
            colours(&mut flags, b"\x1b]11;rgb:ff/00/00\x1b\\"),
            (0, 19, -5, red)
        );
        assert_eq!(flags, TTY_WAITFG);
        let mut flags = waiting;
        assert_eq!(
            colours(&mut flags, b"\x1b]11;rgb:ff/00/00\x1b\x07"),
            (0, 19, -5, red)
        );
        assert_eq!(colours(&mut flags, b"\x1b]10;bogus\x07"), (0, 11, -5, -5));
        assert_eq!(colours(&mut flags, b"\x1b]10;\x07"), (0, 6, -5, -5));
        assert_eq!(colours(&mut flags, b"\x1b]12;rgb:ff/00/00\x07").0, -1);
        assert_eq!(
            colours(&mut flags, b"\x1b]10;rgb:ff/00/00\0junk\x07"),
            (0, 23, red, -5)
        );
        assert_eq!(flags, 0);

        let mut report = b"\x1b]10;".to_vec();
        report.resize(131, b'a');
        assert_eq!(colours(&mut flags, &report).0, 1);
        report.push(b'a');
        assert_eq!(colours(&mut flags, &report).0, -1);
        report.truncate(131);
        report.push(b'\x07');
        assert_eq!(colours(&mut flags, &report), (0, 132, -5, -5));
    }

    fn palette(bytes: &[u8]) -> (i32, size_t, Option<(i32, i32)>) {
        let mut size = 99;
        let mut reply = None;
        let n = tty_keys_palette(bytes, &mut size, &mut reply);
        (n, size, reply.map(|reply| (reply.idx, reply.c)))
    }

    #[test]
    fn palette_reports() {
        let red = colour_parse_x11_logged(c"rgb:ff/00/00").unwrap();
        let sequence = b"\x1b]4;7;rgb:ff/00/00\x07";
        assert_prefixes_wait(|b, s| tty_keys_palette(b, s, &mut None), sequence);
        assert_header_rejects(|b, s| tty_keys_palette(b, s, &mut None), sequence, 4);
        assert_eq!(palette(sequence), (0, 19, Some((7, red))));
        assert_eq!(
            palette(b"\x1b]4;255;rgb:ff/00/00\x1b\\"),
            (0, 22, Some((255, red)))
        );
        assert_eq!(
            palette(b"\x1b]4;7;rgb:ff/00/00\x1b\x07"),
            (0, 20, Some((7, red)))
        );
        assert_eq!(
            palette(b"\x1b]4;;rgb:ff/00/00\x07"),
            (0, 18, Some((0, red)))
        );
        assert_eq!(
            palette(b"\x1b]4; +7;rgb:ff/00/00\x07"),
            (0, 21, Some((7, red)))
        );
        assert_eq!(
            palette(b"\x1b]4;-0;rgb:ff/00/00\x07"),
            (0, 20, Some((0, red)))
        );
        assert_eq!(palette(b"\x1b]4;256;rgb:ff/00/00\x07"), (-1, 21, None));
        assert_eq!(palette(b"\x1b]4;-1;rgb:ff/00/00\x07"), (-1, 20, None));
        assert_eq!(palette(b"\x1b]4;7 ;rgb:ff/00/00\x07"), (-1, 20, None));
        assert_eq!(palette(b"\x1b]4; ;rgb:ff/00/00\x07"), (-1, 19, None));
        assert_eq!(palette(b"\x1b]4;7\x07"), (-1, 6, None));
        assert_eq!(palette(b"\x1b]4;7\0;rgb:ff/00/00\x07"), (-1, 20, None));
        assert_eq!(
            palette(b"\x1b]4;4294967303;rgb:ff/00/00\x07"),
            (0, 28, Some((7, red)))
        );
        assert_eq!(
            palette(b"\x1b]4;9223372036854775807;rgb:ff/00/00\x07").0,
            -1
        );
        assert_eq!(
            palette(b"\x1b]4;99999999999999999999;rgb:ff/00/00\x07").0,
            -1
        );
        assert_eq!(
            palette(b"\x1b]4;-9223372036854775808;rgb:ff/00/00\x07").2,
            Some((0, red))
        );
        assert_eq!(
            palette(b"\x1b]4;-99999999999999999999;rgb:ff/00/00\x07").2,
            Some((0, red))
        );
        assert_eq!(palette(b"\x1b]4;7;bogus\x07"), (0, 12, None));
        assert_eq!(
            palette(b"\x1b]4;7;rgb:ff/00/00\0x\x07"),
            (0, 21, Some((7, red)))
        );
        assert_eq!(palette(b"\x1b]4;\x07"), (0, 5, None));

        let mut report = b"\x1b]4;7;".to_vec();
        report.resize(130, b'a');
        assert_eq!(palette(&report).0, 1);
        report.push(b'a');
        assert_eq!(palette(&report).0, -1);
        report.truncate(130);
        report.push(b'\x07');
        assert_eq!(palette(&report), (0, 131, None));
    }

    #[test]
    fn key_tables_and_utf8_keys() {
        unsafe {
            let owner = client();
            tty_keys_build(owner.borrow_terminal_mut());
            let terminal = owner.borrow_terminal();
            let next1 = |bytes: &[u8], expired| {
                let (mut key, mut size) = (0, 99);
                let n = tty_keys_next1(terminal, None, bytes, &mut key, &mut size, expired);
                (n, size, key)
            };
            assert_eq!(
                next1(b"\x1b[1;5P", 0),
                (0, 6, KEYC_F1 as key_code | KEYC_CTRL)
            );
            assert_eq!(next1(b"\x1bO[", 0), (0, 3, '\x1b' as key_code));
            assert_eq!(next1(b"\x1b[1;5", 0), (-1, 5, 0));
            let (n, size, key) = next1("é".as_bytes(), 0);
            assert_eq!((n, size), (0, 2));
            assert_eq!(next1(b"\xc3\xa9x", 0), (0, 2, key));
            assert_eq!(next1(b"\xc3", 0), (1, 2, 0));
            assert_eq!(next1(b"\xc3", 1), (-1, 2, 0));
            assert_eq!(next1(b"\xc3A", 0).0, -1);
            assert_eq!(next1(b"\x80", 0), (-1, 0, 0));
        }
    }
}
