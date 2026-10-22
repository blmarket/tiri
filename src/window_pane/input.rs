//! Pane-owned input parsing and independent screen-parser components.
use super::{
    window_pane_get_bg, window_pane_get_fg, window_pane_get_fg_control_client,
    window_pane_get_theme, window_pane_update_used_data,
};
use crate::alerts::alerts_queue;
use crate::compat::strtonum::strtonum;
use crate::events::{events_fire, events_fire_pane};
use crate::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_session,
    event_payload_set_string, event_payload_set_target, event_payload_set_time,
    event_payload_set_uint, event_payload_set_window,
};
use crate::format::bytes::cstr_suffix;
use crate::format::bytes::format_bytes;
use crate::format::bytes::format_message_with;
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::grid::{
    grid_cells_look_equal, grid_default_cell, grid_get_cell, grid_get_line_mut, grid_set_tab,
};
use crate::hyperlinks::hyperlinks_put;
use crate::log::{
    fatalx, log_byte, log_bytes, log_cstr, log_cstr_n, log_cstr_width, log_debug, log_hex,
};
use crate::options::OptionsScope;
use crate::options::{options_get_number, options_set_number};
use crate::paste::{paste_add_owned, paste_buffer_data, paste_get_top};
use crate::reactor::BufferEvent;
use crate::reactor::{
    bufferevent_write, evbuffer_add, evbuffer_drain, evbuffer_get_length, evbuffer_new,
};
use crate::screen::{screen_clear_tabs, screen_has_tab, screen_set_tab};
use crate::screen::{
    screen_pop_title, screen_push_title, screen_set_cursor_colour, screen_set_cursor_style,
    screen_set_path, screen_set_progress_bar, screen_set_title,
};
use crate::screen_write::{
    screen_write_alignmenttest, screen_write_alternateoff, screen_write_alternateon,
    screen_write_backspace, screen_write_carriagereturn, screen_write_clearcharacter,
    screen_write_clearendofline, screen_write_clearendofscreen, screen_write_clearhistory,
    screen_write_clearline, screen_write_clearscreen, screen_write_clearstartofline,
    screen_write_clearstartofscreen, screen_write_collect_add, screen_write_collect_end,
    screen_write_cursordown, screen_write_cursorleft, screen_write_cursormove,
    screen_write_cursorright, screen_write_cursorup, screen_write_deletecharacter,
    screen_write_deleteline, screen_write_end_sync, screen_write_fullredraw,
    screen_write_insertcharacter, screen_write_insertline, screen_write_linefeed,
    screen_write_mode_clear, screen_write_mode_set, screen_write_rawstring, screen_write_reset,
    screen_write_reverseindex, screen_write_scrolldown, screen_write_scrollregion,
    screen_write_scrollup, screen_write_setselection, screen_write_start,
    screen_write_start_callback, screen_write_start_pane, screen_write_stop,
};
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::server_fn::{server_redraw_window_borders, server_status_window};
use crate::session::Session as _;
use crate::shared::client::ClientRef;
use crate::shared::input::{input_request_clipboard_data, input_request_palette_data};
use crate::shared::pane::pane_output_data as window_pane_get_new_data;
use crate::shared::window::WindowRef;
use crate::style::colour::{
    colour_force_rgb, colour_join_rgb, colour_palette_clear, colour_palette_get,
    colour_palette_set, colour_parse_x11_logged, colour_split_rgb,
};
use crate::text::base64;
use crate::text::utf8::{utf8_append, utf8_copy, utf8_isvalid, utf8_open, utf8_set};
use crate::tmux::{get_timer, getversion};
use crate::tty::tty_set_selection;
use crate::window::Window as _;
use crate::window::WindowPane;
use libc::strtol;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::time::Duration;
use tiri_buffer::SegmentedBuf;

use crate::shared::abi::*;
use crate::shared::client::CLIENT_UNATTACHEDFLAGS;
use crate::shared::colour::COLOUR_FLAG_256;
use crate::shared::command::cmd_find_state;
use crate::shared::display::*;
use crate::shared::event::*;
use crate::shared::grid::*;
use crate::shared::input::{
    input_ctx, input_end_type, input_param, input_request, input_state, input_transition,
};
use crate::shared::input::{input_handler_cb, input_request_type};
use crate::shared::input::{
    INPUT_BUF_DEFAULT_SIZE, INPUT_REQUEST_CLIPBOARD, INPUT_REQUEST_PALETTE, INPUT_REQUEST_QUEUE,
};
use crate::shared::limits::INT_MAX;
use crate::shared::pane::window_pane;
use crate::shared::pane::{
    PANE_ACTIVITY, PANE_CHANGED, PANE_CMDRUNNING, PANE_STYLECHANGED, PANE_THEMECHANGED,
    PANE_UNSEENCHANGES,
};
use crate::shared::screen::{
    screen, ALL_MOUSE_MODES, EXTENDED_KEY_MODES, MODE_BRACKETPASTE, MODE_CRLF, MODE_CURSOR,
    MODE_CURSOR_BLINKING, MODE_CURSOR_BLINKING_SET, MODE_CURSOR_VERY_VISIBLE, MODE_FOCUSON,
    MODE_INSERT, MODE_KCURSOR, MODE_KEYS_EXTENDED, MODE_KEYS_EXTENDED_2, MODE_KKEYPAD,
    MODE_MOUSE_ALL, MODE_MOUSE_BUTTON, MODE_MOUSE_SGR, MODE_MOUSE_STANDARD, MODE_MOUSE_UTF8,
    MODE_ORIGIN, MODE_SYNC, MODE_THEME_UPDATES, MODE_WRAP,
};
use crate::shared::screen_write::{screen_write_ctx, screen_write_init_ctx_cb};
use crate::shared::utf8::*;
use crate::shared::window::WINDOW_BELL;

pub const INPUT_END_BEL: input_end_type = 1;
pub const INPUT_END_ST: input_end_type = 0;

impl input_ctx {
    fn new() -> Self {
        Self {
            wp: std::rc::Weak::new(),
            event: Default::default(),
            palette: Default::default(),
            c: std::rc::Weak::new(),
            cell: Default::default(),
            old_cell: Default::default(),
            old_cx: 0,
            old_cy: 0,
            old_mode: 0,
            interm_buf: [0; 4],
            interm_len: 0,
            param_buf: [0; 64],
            param_len: 0,
            input_buf: vec![0; INPUT_BUF_START as usize],
            input_len: 0,
            input_end: INPUT_END_ST,
            param_list: std::array::from_fn(|_| input_param::Missing),
            param_list_len: 0,
            utf8data: Default::default(),
            utf8started: 0,
            ch: 0,
            last: Default::default(),
            state: &input_state_ground,
            flags: 0,
            requests: VecDeque::new(),
            request_timer: Default::default(),
            since_ground: evbuffer_new(),
            ground_timer: Default::default(),
        }
    }

    fn shrink_buffer(&mut self) {
        if self.input_buf.len() > INPUT_BUF_START as usize {
            self.input_buf.truncate(INPUT_BUF_START as usize);
            self.input_buf.shrink_to_fit();
        }
    }
}

fn input_clear_params(ictx: &mut input_ctx) {
    for param in &mut ictx.param_list[..ictx.param_list_len as usize] {
        *param = input_param::Missing;
    }
}

#[cfg(test)]
mod input_buffer_ownership_tests {
    use super::*;
    use crate::reactor::{bufferevent_new, StreamHandle};

    #[test]
    fn palette_source_observes_pane_or_popup_owner() {
        use crate::shared::input::InputPalette;
        let pane = window_pane::new();
        let pane_source = InputPalette::Pane(std::rc::Rc::downgrade(&pane));
        pane_source.with_mut(|palette| palette.fg = 17);
        assert_eq!(unsafe { (*pane.as_ptr()).palette.fg }, 17);
        drop(pane);
        assert!(pane_source.with_mut(|palette| palette.fg).is_none());

        let popup = refbox::RefBox::new(crate::shared::colour::colour_palette::default());
        let popup_source = InputPalette::Popup(popup.downgrade());
        popup_source.with_mut(|palette| palette.bg = 23);
        assert_eq!(popup.try_borrow_mut().unwrap().bg, 23);
        drop(popup);
        assert!(popup_source.with_mut(|palette| palette.bg).is_none());
    }

    #[test]
    fn input_reply_skips_a_freed_stream() {
        unsafe {
            let stream = bufferevent_new(-1, None, None, None).unwrap();
            let mut input = input_init(None, stream, Default::default(), None);
            assert!(input.event.is_alive());
            StreamHandle::from_ptr(stream).free();
            assert!(!input.event.is_alive());
            input_send_reply(&mut input, c"reply");
            drop(input);
        }
    }

    #[test]
    fn input_observes_pane_and_can_outlive_its_allocation() {
        unsafe {
            let pane = window_pane::new();
            let context = input_init(Some(&pane), std::ptr::null_mut(), Default::default(), None);
            assert_eq!(std::rc::Rc::strong_count(&pane), 1);
            assert!(context.wp.ptr_eq(&std::rc::Rc::downgrade(&pane)));
            drop(pane);
            assert!(context.wp.upgrade().is_none());
            drop(context);
        }
    }

    #[test]
    fn colon_parameter_survives_later_numeric_error_until_next_split() {
        let mut ictx_owner = Box::new(input_ctx::new());
        let ictx = &mut *ictx_owner;
        let invalid = b"38:5:196;invalid\0";
        (&mut ictx.param_buf)[..invalid.len()].copy_from_slice(invalid);
        ictx.param_len = invalid.len() - 1;
        assert_eq!(input_split(ictx), -1);
        assert_eq!(ictx.param_list_len, 1);
        assert_eq!(
            match &ictx.param_list[0] {
                input_param::String(value) => value.as_c_str(),
                _ => panic!("expected string parameter"),
            },
            c"38:5:196"
        );

        let next = b"48:5:25\0";
        (&mut ictx.param_buf)[..next.len()].copy_from_slice(next);
        ictx.param_len = next.len() - 1;
        assert_eq!(input_split(ictx), 0);
        assert_eq!(ictx.param_list_len, 1);
        assert_eq!(
            match &ictx.param_list[0] {
                input_param::String(value) => value.as_c_str(),
                _ => panic!("expected string parameter"),
            },
            c"48:5:25"
        );
        input_clear_params(ictx);
        drop(ictx_owner);
    }

    #[test]
    fn parser_buffer_grows_preserves_bytes_and_shrinks() {
        unsafe {
            let mut ictx_owner = Box::new(input_ctx::new());
            let ictx = &mut *ictx_owner;
            let mut s = screen::empty();
            s.grid = Some(crate::grid::grid_create(1, 1, 0));
            let mut ctx = screen_write_start(&mut s);
            assert_eq!(ictx.input_buf.len(), INPUT_BUF_START as usize);
            for ch in (0..96).map(|i| if i == 17 { 0 } else { b'a' + (i % 26) }) {
                ictx.ch = ch as i32;
                input_input(ictx, &mut ctx);
            }
            screen_write_stop(ctx);
            assert_eq!(ictx.input_len, 96);
            assert_eq!((&ictx.input_buf)[17], 0);
            assert_eq!((&ictx.input_buf)[96], 0);
            assert_eq!(ictx.input_buf.len(), 128);

            ictx.shrink_buffer();
            assert_eq!(ictx.input_buf.len(), INPUT_BUF_START as usize);
            drop(ictx_owner);
        }
    }
}

impl input_request {
    fn new() -> Box<Self> {
        Box::new(Self {
            c: std::rc::Weak::new(),
            ictx: ::core::ptr::null_mut(),
            type_0: INPUT_REQUEST_PALETTE,
            t: 0,
            end: INPUT_END_ST,
            idx: 0,
            data: None,
        })
    }
}

unsafe fn input_ctx_requests<'a>(ictx: *mut input_ctx) -> &'a mut VecDeque<Box<input_request>> {
    &mut (*ictx).requests
}

unsafe fn input_ctx_request_handles(ictx: *mut input_ctx) -> Vec<*mut input_request> {
    input_ctx_requests(ictx)
        .iter_mut()
        .map(|owner| &mut **owner as *mut input_request)
        .collect()
}

#[cfg(test)]
mod input_request_ownership_tests {
    use super::*;

    #[test]
    fn reply_without_pending_request_does_not_queue() {
        unsafe {
            let mut ictx_owner = Box::new(input_ctx::new());
            let ictx = &mut *ictx_owner;
            input_reply(ictx, 1, |out| out.write_all(b"\x1b[0n"));
            assert!(ictx.requests.is_empty());
            drop(ictx_owner);
        }
    }
}

pub const INPUT_ESC_ST: input_esc_type = 14;
pub const INPUT_ESC_SCSG1_OFF: input_esc_type = 12;
pub const INPUT_ESC_SCSG1_ON: input_esc_type = 13;
pub const INPUT_ESC_SCSG0_OFF: input_esc_type = 10;
pub const INPUT_ESC_SCSG0_ON: input_esc_type = 11;
pub const INPUT_ESC_DECALN: input_esc_type = 0;
pub const INPUT_ESC_DECRC: input_esc_type = 3;
pub const INPUT_ESC_DECSC: input_esc_type = 4;
pub const INPUT_ESC_DECKPNM: input_esc_type = 2;
pub const INPUT_ESC_DECKPAM: input_esc_type = 1;
pub const INPUT_ESC_RI: input_esc_type = 8;
pub const INPUT_ESC_HTS: input_esc_type = 5;
pub const INPUT_ESC_NEL: input_esc_type = 7;
pub const INPUT_ESC_IND: input_esc_type = 6;
pub const INPUT_ESC_RIS: input_esc_type = 9;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct input_table_entry {
    pub ch: ::core::ffi::c_int,
    pub interm: &'static ::std::ffi::CStr,
    pub type_0: ::core::ffi::c_int,
}
pub const INPUT_CSI_XDA: input_csi_type = 40;
pub const INPUT_CSI_DECSCUSR: input_csi_type = 11;
pub const INPUT_CSI_VPA: input_csi_type = 38;
pub const INPUT_CSI_TBC: input_csi_type = 37;
pub const INPUT_CSI_SD: input_csi_type = 31;
pub const INPUT_CSI_SU: input_csi_type = 36;
pub const INPUT_CSI_SM_GRAPHICS: input_csi_type = 34;
pub const INPUT_CSI_SM_PRIVATE: input_csi_type = 35;
pub const INPUT_CSI_SM: input_csi_type = 33;
pub const INPUT_CSI_SGR: input_csi_type = 32;
pub const INPUT_CSI_SCP: input_csi_type = 30;
pub const INPUT_CSI_RM_PRIVATE: input_csi_type = 29;
pub const INPUT_CSI_RM: input_csi_type = 28;
pub const INPUT_CSI_RCP: input_csi_type = 26;
pub const INPUT_CSI_REP: input_csi_type = 27;
pub const INPUT_CSI_IL: input_csi_type = 21;
pub const INPUT_CSI_ICH: input_csi_type = 20;
pub const INPUT_CSI_HPA: input_csi_type = 19;
pub const INPUT_CSI_EL: input_csi_type = 18;
pub const INPUT_CSI_ED: input_csi_type = 17;
pub const INPUT_CSI_DSR: input_csi_type = 14;
pub const INPUT_CSI_QUERY_PRIVATE: input_csi_type = 25;
pub const INPUT_CSI_QUERY: input_csi_type = 24;
pub const INPUT_CSI_DSR_PRIVATE: input_csi_type = 15;
pub const INPUT_CSI_DL: input_csi_type = 13;
pub const INPUT_CSI_DECSTBM: input_csi_type = 12;
pub const INPUT_CSI_DCH: input_csi_type = 10;
pub const INPUT_CSI_ECH: input_csi_type = 16;
pub const INPUT_CSI_DA_TWO: input_csi_type = 9;
pub const INPUT_CSI_DA: input_csi_type = 8;
pub const INPUT_CSI_CPL: input_csi_type = 2;
pub const INPUT_CSI_CNL: input_csi_type = 1;
pub const INPUT_CSI_CUU: input_csi_type = 7;
pub const INPUT_CSI_WINOPS: input_csi_type = 39;
pub const INPUT_CSI_MODOFF: input_csi_type = 22;
pub const INPUT_CSI_MODSET: input_csi_type = 23;
pub const INPUT_CSI_CUP: input_csi_type = 6;
pub const INPUT_CSI_CUF: input_csi_type = 5;
pub const INPUT_CSI_CUD: input_csi_type = 4;
pub const INPUT_CSI_CUB: input_csi_type = 3;
pub const INPUT_CSI_CBT: input_csi_type = 0;
pub type input_esc_type = ::core::ffi::c_uint;
pub type input_csi_type = ::core::ffi::c_uint;

