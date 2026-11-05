//! Pane-owned input parsing and independent screen-parser components.
use super::{
    window_pane_get_bg, window_pane_get_fg, window_pane_get_fg_control_client,
    window_pane_get_theme, window_pane_update_used_data,
};
use crate::alerts::alerts_queue;
use crate::events::{events_fire, events_fire_pane};
use crate::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_session,
    event_payload_set_string, event_payload_set_target, event_payload_set_time,
    event_payload_set_uint, event_payload_set_window,
};
use crate::format::bytes::{format_bytes, format_message_with, write_cstr};
use crate::grid::{
    grid_cells_look_equal, grid_default_cell, grid_get_cell, grid_get_line_mut, grid_set_tab,
};
use crate::hyperlinks::hyperlinks_put;
use crate::log::{log_bytes, log_cstr, log_cstr_n, log_cstr_width, log_debug, log_hex};
use crate::options::{OptionsScope, options_get_number, options_set_number};
use crate::paste::{paste_add_owned, paste_buffer_data, paste_get_top};
use crate::reactor::{BufferEvent, bufferevent_write};
use crate::screen::{
    screen_clear_tabs, screen_has_tab, screen_pop_title, screen_push_title,
    screen_set_cursor_colour, screen_set_cursor_style, screen_set_path, screen_set_progress_bar,
    screen_set_tab, screen_set_title,
};
use crate::screen_write::{
    screen_write_alignmenttest, screen_write_alternateoff, screen_write_alternateon,
    screen_write_backspace, screen_write_carriagereturn, screen_write_clearcharacter,
    screen_write_clearendofline, screen_write_clearendofscreen, screen_write_clearhistory,
    screen_write_clearline, screen_write_clearscreen, screen_write_clearstartofline,
    screen_write_clearstartofscreen, screen_write_collect_add, screen_write_collect_add_run,
    screen_write_collect_end, screen_write_cursordown, screen_write_cursorleft,
    screen_write_cursormove, screen_write_cursorright, screen_write_cursorup,
    screen_write_deletecharacter, screen_write_deleteline, screen_write_end_sync,
    screen_write_fullredraw, screen_write_insertcharacter, screen_write_insertline,
    screen_write_linefeed, screen_write_mode_clear, screen_write_mode_set, screen_write_rawstring,
    screen_write_reset, screen_write_reverseindex, screen_write_scrolldown,
    screen_write_scrollregion, screen_write_scrollup, screen_write_setselection,
    screen_write_start_base, screen_write_start_beside, screen_write_start_callback,
    screen_write_stop,
};
use crate::server_client::{Client as _, ClientRegistry};
use crate::server_fn::{server_redraw_window_borders, server_status_window};
use crate::session::Session as _;
use crate::shared::abi::*;
use crate::shared::client::{CLIENT_UNATTACHEDFLAGS, ClientRef};
use crate::shared::colour::COLOUR_FLAG_256;
use crate::shared::command::cmd_find_state;
use crate::shared::display::*;
use crate::shared::event::*;
use crate::shared::grid::*;
use crate::shared::input::{
    INPUT_REQUEST_CLIPBOARD, INPUT_REQUEST_PALETTE, INPUT_REQUEST_QUEUE, input_ctx, input_end_type,
    input_request, input_request_clipboard_data, input_request_palette_data, input_request_type,
};
use crate::shared::limits::INT_MAX;
use crate::shared::pane::{
    PANE_ACTIVITY, PANE_CHANGED, PANE_CMDRUNNING, PANE_STYLECHANGED, PANE_THEMECHANGED,
    PANE_UNSEENCHANGES, pane_output_data as window_pane_get_new_data, window_pane,
};
use crate::shared::screen::{
    ALL_MOUSE_MODES, EXTENDED_KEY_MODES, MODE_BRACKETPASTE, MODE_CRLF, MODE_CURSOR,
    MODE_CURSOR_BLINKING, MODE_CURSOR_BLINKING_SET, MODE_CURSOR_VERY_VISIBLE, MODE_FOCUSON,
    MODE_INSERT, MODE_KCURSOR, MODE_KEYS_EXTENDED, MODE_KEYS_EXTENDED_2, MODE_KKEYPAD,
    MODE_MOUSE_ALL, MODE_MOUSE_BUTTON, MODE_MOUSE_SGR, MODE_MOUSE_STANDARD, MODE_MOUSE_UTF8,
    MODE_ORIGIN, MODE_SYNC, MODE_THEME_UPDATES, MODE_WRAP, screen,
};
use crate::shared::screen_write::{screen_write_ctx, screen_write_init_ctx_cb};
use crate::shared::utf8::*;
use crate::shared::window::{ WINDOW_BELL, WindowRef};
use crate::style::colour::{
    colour_force_rgb, colour_join_rgb, colour_palette_clear, colour_palette_get,
    colour_palette_set, colour_parse_x11_logged, colour_split_rgb,
};
use crate::text::utf8::{utf8_append, utf8_copy, utf8_isvalid, utf8_open, utf8_set};
use crate::tmux::{get_timer, getversion};
use crate::tty::tty_set_selection;
use crate::window::{Window as _, WindowPane};
use ::core::ffi::{c_int, c_uint};
use std::collections::VecDeque;
use std::ffi::CStr;
use std::time::Duration;
use tiri_vt::{
    INPUT_END_BEL, INPUT_END_ST, Osc133Command, Parser, ProgramStatus, SgrAttr, SgrColor, SgrOp,
    VtHost, WinOp,
};

pub const INPUT_REQUEST_TIMEOUT: ::core::ffi::c_int = 500 as ::core::ffi::c_int;

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
            utf8data: Default::default(),
            utf8started: 0,
            last: Default::default(),
            vt: Parser::new(),
            this: refbox::Weak::new(),
            requests: VecDeque::new(),
            request_timer: Default::default(),
            ground_timer: Default::default(),
        }
    }
}

impl input_request {
    fn new(ictx: &input_ctx, type_0: input_request_type) -> Self {
        Self {
            c: std::rc::Weak::new(),
            ictx: ictx.this.clone(),
            type_0,
            t: get_timer(),
            end: INPUT_END_ST,
            idx: 0,
            data: None,
        }
    }
}

fn input_stop_utf8(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    static RC: utf8_data = utf8_data {
        data: *b"\xEF\xBF\xBD\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        have: 3 as u_char,
        size: 3 as u_char,
        width: 1 as u_char,
    };
    if ictx.utf8started != 0 {
        ictx.cell.cell.data = utf8_copy(&RC);
        screen_write_collect_add(sctx, &ictx.cell.cell);
    }
    ictx.utf8started = 0;
}

fn input_fire_pane_title_changed(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    title: &CStr,
) {
    let mut ep = event_payload_create();
    let fs = cmd_find_state::find_from_pane(wp_owner, 0)
        .unwrap_or_else(|| cmd_find_state::clear_state(0));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    event_payload_set_window(
        &mut ep,
        c"window",
        wp_owner.window_observer().upgrade().expect("live window"),
    );
    event_payload_set_string(&mut ep, c"new_title", |out| write_cstr(out, Some(title)));
    events_fire(c"pane-title-changed", ep);
}

fn input_ground_timer_callback(ictx: &mut input_ctx) {
    log_debug(format_args!(
        "{}: {} expired",
        "input_ground_timer_callback",
        crate::log::log_bytes(ictx.vt.state_name().to_bytes())
    ));
    input_reset(ictx, 0);
}

fn input_start_ground_timer(ictx: &mut input_ctx) {
    let tv = Duration::from_secs(5);
    drop(ictx.ground_timer.take());
    let observer = ictx.this.clone();
    ictx.ground_timer = Some(
        Timer::new(tv, move || {
            input_ground_timer_callback(&mut observer.try_borrow_mut().expect("live input context"))
        })
        .expect("arm timer"),
    );
}

fn input_reset_cell(ictx: &mut input_ctx) {
    ictx.cell.cell = grid_default_cell;
    ictx.cell.set = 0;
    ictx.cell.g1set = 0;
    ictx.cell.g0set = ictx.cell.g1set;
    ictx.old_cell = ictx.cell;
    ictx.old_cx = 0;
    ictx.old_cy = 0;
}

fn input_save_state(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    ictx.old_cell = ictx.cell;
    ictx.old_cx = sctx.with_screen(|s| s.cx);
    ictx.old_cy = sctx.with_screen(|s| s.cy);
    ictx.old_mode = sctx.with_screen(|s| s.mode);
}

fn input_restore_state(ictx: &mut input_ctx, sctx: &mut screen_write_ctx) {
    ictx.cell = ictx.old_cell;
    if ictx.old_mode & MODE_ORIGIN != 0 {
        screen_write_mode_set(sctx, MODE_ORIGIN);
    } else {
        screen_write_mode_clear(sctx, MODE_ORIGIN);
    }
    screen_write_cursormove(sctx, ictx.old_cx as c_int, ictx.old_cy as c_int, 0);
}

pub fn input_init(
    wp: Option<&std::rc::Rc<std::cell::RefCell<window_pane>>>,
    bev: crate::reactor::StreamHandle,
    palette: crate::shared::input::InputPalette,
    c: Option<&ClientRef>,
) -> refbox::RefBox<input_ctx> {
    let owner = refbox::RefBox::new(input_ctx::new());
    let mut ictx = owner.try_borrow_mut().expect("new input context");
    ictx.this = owner.downgrade();
    ictx.wp = wp.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    ictx.event = bev;
    ictx.palette = palette;
    ictx.c = c.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    input_reset(&mut ictx, 0);
    drop(ictx);
    owner
}

