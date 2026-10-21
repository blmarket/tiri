use crate::arguments::args;
use crate::format::bytes::cstr_append_into;
use crate::format::bytes::write_cstr;
use crate::format::format_create_defaults;
use crate::grid::grid_default_cell;
use crate::options::options_get_number;
use crate::options::OptionsScope;
use crate::screen::{screen_init, screen_resize};
use crate::screen_write::{
    screen_write_clearscreen, screen_write_cursormove, screen_write_putc, screen_write_puts,
    screen_write_start, screen_write_stop,
};
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::event::*;
use crate::shared::grid::*;
use crate::shared::key::*;
use crate::shared::mouse::mouse_event;
use crate::shared::screen::{screen, MODE_CURSOR};
use crate::shared::time::tm;
use crate::shared::window::{
    window_mode, window_mode_entry, window_mode_screen_payload, winlink, ModeDataRef,
};
use crate::style::style_apply_with_options;
use crate::window::Window as _;
use libc::{gmtime_r, localtime, strftime, strlen};

use crate::window_pane::WindowPane as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[repr(C)]
pub struct window_clock_mode_data {
    pub screen: screen,
    pub tim: time_t,
    pub timer: Option<Timer>,
}
pub static window_clock_mode: window_mode = {
    window_mode {
        name: c"clock-mode",
        default_format: None,
        flags: 0,
        init: Some(
            window_clock_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
                    Option<&cmd_find_state>,
                    Option<&args>,
                ) -> bool,
        ),
        free: None,
        resize: Some(
            window_clock_resize as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: None,
        style_changed: None,
        key: Some(
            window_clock_key
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    &ClientRef,
                    refbox::Weak<winlink>,
                    key_code,
                    Option<&mut mouse_event>,
                ) -> (),
        ),
        key_table: None,
        command: None,
        formats: None,
        get_screen: None,
        display_screen: Some(window_clock_get_screen),
    }
};
pub static mut window_clock_table: [[[::core::ffi::c_char; 5]; 5]; 14] = [
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
    [
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
        [
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            0 as ::core::ffi::c_int as ::core::ffi::c_char,
            1 as ::core::ffi::c_int as ::core::ffi::c_char,
        ],
    ],
];
unsafe fn window_clock_data(
    wme: &refbox::Weak<window_mode_entry>,
) -> ModeDataRef<'_, window_clock_mode_data> {
    ModeDataRef::new(wme.try_borrow().expect("live mode entry")).expect("clock mode payload")
}

unsafe fn window_clock_start_timer(mut wme: refbox::Weak<window_mode_entry>) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let delay = Duration::from_micros(1_000_000 - u64::from(now.subsec_micros()));
    let mode_observer = wme.clone();
    window_clock_data(&wme).borrow_mut().timer = Some(
        Timer::new(delay, move || unsafe {
            let live = match mode_observer.try_borrow() {
                Ok(_) => true,
                Err(refbox::BorrowError::Dropped) => false,
                Err(refbox::BorrowError::Borrowed) => panic!("clock mode already borrowed"),
            };
            if live {
                window_clock_timer_callback(mode_observer.clone());
            }
        })
        .expect("arm timer"),
    );
}
unsafe fn window_clock_timer_callback(wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let data_cell = window_clock_data(&wme);
    let mut data = data_cell.borrow_mut();
    let mut now: tm = unsafe { ::core::mem::zeroed() };
    let mut then: tm = unsafe { ::core::mem::zeroed() };
    let mut t: time_t = 0;
    drop(data.timer.take());
    t = crate::shared::time::unix_seconds(std::time::SystemTime::now());
    gmtime_r(&raw mut t, &raw mut now);
    gmtime_r(&raw mut data.tim, &raw mut then);
    let changed = now.tm_sec != then.tm_sec;
    if changed {
        data.tim = t;
    }
    drop(data);
    if changed {
        window_clock_draw_screen(wme.clone());
        mode_pane_owner.request_redraw(false);
    }
    window_clock_start_timer(wme.clone());
}
unsafe fn window_clock_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    _fs: Option<&cmd_find_state>,
    _args: Option<&args>,
) -> bool {
    let mode_pane_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let owner = Box::new(std::cell::RefCell::new(window_clock_mode_data {
        screen: screen::empty(),
        tim: crate::shared::time::unix_seconds(std::time::SystemTime::now()),
        timer: Default::default(),
    }));
    wme.try_borrow_mut().expect("live mode entry").boxed_data = Some(owner);
    window_clock_start_timer(wme.clone());
    let (pane_sx, pane_sy) = mode_pane_owner.screen_size(false);
    {
        let data_cell = window_clock_data(&wme);
        let mut data = data_cell.borrow_mut();
        screen_init(&mut data.screen, pane_sx, pane_sy, 0 as u_int);
        data.screen.mode &= !MODE_CURSOR;
    }
    window_clock_draw_screen(wme.clone());
    true
}
unsafe fn window_clock_get_screen(
    wme: refbox::Weak<window_mode_entry>,
    visit: &mut dyn FnMut(&mut screen),
) {
    if let Some(data) = wme
        .try_borrow()
        .expect("live mode entry")
        .boxed_data_cell::<window_clock_mode_data>()
    {
        window_mode_screen_payload(data, |data| visit(&mut data.screen));
    }
}