pub const INPUT_REQUEST_TIMEOUT: ::core::ffi::c_int = 500 as ::core::ffi::c_int;
pub const INPUT_BUF_START: ::core::ffi::c_int = 32 as ::core::ffi::c_int;
pub const INPUT_DISCARD: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const INPUT_LAST: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
static input_esc_table: [input_table_entry; 15] = [
    input_table_entry {
        ch: '0' as i32,
        interm: c"(",
        type_0: INPUT_ESC_SCSG0_ON as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '0' as i32,
        interm: c")",
        type_0: INPUT_ESC_SCSG1_ON as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '7' as i32,
        interm: c"",
        type_0: INPUT_ESC_DECSC as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '8' as i32,
        interm: c"",
        type_0: INPUT_ESC_DECRC as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '8' as i32,
        interm: c"#",
        type_0: INPUT_ESC_DECALN as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '=' as i32,
        interm: c"",
        type_0: INPUT_ESC_DECKPAM as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '>' as i32,
        interm: c"",
        type_0: INPUT_ESC_DECKPNM as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'B' as i32,
        interm: c"(",
        type_0: INPUT_ESC_SCSG0_OFF as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'B' as i32,
        interm: c")",
        type_0: INPUT_ESC_SCSG1_OFF as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'D' as i32,
        interm: c"",
        type_0: INPUT_ESC_IND as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'E' as i32,
        interm: c"",
        type_0: INPUT_ESC_NEL as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'H' as i32,
        interm: c"",
        type_0: INPUT_ESC_HTS as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'M' as i32,
        interm: c"",
        type_0: INPUT_ESC_RI as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '\\' as i32,
        interm: c"",
        type_0: INPUT_ESC_ST as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'c' as i32,
        interm: c"",
        type_0: INPUT_ESC_RIS as ::core::ffi::c_int,
    },
];
static input_csi_table: [input_table_entry; 43] = [
    input_table_entry {
        ch: '@' as i32,
        interm: c"",
        type_0: INPUT_CSI_ICH as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'A' as i32,
        interm: c"",
        type_0: INPUT_CSI_CUU as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'B' as i32,
        interm: c"",
        type_0: INPUT_CSI_CUD as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'C' as i32,
        interm: c"",
        type_0: INPUT_CSI_CUF as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'D' as i32,
        interm: c"",
        type_0: INPUT_CSI_CUB as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'E' as i32,
        interm: c"",
        type_0: INPUT_CSI_CNL as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'F' as i32,
        interm: c"",
        type_0: INPUT_CSI_CPL as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'G' as i32,
        interm: c"",
        type_0: INPUT_CSI_HPA as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'H' as i32,
        interm: c"",
        type_0: INPUT_CSI_CUP as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'J' as i32,
        interm: c"",
        type_0: INPUT_CSI_ED as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'K' as i32,
        interm: c"",
        type_0: INPUT_CSI_EL as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'L' as i32,
        interm: c"",
        type_0: INPUT_CSI_IL as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'M' as i32,
        interm: c"",
        type_0: INPUT_CSI_DL as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'P' as i32,
        interm: c"",
        type_0: INPUT_CSI_DCH as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'S' as i32,
        interm: c"",
        type_0: INPUT_CSI_SU as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'S' as i32,
        interm: c"?",
        type_0: INPUT_CSI_SM_GRAPHICS as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'T' as i32,
        interm: c"",
        type_0: INPUT_CSI_SD as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'X' as i32,
        interm: c"",
        type_0: INPUT_CSI_ECH as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'Z' as i32,
        interm: c"",
        type_0: INPUT_CSI_CBT as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: '`' as i32,
        interm: c"",
        type_0: INPUT_CSI_HPA as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'b' as i32,
        interm: c"",
        type_0: INPUT_CSI_REP as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'c' as i32,
        interm: c"",
        type_0: INPUT_CSI_DA as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'c' as i32,
        interm: c">",
        type_0: INPUT_CSI_DA_TWO as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'd' as i32,
        interm: c"",
        type_0: INPUT_CSI_VPA as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'f' as i32,
        interm: c"",
        type_0: INPUT_CSI_CUP as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'g' as i32,
        interm: c"",
        type_0: INPUT_CSI_TBC as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'h' as i32,
        interm: c"",
        type_0: INPUT_CSI_SM as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'h' as i32,
        interm: c"?",
        type_0: INPUT_CSI_SM_PRIVATE as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'l' as i32,
        interm: c"",
        type_0: INPUT_CSI_RM as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'l' as i32,
        interm: c"?",
        type_0: INPUT_CSI_RM_PRIVATE as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'm' as i32,
        interm: c"",
        type_0: INPUT_CSI_SGR as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'm' as i32,
        interm: c">",
        type_0: INPUT_CSI_MODSET as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'n' as i32,
        interm: c"",
        type_0: INPUT_CSI_DSR as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'n' as i32,
        interm: c">",
        type_0: INPUT_CSI_MODOFF as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'n' as i32,
        interm: c"?",
        type_0: INPUT_CSI_DSR_PRIVATE as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'p' as i32,
        interm: c"$",
        type_0: INPUT_CSI_QUERY as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'p' as i32,
        interm: c"?$",
        type_0: INPUT_CSI_QUERY_PRIVATE as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'q' as i32,
        interm: c" ",
        type_0: INPUT_CSI_DECSCUSR as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'q' as i32,
        interm: c">",
        type_0: INPUT_CSI_XDA as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'r' as i32,
        interm: c"",
        type_0: INPUT_CSI_DECSTBM as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 's' as i32,
        interm: c"",
        type_0: INPUT_CSI_SCP as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 't' as i32,
        interm: c"",
        type_0: INPUT_CSI_WINOPS as ::core::ffi::c_int,
    },
    input_table_entry {
        ch: 'u' as i32,
        interm: c"",
        type_0: INPUT_CSI_RCP as ::core::ffi::c_int,
    },
];
fn input_table_find<'a>(
    table: &'a [input_table_entry],
    ictx: &input_ctx,
) -> Option<&'a input_table_entry> {
    let interm =
        CStr::from_bytes_until_nul(&ictx.interm_buf).expect("intermediate buffer is terminated");
    table
        .binary_search_by(|entry| {
            entry
                .ch
                .cmp(&ictx.ch)
                .then_with(|| entry.interm.to_bytes().cmp(interm.to_bytes()))
        })
        .ok()
        .map(|index| &table[index])
}
static input_state_ground: input_state = {
    input_state {
        name: c"ground",
        enter: Some(input_ground),
        exit: None,
        transitions: &input_state_ground_table,
    }
};
static input_state_esc_enter: input_state = {
    input_state {
        name: c"esc_enter",
        enter: Some(input_clear),
        exit: None,
        transitions: &input_state_esc_enter_table,
    }
};
static input_state_esc_intermediate: input_state = {
    input_state {
        name: c"esc_intermediate",
        enter: None,
        exit: None,
        transitions: &input_state_esc_intermediate_table,
    }
};
static input_state_csi_enter: input_state = {
    input_state {
        name: c"csi_enter",
        enter: Some(input_clear),
        exit: None,
        transitions: &input_state_csi_enter_table,
    }
};
static input_state_csi_parameter: input_state = {
    input_state {
        name: c"csi_parameter",
        enter: None,
        exit: None,
        transitions: &input_state_csi_parameter_table,
    }
};
static input_state_csi_intermediate: input_state = {
    input_state {
        name: c"csi_intermediate",
        enter: None,
        exit: None,
        transitions: &input_state_csi_intermediate_table,
    }
};
static input_state_csi_ignore: input_state = {
    input_state {
        name: c"csi_ignore",
        enter: None,
        exit: None,
        transitions: &input_state_csi_ignore_table,
    }
};
static input_state_dcs_enter: input_state = {
    input_state {
        name: c"dcs_enter",
        enter: Some(input_enter_dcs),
        exit: None,
        transitions: &input_state_dcs_enter_table,
    }
};
static input_state_dcs_parameter: input_state = {
    input_state {
        name: c"dcs_parameter",
        enter: None,
        exit: None,
        transitions: &input_state_dcs_parameter_table,
    }
};
static input_state_dcs_intermediate: input_state = {
    input_state {
        name: c"dcs_intermediate",
        enter: None,
        exit: None,
        transitions: &input_state_dcs_intermediate_table,
    }
};
static input_state_dcs_handler: input_state = {
    input_state {
        name: c"dcs_handler",
        enter: None,
        exit: None,
        transitions: &input_state_dcs_handler_table,
    }
};
static input_state_dcs_escape: input_state = {
    input_state {
        name: c"dcs_escape",
        enter: None,
        exit: None,
        transitions: &input_state_dcs_escape_table,
    }
};
static input_state_dcs_ignore: input_state = {
    input_state {
        name: c"dcs_ignore",
        enter: None,
        exit: None,
        transitions: &input_state_dcs_ignore_table,
    }
};
static input_state_osc_string: input_state = {
    input_state {
        name: c"osc_string",
        enter: Some(input_enter_osc),
        exit: Some(input_exit_osc),
        transitions: &input_state_osc_string_table,
    }
};
static input_state_apc_string: input_state = {
    input_state {
        name: c"apc_string",
        enter: Some(input_enter_apc),
        exit: Some(input_exit_apc),
        transitions: &input_state_apc_string_table,
    }
};
static input_state_rename_string: input_state = {
    input_state {
        name: c"rename_string",
        enter: Some(input_enter_rename),
        exit: Some(input_exit_rename),
        transitions: &input_state_rename_string_table,
    }
};
static input_state_consume_st: input_state = {
    input_state {
        name: c"consume_st",
        enter: Some(input_enter_rename),
        exit: None,
        transitions: &input_state_consume_st_table,
    }
};
static input_state_ground_table: [input_transition; 10] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_print),
            state: None,
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0x7f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x80 as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: Some(input_top_bit_set),
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_esc_enter_table: [input_transition; 23] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_esc_intermediate),
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x4f as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x50 as ::core::ffi::c_int,
            last: 0x50 as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_dcs_enter),
        },
        input_transition {
            first: 0x51 as ::core::ffi::c_int,
            last: 0x57 as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x58 as ::core::ffi::c_int,
            last: 0x58 as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_consume_st),
        },
        input_transition {
            first: 0x59 as ::core::ffi::c_int,
            last: 0x59 as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x5a as ::core::ffi::c_int,
            last: 0x5a as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x5b as ::core::ffi::c_int,
            last: 0x5b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_csi_enter),
        },
        input_transition {
            first: 0x5c as ::core::ffi::c_int,
            last: 0x5c as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x5d as ::core::ffi::c_int,
            last: 0x5d as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_osc_string),
        },
        input_transition {
            first: 0x5e as ::core::ffi::c_int,
            last: 0x5e as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_consume_st),
        },
        input_transition {
            first: 0x5f as ::core::ffi::c_int,
            last: 0x5f as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_apc_string),
        },
        input_transition {
            first: 0x60 as ::core::ffi::c_int,
            last: 0x6a as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x6b as ::core::ffi::c_int,
            last: 0x6b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_rename_string),
        },
        input_transition {
            first: 0x6c as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_esc_intermediate_table: [input_transition; 10] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: None,
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_esc_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_csi_enter_table: [input_transition; 14] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_csi_intermediate),
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x39 as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: Some(&input_state_csi_parameter),
        },
        input_transition {
            first: 0x3a as ::core::ffi::c_int,
            last: 0x3a as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: Some(&input_state_csi_parameter),
        },
        input_transition {
            first: 0x3b as ::core::ffi::c_int,
            last: 0x3b as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: Some(&input_state_csi_parameter),
        },
        input_transition {
            first: 0x3c as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_csi_parameter),
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_csi_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_csi_parameter_table: [input_transition; 14] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_csi_intermediate),
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x39 as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: None,
        },
        input_transition {
            first: 0x3a as ::core::ffi::c_int,
            last: 0x3a as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: None,
        },
        input_transition {
            first: 0x3b as ::core::ffi::c_int,
            last: 0x3b as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: None,
        },
        input_transition {
            first: 0x3c as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_csi_ignore),
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_csi_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_csi_intermediate_table: [input_transition; 11] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: None,
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_csi_ignore),
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_csi_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_csi_ignore_table: [input_transition; 10] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_dcs_enter_table: [input_transition; 14] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_dcs_intermediate),
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x39 as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: Some(&input_state_dcs_parameter),
        },
        input_transition {
            first: 0x3a as ::core::ffi::c_int,
            last: 0x3a as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_dcs_ignore),
        },
        input_transition {
            first: 0x3b as ::core::ffi::c_int,
            last: 0x3b as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: Some(&input_state_dcs_parameter),
        },
        input_transition {
            first: 0x3c as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_dcs_parameter),
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_input),
            state: Some(&input_state_dcs_handler),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_dcs_parameter_table: [input_transition; 14] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: Some(&input_state_dcs_intermediate),
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x39 as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: None,
        },
        input_transition {
            first: 0x3a as ::core::ffi::c_int,
            last: 0x3a as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_dcs_ignore),
        },
        input_transition {
            first: 0x3b as ::core::ffi::c_int,
            last: 0x3b as ::core::ffi::c_int,
            handler: Some(input_parameter),
            state: None,
        },
        input_transition {
            first: 0x3c as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_dcs_ignore),
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_input),
            state: Some(&input_state_dcs_handler),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_dcs_intermediate_table: [input_transition; 11] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0x2f as ::core::ffi::c_int,
            handler: Some(input_intermediate),
            state: None,
        },
        input_transition {
            first: 0x30 as ::core::ffi::c_int,
            last: 0x3f as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_dcs_ignore),
        },
        input_transition {
            first: 0x40 as ::core::ffi::c_int,
            last: 0x7e as ::core::ffi::c_int,
            handler: Some(input_input),
            state: Some(&input_state_dcs_handler),
        },
        input_transition {
            first: 0x7f as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_dcs_handler_table: [input_transition; 4] = {
    [
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_input),
            state: None,
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_dcs_escape),
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: Some(input_input),
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_dcs_escape_table: [input_transition; 4] = {
    [
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x5b as ::core::ffi::c_int,
            handler: Some(input_input),
            state: Some(&input_state_dcs_handler),
        },
        input_transition {
            first: 0x5c as ::core::ffi::c_int,
            last: 0x5c as ::core::ffi::c_int,
            handler: Some(input_dcs_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x5d as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: Some(input_input),
            state: Some(&input_state_dcs_handler),
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_dcs_ignore_table: [input_transition; 8] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_osc_string_table: [input_transition; 10] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x6 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x7 as ::core::ffi::c_int,
            last: 0x7 as ::core::ffi::c_int,
            handler: Some(input_end_bel),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x8 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: Some(input_input),
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_apc_string_table: [input_transition; 8] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: Some(input_input),
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_rename_string_table: [input_transition; 8] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: Some(input_input),
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static input_state_consume_st_table: [input_transition; 8] = {
    [
        input_transition {
            first: 0x18 as ::core::ffi::c_int,
            last: 0x18 as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1a as ::core::ffi::c_int,
            last: 0x1a as ::core::ffi::c_int,
            handler: Some(input_c0_dispatch),
            state: Some(&input_state_ground),
        },
        input_transition {
            first: 0x1b as ::core::ffi::c_int,
            last: 0x1b as ::core::ffi::c_int,
            handler: None,
            state: Some(&input_state_esc_enter),
        },
        input_transition {
            first: 0 as ::core::ffi::c_int,
            last: 0x17 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x19 as ::core::ffi::c_int,
            last: 0x19 as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x1c as ::core::ffi::c_int,
            last: 0x1f as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: 0x20 as ::core::ffi::c_int,
            last: 0xff as ::core::ffi::c_int,
            handler: None,
            state: None,
        },
        input_transition {
            first: -(1 as ::core::ffi::c_int),
            last: -(1 as ::core::ffi::c_int),
            handler: None,
            state: None,
        },
    ]
};
static mut input_buffer_size: size_t = INPUT_BUF_DEFAULT_SIZE as size_t;
unsafe fn input_stop_utf8(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    static mut rc: utf8_data = {
        utf8_data {
            data: *b"\xEF\xBF\xBD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            have: 3 as u_char,
            size: 3 as u_char,
            width: 1 as u_char,
        }
    };
    if ictx.utf8started != 0 {
        ictx.cell.cell.data = utf8_copy(&rc);
        screen_write_collect_add(sctx, &ictx.cell.cell);
    }
    ictx.utf8started = 0 as ::core::ffi::c_int;
}
unsafe fn input_fire_pane_title_changed(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    title: &CStr,
) {
    let mut wp = wp_owner.as_ptr();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_pane(wp_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    event_payload_set_window(
        &mut ep,
        c"window",
        WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_string(&mut ep, c"new_title", |out| {
        write_cstr(out, nullable_cstr(title.as_ptr()))
    });
    events_fire(c"pane-title-changed", ep);
}
unsafe fn input_ground_timer_callback(ictx: &mut input_ctx) {
    log_debug(format_args!(
        "{}: {} expired",
        "input_ground_timer_callback",
        crate::log::log_bytes(ictx.state.name.to_bytes())
    ));
    input_reset(ictx, 0 as ::core::ffi::c_int);
}
unsafe fn input_start_ground_timer(ictx: &mut input_ctx) {
    let tv = Duration::from_secs(5);
    drop(ictx.ground_timer.take());
    let ictx_ptr: *mut input_ctx = ictx;
    ictx.ground_timer = Some(
        Timer::new(tv, move || unsafe {
            input_ground_timer_callback(&mut *ictx_ptr)
        })
        .expect("arm timer"),
    );
}
fn input_reset_cell(ictx: &mut input_ctx) {
    ictx.cell.cell = grid_default_cell;
    ictx.cell.set = 0 as ::core::ffi::c_int;
    ictx.cell.g1set = 0 as ::core::ffi::c_int;
    ictx.cell.g0set = ictx.cell.g1set;
    ictx.old_cell = ictx.cell;
    ictx.old_cx = 0 as u_int;
    ictx.old_cy = 0 as u_int;
}
fn input_save_state(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    ictx.old_cell = ictx.cell;
    ictx.old_cx = sctx.screen.cx;
    ictx.old_cy = sctx.screen.cy;
    ictx.old_mode = sctx.screen.mode;
}
unsafe fn input_restore_state(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    ictx.cell = ictx.old_cell;
    if ictx.old_mode & MODE_ORIGIN != 0 {
        screen_write_mode_set(sctx, MODE_ORIGIN);
    } else {
        screen_write_mode_clear(sctx, MODE_ORIGIN);
    }
    screen_write_cursormove(
        sctx,
        ictx.old_cx as ::core::ffi::c_int,
        ictx.old_cy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
}
pub unsafe fn input_init(
    wp: Option<&std::rc::Rc<std::cell::RefCell<window_pane>>>,
    mut bev: *mut bufferevent,
    palette: crate::shared::input::InputPalette,
    c: Option<&ClientRef>,
) -> Box<input_ctx> {
    let mut owner = Box::new(input_ctx::new());
    let ictx = &mut *owner;
    ictx.wp = wp.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    ictx.event = crate::reactor::StreamHandle::from_ptr(bev);
    ictx.palette = palette;
    ictx.c = c.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    input_reset(ictx, 0 as ::core::ffi::c_int);
    owner
}

impl Drop for input_ctx {
    fn drop(&mut self) {
        unsafe {
            let ictx = self as *mut input_ctx;
            loop {
                let ir = input_ctx_requests(ictx)
                    .front_mut()
                    .map(|owner| &mut **owner as *mut input_request);
                let Some(ir) = ir else {
                    break;
                };
                input_free_request(ir);
            }
            if let Some(pane) = self.wp.upgrade() {
                pane.stop_sync(None);
            }
        }
    }
}
pub unsafe fn input_reset(ictx: &mut input_ctx, mut clear: ::core::ffi::c_int) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    input_reset_cell(ictx);
    if clear != 0 && !wp.is_null() {
        let mut ctx = if (*wp).modes.is_empty() {
            screen_write_start_pane(
                input_pane_owner.as_ref().expect("live input pane"),
                &mut (*wp).base,
            )
        } else {
            screen_write_start(&mut (*wp).base)
        };
        screen_write_reset(&mut ctx);
        screen_write_stop(ctx);
    }
    input_clear(ictx);
    ictx.state = &input_state_ground;
    ictx.flags = 0 as ::core::ffi::c_int;
}
pub fn input_pending(ictx: &mut input_ctx) -> &mut SegmentedBuf {
    &mut ictx.since_ground
}
unsafe fn input_set_state(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
    state: &'static input_state,
) {
    if ictx.state.exit.is_some() {
        ictx.state.exit.expect("non-null function pointer")(ictx, sctx);
    }
    ictx.state = state;
    if ictx.state.enter.is_some() {
        ictx.state.enter.expect("non-null function pointer")(ictx);
    }
}
unsafe fn input_parse(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, buf: &[u8]) {
    let mut state: Option<&'static input_state> = None;
    let mut transition: Option<&'static input_transition> = None;
    for &byte in buf {
        ictx.ch = byte as ::core::ffi::c_int;
        if !state.is_some_and(|state| std::ptr::eq(ictx.state, state))
            || !transition.is_some_and(|itr| ictx.ch >= itr.first && ictx.ch <= itr.last)
        {
            transition = ictx
                .state
                .transitions
                .iter()
                .take_while(|itr| itr.first != -1 && itr.last != -1)
                .find(|itr| ictx.ch >= itr.first && ictx.ch <= itr.last);
        }
        let Some(itr) = transition else {
            fatalx(|out| out.write_all(b"no transition from state"));
        };
        state = Some(ictx.state);
        if !itr
            .handler
            .is_some_and(|handler| std::ptr::fn_addr_eq(handler, input_print as input_handler_cb))
        {
            screen_write_collect_end(sctx);
        }
        if itr.handler.is_some()
            && itr.handler.expect("non-null function pointer")(ictx, sctx)
                != 0 as ::core::ffi::c_int
        {
            continue;
        }
        if let Some(state) = itr.state {
            input_set_state(ictx, sctx, state);
        }
        if !std::ptr::eq(ictx.state, &input_state_ground) {
            evbuffer_add(&mut ictx.since_ground, &[ictx.ch as u8]);
        }
    }
}
pub(super) unsafe fn input_parse_pane(wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>) {
    let mut wp = wp_owner.as_ptr();
    let (stream, base, offset) = {
        let pane = &*wp_owner.as_ptr();
        (pane.event.clone(), pane.base_offset, pane.offset)
    };
    let data = stream
        .with_ptr(|event| unsafe {
            window_pane_get_new_data(&mut (*event).input, base, &offset).to_vec()
        })
        .unwrap_or_default();
    if !data.is_empty() {
        (*wp).last_output_time = crate::shared::time::unix_seconds(std::time::SystemTime::now());
    }
    input_parse_buffer(wp_owner, &data);
    window_pane_update_used_data(wp_owner, |wp| &mut wp.offset, data.len());
}
pub(super) unsafe fn input_parse_buffer(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    buf: &[u8],
) {
    let mut wp = wp_owner.as_ptr();
    if buf.is_empty() {
        return;
    }
    let ictx = (*wp).ictx.as_deref_mut().expect("pane input context");
    (*wp).output_generation = (*wp).output_generation.wrapping_add(1);
    WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")).update_activity();
    if !(*wp).flags & PANE_ACTIVITY != 0 {
        (*wp).flags |= PANE_ACTIVITY;
        events_fire_pane(c"pane-activity", std::rc::Rc::clone(wp_owner));
    }
    (*wp).flags |= PANE_CHANGED;
    if !(*wp).modes.is_empty() {
        (*wp).flags |= PANE_UNSEENCHANGES;
    }
    // The context borrows `base` until stop. While it does, nothing else may
    // read or write `base`: parser callbacks reach the pane through `ictx.wp`
    // only for its other fields, and sync state goes through the context
    // (`start_sync`, `screen_write_end_sync`). Known exception: entering or
    // leaving the alternate screen refits the window, which can resize this
    // pane's `base` in place through the pane.
    let mut ctx = if (*wp).modes.is_empty() {
        screen_write_start_pane(wp_owner, &mut (*wp).base)
    } else {
        screen_write_start(&mut (*wp).base)
    };
    log_debug(format_args!(
        "{}: %{} {}, {} bytes: {}",
        "input_parse_buffer",
        { (*wp).id },
        crate::log::log_bytes(ictx.state.name.to_bytes()),
        buf.len(),
        log_cstr_n(buf)
    ));
    input_parse(ictx, &mut ctx, buf);
    screen_write_stop(ctx);
}
pub unsafe fn input_parse_screen(
    ictx: &mut input_ctx,
    s: &mut screen,
    cb: screen_write_init_ctx_cb,
    buf: &[u8],
) {
    if buf.is_empty() {
        return;
    }
    let mut ctx = screen_write_start_callback(s, cb);
    input_parse(ictx, &mut ctx, buf);
    screen_write_stop(ctx);
}
fn input_split_fields(buf: &mut [u8], delim: u8) -> impl Iterator<Item = &CStr> {
    let len = CStr::from_bytes_until_nul(buf)
        .expect("field buffer is terminated")
        .count_bytes();
    buf[..len]
        .iter_mut()
        .filter(|byte| **byte == delim)
        .for_each(|byte| *byte = 0);
    buf[..=len]
        .split_inclusive(|&byte| byte == 0)
        .map(|field| CStr::from_bytes_with_nul(field).expect("field is terminated"))
}
fn input_split(ictx: &mut input_ctx) -> ::core::ffi::c_int {
    input_clear_params(ictx);
    ictx.param_list_len = 0 as u_int;
    if ictx.param_len == 0 as size_t {
        return 0 as ::core::ffi::c_int;
    }
    let mut params = ictx.param_buf;
    for out in input_split_fields(&mut params, b';') {
        let ip = &mut ictx.param_list[ictx.param_list_len as usize];
        if out.is_empty() {
            *ip = input_param::Missing;
        } else if out.to_bytes().contains(&b':') {
            *ip = input_param::String(out.to_owned());
        } else {
            let Ok(number) = strtonum(out, 0, INT_MAX as ::core::ffi::c_longlong) else {
                return -(1 as ::core::ffi::c_int);
            };
            *ip = input_param::Number(number as ::core::ffi::c_int);
        }
        ictx.param_list_len = ictx.param_list_len.wrapping_add(1);
        if ictx.param_list_len as usize == ictx.param_list.len() {
            return -(1 as ::core::ffi::c_int);
        }
    }
    for (i, ip) in ictx.param_list[..ictx.param_list_len as usize]
        .iter()
        .enumerate()
    {
        match ip {
            input_param::Missing => log_debug(format_args!("parameter {}: missing", i)),
            input_param::String(value) => {
                log_debug(format_args!("parameter {}: string {}", i, log_cstr(value)));
            }
            input_param::Number(value) => {
                log_debug(format_args!("parameter {}: number {}", i, value))
            }
        }
    }
    0 as ::core::ffi::c_int
}
fn input_get(
    ictx: &input_ctx,
    validx: u_int,
    minval: ::core::ffi::c_int,
    defval: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    if validx >= ictx.param_list_len {
        return defval;
    }
    let retval = match &ictx.param_list[validx as usize] {
        input_param::Missing => return defval,
        input_param::String(_) => return -1,
        input_param::Number(value) => *value,
    };
    if retval < minval {
        return minval;
    }
    retval
}
unsafe fn input_send_reply(ictx: &mut input_ctx, reply: &CStr) {
    if ictx.event.is_alive() {
        log_debug(format_args!("{}: {}", "input_send_reply", log_cstr(reply)));
        let _ = ictx
            .event
            .with_ptr(|event| unsafe { bufferevent_write(event, reply.to_bytes()) });
    }
}
unsafe fn input_reply(
    ictx: &mut input_ctx,
    mut add: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut ir: *mut input_request = ::core::ptr::null_mut::<input_request>();
    let reply = format_message_with(write);
    if add != 0 && !ictx.requests.is_empty() {
        ir = input_make_request(ictx, INPUT_REQUEST_QUEUE);
        (*ir).data = Some(reply);
    } else {
        input_send_reply(ictx, &reply);
    };
}
unsafe fn input_clear(ictx: &mut input_ctx) {
    drop(ictx.ground_timer.take());
    *(&raw mut ictx.interm_buf as *mut u_char) = '\0' as i32 as u_char;
    ictx.interm_len = 0 as size_t;
    *(&raw mut ictx.param_buf as *mut u_char) = '\0' as i32 as u_char;
    ictx.param_len = 0 as size_t;
    (&mut ictx.input_buf)[0] = 0;
    ictx.input_len = 0 as size_t;
    ictx.input_end = INPUT_END_ST;
    ictx.flags &= !INPUT_DISCARD;
}
fn input_ground(ictx: &mut input_ctx) {
    drop(ictx.ground_timer.take());
    let len = evbuffer_get_length(&ictx.since_ground);
    evbuffer_drain(&mut ictx.since_ground, len);
    ictx.shrink_buffer();
}
unsafe fn input_print(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    let mut set: ::core::ffi::c_int = 0;
    input_stop_utf8(ictx, sctx);
    set = if ictx.cell.set == 0 as ::core::ffi::c_int {
        ictx.cell.g0set
    } else {
        ictx.cell.g1set
    };
    if set == 1 as ::core::ffi::c_int {
        ictx.cell.cell.attr =
            (ictx.cell.cell.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
    } else {
        ictx.cell.cell.attr =
            (ictx.cell.cell.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
    }
    utf8_set(&mut ictx.cell.cell.data, ictx.ch as u_char);
    screen_write_collect_add(sctx, &ictx.cell.cell);
    ictx.last = utf8_copy(&ictx.cell.cell.data);
    ictx.flags |= INPUT_LAST;
    ictx.cell.cell.attr =
        (ictx.cell.cell.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
    0 as ::core::ffi::c_int
}
fn input_intermediate(ictx: &mut input_ctx, _sctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    if ictx.interm_len == (::core::mem::size_of::<[u_char; 4]>() as usize).wrapping_sub(1_usize) {
        ictx.flags |= INPUT_DISCARD;
    } else {
        let fresh15 = ictx.interm_len;
        ictx.interm_len = ictx.interm_len.wrapping_add(1);
        ictx.interm_buf[fresh15] = ictx.ch as u_char;
        ictx.interm_buf[ictx.interm_len] = '\0' as i32 as u_char;
    }
    0 as ::core::ffi::c_int
}
fn input_parameter(ictx: &mut input_ctx, _sctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    if ictx.param_len == (::core::mem::size_of::<[u_char; 64]>() as usize).wrapping_sub(1_usize) {
        ictx.flags |= INPUT_DISCARD;
    } else {
        let fresh14 = ictx.param_len;
        ictx.param_len = ictx.param_len.wrapping_add(1);
        ictx.param_buf[fresh14] = ictx.ch as u_char;
        ictx.param_buf[ictx.param_len] = '\0' as i32 as u_char;
    }
    0 as ::core::ffi::c_int
}
unsafe fn input_input(ictx: &mut input_ctx, _sctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    let mut available: size_t = 0;
    available = ictx.input_buf.len();
    while ictx.input_len.wrapping_add(1 as size_t) >= available {
        available = available.wrapping_mul(2 as size_t);
        if available > input_buffer_size {
            ictx.flags |= INPUT_DISCARD;
            return 0 as ::core::ffi::c_int;
        }
        ictx.input_buf.resize(available, 0);
    }
    let fresh1 = ictx.input_len;
    ictx.input_len = ictx.input_len.wrapping_add(1);
    (&mut ictx.input_buf)[fresh1] = ictx.ch as u_char;
    (&mut ictx.input_buf)[ictx.input_len] = '\0' as i32 as u_char;
    0 as ::core::ffi::c_int
}
unsafe fn input_c0_dispatch(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
) -> ::core::ffi::c_int {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
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
    let mut first_gc: grid_cell = grid_cell {
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
    let mut cx: u_int = 0;
    let mut line: u_int = 0;
    let mut width: u_int = 0;
    let mut has_content: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    input_stop_utf8(ictx, sctx);
    log_debug(format_args!(
        "{}: '{}'",
        "input_c0_dispatch",
        log_byte((ictx.ch) as u8)
    ));
    match ictx.ch {
        0 => {}
        7 => {
            if !wp.is_null() {
                events_fire_pane(
                    c"pane-bell",
                    std::rc::Rc::clone(input_pane_owner.as_ref().expect("live pane")),
                );
                alerts_queue(
                    &WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")),
                    WINDOW_BELL,
                );
            }
        }
        8 => {
            screen_write_backspace(sctx);
        }
        9 => {
            cx = sctx.screen.cx;
            if !(cx >= sctx.screen.grid().sx.wrapping_sub(1 as u_int)) {
                line = sctx.screen.cy.wrapping_add(sctx.screen.grid().hsize);
                grid_get_cell(sctx.screen.grid(), cx, line, &mut first_gc);
                loop {
                    if has_content == 0 {
                        grid_get_cell(sctx.screen.grid(), cx, line, &mut gc);
                        if gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int
                            || *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_int
                                != ' ' as i32
                            || !grid_cells_look_equal(&gc, &first_gc)
                        {
                            has_content = 1 as ::core::ffi::c_int;
                        }
                    }
                    cx = cx.wrapping_add(1);
                    if screen_has_tab(&*sctx.screen, cx) {
                        break;
                    }
                    if !(cx < sctx.screen.grid().sx.wrapping_sub(1 as u_int)) {
                        break;
                    }
                }
                width = cx.wrapping_sub(sctx.screen.cx);
                if has_content != 0
                    || width as usize > ::core::mem::size_of::<[u_char; 32]>() as usize
                {
                    sctx.screen.cx = cx;
                } else {
                    grid_get_cell(sctx.screen.grid(), sctx.screen.cx, line, &mut gc);
                    grid_set_tab(&mut gc, width);
                    screen_write_collect_add(sctx, &gc);
                }
            }
        }
        10..=12 => {
            screen_write_linefeed(sctx, 0 as ::core::ffi::c_int, ictx.cell.cell.bg as u_int);
            if sctx.screen.mode & MODE_CRLF != 0 {
                screen_write_carriagereturn(sctx);
            }
        }
        13 => {
            screen_write_carriagereturn(sctx);
        }
        14 => {
            ictx.cell.set = 1 as ::core::ffi::c_int;
        }
        15 => {
            ictx.cell.set = 0 as ::core::ffi::c_int;
        }
        _ => {
            log_debug(format_args!(
                "{}: unknown '{}'",
                "input_c0_dispatch",
                log_byte((ictx.ch) as u8)
            ));
        }
    }
    ictx.flags &= !INPUT_LAST;
    0 as ::core::ffi::c_int
}
unsafe fn input_esc_dispatch(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
) -> ::core::ffi::c_int {
    if ictx.flags & INPUT_DISCARD != 0 {
        return 0 as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: '{}', {}",
        "input_esc_dispatch",
        log_byte((ictx.ch) as u8),
        log_bytes(&ictx.interm_buf)
    ));
    let Some(entry) = input_table_find(&input_esc_table, ictx) else {
        log_debug(format_args!(
            "{}: unknown '{}'",
            "input_esc_dispatch",
            log_byte((ictx.ch) as u8)
        ));
        return 0 as ::core::ffi::c_int;
    };
    match entry.type_0 {
        9 => {
            let _ = ictx
                .palette
                .with_mut(|palette| colour_palette_clear(Some(palette)));
            input_reset_cell(ictx);
            screen_write_reset(sctx);
            screen_write_fullredraw(sctx);
        }
        6 => {
            screen_write_linefeed(sctx, 0 as ::core::ffi::c_int, ictx.cell.cell.bg as u_int);
        }
        7 => {
            screen_write_carriagereturn(sctx);
            screen_write_linefeed(sctx, 0 as ::core::ffi::c_int, ictx.cell.cell.bg as u_int);
        }
        5 => {
            if sctx.screen.cx < sctx.screen.grid().sx {
                let column = sctx.screen.cx;
                screen_set_tab(&mut *sctx.screen, column, true);
            }
        }
        8 => {
            screen_write_reverseindex(sctx, ictx.cell.cell.bg as u_int);
        }
        1 => {
            screen_write_mode_set(sctx, MODE_KKEYPAD);
        }
        2 => {
            screen_write_mode_clear(sctx, MODE_KKEYPAD);
        }
        4 => {
            input_save_state(ictx, sctx);
        }
        3 => {
            input_restore_state(ictx, sctx);
        }
        0 => {
            screen_write_alignmenttest(sctx);
        }
        11 => {
            ictx.cell.g0set = 1 as ::core::ffi::c_int;
        }
        10 => {
            ictx.cell.g0set = 0 as ::core::ffi::c_int;
        }
        13 => {
            ictx.cell.g1set = 1 as ::core::ffi::c_int;
        }
        12 => {
            ictx.cell.g1set = 0 as ::core::ffi::c_int;
        }
        _ => {}
    }
    ictx.flags &= !INPUT_LAST;
    0 as ::core::ffi::c_int
}
unsafe fn input_csi_dispatch(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
) -> ::core::ffi::c_int {
    let input_pane_owner = ictx.wp.upgrade();
    let mut i: ::core::ffi::c_int = 0;
    let mut n: ::core::ffi::c_int = 0;
    let mut m: ::core::ffi::c_int = 0;
    let mut ek: ::core::ffi::c_int = 0;
    let mut set: ::core::ffi::c_int = 0;
    let mut p: ::core::ffi::c_int = 0;
    let mut cx: u_int = 0;
    let mut bg: u_int = ictx.cell.cell.bg as u_int;
    if ictx.flags & INPUT_DISCARD != 0 {
        return 0 as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: '{}' \"{}\" \"{}\"",
        "input_csi_dispatch",
        log_byte((ictx.ch) as u8),
        log_bytes(&ictx.interm_buf),
        log_bytes(&ictx.param_buf)
    ));
    if input_split(ictx) != 0 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    let Some(entry) = input_table_find(&input_csi_table, ictx) else {
        log_debug(format_args!(
            "{}: unknown '{}'",
            "input_csi_dispatch",
            log_byte((ictx.ch) as u8)
        ));
        return 0 as ::core::ffi::c_int;
    };
    match entry.type_0 {
        0 => {
            cx = sctx.screen.cx;
            if cx > sctx.screen.grid().sx.wrapping_sub(1 as u_int) {
                cx = sctx.screen.grid().sx.wrapping_sub(1 as u_int);
            }
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if !(n == -(1 as ::core::ffi::c_int)) {
                while cx > 0 as u_int && {
                    let fresh10 = n;
                    n -= 1;
                    fresh10 > 0 as ::core::ffi::c_int
                } {
                    loop {
                        cx = cx.wrapping_sub(1);
                        if !(cx > 0 as u_int && !screen_has_tab(&*sctx.screen, cx)) {
                            break;
                        }
                    }
                }
                sctx.screen.cx = cx;
            }
        }
        3 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_cursorleft(sctx, n as u_int);
            }
        }
        4 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_cursordown(sctx, n as u_int);
            }
        }
        5 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_cursorright(sctx, n as u_int);
            }
        }
        6 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            m = input_get(
                ictx,
                1 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) && m != -(1 as ::core::ffi::c_int) {
                screen_write_cursormove(
                    sctx,
                    m - 1 as ::core::ffi::c_int,
                    n - 1 as ::core::ffi::c_int,
                    1 as ::core::ffi::c_int,
                );
            }
        }
        23 => {
            n = input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            if !(n != 4 as ::core::ffi::c_int) {
                m = input_get(
                    ictx,
                    1 as u_int,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                ek = OptionsScope::GlobalServer
                    .with_local(|oo| options_get_number(oo, c"extended-keys"))
                    as ::core::ffi::c_int;
                if !(ek == 0 as ::core::ffi::c_int) {
                    screen_write_mode_clear(sctx, EXTENDED_KEY_MODES);
                    if m == 2 as ::core::ffi::c_int {
                        screen_write_mode_set(sctx, MODE_KEYS_EXTENDED_2);
                    } else if m == 1 as ::core::ffi::c_int || ek == 2 as ::core::ffi::c_int {
                        screen_write_mode_set(sctx, MODE_KEYS_EXTENDED);
                    }
                }
            }
        }
        22 => {
            n = input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            if !(n != 4 as ::core::ffi::c_int) {
                screen_write_mode_clear(sctx, MODE_KEYS_EXTENDED | MODE_KEYS_EXTENDED_2);
                if OptionsScope::GlobalServer
                    .with_local(|oo| options_get_number(oo, c"extended-keys"))
                    == 2 as ::core::ffi::c_longlong
                {
                    screen_write_mode_set(sctx, MODE_KEYS_EXTENDED);
                }
            }
        }
        39 => {
            input_csi_dispatch_winops(ictx, sctx);
        }
        7 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_cursorup(sctx, n as u_int);
            }
        }
        1 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_carriagereturn(sctx);
                screen_write_cursordown(sctx, n as u_int);
            }
        }
        2 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_carriagereturn(sctx);
                screen_write_cursorup(sctx, n as u_int);
            }
        }
        8 => {
            match input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) {
                -1 => {}
                0 => {
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        out.write_all(b"\x1B[?1;2c")
                    });
                }
                _ => {
                    log_debug(format_args!(
                        "{}: unknown '{}'",
                        "input_csi_dispatch",
                        log_byte((ictx.ch) as u8)
                    ));
                }
            }
        }
        9 => {
            match input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) {
                -1 => {}
                0 => {
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        out.write_all(b"\x1B[>84;0;0c")
                    });
                }
                _ => {
                    log_debug(format_args!(
                        "{}: unknown '{}'",
                        "input_csi_dispatch",
                        log_byte((ictx.ch) as u8)
                    ));
                }
            }
        }
        16 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_clearcharacter(sctx, n as u_int, bg);
            }
        }
        10 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_deletecharacter(sctx, n as u_int, bg);
            }
        }
        12 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            m = input_get(
                ictx,
                1 as u_int,
                1 as ::core::ffi::c_int,
                sctx.screen.grid().sy as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) && m != -(1 as ::core::ffi::c_int) {
                screen_write_scrollregion(
                    sctx,
                    (n - 1 as ::core::ffi::c_int) as u_int,
                    (m - 1 as ::core::ffi::c_int) as u_int,
                );
            }
        }
        13 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_deleteline(sctx, n as u_int, bg);
            }
        }
        15 => {
            if input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) == 996
            {
                input_report_current_theme(ictx);
            }
        }
        24 => {
            m = input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            match m {
                4 => {
                    n = if sctx.screen.mode & MODE_INSERT != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                _ => {
                    n = 0 as ::core::ffi::c_int;
                }
            }
            if m > 0 as ::core::ffi::c_int {
                input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                    write!(out, "\x1B[{};{}$y", (m) as i32, (n) as i32)
                });
            }
        }
        25 => {
            m = input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            match m {
                1 => {
                    n = if sctx.screen.mode & MODE_KCURSOR != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                3 => {
                    n = 4 as ::core::ffi::c_int;
                }
                6 => {
                    n = if sctx.screen.mode & MODE_ORIGIN != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                7 => {
                    n = if sctx.screen.mode & MODE_WRAP != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                12 => {
                    if sctx.screen.cstyle as ::core::ffi::c_uint
                        != SCREEN_CURSOR_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
                        || sctx.screen.mode & MODE_CURSOR_BLINKING_SET != 0
                    {
                        n = if sctx.screen.mode & MODE_CURSOR_BLINKING != 0 {
                            1 as ::core::ffi::c_int
                        } else {
                            2 as ::core::ffi::c_int
                        };
                    } else {
                        p = if input_pane_owner.is_some() {
                            options_get_number(
                                &input_pane_owner
                                    .as_ref()
                                    .expect("live pane")
                                    .borrow_options_mut(),
                                c"cursor-style",
                            )
                        } else {
                            OptionsScope::GlobalWindow
                                .with_local(|oo| options_get_number(oo, c"cursor-style"))
                        } as ::core::ffi::c_int;
                        n = if p == 1 as ::core::ffi::c_int
                            || p == 3 as ::core::ffi::c_int
                            || p == 5 as ::core::ffi::c_int
                        {
                            1 as ::core::ffi::c_int
                        } else {
                            2 as ::core::ffi::c_int
                        };
                    }
                }
                25 => {
                    n = if sctx.screen.mode & MODE_CURSOR != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                47 | 1047 | 1049 => {
                    n = if sctx.screen.saved_grid.is_some() {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                1000 => {
                    n = if sctx.screen.mode & MODE_MOUSE_STANDARD != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                1002 => {
                    n = if sctx.screen.mode & MODE_MOUSE_BUTTON != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                1003 => {
                    n = if sctx.screen.mode & MODE_MOUSE_ALL != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                1004 => {
                    n = if sctx.screen.mode & MODE_FOCUSON != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                1005 => {
                    n = if sctx.screen.mode & MODE_MOUSE_UTF8 != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                1006 => {
                    n = if sctx.screen.mode & MODE_MOUSE_SGR != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                2004 => {
                    n = if sctx.screen.mode & MODE_BRACKETPASTE != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                2026 => {
                    n = if sctx.screen.mode & MODE_SYNC != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                2031 => {
                    n = if sctx.screen.mode & MODE_THEME_UPDATES != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                _ => {
                    n = 0 as ::core::ffi::c_int;
                }
            }
            if m > 0 as ::core::ffi::c_int {
                input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                    write!(out, "\x1B[?{};{}$y", (m) as i32, (n) as i32)
                });
            }
        }
        14 => {
            match input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) {
                -1 => {}
                5 => {
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        out.write_all(b"\x1B[0n")
                    });
                }
                6 => {
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        write!(
                            out,
                            "\x1B[{};{}R",
                            { sctx.screen.cy.wrapping_add(1 as u_int) },
                            { sctx.screen.cx.wrapping_add(1 as u_int) }
                        )
                    });
                }
                _ => {
                    log_debug(format_args!(
                        "{}: unknown '{}'",
                        "input_csi_dispatch",
                        log_byte((ictx.ch) as u8)
                    ));
                }
            }
        }
        17 => {
            match input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) {
                -1 => {}
                0 => {
                    screen_write_clearendofscreen(sctx, bg);
                }
                1 => {
                    screen_write_clearstartofscreen(sctx, bg);
                }
                2 => {
                    screen_write_clearscreen(sctx, bg);
                }
                3 => {
                    if input_get(
                        ictx,
                        1 as u_int,
                        0 as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    ) == 0 as ::core::ffi::c_int
                    {
                        screen_write_clearhistory(sctx);
                    }
                }
                _ => {
                    log_debug(format_args!(
                        "{}: unknown '{}'",
                        "input_csi_dispatch",
                        log_byte((ictx.ch) as u8)
                    ));
                }
            }
        }
        18 => {
            match input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) {
                -1 => {}
                0 => {
                    screen_write_clearendofline(sctx, bg);
                }
                1 => {
                    screen_write_clearstartofline(sctx, bg);
                }
                2 => {
                    screen_write_clearline(sctx, bg);
                }
                _ => {
                    log_debug(format_args!(
                        "{}: unknown '{}'",
                        "input_csi_dispatch",
                        log_byte((ictx.ch) as u8)
                    ));
                }
            }
        }
        19 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_cursormove(
                    sctx,
                    n - 1 as ::core::ffi::c_int,
                    -(1 as ::core::ffi::c_int),
                    1 as ::core::ffi::c_int,
                );
            }
        }
        20 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_insertcharacter(sctx, n as u_int, bg);
            }
        }
        21 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_insertline(sctx, n as u_int, bg);
            }
        }
        27 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if !(n == -(1 as ::core::ffi::c_int)) {
                m = sctx.screen.grid().sx.wrapping_sub(sctx.screen.cx) as ::core::ffi::c_int;
                if n > m {
                    n = m;
                }
                if !(!ictx.flags & INPUT_LAST != 0) {
                    set = if ictx.cell.set == 0 as ::core::ffi::c_int {
                        ictx.cell.g0set
                    } else {
                        ictx.cell.g1set
                    };
                    if set == 1 as ::core::ffi::c_int {
                        ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                            | GRID_ATTR_CHARSET)
                            as u_short;
                    } else {
                        ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                            & !GRID_ATTR_CHARSET)
                            as u_short;
                    }
                    ictx.cell.cell.data = utf8_copy(&ictx.last);
                    i = 0 as ::core::ffi::c_int;
                    while i < n {
                        screen_write_collect_add(sctx, &ictx.cell.cell);
                        i += 1;
                    }
                }
            }
        }
        26 => {
            input_restore_state(ictx, sctx);
        }
        28 => {
            input_csi_dispatch_rm(ictx, sctx);
        }
        29 => {
            input_csi_dispatch_rm_private(ictx, sctx);
        }
        30 => {
            input_save_state(ictx, sctx);
        }
        32 => {
            input_csi_dispatch_sgr(ictx);
        }
        33 => {
            input_csi_dispatch_sm(ictx, sctx);
        }
        35 => {
            input_csi_dispatch_sm_private(ictx, sctx);
        }
        34 => {
            input_csi_dispatch_sm_graphics();
        }
        36 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_scrollup(sctx, n as u_int, bg);
            }
        }
        31 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_scrolldown(sctx, n as u_int, bg);
            }
        }
        37 => {
            match input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) {
                -1 => {}
                0 => {
                    if sctx.screen.cx < sctx.screen.grid().sx {
                        let column = sctx.screen.cx;
                        screen_set_tab(&mut *sctx.screen, column, false);
                    }
                }
                3 => {
                    screen_clear_tabs(&mut *sctx.screen);
                }
                _ => {
                    log_debug(format_args!(
                        "{}: unknown '{}'",
                        "input_csi_dispatch",
                        log_byte((ictx.ch) as u8)
                    ));
                }
            }
        }
        38 => {
            n = input_get(
                ictx,
                0 as u_int,
                1 as ::core::ffi::c_int,
                1 as ::core::ffi::c_int,
            );
            if n != -(1 as ::core::ffi::c_int) {
                screen_write_cursormove(
                    sctx,
                    -(1 as ::core::ffi::c_int),
                    n - 1 as ::core::ffi::c_int,
                    1 as ::core::ffi::c_int,
                );
            }
        }
        11 => {
            n = input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            if !(n == -(1 as ::core::ffi::c_int)) {
                screen_set_cursor_style(n as u_int, &mut sctx.screen.cstyle, &mut sctx.screen.mode);
                if n == 0 as ::core::ffi::c_int {
                    screen_write_mode_clear(sctx, MODE_CURSOR_BLINKING_SET);
                }
            }
        }
        40 => {
            n = input_get(
                ictx,
                0 as u_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            if n == 0 as ::core::ffi::c_int {
                input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                    out.write_all(b"\x1BP>|tmux ")?;
                    out.write_all(getversion().to_bytes())?;
                    out.write_all(b"\x1B\\")
                });
            }
        }
        _ => {}
    }
    ictx.flags &= !INPUT_LAST;
    0 as ::core::ffi::c_int
}
fn input_csi_dispatch_rm(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let mut i: u_int = 0;
    i = 0 as u_int;
    while i < ictx.param_list_len {
        match input_get(ictx, i, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int)) {
            -1 => {}
            4 => {
                screen_write_mode_clear(sctx, MODE_INSERT);
            }
            34 => {
                screen_write_mode_set(sctx, MODE_CURSOR_VERY_VISIBLE);
            }
            _ => {
                log_debug(format_args!(
                    "{}: unknown '{}'",
                    "input_csi_dispatch_rm",
                    log_byte((ictx.ch) as u8)
                ));
            }
        }
        i = i.wrapping_add(1);
    }
}
unsafe fn input_csi_dispatch_rm_private(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut i: u_int = 0;
    i = 0 as u_int;
    while i < ictx.param_list_len {
        match input_get(ictx, i, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int)) {
            -1 => {}
            1 => {
                screen_write_mode_clear(sctx, MODE_KCURSOR);
            }
            3 => {
                screen_write_cursormove(
                    sctx,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    1 as ::core::ffi::c_int,
                );
                screen_write_clearscreen(sctx, ictx.cell.cell.bg as u_int);
            }
            6 => {
                screen_write_mode_clear(sctx, MODE_ORIGIN);
                screen_write_cursormove(
                    sctx,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    1 as ::core::ffi::c_int,
                );
            }
            7 => {
                screen_write_mode_clear(sctx, MODE_WRAP);
            }
            12 => {
                screen_write_mode_clear(sctx, MODE_CURSOR_BLINKING);
                screen_write_mode_set(sctx, MODE_CURSOR_BLINKING_SET);
            }
            25 => {
                screen_write_mode_clear(sctx, MODE_CURSOR);
            }
            1000..=1003 => {
                screen_write_mode_clear(sctx, ALL_MOUSE_MODES);
            }
            1004 => {
                screen_write_mode_clear(sctx, MODE_FOCUSON);
            }
            1005 => {
                screen_write_mode_clear(sctx, MODE_MOUSE_UTF8);
            }
            1006 => {
                screen_write_mode_clear(sctx, MODE_MOUSE_SGR);
            }
            47 | 1047 => {
                screen_write_alternateoff(sctx, &mut ictx.cell.cell, 0 as ::core::ffi::c_int);
            }
            1049 => {
                screen_write_alternateoff(sctx, &mut ictx.cell.cell, 1 as ::core::ffi::c_int);
            }
            2004 => {
                screen_write_mode_clear(sctx, MODE_BRACKETPASTE);
            }
            2026 => {
                screen_write_end_sync(sctx);
            }
            2031 => {
                screen_write_mode_clear(sctx, MODE_THEME_UPDATES);
                if !input_pane.is_null() {
                    (*input_pane).flags &= !PANE_THEMECHANGED;
                }
            }
            _ => {
                log_debug(format_args!(
                    "{}: unknown '{}'",
                    "input_csi_dispatch_rm_private",
                    log_byte((ictx.ch) as u8)
                ));
            }
        }
        i = i.wrapping_add(1);
    }
}
fn input_csi_dispatch_sm(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let mut i: u_int = 0;
    i = 0 as u_int;
    while i < ictx.param_list_len {
        match input_get(ictx, i, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int)) {
            -1 => {}
            4 => {
                screen_write_mode_set(sctx, MODE_INSERT);
            }
            34 => {
                screen_write_mode_clear(sctx, MODE_CURSOR_VERY_VISIBLE);
            }
            _ => {
                log_debug(format_args!(
                    "{}: unknown '{}'",
                    "input_csi_dispatch_sm",
                    log_byte((ictx.ch) as u8)
                ));
            }
        }
        i = i.wrapping_add(1);
    }
}
unsafe fn input_csi_dispatch_sm_private(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut i: u_int = 0;
    i = 0 as u_int;
    while i < ictx.param_list_len {
        match input_get(ictx, i, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int)) {
            -1 => {}
            1 => {
                screen_write_mode_set(sctx, MODE_KCURSOR);
            }
            3 => {
                screen_write_cursormove(
                    sctx,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    1 as ::core::ffi::c_int,
                );
                screen_write_clearscreen(sctx, ictx.cell.cell.bg as u_int);
            }
            6 => {
                screen_write_mode_set(sctx, MODE_ORIGIN);
                screen_write_cursormove(
                    sctx,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    1 as ::core::ffi::c_int,
                );
            }
            7 => {
                screen_write_mode_set(sctx, MODE_WRAP);
            }
            12 => {
                screen_write_mode_set(sctx, MODE_CURSOR_BLINKING);
                screen_write_mode_set(sctx, MODE_CURSOR_BLINKING_SET);
            }
            25 => {
                screen_write_mode_set(sctx, MODE_CURSOR);
            }
            1000 => {
                screen_write_mode_clear(sctx, ALL_MOUSE_MODES);
                screen_write_mode_set(sctx, MODE_MOUSE_STANDARD);
            }
            1002 => {
                screen_write_mode_clear(sctx, ALL_MOUSE_MODES);
                screen_write_mode_set(sctx, MODE_MOUSE_BUTTON);
            }
            1003 => {
                screen_write_mode_clear(sctx, ALL_MOUSE_MODES);
                screen_write_mode_set(sctx, MODE_MOUSE_ALL);
            }
            1004 => {
                screen_write_mode_set(sctx, MODE_FOCUSON);
            }
            1005 => {
                screen_write_mode_set(sctx, MODE_MOUSE_UTF8);
            }
            1006 => {
                screen_write_mode_set(sctx, MODE_MOUSE_SGR);
            }
            47 | 1047 => {
                screen_write_alternateon(sctx, &ictx.cell.cell, 0 as ::core::ffi::c_int);
            }
            1049 => {
                screen_write_alternateon(sctx, &ictx.cell.cell, 1 as ::core::ffi::c_int);
            }
            2004 => {
                screen_write_mode_set(sctx, MODE_BRACKETPASTE);
            }
            2031 => {
                screen_write_mode_set(sctx, MODE_THEME_UPDATES);
                if !input_pane.is_null() {
                    (*input_pane).last_theme = window_pane_get_theme(input_pane_owner.as_ref());
                    (*input_pane).flags &= !PANE_THEMECHANGED;
                }
            }
            2026 => {
                if let Some(pane) = input_pane_owner.as_ref() {
                    pane.start_sync(&mut *sctx.screen);
                }
            }
            _ => {
                log_debug(format_args!(
                    "{}: unknown '{}'",
                    "input_csi_dispatch_sm_private",
                    log_byte((ictx.ch) as u8)
                ));
            }
        }
        i = i.wrapping_add(1);
    }
}
fn input_csi_dispatch_sm_graphics() {}
unsafe fn input_csi_dispatch_winops(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let input_pane_owner = ictx.wp.upgrade();
    let mut x: u_int = sctx.screen.grid().sx;
    let mut y: u_int = sctx.screen.grid().sy;
    let mut n: ::core::ffi::c_int = 0;
    let mut m: ::core::ffi::c_int = 0;
    let window_owner = input_pane_owner
        .as_ref()
        .and_then(|pane| pane.window_observer().upgrade());
    m = 0 as ::core::ffi::c_int;
    loop {
        n = input_get(
            ictx,
            m as u_int,
            0 as ::core::ffi::c_int,
            -(1 as ::core::ffi::c_int),
        );
        if !(n != -(1 as ::core::ffi::c_int)) {
            break;
        }
        let mut current_block_25: u64;
        match n {
            1 | 2 | 5 | 6 | 7 | 11 | 13 | 20 | 21 | 24 => {
                current_block_25 = 980989089337379490;
            }
            3 | 4 | 8 => {
                m += 1;
                if input_get(
                    ictx,
                    m as u_int,
                    0 as ::core::ffi::c_int,
                    -(1 as ::core::ffi::c_int),
                ) == -(1 as ::core::ffi::c_int)
                {
                    break;
                }
                current_block_25 = 8019652857213515700;
            }
            9 | 10 => {
                current_block_25 = 8019652857213515700;
            }
            14 => {
                if let Some(window) = &window_owner {
                    let (xpixel, ypixel) = window.cell_size();
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        write!(
                            out,
                            "\x1B[4;{};{}t",
                            (y.wrapping_mul(ypixel)) as u32,
                            (x.wrapping_mul(xpixel)) as u32
                        )
                    });
                }
                current_block_25 = 980989089337379490;
            }
            15 => {
                if let Some(window) = &window_owner {
                    let (xpixel, ypixel) = window.cell_size();
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        write!(
                            out,
                            "\x1B[5;{};{}t",
                            (y.wrapping_mul(ypixel)) as u32,
                            (x.wrapping_mul(xpixel)) as u32
                        )
                    });
                }
                current_block_25 = 980989089337379490;
            }
            16 => {
                if let Some(window) = &window_owner {
                    let (xpixel, ypixel) = window.cell_size();
                    input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                        write!(out, "\x1B[6;{};{}t", { ypixel }, { xpixel })
                    });
                }
                current_block_25 = 980989089337379490;
            }
            18 => {
                input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                    write!(out, "\x1B[8;{};{}t", (y) as u32, (x) as u32)
                });
                current_block_25 = 980989089337379490;
            }
            19 => {
                input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
                    write!(out, "\x1B[9;{};{}t", (y) as u32, (x) as u32)
                });
                current_block_25 = 980989089337379490;
            }
            22 => {
                m += 1;
                match input_get(
                    ictx,
                    m as u_int,
                    0 as ::core::ffi::c_int,
                    -(1 as ::core::ffi::c_int),
                ) {
                    -1 => break,
                    0 | 2 => {
                        screen_push_title(&mut *sctx.screen);
                    }
                    _ => {}
                }
                current_block_25 = 980989089337379490;
            }
            23 => {
                m += 1;
                match input_get(
                    ictx,
                    m as u_int,
                    0 as ::core::ffi::c_int,
                    -(1 as ::core::ffi::c_int),
                ) {
                    -1 => break,
                    0 | 2 => {
                        screen_pop_title(&mut *sctx.screen);
                        if input_pane_owner.is_some() {
                            input_fire_pane_title_changed(
                                input_pane_owner.as_ref().expect("live pane"),
                                &(sctx.screen.title),
                            );
                            server_redraw_window_borders(
                                window_owner.as_ref().expect("pane window"),
                            );
                            server_status_window(window_owner.as_ref().expect("pane window"));
                        }
                    }
                    _ => {}
                }
                current_block_25 = 980989089337379490;
            }
            _ => {
                log_debug(format_args!(
                    "{}: unknown '{}'",
                    "input_csi_dispatch_winops",
                    log_byte((ictx.ch) as u8)
                ));
                current_block_25 = 980989089337379490;
            }
        }
        if current_block_25 == 8019652857213515700 {
            m += 1;
            if input_get(
                ictx,
                m as u_int,
                0 as ::core::ffi::c_int,
                -(1 as ::core::ffi::c_int),
            ) == -(1 as ::core::ffi::c_int)
            {
                break;
            }
        }
        m += 1;
    }
    if let Some(window) = window_owner {
        drop(window);
    }
}
fn input_csi_dispatch_sgr_256_do(
    ictx: &mut input_ctx,
    mut fgbg: ::core::ffi::c_int,
    mut c: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    if c == -(1 as ::core::ffi::c_int) || c > 255 as ::core::ffi::c_int {
        if fgbg == 38 as ::core::ffi::c_int {
            ictx.cell.cell.fg = 8 as ::core::ffi::c_int;
        } else if fgbg == 48 as ::core::ffi::c_int {
            ictx.cell.cell.bg = 8 as ::core::ffi::c_int;
        }
    } else if fgbg == 38 as ::core::ffi::c_int {
        ictx.cell.cell.fg = c | COLOUR_FLAG_256;
    } else if fgbg == 48 as ::core::ffi::c_int {
        ictx.cell.cell.bg = c | COLOUR_FLAG_256;
    } else if fgbg == 58 as ::core::ffi::c_int {
        ictx.cell.cell.us = c | COLOUR_FLAG_256;
    }
    1 as ::core::ffi::c_int
}
fn input_csi_dispatch_sgr_256(ictx: &mut input_ctx, mut fgbg: ::core::ffi::c_int, i: &mut u_int) {
    let mut c: ::core::ffi::c_int = 0;
    c = input_get(
        ictx,
        i.wrapping_add(1 as u_int),
        0 as ::core::ffi::c_int,
        -(1 as ::core::ffi::c_int),
    );
    if input_csi_dispatch_sgr_256_do(ictx, fgbg, c) != 0 {
        *i = i.wrapping_add(1);
    }
}
fn input_csi_dispatch_sgr_rgb_do(
    ictx: &mut input_ctx,
    mut fgbg: ::core::ffi::c_int,
    mut r: ::core::ffi::c_int,
    mut g: ::core::ffi::c_int,
    mut b: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    if r == -(1 as ::core::ffi::c_int) || r > 255 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    if g == -(1 as ::core::ffi::c_int) || g > 255 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    if b == -(1 as ::core::ffi::c_int) || b > 255 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    if fgbg == 38 as ::core::ffi::c_int {
        ictx.cell.cell.fg = colour_join_rgb(r as u_char, g as u_char, b as u_char);
    } else if fgbg == 48 as ::core::ffi::c_int {
        ictx.cell.cell.bg = colour_join_rgb(r as u_char, g as u_char, b as u_char);
    } else if fgbg == 58 as ::core::ffi::c_int {
        ictx.cell.cell.us = colour_join_rgb(r as u_char, g as u_char, b as u_char);
    }
    1 as ::core::ffi::c_int
}
fn input_csi_dispatch_sgr_rgb(ictx: &mut input_ctx, mut fgbg: ::core::ffi::c_int, i: &mut u_int) {
    let mut r: ::core::ffi::c_int = 0;
    let mut g: ::core::ffi::c_int = 0;
    let mut b: ::core::ffi::c_int = 0;
    r = input_get(
        ictx,
        i.wrapping_add(1 as u_int),
        0 as ::core::ffi::c_int,
        -(1 as ::core::ffi::c_int),
    );
    g = input_get(
        ictx,
        i.wrapping_add(2 as u_int),
        0 as ::core::ffi::c_int,
        -(1 as ::core::ffi::c_int),
    );
    b = input_get(
        ictx,
        i.wrapping_add(3 as u_int),
        0 as ::core::ffi::c_int,
        -(1 as ::core::ffi::c_int),
    );
    if input_csi_dispatch_sgr_rgb_do(ictx, fgbg, r, g, b) != 0 {
        *i = i.wrapping_add(3 as u_int);
    }
}
fn input_csi_dispatch_sgr_colon(ictx: &mut input_ctx, mut i: u_int) {
    let s = match &ictx.param_list[i as usize] {
        input_param::String(value) => value.as_c_str(),
        _ => panic!("SGR colon parser requires a string parameter"),
    };
    let mut p: [::core::ffi::c_int; 8] = [0; 8];
    let mut n: u_int = 0;
    n = 0 as u_int;
    while (n as usize)
        < (::core::mem::size_of::<[::core::ffi::c_int; 8]>() as usize)
            .wrapping_div(::core::mem::size_of::<::core::ffi::c_int>() as usize)
    {
        p[n as usize] = -(1 as ::core::ffi::c_int);
        n = n.wrapping_add(1);
    }
    n = 0 as u_int;
    let mut copy = s.to_bytes_with_nul().to_vec();
    for out in input_split_fields(&mut copy, b':') {
        if !out.is_empty() {
            let fresh13 = n;
            n = n.wrapping_add(1);
            let parsed = strtonum(out, 0, INT_MAX as ::core::ffi::c_longlong);
            p[fresh13 as usize] = parsed.unwrap_or(0) as ::core::ffi::c_int;
            if parsed.is_err()
                || n as usize
                    == (::core::mem::size_of::<[::core::ffi::c_int; 8]>() as usize)
                        .wrapping_div(::core::mem::size_of::<::core::ffi::c_int>() as usize)
            {
                return;
            }
        } else {
            n = n.wrapping_add(1);
            if n as usize
                == (::core::mem::size_of::<[::core::ffi::c_int; 8]>() as usize)
                    .wrapping_div(::core::mem::size_of::<::core::ffi::c_int>() as usize)
            {
                return;
            }
        }
        log_debug(format_args!(
            "{}: {} = {}",
            "input_csi_dispatch_sgr_colon",
            (n.wrapping_sub(1 as u_int)) as u32,
            (p[n.wrapping_sub(1 as u_int) as usize]) as i32
        ));
    }
    if n == 0 as u_int {
        return;
    }
    if p[0 as ::core::ffi::c_int as usize] == 4 as ::core::ffi::c_int {
        if n != 2 as u_int {
            return;
        }
        match p[1 as ::core::ffi::c_int as usize] {
            0 => {
                ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                    & !GRID_ATTR_ALL_UNDERSCORE) as u_short;
            }
            1 => {
                ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                    & !GRID_ATTR_ALL_UNDERSCORE) as u_short;
                ictx.cell.cell.attr =
                    (ictx.cell.cell.attr as ::core::ffi::c_int | GRID_ATTR_UNDERSCORE) as u_short;
            }
            2 => {
                ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                    & !GRID_ATTR_ALL_UNDERSCORE) as u_short;
                ictx.cell.cell.attr =
                    (ictx.cell.cell.attr as ::core::ffi::c_int | GRID_ATTR_UNDERSCORE_2) as u_short;
            }
            3 => {
                ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                    & !GRID_ATTR_ALL_UNDERSCORE) as u_short;
                ictx.cell.cell.attr =
                    (ictx.cell.cell.attr as ::core::ffi::c_int | GRID_ATTR_UNDERSCORE_3) as u_short;
            }
            4 => {
                ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                    & !GRID_ATTR_ALL_UNDERSCORE) as u_short;
                ictx.cell.cell.attr =
                    (ictx.cell.cell.attr as ::core::ffi::c_int | GRID_ATTR_UNDERSCORE_4) as u_short;
            }
            5 => {
                ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                    & !GRID_ATTR_ALL_UNDERSCORE) as u_short;
                ictx.cell.cell.attr =
                    (ictx.cell.cell.attr as ::core::ffi::c_int | GRID_ATTR_UNDERSCORE_5) as u_short;
            }
            _ => {}
        }
        return;
    }
    if n < 2 as u_int
        || p[0 as ::core::ffi::c_int as usize] != 38 as ::core::ffi::c_int
            && p[0 as ::core::ffi::c_int as usize] != 48 as ::core::ffi::c_int
            && p[0 as ::core::ffi::c_int as usize] != 58 as ::core::ffi::c_int
    {
        return;
    }
    match p[1 as ::core::ffi::c_int as usize] {
        2 => {
            if !(n < 3 as u_int) {
                if n == 5 as u_int {
                    i = 2 as u_int;
                } else {
                    i = 3 as u_int;
                }
                if !(n < i.wrapping_add(3 as u_int)) {
                    input_csi_dispatch_sgr_rgb_do(
                        ictx,
                        p[0 as ::core::ffi::c_int as usize],
                        p[i as usize],
                        p[i.wrapping_add(1 as u_int) as usize],
                        p[i.wrapping_add(2 as u_int) as usize],
                    );
                }
            }
        }
        5 if !(n < 3 as u_int) => {
            input_csi_dispatch_sgr_256_do(
                ictx,
                p[0 as ::core::ffi::c_int as usize],
                p[2 as ::core::ffi::c_int as usize],
            );
        }
        _ => {}
    };
}