impl Drop for input_ctx {
    fn drop(&mut self) {
        while !self.requests.is_empty() {
            drop(input_free_request(self, 0));
        }
        if let Some(pane) = self.wp.upgrade() {
            pane.stop_sync();
        }
    }
}

pub fn input_reset(ictx: &mut input_ctx, clear: ::core::ffi::c_int) {
    let input_pane_owner = ictx.wp.upgrade();
    input_reset_cell(ictx);
    if let Some(pane) = input_pane_owner.as_ref().filter(|_| clear != 0) {
        let modes_empty = pane.borrow().modes.is_empty();
        let mut ctx = screen_write_start_base(pane, modes_empty);
        screen_write_reset(&mut ctx);
        screen_write_stop(ctx);
    }
    ictx.ground_timer = None;
    ictx.vt.reset();
}

pub fn input_pending(ictx: &input_ctx) -> &[u8] {
    ictx.vt.pending()
}

fn input_parse(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, buf: &[u8]) {
    let mut vt = std::mem::take(&mut ictx.vt);
    let mut host = ScreenVtHost { ictx, sctx };
    vt.parse(&mut host, buf);
    ictx.vt = vt;
}

/// Parse the pane's new output into `write`, which the caller stops once
/// it has parsed all the output it will this turn.
pub(super) fn input_parse_pane(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    write: &mut Option<screen_write_ctx<'static>>,
) {
    let (stream, base, offset) = {
        let pane = wp_owner.borrow();
        (pane.event.clone(), pane.base_offset, pane.offset)
    };
    let data = stream
        .with_stream(|event| window_pane_get_new_data(&event.input, base, &offset))
        .unwrap_or_default();
    if !data.is_empty() {
        wp_owner.borrow_mut().last_output_time =
            crate::shared::time::unix_seconds(std::time::SystemTime::now());
    }
    input_parse_into(wp_owner, &data, write);
    window_pane_update_used_data(wp_owner, |wp| &mut wp.offset, data.len());
}

pub(super) fn input_parse_buffer(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    buf: &[u8],
) {
    let mut write = None;
    input_parse_into(wp_owner, buf, &mut write);
    if let Some(ctx) = write {
        screen_write_stop(ctx);
    }
}

fn input_parse_into(
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    buf: &[u8],
    write: &mut Option<screen_write_ctx<'static>>,
) {
    if buf.is_empty() {
        return;
    }
    let (ictx, window) = {
        let mut wp = wp_owner.borrow_mut();
        wp.output_generation = wp.output_generation.wrapping_add(1);
        (
            wp.ictx
                .as_ref()
                .map(refbox::RefBox::downgrade)
                .expect("pane input context"),
            wp.window_handle().expect("live window"),
        )
    };
    let mut ictx = ictx
        .try_borrow_mut()
        .expect("pane input context not already borrowed");
    let ictx = &mut *ictx;
    window.update_activity();
    drop(window);
    let first_activity = {
        let mut wp = wp_owner.borrow_mut();
        let first = wp.flags & PANE_ACTIVITY == 0;
        wp.flags |= PANE_ACTIVITY;
        first
    };
    if first_activity {
        events_fire_pane(c"pane-activity", std::rc::Rc::clone(wp_owner));
    }
    let (id, modes_empty) = {
        let mut wp = wp_owner.borrow_mut();
        wp.flags |= PANE_CHANGED;
        if !wp.modes.is_empty() {
            wp.flags |= PANE_UNSEENCHANGES;
        }
        (wp.id, wp.modes.is_empty())
    };
    let observed = write.as_ref().map(|ctx| ctx.wp.strong_count() != 0);
    if observed != Some(modes_empty) {
        if let Some(ctx) = write.take() {
            screen_write_stop(ctx);
        }
        *write = Some(screen_write_start_base(wp_owner, modes_empty));
    }
    let ctx = write.as_mut().expect("pane screen write");
    log_debug(format_args!(
        "{}: %{} {}, {} bytes: {}",
        "input_parse_buffer",
        id,
        crate::log::log_bytes(ictx.vt.state_name().to_bytes()),
        buf.len(),
        log_cstr_n(buf)
    ));
    input_parse(ictx, ctx, buf);
}