unsafe fn window_clock_resize(
    mut wme: refbox::Weak<window_mode_entry>,
    mut sx: u_int,
    mut sy: u_int,
) {
    screen_resize(
        &mut window_clock_data(&wme).borrow_mut().screen,
        sx,
        sy,
        0 as ::core::ffi::c_int,
    );
    window_clock_draw_screen(wme.clone());
}
unsafe fn window_clock_key(
    mut wme: refbox::Weak<window_mode_entry>,
    _client_owner: &ClientRef,
    _wl: refbox::Weak<winlink>,
    _key: key_code,
    _m: Option<&mut mouse_event>,
) {
    let mode_pane_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    mode_pane_owner.reset_mode();
}
unsafe fn window_clock_draw_screen(mut wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .try_borrow()
        .expect("live mode entry")
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let options_window = mode_pane_owner.window_observer();
    let mut colour: ::core::ffi::c_int = 0;
    let mut style: ::core::ffi::c_int = 0;
    let mut gc: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut tim: [u8; 64] = [0; 64];
    let mut ptr: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut t: time_t = 0;
    let mut tm: *mut tm = ::core::ptr::null_mut::<tm>();
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut idx: u_int = 0;
    let mut ft_owner = format_create_defaults(
        None,
        None,
        None,
        (refbox::Weak::new()).clone(),
        Some(&mode_pane_owner),
    );
    let ft = &mut *ft_owner;
    style_apply_with_options(
        &mut gc,
        c"clock-mode-colour",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    drop(ft_owner);
    colour = gc.fg;
    style = options_get_number(
        &options_window
            .upgrade()
            .expect("live clock window")
            .borrow_options_mut(),
        c"clock-mode-style",
    ) as ::core::ffi::c_int;
    let data_cell = window_clock_data(&wme);
    let mut data = data_cell.borrow_mut();
    let mut ctx = screen_write_start(&mut data.screen);
    t = crate::shared::time::unix_seconds(std::time::SystemTime::now());
    tm = localtime(&raw mut t);
    if style == 0 as ::core::ffi::c_int || style == 2 as ::core::ffi::c_int {
        if style == 2 as ::core::ffi::c_int {
            strftime(
                &raw mut tim as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as size_t,
                c"%l:%M:%S ".as_ptr(),
                localtime(&raw mut t),
            );
        } else {
            strftime(
                &raw mut tim as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as size_t,
                c"%l:%M ".as_ptr(),
                localtime(&raw mut t),
            );
        }
        if (*tm).tm_hour >= 12 as ::core::ffi::c_int {
            cstr_append_into(&mut tim, c"PM");
        } else {
            cstr_append_into(&mut tim, c"AM");
        }
    } else if style == 3 as ::core::ffi::c_int {
        strftime(
            &raw mut tim as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as size_t,
            c"%H:%M:%S".as_ptr(),
            tm,
        );
    } else {
        strftime(
            &raw mut tim as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 64]>() as size_t,
            c"%H:%M".as_ptr(),
            tm,
        );
    }
    screen_write_clearscreen(&mut ctx, 8 as u_int);
    if (ctx.screen.grid().sx as size_t)
        < (6 as size_t).wrapping_mul(strlen(&raw mut tim as *mut ::core::ffi::c_char))
        || ctx.screen.grid().sy < 6 as u_int
    {
        if ctx.screen.grid().sx as size_t >= strlen(&raw mut tim as *mut ::core::ffi::c_char)
            && ctx.screen.grid().sy != 0 as u_int
        {
            x = (ctx.screen.grid().sx.wrapping_div(2 as u_int) as size_t).wrapping_sub(
                strlen(&raw mut tim as *mut ::core::ffi::c_char).wrapping_div(2 as size_t),
            ) as u_int;
            y = ctx.screen.grid().sy.wrapping_div(2 as u_int);
            screen_write_cursormove(
                &mut ctx,
                x as ::core::ffi::c_int,
                y as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            gc = grid_default_cell;
            gc.flags = (gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
            gc.fg = colour;
            screen_write_puts(&mut ctx, &gc, |out| {
                write_cstr(
                    out,
                    std::ffi::CStr::from_bytes_until_nul(&tim).expect("clock text is terminated"),
                )
            });
        }
        screen_write_stop(ctx);
        return;
    }
    x = (ctx.screen.grid().sx.wrapping_div(2 as u_int) as size_t)
        .wrapping_sub((3 as size_t).wrapping_mul(strlen(&raw mut tim as *mut ::core::ffi::c_char)))
        as u_int;
    y = ctx
        .screen
        .grid()
        .sy
        .wrapping_div(2 as u_int)
        .wrapping_sub(3 as u_int);
    gc = grid_default_cell;
    gc.flags = (gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    gc.bg = colour;
    gc.fg = colour;
    let mut current_block_56: u64;
    ptr = &raw mut tim as *mut ::core::ffi::c_char;
    while *ptr as ::core::ffi::c_int != '\0' as i32 {
        if *ptr as ::core::ffi::c_int >= '0' as i32 && *ptr as ::core::ffi::c_int <= '9' as i32 {
            idx = (*ptr as ::core::ffi::c_int - '0' as i32) as u_int;
            current_block_56 = 2543120759711851213;
        } else if *ptr as ::core::ffi::c_int == ':' as i32 {
            idx = 10 as u_int;
            current_block_56 = 2543120759711851213;
        } else if *ptr as ::core::ffi::c_int == 'A' as i32 {
            idx = 11 as u_int;
            current_block_56 = 2543120759711851213;
        } else if *ptr as ::core::ffi::c_int == 'P' as i32 {
            idx = 12 as u_int;
            current_block_56 = 2543120759711851213;
        } else if *ptr as ::core::ffi::c_int == 'M' as i32 {
            idx = 13 as u_int;
            current_block_56 = 2543120759711851213;
        } else {
            x = x.wrapping_add(6 as u_int);
            current_block_56 = 11913429853522160501;
        }
        if current_block_56 == 2543120759711851213 {
            j = 0 as u_int;
            while j < 5 as u_int {
                i = 0 as u_int;
                while i < 5 as u_int {
                    screen_write_cursormove(
                        &mut ctx,
                        x.wrapping_add(i) as ::core::ffi::c_int,
                        y.wrapping_add(j) as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    if window_clock_table[idx as usize][j as usize][i as usize] != 0 {
                        screen_write_putc(&mut ctx, &gc, '#' as i32 as u_char);
                    }
                    i = i.wrapping_add(1);
                }
                j = j.wrapping_add(1);
            }
            x = x.wrapping_add(6 as u_int);
        }
        ptr = ptr.offset(1);
    }
    screen_write_stop(ctx);
}