#[cfg(test)]
mod sgr_colon_tests {
    use super::{
        colour_join_rgb, grid_cell, input_csi_dispatch_sgr_colon, input_ctx, input_param,
        COLOUR_FLAG_256, GRID_ATTR_UNDERSCORE_2, GRID_ATTR_UNDERSCORE_3,
    };

    fn parse(source: &[u8]) -> grid_cell {
        assert!(source.contains(&0));
        let mut ictx = input_ctx::new();
        let parameter = std::ffi::CStr::from_bytes_until_nul(source).unwrap();
        ictx.param_list[0] = input_param::String(parameter.to_owned());
        ictx.cell.cell.fg = 11;
        ictx.cell.cell.bg = 12;
        ictx.cell.cell.us = 13;
        ictx.cell.cell.attr = GRID_ATTR_UNDERSCORE_2 as u16;
        input_csi_dispatch_sgr_colon(&mut ictx, 0);
        assert_eq!(
            match &ictx.param_list[0] {
                input_param::String(value) => value.as_c_str(),
                _ => panic!("expected string parameter"),
            },
            parameter,
            "the input parameter must not be tokenized"
        );
        ictx.cell.cell
    }

    #[test]
    fn parses_rgb_indexed_and_underline_colon_parameters() {
        assert_eq!(parse(b"38:2:1:2:3\0").fg, colour_join_rgb(1, 2, 3));
        assert_eq!(parse(b"38:2::1:2:3\0").fg, colour_join_rgb(1, 2, 3));
        assert_eq!(parse(b"48:5:196\0").bg, 196 | COLOUR_FLAG_256);
        assert_eq!(parse(b"58:2:5:6:7\0").us, colour_join_rgb(5, 6, 7));
        assert_eq!(parse(b"4:3\0").attr as i32, GRID_ATTR_UNDERSCORE_3);
    }