pub fn input_parse_screen(
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

fn input_send_reply(ictx: &mut input_ctx, reply: &CStr) {
    if ictx.event.is_alive() {
        log_debug(format_args!("{}: {}", "input_send_reply", log_cstr(reply)));
        let _ = ictx
            .event
            .with_stream(|event| bufferevent_write(event, reply.to_bytes()));
    }
}

fn input_reply(
    ictx: &mut input_ctx,
    add: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let reply = format_message_with(write);
    if add != 0 && !ictx.requests.is_empty() {
        let mut ir = input_request::new(ictx, INPUT_REQUEST_QUEUE);
        ir.data = Some(reply);
        input_make_request(ictx, ir);
    } else {
        input_send_reply(ictx, &reply);
    }
}

fn input_print(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, byte: u8) {
    input_stop_utf8(ictx, sctx);
    let set = if ictx.cell.set == 0 {
        ictx.cell.g0set
    } else {
        ictx.cell.g1set
    };
    if set == 1 {
        ictx.cell.cell.attr = (ictx.cell.cell.attr as c_int | GRID_ATTR_CHARSET) as u_short;
    } else {
        ictx.cell.cell.attr = (ictx.cell.cell.attr as c_int & !GRID_ATTR_CHARSET) as u_short;
    }
    let mut data = utf8_data {
        data: [0; 32],
        have: 1,
        size: 1,
        width: 1,
    };
    data.data[0] = byte;
    ictx.cell.cell.data = data;
    screen_write_collect_add(sctx, &ictx.cell.cell);
    ictx.last = data;
    ictx.cell.cell.attr = (ictx.cell.cell.attr as c_int & !GRID_ATTR_CHARSET) as u_short;
}

/// Print the run of printable ASCII at the start of `bytes` as
/// [`input_print`] would byte by byte, returning its length; zero when a
/// pending UTF-8 character or the line-drawing set needs the byte path.
fn input_print_run(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, bytes: &[u8]) -> usize {
    let set = if ictx.cell.set == 0 {
        ictx.cell.g0set
    } else {
        ictx.cell.g1set
    };
    if ictx.utf8started != 0 || set == 1 {
        return 0;
    }
    let length = bytes
        .iter()
        .position(|byte| !(0x20..0x7f).contains(byte))
        .unwrap_or(bytes.len());
    let Some(&last) = bytes[..length].last() else {
        return 0;
    };
    ictx.cell.cell.attr &= !(GRID_ATTR_CHARSET as u_short);
    screen_write_collect_add_run(sctx, &ictx.cell.cell, &bytes[..length]);
    utf8_set(&mut ictx.cell.cell.data, last);
    ictx.last = ictx.cell.cell.data;
    length
}

fn input_top_bit_set(ictx: &mut input_ctx, sctx: &mut screen_write_ctx, byte: u8) -> bool {
    if ictx.utf8started == 0 {
        ictx.utf8started = 1;
        if utf8_open(&mut ictx.utf8data, byte) as c_uint != UTF8_MORE as c_uint {
            input_stop_utf8(ictx, sctx);
        }
        return false;
    }
    match utf8_append(&mut ictx.utf8data, byte) as c_uint {
        0 => return false,
        2 => {
            input_stop_utf8(ictx, sctx);
            return false;
        }
        _ => {}
    }
    ictx.utf8started = 0;
    log_debug(format_args!(
        "{} {} '{}' (width {})",
        "input_top_bit_set",
        ictx.utf8data.size,
        log_cstr_width(
            CStr::from_bytes_until_nul(&ictx.utf8data.data).unwrap_or_default(),
            ictx.utf8data.size as c_int,
        ),
        ictx.utf8data.width
    ));
    ictx.cell.cell.data = utf8_copy(&ictx.utf8data);
    screen_write_collect_add(sctx, &ictx.cell.cell);
    ictx.last = utf8_copy(&ictx.cell.cell.data);
    true
}

fn sgr_color_to_cell(color: SgrColor) -> c_int {
    match color {
        SgrColor::Default => 8,
        SgrColor::Ansi(c) => c as c_int,
        SgrColor::Indexed(c) => (c as c_int) | COLOUR_FLAG_256,
        SgrColor::Rgb(r, g, b) => colour_join_rgb(r, g, b),
    }
}

fn input_exit_rename(ictx: &mut input_ctx, input_buf: &[u8], input_len: usize) {
    let Some(pane) = ictx.wp.upgrade() else {
        return;
    };
    if options_get_number(&pane.borrow_options_mut(), c"allow-rename") == 0 {
        return;
    }
    log_debug(format_args!(
        "{}: \"{}\"",
        "input_exit_rename",
        log_bytes(input_buf)
    ));
    let name = CStr::from_bytes_until_nul(input_buf).expect("input buffer is terminated");
    if !utf8_isvalid(name) {
        return;
    }
    let name = name.to_owned();
    let window = pane.window_observer().upgrade().expect("pane window");
    if input_len == 0 {
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

fn input_osc_colour_reply(
    ictx: &mut input_ctx,
    add: ::core::ffi::c_int,
    n: u_int,
    idx: ::core::ffi::c_int,
    mut c: ::core::ffi::c_int,
    end_type: input_end_type,
) {
    if c != -1 {
        c = colour_force_rgb(c);
    }
    if c == -1 {
        return;
    }
    let (r, g, b) = colour_split_rgb(c);
    let end = if end_type == INPUT_END_BEL {
        c"\x07"
    } else {
        c"\x1B\\"
    };
    if n == 4 {
        input_reply(ictx, add, |out| {
            write!(
                out,
                "\x1B]{n};{idx};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}"
            )?;
            write_cstr(out, end)
        });
    } else {
        input_reply(ictx, add, |out| {
            write!(
                out,
                "\x1B]{n};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}"
            )?;
            write_cstr(out, end)
        });
    }
}

fn input_fire_command_event(wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>, name: &CStr) {
    let tstart: time_t = wp_owner.borrow().cmd_start_time;
    let tend: time_t = wp_owner.borrow().cmd_end_time;
    let mut ep = event_payload_create();
    let fs = cmd_find_state::find_from_pane(wp_owner, 0)
        .unwrap_or_else(|| cmd_find_state::clear_state(0));
    event_payload_set_target(&mut ep, &fs);
    if let Some(session) = fs.session_handle() {
        event_payload_set_session(&mut ep, c"session", session);
    }
    if fs.winlink_handle().is_alive() {
        event_payload_set_int(
            &mut ep,
            c"window_index",
            fs.winlink_handle().try_borrow().expect("live winlink").idx,
        );
    }
    event_payload_set_window(
        &mut ep,
        c"window",
        wp_owner.window_observer().upgrade().expect("live window"),
    );
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    let cmd_status = wp_owner.borrow().cmd_status;
    if cmd_status != -1 {
        event_payload_set_int(&mut ep, c"command_status", cmd_status);
    }
    if tstart != 0 {
        event_payload_set_time(&mut ep, c"command_start_time", tstart);
    }
    if tend != 0 {
        event_payload_set_time(&mut ep, c"command_end_time", tend);
    }
    if tstart != 0 {
        let mut end = if wp_owner.borrow().flags & PANE_CMDRUNNING != 0 {
            crate::shared::time::unix_seconds(std::time::SystemTime::now())
        } else {
            tend
        };
        if end < tstart {
            end = tstart;
        }
        end -= tstart;
        event_payload_set_uint(&mut ep, c"command_duration", end as u_int);
    }
    events_fire(name, ep);
}

fn input_report_current_theme(ictx: &mut input_ctx) {
    if let Some(pane) = ictx.wp.upgrade() {
        let theme = window_pane_get_theme(Some(&pane));
        let id = {
            let mut state = pane.borrow_mut();
            state.last_theme = theme;
            state.flags &= !PANE_THEMECHANGED;
            state.id
        };
        match theme {
            2 => {
                log_debug(format_args!(
                    "{}: %{} dark theme",
                    "input_report_current_theme", id
                ));
                input_reply(ictx, 0, |out| out.write_all(b"\x1B[?997;1n"));
            }
            1 => {
                log_debug(format_args!(
                    "{}: %{} light theme",
                    "input_report_current_theme", id
                ));
                input_reply(ictx, 0, |out| out.write_all(b"\x1B[?997;2n"));
            }
            0 => {
                log_debug(format_args!(
                    "{}: %{} unknown theme",
                    "input_report_current_theme", id
                ));
            }
            _ => {}
        }
    }
}

struct ScreenVtHost<'a, 'b> {
    ictx: &'a mut input_ctx,
    sctx: &'a mut screen_write_ctx<'b>,
}

impl VtHost for ScreenVtHost<'_, '_> {
    fn collect_end(&mut self) {
        screen_write_collect_end(self.sctx);
    }

    fn print_ascii_run(&mut self, bytes: &[u8]) -> usize {
        input_print_run(self.ictx, self.sctx, bytes)
    }

    fn print_ascii(&mut self, byte: u8) {
        input_print(self.ictx, self.sctx, byte);
    }

    fn print_utf8_byte(&mut self, byte: u8) -> bool {
        input_top_bit_set(self.ictx, self.sctx, byte)
    }

    fn stop_utf8(&mut self) {
        input_stop_utf8(self.ictx, self.sctx);
    }

    fn repeat_last(&mut self, count: u32) {
        let m = self.sctx.with_screen(|s| s.grid().sx.wrapping_sub(s.cx));
        let n = count.min(m);
        let set = if self.ictx.cell.set == 0 {
            self.ictx.cell.g0set
        } else {
            self.ictx.cell.g1set
        };
        if set == 1 {
            self.ictx.cell.cell.attr =
                (self.ictx.cell.cell.attr as c_int | GRID_ATTR_CHARSET) as u_short;
        } else {
            self.ictx.cell.cell.attr =
                (self.ictx.cell.cell.attr as c_int & !GRID_ATTR_CHARSET) as u_short;
        }
        self.ictx.cell.cell.data = utf8_copy(&self.ictx.last);
        for _ in 0..n {
            screen_write_collect_add(self.sctx, &self.ictx.cell.cell);
        }
    }

    fn start_ground_timer(&mut self) {
        input_start_ground_timer(self.ictx);
    }

    fn clear_ground_timer(&mut self) {
        self.ictx.ground_timer = None;
    }

    fn reset_cell(&mut self) {
        input_reset_cell(self.ictx);
    }

    fn save_state(&mut self) {
        input_save_state(self.ictx, self.sctx);
    }

    fn restore_state(&mut self) {
        input_restore_state(self.ictx, self.sctx);
    }

    fn select_charset(&mut self, set: i32) {
        self.ictx.cell.set = set;
    }

    fn define_charset(&mut self, bank: u8, acs: bool) {
        let val = i32::from(acs);
        if bank == 0 {
            self.ictx.cell.g0set = val;
        } else {
            self.ictx.cell.g1set = val;
        }
    }

    fn apply_sgr(&mut self, op: SgrOp) {
        let cell = &mut self.ictx.cell.cell;
        match op {
            SgrOp::ResetAll => *cell = grid_default_cell,
            SgrOp::ResetKeepLink => {
                let link = cell.link;
                *cell = grid_default_cell;
                cell.link = link;
            }
            SgrOp::SetAttr(attr) => match attr {
                SgrAttr::Bright => cell.attr |= GRID_ATTR_BRIGHT as u_short,
                SgrAttr::Dim => cell.attr |= GRID_ATTR_DIM as u_short,
                SgrAttr::Italics => cell.attr |= GRID_ATTR_ITALICS as u_short,
                SgrAttr::Underscore => {
                    cell.attr = (cell.attr & !(GRID_ATTR_ALL_UNDERSCORE as u_short))
                        | (GRID_ATTR_UNDERSCORE as u_short);
                }
                SgrAttr::Underscore2 => {
                    cell.attr = (cell.attr & !(GRID_ATTR_ALL_UNDERSCORE as u_short))
                        | (GRID_ATTR_UNDERSCORE_2 as u_short);
                }
                SgrAttr::Underscore3 => {
                    cell.attr = (cell.attr & !(GRID_ATTR_ALL_UNDERSCORE as u_short))
                        | (GRID_ATTR_UNDERSCORE_3 as u_short);
                }
                SgrAttr::Underscore4 => {
                    cell.attr = (cell.attr & !(GRID_ATTR_ALL_UNDERSCORE as u_short))
                        | (GRID_ATTR_UNDERSCORE_4 as u_short);
                }
                SgrAttr::Underscore5 => {
                    cell.attr = (cell.attr & !(GRID_ATTR_ALL_UNDERSCORE as u_short))
                        | (GRID_ATTR_UNDERSCORE_5 as u_short);
                }
                SgrAttr::Blink => cell.attr |= GRID_ATTR_BLINK as u_short,
                SgrAttr::Reverse => cell.attr |= GRID_ATTR_REVERSE as u_short,
                SgrAttr::Hidden => cell.attr |= GRID_ATTR_HIDDEN as u_short,
                SgrAttr::Strikethrough => cell.attr |= GRID_ATTR_STRIKETHROUGH as u_short,
                SgrAttr::Overline => cell.attr |= GRID_ATTR_OVERLINE as u_short,
            },
            SgrOp::ClearBrightDim => {
                cell.attr &= !((GRID_ATTR_BRIGHT | GRID_ATTR_DIM) as u_short);
            }
            SgrOp::ClearItalics => cell.attr &= !(GRID_ATTR_ITALICS as u_short),
            SgrOp::ClearAllUnderscore => cell.attr &= !(GRID_ATTR_ALL_UNDERSCORE as u_short),
            SgrOp::ClearBlink => cell.attr &= !(GRID_ATTR_BLINK as u_short),
            SgrOp::ClearReverse => cell.attr &= !(GRID_ATTR_REVERSE as u_short),
            SgrOp::ClearHidden => cell.attr &= !(GRID_ATTR_HIDDEN as u_short),
            SgrOp::ClearStrikethrough => cell.attr &= !(GRID_ATTR_STRIKETHROUGH as u_short),
            SgrOp::ClearOverline => cell.attr &= !(GRID_ATTR_OVERLINE as u_short),
            SgrOp::Fg(color) => cell.fg = sgr_color_to_cell(color),
            SgrOp::Bg(color) => cell.bg = sgr_color_to_cell(color),
            SgrOp::Us(color) => cell.us = sgr_color_to_cell(color),
        }
    }

    fn bell(&mut self) {
        if let Some(pane) = self.ictx.wp.upgrade() {
            events_fire_pane(c"pane-bell", std::rc::Rc::clone(&pane));
            alerts_queue(
                &pane.window_observer().upgrade().expect("live window"),
                WINDOW_BELL,
            );
        }
    }

    fn backspace(&mut self) {
        screen_write_backspace(self.sctx);
    }

    fn horizontal_tab(&mut self) {
        let mut cx = self.sctx.with_screen(|s| s.cx);
        let max_cx = self.sctx.with_screen(|s| s.grid().sx.wrapping_sub(1));
        if cx < max_cx {
            let line = self.sctx.with_screen(|s| s.cy.wrapping_add(s.grid().hsize));
            let mut first_gc = grid_cell::default();
            let mut gc = grid_cell::default();
            let mut has_content = false;
            self.sctx
                .with_screen(|s| grid_get_cell(s.grid(), cx, line, &mut first_gc));
            loop {
                if !has_content {
                    self.sctx
                        .with_screen(|s| grid_get_cell(s.grid(), cx, line, &mut gc));
                    if gc.data.size != 1
                        || gc.data.data[0] != b' '
                        || !grid_cells_look_equal(&gc, &first_gc)
                    {
                        has_content = true;
                    }
                }
                cx = cx.wrapping_add(1);
                if self.sctx.with_screen(|s| screen_has_tab(s, cx)) || cx >= max_cx {
                    break;
                }
            }
            let width = cx.wrapping_sub(self.sctx.with_screen(|s| s.cx));
            if has_content || width as usize > 32 {
                self.sctx.with_screen_mut(|s| s.cx = cx);
            } else {
                self.sctx
                    .with_screen(|s| grid_get_cell(s.grid(), s.cx, line, &mut gc));
                grid_set_tab(&mut gc, width);
                screen_write_collect_add(self.sctx, &gc);
            }
        }
    }

    fn linefeed(&mut self, cr_on_crlf: bool) {
        screen_write_linefeed(self.sctx, 0, self.ictx.cell.cell.bg as u_int);
        if cr_on_crlf && (self.sctx.with_screen(|s| s.mode) & MODE_CRLF != 0) {
            screen_write_carriagereturn(self.sctx);
        }
    }

    fn carriage_return(&mut self) {
        screen_write_carriagereturn(self.sctx);
    }

    fn reverse_index(&mut self) {
        screen_write_reverseindex(self.sctx, self.ictx.cell.cell.bg as u_int);
    }

    fn alignment_test(&mut self) {
        screen_write_alignmenttest(self.sctx);
    }

    fn hard_reset(&mut self) {
        let _ = self
            .ictx
            .palette
            .with_mut(|palette| colour_palette_clear(Some(palette)));
        input_reset_cell(self.ictx);
        screen_write_reset(self.sctx);
        screen_write_fullredraw(self.sctx);
    }

    fn set_tab_at_cursor(&mut self, enabled: bool) {
        if self.sctx.with_screen(|s| s.cx < s.grid().sx) {
            let column = self.sctx.with_screen(|s| s.cx);
            self.sctx
                .with_screen_mut(|s| screen_set_tab(s, column, enabled));
        }
    }

    fn clear_all_tabs(&mut self) {
        self.sctx.with_screen_mut(screen_clear_tabs);
    }

    fn cursor_back_tab(&mut self, mut count: u32) {
        let mut cx = self.sctx.with_screen(|s| s.cx);
        let max_cx = self.sctx.with_screen(|s| s.grid().sx.wrapping_sub(1));
        if cx > max_cx {
            cx = max_cx;
        }
        while cx > 0 && count > 0 {
            count -= 1;
            loop {
                cx = cx.wrapping_sub(1);
                if cx == 0 || self.sctx.with_screen(|s| screen_has_tab(s, cx)) {
                    break;
                }
            }
        }
        self.sctx.with_screen_mut(|s| s.cx = cx);
    }

    fn cursor_up(&mut self, count: u32) {
        screen_write_cursorup(self.sctx, count);
    }

    fn cursor_down(&mut self, count: u32) {
        screen_write_cursordown(self.sctx, count);
    }

    fn cursor_right(&mut self, count: u32) {
        screen_write_cursorright(self.sctx, count);
    }

    fn cursor_left(&mut self, count: u32) {
        screen_write_cursorleft(self.sctx, count);
    }

    fn cursor_move(&mut self, x: i32, y: i32) {
        screen_write_cursormove(self.sctx, x, y, 1);
    }

    fn erase_chars(&mut self, count: u32) {
        screen_write_clearcharacter(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn delete_chars(&mut self, count: u32) {
        screen_write_deletecharacter(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn insert_chars(&mut self, count: u32) {
        screen_write_insertcharacter(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn delete_lines(&mut self, count: u32) {
        screen_write_deleteline(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn insert_lines(&mut self, count: u32) {
        screen_write_insertline(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn scroll_up(&mut self, count: u32) {
        screen_write_scrollup(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn scroll_down(&mut self, count: u32) {
        screen_write_scrolldown(self.sctx, count, self.ictx.cell.cell.bg as u_int);
    }

    fn screen_height(&self) -> u32 {
        self.sctx.with_screen(|s| s.grid().sy)
    }

    fn set_scroll_region(&mut self, upper: u32, lower: u32) {
        screen_write_scrollregion(self.sctx, upper, lower);
    }

    fn erase_in_display(&mut self, mode: i32, submode: i32) {
        let bg = self.ictx.cell.cell.bg as u_int;
        match mode {
            0 => screen_write_clearendofscreen(self.sctx, bg),
            1 => screen_write_clearstartofscreen(self.sctx, bg),
            2 => screen_write_clearscreen(self.sctx, bg),
            3 if submode == 0 => screen_write_clearhistory(self.sctx),
            _ => {}
        }
    }

    fn erase_in_line(&mut self, mode: i32) {
        let bg = self.ictx.cell.cell.bg as u_int;
        match mode {
            0 => screen_write_clearendofline(self.sctx, bg),
            1 => screen_write_clearstartofline(self.sctx, bg),
            2 => screen_write_clearline(self.sctx, bg),
            _ => {}
        }
    }

    fn set_cursor_style(&mut self, style: u32) {
        self.sctx
            .with_screen_mut(|s| screen_set_cursor_style(style, &mut s.cstyle, &mut s.mode));
        if style == 0 {
            screen_write_mode_clear(self.sctx, MODE_CURSOR_BLINKING_SET);
        }
    }

    fn set_mode(&mut self, mode: i32, private: bool, enabled: bool) {
        if !private {
            match (mode, enabled) {
                (4, true) => screen_write_mode_set(self.sctx, MODE_INSERT),
                (4, false) => screen_write_mode_clear(self.sctx, MODE_INSERT),
                (34, true) => screen_write_mode_clear(self.sctx, MODE_CURSOR_VERY_VISIBLE),
                (34, false) => screen_write_mode_set(self.sctx, MODE_CURSOR_VERY_VISIBLE),
                _ => {}
            }
            return;
        }
        let bg = self.ictx.cell.cell.bg as u_int;
        if enabled {
            match mode {
                1 => screen_write_mode_set(self.sctx, MODE_KCURSOR),
                3 => {
                    screen_write_cursormove(self.sctx, 0, 0, 1);
                    screen_write_clearscreen(self.sctx, bg);
                }
                6 => {
                    screen_write_mode_set(self.sctx, MODE_ORIGIN);
                    screen_write_cursormove(self.sctx, 0, 0, 1);
                }
                7 => screen_write_mode_set(self.sctx, MODE_WRAP),
                12 => {
                    screen_write_mode_set(self.sctx, MODE_CURSOR_BLINKING);
                    screen_write_mode_set(self.sctx, MODE_CURSOR_BLINKING_SET);
                }
                25 => screen_write_mode_set(self.sctx, MODE_CURSOR),
                1000 => {
                    screen_write_mode_clear(self.sctx, ALL_MOUSE_MODES);
                    screen_write_mode_set(self.sctx, MODE_MOUSE_STANDARD);
                }
                1002 => {
                    screen_write_mode_clear(self.sctx, ALL_MOUSE_MODES);
                    screen_write_mode_set(self.sctx, MODE_MOUSE_BUTTON);
                }
                1003 => {
                    screen_write_mode_clear(self.sctx, ALL_MOUSE_MODES);
                    screen_write_mode_set(self.sctx, MODE_MOUSE_ALL);
                }
                1004 => screen_write_mode_set(self.sctx, MODE_FOCUSON),
                1005 => screen_write_mode_set(self.sctx, MODE_MOUSE_UTF8),
                1006 => screen_write_mode_set(self.sctx, MODE_MOUSE_SGR),
                47 | 1047 => screen_write_alternateon(self.sctx, &self.ictx.cell.cell, 0),
                1049 => screen_write_alternateon(self.sctx, &self.ictx.cell.cell, 1),
                2004 => screen_write_mode_set(self.sctx, MODE_BRACKETPASTE),
                2031 => {
                    screen_write_mode_set(self.sctx, MODE_THEME_UPDATES);
                    if let Some(pane) = self.ictx.wp.upgrade() {
                        let theme = window_pane_get_theme(Some(&pane));
                        let mut state = pane.borrow_mut();
                        state.last_theme = theme;
                        state.flags &= !PANE_THEMECHANGED;
                    }
                }
                2026 => {
                    if let Some(pane) = self.ictx.wp.upgrade() {
                        pane.start_sync();
                    }
                }
                _ => {}
            }
        } else {
            match mode {
                1 => screen_write_mode_clear(self.sctx, MODE_KCURSOR),
                3 => {
                    screen_write_cursormove(self.sctx, 0, 0, 1);
                    screen_write_clearscreen(self.sctx, bg);
                }
                6 => {
                    screen_write_mode_clear(self.sctx, MODE_ORIGIN);
                    screen_write_cursormove(self.sctx, 0, 0, 1);
                }
                7 => screen_write_mode_clear(self.sctx, MODE_WRAP),
                12 => {
                    screen_write_mode_clear(self.sctx, MODE_CURSOR_BLINKING);
                    screen_write_mode_set(self.sctx, MODE_CURSOR_BLINKING_SET);
                }
                25 => screen_write_mode_clear(self.sctx, MODE_CURSOR),
                1000..=1003 => screen_write_mode_clear(self.sctx, ALL_MOUSE_MODES),
                1004 => screen_write_mode_clear(self.sctx, MODE_FOCUSON),
                1005 => screen_write_mode_clear(self.sctx, MODE_MOUSE_UTF8),
                1006 => screen_write_mode_clear(self.sctx, MODE_MOUSE_SGR),
                47 | 1047 => screen_write_alternateoff(self.sctx, &mut self.ictx.cell.cell, 0),
                1049 => screen_write_alternateoff(self.sctx, &mut self.ictx.cell.cell, 1),
                2004 => screen_write_mode_clear(self.sctx, MODE_BRACKETPASTE),
                2026 => screen_write_end_sync(self.sctx),
                2031 => {
                    screen_write_mode_clear(self.sctx, MODE_THEME_UPDATES);
                    if let Some(pane) = self.ictx.wp.upgrade() {
                        pane.borrow_mut().flags &= !PANE_THEMECHANGED;
                    }
                }
                _ => {}
            }
        }
    }

    fn set_keypad_mode(&mut self, enabled: bool) {
        if enabled {
            screen_write_mode_set(self.sctx, MODE_KKEYPAD);
        } else {
            screen_write_mode_clear(self.sctx, MODE_KKEYPAD);
        }
    }

    fn set_extended_keys_mode(&mut self, param: Option<i32>) {
        match param {
            Some(m) => {
                let ek = OptionsScope::GlobalServer
                    .with_local(|oo| options_get_number(oo, c"extended-keys"))
                    as c_int;
                if ek != 0 {
                    screen_write_mode_clear(self.sctx, EXTENDED_KEY_MODES);
                    if m == 2 {
                        screen_write_mode_set(self.sctx, MODE_KEYS_EXTENDED_2);
                    } else if m == 1 || ek == 2 {
                        screen_write_mode_set(self.sctx, MODE_KEYS_EXTENDED);
                    }
                }
            }
            None => {
                screen_write_mode_clear(self.sctx, MODE_KEYS_EXTENDED | MODE_KEYS_EXTENDED_2);
                if OptionsScope::GlobalServer
                    .with_local(|oo| options_get_number(oo, c"extended-keys"))
                    == 2
                {
                    screen_write_mode_set(self.sctx, MODE_KEYS_EXTENDED);
                }
            }
        }
    }

    fn reply(&mut self, queue: bool, bytes: &[u8]) {
        input_reply(self.ictx, i32::from(queue), |out| out.write_all(bytes));
    }

    fn query_cursor_position(&mut self) {
        let cy = self.sctx.with_screen(|s| s.cy.wrapping_add(1));
        let cx = self.sctx.with_screen(|s| s.cx.wrapping_add(1));
        input_reply(self.ictx, 1, |out| write!(out, "\x1B[{cy};{cx}R"));
    }

    fn query_mode(&mut self, m: i32, private: bool) {
        if !private {
            let n = match m {
                4 => {
                    if self.sctx.with_screen(|s| s.mode) & MODE_INSERT != 0 {
                        1
                    } else {
                        2
                    }
                }
                _ => 0,
            };
            input_reply(self.ictx, 1, |out| write!(out, "\x1B[{m};{n}$y"));
            return;
        }
        let mode = self.sctx.with_screen(|s| s.mode);
        let n = match m {
            1 => if mode & MODE_KCURSOR != 0 { 1 } else { 2 },
            3 => 4,
            6 => if mode & MODE_ORIGIN != 0 { 1 } else { 2 },
            7 => if mode & MODE_WRAP != 0 { 1 } else { 2 },
            12 => {
                if self.sctx.with_screen(|s| s.cstyle) != SCREEN_CURSOR_DEFAULT as c_uint
                    || mode & MODE_CURSOR_BLINKING_SET != 0
                {
                    if mode & MODE_CURSOR_BLINKING != 0 { 1 } else { 2 }
                } else {
                    let p = if let Some(pane) = self.ictx.wp.upgrade() {
                        options_get_number(&pane.borrow_options_mut(), c"cursor-style")
                    } else {
                        OptionsScope::GlobalWindow
                            .with_local(|oo| options_get_number(oo, c"cursor-style"))
                    } as c_int;
                    if p == 1 || p == 3 || p == 5 { 1 } else { 2 }
                }
            }
            25 => if mode & MODE_CURSOR != 0 { 1 } else { 2 },
            47 | 1047 | 1049 => {
                if self.sctx.with_screen(|s| s.saved_grid.is_some()) {
                    1
                } else {
                    2
                }
            }
            1000 => if mode & MODE_MOUSE_STANDARD != 0 { 1 } else { 2 },
            1002 => if mode & MODE_MOUSE_BUTTON != 0 { 1 } else { 2 },
            1003 => if mode & MODE_MOUSE_ALL != 0 { 1 } else { 2 },
            1004 => if mode & MODE_FOCUSON != 0 { 1 } else { 2 },
            1005 => if mode & MODE_MOUSE_UTF8 != 0 { 1 } else { 2 },
            1006 => if mode & MODE_MOUSE_SGR != 0 { 1 } else { 2 },
            2004 => if mode & MODE_BRACKETPASTE != 0 { 1 } else { 2 },
            2026 => if mode & MODE_SYNC != 0 { 1 } else { 2 },
            2031 => if mode & MODE_THEME_UPDATES != 0 { 1 } else { 2 },
            _ => 0,
        };
        input_reply(self.ictx, 1, |out| write!(out, "\x1B[?{m};{n}$y"));
    }

    fn query_theme(&mut self) {
        input_report_current_theme(self.ictx);
    }

    fn query_xda(&mut self) {
        input_reply(self.ictx, 1, |out| {
            out.write_all(b"\x1BP>|tmux ")?;
            out.write_all(getversion().to_bytes())?;
            out.write_all(b"\x1B\\")
        });
    }

    fn query_decrqss_cursor_style(&mut self) {
        let cstyle = self.sctx.with_screen(|s| s.cstyle);
        let mode = self.sctx.with_screen(|s| s.mode);
        let ps = if cstyle == SCREEN_CURSOR_BLOCK as c_uint
            || cstyle == SCREEN_CURSOR_UNDERLINE as c_uint
            || cstyle == SCREEN_CURSOR_BAR as c_uint
        {
            let blinking = mode & MODE_CURSOR_BLINKING != 0;
            match cstyle {
                1 => if blinking { 1 } else { 2 },
                2 => if blinking { 3 } else { 4 },
                3 => if blinking { 5 } else { 6 },
                _ => 0,
            }
        } else {
            let mut opt_ps = if let Some(pane) = self.ictx.wp.upgrade() {
                options_get_number(&pane.borrow_options_mut(), c"cursor-style")
            } else {
                OptionsScope::GlobalWindow.with_local(|oo| options_get_number(oo, c"cursor-style"))
            } as c_int;
            if !(0..=6).contains(&opt_ps) {
                opt_ps = 0;
            }
            opt_ps
        };
        log_debug(format_args!(
            "{}: DECRQSS cursor -> Ps={} (cstyle={} mode={})",
            "input_handle_decrqss",
            ps,
            cstyle as i32,
            log_hex((mode as u32) as u64)
        ));
        input_reply(self.ictx, 1, |out| write!(out, "\x1BP1$r q{ps} q\x1B\\"));
    }

    fn winop(&mut self, op: WinOp) {
        let x = self.sctx.with_screen(|s| s.grid().sx);
        let y = self.sctx.with_screen(|s| s.grid().sy);
        let pane_owner = self.ictx.wp.upgrade();
        let window_owner = pane_owner
            .as_ref()
            .and_then(|pane| pane.window_observer().upgrade());
        match op {
            WinOp::ReportTextSizePixels => {
                if let Some(window) = &window_owner {
                    let (xpixel, ypixel) = window.cell_size();
                    input_reply(self.ictx, 1, |out| {
                        write!(
                            out,
                            "\x1B[4;{};{}t",
                            y.wrapping_mul(ypixel),
                            x.wrapping_mul(xpixel)
                        )
                    });
                }
            }
            WinOp::ReportScreenSizePixels => {
                if let Some(window) = &window_owner {
                    let (xpixel, ypixel) = window.cell_size();
                    input_reply(self.ictx, 1, |out| {
                        write!(
                            out,
                            "\x1B[5;{};{}t",
                            y.wrapping_mul(ypixel),
                            x.wrapping_mul(xpixel)
                        )
                    });
                }
            }
            WinOp::ReportCellSizePixels => {
                if let Some(window) = &window_owner {
                    let (xpixel, ypixel) = window.cell_size();
                    input_reply(self.ictx, 1, |out| write!(out, "\x1B[6;{ypixel};{xpixel}t"));
                }
            }
            WinOp::ReportTextSizeChars => {
                input_reply(self.ictx, 1, |out| write!(out, "\x1B[8;{y};{x}t"));
            }
            WinOp::ReportScreenSizeChars => {
                input_reply(self.ictx, 1, |out| write!(out, "\x1B[9;{y};{x}t"));
            }
            WinOp::PushTitle => {
                self.sctx.with_screen_mut(screen_push_title);
            }
            WinOp::PopTitle => {
                self.sctx.with_screen_mut(screen_pop_title);
                if let (Some(pane), Some(window)) = (&pane_owner, &window_owner) {
                    let title = self.sctx.with_screen(|s| s.title.clone());
                    input_fire_pane_title_changed(pane, &title);
                    server_redraw_window_borders(window);
                    server_status_window(window);
                }
            }
        }
    }

    fn dcs_tmux_passthrough(&mut self, payload: &[u8]) {
        let allow_passthrough = if let Some(pane) = self.ictx.wp.upgrade() {
            options_get_number(&pane.borrow_options_mut(), c"allow-passthrough")
        } else {
            OptionsScope::GlobalWindow.with_local(|oo| options_get_number(oo, c"allow-passthrough"))
        };
        if allow_passthrough == 0 {
            return;
        }
        screen_write_rawstring(self.sctx, payload, i32::from(allow_passthrough == 2));
    }

    fn set_title(&mut self, title: &CStr) {
        if let Some(pane) = self.ictx.wp.upgrade().filter(|pane| {
            options_get_number(&pane.borrow_options_mut(), c"allow-set-title") != 0
                && self.sctx.with_screen_mut(|s| screen_set_title(s, title, 1)) != 0
        }) {
            input_fire_pane_title_changed(&pane, title);
            let window = pane.window_observer().upgrade().expect("live window");
            server_redraw_window_borders(&window);
            server_status_window(&window);
        }
    }

    fn set_path(&mut self, path: &CStr) {
        if let Some(pane) = self
            .ictx
            .wp
            .upgrade()
            .filter(|_| self.sctx.with_screen_mut(|s| screen_set_path(s, path)) != 0)
        {
            let window = pane.window_observer().upgrade().expect("live window");
            server_redraw_window_borders(&window);
            server_status_window(&window);
        }
    }

    fn set_hyperlink(&mut self, uri: &CStr, id: Option<&CStr>) {
        if uri.is_empty() {
            self.ictx.cell.cell.link = 0;
            return;
        }
        let hl = self
            .sctx
            .with_screen(|s| s.hyperlinks.clone().expect("screen hyperlink table"));
        self.ictx.cell.cell.link = hyperlinks_put(&hl, uri, id);
    }

    fn set_progress_bar(&mut self, state: u32, progress: i32) {
        self.sctx
            .with_screen_mut(|s| screen_set_progress_bar(s, state, progress));
        if let Some(pane) = self.ictx.wp.upgrade() {
            let window = pane.window_observer().upgrade().expect("live window");
            server_redraw_window_borders(&window);
            server_status_window(&window);
        }
    }

    fn osc_4_query(&mut self, idx: i32, end_type: input_end_type) {
        let c = self
            .ictx
            .palette
            .with_mut(|palette| colour_palette_get(Some(palette), idx | COLOUR_FLAG_256))
            .unwrap_or(-1);
        if c != -1 {
            input_osc_colour_reply(self.ictx, 1, 4, idx, c, end_type);
        } else {
            input_add_request(self.ictx, INPUT_REQUEST_PALETTE, idx);
        }
    }

    fn osc_4_set(&mut self, idx: i32, spec: &CStr) -> bool {
        let c = colour_parse_x11_logged(spec).unwrap_or(-1);
        if c == -1 {
            return false;
        }
        self.ictx
            .palette
            .with_mut(|palette| colour_palette_set(Some(palette), idx, c))
            .unwrap_or(0)
            != 0
    }

    fn osc_104_reset_all(&mut self) {
        let _ = self
            .ictx
            .palette
            .with_mut(|palette| colour_palette_clear(Some(palette)));
        screen_write_fullredraw(self.sctx);
    }

    fn osc_104_reset_one(&mut self, idx: i32) -> bool {
        self.ictx
            .palette
            .with_mut(|palette| colour_palette_set(Some(palette), idx, -1))
            .unwrap_or(0)
            != 0
    }

    fn full_redraw(&mut self) {
        screen_write_fullredraw(self.sctx);
    }

    fn osc_10(&mut self, spec: Option<&CStr>, end_type: input_end_type) {
        let pane_owner = self.ictx.wp.upgrade();
        let Some(spec) = spec else {
            let Some(pane) = pane_owner.as_ref() else {
                return;
            };
            let mut c = window_pane_get_fg_control_client(pane);
            if c == -1 {
                let defaults = pane.default_colours().0;
                c = if defaults.fg == 8 || defaults.fg == 9 {
                    window_pane_get_fg(pane)
                } else {
                    defaults.fg
                };
            }
            input_osc_colour_reply(self.ictx, 1, 10, 0, c, end_type);
            return;
        };
        let c = colour_parse_x11_logged(spec).unwrap_or(-1);
        if c == -1 {
            return;
        }
        if self
            .ictx
            .palette
            .with_mut(|palette| palette.fg = c)
            .is_some()
        {
            if let Some(pane) = pane_owner.as_ref() {
                pane.borrow_mut().flags |= PANE_STYLECHANGED;
            }
            screen_write_fullredraw(self.sctx);
        }
    }

    fn osc_110(&mut self) {
        if self
            .ictx
            .palette
            .with_mut(|palette| palette.fg = 8)
            .is_some()
        {
            if let Some(pane) = self.ictx.wp.upgrade() {
                pane.borrow_mut().flags |= PANE_STYLECHANGED;
            }
            screen_write_fullredraw(self.sctx);
        }
    }

    fn osc_11(&mut self, spec: Option<&CStr>, end_type: input_end_type) {
        let pane_owner = self.ictx.wp.upgrade();
        let Some(spec) = spec else {
            let Some(pane) = pane_owner.as_ref() else {
                return;
            };
            let c = window_pane_get_bg(pane);
            input_osc_colour_reply(self.ictx, 1, 11, 0, c, end_type);
            return;
        };
        let c = colour_parse_x11_logged(spec).unwrap_or(-1);
        if c == -1 {
            return;
        }
        if self
            .ictx
            .palette
            .with_mut(|palette| palette.bg = c)
            .is_some()
        {
            if let Some(pane) = pane_owner.as_ref() {
                pane.borrow_mut().flags |= PANE_STYLECHANGED | PANE_THEMECHANGED;
            }
            screen_write_fullredraw(self.sctx);
        }
    }

    fn osc_111(&mut self) {
        if self
            .ictx
            .palette
            .with_mut(|palette| palette.bg = 8)
            .is_some()
        {
            if let Some(pane) = self.ictx.wp.upgrade() {
                pane.borrow_mut().flags |= PANE_STYLECHANGED | PANE_THEMECHANGED;
            }
            screen_write_fullredraw(self.sctx);
        }
    }

    fn osc_12(&mut self, spec: Option<&CStr>, end_type: input_end_type) {
        let Some(spec) = spec else {
            if self.ictx.wp.upgrade().is_some() {
                let mut c = self.sctx.with_screen(|s| s.ccolour);
                if c == -1 {
                    c = self.sctx.with_screen(|s| s.default_ccolour);
                }
                input_osc_colour_reply(self.ictx, 1, 12, 0, c, end_type);
            }
            return;
        };
        let c = colour_parse_x11_logged(spec).unwrap_or(-1);
        if c == -1 {
            return;
        }
        self.sctx
            .with_screen_mut(|s| screen_set_cursor_colour(s, c));
    }

    fn osc_112(&mut self) {
        self.sctx
            .with_screen_mut(|s| screen_set_cursor_colour(s, -1));
    }

    fn allow_set_clipboard(&self) -> bool {
        OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"set-clipboard")) == 2
    }

    fn osc_52_query(&mut self, clip: ::core::ffi::c_char, end_type: input_end_type) {
        let state = OptionsScope::GlobalServer
            .with_local(|oo| options_get_number(oo, c"get-clipboard")) as c_int;
        if state == 0 {
            return;
        }
        if state == 1 {
            let Some(pb) = paste_get_top(None) else {
                return;
            };
            let buffer = pb.try_borrow().expect("live paste buffer");
            let buf = paste_buffer_data(&buffer).unwrap_or_default();
            let end = if end_type == INPUT_END_BEL {
                c"\x07"
            } else {
                c"\x1B\\"
            };
            let _ = self
                .ictx
                .event
                .with_stream(|event| input_reply_clipboard(event, buf, end, clip));
            return;
        }
        input_add_request(self.ictx, INPUT_REQUEST_CLIPBOARD, end_type as c_int);
    }

    fn osc_52_set(&mut self, clip: &CStr, out: Vec<u8>) {
        if let Some(pane) = self.ictx.wp.upgrade() {
            let mut ctx = screen_write_start_beside(self.sctx, &pane);
            screen_write_setselection(&mut ctx, clip, &out);
            screen_write_stop(ctx);
            events_fire_pane(c"pane-set-clipboard", pane);
            paste_add_owned(None, out.into_boxed_slice());
        } else if let Some(client) = self.ictx.c.upgrade() {
            tty_set_selection(&client, clip, &out);
            paste_add_owned(None, out.into_boxed_slice());
        }
    }

    fn osc_133(&mut self, cmd: Osc133Command) {
        let pane_owner = self.ictx.wp.upgrade();
        let running = match &cmd {
            Osc133Command::PromptStart | Osc133Command::CommandFinished { .. } => Some(false),
            Osc133Command::OutputStart => Some(true),
            _ => None,
        };
        if let Some(pane) = pane_owner.as_ref() {
            let changed = super::process_status::shell_event(&pane.borrow(), running);
            if changed {
                if let Some(window) = pane.window_observer().upgrade() {
                    server_status_window(&window);
                }
            }
        }
        let cx = self.sctx.with_screen(|s| s.cx);
        let cy = self.sctx.with_screen(|s| s.cy);
        let line = self.sctx.with_screen(|s| {
            let gd = s.grid();
            let line = cy.wrapping_add(gd.hsize);
            (line < gd.hsize.wrapping_add(gd.sy)).then_some(line)
        });
        match cmd {
            Osc133Command::PromptStart => {
                if let Some(line) = line {
                    self.sctx.with_screen_mut(|s| {
                        let gl = grid_get_line_mut(s.grid_mut(), line);
                        gl.osc133_data = osc133_data::default();
                        gl.osc133_data.prompt_col = cx as u_short;
                        gl.flags |= GRID_LINE_START_PROMPT as u_short;
                    });
                }
                if let Some(pane) = pane_owner {
                    pane.borrow_mut().last_prompt_time =
                        crate::shared::time::unix_seconds(std::time::SystemTime::now());
                    events_fire_pane(c"pane-shell-prompt", pane);
                }
            }
            Osc133Command::Prompt { secondary } => {
                if let Some(line) = line {
                    self.sctx.with_screen_mut(|s| {
                        let gl = grid_get_line_mut(s.grid_mut(), line);
                        if secondary {
                            gl.flags |= GRID_LINE_SECOND_PROMPT as u_short;
                        } else {
                            gl.flags |= GRID_LINE_START_PROMPT as u_short;
                        }
                        gl.osc133_data.prompt_col = cx as u_short;
                    });
                }
            }
            Osc133Command::CommandStart => {
                if let Some(line) = line {
                    self.sctx.with_screen_mut(|s| {
                        let gl = grid_get_line_mut(s.grid_mut(), line);
                        gl.flags |= GRID_LINE_START_COMMAND as u_short;
                        gl.osc133_data.cmd_col = cx as u_short;
                    });
                }
            }
            Osc133Command::OutputStart => {
                if let Some(line) = line {
                    self.sctx.with_screen_mut(|s| {
                        let gl = grid_get_line_mut(s.grid_mut(), line);
                        gl.flags |= GRID_LINE_START_OUTPUT as u_short;
                        gl.osc133_data.out_start_col = cx as u_short;
                    });
                }
                if let Some(pane) = pane_owner.as_ref() {
                    {
                        let mut state = pane.borrow_mut();
                        state.cmd_start_time =
                            crate::shared::time::unix_seconds(std::time::SystemTime::now());
                        state.cmd_end_time = 0;
                        state.flags |= PANE_CMDRUNNING;
                        state.cmd_status = -1;
                    }
                    input_fire_command_event(pane, c"pane-command-started");
                }
            }
            Osc133Command::CommandFinished { exit_status } => {
                if let Some(pane) = pane_owner.as_ref() {
                    {
                        let mut state = pane.borrow_mut();
                        state.cmd_end_time =
                            crate::shared::time::unix_seconds(std::time::SystemTime::now());
                        state.flags &= !PANE_CMDRUNNING;
                        state.cmd_status = exit_status;
                    }
                    input_fire_command_event(pane, c"pane-command-finished");
                }
                if let Some(line) = line {
                    self.sctx.with_screen_mut(|s| {
                        let gl = grid_get_line_mut(s.grid_mut(), line);
                        gl.flags |= GRID_LINE_END_OUTPUT as u_short;
                        gl.osc133_data.out_end_col = cx as u_short;
                        gl.osc133_data.exit_status = exit_status as u_char;
                    });
                }
            }
        }
    }

    fn set_program_status(&mut self, status: Option<ProgramStatus>) {
        let changed = self.sctx.with_screen_mut(|screen| {
            if screen.program_status == status {
                return false;
            }
            screen.program_status = status;
            true
        });
        if changed {
            if let Some(pane) = self.ictx.wp.upgrade() {
                super::process_status::shell_event(&pane.borrow(), None);
                if let Some(window) = pane.window_observer().upgrade() {
                    server_status_window(&window);
                }
            }
        }
    }

    fn exit_rename(&mut self, input_buf: &[u8], input_len: usize) {
        input_exit_rename(self.ictx, input_buf, input_len);
    }
}

pub fn input_reply_clipboard(
    bev: &mut bufferevent,
    buf: &[u8],
    end: &CStr,
    clip: ::core::ffi::c_char,
) {
    if buf.len()
        >= (INT_MAX as size_t)
            .wrapping_mul(3)
            .wrapping_div(4)
            .wrapping_sub(1)
    {
        return;
    }
    let out = tiri_vt::base64::encode(buf);
    bufferevent_write(bev, b"\x1B]52;");
    if clip != 0 {
        bufferevent_write(bev, &[clip as u8]);
    }
    bufferevent_write(bev, b";");
    if !out.is_empty() {
        bufferevent_write(bev, &out);
    }
    bufferevent_write(bev, end.to_bytes());
}

pub fn input_set_buffer_size(buffer_size: size_t) {
    log_debug(format_args!(
        "{}: {} -> {}",
        "input_set_buffer_size",
        tiri_vt::buffer_size() as ::core::ffi::c_ulong,
        buffer_size as ::core::ffi::c_ulong
    ));
    tiri_vt::set_buffer_size(buffer_size);
}

fn input_request_timer_callback(ictx: &mut input_ctx) {
    let t: uint64_t = get_timer();
    let mut index = 0;
    while index < ictx.requests.len() {
        let expired = {
            let ir = ictx.requests[index]
                .try_borrow()
                .expect("live input request");
            ir.t < t.wrapping_sub(INPUT_REQUEST_TIMEOUT as uint64_t)
        };
        if !expired {
            index += 1;
            continue;
        }
        let owner = input_free_request(ictx, index);
        let ir = owner.try_borrow().expect("live input request");
        if ir.type_0 == INPUT_REQUEST_QUEUE {
            input_send_reply(
                ictx,
                ir.data
                    .as_deref()
                    .expect("queued input request has no reply data"),
            );
        }
    }
    if !ictx.requests.is_empty() {
        input_start_request_timer(ictx);
    }
}

fn input_start_request_timer(ictx: &mut input_ctx) {
    let tv = Duration::from_micros(100000);
    drop(ictx.request_timer.take());
    let observer = ictx.this.clone();
    ictx.request_timer = Some(
        Timer::new(tv, move || {
            input_request_timer_callback(
                &mut observer.try_borrow_mut().expect("live input context"),
            )
        })
        .expect("arm timer"),
    );
}

fn input_make_request(ictx: &mut input_ctx, request: input_request) -> refbox::Weak<input_request> {
    if ictx.requests.is_empty() {
        input_start_request_timer(ictx);
    }
    let owner = refbox::RefBox::new(request);
    let observer = owner.downgrade();
    ictx.requests.push_back(owner);
    observer
}

/// Remove the request at `index`, retiring its client observation.
fn input_free_request(ictx: &mut input_ctx, index: usize) -> refbox::RefBox<input_request> {
    let owner = ictx
        .requests
        .remove(index)
        .expect("request missing from its input context owner");
    let client = owner.try_borrow().expect("live input request").c.upgrade();
    if let Some(client) = client {
        client.forget_input_request(&owner);
    }
    owner
}

fn input_add_request(
    ictx: &mut input_ctx,
    type_0: input_request_type,
    idx: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let Some(input_pane_owner) = ictx.wp.upgrade() else {
        return -1;
    };
    let window = input_pane_owner
        .window_observer()
        .upgrade()
        .expect("pane window");
    let mut c: Option<ClientRef> = None;
    let mut registry_loop_0_owner = ClientRegistry::first();
    let mut loop_0 = registry_loop_0_owner.clone();
    while let Some(current) = loop_0 {
        if (current.flags() & CLIENT_UNATTACHEDFLAGS as uint64_t == 0)
            && current
                .attached_session()
                .upgrade()
                .is_some_and(|session| session.contains_window(&window))
            && current.terminal_started()
            && c.as_ref()
                .is_none_or(|best| current.activity_time() > best.activity_time())
        {
            c = Some(current);
        }
        registry_loop_0_owner =
            ClientRegistry::next(registry_loop_0_owner.as_ref().expect("current registry client"));
        loop_0 = registry_loop_0_owner.clone();
    }
    drop(window);
    let Some(client) = c else {
        return -1;
    };
    let mut ir = input_request::new(ictx, type_0);
    ir.c = std::rc::Rc::downgrade(&client);
    ir.idx = idx;
    ir.end = ictx.vt.input_end;
    let observer = input_make_request(ictx, ir);
    client.observe_input_request(observer);
    match type_0 {
        0 => client.write_terminal(&format_bytes(format_args!("\x1B]4;{idx};?\x1B\\"))),
        1 => crate::tty::tty_request_clipboard(&client),
        _ => {}
    }
    0
}

fn input_request_palette_reply(
    ictx: &mut input_ctx,
    ir: &input_request,
    pd: &input_request_palette_data,
) {
    input_osc_colour_reply(ictx, 0, 4, pd.idx, pd.c, ir.end);
}

fn input_request_clipboard_reply(
    ictx: &mut input_ctx,
    ir: &input_request,
    cd: &input_request_clipboard_data,
) {
    let state = OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"get-clipboard"))
        as c_int;
    if state == 0 || state == 1 {
        return;
    }
    if state == 3 && !cd.data.is_empty() {
        let owned: Box<[u8]> = cd.data.as_slice().into();
        paste_add_owned(None, owned);
    }
    let end = if ir.idx == INPUT_END_BEL as c_int {
        c"\x07"
    } else {
        c"\x1B\\"
    };
    let _ = ictx
        .event
        .with_stream(|event| input_reply_clipboard(event, &cd.data, end, cd.clip));
}

#[derive(Clone, Copy)]
pub enum InputRequestReply<'a> {
    Palette(&'a input_request_palette_data),
    Clipboard(&'a input_request_clipboard_data),
}

pub fn input_request_reply(client: &ClientRef, reply: InputRequestReply<'_>) {
    client.reply_input_request(reply);
}

/// `requests` are client observations, which input_free_request retires
/// before freeing, so each is live until it is freed here.
pub(crate) fn input_reply_observed_requests(
    requests: Vec<refbox::Weak<input_request>>,
    reply: InputRequestReply<'_>,
) {
    for request in requests {
        let (matches, observer) = {
            let ir = request
                .try_borrow()
                .expect("observed input request is live");
            (input_request_matches(&ir, reply), ir.ictx.clone())
        };
        let mut ictx = observer
            .try_borrow_mut()
            .expect("live request input context");
        if matches {
            input_complete_request(&mut ictx, &request, reply);
            return;
        }
        let index = input_request_index(&ictx, &request);
        drop(input_free_request(&mut ictx, index));
    }
}

/// `requests` were taken from a client's observations, so each is live.
pub(crate) fn input_cancel_observed_requests(requests: Vec<refbox::Weak<input_request>>) {
    for request in requests {
        let observer = {
            let mut ir = request
                .try_borrow_mut()
                .expect("observed input request is live");
            ir.c = std::rc::Weak::new();
            ir.ictx.clone()
        };
        let mut ictx = observer
            .try_borrow_mut()
            .expect("live request input context");
        let index = input_request_index(&ictx, &request);
        drop(input_free_request(&mut ictx, index));
    }
}

fn input_request_index(ictx: &input_ctx, request: &refbox::Weak<input_request>) -> usize {
    ictx.requests
        .iter()
        .position(|owner| *request == *owner)
        .expect("request missing from its input context owner")
}

fn input_request_matches(request: &input_request, reply: InputRequestReply<'_>) -> bool {
    match reply {
        InputRequestReply::Palette(data) => {
            request.type_0 == INPUT_REQUEST_PALETTE && request.idx == data.idx
        }
        InputRequestReply::Clipboard(_) => request.type_0 == INPUT_REQUEST_CLIPBOARD,
    }
}

fn input_complete_request(
    ictx: &mut input_ctx,
    found: &refbox::Weak<input_request>,
    reply: InputRequestReply<'_>,
) {
    let mut complete = false;
    while let Some(front) = ictx.requests.front() {
        let queued = front.try_borrow().expect("live input request").type_0 == INPUT_REQUEST_QUEUE;
        if complete && !queued {
            break;
        }
        let is_found = *found == *front;
        let owner = input_free_request(ictx, 0);
        let ir = owner.try_borrow().expect("live input request");
        if queued {
            input_send_reply(
                ictx,
                ir.data
                    .as_deref()
                    .expect("queued input request has no reply data"),
            );
        } else if is_found {
            match reply {
                InputRequestReply::Palette(pd) => input_request_palette_reply(ictx, &ir, pd),
                InputRequestReply::Clipboard(cd) => input_request_clipboard_reply(ictx, &ir, cd),
            }
            complete = true;
        }
    }
}

#[cfg(test)]
mod input_buffer_ownership_tests {
    use super::*;
    use crate::reactor::bufferevent_new;

    #[test]
    fn palette_source_observes_pane_or_popup_owner() {
        use crate::shared::input::InputPalette;
        let pane = window_pane::new();
        let pane_source = InputPalette::Pane(std::rc::Rc::downgrade(&pane));
        pane_source.with_mut(|palette| palette.fg = 17);
        assert_eq!(pane.borrow().palette.fg, 17);
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
        let stream = bufferevent_new(None, None, None, None).unwrap();
        let owner = input_init(None, stream.clone(), Default::default(), None);
        let mut input = owner.try_borrow_mut().unwrap();
        assert!(input.event.is_alive());
        stream.free();
        assert!(!input.event.is_alive());
        input_send_reply(&mut input, c"reply");
        drop(input);
        drop(owner);
    }

    #[test]
    fn input_observes_pane_and_can_outlive_its_allocation() {
        let pane = window_pane::new();
        let context = input_init(Some(&pane), Default::default(), Default::default(), None);
        assert_eq!(std::rc::Rc::strong_count(&pane), 1);
        assert!(context
            .try_borrow()
            .unwrap()
            .wp
            .ptr_eq(&std::rc::Rc::downgrade(&pane)));
        drop(pane);
        assert!(context.try_borrow().unwrap().wp.upgrade().is_none());
        drop(context);
    }
}

#[cfg(test)]
mod input_request_ownership_tests {
    use super::*;

    #[test]
    fn reply_without_pending_request_does_not_queue() {
        let mut ictx_owner = Box::new(input_ctx::new());
        let ictx = &mut *ictx_owner;
        input_reply(ictx, 1, |out| out.write_all(b"\x1b[0n"));
        assert!(ictx.requests.is_empty());
        drop(ictx_owner);
    }
}

#[cfg(test)]
mod print_run_tests {
    use super::*;
    use crate::grid::{grid_cells_equal, grid_get_cell, grid_get_line};
    use crate::screen::screen_init;
    use tiri_rt::Runtime as _;

    fn parsed(chunks: &[&[u8]]) -> screen {
        let _rt = tiri_rt::mio::Runtime::new().unwrap();
        crate::options::OptionsScope::GlobalServer.reset_default(
            crate::options::options_table
                .iter()
                .find(|entry| entry.name == Some(c"extended-keys"))
                .unwrap(),
        );
        let mut s = screen::empty();
        screen_init(&mut s, 10, 4, 100);
        let owner = input_init(None, Default::default(), Default::default(), None);
        let mut ictx = owner.try_borrow_mut().unwrap();
        for chunk in chunks {
            input_parse_screen(&mut ictx, &mut s, Some(Box::new(|_| {})), chunk);
        }
        drop(ictx);
        drop(owner);
        s
    }

    #[test]
    fn a_printed_run_matches_printing_byte_by_byte() {
        let input: &[u8] = b"hello world, this wraps around twice\r\nshort\r\n\
            \x1b[31mred text that is long enough to wrap\x1b[0m\ttab\r\n\
            \x1b(0qqqx\x1b(Bplain\xc3\xa9after \xe2\x82and on\r\n\
            12345\x1b[3D\x1b[4hins\x1b[4lover\x1b[2bend\x1b[?7lno wrap past the edge\x1b[?7h\r\n\
            0123456789x";
        let whole = parsed(&[input]);
        let bytes: Vec<&[u8]> = input.chunks(1).collect();
        let single = parsed(&bytes);
        assert_eq!((whole.cx, whole.cy), (single.cx, single.cy));
        let (a, b) = (whole.grid(), single.grid());
        assert_eq!((a.hsize, a.sy), (b.hsize, b.sy));
        for y in 0..a.hsize + a.sy {
            assert_eq!(
                grid_get_line(a, y).flags,
                grid_get_line(b, y).flags,
                "line {y}"
            );
            for x in 0..a.sx {
                let (mut left, mut right) = (grid_default_cell, grid_default_cell);
                grid_get_cell(a, x, y, &mut left);
                grid_get_cell(b, x, y, &mut right);
                assert!(grid_cells_equal(&left, &right), "cell {x},{y}");
            }
        }
    }

    #[test]
    fn osc_7501_updates_and_clears_program_status() {
        use tiri_vt::{ProgramState, ProgramStatus};

        let s = parsed(&[b"\x1b]7501;state=working:app=antigravity\x07"]);
        assert_eq!(
            s.program_status,
            Some(ProgramStatus::new(
                ProgramState::Working,
                Some("antigravity".into()),
            ))
        );

        let s = parsed(&[
            b"\x1b]7501;state=blocked:kind=permission:app=jetski:title=aGk:msg=dGVzdA==\x1b\\",
        ]);
        assert_eq!(
            s.program_status,
            Some(ProgramStatus::new(
                ProgramState::Blocked,
                Some("jetski".into()),
            ))
        );

        let s = parsed(&[
            b"\x1b]7501;state=working:app=antigravity\x07",
            b"\x1b]7501;state=clear\x07",
        ]);
        assert_eq!(s.program_status, None);

        let s = parsed(&[b"\x1b]7501;state=done:app=bad/app\x07"]);
        assert_eq!(
            s.program_status,
            Some(ProgramStatus::new(ProgramState::Done, None))
        );

        let s = parsed(&[
            b"\x1b]7501;state=working:app=antigravity\x07",
            b"\x1b]133;A\x07",
        ]);
        assert_eq!(s.program_status, None);

        let s = parsed(&[
            b"\x1b]7501;state=working:app=antigravity\x07",
            b"\x1b]133;D;0\x07",
        ]);
        assert_eq!(s.program_status, None);

        let s = parsed(&[
            b"\x1b]7501;state=working:app=antigravity\x07",
            b"\x1bc",
        ]);
        assert_eq!(s.program_status, None);
    }

    #[test]
    fn osc_7501_rejects_malformed_and_subcontext_frames() {
        use tiri_vt::{ProgramState, ProgramStatus};

        for frame in [
            b"\x1b]7501;state=unknown\x07".as_slice(),
            b"\x1b]7501;state=working:title=!!!\x07",
            b"\x1b]7501;state=working:id=a//b\x07",
            b"\x1b]7501;state=working:app=way_too_long_application_identifier_value\x07",
            b"\x1b]7501;state=working:id=sub/1:app=claude-code\x07",
        ] {
            let s = parsed(&[
                b"\x1b]7501;state=idle:app=antigravity\x07",
                frame,
            ]);
            assert_eq!(
                s.program_status,
                Some(ProgramStatus::new(
                    ProgramState::Idle,
                    Some("antigravity".into()),
                )),
                "{frame:?}"
            );
        }
    }
}