    #[test]
    fn rejects_bad_and_excess_tokens_without_changing_colours() {
        for source in [
            b"38:2:999999999999999:2:3\0".as_slice(),
            b"38:2:abc:2:3\0",
            b"38:2:1:2:3:4:5:6\0",
            b":::::::\0",
            b"38:2:300:2:3\0",
            b"38:2:1:2\0",
        ] {
            let cell = parse(source);
            assert_eq!((cell.fg, cell.bg, cell.us), (11, 12, 13), "{source:?}");
        }
        assert_eq!(parse(b"38:5:196\0:2:1:2:3\0").fg, 196 | COLOUR_FLAG_256);
    }
}
fn input_csi_dispatch_sgr(ictx: &mut input_ctx) {
    let mut i: u_int = 0;
    let mut link: u_int = 0;
    let mut n: ::core::ffi::c_int = 0;
    if ictx.param_list_len == 0 as u_int {
        ictx.cell.cell = grid_default_cell;
        return;
    }
    i = 0 as u_int;
    while i < ictx.param_list_len {
        if matches!(&ictx.param_list[i as usize], input_param::String(_)) {
            input_csi_dispatch_sgr_colon(ictx, i);
        } else {
            n = input_get(ictx, i, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
            if !(n == -(1 as ::core::ffi::c_int)) {
                if n == 38 as ::core::ffi::c_int
                    || n == 48 as ::core::ffi::c_int
                    || n == 58 as ::core::ffi::c_int
                {
                    i = i.wrapping_add(1);
                    match input_get(ictx, i, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int)) {
                        2 => {
                            input_csi_dispatch_sgr_rgb(ictx, n, &mut i);
                        }
                        5 => {
                            input_csi_dispatch_sgr_256(ictx, n, &mut i);
                        }
                        _ => {}
                    }
                } else {
                    match n {
                        0 => {
                            link = ictx.cell.cell.link;
                            ictx.cell.cell = grid_default_cell;
                            ictx.cell.cell.link = link;
                        }
                        1 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_BRIGHT)
                                as u_short;
                        }
                        2 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_DIM)
                                as u_short;
                        }
                        3 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_ITALICS)
                                as u_short;
                        }
                        4 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_ALL_UNDERSCORE)
                                as u_short;
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_UNDERSCORE)
                                as u_short;
                        }
                        5 | 6 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_BLINK)
                                as u_short;
                        }
                        7 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_REVERSE)
                                as u_short;
                        }
                        8 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_HIDDEN)
                                as u_short;
                        }
                        9 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_STRIKETHROUGH)
                                as u_short;
                        }
                        21 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_ALL_UNDERSCORE)
                                as u_short;
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_UNDERSCORE_2)
                                as u_short;
                        }
                        22 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !(GRID_ATTR_BRIGHT | GRID_ATTR_DIM))
                                as u_short;
                        }
                        23 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_ITALICS)
                                as u_short;
                        }
                        24 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_ALL_UNDERSCORE)
                                as u_short;
                        }
                        25 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_BLINK)
                                as u_short;
                        }
                        27 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_REVERSE)
                                as u_short;
                        }
                        28 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_HIDDEN)
                                as u_short;
                        }
                        29 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_STRIKETHROUGH)
                                as u_short;
                        }
                        30..=37 => {
                            ictx.cell.cell.fg = n - 30 as ::core::ffi::c_int;
                        }
                        39 => {
                            ictx.cell.cell.fg = 8 as ::core::ffi::c_int;
                        }
                        40..=47 => {
                            ictx.cell.cell.bg = n - 40 as ::core::ffi::c_int;
                        }
                        49 => {
                            ictx.cell.cell.bg = 8 as ::core::ffi::c_int;
                        }
                        53 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                | GRID_ATTR_OVERLINE)
                                as u_short;
                        }
                        55 => {
                            ictx.cell.cell.attr = (ictx.cell.cell.attr as ::core::ffi::c_int
                                & !GRID_ATTR_OVERLINE)
                                as u_short;
                        }
                        59 => {
                            ictx.cell.cell.us = 8 as ::core::ffi::c_int;
                        }
                        90..=97 => {
                            ictx.cell.cell.fg = n;
                        }
                        100..=107 => {
                            ictx.cell.cell.bg = n - 10 as ::core::ffi::c_int;
                        }
                        _ => {}
                    }
                }
            }
        }
        i = i.wrapping_add(1);
    }
}
fn input_end_bel(ictx: &mut input_ctx, _sctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    log_debug(format_args!("{}", "input_end_bel"));
    ictx.input_end = INPUT_END_BEL;
    0 as ::core::ffi::c_int
}
unsafe fn input_enter_dcs(ictx: &mut input_ctx) {
    log_debug(format_args!("{}", "input_enter_dcs"));
    input_clear(ictx);
    input_start_ground_timer(ictx);
    ictx.flags &= !INPUT_LAST;
}
unsafe fn input_handle_decrqss(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
) -> ::core::ffi::c_int {
    let input_pane_owner = ictx.wp.upgrade();
    let mut buf: *mut u_char = ictx.input_buf.as_mut_ptr();
    let mut len: size_t = ictx.input_len;
    let mut ps: ::core::ffi::c_int = 0;
    let mut opt_ps: ::core::ffi::c_int = 0;
    let mut blinking: ::core::ffi::c_int = 0;
    if len < 3 as size_t
        || *buf.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != ' ' as i32
        || *buf.offset(2 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != 'q' as i32
    {
        input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
            out.write_all(b"\x1BP0$r\x1B\\")
        });
        0 as ::core::ffi::c_int
    } else {
        if sctx.screen.cstyle as ::core::ffi::c_uint
            == SCREEN_CURSOR_BLOCK as ::core::ffi::c_int as ::core::ffi::c_uint
            || sctx.screen.cstyle as ::core::ffi::c_uint
                == SCREEN_CURSOR_UNDERLINE as ::core::ffi::c_int as ::core::ffi::c_uint
            || sctx.screen.cstyle as ::core::ffi::c_uint
                == SCREEN_CURSOR_BAR as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            blinking = (sctx.screen.mode & MODE_CURSOR_BLINKING != 0 as ::core::ffi::c_int)
                as ::core::ffi::c_int;
            match sctx.screen.cstyle as ::core::ffi::c_uint {
                1 => {
                    ps = if blinking != 0 {
                        1 as ::core::ffi::c_int
                    } else {
                        2 as ::core::ffi::c_int
                    };
                }
                2 => {
                    ps = if blinking != 0 {
                        3 as ::core::ffi::c_int
                    } else {
                        4 as ::core::ffi::c_int
                    };
                }
                3 => {
                    ps = if blinking != 0 {
                        5 as ::core::ffi::c_int
                    } else {
                        6 as ::core::ffi::c_int
                    };
                }
                _ => {
                    ps = 0 as ::core::ffi::c_int;
                }
            }
        } else {
            opt_ps = if input_pane_owner.is_some() {
                options_get_number(
                    &input_pane_owner
                        .as_ref()
                        .expect("live pane")
                        .borrow_options_mut(),
                    c"cursor-style",
                )
            } else {
                OptionsScope::GlobalWindow.with_local(|oo| options_get_number(oo, c"cursor-style"))
            } as ::core::ffi::c_int;
            if opt_ps < 0 as ::core::ffi::c_int || opt_ps > 6 as ::core::ffi::c_int {
                opt_ps = 0 as ::core::ffi::c_int;
            }
            ps = opt_ps;
        }
        log_debug(format_args!(
            "{}: DECRQSS cursor -> Ps={} (cstyle={} mode={})",
            "input_handle_decrqss",
            (ps) as i32,
            (sctx.screen.cstyle as ::core::ffi::c_uint) as i32,
            log_hex(((sctx.screen.mode) as u32) as u64)
        ));
        input_reply(ictx, 1 as ::core::ffi::c_int, |out| {
            write!(out, "\x1BP1$r q{} q\x1B\\", (ps) as i32)
        });
        0 as ::core::ffi::c_int
    }
}
unsafe fn input_dcs_dispatch(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
) -> ::core::ffi::c_int {
    let input_pane_owner = ictx.wp.upgrade();
    let mut buf: *mut u_char = ictx.input_buf.as_mut_ptr();
    let mut len: size_t = ictx.input_len;
    const PREFIX: &[u8] = b"tmux;";
    let mut allow_passthrough: ::core::ffi::c_longlong = 0 as ::core::ffi::c_longlong;
    if ictx.flags & INPUT_DISCARD != 0 {
        log_debug(format_args!(
            "{}: {} bytes (discard)",
            "input_dcs_dispatch",
            (len) as usize
        ));
        return 0 as ::core::ffi::c_int;
    }
    if ictx.interm_len == 1 as size_t
        && ictx.interm_buf[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int == '$' as i32
        && len >= 1 as size_t
        && *buf.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == 'q' as i32
    {
        return input_handle_decrqss(ictx, sctx);
    }
    allow_passthrough = if input_pane_owner.is_none() {
        OptionsScope::GlobalWindow.with_local(|oo| options_get_number(oo, c"allow-passthrough"))
    } else {
        options_get_number(
            &input_pane_owner
                .as_ref()
                .expect("live pane")
                .borrow_options_mut(),
            c"allow-passthrough",
        )
    };
    if allow_passthrough == 0 {
        return 0 as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: \"{}\"",
        "input_dcs_dispatch",
        log_bytes(&ictx.input_buf)
    ));
    if ictx.input_buf[..len].starts_with(PREFIX) {
        screen_write_rawstring(
            sctx,
            &(&ictx.input_buf)[PREFIX.len()..len],
            (allow_passthrough == 2 as ::core::ffi::c_longlong) as ::core::ffi::c_int,
        );
    }
    0 as ::core::ffi::c_int
}
unsafe fn input_enter_osc(ictx: &mut input_ctx) {
    log_debug(format_args!("{}", "input_enter_osc"));
    input_clear(ictx);
    input_start_ground_timer(ictx);
    ictx.flags &= !INPUT_LAST;
}
unsafe fn input_exit_osc(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    let mut option: u_int = 0;
    if ictx.flags & INPUT_DISCARD != 0 {
        return;
    }
    // The OSC payload stays borrowed from `input_buf` while handlers take the
    // context; none of them touch `input_buf`, so view it through a raw place.
    #[allow(clippy::deref_addrof)]
    let input = CStr::from_bytes_until_nul(&*(&raw const ictx.input_buf))
        .expect("input buffer has a terminator");
    let bytes = input.to_bytes();
    if ictx.input_len < 1 as size_t || !bytes.first().is_some_and(u8::is_ascii_digit) {
        return;
    }
    log_debug(format_args!(
        "{}: \"{}\" (end {})",
        "input_exit_osc",
        log_bytes(&ictx.input_buf),
        log_cstr(
            if ictx.input_end as ::core::ffi::c_uint
                == INPUT_END_ST as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                c"ST"
            } else {
                c"BEL"
            }
        )
    ));
    option = 0 as u_int;
    let mut offset = 0;
    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
        option = option
            .wrapping_mul(10 as u_int)
            .wrapping_add(bytes[offset] as u_int)
            .wrapping_sub('0' as i32 as u_int);
        offset += 1;
    }
    match bytes.get(offset) {
        Some(b';') => offset += 1,
        Some(_) => return,
        None => {}
    }
    let p = cstr_suffix(input, offset);
    match option {
        0 | 2 => {
            if !wp.is_null()
                && options_get_number(
                    &input_pane_owner
                        .as_ref()
                        .expect("live pane")
                        .borrow_options_mut(),
                    c"allow-set-title",
                ) != 0
                && screen_set_title(&mut *sctx.screen, p, 1 as ::core::ffi::c_int) != 0
            {
                input_fire_pane_title_changed(input_pane_owner.as_ref().expect("live pane"), p);
                server_redraw_window_borders(
                    ((*wp).window_handle().as_ref()).expect("live window"),
                );
                server_status_window(((*wp).window_handle().as_ref()).expect("live window"));
            }
        }
        4 => {
            input_osc_4(ictx, sctx, p);
        }
        7 => {
            if !wp.is_null() && screen_set_path(&mut *sctx.screen, p) != 0 {
                server_redraw_window_borders(
                    ((*wp).window_handle().as_ref()).expect("live window"),
                );
                server_status_window(((*wp).window_handle().as_ref()).expect("live window"));
            }
        }
        8 => {
            input_osc_8(ictx, sctx, p);
        }
        9 => {
            input_osc_9(ictx, sctx, p);
        }
        10 => {
            input_osc_10(ictx, sctx, p);
        }
        11 => {
            input_osc_11(ictx, sctx, p);
        }
        12 => {
            input_osc_12(ictx, sctx, p);
        }
        52 => {
            input_osc_52(ictx, sctx, p);
        }
        104 => {
            input_osc_104(ictx, sctx, p);
        }
        110 => {
            input_osc_110(ictx, sctx, p);
        }
        111 => {
            input_osc_111(ictx, sctx, p);
        }
        112 => {
            input_osc_112(sctx, p);
        }
        133 => {
            input_osc_133(ictx, sctx, p);
        }
        _ => {
            log_debug(format_args!(
                "{}: unknown '{}'",
                "input_exit_osc",
                (option) as u32
            ));
        }
    };
}
unsafe fn input_enter_apc(ictx: &mut input_ctx) {
    log_debug(format_args!("{}", "input_enter_apc"));
    input_clear(ictx);
    input_start_ground_timer(ictx);
    ictx.flags &= !INPUT_LAST;
}
unsafe fn input_exit_apc(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    if ictx.flags & INPUT_DISCARD != 0 {
        return;
    }
    log_debug(format_args!(
        "{}: \"{}\"",
        "input_exit_apc",
        log_bytes(&ictx.input_buf)
    ));
    let title = CStr::from_bytes_until_nul(&ictx.input_buf).expect("input buffer has a terminator");
    if !wp.is_null()
        && options_get_number(
            &input_pane_owner
                .as_ref()
                .expect("live pane")
                .borrow_options_mut(),
            c"allow-set-title",
        ) != 0
        && screen_set_title(&mut *sctx.screen, title, 1 as ::core::ffi::c_int) != 0
    {
        input_fire_pane_title_changed(input_pane_owner.as_ref().expect("live pane"), title);
        server_redraw_window_borders(((*wp).window_handle().as_ref()).expect("live window"));
        server_status_window(((*wp).window_handle().as_ref()).expect("live window"));
    }
}
unsafe fn input_enter_rename(ictx: &mut input_ctx) {
    log_debug(format_args!("{}", "input_enter_rename"));
    input_clear(ictx);
    input_start_ground_timer(ictx);
    ictx.flags &= !INPUT_LAST;
}
unsafe fn input_exit_rename(ictx: &mut input_ctx, _sctx: &mut screen_write_ctx) {
    let Some(pane) = ictx.wp.upgrade() else {
        return;
    };
    if ictx.flags & INPUT_DISCARD != 0 {
        return;
    }
    if options_get_number(&pane.borrow_options_mut(), c"allow-rename") == 0 {
        return;
    }
    log_debug(format_args!(
        "{}: \"{}\"",
        "input_exit_rename",
        log_bytes(&ictx.input_buf)
    ));
    let name = CStr::from_bytes_until_nul(&ictx.input_buf).expect("input buffer is terminated");
    if !utf8_isvalid(name) {
        return;
    }
    let name = name.to_owned();
    let window = pane.window_observer().upgrade().expect("pane window");
    if ictx.input_len == 0 {
        let scope = crate::options::OptionsScope::Window(WindowRef::downgrade(&window));
        scope
            .remove_or_default(c"automatic-rename", None)
            .expect("remove local automatic-rename option");
        let automatic = options_get_number(&window.borrow_options_mut(), c"automatic-rename") != 0;
        if !automatic {
            window.rename(c"", true);
        }
    } else {
        options_set_number(&mut window.borrow_options_mut(), c"automatic-rename", 0);
        window.rename(&name, true);
    }
    server_redraw_window_borders(&window);
    server_status_window(&window);
    drop(window);
}
unsafe fn input_top_bit_set(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
) -> ::core::ffi::c_int {
    ictx.flags &= !INPUT_LAST;
    if ictx.utf8started == 0 {
        ictx.utf8started = 1 as ::core::ffi::c_int;
        if utf8_open(&mut ictx.utf8data, ictx.ch as u_char) as ::core::ffi::c_uint
            != UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            input_stop_utf8(ictx, sctx);
        }
        return 0 as ::core::ffi::c_int;
    }
    match utf8_append(&mut ictx.utf8data, ictx.ch as u_char) as ::core::ffi::c_uint {
        0 => return 0 as ::core::ffi::c_int,
        2 => {
            input_stop_utf8(ictx, sctx);
            return 0 as ::core::ffi::c_int;
        }
        _ => {}
    }
    ictx.utf8started = 0 as ::core::ffi::c_int;
    log_debug(format_args!(
        "{} {} '{}' (width {})",
        "input_top_bit_set",
        (ictx.utf8data.size as ::core::ffi::c_int) as u8,
        log_cstr_width(
            CStr::from_bytes_until_nul(&ictx.utf8data.data).unwrap_or_default(),
            ictx.utf8data.size as ::core::ffi::c_int,
        ),
        (ictx.utf8data.width as ::core::ffi::c_int) as u8
    ));
    ictx.cell.cell.data = utf8_copy(&ictx.utf8data);
    screen_write_collect_add(sctx, &ictx.cell.cell);
    ictx.last = utf8_copy(&ictx.cell.cell.data);
    ictx.flags |= INPUT_LAST;
    0 as ::core::ffi::c_int
}
unsafe fn input_osc_colour_reply(
    ictx: &mut input_ctx,
    mut add: ::core::ffi::c_int,
    mut n: u_int,
    mut idx: ::core::ffi::c_int,
    mut c: ::core::ffi::c_int,
    mut end_type: input_end_type,
) {
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if c != -(1 as ::core::ffi::c_int) {
        c = colour_force_rgb(c);
    }
    if c == -(1 as ::core::ffi::c_int) {
        return;
    }
    (r, g, b) = colour_split_rgb(c);
    if end_type as ::core::ffi::c_uint == INPUT_END_BEL as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        end = c"\x07".as_ptr();
    } else {
        end = c"\x1B\\".as_ptr();
    }
    if n == 4 as u_int {
        input_reply(ictx, add, |out| {
            write!(
                out,
                "\x1B]{};{};rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}",
                { n },
                { idx },
                (r as ::core::ffi::c_int) as u8,
                (r as ::core::ffi::c_int) as u8,
                (g as ::core::ffi::c_int) as u8,
                (g as ::core::ffi::c_int) as u8,
                (b as ::core::ffi::c_int) as u8,
                (b as ::core::ffi::c_int) as u8
            )?;
            write_cstr(out, nullable_cstr(end))
        });
    } else {
        input_reply(ictx, add, |out| {
            write!(
                out,
                "\x1B]{};rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}",
                { n },
                (r as ::core::ffi::c_int) as u8,
                (r as ::core::ffi::c_int) as u8,
                (g as ::core::ffi::c_int) as u8,
                (g as ::core::ffi::c_int) as u8,
                (b as ::core::ffi::c_int) as u8,
                (b as ::core::ffi::c_int) as u8
            )?;
            write_cstr(out, nullable_cstr(end))
        });
    };
}
unsafe fn input_osc_4(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let mut s: Option<&CStr> = Some(p);
    let mut idx: ::core::ffi::c_long = 0;
    let mut c: ::core::ffi::c_int = 0;
    let mut bad: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut redraw: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    while let Some(field) = s.filter(|field| !field.is_empty()) {
        let mut end: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
        idx = strtol(field.as_ptr(), &raw mut end, 10 as ::core::ffi::c_int);
        let end = end.offset_from(field.as_ptr()) as usize;
        if field.to_bytes().get(end) != Some(&b';')
            || (idx < 0 as ::core::ffi::c_long || idx >= 256 as ::core::ffi::c_long)
        {
            bad = 1 as ::core::ffi::c_int;
            break;
        } else {
            let rest = cstr_suffix(field, end + 1);
            let (spec, next) = match rest.to_bytes().iter().position(|&byte| byte == b';') {
                Some(semi) => (&rest.to_bytes()[..semi], Some(cstr_suffix(rest, semi + 1))),
                None => (rest.to_bytes(), None),
            };
            if spec == b"?" {
                c = ictx
                    .palette
                    .with_mut(|palette| {
                        colour_palette_get(
                            Some(palette),
                            (idx | COLOUR_FLAG_256 as ::core::ffi::c_long) as ::core::ffi::c_int,
                        )
                    })
                    .unwrap_or(-1);
                if c != -(1 as ::core::ffi::c_int) {
                    input_osc_colour_reply(
                        ictx,
                        1 as ::core::ffi::c_int,
                        4 as u_int,
                        idx as ::core::ffi::c_int,
                        c,
                        ictx.input_end,
                    );
                    s = next;
                } else {
                    input_add_request(ictx, INPUT_REQUEST_PALETTE, idx as ::core::ffi::c_int);
                    s = next;
                }
            } else {
                let spec = CString::new(spec).expect("OSC 4 colour ends before the first NUL");
                c = colour_parse_x11_logged(&spec).unwrap_or(-1);
                if c == -(1 as ::core::ffi::c_int) {
                    s = next;
                } else {
                    if ictx
                        .palette
                        .with_mut(|palette| {
                            colour_palette_set(Some(palette), idx as ::core::ffi::c_int, c)
                        })
                        .unwrap_or(0)
                        != 0
                    {
                        redraw = 1 as ::core::ffi::c_int;
                    }
                    s = next;
                }
            }
        }
    }
    if bad != 0 {
        log_debug(format_args!("bad OSC 4: {}", log_cstr(p)));
    }
    if redraw != 0 {
        screen_write_fullredraw(sctx);
    }
}
fn input_osc_8(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let hl = sctx
        .screen
        .hyperlinks
        .as_ref()
        .expect("screen hyperlink table");
    let bytes = p.to_bytes();
    let mut start = 0;
    let mut id: Option<std::ffi::CString> = None;
    let uri = loop {
        let Some(end) = bytes[start..]
            .iter()
            .position(|&byte| byte == b':' || byte == b';')
            .map(|end| start + end)
        else {
            break None;
        };
        let field = &bytes[start..end];
        if field.len() >= 4 && field.starts_with(b"id=") {
            if id.is_some() {
                break None;
            }
            id = Some(
                std::ffi::CString::new(&field[3..]).expect("OSC 8 ID ends before the first NUL"),
            );
        }
        if bytes[end] == b';' {
            break Some(cstr_suffix(p, end + 1));
        }
        start = end + 1;
    };
    if let Some(uri) = uri {
        if uri.is_empty() {
            ictx.cell.cell.link = 0 as u_int;
            return;
        }
        ictx.cell.cell.link = hyperlinks_put(hl, uri, id.as_deref());
        match id.as_deref() {
            None => log_debug(format_args!(
                "hyperlink (anonymous) {} = {}",
                log_cstr(uri),
                { ictx.cell.cell.link }
            )),
            Some(id) => log_debug(format_args!(
                "hyperlink (id={}) {} = {}",
                log_cstr(id),
                log_cstr(uri),
                { ictx.cell.cell.link }
            )),
        }
        return;
    }
    log_debug(format_args!("bad OSC 8 {}", log_cstr(p)));
}
unsafe fn input_set_progress_bar(
    ictx: &mut input_ctx,
    sctx: &mut screen_write_ctx,
    mut state: progress_bar_state,
    mut p: ::core::ffi::c_int,
) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    screen_set_progress_bar(&mut *sctx.screen, state, p);
    if !input_pane.is_null() {
        server_redraw_window_borders(
            ((*input_pane).window_handle().as_ref()).expect("live window"),
        );
        server_status_window(((*input_pane).window_handle().as_ref()).expect("live window"));
    }
}
unsafe fn input_osc_9(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let pb = p.to_bytes_with_nul();
    let mut state: progress_bar_state = PROGRESS_BAR_HIDDEN;
    let mut progress: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if pb[0] != b'4' {
        return;
    }
    if pb[1] == b'\0' || pb[1] == b';' && pb[2] == b'\0' {
        return;
    }
    if pb[1] != b';' {
        return;
    }
    if (b'0'..=b'4').contains(&pb[2]) {
        state = (pb[2] - b'0') as progress_bar_state;
        if pb[3] == b'\0' || pb[3] == b';' && pb[4] == b'\0' {
            input_set_progress_bar(ictx, sctx, state, -(1 as ::core::ffi::c_int));
            return;
        }
        if pb[3] == b';' {
            let mut i = 4;
            while pb[i].is_ascii_digit() && progress <= 100 as ::core::ffi::c_int {
                progress =
                    progress * 10 as ::core::ffi::c_int + (pb[i] - b'0') as ::core::ffi::c_int;
                i += 1;
            }
            if pb[i] == b'\0' && (0..=100).contains(&progress) {
                input_set_progress_bar(ictx, sctx, state, progress);
                return;
            }
        }
    }
    log_debug(format_args!("bad OSC 9;4 {}", log_cstr(p)));
}
unsafe fn input_osc_10(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    let mut defaults: grid_cell = grid_cell {
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
    let mut c: ::core::ffi::c_int = 0;
    if p == c"?" {
        if wp.is_null() {
            return;
        }
        c = window_pane_get_fg_control_client(input_pane_owner.as_ref().expect("live pane"));
        if c == -(1 as ::core::ffi::c_int) {
            defaults = input_pane_owner
                .as_ref()
                .expect("live pane")
                .default_colours()
                .0;
            if defaults.fg == 8 as ::core::ffi::c_int || defaults.fg == 9 as ::core::ffi::c_int {
                c = window_pane_get_fg(input_pane_owner.as_ref().expect("live pane"));
            } else {
                c = defaults.fg;
            }
        }
        input_osc_colour_reply(
            ictx,
            1 as ::core::ffi::c_int,
            10 as u_int,
            0 as ::core::ffi::c_int,
            c,
            ictx.input_end,
        );
        return;
    }
    c = colour_parse_x11_logged(p).unwrap_or(-1);
    if c == -(1 as ::core::ffi::c_int) {
        log_debug(format_args!("bad OSC 10: {}", log_cstr(p)));
        return;
    }
    if ictx.palette.with_mut(|palette| palette.fg = c).is_some() {
        if !wp.is_null() {
            (*wp).flags |= PANE_STYLECHANGED;
        }
        screen_write_fullredraw(sctx);
    }
}
unsafe fn input_osc_110(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    if !p.is_empty() {
        return;
    }
    if ictx
        .palette
        .with_mut(|palette| palette.fg = 8 as ::core::ffi::c_int)
        .is_some()
    {
        if !wp.is_null() {
            (*wp).flags |= PANE_STYLECHANGED;
        }
        screen_write_fullredraw(sctx);
    }
}
unsafe fn input_osc_11(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    let mut c: ::core::ffi::c_int = 0;
    if p == c"?" {
        if wp.is_null() {
            return;
        }
        c = window_pane_get_bg(input_pane_owner.as_ref().expect("live pane"));
        input_osc_colour_reply(
            ictx,
            1 as ::core::ffi::c_int,
            11 as u_int,
            0 as ::core::ffi::c_int,
            c,
            ictx.input_end,
        );
        return;
    }
    c = colour_parse_x11_logged(p).unwrap_or(-1);
    if c == -(1 as ::core::ffi::c_int) {
        log_debug(format_args!("bad OSC 11: {}", log_cstr(p)));
        return;
    }
    if ictx.palette.with_mut(|palette| palette.bg = c).is_some() {
        if !wp.is_null() {
            (*wp).flags |= PANE_STYLECHANGED | PANE_THEMECHANGED;
        }
        screen_write_fullredraw(sctx);
    }
}
unsafe fn input_osc_111(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    if !p.is_empty() {
        return;
    }
    if ictx
        .palette
        .with_mut(|palette| palette.bg = 8 as ::core::ffi::c_int)
        .is_some()
    {
        if !wp.is_null() {
            (*wp).flags |= PANE_STYLECHANGED | PANE_THEMECHANGED;
        }
        screen_write_fullredraw(sctx);
    }
}
unsafe fn input_osc_12(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let mut c: ::core::ffi::c_int = 0;
    if p == c"?" {
        if input_pane_owner.is_some() {
            c = sctx.screen.ccolour;
            if c == -(1 as ::core::ffi::c_int) {
                c = sctx.screen.default_ccolour;
            }
            input_osc_colour_reply(
                ictx,
                1 as ::core::ffi::c_int,
                12 as u_int,
                0 as ::core::ffi::c_int,
                c,
                ictx.input_end,
            );
        }
        return;
    }
    c = colour_parse_x11_logged(p).unwrap_or(-1);
    if c == -(1 as ::core::ffi::c_int) {
        log_debug(format_args!("bad OSC 12: {}", log_cstr(p)));
        return;
    }
    screen_set_cursor_colour(&mut *sctx.screen, c);
}
fn input_osc_112(sctx: &mut screen_write_ctx, p: &CStr) {
    if p.is_empty() {
        screen_set_cursor_colour(&mut *sctx.screen, -(1 as ::core::ffi::c_int));
    }
}
fn input_osc_133_exit_status(p: &CStr) -> ::core::ffi::c_int {
    let bytes = p.to_bytes();
    if bytes.get(1) != Some(&b';') || bytes.get(2).is_none_or(|&byte| byte == b'=') {
        return 0 as ::core::ffi::c_int;
    }

    let tail = &bytes[2..];
    let end = tail
        .iter()
        .position(|&byte| byte == b';')
        .unwrap_or(tail.len());
    if end == 0 {
        return 0 as ::core::ffi::c_int;
    }
    let number = &tail[..end];
    if number.contains(&b'=') {
        return 0 as ::core::ffi::c_int;
    }

    let copy = CString::new(number).expect("OSC 133 status token contains no NUL");
    match strtonum(&copy, 0, 255) {
        Ok(status) => status as ::core::ffi::c_int,
        Err(_) => 255 as ::core::ffi::c_int,
    }
}

#[cfg(test)]
mod osc_133_exit_status_tests {
    use super::input_osc_133_exit_status;

    #[test]
    fn preserves_numeric_and_parameter_syntax() {
        for (input, expected) in [
            (b"D\0".as_slice(), 0),
            (b"D;\0", 0),
            (b"D;;42\0", 0),
            (b"D;=42\0", 0),
            (b"D;42=ignored\0", 0),
            (b"D;42;k=v\0", 42),
            (b"D;0\0", 0),
            (b"D;255\0", 255),
            (b"D;+7\0", 7),
            (b"D; 7\0", 7),
            (b"D;256\0", 255),
            (b"D;-1\0", 255),
            (b"D;abc\0", 255),
            (b"D;4x\0", 255),
            (b"D;12\0;99\0", 12),
            (b"D;\xff\0", 255),
        ] {
            let input = std::ffi::CStr::from_bytes_until_nul(input).unwrap();
            let actual = input_osc_133_exit_status(input);
            assert_eq!(actual, expected, "input {input:?}");
        }
    }
}
unsafe fn input_fire_command_event(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    name: &CStr,
) {
    let mut wp = wp_owner.as_ptr();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut tstart: time_t = (*wp).cmd_start_time;
    let mut end: time_t = 0;
    let mut tend: time_t = (*wp).cmd_end_time;
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_pane(wp_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    if !fs.session_handle().is_none() {
        event_payload_set_session(
            &mut ep,
            c"session",
            fs.session_handle().expect("live session"),
        );
    }
    if fs.winlink_handle().is_alive() {
        event_payload_set_int(
            &mut ep,
            c"window_index",
            (fs.winlink_handle())
                .try_borrow()
                .expect("live winlink")
                .idx,
        );
    }
    event_payload_set_window(
        &mut ep,
        c"window",
        WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    if (*wp).cmd_status != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"command_status", (*wp).cmd_status);
    }
    if tstart != 0 as time_t {
        event_payload_set_time(&mut ep, c"command_start_time", tstart);
    }
    if tend != 0 as time_t {
        event_payload_set_time(&mut ep, c"command_end_time", tend);
    }
    if tstart != 0 as time_t {
        if (*wp).flags & PANE_CMDRUNNING != 0 {
            end = crate::shared::time::unix_seconds(std::time::SystemTime::now());
        } else {
            end = tend;
        }
        if end < tstart {
            end = tstart;
        }
        end -= tstart;
        event_payload_set_uint(&mut ep, c"command_duration", end as u_int);
    }
    events_fire(name, ep);
}
unsafe fn input_osc_133(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    let cx = sctx.screen.cx;
    let cy = sctx.screen.cy;
    let gd = sctx.screen.grid_mut();
    let mut line: u_int = cy.wrapping_add(gd.hsize);
    let mut status: ::core::ffi::c_int = 0;
    let line = (line < gd.hsize.wrapping_add(gd.sy)).then_some(line);
    match p.to_bytes_with_nul()[0] as ::core::ffi::c_int {
        65 | 78 => {
            if let Some(line) = line {
                let gl = grid_get_line_mut(gd, line);
                gl.osc133_data = osc133_data::default();
                gl.osc133_data.prompt_col = cx as u_short;
                gl.flags = (gl.flags as ::core::ffi::c_int | GRID_LINE_START_PROMPT) as u_short;
            }
            if !wp.is_null() {
                (*wp).last_prompt_time =
                    crate::shared::time::unix_seconds(std::time::SystemTime::now());
                events_fire_pane(
                    c"pane-shell-prompt",
                    std::rc::Rc::clone(input_pane_owner.as_ref().expect("live pane")),
                );
            }
        }
        80 => {
            if let Some(line) = line {
                let gl = grid_get_line_mut(gd, line);
                let bytes = p.to_bytes_with_nul();
                if bytes
                    .windows(4)
                    .position(|window| window == b";k=s")
                    .is_some_and(|cp| bytes[cp + 4] == b';' || bytes[cp + 4] == b'\0')
                {
                    gl.flags =
                        (gl.flags as ::core::ffi::c_int | GRID_LINE_SECOND_PROMPT) as u_short;
                } else {
                    gl.flags = (gl.flags as ::core::ffi::c_int | GRID_LINE_START_PROMPT) as u_short;
                }
                gl.osc133_data.prompt_col = cx as u_short;
            }
        }
        66 | 73 => {
            if let Some(line) = line {
                let gl = grid_get_line_mut(gd, line);
                gl.flags = (gl.flags as ::core::ffi::c_int | GRID_LINE_START_COMMAND) as u_short;
                gl.osc133_data.cmd_col = cx as u_short;
            }
        }
        67 => {
            if let Some(line) = line {
                let gl = grid_get_line_mut(gd, line);
                gl.flags = (gl.flags as ::core::ffi::c_int | GRID_LINE_START_OUTPUT) as u_short;
                gl.osc133_data.out_start_col = cx as u_short;
            }
            if !wp.is_null() {
                (*wp).cmd_start_time =
                    crate::shared::time::unix_seconds(std::time::SystemTime::now());
                (*wp).cmd_end_time = 0 as time_t;
                (*wp).flags |= PANE_CMDRUNNING;
                (*wp).cmd_status = -(1 as ::core::ffi::c_int);
                input_fire_command_event(
                    input_pane_owner.as_ref().expect("live pane"),
                    c"pane-command-started",
                );
            }
        }
        68 => {
            status = input_osc_133_exit_status(p);
            if !wp.is_null() {
                (*wp).cmd_end_time =
                    crate::shared::time::unix_seconds(std::time::SystemTime::now());
                (*wp).flags &= !PANE_CMDRUNNING;
                (*wp).cmd_status = status;
                input_fire_command_event(
                    input_pane_owner.as_ref().expect("live pane"),
                    c"pane-command-finished",
                );
            }
            if let Some(line) = line {
                let gl = grid_get_line_mut(gd, line);
                gl.flags = (gl.flags as ::core::ffi::c_int | GRID_LINE_END_OUTPUT) as u_short;
                gl.osc133_data.out_end_col = cx as u_short;
                gl.osc133_data.exit_status = status as u_char;
            }
        }
        _ => {}
    };
}
unsafe fn input_osc_52_reply(ictx: &mut input_ctx, mut clip: ::core::ffi::c_char) {
    let mut state: ::core::ffi::c_int = 0;
    state = OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"get-clipboard"))
        as ::core::ffi::c_int;
    if state == 0 as ::core::ffi::c_int {
        return;
    }
    if state == 1 as ::core::ffi::c_int {
        let Some(pb) = paste_get_top(None) else {
            return;
        };
        let buffer = pb.borrow();
        let buf = paste_buffer_data(&buffer).unwrap_or_default();
        if ictx.input_end as ::core::ffi::c_uint
            == INPUT_END_BEL as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            let _ = ictx
                .event
                .with_ptr(|event| unsafe { input_reply_clipboard(event, buf, c"\x07", clip) });
        } else {
            let _ = ictx
                .event
                .with_ptr(|event| unsafe { input_reply_clipboard(event, buf, c"\x1B\\", clip) });
        }
        return;
    }
    input_add_request(
        ictx,
        INPUT_REQUEST_CLIPBOARD,
        ictx.input_end as ::core::ffi::c_int,
    );
}
unsafe fn input_osc_52_parse(
    ictx: &mut input_ctx,
    p: &CStr,
    clip: &mut [u8; 13],
) -> Option<Vec<u8>> {
    let allow = b"cpqs01234567";
    let mut j: usize = 0;
    if OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"set-clipboard"))
        != 2 as ::core::ffi::c_longlong
    {
        return None;
    }
    let bytes = p.to_bytes();
    let semi = bytes.iter().position(|&byte| byte == b';')?;
    let end = cstr_suffix(p, semi + 1);
    if end.is_empty() {
        return None;
    }
    log_debug(format_args!("{}: {}", "input_osc_52_parse", log_cstr(end)));
    for &byte in &bytes[..=semi] {
        if allow.contains(&byte) && !clip[..j].contains(&byte) {
            clip[j] = byte;
            j += 1;
        }
    }
    log_debug(format_args!(
        "{}: {} {}",
        "input_osc_52_parse",
        log_cstr_n(&bytes[..semi]),
        log_cstr_n(&clip[..j])
    ));
    if end == c"?" {
        input_osc_52_reply(ictx, clip[0] as ::core::ffi::c_char);
        return None;
    }
    if end.is_empty() {
        return None;
    }
    base64::decode(end.to_bytes())
}
unsafe fn input_osc_52(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let input_pane_owner = ictx.wp.upgrade();
    let mut clip = [0u8; 13];
    let Some(out) = input_osc_52_parse(ictx, p, &mut clip) else {
        return;
    };
    let clip = std::ffi::CStr::from_bytes_until_nul(&clip).expect("terminated clipboard selectors");
    if input_pane_owner.is_none() {
        let Some(client) = ictx.c.upgrade() else {
            return;
        };
        tty_set_selection(&client, clip, &out);
        paste_add_owned(None, out.into_boxed_slice());
    } else {
        // The selection carries no screen state, so a fresh context on the
        // screen the parser is writing reaches the pane's clients whether or
        // not a mode is displayed.
        let mut ctx = screen_write_start_pane(
            input_pane_owner.as_ref().expect("live input pane"),
            &mut *sctx.screen,
        );
        screen_write_setselection(&mut ctx, clip, &out);
        screen_write_stop(ctx);
        events_fire_pane(
            c"pane-set-clipboard",
            std::rc::Rc::clone(input_pane_owner.as_ref().expect("live pane")),
        );
        paste_add_owned(None, out.into_boxed_slice());
    };
}
unsafe fn input_osc_104(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, p: &CStr) {
    let mut idx: ::core::ffi::c_long = 0;
    let mut bad: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut redraw: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if p.is_empty() {
        let _ = ictx
            .palette
            .with_mut(|palette| colour_palette_clear(Some(palette)));
        screen_write_fullredraw(sctx);
        return;
    }
    let mut s = p;
    while !s.is_empty() {
        let mut end: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
        idx = strtol(s.as_ptr(), &raw mut end, 10 as ::core::ffi::c_int);
        let end = end.offset_from(s.as_ptr()) as usize;
        s = cstr_suffix(s, end);
        if !s.is_empty() && s.to_bytes()[0] != b';'
            || (idx < 0 as ::core::ffi::c_long || idx >= 256 as ::core::ffi::c_long)
        {
            bad = 1 as ::core::ffi::c_int;
            break;
        } else {
            if ictx
                .palette
                .with_mut(|palette| {
                    colour_palette_set(
                        Some(palette),
                        idx as ::core::ffi::c_int,
                        -(1 as ::core::ffi::c_int),
                    )
                })
                .unwrap_or(0)
                != 0
            {
                redraw = 1 as ::core::ffi::c_int;
            }
            if s.to_bytes().first() == Some(&b';') {
                s = cstr_suffix(s, 1);
            }
        }
    }
    if bad != 0 {
        log_debug(format_args!("bad OSC 104: {}", log_cstr(p)));
    }
    if redraw != 0 {
        screen_write_fullredraw(sctx);
    }
}
pub unsafe fn input_reply_clipboard(
    mut bev: *mut bufferevent,
    buf: &[u8],
    end: &CStr,
    mut clip: ::core::ffi::c_char,
) {
    if buf.len()
        >= (INT_MAX as size_t)
            .wrapping_mul(3 as size_t)
            .wrapping_div(4 as size_t)
            .wrapping_sub(1 as size_t)
    {
        return;
    }
    let out = base64::encode(buf);
    bufferevent_write(bev, b"\x1B]52;");
    if clip as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
        bufferevent_write(bev, &[clip as u8]);
    }
    bufferevent_write(bev, b";");
    if !out.is_empty() {
        bufferevent_write(bev, &out);
    }
    bufferevent_write(bev, end.to_bytes());
}
pub unsafe fn input_set_buffer_size(mut buffer_size: size_t) {
    log_debug(format_args!(
        "{}: {} -> {}",
        "input_set_buffer_size",
        (input_buffer_size) as ::core::ffi::c_ulong,
        (buffer_size) as ::core::ffi::c_ulong
    ));
    input_buffer_size = buffer_size;
}
unsafe fn input_request_timer_callback(ictx: *mut input_ctx) {
    let mut t: uint64_t = get_timer();
    for ir in input_ctx_request_handles(ictx) {
        if !input_ctx_requests(ictx)
            .iter()
            .any(|owner| std::ptr::eq(&**owner, ir))
        {
            continue;
        }
        if !((*ir).t >= t.wrapping_sub(INPUT_REQUEST_TIMEOUT as uint64_t)) {
            if (*ir).type_0 as ::core::ffi::c_uint
                == INPUT_REQUEST_QUEUE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                input_send_reply(
                    &mut *(*ir).ictx,
                    (*ir)
                        .data
                        .as_deref()
                        .expect("queued input request has no reply data"),
                );
            }
            input_free_request(ir);
        }
    }
    if !(*ictx).requests.is_empty() {
        input_start_request_timer(&mut *ictx);
    }
}
unsafe fn input_start_request_timer(ictx: &mut input_ctx) {
    let tv = Duration::from_micros(100000);
    drop(ictx.request_timer.take());
    let ictx_ptr: *mut input_ctx = ictx;
    ictx.request_timer = Some(
        Timer::new(tv, move || unsafe {
            input_request_timer_callback(ictx_ptr)
        })
        .expect("arm timer"),
    );
}
unsafe fn input_make_request(
    ictx: &mut input_ctx,
    mut type_0: input_request_type,
) -> *mut input_request {
    let mut owner = input_request::new();
    let ir = &mut *owner as *mut input_request;
    (*ir).type_0 = type_0;
    (*ir).ictx = ictx;
    (*ir).t = get_timer();
    if ictx.requests.is_empty() {
        input_start_request_timer(ictx);
    }
    ictx.requests.push_back(owner);
    ir
}
pub(crate) unsafe fn input_free_request(mut ir: *mut input_request) {
    let mut ictx: *mut input_ctx = (*ir).ictx;
    if let Some(client) = (*ir).c.upgrade() {
        client.forget_input_request(&*ir);
    }
    let requests = input_ctx_requests(ictx);
    let index = requests
        .iter()
        .position(|owner| std::ptr::eq(&**owner, ir))
        .expect("request missing from its input context owner");
    drop(requests.remove(index).unwrap());
}
unsafe fn input_add_request(
    ictx: &mut input_ctx,
    mut type_0: input_request_type,
    mut idx: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let input_pane_owner = ictx.wp.upgrade();
    let mut c: Option<ClientRef> = None;
    let mut loop_0: Option<ClientRef> = None;
    let mut ir: *mut input_request = ::core::ptr::null_mut::<input_request>();
    if input_pane_owner.is_none() {
        return -(1 as ::core::ffi::c_int);
    }
    let window = input_pane_owner
        .as_ref()
        .expect("input pane")
        .window_observer()
        .upgrade()
        .expect("pane window");
    let mut registry_loop_0_owner = ClientRegistry::first();
    loop_0 = registry_loop_0_owner.clone();
    while !loop_0.is_none() {
        if !(loop_0.as_ref().expect("live client").flags() & CLIENT_UNATTACHEDFLAGS as uint64_t
            != 0)
            && !(loop_0
                .as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .is_none()
                || (loop_0
                    .as_ref()
                    .expect("live client")
                    .attached_session()
                    .upgrade()
                    .expect("live session")
                    .contains_window(&window) as i32)
                    == 0)
            && loop_0.as_ref().expect("live client").terminal_started()
            && (c.is_none()
                || (loop_0.as_ref().expect("live client").activity_time()
                    > c.as_ref().expect("live client").activity_time()))
        {
            c = loop_0;
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    drop(window);
    if c.is_none() {
        return -(1 as ::core::ffi::c_int);
    }
    ir = input_make_request(ictx, type_0);
    (*ir).c = std::rc::Rc::downgrade(c.as_ref().expect("live client"));
    (*ir).idx = idx;
    (*ir).end = ictx.input_end;
    c.as_ref()
        .expect("live client")
        .observe_input_request(&mut *ir);
    match type_0 as ::core::ffi::c_uint {
        0 => {
            c.as_ref()
                .expect("live client")
                .write_terminal(&format_bytes(format_args!("\x1B]4;{};?\x1B\\", { idx })));
        }
        1 => {
            crate::tty::tty_request_clipboard(c.as_ref().expect("live client"));
        }
        _ => {}
    }
    0 as ::core::ffi::c_int
}
unsafe fn input_request_palette_reply(ir: *mut input_request, pd: &input_request_palette_data) {
    input_osc_colour_reply(
        &mut *(*ir).ictx,
        0 as ::core::ffi::c_int,
        4 as u_int,
        pd.idx,
        pd.c,
        (*ir).end,
    );
}
unsafe fn input_request_clipboard_reply(ir: *mut input_request, cd: &input_request_clipboard_data) {
    let mut ictx: *mut input_ctx = (*ir).ictx;
    let mut state: ::core::ffi::c_int = 0;
    state = OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"get-clipboard"))
        as ::core::ffi::c_int;
    if state == 0 as ::core::ffi::c_int || state == 1 as ::core::ffi::c_int {
        return;
    }
    if state == 3 as ::core::ffi::c_int && !cd.data.is_empty() {
        let owned: Box<[u8]> = cd.data.as_slice().into();
        paste_add_owned(None, owned);
    }
    if (*ir).idx == INPUT_END_BEL as ::core::ffi::c_int {
        let _ = (*ictx)
            .event
            .with_ptr(|event| unsafe { input_reply_clipboard(event, &cd.data, c"\x07", cd.clip) });
    } else {
        let _ = (*ictx).event.with_ptr(|event| unsafe {
            input_reply_clipboard(event, &cd.data, c"\x1B\\", cd.clip)
        });
    };
}
#[derive(Clone, Copy)]
pub enum InputRequestReply<'a> {
    Palette(&'a input_request_palette_data),
    Clipboard(&'a input_request_clipboard_data),
}

pub unsafe fn input_request_reply(client: &ClientRef, reply: InputRequestReply<'_>) {
    client.reply_input_request(reply);
}

pub(crate) fn input_request_matches(request: &input_request, reply: InputRequestReply<'_>) -> bool {
    match reply {
        InputRequestReply::Palette(data) => {
            request.type_0 == INPUT_REQUEST_PALETTE && request.idx == data.idx
        }
        InputRequestReply::Clipboard(_) => request.type_0 == INPUT_REQUEST_CLIPBOARD,
    }
}

pub(crate) unsafe fn input_complete_request(
    found: *mut input_request,
    reply: InputRequestReply<'_>,
) {
    let mut complete = 0;
    let ictx = (*found).ictx;
    for ir in input_ctx_request_handles(ictx) {
        if !input_ctx_requests(ictx)
            .iter()
            .any(|owner| std::ptr::eq(&**owner, ir))
        {
            continue;
        }
        if complete != 0
            && (*ir).type_0 as ::core::ffi::c_uint
                != INPUT_REQUEST_QUEUE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            break;
        }
        if (*ir).type_0 as ::core::ffi::c_uint
            == INPUT_REQUEST_QUEUE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            input_send_reply(
                &mut *(*ir).ictx,
                (*ir)
                    .data
                    .as_deref()
                    .expect("queued input request has no reply data"),
            );
        } else if ir == found {
            match reply {
                InputRequestReply::Palette(pd) => input_request_palette_reply(ir, pd),
                InputRequestReply::Clipboard(cd) => input_request_clipboard_reply(ir, cd),
            }
            complete = 1 as ::core::ffi::c_int;
        }
        input_free_request(ir);
    }
}
unsafe fn input_report_current_theme(ictx: &mut input_ctx) {
    let input_pane_owner = ictx.wp.upgrade();
    let input_pane = input_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |pane| pane.as_ptr());
    let mut wp: *mut window_pane = input_pane;
    if !wp.is_null() {
        (*wp).last_theme = window_pane_get_theme(input_pane_owner.as_ref());
        (*wp).flags &= !PANE_THEMECHANGED;
        match (*wp).last_theme as ::core::ffi::c_uint {
            2 => {
                log_debug(format_args!(
                    "{}: %{} dark theme",
                    "input_report_current_theme",
                    ((*wp).id) as u32
                ));
                input_reply(ictx, 0 as ::core::ffi::c_int, |out| {
                    out.write_all(b"\x1B[?997;1n")
                });
            }
            1 => {
                log_debug(format_args!(
                    "{}: %{} light theme",
                    "input_report_current_theme",
                    ((*wp).id) as u32
                ));
                input_reply(ictx, 0 as ::core::ffi::c_int, |out| {
                    out.write_all(b"\x1B[?997;2n")
                });
            }
            0 => {
                log_debug(format_args!(
                    "{}: %{} unknown theme",
                    "input_report_current_theme",
                    ((*wp).id) as u32
                ));
            }
            _ => {}
        }
    }
}
