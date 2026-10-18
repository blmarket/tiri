use crate::src::arguments::{args, args_count, args_has, args_parse, args_string};
use crate::src::cmd::{cmd_mouse_at, cmd_mouse_pane};
use crate::src::compat::strtonum::strtonum;
use crate::src::events::events_fire_pane;
use crate::src::ffi::libc::llabs;
use crate::src::ffi::regex::{CompiledRegex, RegexMatch, RegexStorage};
use crate::src::format::bytes::cstr_suffix;
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{
    format_add, format_add_owned_cb, format_create_defaults, format_expand_cstring,
    format_get_pane, format_grid_hyperlink_cstring, format_single_cstring,
};
use crate::src::format_draw::format_draw;
use crate::src::grid::reader::{
    grid_reader_cursor_back_to_indentation, grid_reader_cursor_end_of_line,
    grid_reader_cursor_jump, grid_reader_cursor_jump_back, grid_reader_cursor_left,
    grid_reader_cursor_next_word, grid_reader_cursor_next_word_end,
    grid_reader_cursor_previous_word, grid_reader_cursor_right, grid_reader_cursor_start_of_line,
    grid_reader_get_cursor, grid_reader_in_set, grid_reader_start,
};
use crate::src::grid::{
    grid_default_cell, grid_duplicate_lines, grid_free_lines, grid_get_cell, grid_get_line,
    grid_in_set, grid_line_length, grid_line_limit, grid_line_time, grid_peek_line,
    grid_unwrap_position, grid_wrap_position,
};
use crate::src::input::{input_init, input_parse_screen};
use crate::src::job::{job_get_event, job_run};
use crate::src::log::{fatal, fatalx, log_cstr, log_debug};
use crate::src::options::OptionsScope;
use crate::src::options::{options_get_number, options_get_string};
use crate::src::paste::{paste_add_owned, paste_buffer_data, paste_get_top, paste_set_owned};
use crate::src::reactor::bufferevent_write;
use crate::src::screen::{
    screen_check_selection, screen_clear_selection, screen_hide_selection, screen_init,
    screen_resize, screen_resize_cursor, screen_set_default_cursor, screen_set_selection,
};
use crate::src::screen_write::{
    screen_write_carriagereturn, screen_write_cell, screen_write_cursormove,
    screen_write_deleteline, screen_write_insertline, screen_write_linefeed, screen_write_nputs,
    screen_write_putc, screen_write_setselection, screen_write_start, screen_write_start_pane,
    screen_write_stop, screen_write_strlen,
};
use crate::src::server_client::Client as _;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::abi::{__int32_t, ssize_t};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_READONLY;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::event::*;
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::input::input_ctx;
use crate::src::shared::job::JOB_NOWAIT;
use crate::src::shared::key::{MODEKEY_EMACS, MODEKEY_VI};
use crate::src::shared::limits::{INT_MAX, UCHAR_MAX, UINT_MAX};
use crate::src::shared::mouse::{
    mouse_event, MOUSE_MASK_BUTTONS, MOUSE_WHEEL_DOWN, MOUSE_WHEEL_UP,
};
use crate::src::shared::pane::window_pane;
use crate::src::shared::pane::PANE_REDRAW;
use crate::src::shared::screen::screen;
use crate::src::shared::screen_write::screen_write_ctx;
use crate::src::shared::session::SessionRef;
use crate::src::shared::style::*;
use crate::src::shared::window::WindowRef;
pub use crate::src::shared::window::{window_mode, window_mode_entry, winlink};
use crate::src::status::status_message_set;
use crate::src::style::style_apply_with_options;
use crate::src::text::utf8::{utf8_copy, utf8_fromcstr_vec, utf8_set, utf8_to_data};
use crate::src::tmux::{get_timer, with_global_options};
use crate::src::tty::tty_acs_get;
use crate::src::window::Window as _;
use crate::src::window_pane::WindowPane as _;
use libc::{abs, memcmp, strcasecmp, strchr, strcmp, strncmp};
use libc::{REG_EXTENDED, REG_ICASE};
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::time::Duration;

#[repr(C)]
pub struct window_copy_mode_data {
    pub screen: screen,
    /// Owned snapshot for copy mode, or command output for view mode.
    backing: Option<Box<screen>>,
    pub backing_written: ::core::ffi::c_int,
    pub ictx: Option<Box<input_ctx>>,
    pub sync_added: u_int,
    pub sync_collected: u_int,
    pub sync_generation: u_int,
    pub viewmode: ::core::ffi::c_int,
    pub oy: u_int,
    pub selx: u_int,
    pub sely: u_int,
    pub endselx: u_int,
    pub endsely: u_int,
    pub cursordrag: C2RustUnnamed_45,
    pub modekeys: ::core::ffi::c_int,
    pub lineflag: C2RustUnnamed_44,
    pub rectflag: ::core::ffi::c_int,
    pub scroll_exit: ::core::ffi::c_int,
    pub hide_position: ::core::ffi::c_int,
    pub line_numbers: ::core::ffi::c_int,
    pub selflag: C2RustUnnamed_43,
    pub recentre_state: C2RustUnnamed_42,
    pub recentre_line: u_int,
    pub separators: Option<CString>,
    pub dx: u_int,
    pub dy: u_int,
    pub selrx: u_int,
    pub selry: u_int,
    pub endselrx: u_int,
    pub endselry: u_int,
    pub cx: u_int,
    pub cy: u_int,
    pub lastcx: u_int,
    pub lastsx: u_int,
    pub mx: u_int,
    pub my: u_int,
    pub showmark: ::core::ffi::c_int,
    pub searchtype: ::core::ffi::c_int,
    pub searchdirection: ::core::ffi::c_int,
    pub searchregex: ::core::ffi::c_int,
    searchstr: Option<CString>,
    /// Empty when no search marks are active; retains capacity between searches.
    pub searchmark: Vec<u8>,
    pub searchcount: ::core::ffi::c_int,
    pub searchmore: ::core::ffi::c_int,
    pub searchall: ::core::ffi::c_int,
    pub searchx: ::core::ffi::c_int,
    pub searchy: ::core::ffi::c_int,
    pub searcho: ::core::ffi::c_int,
    pub searchgen: u_char,
    pub timeout: ::core::ffi::c_int,
    pub jumptype: ::core::ffi::c_int,
    pub jumpchar: Vec<utf8_data>,
    pub dragtimer: Option<Timer>,
    pub refresh_timer: Option<Timer>,
    pub refresh_active: ::core::ffi::c_int,
}

impl window_copy_mode_data {
    fn backing(&self) -> &screen {
        self.backing
            .as_deref()
            .expect("copy backing is initialized")
    }

    fn backing_mut(&mut self) -> &mut screen {
        self.backing
            .as_deref_mut()
            .expect("copy backing is initialized")
    }

    fn clear_backing(&mut self) {
        if let Some(mut backing) = self.backing.take() {
            drop(backing);
        }
    }
}

impl Drop for window_copy_mode_data {
    fn drop(&mut self) {
        drop(self.ictx.take());
        self.clear_backing();
    }
}

impl Default for window_copy_mode_data {
    fn default() -> Self {
        Self {
            screen: screen::empty(),
            backing: None,
            backing_written: 0,
            ictx: None,
            sync_added: 0,
            sync_collected: 0,
            sync_generation: 0,
            viewmode: 0,
            oy: 0,
            selx: 0,
            sely: 0,
            endselx: 0,
            endsely: 0,
            cursordrag: CURSORDRAG_NONE,
            modekeys: 0,
            lineflag: LINE_SEL_NONE,
            rectflag: 0,
            scroll_exit: 0,
            hide_position: 0,
            line_numbers: 0,
            selflag: SEL_CHAR,
            recentre_state: RECENTRE_TOP,
            recentre_line: 0,
            separators: None,
            dx: 0,
            dy: 0,
            selrx: 0,
            selry: 0,
            endselrx: 0,
            endselry: 0,
            cx: 0,
            cy: 0,
            lastcx: 0,
            lastsx: 0,
            mx: 0,
            my: 0,
            showmark: 0,
            searchtype: 0,
            searchdirection: 0,
            searchregex: 0,
            searchstr: None,
            searchmark: Vec::new(),
            searchcount: 0,
            searchmore: 0,
            searchall: 0,
            searchx: 0,
            searchy: 0,
            searcho: 0,
            searchgen: 0,
            timeout: 0,
            jumptype: 0,
            jumpchar: Vec::new(),
            dragtimer: None,
            refresh_timer: None,
            refresh_active: 0,
        }
    }
}

fn window_copy_searchstr(data: &window_copy_mode_data) -> Option<&CStr> {
    data.searchstr.as_deref()
}

fn window_copy_separators(data: &window_copy_mode_data) -> Option<&CStr> {
    data.separators.as_deref()
}

unsafe fn window_copy_snapshot_separators(scope: &OptionsScope) -> Option<CString> {
    scope.with_local(|oo| crate::src::options::options_get_string_optional(oo, c"word-separators"))
}

#[cfg(test)]
mod separator_snapshot_tests {
    use super::*;
    use crate::src::options::options_table;
    use crate::src::options::{options_default, options_set_string};

    #[test]
    fn selected_word_separators_survive_option_replacement() {
        unsafe {
            let scope = OptionsScope::GlobalSession;
            let definition = options_table
                .iter()
                .find(|entry| entry.name == Some(c"word-separators"))
                .unwrap();
            scope.with_local(|oo| {
                options_default(oo, definition);
                options_set_string(oo, c"word-separators", 0, |out| out.write_all(b"original"));
            });
            let mut data = window_copy_mode_data::default();
            data.separators = window_copy_snapshot_separators(&scope);
            scope.with_local(|oo| {
                options_set_string(oo, c"word-separators", 0, |out| {
                    out.write_all(b"replacement")
                });
            });
            assert_eq!(window_copy_separators(&data), Some(c"original"));
        }
    }
}
pub type C2RustUnnamed_42 = ::core::ffi::c_uint;
pub const RECENTRE_BOTTOM: C2RustUnnamed_42 = 2;
pub const RECENTRE_MIDDLE: C2RustUnnamed_42 = 1;
pub const RECENTRE_TOP: C2RustUnnamed_42 = 0;
pub type C2RustUnnamed_43 = ::core::ffi::c_uint;
pub const SEL_LINE: C2RustUnnamed_43 = 2;
pub const SEL_WORD: C2RustUnnamed_43 = 1;
pub const SEL_CHAR: C2RustUnnamed_43 = 0;
pub type C2RustUnnamed_44 = ::core::ffi::c_uint;
pub const LINE_SEL_RIGHT_LEFT: C2RustUnnamed_44 = 2;
pub const LINE_SEL_LEFT_RIGHT: C2RustUnnamed_44 = 1;
pub const LINE_SEL_NONE: C2RustUnnamed_44 = 0;
pub type C2RustUnnamed_45 = ::core::ffi::c_uint;
pub const CURSORDRAG_SEL: C2RustUnnamed_45 = 2;
pub const CURSORDRAG_ENDSEL: C2RustUnnamed_45 = 1;
pub const CURSORDRAG_NONE: C2RustUnnamed_45 = 0;
pub const WINDOW_COPY_LINE_NUMBERS_OFF: window_copy_line_numbers = 0;
pub const WINDOW_COPY_LINE_NUMBERS_DEFAULT: window_copy_line_numbers = 1;
pub const WINDOW_COPY_LINE_NUMBERS_HYBRID: window_copy_line_numbers = 4;
pub const WINDOW_COPY_LINE_NUMBERS_ABSOLUTE: window_copy_line_numbers = 2;
pub const WINDOW_COPY_CMD_MOVE: window_copy_cmd_action = 1;
pub type window_copy_cmd_action = ::core::ffi::c_uint;
pub const WINDOW_COPY_CMD_CANCEL: window_copy_cmd_action = 3;
pub const WINDOW_COPY_CMD_REDRAW: window_copy_cmd_action = 2;
pub const WINDOW_COPY_CMD_NOTHING: window_copy_cmd_action = 0;
pub const WINDOW_COPY_CMD_CLEAR_NEVER: window_copy_cmd_clear = 1;
pub type window_copy_cmd_clear = ::core::ffi::c_uint;
pub const WINDOW_COPY_CMD_CLEAR_EMACS_ONLY: window_copy_cmd_clear = 2;
pub const WINDOW_COPY_CMD_CLEAR_ALWAYS: window_copy_cmd_clear = 0;
pub struct window_copy_cmd_state<'a> {
    pub wme: refbox::Weak<window_mode_entry>,
    pub format_search: bool,
    pub wargs: Option<Box<args>>,
    pub m: Option<mouse_event>,
    pub c: Option<&'a ClientRef>,
    pub s: Option<&'a SessionRef>,
    pub wl: refbox::Weak<winlink>,
}
impl window_copy_cmd_state<'_> {
    /// Command callbacks run synchronously before their caller resets the mode.
    /// Recheck after nested callbacks, which may remove the entry.
    fn mode_handle(&self) -> refbox::Weak<window_mode_entry> {
        assert!(
            self.wme.is_alive(),
            "copy mode was removed during command dispatch"
        );
        self.wme.clone()
    }

    fn session_handle(&self) -> Option<SessionRef> {
        self.s.cloned()
    }

    /// Observe the command target without extending its lifetime.
    fn winlink_handle(&self) -> refbox::Weak<winlink> {
        self.wl.clone()
    }

    fn parsed_args(&mut self) -> &mut args {
        self.wargs
            .as_deref_mut()
            .expect("parsed copy-mode arguments")
    }
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_46 {
    pub command: &'static CStr,
    pub args: args_parse,
    pub flags: ::core::ffi::c_int,
    pub clear: window_copy_cmd_clear,
    pub f: Option<unsafe fn(*mut window_copy_cmd_state) -> window_copy_cmd_action>,
}
pub const WINDOW_COPY_REL_POS_ON_SCREEN: C2RustUnnamed_50 = 1;
pub const WINDOW_COPY_REL_POS_BELOW: C2RustUnnamed_50 = 2;
pub const WINDOW_COPY_REL_POS_ABOVE: C2RustUnnamed_50 = 0;
pub const WINDOW_COPY_SEARCHDOWN: C2RustUnnamed_49 = 2;
pub const WINDOW_COPY_SEARCHUP: C2RustUnnamed_49 = 1;
pub const BOTTOM: C2RustUnnamed_48 = 2;
pub const TOP: C2RustUnnamed_48 = 1;
pub const MIDDLE: C2RustUnnamed_48 = 0;
pub type C2RustUnnamed_48 = ::core::ffi::c_uint;
pub const WINDOW_COPY_JUMPTOFORWARD: C2RustUnnamed_49 = 5;
pub const WINDOW_COPY_JUMPTOBACKWARD: C2RustUnnamed_49 = 6;
pub const WINDOW_COPY_JUMPBACKWARD: C2RustUnnamed_49 = 4;
pub const WINDOW_COPY_JUMPFORWARD: C2RustUnnamed_49 = 3;
pub const WINDOW_COPY_OFF: C2RustUnnamed_49 = 0;
pub type C2RustUnnamed_49 = ::core::ffi::c_uint;
pub type C2RustUnnamed_50 = ::core::ffi::c_uint;
pub type window_copy_line_numbers = ::core::ffi::c_uint;
#[inline]
unsafe fn tolower(mut __c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if __c >= -(128 as ::core::ffi::c_int) && __c < 256 as ::core::ffi::c_int {
        ::libc::tolower(__c as ::core::ffi::c_int) as ::core::ffi::c_int
    } else {
        __c
    }
}

pub use ::libc::REG_NOTBOL;
pub static window_copy_mode: window_mode = {
    window_mode {
        name: c"copy-mode",
        default_format: None,
        flags: 0,
        init: Some(
            window_copy_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
                    *mut cmd_find_state,
                    *mut args,
                ) -> *mut screen,
        ),
        free: None,
        resize: Some(
            window_copy_resize as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: None,
        style_changed: Some(
            window_copy_style_changed as unsafe fn(refbox::Weak<window_mode_entry>) -> (),
        ),
        key: None,
        key_table: Some(
            window_copy_key_table as unsafe fn(refbox::Weak<window_mode_entry>) -> &'static CStr,
        ),
        command: Some(
            window_copy_command
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&ClientRef>,
                    Option<&SessionRef>,
                    refbox::Weak<winlink>,
                    *mut args,
                    *mut mouse_event,
                ) -> (),
        ),
        formats: Some(
            window_copy_formats
                as unsafe fn(refbox::Weak<window_mode_entry>, *mut format_tree) -> (),
        ),
        get_screen: Some(
            window_copy_get_screen as unsafe fn(refbox::Weak<window_mode_entry>) -> *mut screen,
        ),
        display_screen: Some(window_copy_display_screen),
    }
};
pub static window_view_mode: window_mode = {
    window_mode {
        name: c"view-mode",
        default_format: None,
        flags: 0,
        init: Some(
            window_copy_view_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
                    *mut cmd_find_state,
                    *mut args,
                ) -> *mut screen,
        ),
        free: None,
        resize: Some(
            window_copy_resize as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: None,
        style_changed: Some(
            window_copy_style_changed as unsafe fn(refbox::Weak<window_mode_entry>) -> (),
        ),
        key: None,
        key_table: Some(
            window_copy_key_table as unsafe fn(refbox::Weak<window_mode_entry>) -> &'static CStr,
        ),
        command: Some(
            window_copy_command
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&ClientRef>,
                    Option<&SessionRef>,
                    refbox::Weak<winlink>,
                    *mut args,
                    *mut mouse_event,
                ) -> (),
        ),
        formats: Some(
            window_copy_formats
                as unsafe fn(refbox::Weak<window_mode_entry>, *mut format_tree) -> (),
        ),
        get_screen: Some(
            window_copy_get_screen as unsafe fn(refbox::Weak<window_mode_entry>) -> *mut screen,
        ),
        display_screen: Some(window_copy_display_screen),
    }
};
pub const WINDOW_COPY_SEARCH_TIMEOUT: ::core::ffi::c_int = 10000 as ::core::ffi::c_int;
pub const WINDOW_COPY_SEARCH_ALL_TIMEOUT: ::core::ffi::c_int = 200 as ::core::ffi::c_int;
pub const WINDOW_COPY_SEARCH_MAX_LINE: ::core::ffi::c_int = 2000 as ::core::ffi::c_int;
pub const WINDOW_COPY_DRAG_REPEAT_TIME: ::core::ffi::c_int = 50000 as ::core::ffi::c_int;
pub const WINDOW_COPY_REFRESH_INTERVAL: ::core::ffi::c_int = 50000 as ::core::ffi::c_int;
fn window_copy_timer(
    observer: refbox::Weak<window_mode_entry>,
    delay: Duration,
    callback: unsafe fn(refbox::Weak<window_mode_entry>),
) -> Timer {
    Timer::new(delay, move || unsafe {
        let live = match observer.try_borrow_mut() {
            Ok(_) => true,
            Err(refbox::BorrowError::Dropped) => false,
            Err(refbox::BorrowError::Borrowed) => panic!("copy mode already borrowed"),
        };
        if live {
            callback(observer.clone());
        }
    })
    .expect("arm timer")
}
unsafe fn window_copy_scroll_timer(wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let tv = Duration::from_micros(WINDOW_COPY_DRAG_REPEAT_TIME as u64);
    drop((*data).dragtimer.take());
    if mode_pane_owner.mode_entry() != wme {
        return;
    }
    if (*data).cy == 0 as u_int {
        (*data).dragtimer = Some(window_copy_timer(wme.clone(), tv, window_copy_scroll_timer));
        window_copy_cursor_up(wme.clone(), 1 as ::core::ffi::c_int);
    } else if (*data).cy == (*data).screen.grid().sy.wrapping_sub(1 as u_int) {
        (*data).dragtimer = Some(window_copy_timer(wme.clone(), tv, window_copy_scroll_timer));
        window_copy_cursor_down(wme.clone(), 1 as ::core::ffi::c_int);
    }
}
pub(crate) unsafe fn window_copy_clone_screen(
    src: &screen,
    hint: &screen,
    mut cursor: Option<(&mut u_int, &mut u_int)>,
    trim: bool,
) -> Box<screen> {
    let mut dst = Box::new(screen::empty());
    let mut sy = src.grid().hsize.wrapping_add(src.grid().sy);
    if trim {
        while sy > src.grid().hsize.wrapping_add(1) {
            let Some(gl) = grid_peek_line(src.grid(), sy.wrapping_sub(1)) else {
                break;
            };
            if gl.cellused != 0 {
                break;
            }
            sy = sy.wrapping_sub(1);
        }
    }
    log_debug(format_args!(
        "{}: target screen is {}x{}, source {}x{}",
        "window_copy_clone_screen",
        src.grid().sx,
        sy,
        hint.grid().sx,
        src.grid().hsize.wrapping_add(src.grid().sy)
    ));
    screen_init(&mut dst, src.grid().sx, sy, src.grid().hlimit);
    dst.grid_mut().flags |= GRID_HISTORY;
    grid_duplicate_lines(dst.grid_mut(), 0, src.grid(), 0, sy);
    dst.grid_mut().sy = sy.wrapping_sub(src.grid().hsize);
    dst.grid_mut().hsize = src.grid().hsize;
    dst.grid_mut().hscrolled = src.grid().hscrolled;
    if src.cy > dst.grid().sy.wrapping_sub(1) {
        dst.cx = 0;
        dst.cy = dst.grid().sy.wrapping_sub(1);
    } else {
        dst.cx = src.cx;
        dst.cy = src.cy;
    }
    let reflow = cursor.is_some() && hint.grid().sx != dst.grid().sx;
    let mut wrapped = (0, 0);
    if let Some((cx, cy)) = cursor.as_mut() {
        **cx = dst.cx;
        **cy = dst.grid().hsize.wrapping_add(dst.cy);
        if reflow {
            wrapped = grid_wrap_position(dst.grid(), **cx, **cy);
        }
    }
    screen_resize_cursor(&mut dst, hint.grid().sx, hint.grid().sy, 1, 0, 0);
    if reflow {
        let (cx, cy) = cursor.expect("reflow cursor");
        (*cx, *cy) = grid_unwrap_position(dst.grid(), wrapped.0, wrapped.1);
    }
    dst
}

unsafe fn window_copy_sync_snapshot(data: *mut window_copy_mode_data, sync: (u_int, u_int, u_int)) {
    (
        (*data).sync_added,
        (*data).sync_collected,
        (*data).sync_generation,
    ) = sync;
}
pub(crate) unsafe fn window_copy_sync_screen(
    source: &screen,
    target: &mut screen,
    sync: (u_int, u_int, u_int),
) -> bool {
    let dst: *mut screen = target;
    let sg: *const grid = source.grid();
    let mut dg: *mut grid = (*dst).grid_mut();
    let mut sy: u_int = (*sg).sy;
    let mut old_hsize: u_int = (*dg).hsize;
    let mut new_hsize: u_int = (*sg).hsize;
    let mut added: u_int = 0;
    let mut collected: u_int = 0;
    let mut kept: u_int = 0;
    if (*sg).sx != (*dg).sx || (*sg).sy != (*dg).sy || (*sg).scroll_generation != sync.2 {
        return false;
    }
    added = (*sg).scroll_added.wrapping_sub(sync.0);
    collected = (*sg).scroll_collected.wrapping_sub(sync.1);
    if added > INT_MAX as u_int
        || collected > INT_MAX as u_int
        || collected > old_hsize
        || old_hsize.wrapping_add(added) < collected
        || old_hsize.wrapping_add(added).wrapping_sub(collected) != new_hsize
    {
        return false;
    }
    kept = old_hsize.wrapping_sub(collected);
    if added == 0 as u_int && collected == 0 as u_int {
        grid_duplicate_lines(&mut *dg, old_hsize, &*sg, new_hsize, sy);
    } else {
        if collected > 0 as u_int {
            grid_free_lines(&mut *dg, 0 as u_int, collected);
            let live = old_hsize.wrapping_add(sy) as usize;
            (&mut (*dg).linedata)[..live].rotate_left(collected as usize);
        }
        (*dg)
            .linedata
            .resize_with(new_hsize.wrapping_add(sy) as usize, grid_line::default);
        (*dg).hsize = new_hsize;
        if added > 0 as u_int {
            grid_duplicate_lines(&mut *dg, kept, &*sg, kept, added);
        }
        grid_duplicate_lines(&mut *dg, new_hsize, &*sg, new_hsize, sy);
    }
    (*dg).hscrolled = (*sg).hscrolled;
    if source.cy > (*dg).sy.wrapping_sub(1 as u_int) {
        (*dst).cx = 0 as u_int;
        (*dst).cy = (*dg).sy.wrapping_sub(1 as u_int);
    } else {
        (*dst).cx = source.cx;
        (*dst).cy = source.cy;
    }
    true
}
unsafe fn window_copy_sync_backing(wme: refbox::Weak<window_mode_entry>) -> ::core::ffi::c_int {
    let data = window_copy_data(wme.clone());
    let Some(source) = wme.get_unchecked().swp.upgrade() else {
        return 0;
    };
    if (*data).viewmode != 0 || !wme.get_unchecked().swp.ptr_eq(&wme.get_unchecked().wp) {
        return 0;
    }
    let sync = (
        (*data).sync_added,
        (*data).sync_collected,
        (*data).sync_generation,
    );
    source.sync_history(&mut *(*data).backing_mut(), sync) as i32
}
unsafe fn window_copy_data(wme: refbox::Weak<window_mode_entry>) -> *mut window_copy_mode_data {
    wme.get_unchecked()
        .boxed_data_ptr::<window_copy_mode_data>()
        .expect("copy mode payload")
}

unsafe fn window_copy_common_init(
    mut wme: refbox::Weak<window_mode_entry>,
) -> *mut window_copy_mode_data {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let (sx, sy) = mode_pane_owner.screen_size(false);
    let (search, regex) = mode_pane_owner.saved_search();
    let owner = Box::new(std::cell::UnsafeCell::new(window_copy_mode_data::default()));
    let data = owner.get();
    wme.get_mut_unchecked().boxed_data = Some(owner);
    (*data).cursordrag = CURSORDRAG_NONE;
    (*data).lineflag = LINE_SEL_NONE;
    (*data).selflag = SEL_CHAR;
    if search.is_some() {
        (*data).searchtype = WINDOW_COPY_SEARCHUP as ::core::ffi::c_int;
        (*data).searchregex = regex;
        (*data).searchstr = search;
    } else {
        (*data).searchtype = WINDOW_COPY_OFF as ::core::ffi::c_int;
        (*data).searchregex = 0 as ::core::ffi::c_int;
        (*data).searchstr = None;
    }
    (*data).searcho = -(1 as ::core::ffi::c_int);
    (*data).searchy = (*data).searcho;
    (*data).searchx = (*data).searchy;
    (*data).searchall = 1 as ::core::ffi::c_int;
    (*data).jumptype = WINDOW_COPY_OFF as ::core::ffi::c_int;
    (*data).line_numbers = 1 as ::core::ffi::c_int;
    screen_init(&mut (*data).screen, sx, sy, 0 as u_int);
    screen_set_default_cursor(&mut (*data).screen, &OptionsScope::GlobalWindow);
    (*data).modekeys = options_get_number(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live window")
            .borrow_options_mut(),
        c"mode-keys",
    ) as ::core::ffi::c_int;
    data
}
unsafe fn window_copy_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    _fs: *mut cmd_find_state,
    mut args: *mut args,
) -> *mut screen {
    let Some(source_owner) = wme.get_unchecked().swp.upgrade() else {
        return std::ptr::null_mut();
    };
    let mut data: *mut window_copy_mode_data = ::core::ptr::null_mut::<window_copy_mode_data>();
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut i: u_int = 0;
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    data = window_copy_common_init(wme.clone());
    (*data).backing = Some(source_owner.clone_history(
        &(*data).screen,
        Some((&mut cx, &mut cy)),
        !wme.get_unchecked().swp.ptr_eq(&wme.get_unchecked().wp),
    ));
    window_copy_sync_snapshot(data, source_owner.history_scroll());
    (*data).cx = cx;
    if cy < (*data).backing().grid().hsize {
        (*data).cy = 0 as u_int;
        (*data).oy = (*data).backing().grid().hsize.wrapping_sub(cy);
    } else {
        (*data).cy = cy.wrapping_sub((*data).backing().grid().hsize);
        (*data).oy = 0 as u_int;
    }
    (*data).scroll_exit = args_has(&*args, 'e' as i32 as u_char);
    (*data).hide_position = args_has(&*args, 'H' as i32 as u_char);
    source_owner.share_screen_hyperlinks(&mut (*data).screen);
    (*data).screen.cx =
        window_copy_cursor_offset(wme.clone(), (*data).cx, (*data).screen.grid().sx);
    (*data).screen.cy = (*data).cy;
    (*data).mx = (*data).cx;
    (*data).my = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).showmark = 0 as ::core::ffi::c_int;
    screen_write_start(&mut ctx, &raw mut (*data).screen);
    i = 0 as u_int;
    while i < (*data).screen.grid().sy {
        window_copy_write_line(wme.clone(), &raw mut ctx, i);
        i = i.wrapping_add(1);
    }
    screen_write_cursormove(
        &mut ctx,
        window_copy_cursor_offset(wme.clone(), (*data).cx, (*data).screen.grid().sx)
            as ::core::ffi::c_int,
        (*data).cy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_stop(&mut ctx);
    (*data).recentre_state = RECENTRE_MIDDLE;
    (*data).recentre_line = 0 as u_int;
    &raw mut (*data).screen
}
unsafe fn window_copy_view_init(
    mut wme: refbox::Weak<window_mode_entry>,
    _item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    _fs: *mut cmd_find_state,
    _args: *mut args,
) -> *mut screen {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = ::core::ptr::null_mut::<window_copy_mode_data>();
    let (sx, sy) = mode_pane_owner.screen_size(false);
    data = window_copy_common_init(wme.clone());
    (*data).viewmode = 1 as ::core::ffi::c_int;
    (*data).line_numbers = 0 as ::core::ffi::c_int;
    (*data).backing = Some(Box::new(screen::empty()));
    screen_init((*data).backing_mut(), sx, sy, UINT_MAX);
    (*data).ictx = Some(input_init(
        None,
        ::core::ptr::null_mut::<bufferevent>(),
        Default::default(),
        None,
    ));
    (*data).mx = (*data).cx;
    (*data).my = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).showmark = 0 as ::core::ffi::c_int;
    &raw mut (*data).screen
}
pub unsafe fn window_copy_add(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut parse: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut backing: *mut screen = (*data).backing_mut();
    let mut backing_ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
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
    let mut old_hsize: u_int = 0;
    let mut old_cy: u_int = 0;
    old_hsize = (*data).backing().grid().hsize;
    screen_write_start(&mut backing_ctx, backing);
    if (*data).backing_written != 0 {
        screen_write_carriagereturn(&mut backing_ctx);
        screen_write_linefeed(&mut backing_ctx, 0 as ::core::ffi::c_int, 8 as u_int);
    } else {
        (*data).backing_written = 1 as ::core::ffi::c_int;
    }
    old_cy = (*backing).cy;
    if parse != 0 {
        let text = format_message_with(write);
        input_parse_screen(
            (*data)
                .ictx
                .as_deref_mut()
                .map_or(std::ptr::null_mut(), |ictx| ictx),
            backing,
            Some(Box::new(|_| {})),
            text.as_bytes(),
        );
    } else {
        gc = grid_default_cell;
        screen_write_nputs(&mut backing_ctx, 0 as ssize_t, &gc, write);
    }
    screen_write_stop(&mut backing_ctx);
    (*data).oy = (*data)
        .oy
        .wrapping_add((*data).backing().grid().hsize.wrapping_sub(old_hsize));
    screen_write_start_pane(&mut ctx, pane_owner, &raw mut (*data).screen);
    if (*data).backing().grid().hsize != 0 {
        window_copy_redraw_lines(wme.clone(), 0 as u_int, 1 as u_int);
    }
    window_copy_redraw_lines(
        wme.clone(),
        old_cy,
        (*backing).cy.wrapping_sub(old_cy).wrapping_add(1 as u_int),
    );
    screen_write_stop(&mut ctx);
}
pub unsafe fn window_copy_scroll(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut sl_mpos: ::core::ffi::c_int,
    mut my: u_int,
    mut tty_oy: u_int,
    mut scroll_exit: ::core::ffi::c_int,
) {
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    if !!wme.is_alive() {
        WindowRef::clone(&pane_owner.window_observer().upgrade().expect("live window"))
            .select_pane(pane_owner, false);
        window_copy_scroll1(wme.clone(), pane_owner, sl_mpos, my, tty_oy, scroll_exit);
    }
}
unsafe fn window_copy_scroll1(
    mut wme: refbox::Weak<window_mode_entry>,
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut sl_mpos: ::core::ffi::c_int,
    mut my: u_int,
    mut tty_oy: u_int,
    mut scroll_exit: ::core::ffi::c_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut n: u_int = 0;
    let mut new_offset: u_int = 0;
    let (_, sb_height, _, yoff) = pane_owner.geometry();
    let slider_height = pane_owner.scrollbar_slider_height();
    let sb_top = yoff as u_int;
    let mut sy: u_int = (*data).backing().grid().sy;
    let mut my_w: u_int = 0;
    let mut new_slider_y: ::core::ffi::c_int = 0;
    let mut delta: ::core::ffi::c_int = 0;
    my_w = my.wrapping_add(tty_oy);
    if my_w <= sb_top.wrapping_add(sl_mpos as u_int) {
        new_slider_y = sb_top.wrapping_sub(yoff as u_int) as ::core::ffi::c_int;
    } else if my_w.wrapping_sub(sl_mpos as u_int)
        > sb_top.wrapping_add(sb_height).wrapping_sub(slider_height)
    {
        new_slider_y = sb_top
            .wrapping_sub(yoff as u_int)
            .wrapping_add(sb_height.wrapping_sub(slider_height))
            as ::core::ffi::c_int;
    } else {
        new_slider_y = my_w
            .wrapping_sub(yoff as u_int)
            .wrapping_sub(sl_mpos as u_int) as ::core::ffi::c_int;
    }
    let Some((offset, size)) = window_copy_get_current_offset(pane_owner) else {
        return;
    };
    new_offset = (new_slider_y as ::core::ffi::c_float
        * (size.wrapping_add(sb_height) as ::core::ffi::c_float
            / sb_height as ::core::ffi::c_float)) as u_int;
    delta = (offset as ::core::ffi::c_int as u_int).wrapping_sub(new_offset) as ::core::ffi::c_int;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    ox = window_copy_find_length(wme.clone(), oy);
    if (*data).cx != ox {
        (*data).lastcx = (*data).cx;
        (*data).lastsx = ox;
    }
    (*data).cx = (*data).lastcx;
    if delta >= 0 as ::core::ffi::c_int {
        n = delta as u_int;
        if (*data).oy.wrapping_add(n) > (*data).backing().grid().hsize {
            (*data).oy = (*data).backing().grid().hsize;
            if (*data).cy < n {
                (*data).cy = 0 as u_int;
            } else {
                (*data).cy = (*data).cy.wrapping_sub(n);
            }
        } else {
            (*data).oy = (*data).oy.wrapping_add(n);
        }
    } else {
        n = -delta as u_int;
        if (*data).oy < n {
            (*data).oy = 0 as u_int;
            if (*data).cy.wrapping_add(n.wrapping_sub((*data).oy)) >= sy {
                (*data).cy = sy.wrapping_sub(1 as u_int);
            } else {
                (*data).cy = (*data).cy.wrapping_add(n.wrapping_sub((*data).oy));
            }
        } else {
            (*data).oy = (*data).oy.wrapping_sub(n);
        }
    }
    (*data).cursordrag = CURSORDRAG_NONE;
    if (*data).screen.sel.is_none() || (*data).rectflag == 0 {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        px = window_copy_find_length(wme.clone(), py);
        if (*data).cx >= (*data).lastsx && (*data).cx != px || (*data).cx > px {
            window_copy_cursor_end_of_line(wme.clone());
        }
    }
    if scroll_exit != 0 && (*data).oy == 0 as u_int && (*data).screen.sel.is_none() {
        pane_owner.reset_mode();
        return;
    }
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection_view(wme.clone(), 1 as ::core::ffi::c_int);
    pane_owner.show_scrollbar();
    window_copy_redraw_screen(wme.clone());
}
pub unsafe fn window_copy_pageup(pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>) {
    let mut half_page: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    window_copy_pageup1(pane_owner.mode_entry(), half_page);
}
unsafe fn window_copy_pageup1(
    mut wme: refbox::Weak<window_mode_entry>,
    mut half_page: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut n: u_int = 0;
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    ox = window_copy_find_length(wme.clone(), oy);
    if (*data).cx != ox {
        (*data).lastcx = (*data).cx;
        (*data).lastsx = ox;
    }
    (*data).cx = (*data).lastcx;
    n = 1 as u_int;
    if (*s).grid().sy > 2 as u_int {
        if half_page != 0 {
            n = (*s).grid().sy.wrapping_div(2 as u_int);
        } else {
            n = (*s).grid().sy.wrapping_sub(2 as u_int);
        }
    }
    if (*data).oy.wrapping_add(n) > (*data).backing().grid().hsize {
        (*data).oy = (*data).backing().grid().hsize;
        if (*data).cy < n {
            (*data).cy = 0 as u_int;
        } else {
            (*data).cy = (*data).cy.wrapping_sub(n);
        }
    } else {
        (*data).oy = (*data).oy.wrapping_add(n);
    }
    if (*data).screen.sel.is_none() || (*data).rectflag == 0 {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        px = window_copy_find_length(wme.clone(), py);
        if (*data).cx >= (*data).lastsx && (*data).cx != px || (*data).cx > px {
            window_copy_cursor_end_of_line(wme.clone());
        }
    }
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    mode_pane_owner.show_scrollbar();
    window_copy_redraw_screen(wme.clone());
}
pub unsafe fn window_copy_pagedown(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut scroll_exit: ::core::ffi::c_int,
) {
    let mut half_page: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if window_copy_pagedown1(pane_owner.mode_entry(), half_page, scroll_exit) != 0 {
        pane_owner.reset_mode();
    }
}
unsafe fn window_copy_pagedown1(
    mut wme: refbox::Weak<window_mode_entry>,
    mut half_page: ::core::ffi::c_int,
    mut scroll_exit: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut n: u_int = 0;
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    ox = window_copy_find_length(wme.clone(), oy);
    if (*data).cx != ox {
        (*data).lastcx = (*data).cx;
        (*data).lastsx = ox;
    }
    (*data).cx = (*data).lastcx;
    n = 1 as u_int;
    if (*s).grid().sy > 2 as u_int {
        if half_page != 0 {
            n = (*s).grid().sy.wrapping_div(2 as u_int);
        } else {
            n = (*s).grid().sy.wrapping_sub(2 as u_int);
        }
    }
    if (*data).oy < n {
        (*data).oy = 0 as u_int;
        if (*data).cy.wrapping_add(n.wrapping_sub((*data).oy)) >= (*data).backing().grid().sy {
            (*data).cy = (*data).backing().grid().sy.wrapping_sub(1 as u_int);
        } else {
            (*data).cy = (*data).cy.wrapping_add(n.wrapping_sub((*data).oy));
        }
    } else {
        (*data).oy = (*data).oy.wrapping_sub(n);
    }
    if (*data).screen.sel.is_none() || (*data).rectflag == 0 {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        px = window_copy_find_length(wme.clone(), py);
        if (*data).cx >= (*data).lastsx && (*data).cx != px || (*data).cx > px {
            window_copy_cursor_end_of_line(wme.clone());
        }
    }
    if scroll_exit != 0 && (*data).oy == 0 as u_int && (*data).screen.sel.is_none() {
        return 1 as ::core::ffi::c_int;
    }
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    mode_pane_owner.show_scrollbar();
    window_copy_redraw_screen(wme.clone());
    0 as ::core::ffi::c_int
}
unsafe fn window_copy_previous_paragraph(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut oy: u_int = 0;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    while oy > 0 as u_int && window_copy_find_length(wme.clone(), oy) == 0 as u_int {
        oy = oy.wrapping_sub(1);
    }
    while oy > 0 as u_int && window_copy_find_length(wme.clone(), oy) > 0 as u_int {
        oy = oy.wrapping_sub(1);
    }
    window_copy_scroll_to(wme.clone(), 0 as u_int, oy, 0 as ::core::ffi::c_int);
}
unsafe fn window_copy_next_paragraph(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut maxy: u_int = 0;
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    maxy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*s).grid().sy)
        .wrapping_sub(1 as u_int);
    while oy < maxy && window_copy_find_length(wme.clone(), oy) == 0 as u_int {
        oy = oy.wrapping_add(1);
    }
    while oy < maxy && window_copy_find_length(wme.clone(), oy) > 0 as u_int {
        oy = oy.wrapping_add(1);
    }
    ox = window_copy_find_length(wme.clone(), oy);
    window_copy_scroll_to(wme.clone(), ox, oy, 0 as ::core::ffi::c_int);
}
pub(crate) unsafe fn window_copy_get_word_cstring(
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut x: u_int,
    mut y: u_int,
) -> Option<CString> {
    let mut wme: refbox::Weak<window_mode_entry> = wp_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
    crate::src::format::format_grid_word_cstring(
        &*gd,
        x,
        (*gd).hsize.wrapping_add(y).wrapping_sub((*data).oy),
    )
}
pub(crate) unsafe fn window_copy_get_line_cstring(
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut y: u_int,
) -> Option<CString> {
    let mut wme: refbox::Weak<window_mode_entry> = wp_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
    crate::src::format::format_grid_line_cstring(
        &*gd,
        (*gd).hsize.wrapping_add(y).wrapping_sub((*data).oy),
    )
}
pub(crate) unsafe fn window_copy_get_hyperlink_cstring(
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut x: u_int,
    mut y: u_int,
) -> Option<CString> {
    let mut wme: refbox::Weak<window_mode_entry> = wp_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).screen.grid_mut();
    wp_owner.grid_hyperlink(&*gd, x, (*gd).hsize.wrapping_add(y))
}
unsafe fn window_copy_cursor_hyperlink_cb(mut ft: *mut format_tree) -> Option<CString> {
    let pane_owner = format_get_pane(&*ft)?;
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).screen.grid_mut();
    format_grid_hyperlink_cstring(
        &*gd,
        (*data).cx,
        (*gd).hsize.wrapping_add((*data).cy),
        &(*data).screen,
    )
}
unsafe fn window_copy_cursor_word_cb(mut ft: *mut format_tree) -> Option<CString> {
    let pane_owner = format_get_pane(&*ft)?;
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    window_copy_get_word_cstring(&pane_owner, (*data).cx, (*data).cy)
}
unsafe fn window_copy_cursor_line_cb(mut ft: *mut format_tree) -> Option<CString> {
    let pane_owner = format_get_pane(&*ft)?;
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    window_copy_get_line_cstring(&pane_owner, (*data).cy)
}
unsafe fn window_copy_search_match_cb(mut ft: *mut format_tree) -> Option<CString> {
    let pane_owner = format_get_pane(&*ft)?;
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    window_copy_match_at_cursor_cstring(data)
}
unsafe fn window_copy_formats(mut wme: refbox::Weak<window_mode_entry>, mut ft: *mut format_tree) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut hsize: u_int = (*data).backing().grid().hsize;
    let mut position: u_int = 0;
    let mut limit: u_int = 0;
    let mut t: time_t = 0;
    let gd = (*data).backing().grid();
    let gl = &gd.linedata[hsize.wrapping_sub((*data).oy) as usize];
    t = grid_line_time(gl);
    format_add(ft, c"top_line_time", |out| {
        write!(out, "{}", (t as ::core::ffi::c_ulonglong) as u64)
    });
    format_add(ft, c"scroll_position", |out| {
        write!(out, "{}", ((*data).oy) as i32)
    });
    if window_copy_line_number_is_absolute(wme.clone()) != 0 {
        position = hsize.wrapping_sub((*data).oy).wrapping_add(1 as u_int);
        limit = hsize.wrapping_add((*data).backing().grid().sy);
    } else {
        position = (*data).oy;
        limit = hsize;
    }
    format_add(ft, c"copy_position", |out| {
        write!(out, "{}", (position) as u32)
    });
    format_add(ft, c"copy_position_limit", |out| {
        write!(out, "{}", (limit) as u32)
    });
    format_add(ft, c"copy_line_numbers", |out| {
        write!(
            out,
            "{}",
            (window_copy_line_numbers_active(wme.clone())) as i32
        )
    });
    format_add(ft, c"refresh_active", |out| {
        write!(out, "{}", { (*data).refresh_active })
    });
    format_add(ft, c"rectangle_toggle", |out| {
        write!(out, "{}", { (*data).rectflag })
    });
    format_add(ft, c"copy_cursor_x", |out| {
        write!(out, "{}", ((*data).cx) as i32)
    });
    format_add(ft, c"copy_cursor_y", |out| {
        write!(out, "{}", ((*data).cy) as i32)
    });
    if !(*data).screen.sel.is_none() {
        format_add(ft, c"selection_start_x", |out| {
            write!(out, "{}", ((*data).selx) as i32)
        });
        format_add(ft, c"selection_start_y", |out| {
            write!(out, "{}", ((*data).sely) as i32)
        });
        format_add(ft, c"selection_end_x", |out| {
            write!(out, "{}", ((*data).endselx) as i32)
        });
        format_add(ft, c"selection_end_y", |out| {
            write!(out, "{}", ((*data).endsely) as i32)
        });
        if (*data).cursordrag as ::core::ffi::c_uint
            != CURSORDRAG_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            format_add(ft, c"selection_active", |out| out.write_all(b"1"));
        } else {
            format_add(ft, c"selection_active", |out| out.write_all(b"0"));
        }
        if (*data).endselx != (*data).selx || (*data).endsely != (*data).sely {
            format_add(ft, c"selection_present", |out| out.write_all(b"1"));
        } else {
            format_add(ft, c"selection_present", |out| out.write_all(b"0"));
        }
    } else {
        format_add(ft, c"selection_active", |out| out.write_all(b"0"));
        format_add(ft, c"selection_present", |out| out.write_all(b"0"));
    }
    match (*data).selflag as ::core::ffi::c_uint {
        0 => {
            format_add(ft, c"selection_mode", |out| out.write_all(b"char"));
        }
        1 => {
            format_add(ft, c"selection_mode", |out| out.write_all(b"word"));
        }
        2 => {
            format_add(ft, c"selection_mode", |out| out.write_all(b"line"));
        }
        _ => {}
    }
    format_add(ft, c"search_present", |out| {
        write!(out, "{}", {
            (!(*data).searchmark.is_empty()) as ::core::ffi::c_int
        })
    });
    format_add(ft, c"search_timed_out", |out| {
        write!(out, "{}", { (*data).timeout })
    });
    if (*data).searchcount != -(1 as ::core::ffi::c_int) {
        format_add(ft, c"search_count", |out| {
            write!(out, "{}", { (*data).searchcount })
        });
        format_add(ft, c"search_count_partial", |out| {
            write!(out, "{}", { (*data).searchmore })
        });
    }
    format_add_owned_cb(ft, c"search_match", |ft| unsafe {
        window_copy_search_match_cb(ft.as_ptr())
    });
    format_add_owned_cb(ft, c"copy_cursor_word", |ft| unsafe {
        window_copy_cursor_word_cb(ft.as_ptr())
    });
    format_add_owned_cb(ft, c"copy_cursor_line", |ft| unsafe {
        window_copy_cursor_line_cb(ft.as_ptr())
    });
    format_add_owned_cb(ft, c"copy_cursor_hyperlink", |ft| unsafe {
        window_copy_cursor_hyperlink_cb(ft.as_ptr())
    });
}
unsafe fn window_copy_get_screen(mut wme: refbox::Weak<window_mode_entry>) -> *mut screen {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).backing_mut()
}
unsafe fn window_copy_display_screen(wme: refbox::Weak<window_mode_entry>) -> *mut screen {
    let data = wme
        .get_unchecked()
        .boxed_data_ptr::<window_copy_mode_data>();
    data.map_or(std::ptr::null_mut(), |data| &raw mut (*data).screen)
}
unsafe fn window_copy_size_changed(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut search: ::core::ffi::c_int = (!(*data).searchmark.is_empty()) as ::core::ffi::c_int;
    window_copy_clear_selection(wme.clone());
    window_copy_clear_marks(wme.clone());
    screen_write_start(&mut ctx, s);
    window_copy_write_lines(wme.clone(), &raw mut ctx, 0 as u_int, (*s).grid().sy);
    screen_write_stop(&mut ctx);
    if search != 0 && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            0 as ::core::ffi::c_int,
        );
    }
    (*data).searchx = (*data).cx as ::core::ffi::c_int;
    (*data).searchy = (*data).cy as ::core::ffi::c_int;
    (*data).searcho = (*data).oy as ::core::ffi::c_int;
}
unsafe fn window_copy_resize(
    mut wme: refbox::Weak<window_mode_entry>,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut wx: u_int = 0;
    let mut wy: u_int = 0;
    let mut reflow: ::core::ffi::c_int = 0;
    screen_resize(&mut *s, sx, sy, 0 as ::core::ffi::c_int);
    let gd = (*data).backing().grid();
    cx = (*data).cx;
    if (*data).oy > gd.hsize.wrapping_add((*data).cy) {
        (*data).oy = gd.hsize.wrapping_add((*data).cy);
    }
    cy = gd.hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    reflow = (gd.sx != sx) as ::core::ffi::c_int;
    if reflow != 0 {
        (wx, wy) = grid_wrap_position(gd, cx, cy);
    }
    screen_resize_cursor(
        (*data).backing_mut(),
        sx,
        sy,
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let gd = (*data).backing().grid();
    if reflow != 0 {
        (cx, cy) = grid_unwrap_position(gd, wx, wy);
    }
    (*data).cx = cx;
    if cy < gd.hsize {
        (*data).cy = 0 as u_int;
        (*data).oy = gd.hsize.wrapping_sub(cy);
    } else {
        (*data).cy = cy.wrapping_sub(gd.hsize);
        (*data).oy = 0 as u_int;
    }
    window_copy_size_changed(wme.clone());
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_key_table(mut wme: refbox::Weak<window_mode_entry>) -> &'static CStr {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    if options_get_number(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live window")
            .borrow_options_mut(),
        c"mode-keys",
    ) == MODEKEY_VI as ::core::ffi::c_longlong
    {
        return c"copy-mode-vi";
    }
    c"copy-mode"
}
unsafe fn window_copy_expand_search_string(
    mut cs: *mut window_copy_cmd_state,
) -> ::core::ffi::c_int {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let Some(ss) = args_string(&mut *((*cs).parsed_args()), 0 as u_int).filter(|ss| !ss.is_empty())
    else {
        return 0 as ::core::ffi::c_int;
    };
    if (*cs).format_search {
        let expanded = format_single_cstring(
            None,
            ss.as_ptr(),
            None,
            None,
            (refbox::Weak::new()).clone(),
            Some(&mode_pane_owner),
        );
        if expanded.as_bytes().is_empty() {
            return 0 as ::core::ffi::c_int;
        }
        (*data).searchstr = Some(expanded);
    } else {
        (*data).searchstr = Some(ss.to_owned());
    }
    1 as ::core::ffi::c_int
}
unsafe fn window_copy_cmd_append_selection(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut s: Option<SessionRef> = (*cs).session_handle();
    if !s.is_none() {
        window_copy_append_selection(wme.clone());
    }
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_append_selection_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut s: Option<SessionRef> = (*cs).session_handle();
    if !s.is_none() {
        window_copy_append_selection(wme.clone());
    }
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_CANCEL
}
unsafe fn window_copy_cmd_back_to_indentation(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cursor_back_to_indentation(wme.clone());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_begin_selection(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    if let Some(mut m) = (*cs).m {
        window_copy_start_drag((*cs).c, &raw mut m);
        return WINDOW_COPY_CMD_MOVE;
    }
    (*data).lineflag = LINE_SEL_NONE;
    (*data).selflag = SEL_CHAR;
    window_copy_start_selection(wme.clone());
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_stop_selection(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).cursordrag = CURSORDRAG_NONE;
    (*data).lineflag = LINE_SEL_NONE;
    (*data).selflag = SEL_CHAR;
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_bottom_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).cx = 0 as u_int;
    (*data).cy = (*data).screen.grid().sy.wrapping_sub(1 as u_int);
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_REDRAW
}
fn window_copy_cmd_cancel(_cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    WINDOW_COPY_CMD_CANCEL
}
unsafe fn window_copy_cmd_clear_selection(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_do_copy_end_of_line(
    mut cs: *mut window_copy_cmd_state,
    mut pipe: ::core::ffi::c_int,
    mut cancel: ::core::ffi::c_int,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let c = (*cs).c;
    let mut s: Option<SessionRef> = (*cs).session_handle();
    let mut wl: refbox::Weak<winlink> = (*cs).winlink_handle();
    let mut count: u_int = args_count((*cs).parsed_args());
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut ocx: u_int = 0;
    let mut ocy: u_int = 0;
    let mut ooy: u_int = 0;
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut prefix: Option<CString> = None;
    let mut command: Option<CString> = None;
    let mut arg0: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut arg1: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 1 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut set_paste: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'P' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let mut set_clip: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'C' as i32 as u_char) == 0) as ::core::ffi::c_int;
    if pipe != 0 {
        if count == 2 as u_int {
            prefix = Some(format_single_cstring(
                None,
                arg1,
                c,
                s.as_ref(),
                wl.clone(),
                Some(&mode_pane_owner),
            ));
        }
        if !s.is_none() && count > 0 as u_int && *arg0 as ::core::ffi::c_int != '\0' as i32 {
            command = Some(format_single_cstring(
                None,
                arg0,
                c,
                s.as_ref(),
                wl.clone(),
                Some(&mode_pane_owner),
            ));
        }
    } else if count == 1 as u_int {
        prefix = Some(format_single_cstring(
            None,
            arg0,
            c,
            s.as_ref(),
            wl.clone(),
            Some(&mode_pane_owner),
        ));
    }
    ocx = (*data).cx;
    ocy = (*data).cy;
    ooy = (*data).oy;
    window_copy_start_selection(wme.clone());
    while np > 1 as u_int {
        window_copy_cursor_down(wme.clone(), 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    window_copy_cursor_end_of_line(wme.clone());
    if !s.is_none() {
        if pipe != 0 {
            window_copy_copy_pipe(
                wme.clone(),
                s.as_ref(),
                prefix.as_deref(),
                command.as_deref(),
                set_paste,
                set_clip,
            );
        } else {
            window_copy_copy_selection(wme.clone(), prefix.as_deref(), set_paste, set_clip);
        }
        if cancel != 0 {
            return WINDOW_COPY_CMD_CANCEL;
        }
    }
    window_copy_clear_selection(wme.clone());
    (*data).cx = ocx;
    (*data).cy = ocy;
    (*data).oy = ooy;
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_copy_end_of_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_end_of_line(cs, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_end_of_line_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_end_of_line(cs, 0 as ::core::ffi::c_int, 1 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_pipe_end_of_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_end_of_line(cs, 1 as ::core::ffi::c_int, 0 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_pipe_end_of_line_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_end_of_line(cs, 1 as ::core::ffi::c_int, 1 as ::core::ffi::c_int)
}
unsafe fn window_copy_do_copy_line(
    mut cs: *mut window_copy_cmd_state,
    mut pipe: ::core::ffi::c_int,
    mut cancel: ::core::ffi::c_int,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let c = (*cs).c;
    let mut s: Option<SessionRef> = (*cs).session_handle();
    let mut wl: refbox::Weak<winlink> = (*cs).winlink_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut count: u_int = args_count((*cs).parsed_args());
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut ocx: u_int = 0;
    let mut ocy: u_int = 0;
    let mut ooy: u_int = 0;
    let mut prefix: Option<CString> = None;
    let mut command: Option<CString> = None;
    let mut arg0: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut arg1: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 1 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut set_paste: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'P' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let mut set_clip: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'C' as i32 as u_char) == 0) as ::core::ffi::c_int;
    if pipe != 0 {
        if count == 2 as u_int {
            prefix = Some(format_single_cstring(
                None,
                arg1,
                c,
                s.as_ref(),
                wl.clone(),
                Some(&mode_pane_owner),
            ));
        }
        if !s.is_none() && count > 0 as u_int && *arg0 as ::core::ffi::c_int != '\0' as i32 {
            command = Some(format_single_cstring(
                None,
                arg0,
                c,
                s.as_ref(),
                wl.clone(),
                Some(&mode_pane_owner),
            ));
        }
    } else if count == 1 as u_int {
        prefix = Some(format_single_cstring(
            None,
            arg0,
            c,
            s.as_ref(),
            wl.clone(),
            Some(&mode_pane_owner),
        ));
    }
    ocx = (*data).cx;
    ocy = (*data).cy;
    ooy = (*data).oy;
    (*data).selflag = SEL_CHAR;
    window_copy_cursor_start_of_line(wme.clone());
    window_copy_start_selection(wme.clone());
    while np > 1 as u_int {
        window_copy_cursor_down(wme.clone(), 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    window_copy_cursor_end_of_line(wme.clone());
    if !s.is_none() {
        if pipe != 0 {
            window_copy_copy_pipe(
                wme.clone(),
                s.as_ref(),
                prefix.as_deref(),
                command.as_deref(),
                set_paste,
                set_clip,
            );
        } else {
            window_copy_copy_selection(wme.clone(), prefix.as_deref(), set_paste, set_clip);
        }
        if cancel != 0 {
            return WINDOW_COPY_CMD_CANCEL;
        }
    }
    window_copy_clear_selection(wme.clone());
    (*data).cx = ocx;
    (*data).cy = ocy;
    (*data).oy = ooy;
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_copy_line(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    window_copy_do_copy_line(cs, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_line_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_line(cs, 0 as ::core::ffi::c_int, 1 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_pipe_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_line(cs, 1 as ::core::ffi::c_int, 0 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_pipe_line_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_do_copy_line(cs, 1 as ::core::ffi::c_int, 1 as ::core::ffi::c_int)
}
unsafe fn window_copy_cmd_copy_selection_no_clear(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let c = (*cs).c;
    let mut s: Option<SessionRef> = (*cs).session_handle();
    let mut wl: refbox::Weak<winlink> = (*cs).winlink_handle();
    let mut prefix: Option<CString> = None;
    let mut arg0: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut set_paste: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'P' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let mut set_clip: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'C' as i32 as u_char) == 0) as ::core::ffi::c_int;
    if !arg0.is_null() {
        prefix = Some(format_single_cstring(
            None,
            arg0,
            c,
            s.as_ref(),
            wl.clone(),
            Some(&mode_pane_owner),
        ));
    }
    if !s.is_none() {
        window_copy_copy_selection(wme.clone(), prefix.as_deref(), set_paste, set_clip);
    }
    WINDOW_COPY_CMD_NOTHING
}
unsafe fn window_copy_cmd_copy_selection(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cmd_copy_selection_no_clear(cs);
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_copy_selection_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cmd_copy_selection_no_clear(cs);
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_CANCEL
}
unsafe fn window_copy_cmd_cursor_down(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_down(wme.clone(), 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_cursor_down_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut cy: u_int = 0;
    cy = (*data).cy;
    while np != 0 as u_int {
        window_copy_cursor_down(wme.clone(), 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    if cy == (*data).cy && (*data).oy == 0 as u_int {
        return WINDOW_COPY_CMD_CANCEL;
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_cursor_left(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_left(wme.clone());
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_cursor_right(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_right(
            wme.clone(),
            (!(*data).screen.sel.is_none() && (*data).rectflag != 0) as ::core::ffi::c_int,
        );
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_to(
    mut cs: *mut window_copy_cmd_state,
    mut to: u_int,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut oy: u_int = 0;
    let mut delta: u_int = 0;
    let mut scroll_up: ::core::ffi::c_int = 0;
    scroll_up = (*data).cy.wrapping_sub(to) as ::core::ffi::c_int;
    delta = abs(scroll_up) as u_int;
    oy = (*data).backing().grid().hsize.wrapping_sub((*data).oy);
    if scroll_up > 0 as ::core::ffi::c_int && (*data).oy >= delta {
        window_copy_scroll_up(wme.clone(), delta);
        (*data).cy = (*data).cy.wrapping_sub(delta);
    } else if scroll_up < 0 as ::core::ffi::c_int && oy >= delta {
        window_copy_scroll_down(wme.clone(), delta);
        (*data).cy = (*data).cy.wrapping_add(delta);
    }
    window_copy_update_selection_view(wme.clone(), 0 as ::core::ffi::c_int);
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_scroll_bottom(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    let mut bottom: u_int = 0;
    bottom = (*data).screen.grid().sy.wrapping_sub(1 as u_int);
    window_copy_cmd_scroll_to(cs, bottom)
}
unsafe fn window_copy_cmd_scroll_middle(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    let mut mid_value: u_int = 0;
    mid_value = (*data)
        .screen
        .grid()
        .sy
        .wrapping_sub(1 as u_int)
        .wrapping_div(2 as u_int);
    window_copy_cmd_scroll_to(cs, mid_value)
}
unsafe fn window_copy_cmd_scroll_to_mouse(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let c = (*cs).c;
    let mut scroll_exit: ::core::ffi::c_int = args_has((*cs).parsed_args(), 'e' as i32 as u_char);
    let client = c.expect("live client");
    let tty_oy = client.terminal_view().oy;
    let slider_position = client.mouse_slider_position();
    let mouse = (*cs).m.expect("scroll-to-mouse requires a mouse event");
    window_copy_scroll(
        &mode_pane_owner,
        slider_position,
        mouse.y,
        tty_oy,
        scroll_exit,
    );
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_top(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    window_copy_cmd_scroll_to(cs, 0 as u_int)
}
unsafe fn window_copy_cmd_cursor_up(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_up(wme.clone(), 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_centre_vertical(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    window_copy_update_cursor(
        wme.clone(),
        (*data).cx,
        mode_pane_owner.geometry().1.wrapping_div(2 as u_int),
    );
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_centre_horizontal(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    window_copy_update_cursor(
        wme.clone(),
        mode_pane_owner.geometry().0.wrapping_div(2 as u_int),
        (*data).cy,
    );
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_end_of_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cursor_end_of_line(wme.clone());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_halfpage_down(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        if window_copy_pagedown1(wme.clone(), 1 as ::core::ffi::c_int, (*data).scroll_exit) != 0 {
            return WINDOW_COPY_CMD_CANCEL;
        }
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_halfpage_down_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        if window_copy_pagedown1(
            wme.clone(),
            1 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
        ) != 0
        {
            return WINDOW_COPY_CMD_CANCEL;
        }
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_halfpage_up(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_pageup1(wme.clone(), 1 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_toggle_position(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).hide_position = ((*data).hide_position == 0) as ::core::ffi::c_int;
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_history_bottom(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let s = (*data).backing();
    let mut oy: u_int = 0;
    let mut old_oy: u_int = (*data).oy;
    oy = (*s)
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_RIGHT_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
        && oy == (*data).endsely
    {
        window_copy_other_end(wme.clone());
    }
    (*data).cy = (*data).screen.grid().sy.wrapping_sub(1 as u_int);
    (*data).cx = window_copy_cursor_limit(
        wme.clone(),
        (*s).grid().hsize.wrapping_add((*data).cy),
        0 as ::core::ffi::c_int,
    );
    (*data).oy = 0 as u_int;
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if (*data).oy != old_oy {
        mode_pane_owner.show_scrollbar();
    }
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_history_top(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut oy: u_int = 0;
    let mut old_oy: u_int = (*data).oy;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_LEFT_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
        && oy == (*data).sely
    {
        window_copy_other_end(wme.clone());
    }
    (*data).cy = 0 as u_int;
    (*data).cx = 0 as u_int;
    (*data).oy = (*data).backing().grid().hsize;
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if (*data).oy != old_oy {
        mode_pane_owner.show_scrollbar();
    }
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_jump_again(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    match (*data).jumptype {
        3 => {
            while np != 0 as u_int {
                window_copy_cursor_jump(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        4 => {
            while np != 0 as u_int {
                window_copy_cursor_jump_back(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        5 => {
            while np != 0 as u_int {
                window_copy_cursor_jump_to(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        6 => {
            while np != 0 as u_int {
                window_copy_cursor_jump_to_back(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        _ => {}
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_jump_reverse(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    match (*data).jumptype {
        3 => {
            while np != 0 as u_int {
                window_copy_cursor_jump_back(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        4 => {
            while np != 0 as u_int {
                window_copy_cursor_jump(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        5 => {
            while np != 0 as u_int {
                window_copy_cursor_jump_to_back(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        6 => {
            while np != 0 as u_int {
                window_copy_cursor_jump_to(wme.clone());
                np = np.wrapping_sub(1);
            }
        }
        _ => {}
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_middle_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).cx = 0 as u_int;
    (*data).cy = (*data)
        .screen
        .grid()
        .sy
        .wrapping_sub(1 as u_int)
        .wrapping_div(2 as u_int);
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_previous_matching_bracket(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let s = (*data).backing();
    let mut open: [::core::ffi::c_char; 4] =
        ::core::mem::transmute::<[u8; 4], [::core::ffi::c_char; 4]>(*b"{[(\0");
    let mut close: [::core::ffi::c_char; 4] =
        ::core::mem::transmute::<[u8; 4], [::core::ffi::c_char; 4]>(*b"}])\0");
    let mut tried: ::core::ffi::c_char = 0;
    let mut found: ::core::ffi::c_char = 0;
    let mut start: ::core::ffi::c_char = 0;
    let mut cp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut xx: u_int = 0;
    let mut n: u_int = 0;
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
    let mut failed: ::core::ffi::c_int = 0;
    while np != 0 as u_int {
        px = (*data).cx;
        py = (*s)
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        xx = window_copy_find_length(wme.clone(), py);
        if xx == 0 as u_int {
            break;
        }
        tried = 0 as ::core::ffi::c_char;
        loop {
            grid_get_cell((*s).grid(), px, py, &mut gc);
            if gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int
                || gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0
            {
                cp = ::core::ptr::null_mut::<::core::ffi::c_char>();
            } else {
                found = *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_char;
                cp = strchr(
                    &raw mut close as *mut ::core::ffi::c_char,
                    found as ::core::ffi::c_int,
                );
            }
            if cp.is_null() {
                if !((*data).modekeys == MODEKEY_EMACS) {
                    break;
                }
                if tried == 0 && px > 0 as u_int {
                    px = px.wrapping_sub(1);
                    tried = 1 as ::core::ffi::c_char;
                } else {
                    window_copy_cursor_previous_word(wme.clone(), c"}])", 1 as ::core::ffi::c_int);
                    break;
                }
            } else {
                start = open[cp.offset_from(&raw mut close as *mut ::core::ffi::c_char)
                    as ::core::ffi::c_long as usize];
                n = 1 as u_int;
                failed = 0 as ::core::ffi::c_int;
                loop {
                    if px == 0 as u_int {
                        if py == 0 as u_int {
                            failed = 1 as ::core::ffi::c_int;
                            break;
                        } else {
                            loop {
                                py = py.wrapping_sub(1);
                                xx = window_copy_find_length(wme.clone(), py);
                                if !(xx == 0 as u_int && py > 0 as u_int) {
                                    break;
                                }
                            }
                            if xx == 0 as u_int && py == 0 as u_int {
                                failed = 1 as ::core::ffi::c_int;
                                break;
                            } else {
                                px = xx.wrapping_sub(1 as u_int);
                            }
                        }
                    } else {
                        px = px.wrapping_sub(1);
                    }
                    grid_get_cell((*s).grid(), px, py, &mut gc);
                    if gc.data.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                        && !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0
                    {
                        if *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_int
                            == found as ::core::ffi::c_int
                        {
                            n = n.wrapping_add(1);
                        } else if *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_int
                            == start as ::core::ffi::c_int
                        {
                            n = n.wrapping_sub(1);
                        }
                    }
                    if !(n != 0 as u_int) {
                        break;
                    }
                }
                if failed == 0 {
                    window_copy_scroll_to(wme.clone(), px, py, 0 as ::core::ffi::c_int);
                }
                break;
            }
        }
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_matching_bracket(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let s = (*data).backing();
    let mut open: [::core::ffi::c_char; 4] =
        ::core::mem::transmute::<[u8; 4], [::core::ffi::c_char; 4]>(*b"{[(\0");
    let mut close: [::core::ffi::c_char; 4] =
        ::core::mem::transmute::<[u8; 4], [::core::ffi::c_char; 4]>(*b"}])\0");
    let mut tried: ::core::ffi::c_char = 0;
    let mut found: ::core::ffi::c_char = 0;
    let mut end: ::core::ffi::c_char = 0;
    let mut cp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut n: u_int = 0;
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
    let mut failed: ::core::ffi::c_int = 0;
    's_22: while np != 0 as u_int {
        px = (*data).cx;
        py = (*s)
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        xx = window_copy_find_length(wme.clone(), py);
        yy = (*s)
            .grid()
            .hsize
            .wrapping_add((*s).grid().sy)
            .wrapping_sub(1 as u_int);
        if xx == 0 as u_int {
            break;
        }
        tried = 0 as ::core::ffi::c_char;
        loop {
            grid_get_cell((*s).grid(), px, py, &mut gc);
            if gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int
                || gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0
            {
                cp = ::core::ptr::null_mut::<::core::ffi::c_char>();
            } else {
                found = *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_char;
                cp = strchr(
                    &raw mut close as *mut ::core::ffi::c_char,
                    found as ::core::ffi::c_int,
                );
                if !cp.is_null() && (*data).modekeys == MODEKEY_VI {
                    sx = (*data).cx;
                    sy = (*s)
                        .grid()
                        .hsize
                        .wrapping_add((*data).cy)
                        .wrapping_sub((*data).oy);
                    window_copy_scroll_to(wme.clone(), px, py, 0 as ::core::ffi::c_int);
                    window_copy_cmd_previous_matching_bracket(cs);
                    px = (*data).cx;
                    py = (*s)
                        .grid()
                        .hsize
                        .wrapping_add((*data).cy)
                        .wrapping_sub((*data).oy);
                    grid_get_cell((*s).grid(), px, py, &mut gc);
                    if gc.data.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                        && !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0
                        && !strchr(
                            &raw mut close as *mut ::core::ffi::c_char,
                            *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_int,
                        )
                        .is_null()
                    {
                        window_copy_scroll_to(wme.clone(), sx, sy, 0 as ::core::ffi::c_int);
                    }
                    break 's_22;
                } else {
                    cp = strchr(
                        &raw mut open as *mut ::core::ffi::c_char,
                        found as ::core::ffi::c_int,
                    );
                }
            }
            if cp.is_null() {
                if (*data).modekeys == MODEKEY_EMACS {
                    if tried == 0 && px <= xx {
                        px = px.wrapping_add(1);
                        tried = 1 as ::core::ffi::c_char;
                    } else {
                        window_copy_cursor_next_word_end(
                            wme.clone(),
                            c"{[(",
                            0 as ::core::ffi::c_int,
                        );
                        break;
                    }
                } else if px > xx {
                    if py == yy {
                        break;
                    }
                    let gl = grid_get_line((*s).grid(), py);
                    if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
                        break;
                    }
                    if gl.cellsize as u_int > (*s).grid().sx {
                        break;
                    }
                    px = 0 as u_int;
                    py = py.wrapping_add(1);
                    xx = window_copy_find_length(wme.clone(), py);
                } else {
                    px = px.wrapping_add(1);
                }
            } else {
                end = close[cp.offset_from(&raw mut open as *mut ::core::ffi::c_char)
                    as ::core::ffi::c_long as usize];
                n = 1 as u_int;
                failed = 0 as ::core::ffi::c_int;
                loop {
                    if px > xx {
                        if py == yy {
                            failed = 1 as ::core::ffi::c_int;
                            break;
                        } else {
                            px = 0 as u_int;
                            py = py.wrapping_add(1);
                            xx = window_copy_find_length(wme.clone(), py);
                        }
                    } else {
                        px = px.wrapping_add(1);
                    }
                    grid_get_cell((*s).grid(), px, py, &mut gc);
                    if gc.data.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                        && !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0
                    {
                        if *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_int
                            == found as ::core::ffi::c_int
                        {
                            n = n.wrapping_add(1);
                        } else if *(&raw mut gc.data.data as *mut u_char) as ::core::ffi::c_int
                            == end as ::core::ffi::c_int
                        {
                            n = n.wrapping_sub(1);
                        }
                    }
                    if !(n != 0 as u_int) {
                        break;
                    }
                }
                if failed == 0 {
                    window_copy_scroll_to(wme.clone(), px, py, 0 as ::core::ffi::c_int);
                }
                break;
            }
        }
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_paragraph(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_next_paragraph(wme.clone());
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_space(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_next_word(wme.clone(), c"");
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_space_end(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_next_word_end(wme.clone(), c"", 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_word(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    let separators = options_get_string(
        &(*cs)
            .session_handle()
            .expect("live session")
            .borrow_options_mut(),
        c"word-separators",
    );
    while np != 0 as u_int {
        window_copy_cursor_next_word(wme.clone(), &separators);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_word_end(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    let separators = options_get_string(
        &(*cs)
            .session_handle()
            .expect("live session")
            .borrow_options_mut(),
        c"word-separators",
    );
    while np != 0 as u_int {
        window_copy_cursor_next_word_end(wme.clone(), &separators, 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_other_end(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).selflag = SEL_CHAR;
    if np.wrapping_rem(2 as u_int) != 0 as u_int {
        window_copy_other_end(wme.clone());
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_selection_mode(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();

    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut ex: u_int = 0;
    let mut ey: u_int = 0;
    let mut fx: u_int = 0;
    let mut fy: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if s.is_null()
        || strcasecmp(s, c"char".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(s, c"c".as_ptr()) == 0 as ::core::ffi::c_int
    {
        (*data).selflag = SEL_CHAR;
    } else if strcasecmp(s, c"word".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(s, c"w".as_ptr()) == 0 as ::core::ffi::c_int
    {
        (*data).separators = window_copy_snapshot_separators(&OptionsScope::Session(
            std::rc::Rc::downgrade(&(*cs).session_handle().expect("live session")),
        ));
        (*data).selflag = SEL_WORD;
    } else if strcasecmp(s, c"line".as_ptr()) == 0 as ::core::ffi::c_int
        || strcasecmp(s, c"l".as_ptr()) == 0 as ::core::ffi::c_int
    {
        (*data).selflag = SEL_LINE;
        if (*data).screen.sel.is_none() {
            return WINDOW_COPY_CMD_MOVE;
        }
        if (*data).cursordrag as ::core::ffi::c_uint
            == CURSORDRAG_SEL as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            fx = (*data).endselx;
            fy = (*data).endsely;
        } else {
            fx = (*data).selx;
            fy = (*data).sely;
        }
        sx = (*data).selx;
        sy = (*data).sely;
        ex = (*data).endselx;
        ey = (*data).endsely;
        if ey < sy || ey == sy && ex < sx {
            x = sx;
            sx = ex;
            ex = x;
            y = sy;
            sy = ey;
            ey = y;
        }
        let mut gr = grid_reader_start((*data).backing().grid(), sx, sy);
        grid_reader_cursor_start_of_line(&mut gr, 1 as ::core::ffi::c_int);
        (sx, sy) = grid_reader_get_cursor(&gr);
        let mut gr = grid_reader_start((*data).backing().grid(), ex, ey);
        grid_reader_cursor_end_of_line(&mut gr, 1 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
        (ex, ey) = grid_reader_get_cursor(&gr);
        (*data).rectflag = 0 as ::core::ffi::c_int;
        (*data).selx = sx;
        (*data).selrx = (*data).selx;
        (*data).sely = sy;
        (*data).selry = (*data).sely;
        (*data).endselx = ex;
        (*data).endselrx = (*data).endselx;
        (*data).endsely = ey;
        (*data).endselry = (*data).endsely;
        x = (*data).cx;
        y = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        (*data).dx = fx;
        (*data).dy = fy;
        if (*data).cursordrag as ::core::ffi::c_uint
            != CURSORDRAG_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            && (y < fy || y == fy && x < fx)
        {
            (*data).lineflag = LINE_SEL_RIGHT_LEFT;
            (*data).cursordrag = CURSORDRAG_SEL;
            window_copy_scroll_to(wme.clone(), sx, sy, 1 as ::core::ffi::c_int);
        } else {
            (*data).lineflag = LINE_SEL_LEFT_RIGHT;
            if (*data).cursordrag as ::core::ffi::c_uint
                != CURSORDRAG_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                (*data).cursordrag = CURSORDRAG_ENDSEL;
                x = window_copy_cursor_limit(wme.clone(), ey, 0 as ::core::ffi::c_int);
                window_copy_scroll_to(wme.clone(), x, ey, 1 as ::core::ffi::c_int);
            }
        }
        if (*data).cursordrag as ::core::ffi::c_uint
            == CURSORDRAG_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            window_copy_set_selection(
                wme.clone(),
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
        }
        return WINDOW_COPY_CMD_REDRAW;
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_page_down(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        if window_copy_pagedown1(wme.clone(), 0 as ::core::ffi::c_int, (*data).scroll_exit) != 0 {
            return WINDOW_COPY_CMD_CANCEL;
        }
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_page_down_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        if window_copy_pagedown1(
            wme.clone(),
            0 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
        ) != 0
        {
            return WINDOW_COPY_CMD_CANCEL;
        }
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_page_up(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_pageup1(wme.clone(), 0 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_previous_paragraph(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_previous_paragraph(wme.clone());
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_previous_space(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    while np != 0 as u_int {
        window_copy_cursor_previous_word(wme.clone(), c"", 1 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_previous_word(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut np: u_int = wme.get_unchecked().prefix;
    let separators = options_get_string(
        &(*cs)
            .session_handle()
            .expect("live session")
            .borrow_options_mut(),
        c"word-separators",
    );
    while np != 0 as u_int {
        window_copy_cursor_previous_word(wme.clone(), &separators, 1 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_rectangle_on(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).lineflag = LINE_SEL_NONE;
    window_copy_rectangle_set(wme.clone(), 1 as ::core::ffi::c_int);
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_rectangle_off(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).lineflag = LINE_SEL_NONE;
    window_copy_rectangle_set(wme.clone(), 0 as ::core::ffi::c_int);
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_rectangle_toggle(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).lineflag = LINE_SEL_NONE;
    window_copy_rectangle_set(wme.clone(), ((*data).rectflag == 0) as ::core::ffi::c_int);
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_exit_on(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    (*data).scroll_exit = 1 as ::core::ffi::c_int;
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_exit_off(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    (*data).scroll_exit = 0 as ::core::ffi::c_int;
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_exit_toggle(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    (*data).scroll_exit = ((*data).scroll_exit == 0) as ::core::ffi::c_int;
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_down(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut dragging: ::core::ffi::c_int = 0;
    if (*data).oy == 0 as u_int {
        if (*data).scroll_exit != 0 && (*data).screen.sel.is_none() {
            return WINDOW_COPY_CMD_CANCEL;
        }
        return WINDOW_COPY_CMD_NOTHING;
    }
    dragging = (*cs).c.is_some_and(|owner| owner.mouse_drag_active()) as ::core::ffi::c_int;
    if !(*data).screen.sel.is_none() && dragging == 0 {
        (*data).cursordrag = CURSORDRAG_NONE;
        (*data).lineflag = LINE_SEL_NONE;
        window_copy_scroll_up(wme.clone(), np);
        return WINDOW_COPY_CMD_NOTHING;
    }
    while np != 0 as u_int {
        window_copy_cursor_down(wme.clone(), 1 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    if (*data).scroll_exit != 0 && (*data).oy == 0 as u_int && (*data).screen.sel.is_none() {
        return WINDOW_COPY_CMD_CANCEL;
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_down_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut dragging: ::core::ffi::c_int = 0;
    dragging = (*cs).c.is_some_and(|owner| owner.mouse_drag_active()) as ::core::ffi::c_int;
    if !(*data).screen.sel.is_none() && dragging == 0 {
        (*data).cursordrag = CURSORDRAG_NONE;
        (*data).lineflag = LINE_SEL_NONE;
        window_copy_scroll_up(wme.clone(), np);
        return WINDOW_COPY_CMD_NOTHING;
    }
    while np != 0 as u_int {
        window_copy_cursor_down(wme.clone(), 1 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    if (*data).oy == 0 as u_int {
        return WINDOW_COPY_CMD_CANCEL;
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_scroll_up(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let mut dragging: ::core::ffi::c_int = 0;
    if (*data).oy == (*data).backing().grid().hsize {
        return WINDOW_COPY_CMD_NOTHING;
    }
    dragging = (*cs).c.is_some_and(|owner| owner.mouse_drag_active()) as ::core::ffi::c_int;
    if !(*data).screen.sel.is_none() && dragging == 0 {
        (*data).cursordrag = CURSORDRAG_NONE;
        (*data).lineflag = LINE_SEL_NONE;
        window_copy_scroll_down(wme.clone(), np);
        return WINDOW_COPY_CMD_NOTHING;
    }
    while np != 0 as u_int {
        window_copy_cursor_up(wme.clone(), 1 as ::core::ffi::c_int);
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_again(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    if (*data).searchtype == WINDOW_COPY_SEARCHUP as ::core::ffi::c_int {
        while np != 0 as u_int {
            window_copy_search_up(wme.clone(), (*data).searchregex);
            np = np.wrapping_sub(1);
        }
    } else if (*data).searchtype == WINDOW_COPY_SEARCHDOWN as ::core::ffi::c_int {
        while np != 0 as u_int {
            window_copy_search_down(wme.clone(), (*data).searchregex);
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_reverse(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    if (*data).searchtype == WINDOW_COPY_SEARCHUP as ::core::ffi::c_int {
        while np != 0 as u_int {
            window_copy_search_down(wme.clone(), (*data).searchregex);
            np = np.wrapping_sub(1);
        }
    } else if (*data).searchtype == WINDOW_COPY_SEARCHDOWN as ::core::ffi::c_int {
        while np != 0 as u_int {
            window_copy_search_up(wme.clone(), (*data).searchregex);
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_select_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    (*data).lineflag = LINE_SEL_LEFT_RIGHT;
    (*data).rectflag = 0 as ::core::ffi::c_int;
    (*data).selflag = SEL_LINE;
    (*data).dx = (*data).cx;
    (*data).dy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    window_copy_cursor_start_of_line(wme.clone());
    (*data).selrx = (*data).cx;
    (*data).selry = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).endselry = (*data).selry;
    window_copy_start_selection(wme.clone());
    window_copy_cursor_end_of_line(wme.clone());
    (*data).endselry = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).endselrx = window_copy_find_length(wme.clone(), (*data).endselry);
    while np > 1 as u_int {
        window_copy_cursor_down(wme.clone(), 0 as ::core::ffi::c_int);
        window_copy_cursor_end_of_line(wme.clone());
        np = np.wrapping_sub(1);
    }
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_select_word(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();

    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut nextx: u_int = 0;
    let mut nexty: u_int = 0;
    (*data).lineflag = LINE_SEL_LEFT_RIGHT;
    (*data).rectflag = 0 as ::core::ffi::c_int;
    (*data).selflag = SEL_WORD;
    (*data).dx = (*data).cx;
    (*data).dy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).separators = window_copy_snapshot_separators(&OptionsScope::Session(
        std::rc::Rc::downgrade(&(*cs).session_handle().expect("live session")),
    ));
    window_copy_cursor_previous_word(
        wme.clone(),
        window_copy_separators(&*data).expect("word separators"),
        0 as ::core::ffi::c_int,
    );
    px = (*data).cx;
    py = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).selrx = px;
    (*data).selry = py;
    window_copy_start_selection(wme.clone());
    nextx = px.wrapping_add(1 as u_int);
    nexty = py;
    if grid_get_line((*data).backing().grid(), nexty).flags as ::core::ffi::c_int
        & GRID_LINE_WRAPPED
        != 0
        && nextx > (*data).backing().grid().sx.wrapping_sub(1 as u_int)
    {
        nextx = 0 as u_int;
        nexty = nexty.wrapping_add(1);
    }
    if px >= window_copy_find_length(wme.clone(), py)
        || window_copy_in_set(wme.clone(), nextx, nexty, WHITESPACE) == 0
    {
        window_copy_cursor_next_word_end(
            wme.clone(),
            window_copy_separators(&*data).expect("word separators"),
            1 as ::core::ffi::c_int,
        );
    } else {
        window_copy_update_cursor(wme.clone(), px, (*data).cy);
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
        ) != 0
        {
            window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
        }
    }
    (*data).endselrx = (*data).cx;
    (*data).endselry = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    if (*data).dy > (*data).endselry {
        (*data).dy = (*data).endselry;
        (*data).dx = (*data).endselrx;
    } else if (*data).dx > (*data).endselrx {
        (*data).dx = (*data).endselrx;
    }
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_set_mark(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    (*data).mx = (*data).cx;
    (*data).my = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).showmark = 1 as ::core::ffi::c_int;
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_start_of_line(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cursor_start_of_line(wme.clone());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_top_line(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).cx = 0 as u_int;
    (*data).cy = 0 as u_int;
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_copy_pipe_no_clear(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let c = (*cs).c;
    let mut s: Option<SessionRef> = (*cs).session_handle();
    let mut wl: refbox::Weak<winlink> = (*cs).winlink_handle();
    let mut command: Option<CString> = None;
    let mut prefix: Option<CString> = None;
    let mut arg0: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut arg1: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 1 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    let mut set_paste: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'P' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let mut set_clip: ::core::ffi::c_int =
        (args_has((*cs).parsed_args(), 'C' as i32 as u_char) == 0) as ::core::ffi::c_int;
    if !arg1.is_null() {
        prefix = Some(format_single_cstring(
            None,
            arg1,
            c,
            s.as_ref(),
            wl.clone(),
            Some(&mode_pane_owner),
        ));
    }
    if !s.is_none() && !arg0.is_null() && *arg0 as ::core::ffi::c_int != '\0' as i32 {
        command = Some(format_single_cstring(
            None,
            arg0,
            c,
            s.as_ref(),
            wl.clone(),
            Some(&mode_pane_owner),
        ));
    }
    window_copy_copy_pipe(
        wme.clone(),
        s.as_ref(),
        prefix.as_deref(),
        command.as_deref(),
        set_paste,
        set_clip,
    );
    WINDOW_COPY_CMD_NOTHING
}
unsafe fn window_copy_cmd_copy_pipe(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cmd_copy_pipe_no_clear(cs);
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_copy_pipe_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cmd_copy_pipe_no_clear(cs);
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_CANCEL
}
unsafe fn window_copy_cmd_pipe_no_clear(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let c = (*cs).c;
    let mut s: Option<SessionRef> = (*cs).session_handle();
    let mut wl: refbox::Weak<winlink> = (*cs).winlink_handle();
    let mut command: Option<CString> = None;
    let mut arg0: *const ::core::ffi::c_char = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .map_or(std::ptr::null(), |value| value.as_ptr());
    if !s.is_none() && !arg0.is_null() && *arg0 as ::core::ffi::c_int != '\0' as i32 {
        command = Some(format_single_cstring(
            None,
            arg0,
            c,
            s.as_ref(),
            wl.clone(),
            Some(&mode_pane_owner),
        ));
    }
    window_copy_pipe(wme.clone(), s.as_ref(), command.as_deref());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_pipe(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cmd_pipe_no_clear(cs);
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_pipe_and_cancel(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cmd_pipe_no_clear(cs);
    window_copy_clear_selection(wme.clone());
    WINDOW_COPY_CMD_CANCEL
}
unsafe fn window_copy_cmd_goto_line(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let arg0 =
        args_string(&mut *((*cs).parsed_args()), 0 as u_int).expect("goto-line takes an argument");
    if !arg0.is_empty() {
        window_copy_goto_line(wme.clone(), arg0);
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_jump_backward(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let arg0 = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .expect("jump-backward takes an argument");
    if !arg0.is_empty() {
        (*data).jumptype = WINDOW_COPY_JUMPBACKWARD as ::core::ffi::c_int;
        (*data).jumpchar = utf8_fromcstr_vec(arg0);
        while np != 0 as u_int {
            window_copy_cursor_jump_back(wme.clone());
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_jump_forward(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let arg0 = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .expect("jump-forward takes an argument");
    if !arg0.is_empty() {
        (*data).jumptype = WINDOW_COPY_JUMPFORWARD as ::core::ffi::c_int;
        (*data).jumpchar = utf8_fromcstr_vec(arg0);
        while np != 0 as u_int {
            window_copy_cursor_jump(wme.clone());
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_jump_to_backward(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let arg0 = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .expect("jump-to-backward takes an argument");
    if !arg0.is_empty() {
        (*data).jumptype = WINDOW_COPY_JUMPTOBACKWARD as ::core::ffi::c_int;
        (*data).jumpchar = utf8_fromcstr_vec(arg0);
        while np != 0 as u_int {
            window_copy_cursor_jump_to_back(wme.clone());
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_jump_to_forward(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    let arg0 = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .expect("jump-to-forward takes an argument");
    if !arg0.is_empty() {
        (*data).jumptype = WINDOW_COPY_JUMPTOFORWARD as ::core::ffi::c_int;
        (*data).jumpchar = utf8_fromcstr_vec(arg0);
        while np != 0 as u_int {
            window_copy_cursor_jump_to(wme.clone());
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_jump_to_mark(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_jump_to_mark(wme.clone());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_next_prompt(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cursor_prompt(
        wme.clone(),
        1 as ::core::ffi::c_int,
        args_has((*cs).parsed_args(), 'o' as i32 as u_char),
    );
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_previous_prompt(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    window_copy_cursor_prompt(
        wme.clone(),
        0 as ::core::ffi::c_int,
        args_has((*cs).parsed_args(), 'o' as i32 as u_char),
    );
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_backward(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    if window_copy_expand_search_string(cs) == 0 {
        return WINDOW_COPY_CMD_MOVE;
    }
    if (*data).searchstr.is_some() {
        (*data).searchtype = WINDOW_COPY_SEARCHUP as ::core::ffi::c_int;
        (*data).searchregex = 1 as ::core::ffi::c_int;
        (*data).timeout = 0 as ::core::ffi::c_int;
        while np != 0 as u_int {
            window_copy_search_up(wme.clone(), 1 as ::core::ffi::c_int);
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_backward_text(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    if window_copy_expand_search_string(cs) == 0 {
        return WINDOW_COPY_CMD_MOVE;
    }
    if (*data).searchstr.is_some() {
        (*data).searchtype = WINDOW_COPY_SEARCHUP as ::core::ffi::c_int;
        (*data).searchregex = 0 as ::core::ffi::c_int;
        (*data).timeout = 0 as ::core::ffi::c_int;
        while np != 0 as u_int {
            window_copy_search_up(wme.clone(), 0 as ::core::ffi::c_int);
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_forward(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    if window_copy_expand_search_string(cs) == 0 {
        return WINDOW_COPY_CMD_MOVE;
    }
    if (*data).searchstr.is_some() {
        (*data).searchtype = WINDOW_COPY_SEARCHDOWN as ::core::ffi::c_int;
        (*data).searchregex = 1 as ::core::ffi::c_int;
        (*data).timeout = 0 as ::core::ffi::c_int;
        while np != 0 as u_int {
            window_copy_search_down(wme.clone(), 1 as ::core::ffi::c_int);
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_forward_text(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut np: u_int = wme.get_unchecked().prefix;
    if window_copy_expand_search_string(cs) == 0 {
        return WINDOW_COPY_CMD_MOVE;
    }
    if (*data).searchstr.is_some() {
        (*data).searchtype = WINDOW_COPY_SEARCHDOWN as ::core::ffi::c_int;
        (*data).searchregex = 0 as ::core::ffi::c_int;
        (*data).timeout = 0 as ::core::ffi::c_int;
        while np != 0 as u_int {
            window_copy_search_down(wme.clone(), 0 as ::core::ffi::c_int);
            np = np.wrapping_sub(1);
        }
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_search_backward_incremental(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let arg = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .expect("search-backward-incremental takes an argument");
    let mut action: window_copy_cmd_action = WINDOW_COPY_CMD_MOVE;
    (*data).timeout = 0 as ::core::ffi::c_int;
    log_debug(format_args!(
        "{}: {}",
        "window_copy_cmd_search_backward_incremental",
        log_cstr(arg)
    ));
    let (prefix, arg0) = match arg.to_bytes().first() {
        Some(&prefix) => (prefix, cstr_suffix(arg, 1)),
        None => (0, arg),
    };
    if (*data).searchx == -(1 as ::core::ffi::c_int)
        || (*data).searchy == -(1 as ::core::ffi::c_int)
    {
        (*data).searchx = (*data).cx as ::core::ffi::c_int;
        (*data).searchy = (*data).cy as ::core::ffi::c_int;
        (*data).searcho = (*data).oy as ::core::ffi::c_int;
    } else if window_copy_searchstr(&*data).is_some_and(|ss| ss != arg0) {
        (*data).cx = (*data).searchx as u_int;
        (*data).cy = (*data).searchy as u_int;
        (*data).oy = (*data).searcho as u_int;
        (*data).cx = window_copy_cursor_limit(
            wme.clone(),
            (*data)
                .backing()
                .grid()
                .hsize
                .wrapping_add((*data).cy)
                .wrapping_sub((*data).oy),
            0 as ::core::ffi::c_int,
        );
        action = WINDOW_COPY_CMD_REDRAW;
    }
    if arg0.is_empty() {
        window_copy_clear_marks(wme.clone());
        return WINDOW_COPY_CMD_REDRAW;
    }
    match prefix {
        b'=' | b'-' => {
            (*data).searchtype = WINDOW_COPY_SEARCHUP as ::core::ffi::c_int;
            (*data).searchregex = 0 as ::core::ffi::c_int;
            (*data).searchstr = Some(arg0.to_owned());
            if window_copy_search_up(wme.clone(), 0 as ::core::ffi::c_int) == 0 {
                window_copy_clear_marks(wme.clone());
                return WINDOW_COPY_CMD_REDRAW;
            }
        }
        b'+' => {
            (*data).searchtype = WINDOW_COPY_SEARCHDOWN as ::core::ffi::c_int;
            (*data).searchregex = 0 as ::core::ffi::c_int;
            (*data).searchstr = Some(arg0.to_owned());
            if window_copy_search_down(wme.clone(), 0 as ::core::ffi::c_int) == 0 {
                window_copy_clear_marks(wme.clone());
                return WINDOW_COPY_CMD_REDRAW;
            }
        }
        _ => {}
    }
    action
}
unsafe fn window_copy_cmd_search_forward_incremental(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let arg = args_string(&mut *((*cs).parsed_args()), 0 as u_int)
        .expect("search-forward-incremental takes an argument");
    let mut action: window_copy_cmd_action = WINDOW_COPY_CMD_MOVE;
    (*data).timeout = 0 as ::core::ffi::c_int;
    log_debug(format_args!(
        "{}: {}",
        "window_copy_cmd_search_forward_incremental",
        log_cstr(arg)
    ));
    let (prefix, arg0) = match arg.to_bytes().first() {
        Some(&prefix) => (prefix, cstr_suffix(arg, 1)),
        None => (0, arg),
    };
    if (*data).searchx == -(1 as ::core::ffi::c_int)
        || (*data).searchy == -(1 as ::core::ffi::c_int)
    {
        (*data).searchx = (*data).cx as ::core::ffi::c_int;
        (*data).searchy = (*data).cy as ::core::ffi::c_int;
        (*data).searcho = (*data).oy as ::core::ffi::c_int;
    } else if window_copy_searchstr(&*data).is_some_and(|ss| ss != arg0) {
        (*data).cx = (*data).searchx as u_int;
        (*data).cy = (*data).searchy as u_int;
        (*data).oy = (*data).searcho as u_int;
        (*data).cx = window_copy_cursor_limit(
            wme.clone(),
            (*data)
                .backing()
                .grid()
                .hsize
                .wrapping_add((*data).cy)
                .wrapping_sub((*data).oy),
            0 as ::core::ffi::c_int,
        );
        action = WINDOW_COPY_CMD_REDRAW;
    }
    if arg0.is_empty() {
        window_copy_clear_marks(wme.clone());
        return WINDOW_COPY_CMD_REDRAW;
    }
    match prefix {
        b'=' | b'+' => {
            (*data).searchtype = WINDOW_COPY_SEARCHDOWN as ::core::ffi::c_int;
            (*data).searchregex = 0 as ::core::ffi::c_int;
            (*data).searchstr = Some(arg0.to_owned());
            if window_copy_search_down(wme.clone(), 0 as ::core::ffi::c_int) == 0 {
                window_copy_clear_marks(wme.clone());
                return WINDOW_COPY_CMD_REDRAW;
            }
        }
        b'-' => {
            (*data).searchtype = WINDOW_COPY_SEARCHUP as ::core::ffi::c_int;
            (*data).searchregex = 0 as ::core::ffi::c_int;
            (*data).searchstr = Some(arg0.to_owned());
            if window_copy_search_up(wme.clone(), 0 as ::core::ffi::c_int) == 0 {
                window_copy_clear_marks(wme.clone());
                return WINDOW_COPY_CMD_REDRAW;
            }
        }
        _ => {}
    }
    action
}
unsafe fn window_copy_do_refresh(
    mut wme: refbox::Weak<window_mode_entry>,
    mut follow: ::core::ffi::c_int,
) {
    let Some(source_owner) = wme.get_unchecked().swp.upgrade() else {
        return;
    };
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut oy_from_top: u_int = 0;
    if (*data).oy > (*data).backing().grid().hsize {
        (*data).oy = (*data).backing().grid().hsize;
    }
    oy_from_top = (*data).backing().grid().hsize.wrapping_sub((*data).oy);
    if window_copy_sync_backing(wme.clone()) == 0 {
        (*data).clear_backing();
        (*data).backing = Some(source_owner.clone_history(
            &(*data).screen,
            None,
            !wme.get_unchecked().swp.ptr_eq(&wme.get_unchecked().wp),
        ));
    }
    if follow != 0 {
        (*data).cy = (*data).screen.grid().sy.wrapping_sub(1 as u_int);
        (*data).cx = window_copy_cursor_limit(
            wme.clone(),
            (*data).backing().grid().hsize.wrapping_add((*data).cy),
            0 as ::core::ffi::c_int,
        );
        (*data).oy = 0 as u_int;
    } else if oy_from_top <= (*data).backing().grid().hsize {
        (*data).oy = (*data).backing().grid().hsize.wrapping_sub(oy_from_top);
    } else {
        (*data).cy = 0 as u_int;
        (*data).oy = (*data).backing().grid().hsize;
    }
    window_copy_sync_snapshot(data, source_owner.history_scroll());
    window_copy_size_changed(wme.clone());
}
unsafe fn window_copy_refresh_arm(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let tv = Duration::from_micros(WINDOW_COPY_REFRESH_INTERVAL as u64);
    if (*data).refresh_active != 0 {
        (*data).refresh_timer = Some(window_copy_timer(
            wme.clone(),
            tv,
            window_copy_refresh_timer,
        ));
    }
}
unsafe fn window_copy_refresh_allowed(
    mut wme: refbox::Weak<window_mode_entry>,
) -> ::core::ffi::c_int {
    let Some(_source_owner) = wme.get_unchecked().swp.upgrade() else {
        return 0;
    };
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    if (*data).viewmode != 0 || !wme.get_unchecked().swp.ptr_eq(&wme.get_unchecked().wp) {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}
unsafe fn window_copy_refresh_timer(wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut follow: ::core::ffi::c_int = 0;
    if mode_pane_owner.mode_entry() != wme || (*data).refresh_active == 0 {
        return;
    }
    if mode_pane_owner.has_unseen_changes()
        && (*data).screen.sel.is_none()
        && (*data).cursordrag as ::core::ffi::c_uint
            == CURSORDRAG_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        follow = ((*data).oy == 0 as u_int
            && (*data).cy == (*data).screen.grid().sy.wrapping_sub(1 as u_int))
            as ::core::ffi::c_int;
        window_copy_do_refresh(wme.clone(), follow);
        window_copy_redraw_screen(wme.clone());
        mode_pane_owner.request_redraw(false);
        mode_pane_owner.acknowledge_unseen_changes();
    }
    window_copy_refresh_arm(wme.clone());
}
unsafe fn window_copy_refresh_start(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    if window_copy_refresh_allowed(wme.clone()) == 0 || (*data).refresh_active != 0 {
        return;
    }
    (*data).refresh_active = 1 as ::core::ffi::c_int;
    window_copy_refresh_arm(wme.clone());
}
unsafe fn window_copy_refresh_stop(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).refresh_active = 0 as ::core::ffi::c_int;
    drop((*data).refresh_timer.take());
}
unsafe fn window_copy_cmd_refresh_now(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut follow: ::core::ffi::c_int = 0;
    if window_copy_refresh_allowed(wme.clone()) == 0 {
        return WINDOW_COPY_CMD_NOTHING;
    }
    follow = ((*data).oy == 0 as u_int
        && (*data).cy == (*data).screen.grid().sy.wrapping_sub(1 as u_int))
        as ::core::ffi::c_int;
    window_copy_do_refresh(wme.clone(), follow);
    mode_pane_owner.acknowledge_unseen_changes();
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_refresh_on(mut cs: *mut window_copy_cmd_state) -> window_copy_cmd_action {
    window_copy_refresh_start(((*cs).mode_handle()).clone());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_refresh_off(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_refresh_stop(((*cs).mode_handle()).clone());
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_refresh_toggle(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut data: *mut window_copy_mode_data = window_copy_data(((*cs).mode_handle()).clone());
    if (*data).refresh_active != 0 {
        window_copy_refresh_stop(((*cs).mode_handle()).clone());
    } else {
        window_copy_refresh_start(((*cs).mode_handle()).clone());
    }
    WINDOW_COPY_CMD_MOVE
}
unsafe fn window_copy_cmd_recentre_top_bottom(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    let mut wme: refbox::Weak<window_mode_entry> = (*cs).mode_handle();
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut cy: u_int = (*data).cy;
    let mut oy: u_int = (*data).oy;
    let mut sy: u_int = (*data).screen.grid().sy.wrapping_sub(1 as u_int);
    let mut sm: u_int = sy.wrapping_div(2 as u_int);
    let mut backing_row: u_int = 0;
    let mut target: C2RustUnnamed_48 = MIDDLE;
    backing_row = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add(cy)
        .wrapping_sub((*data).oy);
    if (*data).recentre_line != backing_row {
        (*data).recentre_state = RECENTRE_MIDDLE;
        (*data).recentre_line = backing_row;
    }
    match (*data).recentre_state as ::core::ffi::c_uint {
        1 => {
            (*data).recentre_state = RECENTRE_TOP;
            target = MIDDLE;
        }
        0 => {
            (*data).recentre_state = RECENTRE_BOTTOM;
            target = TOP;
        }
        _ => {
            (*data).recentre_state = RECENTRE_MIDDLE;
            target = BOTTOM;
        }
    }
    oy = (*data).oy;
    match target as ::core::ffi::c_uint {
        0 => {
            if cy < sm {
                window_copy_scroll_down(wme.clone(), sm.wrapping_sub(cy));
            } else if cy > sm {
                window_copy_scroll_up(wme.clone(), cy.wrapping_sub(sm));
            }
            if (*data).oy != oy {
                (*data).cy = cy.wrapping_add((*data).oy.wrapping_sub(oy));
            }
        }
        1 => {
            window_copy_scroll_up(wme.clone(), cy);
            (*data).cy = cy.wrapping_sub(oy.wrapping_sub((*data).oy));
        }
        2 => {
            window_copy_scroll_down(wme.clone(), sy.wrapping_sub(cy));
            (*data).cy = cy.wrapping_add((*data).oy.wrapping_sub(oy));
        }
        _ => {}
    }
    window_copy_update_selection_view(wme.clone(), 0 as ::core::ffi::c_int);
    WINDOW_COPY_CMD_REDRAW
}
unsafe fn window_copy_cmd_line_numbers_on(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_set_line_numbers1(
        ((*cs).mode_handle()).clone(),
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_NOTHING
}
unsafe fn window_copy_cmd_line_numbers_off(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_set_line_numbers1(
        ((*cs).mode_handle()).clone(),
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_NOTHING
}
unsafe fn window_copy_cmd_line_numbers_toggle(
    mut cs: *mut window_copy_cmd_state,
) -> window_copy_cmd_action {
    window_copy_set_line_numbers1(
        ((*cs).mode_handle()).clone(),
        (window_copy_line_numbers_active(((*cs).mode_handle()).clone()) == 0) as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    WINDOW_COPY_CMD_NOTHING
}
static mut window_copy_cmd_table: [C2RustUnnamed_46; 99] = {
    [
        C2RustUnnamed_46 {
            command: c"append-selection",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_append_selection),
        },
        C2RustUnnamed_46 {
            command: c"append-selection-and-cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_append_selection_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"back-to-indentation",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_back_to_indentation),
        },
        C2RustUnnamed_46 {
            command: c"begin-selection",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_begin_selection),
        },
        C2RustUnnamed_46 {
            command: c"bottom-line",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_bottom_line),
        },
        C2RustUnnamed_46 {
            command: c"cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_cancel),
        },
        C2RustUnnamed_46 {
            command: c"clear-selection",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_clear_selection),
        },
        C2RustUnnamed_46 {
            command: c"copy-end-of-line",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_end_of_line),
        },
        C2RustUnnamed_46 {
            command: c"copy-end-of-line-and-cancel",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_end_of_line_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe-end-of-line",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_pipe_end_of_line),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe-end-of-line-and-cancel",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_pipe_end_of_line_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"copy-line",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_line),
        },
        C2RustUnnamed_46 {
            command: c"copy-line-and-cancel",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_line_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe-line",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_pipe_line),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe-line-and-cancel",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_pipe_line_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe-no-clear",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_copy_pipe_no_clear),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_pipe),
        },
        C2RustUnnamed_46 {
            command: c"copy-pipe-and-cancel",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 2 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_pipe_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"copy-selection-no-clear",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_copy_selection_no_clear),
        },
        C2RustUnnamed_46 {
            command: c"copy-selection",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_selection),
        },
        C2RustUnnamed_46 {
            command: c"copy-selection-and-cancel",
            args: args_parse {
                template: c"CP",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_copy_selection_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"cursor-down",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_cursor_down),
        },
        C2RustUnnamed_46 {
            command: c"cursor-down-and-cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_cursor_down_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"cursor-left",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_cursor_left),
        },
        C2RustUnnamed_46 {
            command: c"cursor-right",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_cursor_right),
        },
        C2RustUnnamed_46 {
            command: c"cursor-up",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_cursor_up),
        },
        C2RustUnnamed_46 {
            command: c"cursor-centre-vertical",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_centre_vertical),
        },
        C2RustUnnamed_46 {
            command: c"cursor-centre-horizontal",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_centre_horizontal),
        },
        C2RustUnnamed_46 {
            command: c"end-of-line",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_end_of_line),
        },
        C2RustUnnamed_46 {
            command: c"goto-line",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_goto_line),
        },
        C2RustUnnamed_46 {
            command: c"halfpage-down",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_halfpage_down),
        },
        C2RustUnnamed_46 {
            command: c"halfpage-down-and-cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_halfpage_down_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"halfpage-up",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_halfpage_up),
        },
        C2RustUnnamed_46 {
            command: c"history-bottom",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_history_bottom),
        },
        C2RustUnnamed_46 {
            command: c"history-top",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_history_top),
        },
        C2RustUnnamed_46 {
            command: c"jump-again",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_jump_again),
        },
        C2RustUnnamed_46 {
            command: c"jump-backward",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_jump_backward),
        },
        C2RustUnnamed_46 {
            command: c"jump-forward",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_jump_forward),
        },
        C2RustUnnamed_46 {
            command: c"jump-reverse",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_jump_reverse),
        },
        C2RustUnnamed_46 {
            command: c"jump-to-backward",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_jump_to_backward),
        },
        C2RustUnnamed_46 {
            command: c"jump-to-forward",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_jump_to_forward),
        },
        C2RustUnnamed_46 {
            command: c"jump-to-mark",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_jump_to_mark),
        },
        C2RustUnnamed_46 {
            command: c"line-numbers-on",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_line_numbers_on),
        },
        C2RustUnnamed_46 {
            command: c"line-numbers-off",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_line_numbers_off),
        },
        C2RustUnnamed_46 {
            command: c"line-numbers-toggle",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_line_numbers_toggle),
        },
        C2RustUnnamed_46 {
            command: c"next-prompt",
            args: args_parse {
                template: c"o",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_next_prompt),
        },
        C2RustUnnamed_46 {
            command: c"previous-prompt",
            args: args_parse {
                template: c"o",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_previous_prompt),
        },
        C2RustUnnamed_46 {
            command: c"middle-line",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_middle_line),
        },
        C2RustUnnamed_46 {
            command: c"next-matching-bracket",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_next_matching_bracket),
        },
        C2RustUnnamed_46 {
            command: c"next-paragraph",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_next_paragraph),
        },
        C2RustUnnamed_46 {
            command: c"next-space",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_next_space),
        },
        C2RustUnnamed_46 {
            command: c"next-space-end",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_next_space_end),
        },
        C2RustUnnamed_46 {
            command: c"next-word",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_next_word),
        },
        C2RustUnnamed_46 {
            command: c"next-word-end",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_next_word_end),
        },
        C2RustUnnamed_46 {
            command: c"other-end",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_other_end),
        },
        C2RustUnnamed_46 {
            command: c"page-down",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_page_down),
        },
        C2RustUnnamed_46 {
            command: c"page-down-and-cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_page_down_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"page-up",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_page_up),
        },
        C2RustUnnamed_46 {
            command: c"pipe-no-clear",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_pipe_no_clear),
        },
        C2RustUnnamed_46 {
            command: c"pipe",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_pipe),
        },
        C2RustUnnamed_46 {
            command: c"pipe-and-cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_pipe_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"previous-matching-bracket",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_previous_matching_bracket),
        },
        C2RustUnnamed_46 {
            command: c"previous-paragraph",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_previous_paragraph),
        },
        C2RustUnnamed_46 {
            command: c"previous-space",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_previous_space),
        },
        C2RustUnnamed_46 {
            command: c"previous-word",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_previous_word),
        },
        C2RustUnnamed_46 {
            command: c"recentre-top-bottom",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_recentre_top_bottom),
        },
        C2RustUnnamed_46 {
            command: c"rectangle-on",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_rectangle_on),
        },
        C2RustUnnamed_46 {
            command: c"rectangle-off",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_rectangle_off),
        },
        C2RustUnnamed_46 {
            command: c"rectangle-toggle",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_rectangle_toggle),
        },
        C2RustUnnamed_46 {
            command: c"refresh-on",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_refresh_on),
        },
        C2RustUnnamed_46 {
            command: c"refresh-off",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_refresh_off),
        },
        C2RustUnnamed_46 {
            command: c"refresh-now",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_refresh_now),
        },
        C2RustUnnamed_46 {
            command: c"refresh-toggle",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_refresh_toggle),
        },
        C2RustUnnamed_46 {
            command: c"scroll-bottom",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_bottom),
        },
        C2RustUnnamed_46 {
            command: c"scroll-down",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_scroll_down),
        },
        C2RustUnnamed_46 {
            command: c"scroll-down-and-cancel",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_down_and_cancel),
        },
        C2RustUnnamed_46 {
            command: c"scroll-exit-on",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_exit_on),
        },
        C2RustUnnamed_46 {
            command: c"scroll-exit-off",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_exit_off),
        },
        C2RustUnnamed_46 {
            command: c"scroll-exit-toggle",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_exit_toggle),
        },
        C2RustUnnamed_46 {
            command: c"scroll-middle",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_middle),
        },
        C2RustUnnamed_46 {
            command: c"scroll-to-mouse",
            args: args_parse {
                template: c"e",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_scroll_to_mouse),
        },
        C2RustUnnamed_46 {
            command: c"scroll-top",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_scroll_top),
        },
        C2RustUnnamed_46 {
            command: c"scroll-up",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_scroll_up),
        },
        C2RustUnnamed_46 {
            command: c"search-again",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_again),
        },
        C2RustUnnamed_46 {
            command: c"search-backward",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_backward),
        },
        C2RustUnnamed_46 {
            command: c"search-backward-text",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_backward_text),
        },
        C2RustUnnamed_46 {
            command: c"search-backward-incremental",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_backward_incremental),
        },
        C2RustUnnamed_46 {
            command: c"search-forward",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_forward),
        },
        C2RustUnnamed_46 {
            command: c"search-forward-text",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_forward_text),
        },
        C2RustUnnamed_46 {
            command: c"search-forward-incremental",
            args: args_parse {
                template: c"",
                lower: 1 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_forward_incremental),
        },
        C2RustUnnamed_46 {
            command: c"search-reverse",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_search_reverse),
        },
        C2RustUnnamed_46 {
            command: c"select-line",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_select_line),
        },
        C2RustUnnamed_46 {
            command: c"select-word",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_select_word),
        },
        C2RustUnnamed_46 {
            command: c"selection-mode",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 1 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_selection_mode),
        },
        C2RustUnnamed_46 {
            command: c"set-mark",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_set_mark),
        },
        C2RustUnnamed_46 {
            command: c"start-of-line",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_start_of_line),
        },
        C2RustUnnamed_46 {
            command: c"stop-selection",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: 0 as ::core::ffi::c_int,
            clear: WINDOW_COPY_CMD_CLEAR_ALWAYS,
            f: Some(window_copy_cmd_stop_selection),
        },
        C2RustUnnamed_46 {
            command: c"toggle-position",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_NEVER,
            f: Some(window_copy_cmd_toggle_position),
        },
        C2RustUnnamed_46 {
            command: c"top-line",
            args: args_parse {
                template: c"",
                lower: 0 as ::core::ffi::c_int,
                upper: 0 as ::core::ffi::c_int,
                cb: None,
            },
            flags: WINDOW_COPY_CMD_FLAG_READONLY,
            clear: WINDOW_COPY_CMD_CLEAR_EMACS_ONLY,
            f: Some(window_copy_cmd_top_line),
        },
    ]
};
pub const WINDOW_COPY_CMD_FLAG_READONLY: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
unsafe fn window_copy_command(
    wme: refbox::Weak<window_mode_entry>,
    client_owner: Option<&ClientRef>,
    session: Option<&SessionRef>,
    wl: refbox::Weak<winlink>,
    args: *mut args,
    m: *mut mouse_event,
) {
    let session_owner = session.cloned();
    window_copy_command_with_session(
        wme.clone(),
        client_owner,
        session_owner.as_ref(),
        wl.clone(),
        args,
        m,
    );
    drop(session_owner);
}

unsafe fn window_copy_command_with_session(
    mut wme: refbox::Weak<window_mode_entry>,
    client_owner: Option<&ClientRef>,
    s: Option<&SessionRef>,
    wl: refbox::Weak<winlink>,
    args: *mut args,
    m: *mut mouse_event,
) {
    let mut c: Option<ClientRef> = client_owner.cloned();
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut cs: window_copy_cmd_state = window_copy_cmd_state {
        wme: refbox::Weak::new(),
        format_search: false,
        wargs: None,
        m: None,
        c: None,
        s: None,
        wl: refbox::Weak::new(),
    };
    let mut action: window_copy_cmd_action = WINDOW_COPY_CMD_NOTHING;
    let mut clear: window_copy_cmd_clear = WINDOW_COPY_CMD_CLEAR_NEVER;
    let mut command: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut i: u_int = 0;
    let mut count: u_int = args_count(&*args);
    let mut keys: ::core::ffi::c_int = 0;
    let mut flags: ::core::ffi::c_int = 0;
    if count == 0 as u_int {
        return;
    }
    command =
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    if !m.is_null()
        && (*m).valid != 0
        && !((*m).b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_UP as u_int
            || (*m).b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_DOWN as u_int)
    {
        window_copy_move_mouse(m);
    }
    cs.wme = wme.clone();
    cs.format_search = args_has(&*args, 'F' as i32 as u_char) != 0;
    cs.wargs = None;
    cs.m = m.as_ref().copied();
    cs.c = client_owner;
    cs.s = s;
    cs.wl = wl.clone();
    action = WINDOW_COPY_CMD_MOVE;
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[C2RustUnnamed_46; 99]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_46>() as usize)
    {
        if strcmp(window_copy_cmd_table[i as usize].command.as_ptr(), command)
            == 0 as ::core::ffi::c_int
        {
            flags = window_copy_cmd_table[i as usize].flags;
            if !c.is_none()
                && c.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0
                && !flags & WINDOW_COPY_CMD_FLAG_READONLY != 0
            {
                status_message_set(
                    c.as_ref(),
                    -(1 as ::core::ffi::c_int),
                    1 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    |out| out.write_all(b"client is read-only"),
                );
                return;
            }
            cs.wargs = args_parse(&window_copy_cmd_table[i as usize].args, &(*args).values).ok();
            if cs.wargs.is_none() {
                break;
            }
            clear = window_copy_cmd_table[i as usize].clear;
            action = window_copy_cmd_table[i as usize]
                .f
                .expect("non-null function pointer")(&raw mut cs);
            drop(cs.wargs.take());
            if !cs.wme.is_alive() {
                return;
            }
            break;
        } else {
            i = i.wrapping_add(1);
        }
    }
    if strncmp(command, c"search-".as_ptr(), 7 as size_t) != 0 as ::core::ffi::c_int
        && !(*data).searchmark.is_empty()
    {
        keys = options_get_number(
            &mode_pane_owner
                .window_observer()
                .upgrade()
                .expect("live window")
                .borrow_options_mut(),
            c"mode-keys",
        ) as ::core::ffi::c_int;
        if clear as ::core::ffi::c_uint
            == WINDOW_COPY_CMD_CLEAR_EMACS_ONLY as ::core::ffi::c_int as ::core::ffi::c_uint
            && keys == MODEKEY_VI
        {
            clear = WINDOW_COPY_CMD_CLEAR_NEVER;
        }
        if clear as ::core::ffi::c_uint
            != WINDOW_COPY_CMD_CLEAR_NEVER as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            window_copy_clear_marks(wme.clone());
            (*data).searchy = -(1 as ::core::ffi::c_int);
            (*data).searchx = (*data).searchy;
        }
        if action as ::core::ffi::c_uint
            == WINDOW_COPY_CMD_MOVE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            action = WINDOW_COPY_CMD_REDRAW;
        }
    }
    wme.get_mut_unchecked().prefix = 1 as u_int;
    if action as ::core::ffi::c_uint
        == WINDOW_COPY_CMD_CANCEL as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        mode_pane_owner.reset_mode();
    } else if action as ::core::ffi::c_uint
        == WINDOW_COPY_CMD_REDRAW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        window_copy_redraw_screen(wme.clone());
    } else if action as ::core::ffi::c_uint
        == WINDOW_COPY_CMD_MOVE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        window_copy_redraw_lines(wme.clone(), 0 as u_int, 1 as u_int);
    }
}
unsafe fn window_copy_scroll_to(
    mut wme: refbox::Weak<window_mode_entry>,
    mut px: u_int,
    mut py: u_int,
    mut no_redraw: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
    let mut offset: u_int = 0;
    let mut gap: u_int = 0;
    let mut old_oy: u_int = (*data).oy;
    (*data).cx = px;
    if py >= (*gd).hsize.wrapping_sub((*data).oy)
        && py < (*gd).hsize.wrapping_sub((*data).oy).wrapping_add((*gd).sy)
    {
        (*data).cy = py.wrapping_sub((*gd).hsize.wrapping_sub((*data).oy));
    } else {
        gap = (*gd).sy.wrapping_div(4 as u_int);
        if py < (*gd).sy {
            offset = 0 as u_int;
            (*data).cy = py;
        } else if py > (*gd).hsize.wrapping_add((*gd).sy).wrapping_sub(gap) {
            offset = (*gd).hsize;
            (*data).cy = py.wrapping_sub((*gd).hsize);
        } else {
            offset = py.wrapping_add(gap).wrapping_sub((*gd).sy);
            (*data).cy = py.wrapping_sub(offset);
        }
        (*data).oy = (*gd).hsize.wrapping_sub(offset);
    }
    if no_redraw == 0 && !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if (*data).oy != old_oy {
        mode_pane_owner.show_scrollbar();
    }
    if no_redraw == 0 {
        window_copy_redraw_screen(wme.clone());
    }
}
unsafe fn window_copy_search_compare(
    mut gd: *mut grid,
    mut px: u_int,
    mut py: u_int,
    mut sgd: *mut grid,
    mut spx: u_int,
    mut cis: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
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
    let mut sgc: grid_cell = grid_cell {
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
    grid_get_cell(&*gd, px, py, &mut gc);
    let ud = &gc.data;
    grid_get_cell(&*sgd, spx, 0 as u_int, &mut sgc);
    let sud = &sgc.data;
    if *(&raw const sud.data as *const u_char) as ::core::ffi::c_int == '\t' as i32
        && sud.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
        && gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0
    {
        return 1 as ::core::ffi::c_int;
    }
    if ud.size as ::core::ffi::c_int != sud.size as ::core::ffi::c_int
        || ud.width as ::core::ffi::c_int != sud.width as ::core::ffi::c_int
    {
        return 0 as ::core::ffi::c_int;
    }
    if cis != 0 && ud.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int {
        return (({
            let mut __res: ::core::ffi::c_int = 0;
            if ::core::mem::size_of::<u_char>() as usize > 1_usize {
                if 0 != 0 {
                    let mut __c: ::core::ffi::c_int =
                        ud.data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int;
                    __res =
                        (if __c < -(128 as ::core::ffi::c_int) || __c > 255 as ::core::ffi::c_int {
                            __c as __int32_t
                        } else {
                            ::libc::tolower(__c as ::core::ffi::c_int)
                        }) as ::core::ffi::c_int;
                } else {
                    __res =
                        tolower(ud.data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int);
                }
            } else {
                __res = ::libc::tolower(
                    ud.data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int,
                ) as ::core::ffi::c_int;
            }
            __res
        }) == sud.data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int)
            as ::core::ffi::c_int;
    }
    (memcmp(
        &raw const ud.data as *const u_char as *const ::core::ffi::c_void,
        &raw const sud.data as *const u_char as *const ::core::ffi::c_void,
        ud.size as size_t,
    ) == 0 as ::core::ffi::c_int) as ::core::ffi::c_int
}
unsafe fn window_copy_search_lr(
    mut gd: *mut grid,
    mut sgd: *mut grid,
    mut ppx: *mut u_int,
    mut py: u_int,
    mut first: u_int,
    mut last: u_int,
    mut cis: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut ax: u_int = 0;
    let mut bx: u_int = 0;
    let mut px: u_int = 0;
    let mut pywrap: u_int = 0;
    let mut endline: u_int = 0;
    let mut padding: u_int = 0;
    let mut matched: ::core::ffi::c_int = 0;
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
    endline = (*gd).hsize.wrapping_add((*gd).sy).wrapping_sub(1 as u_int);
    ax = first;
    while ax < last {
        padding = 0 as u_int;
        bx = 0 as u_int;
        while bx < (*sgd).sx {
            px = ax.wrapping_add(bx).wrapping_add(padding);
            pywrap = py;
            while px >= (*gd).sx && pywrap < endline {
                let gl = grid_get_line(&*gd, pywrap);
                if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
                    break;
                }
                px = px.wrapping_sub((*gd).sx);
                pywrap = pywrap.wrapping_add(1);
            }
            if px.wrapping_sub(padding) >= (*gd).sx {
                break;
            }
            grid_get_cell(&*gd, px, pywrap, &mut gc);
            if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
                padding = padding.wrapping_add(
                    (gc.data.width as ::core::ffi::c_int - 1 as ::core::ffi::c_int) as u_int,
                );
            }
            matched = window_copy_search_compare(gd, px, pywrap, sgd, bx, cis);
            if matched == 0 {
                break;
            }
            bx = bx.wrapping_add(1);
        }
        if bx == (*sgd).sx {
            *ppx = ax;
            return 1 as ::core::ffi::c_int;
        }
        ax = ax.wrapping_add(1);
    }
    0 as ::core::ffi::c_int
}
unsafe fn window_copy_search_rl(
    mut gd: *mut grid,
    mut sgd: *mut grid,
    mut ppx: *mut u_int,
    mut py: u_int,
    mut last: u_int,
    mut cis: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut first: u_int = 0 as u_int;
    let mut ax: u_int = 0;
    let mut bx: u_int = 0;
    let mut px: u_int = 0;
    let mut pywrap: u_int = 0;
    let mut endline: u_int = 0;
    let mut padding: u_int = 0;
    let mut matched: ::core::ffi::c_int = 0;
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
    endline = (*gd).hsize.wrapping_add((*gd).sy).wrapping_sub(1 as u_int);
    ax = last;
    while ax > first {
        padding = 0 as u_int;
        bx = 0 as u_int;
        while bx < (*sgd).sx {
            px = ax
                .wrapping_sub(1 as u_int)
                .wrapping_add(bx)
                .wrapping_add(padding);
            pywrap = py;
            while px >= (*gd).sx && pywrap < endline {
                let gl = grid_get_line(&*gd, pywrap);
                if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
                    break;
                }
                px = px.wrapping_sub((*gd).sx);
                pywrap = pywrap.wrapping_add(1);
            }
            if px.wrapping_sub(padding) >= (*gd).sx {
                break;
            }
            grid_get_cell(&*gd, px, pywrap, &mut gc);
            if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
                padding = padding.wrapping_add(
                    (gc.data.width as ::core::ffi::c_int - 1 as ::core::ffi::c_int) as u_int,
                );
            }
            matched = window_copy_search_compare(gd, px, pywrap, sgd, bx, cis);
            if matched == 0 {
                break;
            }
            bx = bx.wrapping_add(1);
        }
        if bx == (*sgd).sx {
            *ppx = ax.wrapping_sub(1 as u_int);
            return 1 as ::core::ffi::c_int;
        }
        ax = ax.wrapping_sub(1);
    }
    0 as ::core::ffi::c_int
}
unsafe fn window_copy_search_lr_regex(
    gd: &grid,
    ppx: &mut u_int,
    psx: &mut u_int,
    mut py: u_int,
    mut first: u_int,
    mut last: u_int,
    reg: &CompiledRegex,
) -> ::core::ffi::c_int {
    let mut eflags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut endline: u_int = 0;
    let mut foundx: u_int = 0;
    let mut foundy: u_int = 0;
    let mut len: u_int = 0;
    let mut pywrap: u_int = 0;
    let mut regmatch: RegexMatch = RegexMatch { rm_so: 0, rm_eo: 0 };
    if first >= last {
        return 0 as ::core::ffi::c_int;
    }
    if first != 0 as u_int {
        eflags |= REG_NOTBOL;
    }
    let mut buf = vec![0u8];
    window_copy_stringify(gd, py, first, gd.sx, &mut buf);
    len = gd.sx.wrapping_sub(first);
    endline = gd.hsize.wrapping_add(gd.sy).wrapping_sub(1 as u_int);
    pywrap = py;
    while pywrap < endline && len < WINDOW_COPY_SEARCH_MAX_LINE as u_int {
        let gl = &gd.linedata[pywrap as usize];
        if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
            break;
        }
        pywrap = pywrap.wrapping_add(1);
        window_copy_stringify(gd, pywrap, 0 as u_int, gd.sx, &mut buf);
        len = len.wrapping_add(gd.sx);
    }
    if reg.execute_at(
        CStr::from_bytes_until_nul(&buf).expect("search line is terminated"),
        0,
        std::slice::from_mut(&mut regmatch),
        eflags,
    ) && regmatch.rm_so != regmatch.rm_eo
    {
        foundx = first;
        foundy = py;
        window_copy_cstrtocellpos(
            gd,
            len,
            &mut foundx,
            &mut foundy,
            &buf[regmatch.rm_so as usize..],
        );
        if foundy == py && foundx < last {
            *ppx = foundx;
            len = len.wrapping_sub(foundx.wrapping_sub(first));
            window_copy_cstrtocellpos(
                gd,
                len,
                &mut foundx,
                &mut foundy,
                &buf[regmatch.rm_eo as usize..],
            );
            *psx = foundx;
            while foundy > py {
                *psx = (*psx).wrapping_add(gd.sx);
                foundy = foundy.wrapping_sub(1);
            }
            *psx = (*psx).wrapping_sub(*ppx);
            return 1 as ::core::ffi::c_int;
        }
    }
    *ppx = 0 as u_int;
    *psx = 0 as u_int;
    0 as ::core::ffi::c_int
}
unsafe fn window_copy_search_rl_regex(
    gd: &grid,
    ppx: &mut u_int,
    psx: &mut u_int,
    mut py: u_int,
    mut last: u_int,
    reg: &CompiledRegex,
) -> ::core::ffi::c_int {
    let mut first: u_int = 0 as u_int;
    let mut eflags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut endline: u_int = 0;
    let mut len: u_int = 0;
    let mut pywrap: u_int = 0;
    let mut buf = vec![0u8];

    window_copy_stringify(gd, py, first, gd.sx, &mut buf);
    len = gd.sx.wrapping_sub(first);
    endline = gd.hsize.wrapping_add(gd.sy).wrapping_sub(1 as u_int);
    pywrap = py;
    while pywrap < endline && len < WINDOW_COPY_SEARCH_MAX_LINE as u_int {
        let gl = &gd.linedata[pywrap as usize];
        if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
            break;
        }
        pywrap = pywrap.wrapping_add(1);
        window_copy_stringify(gd, pywrap, 0 as u_int, gd.sx, &mut buf);
        len = len.wrapping_add(gd.sx);
    }
    if window_copy_last_regex(
        gd,
        py,
        first,
        last,
        len,
        ppx,
        psx,
        CStr::from_bytes_until_nul(&buf).expect("search line is terminated"),
        reg,
        eflags,
    ) != 0
    {
        return 1 as ::core::ffi::c_int;
    }
    *ppx = 0 as u_int;
    *psx = 0 as u_int;
    0 as ::core::ffi::c_int
}
unsafe fn window_copy_cellstring(gl: &grid_line, px: u_int) -> Cow<'_, [u8]> {
    let mut ud = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    if px >= gl.cellsize as u_int {
        return Cow::Borrowed(b" ");
    }
    let gce = &gl.celldata[px as usize];
    if gce.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
        return Cow::Borrowed(b"");
    }
    if !(gce.flags as ::core::ffi::c_int) & GRID_FLAG_EXTENDED != 0 {
        return Cow::Borrowed(std::slice::from_ref(&gce.c2rust_unnamed.data.data));
    }
    if gce.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
        return Cow::Borrowed(b"\t");
    }
    utf8_to_data(
        gl.extddata[gce.c2rust_unnamed.offset as usize].data,
        &mut ud,
    );
    if ud.size == 0 {
        return Cow::Borrowed(b"");
    }
    Cow::Owned(ud.data[..ud.size as usize].to_vec())
}
unsafe fn window_copy_last_regex(
    gd: &grid,
    mut py: u_int,
    mut first: u_int,
    mut last: u_int,
    mut len: u_int,
    ppx: &mut u_int,
    psx: &mut u_int,
    buf: &CStr,
    preg: &CompiledRegex,
    mut eflags: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut foundx: u_int = 0;
    let mut foundy: u_int = 0;
    let mut oldx: u_int = 0;
    let mut px: u_int = 0 as u_int;
    let mut savepx: u_int = 0;
    let mut savesx: u_int = 0 as u_int;
    let mut regmatch: RegexMatch = RegexMatch { rm_so: 0, rm_eo: 0 };
    foundx = first;
    foundy = py;
    oldx = first;
    while preg.execute_at(
        buf,
        px as usize,
        std::slice::from_mut(&mut regmatch),
        eflags,
    ) {
        if regmatch.rm_so == regmatch.rm_eo {
            break;
        }
        window_copy_cstrtocellpos(
            gd,
            len,
            &mut foundx,
            &mut foundy,
            &buf.to_bytes()[px as usize + regmatch.rm_so as usize..],
        );
        if foundy > py || foundx >= last {
            break;
        }
        len = len.wrapping_sub(foundx.wrapping_sub(oldx));
        savepx = foundx;
        window_copy_cstrtocellpos(
            gd,
            len,
            &mut foundx,
            &mut foundy,
            &buf.to_bytes()[px as usize + regmatch.rm_eo as usize..],
        );
        if foundy > py || foundx >= last {
            *ppx = savepx;
            *psx = foundx;
            while foundy > py {
                *psx = (*psx).wrapping_add(gd.sx);
                foundy = foundy.wrapping_sub(1);
            }
            *psx = (*psx).wrapping_sub(*ppx);
            return 1 as ::core::ffi::c_int;
        } else {
            savesx = foundx.wrapping_sub(savepx);
            len = len.wrapping_sub(savesx);
            oldx = foundx;
        }
        px = px.wrapping_add(regmatch.rm_eo as u_int);
    }
    if savesx > 0 as u_int {
        *ppx = savepx;
        *psx = savesx;
        1 as ::core::ffi::c_int
    } else {
        *ppx = 0 as u_int;
        *psx = 0 as u_int;
        0 as ::core::ffi::c_int
    }
}
fn window_copy_search_line(gd: &grid, py: u_int) -> Option<&grid_line> {
    if py >= gd.hsize.wrapping_add(gd.sy) {
        log_debug(format_args!("grid_peek_line: y out of range: {}", py));
        return None;
    }
    gd.linedata.get(py as usize)
}

unsafe fn window_copy_stringify(
    gd: &grid,
    py: u_int,
    first: u_int,
    last: u_int,
    buf: &mut Vec<u8>,
) {
    let Some(gl) = window_copy_search_line(gd, py) else {
        return;
    };
    buf.pop();
    for ax in first..last {
        buf.extend_from_slice(&window_copy_cellstring(gl, ax));
    }
    buf.push(0);
}
unsafe fn window_copy_cstrtocellpos(
    gd: &grid,
    ncells: u_int,
    ppx: &mut u_int,
    ppy: &mut u_int,
    text: &[u8],
) {
    let mut cells: Vec<Cow<'_, [u8]>> = Vec::with_capacity(ncells as usize);
    let mut px = *ppx;
    let mut py = *ppy;
    let Some(mut line) = window_copy_search_line(gd, py) else {
        return;
    };
    for _ in 0..ncells {
        cells.push(window_copy_cellstring(line, px));
        px = px.wrapping_add(1);
        if px == gd.sx {
            px = 0;
            py = py.wrapping_add(1);
            let Some(next) = window_copy_search_line(gd, py) else {
                break;
            };
            line = next;
        }
    }

    let text = &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())];
    let cell = window_copy_find_cell_suffix(&cells, text);
    px = ppx.wrapping_add(cell as u_int);
    py = *ppy;
    while px >= gd.sx {
        px = px.wrapping_sub(gd.sx);
        py = py.wrapping_add(1);
    }
    *ppx = px;
    *ppy = py;
}

fn window_copy_find_cell_suffix(cells: &[Cow<'_, [u8]>], text: &[u8]) -> usize {
    for first in 0..cells.len() {
        let mut remaining = text;
        let matches = cells[first..].iter().all(|cell| {
            if remaining.is_empty() {
                return false;
            }
            let len = cell.len().min(remaining.len());
            if remaining[..len] != cell[..len] {
                return false;
            }
            remaining = &remaining[len..];
            true
        });
        if matches {
            return first;
        }
    }
    cells.len()
}
unsafe fn window_copy_move_left(
    mut s: *mut screen,
    mut fx: *mut u_int,
    mut fy: *mut u_int,
    mut wrapflag: ::core::ffi::c_int,
) {
    if *fx == 0 as u_int {
        if *fy == 0 as u_int {
            if wrapflag != 0 {
                *fx = (*s).grid().sx.wrapping_sub(1 as u_int);
                *fy = (*s)
                    .grid()
                    .hsize
                    .wrapping_add((*s).grid().sy)
                    .wrapping_sub(1 as u_int);
            }
            return;
        }
        *fx = (*s).grid().sx.wrapping_sub(1 as u_int);
        *fy = (*fy).wrapping_sub(1 as u_int);
    } else {
        *fx = (*fx).wrapping_sub(1 as u_int);
    };
}
unsafe fn window_copy_move_right(
    mut s: *mut screen,
    mut fx: *mut u_int,
    mut fy: *mut u_int,
    mut wrapflag: ::core::ffi::c_int,
) {
    if *fx == (*s).grid().sx.wrapping_sub(1 as u_int) {
        if *fy
            == (*s)
                .grid()
                .hsize
                .wrapping_add((*s).grid().sy)
                .wrapping_sub(1 as u_int)
        {
            if wrapflag != 0 {
                *fx = 0 as u_int;
                *fy = 0 as u_int;
            }
            return;
        }
        *fx = 0 as u_int;
        *fy = (*fy).wrapping_add(1 as u_int);
    } else {
        *fx = (*fx).wrapping_add(1 as u_int);
    };
}
unsafe fn window_copy_is_lowercase(mut ptr: *const ::core::ffi::c_char) -> ::core::ffi::c_int {
    while *ptr as ::core::ffi::c_int != '\0' as i32 {
        if *ptr as ::core::ffi::c_int
            != ({
                let mut __res: ::core::ffi::c_int = 0;
                if ::core::mem::size_of::<u_char>() as usize > 1_usize {
                    if 0 != 0 {
                        let mut __c: ::core::ffi::c_int = *ptr as u_char as ::core::ffi::c_int;
                        __res = (if __c < -(128 as ::core::ffi::c_int)
                            || __c > 255 as ::core::ffi::c_int
                        {
                            __c as __int32_t
                        } else {
                            ::libc::tolower(__c as ::core::ffi::c_int)
                        }) as ::core::ffi::c_int;
                    } else {
                        __res = tolower(*ptr as u_char as ::core::ffi::c_int);
                    }
                } else {
                    __res =
                        ::libc::tolower(*ptr as u_char as ::core::ffi::c_int) as ::core::ffi::c_int;
                }
                __res
            })
        {
            return 0 as ::core::ffi::c_int;
        }
        ptr = ptr.offset(1);
    }
    1 as ::core::ffi::c_int
}
unsafe fn window_copy_search_back_overlap(
    gd: &grid,
    preg: &CompiledRegex,
    ppx: &mut u_int,
    psx: &mut u_int,
    ppy: &mut u_int,
    mut endline: u_int,
) {
    let mut endx: u_int = 0;
    let mut endy: u_int = 0;
    let mut oldendx: u_int = 0;
    let mut oldendy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut sx: u_int = 0;
    let mut found: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
    oldendx = (*ppx).wrapping_add(*psx);
    oldendy = (*ppy).wrapping_sub(1 as u_int);
    while oldendx > gd.sx.wrapping_sub(1 as u_int) {
        oldendx = oldendx.wrapping_sub(gd.sx);
        oldendy = oldendy.wrapping_add(1);
    }
    endx = oldendx;
    endy = oldendy;
    px = *ppx;
    py = *ppy;
    while found != 0
        && px == 0 as u_int
        && py.wrapping_sub(1 as u_int) > endline
        && gd.linedata[py.wrapping_sub(2) as usize].flags as ::core::ffi::c_int & GRID_LINE_WRAPPED
            != 0
        && endx == oldendx
        && endy == oldendy
    {
        py = py.wrapping_sub(1);
        found = window_copy_search_rl_regex(
            gd,
            &mut px,
            &mut sx,
            py.wrapping_sub(1 as u_int),
            gd.sx,
            preg,
        );
        if found != 0 {
            endx = px.wrapping_add(sx);
            endy = py.wrapping_sub(1 as u_int);
            while endx > gd.sx.wrapping_sub(1 as u_int) {
                endx = endx.wrapping_sub(gd.sx);
                endy = endy.wrapping_add(1);
            }
            if endx == oldendx && endy == oldendy {
                *ppx = px;
                *ppy = py;
            }
        }
    }
}
unsafe fn window_copy_search_jump(
    mut wme: refbox::Weak<window_mode_entry>,
    mut gd: *mut grid,
    mut sgd: *mut grid,
    mut fx: u_int,
    mut fy: u_int,
    mut endline: u_int,
    mut cis: ::core::ffi::c_int,
    mut wrap: ::core::ffi::c_int,
    mut direction: ::core::ffi::c_int,
    mut regex: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut i: u_int = 0;
    let mut px: u_int = 0;
    let mut sx: u_int = 0;
    let mut found: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut cflags: ::core::ffi::c_int = REG_EXTENDED;
    let mut regex_storage = RegexStorage::default();
    let regex_owner = if regex != 0 {
        let mut sbuf = vec![0u8];
        window_copy_stringify(&*sgd, 0 as u_int, 0 as u_int, (*sgd).sx, &mut sbuf);
        if cis != 0 {
            cflags |= REG_ICASE;
        }
        let Ok(compiled) = regex_storage.compile(
            CStr::from_bytes_until_nul(&sbuf).expect("search pattern is terminated"),
            cflags,
        ) else {
            return 0 as ::core::ffi::c_int;
        };
        Some(compiled)
    } else {
        None
    };
    if direction != 0 {
        i = fy;
        while i <= endline {
            if regex != 0 {
                found = window_copy_search_lr_regex(
                    &*gd,
                    &mut px,
                    &mut sx,
                    i,
                    fx,
                    (*gd).sx,
                    regex_owner
                        .as_ref()
                        .expect("regex search has a compiled pattern"),
                );
            } else {
                found = window_copy_search_lr(gd, sgd, &raw mut px, i, fx, (*gd).sx, cis);
            }
            if found != 0 {
                break;
            }
            fx = 0 as u_int;
            i = i.wrapping_add(1);
        }
    } else {
        i = fy.wrapping_add(1 as u_int);
        while endline < i {
            if regex != 0 {
                found = window_copy_search_rl_regex(
                    &*gd,
                    &mut px,
                    &mut sx,
                    i.wrapping_sub(1 as u_int),
                    fx.wrapping_add(1 as u_int),
                    regex_owner
                        .as_ref()
                        .expect("regex search has a compiled pattern"),
                );
                if found != 0 {
                    window_copy_search_back_overlap(
                        &*gd,
                        regex_owner
                            .as_ref()
                            .expect("regex search has a compiled pattern"),
                        &mut px,
                        &mut sx,
                        &mut i,
                        endline,
                    );
                }
            } else {
                found = window_copy_search_rl(
                    gd,
                    sgd,
                    &raw mut px,
                    i.wrapping_sub(1 as u_int),
                    fx.wrapping_add(1 as u_int),
                    cis,
                );
            }
            if found != 0 {
                i = i.wrapping_sub(1);
                break;
            } else {
                fx = (*gd).sx.wrapping_sub(1 as u_int);
                i = i.wrapping_sub(1);
            }
        }
    }
    drop(regex_owner);
    if found != 0 {
        window_copy_scroll_to(wme.clone(), px, i, 1 as ::core::ffi::c_int);
        return 1 as ::core::ffi::c_int;
    }
    if wrap != 0 {
        return window_copy_search_jump(
            wme.clone(),
            gd,
            sgd,
            if direction != 0 {
                0 as u_int
            } else {
                (*gd).sx.wrapping_sub(1 as u_int)
            },
            if direction != 0 {
                0 as u_int
            } else {
                (*gd).hsize.wrapping_add((*gd).sy).wrapping_sub(1 as u_int)
            },
            fy,
            cis,
            0 as ::core::ffi::c_int,
            direction,
            regex,
        );
    }
    0 as ::core::ffi::c_int
}
unsafe fn window_copy_move_after_search_mark(
    mut data: *mut window_copy_mode_data,
    mut fx: *mut u_int,
    mut fy: *mut u_int,
    mut wrapflag: ::core::ffi::c_int,
) {
    let mut s: *mut screen = (*data).backing_mut();
    let mut at: u_int = 0;
    let mut start: u_int = 0;
    if window_copy_search_mark_at(data, *fx, *fy, &raw mut start) == 0 as ::core::ffi::c_int
        && (&(*data).searchmark)[start as usize] as ::core::ffi::c_int != 0 as ::core::ffi::c_int
    {
        while window_copy_search_mark_at(data, *fx, *fy, &raw mut at) == 0 as ::core::ffi::c_int {
            if (&(*data).searchmark)[at as usize] as ::core::ffi::c_int
                != (&(*data).searchmark)[start as usize] as ::core::ffi::c_int
            {
                break;
            }
            if wrapflag == 0
                && *fx == (*s).grid().sx.wrapping_sub(1 as u_int)
                && *fy
                    == (*s)
                        .grid()
                        .hsize
                        .wrapping_add((*s).grid().sy)
                        .wrapping_sub(1 as u_int)
            {
                break;
            }
            window_copy_move_right(s, fx, fy, wrapflag);
        }
    }
}
unsafe fn window_copy_search(
    mut wme: refbox::Weak<window_mode_entry>,
    mut direction: ::core::ffi::c_int,
    mut regex: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = (*data).backing_mut();
    let mut ss: screen = screen::empty();
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut gd: *mut grid = (*s).grid_mut();
    let searchstr = window_copy_searchstr(&*data)
        .expect("search has a string")
        .to_owned();
    let str = searchstr.as_c_str();
    let mut at: u_int = 0;
    let mut endline: u_int = 0;
    let mut fx: u_int = 0;
    let mut fy: u_int = 0;
    let mut start: u_int = 0;
    let mut ssx: u_int = 0;
    let mut cis: ::core::ffi::c_int = 0;
    let mut found: ::core::ffi::c_int = 0;
    let mut keys: ::core::ffi::c_int = 0;
    let mut visible_only: ::core::ffi::c_int = 0;
    let mut wrapflag: ::core::ffi::c_int = 0;
    if regex != 0
        && !str
            .to_bytes()
            .iter()
            .any(|byte| b"^$*+()?[].\\".contains(byte))
    {
        regex = 0 as ::core::ffi::c_int;
    }
    (*data).searchdirection = direction;
    if (*data).timeout != 0 {
        return 0 as ::core::ffi::c_int;
    }
    let (saved_search, saved_regex) = mode_pane_owner.saved_search();
    if (*data).searchall != 0 || saved_search.is_none() || saved_regex != regex {
        visible_only = 0 as ::core::ffi::c_int;
        (*data).searchall = 0 as ::core::ffi::c_int;
    } else {
        visible_only = (saved_search.as_deref() == Some(str)) as ::core::ffi::c_int;
    }
    if visible_only == 0 as ::core::ffi::c_int && !(*data).searchmark.is_empty() {
        window_copy_clear_marks(wme.clone());
    }
    mode_pane_owner.save_search(searchstr.clone(), regex);
    fx = (*data).cx;
    fy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_sub((*data).oy)
        .wrapping_add((*data).cy);
    ssx = screen_write_strlen(str) as u_int;
    if ssx == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    screen_init(&mut ss, ssx, 1 as u_int, 0 as u_int);
    screen_write_start(&mut ctx, &raw mut ss);
    screen_write_nputs(
        &mut ctx,
        -(1 as ::core::ffi::c_int) as ssize_t,
        &grid_default_cell,
        |out| write_cstr(out, str),
    );
    screen_write_stop(&mut ctx);
    wrapflag = options_get_number(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live window")
            .borrow_options_mut(),
        c"wrap-search",
    ) as ::core::ffi::c_int;
    cis = window_copy_is_lowercase(str.as_ptr());
    keys = options_get_number(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live window")
            .borrow_options_mut(),
        c"mode-keys",
    ) as ::core::ffi::c_int;
    if direction != 0 {
        if keys == MODEKEY_VI {
            if !(*data).searchmark.is_empty() {
                window_copy_move_after_search_mark(data, &raw mut fx, &raw mut fy, wrapflag);
            } else {
                window_copy_move_right(s, &raw mut fx, &raw mut fy, wrapflag);
            }
        }
        endline = (*gd).hsize.wrapping_add((*gd).sy).wrapping_sub(1 as u_int);
    } else {
        window_copy_move_left(s, &raw mut fx, &raw mut fy, wrapflag);
        endline = 0 as u_int;
    }
    found = window_copy_search_jump(
        wme.clone(),
        gd,
        ss.grid_mut(),
        fx,
        fy,
        endline,
        cis,
        wrapflag,
        direction,
        regex,
    );
    if found != 0 {
        window_copy_search_marks(wme.clone(), &raw mut ss, regex, visible_only);
        fx = (*data).cx;
        fy = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_sub((*data).oy)
            .wrapping_add((*data).cy);
        if direction != 0
            && window_copy_search_mark_at(data, fx, fy, &raw mut at) == 0 as ::core::ffi::c_int
            && at > 0 as u_int
            && !(*data).searchmark.is_empty()
            && (&(*data).searchmark)[at as usize] as ::core::ffi::c_int
                == (&(*data).searchmark)[at.wrapping_sub(1 as u_int) as usize] as ::core::ffi::c_int
        {
            window_copy_move_after_search_mark(data, &raw mut fx, &raw mut fy, wrapflag);
            window_copy_search_jump(
                wme.clone(),
                gd,
                ss.grid_mut(),
                fx,
                fy,
                endline,
                cis,
                wrapflag,
                direction,
                regex,
            );
            fx = (*data).cx;
            fy = (*data)
                .backing()
                .grid()
                .hsize
                .wrapping_sub((*data).oy)
                .wrapping_add((*data).cy);
        }
        if direction != 0 {
            if keys == MODEKEY_EMACS {
                window_copy_move_after_search_mark(data, &raw mut fx, &raw mut fy, wrapflag);
                (*data).cx = fx;
                (*data).cy = fy
                    .wrapping_sub((*data).backing().grid().hsize)
                    .wrapping_add((*data).oy);
            }
        } else if window_copy_search_mark_at(data, fx, fy, &raw mut start)
            == 0 as ::core::ffi::c_int
        {
            while window_copy_search_mark_at(data, fx, fy, &raw mut at) == 0 as ::core::ffi::c_int
                && !(*data).searchmark.is_empty()
                && (&(*data).searchmark)[at as usize] as ::core::ffi::c_int
                    == (&(*data).searchmark)[start as usize] as ::core::ffi::c_int
            {
                (*data).cx = fx;
                (*data).cy = fy
                    .wrapping_sub((*data).backing().grid().hsize)
                    .wrapping_add((*data).oy);
                if at == 0 as u_int {
                    break;
                }
                window_copy_move_left(s, &raw mut fx, &raw mut fy, 0 as ::core::ffi::c_int);
            }
        }
    }
    window_copy_redraw_screen(wme.clone());
    drop(ss);
    found
}
unsafe fn window_copy_visible_lines(
    mut data: *mut window_copy_mode_data,
    mut start: *mut u_int,
    mut end: *mut u_int,
) {
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
    *start = (*gd).hsize.wrapping_sub((*data).oy);
    while *start > 0 as u_int {
        let Some(gl) = grid_peek_line(&*gd, (*start).wrapping_sub(1 as u_int)) else {
            break;
        };
        if !(gl.flags as i32) & GRID_LINE_WRAPPED != 0 {
            break;
        }
        *start = (*start).wrapping_sub(1);
    }
    *end = (*gd).hsize.wrapping_sub((*data).oy).wrapping_add((*gd).sy);
}
unsafe fn window_copy_search_mark_at(
    mut data: *mut window_copy_mode_data,
    mut px: u_int,
    mut py: u_int,
    mut at: *mut u_int,
) -> ::core::ffi::c_int {
    let mut s: *mut screen = (*data).backing_mut();
    let mut gd: *mut grid = (*s).grid_mut();
    if py < (*gd).hsize.wrapping_sub((*data).oy) {
        return -(1 as ::core::ffi::c_int);
    }
    if py
        > (*gd)
            .hsize
            .wrapping_sub((*data).oy)
            .wrapping_add((*gd).sy)
            .wrapping_sub(1 as u_int)
    {
        return -(1 as ::core::ffi::c_int);
    }
    *at = py
        .wrapping_sub((*gd).hsize.wrapping_sub((*data).oy))
        .wrapping_mul((*gd).sx)
        .wrapping_add(px);
    0 as ::core::ffi::c_int
}
fn window_copy_clip_width(mut width: u_int, mut b: u_int, mut sx: u_int, mut sy: u_int) -> u_int {
    if b.wrapping_add(width) > sx.wrapping_mul(sy) {
        sx.wrapping_mul(sy).wrapping_sub(b)
    } else {
        width
    }
}
unsafe fn window_copy_search_mark_match(
    mut data: *mut window_copy_mode_data,
    mut px: u_int,
    mut py: u_int,
    mut width: u_int,
    mut regex: ::core::ffi::c_int,
) -> u_int {
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
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
    let mut i: u_int = 0;
    let mut b: u_int = 0;
    let mut w: u_int = width;
    let mut sx: u_int = (*gd).sx;
    let mut sy: u_int = (*gd).sy;
    if window_copy_search_mark_at(data, px, py, &raw mut b) == 0 as ::core::ffi::c_int {
        width = window_copy_clip_width(width, b, sx, sy);
        w = width;
        i = b;
        while i < b.wrapping_add(w) {
            if regex == 0 {
                grid_get_cell(&*gd, px.wrapping_add(i.wrapping_sub(b)), py, &mut gc);
                if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
                    w = w.wrapping_add(
                        (gc.data.width as ::core::ffi::c_int - 1 as ::core::ffi::c_int) as u_int,
                    );
                }
                w = window_copy_clip_width(w, b, sx, sy);
            }
            if !((&(*data).searchmark)[i as usize] as ::core::ffi::c_int != 0 as ::core::ffi::c_int)
            {
                (&mut (*data).searchmark)[i as usize] = (*data).searchgen;
            }
            i = i.wrapping_add(1);
        }
        if (*data).searchgen as ::core::ffi::c_int == UCHAR_MAX {
            (*data).searchgen = 1 as u_char;
        } else {
            (*data).searchgen = (*data).searchgen.wrapping_add(1);
        }
    }
    w
}
fn window_copy_clear_searchmark(data: &mut window_copy_mode_data) {
    data.searchmark.clear();
}

unsafe fn window_copy_replace_searchmark(data: &mut window_copy_mode_data, sx: u_int, sy: u_int) {
    window_copy_clear_searchmark(data);
    if sx == 0 || sy == 0 {
        fatalx(|out| out.write_all(b"xcalloc: zero size"));
    }
    let Some(len) = (sx as usize).checked_mul(sy as usize) else {
        fatalx(|out| out.write_all(b"xcalloc: nmemb * size > SIZE_MAX"));
    };
    let marks = &mut data.searchmark;
    if marks.try_reserve_exact(len).is_err() {
        fatal(|out| write!(out, "xcalloc: allocating {} bytes", { len as size_t }));
    }
    marks.resize(len, 0);
}

unsafe fn window_copy_search_marks(
    mut wme: refbox::Weak<window_mode_entry>,
    mut ssp: *mut screen,
    mut regex: ::core::ffi::c_int,
    mut visible_only: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = (*data).backing_mut();
    let mut ss: screen = screen::empty();
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut gd: *mut grid = (*s).grid_mut();
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
    let mut found: ::core::ffi::c_int = 0;
    let mut cis: ::core::ffi::c_int = 0;
    let mut stopped: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut cflags: ::core::ffi::c_int = REG_EXTENDED;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut nfound: u_int = 0 as u_int;
    let mut width: u_int = 0;
    let mut start: u_int = 0;
    let mut end: u_int = 0;
    let mut sx: u_int = (*gd).sx;
    let mut sy: u_int = (*gd).sy;
    let mut regex_storage = RegexStorage::default();
    let mut stop: uint64_t = 0 as uint64_t;
    let mut tstart: uint64_t = 0;
    let mut t: uint64_t = 0;
    if ssp.is_null() {
        width = screen_write_strlen((*data).searchstr.as_deref().unwrap_or(c"(null)")) as u_int;
        screen_init(&mut ss, width, 1 as u_int, 0 as u_int);
        screen_write_start(&mut ctx, &raw mut ss);
        screen_write_nputs(
            &mut ctx,
            -(1 as ::core::ffi::c_int) as ssize_t,
            &grid_default_cell,
            |out| write_cstr(out, window_copy_searchstr(&*data)),
        );
        screen_write_stop(&mut ctx);
        ssp = &raw mut ss;
    } else {
        width = (*ssp).grid().sx;
    }
    cis = window_copy_is_lowercase(
        window_copy_searchstr(&*data).map_or(std::ptr::null(), std::ffi::CStr::as_ptr),
    );
    let regex_owner = if regex != 0 {
        let mut sbuf = vec![0u8];
        window_copy_stringify(
            (*ssp).grid(),
            0 as u_int,
            0 as u_int,
            (*ssp).grid().sx,
            &mut sbuf,
        );
        if cis != 0 {
            cflags |= REG_ICASE;
        }
        let Ok(compiled) = regex_storage.compile(
            CStr::from_bytes_until_nul(&sbuf).expect("search pattern is terminated"),
            cflags,
        ) else {
            window_copy_clear_searchmark(&mut *data);
            return 0 as ::core::ffi::c_int;
        };
        Some(compiled)
    } else {
        None
    };
    tstart = get_timer();
    if visible_only != 0 {
        window_copy_visible_lines(data, &raw mut start, &raw mut end);
    } else {
        start = 0 as u_int;
        end = (*gd).hsize.wrapping_add(sy);
        stop = get_timer().wrapping_add(WINDOW_COPY_SEARCH_ALL_TIMEOUT as uint64_t);
    }
    loop {
        window_copy_replace_searchmark(&mut *data, sx, sy);
        (*data).searchgen = 1 as u_char;
        py = start;
        while py < end {
            px = 0 as u_int;
            loop {
                if regex != 0 {
                    let first = px;
                    found = window_copy_search_lr_regex(
                        &*gd,
                        &mut px,
                        &mut width,
                        py,
                        first,
                        sx,
                        regex_owner
                            .as_ref()
                            .expect("regex search has a compiled pattern"),
                    );
                    grid_get_cell(
                        &*gd,
                        px.wrapping_add(width).wrapping_sub(1 as u_int),
                        py,
                        &mut gc,
                    );
                    if gc.data.width as ::core::ffi::c_int > 2 as ::core::ffi::c_int {
                        width = width.wrapping_add(
                            (gc.data.width as ::core::ffi::c_int - 1 as ::core::ffi::c_int)
                                as u_int,
                        );
                    }
                    if found == 0 {
                        break;
                    }
                } else {
                    found =
                        window_copy_search_lr(gd, (*ssp).grid_mut(), &raw mut px, py, px, sx, cis);
                    if found == 0 {
                        break;
                    }
                }
                nfound = nfound.wrapping_add(1);
                px = px.wrapping_add(window_copy_search_mark_match(data, px, py, width, regex));
            }
            t = get_timer();
            if t.wrapping_sub(tstart) > WINDOW_COPY_SEARCH_TIMEOUT as uint64_t {
                (*data).timeout = 1 as ::core::ffi::c_int;
                break;
            } else if stop != 0 as uint64_t && t > stop {
                stopped = 1 as ::core::ffi::c_int;
                break;
            } else {
                py = py.wrapping_add(1);
            }
        }
        if (*data).timeout != 0 {
            window_copy_clear_marks(wme.clone());
            break;
        } else if stopped != 0 && stop != 0 as uint64_t {
            window_copy_visible_lines(data, &raw mut start, &raw mut end);
            stop = 0 as uint64_t;
        } else {
            if visible_only == 0 {
                if stopped != 0 {
                    if nfound > 1000 as u_int {
                        (*data).searchcount = 1000 as ::core::ffi::c_int;
                    } else if nfound > 100 as u_int {
                        (*data).searchcount = 100 as ::core::ffi::c_int;
                    } else if nfound > 10 as u_int {
                        (*data).searchcount = 10 as ::core::ffi::c_int;
                    } else {
                        (*data).searchcount = -(1 as ::core::ffi::c_int);
                    }
                    (*data).searchmore = 1 as ::core::ffi::c_int;
                } else {
                    (*data).searchcount = nfound as ::core::ffi::c_int;
                    (*data).searchmore = 0 as ::core::ffi::c_int;
                }
            }
            break;
        }
    }
    if ssp == &raw mut ss {
        drop(ss);
    }
    drop(regex_owner);
    1 as ::core::ffi::c_int
}
unsafe fn window_copy_clear_marks(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).searchcount = -(1 as ::core::ffi::c_int);
    (*data).searchmore = 0 as ::core::ffi::c_int;
    window_copy_clear_searchmark(&mut *data);
}
unsafe fn window_copy_search_up(
    mut wme: refbox::Weak<window_mode_entry>,
    mut regex: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    window_copy_search(wme.clone(), 0 as ::core::ffi::c_int, regex)
}
unsafe fn window_copy_search_down(
    mut wme: refbox::Weak<window_mode_entry>,
    mut regex: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    window_copy_search(wme.clone(), 1 as ::core::ffi::c_int, regex)
}
unsafe fn window_copy_goto_line(mut wme: refbox::Weak<window_mode_entry>, linestr: &CStr) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut hsize: u_int = (*data).backing().grid().hsize;
    let mut line: u_int = 0;
    let mut lineno: ::core::ffi::c_int = 0;
    let parsed = strtonum(
        linestr,
        -(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong,
        INT_MAX as ::core::ffi::c_longlong,
    );
    lineno = parsed.unwrap_or(0) as ::core::ffi::c_int;
    if parsed.is_err() {
        return;
    }
    if window_copy_line_number_is_absolute(wme.clone()) != 0 {
        if lineno <= 0 as ::core::ffi::c_int {
            line = 1 as u_int;
        } else if lineno as u_int > hsize.wrapping_add(1 as u_int) {
            line = hsize.wrapping_add(1 as u_int);
        } else {
            line = lineno as u_int;
        }
        (*data).oy = hsize.wrapping_sub(line.wrapping_sub(1 as u_int));
    } else {
        if lineno < 0 as ::core::ffi::c_int || lineno as u_int > hsize {
            lineno = hsize as ::core::ffi::c_int;
        }
        (*data).oy = lineno as u_int;
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_match_start_end(
    mut data: *mut window_copy_mode_data,
    mut at: u_int,
    mut start: *mut u_int,
    mut end: *mut u_int,
) {
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
    let mut last: u_int = (*gd).sy.wrapping_mul((*gd).sx).wrapping_sub(1 as u_int);
    let mut mark: u_char = (&(*data).searchmark)[at as usize];
    *end = at;
    *start = *end;
    while *start != 0 as u_int
        && (&(*data).searchmark)[*start as usize] as ::core::ffi::c_int
            == mark as ::core::ffi::c_int
    {
        *start = (*start).wrapping_sub(1);
    }
    if (&(*data).searchmark)[*start as usize] as ::core::ffi::c_int != mark as ::core::ffi::c_int {
        *start = (*start).wrapping_add(1);
    }
    while *end != last
        && (&(*data).searchmark)[*end as usize] as ::core::ffi::c_int == mark as ::core::ffi::c_int
    {
        *end = (*end).wrapping_add(1);
    }
    if (&(*data).searchmark)[*end as usize] as ::core::ffi::c_int != mark as ::core::ffi::c_int {
        *end = (*end).wrapping_sub(1);
    }
}
unsafe fn window_copy_match_at_cursor_bytes(
    mut data: *mut window_copy_mode_data,
) -> Option<Vec<u8>> {
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
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
    let mut at: u_int = 0;
    let mut start: u_int = 0;
    let mut end: u_int = 0;
    let mut cy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut sx: u_int = (*data).backing().grid().sx;
    let mut output = Vec::<u8>::new();
    if (*data).searchmark.is_empty() {
        return None;
    }
    cy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_sub((*data).oy)
        .wrapping_add((*data).cy);
    if window_copy_search_mark_at(data, (*data).cx, cy, &raw mut at) != 0 as ::core::ffi::c_int {
        return None;
    }
    if (&(*data).searchmark)[at as usize] as ::core::ffi::c_int == 0 as ::core::ffi::c_int
        && (at == 0 as u_int || {
            at = at.wrapping_sub(1);
            (&(*data).searchmark)[at as usize] as ::core::ffi::c_int == 0 as ::core::ffi::c_int
        })
    {
        return None;
    }
    window_copy_match_start_end(data, at, &raw mut start, &raw mut end);
    at = start;
    while at <= end {
        py = at.wrapping_div(sx);
        px = at.wrapping_sub(py.wrapping_mul(sx));
        grid_get_cell(
            &*gd,
            px,
            (*gd).hsize.wrapping_add(py).wrapping_sub((*data).oy),
            &mut gc,
        );
        if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
            output.push(b'\t');
        } else if !(gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0) {
            output.extend_from_slice(std::slice::from_raw_parts(
                gc.data.data.as_ptr(),
                gc.data.size as usize,
            ));
        }
        at = at.wrapping_add(1);
    }
    if output.is_empty() {
        return None;
    }
    Some(output)
}
unsafe fn window_copy_match_at_cursor_cstring(data: *mut window_copy_mode_data) -> Option<CString> {
    let mut output = window_copy_match_at_cursor_bytes(data)?;
    if let Some(end) = output.iter().position(|&byte| byte == 0) {
        output.truncate(end);
    }
    Some(CString::new(output).expect("format match contains no NUL"))
}
unsafe fn window_copy_update_style(
    mut wme: refbox::Weak<window_mode_entry>,
    mut fx: u_int,
    mut fy: u_int,
    mut gc: *mut grid_cell,
    mgc: &grid_cell,
    cgc: &grid_cell,
    mkgc: &grid_cell,
    clgc: &grid_cell,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut mark: u_int = 0;
    let mut start: u_int = 0;
    let mut end: u_int = 0;
    let mut cy: u_int = 0;
    let mut cursor: u_int = 0;
    let mut current: u_int = 0;
    let mut inv: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut found: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut keys: ::core::ffi::c_int = 0;
    cy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_sub((*data).oy)
        .wrapping_add((*data).cy);
    if fy == cy {
        if clgc.fg != 8 as ::core::ffi::c_int {
            (*gc).fg = clgc.fg;
        }
        if clgc.bg != 8 as ::core::ffi::c_int {
            (*gc).bg = clgc.bg;
        }
        (*gc).attr =
            ((*gc).attr as ::core::ffi::c_int | clgc.attr as ::core::ffi::c_int) as u_short;
    }
    if (*data).showmark != 0 && fy == (*data).my {
        (*gc).attr = mkgc.attr;
        if fx == (*data).mx {
            inv = 1 as ::core::ffi::c_int;
        }
        if inv != 0 {
            (*gc).fg = mkgc.bg;
            (*gc).bg = mkgc.fg;
        } else {
            (*gc).fg = mkgc.fg;
            (*gc).bg = mkgc.bg;
        }
    }
    if (*data).searchmark.is_empty() {
        return;
    }
    if window_copy_search_mark_at(data, fx, fy, &raw mut current) != 0 as ::core::ffi::c_int {
        return;
    }
    mark = (&(*data).searchmark)[current as usize] as u_int;
    if mark == 0 as u_int {
        return;
    }
    if window_copy_search_mark_at(data, (*data).cx, cy, &raw mut cursor) == 0 as ::core::ffi::c_int
    {
        keys = options_get_number(
            &mode_pane_owner
                .window_observer()
                .upgrade()
                .expect("live window")
                .borrow_options_mut(),
            c"mode-keys",
        ) as ::core::ffi::c_int;
        if cursor != 0 as u_int && keys == MODEKEY_EMACS && (*data).searchdirection != 0 {
            if (&(*data).searchmark)[cursor.wrapping_sub(1 as u_int) as usize] as u_int == mark {
                cursor = cursor.wrapping_sub(1);
                found = 1 as ::core::ffi::c_int;
            }
        } else if (&(*data).searchmark)[cursor as usize] as u_int == mark {
            found = 1 as ::core::ffi::c_int;
        }
        if found != 0 {
            window_copy_match_start_end(data, cursor, &raw mut start, &raw mut end);
            if current >= start && current <= end {
                (*gc).attr = cgc.attr;
                if inv != 0 {
                    (*gc).fg = cgc.bg;
                    (*gc).bg = cgc.fg;
                } else {
                    (*gc).fg = cgc.fg;
                    (*gc).bg = cgc.bg;
                }
                return;
            }
        }
    }
    (*gc).attr = mgc.attr;
    if inv != 0 {
        (*gc).fg = mgc.bg;
        (*gc).bg = mgc.fg;
    } else {
        (*gc).fg = mgc.fg;
        (*gc).bg = mgc.bg;
    };
}
unsafe fn window_copy_write_one(
    mut wme: refbox::Weak<window_mode_entry>,
    mut ctx: *mut screen_write_ctx,
    mut px: u_int,
    mut py: u_int,
    mut fy: u_int,
    mut nx: u_int,
    mgc: &grid_cell,
    cgc: &grid_cell,
    mkgc: &grid_cell,
    clgc: &grid_cell,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
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
    let mut fx: u_int = 0;
    let mut i: u_int = 0;
    let mut width: u_int = 0;
    screen_write_cursormove(
        &mut *ctx,
        px as ::core::ffi::c_int,
        py as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    fx = 0 as u_int;
    while fx < nx {
        grid_get_cell(&*gd, fx, fy, &mut gc);
        if fx.wrapping_add(gc.data.width as u_int) <= nx {
            window_copy_update_style(wme.clone(), fx, fy, &raw mut gc, mgc, cgc, mkgc, clgc);
            if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
                if (*(*ctx).screen_ptr()).cy == py
                    && (*(*ctx).screen_ptr()).cx <= px.wrapping_add(fx)
                {
                    gc.flags = (gc.flags as ::core::ffi::c_int & !GRID_FLAG_PADDING) as u_char;
                    screen_write_cursormove(
                        &mut *ctx,
                        px.wrapping_add(fx) as ::core::ffi::c_int,
                        py as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    screen_write_putc(&mut *ctx, &gc, ' ' as i32 as u_char);
                }
            } else if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
                width = gc.data.width as u_int;
                gc.flags = (gc.flags as ::core::ffi::c_int & !GRID_FLAG_TAB) as u_char;
                i = 0 as u_int;
                while i < width {
                    screen_write_putc(&mut *ctx, &gc, ' ' as i32 as u_char);
                    i = i.wrapping_add(1);
                }
            } else {
                screen_write_cell(&mut *ctx, &gc);
            }
        } else {
            screen_write_putc(&mut *ctx, &grid_default_cell, ' ' as i32 as u_char);
        }
        fx = fx.wrapping_add(1);
    }
}
unsafe fn window_copy_line_number_mode(
    mut wme: refbox::Weak<window_mode_entry>,
) -> ::core::ffi::c_int {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let mut mode: ::core::ffi::c_int = 0;
    if (*data).line_numbers == 0 {
        return WINDOW_COPY_LINE_NUMBERS_OFF as ::core::ffi::c_int;
    }
    mode = options_get_number(
        &options_window
            .upgrade()
            .expect("live copy-mode window")
            .borrow_options_mut(),
        c"copy-mode-line-numbers",
    ) as ::core::ffi::c_int;
    if (*data).line_numbers == 2 as ::core::ffi::c_int
        && mode == WINDOW_COPY_LINE_NUMBERS_OFF as ::core::ffi::c_int
    {
        return WINDOW_COPY_LINE_NUMBERS_DEFAULT as ::core::ffi::c_int;
    }
    mode
}
unsafe fn window_copy_line_number_is_absolute(
    mut wme: refbox::Weak<window_mode_entry>,
) -> ::core::ffi::c_int {
    match window_copy_line_number_mode(wme.clone()) {
        2..=4 => return 1 as ::core::ffi::c_int,
        0 | 1 => return 0 as ::core::ffi::c_int,
        _ => {}
    }
    fatalx(|out| out.write_all(b"bad line number mode"));
}
unsafe fn window_copy_line_numbers_active(
    mut wme: refbox::Weak<window_mode_entry>,
) -> ::core::ffi::c_int {
    (window_copy_line_number_mode(wme.clone())
        != WINDOW_COPY_LINE_NUMBERS_OFF as ::core::ffi::c_int) as ::core::ffi::c_int
}
unsafe fn window_copy_cursor_line_active(
    mut wme: refbox::Weak<window_mode_entry>,
) -> ::core::ffi::c_int {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let options_window = mode_pane_owner.window_observer();
    let window = options_window.upgrade().expect("live copy-mode window");
    let style = options_get_string(
        &window.borrow_options_mut(),
        c"copy-mode-current-line-style",
    );
    (style.as_c_str() != c"default") as ::core::ffi::c_int
}
unsafe fn window_copy_line_number_width(mut wme: refbox::Weak<window_mode_entry>) -> u_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut lines: u_int = 0;
    let mut digits: u_int = 0;
    if window_copy_line_numbers_active(wme.clone()) == 0 {
        return 0 as u_int;
    }
    lines = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).backing().grid().sy)
        .wrapping_add(1 as u_int);
    digits = 1 as u_int;
    while lines >= 10 as u_int {
        lines = lines.wrapping_div(10 as u_int);
        digits = digits.wrapping_add(1);
    }
    if digits < 3 as u_int {
        digits = 3 as u_int;
    }
    digits.wrapping_add(1 as u_int)
}
unsafe fn window_copy_cursor_offset(
    mut wme: refbox::Weak<window_mode_entry>,
    mut cx: u_int,
    mut sx: u_int,
) -> u_int {
    let mut width: u_int = window_copy_line_number_width(wme.clone());
    let mut content: u_int = 0;
    if width == 0 as u_int {
        return cx;
    }
    if width >= sx {
        content = 1 as u_int;
    } else {
        content = sx.wrapping_sub(width);
    }
    if cx >= content {
        return sx.wrapping_sub(1 as u_int);
    }
    width.wrapping_add(cx)
}
unsafe fn window_copy_cursor_unoffset(
    mut wme: refbox::Weak<window_mode_entry>,
    mut vx: u_int,
    mut sx: u_int,
) -> u_int {
    let mut width: u_int = window_copy_line_number_width(wme.clone());
    let mut content: u_int = 0;
    if width == 0 as u_int {
        return vx;
    }
    if width >= sx {
        content = 1 as u_int;
    } else {
        content = sx.wrapping_sub(width);
    }
    if vx < width {
        return 0 as u_int;
    }
    vx = vx.wrapping_sub(width);
    if vx >= content {
        return content.wrapping_sub(1 as u_int);
    }
    vx
}
pub unsafe fn window_copy_set_line_numbers(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut enabled: ::core::ffi::c_int,
) {
    let mut wme: refbox::Weak<window_mode_entry> = pane_owner.mode_entry();
    if !wme.is_alive() || !std::ptr::eq(wme.get_unchecked().mode, &window_copy_mode) {
        return;
    }
    window_copy_set_line_numbers1(wme.clone(), enabled, 0 as ::core::ffi::c_int);
}
unsafe fn window_copy_set_line_numbers1(
    mut wme: refbox::Weak<window_mode_entry>,
    mut enabled: ::core::ffi::c_int,
    mut force: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let mut active: ::core::ffi::c_int = 0;
    let mut line_numbers: ::core::ffi::c_int = 0;
    if data.is_null() {
        return;
    }
    active = window_copy_line_numbers_active(wme.clone());
    if enabled == 0 {
        line_numbers = 0 as ::core::ffi::c_int;
    } else if force != 0
        && options_get_number(
            &options_window
                .upgrade()
                .expect("live copy-mode window")
                .borrow_options_mut(),
            c"copy-mode-line-numbers",
        ) == WINDOW_COPY_LINE_NUMBERS_OFF as ::core::ffi::c_int as ::core::ffi::c_longlong
    {
        line_numbers = 2 as ::core::ffi::c_int;
    } else {
        line_numbers = 1 as ::core::ffi::c_int;
    }
    if (*data).line_numbers == line_numbers && active == enabled {
        return;
    }
    (*data).line_numbers = line_numbers;
    window_copy_redraw_screen(wme.clone());
}
pub unsafe fn window_copy_get_current_offset(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) -> Option<(u_int, u_int)> {
    let entry = pane.mode_entry();
    if !entry.is_alive() {
        return None;
    }
    let mode = entry
        .try_borrow_mut()
        .expect("active copy mode already borrowed");
    if !std::ptr::eq(mode.mode, &window_copy_mode) && !std::ptr::eq(mode.mode, &window_view_mode) {
        return None;
    }
    let data = mode.boxed_data_ptr::<window_copy_mode_data>()?.as_ref()?;
    let hsize = data.backing().grid().hsize;
    Some((hsize.wrapping_sub(data.oy), hsize))
}

unsafe fn window_copy_write_line(
    mut wme: refbox::Weak<window_mode_entry>,
    mut ctx: *mut screen_write_ctx,
    mut py: u_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let options_window = mode_pane_owner.window_observer();
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
    let mut mgc: grid_cell = grid_cell {
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
    let mut cgc: grid_cell = grid_cell {
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
    let mut mkgc: grid_cell = grid_cell {
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
    let mut clgc: grid_cell = grid_cell {
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
    let mut ln_gc: grid_cell = grid_cell {
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
    let mut cur_ln_gc: grid_cell = grid_cell {
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
    let mut sx: u_int = (*s).grid().sx;
    let mut hsize: u_int = (*data).backing().grid().hsize;
    let mut width: u_int = 0;
    let mut absolute: u_int = 0;
    let mut line_number: u_int = 0;
    let mut content_sx: u_int = 0;
    let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut current: ::core::ffi::c_int = 0;
    let mut mode: ::core::ffi::c_int = 0;
    width = window_copy_line_number_width(wme.clone());
    if width >= sx {
        content_sx = 1 as u_int;
    } else if width != 0 as u_int {
        content_sx = sx.wrapping_sub(width);
    } else {
        content_sx = sx;
    }
    screen_write_cursormove(
        &mut *ctx,
        0 as ::core::ffi::c_int,
        py as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let mut ft_owner = format_create_defaults(
        None,
        None,
        None,
        (refbox::Weak::new()).clone(),
        Some(&mode_pane_owner),
    );
    ft = &raw mut *ft_owner;
    style_apply_with_options(
        &mut gc,
        c"copy-mode-position-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    gc.flags = (gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    style_apply_with_options(
        &mut mgc,
        c"copy-mode-match-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    mgc.flags = (mgc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    style_apply_with_options(
        &mut cgc,
        c"copy-mode-current-match-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    cgc.flags = (cgc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    style_apply_with_options(
        &mut mkgc,
        c"copy-mode-mark-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    mkgc.flags = (mkgc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    style_apply_with_options(
        &mut clgc,
        c"copy-mode-current-line-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    clgc.flags = (clgc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    if width != 0 as u_int {
        style_apply_with_options(
            &mut ln_gc,
            c"copy-mode-line-number-style",
            Some(&mut *ft),
            &OptionsScope::Window(options_window.clone()),
        );
        ln_gc.flags = (ln_gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
        style_apply_with_options(
            &mut cur_ln_gc,
            c"copy-mode-current-line-number-style",
            Some(&mut *ft),
            &OptionsScope::Window(options_window.clone()),
        );
        cur_ln_gc.flags = (cur_ln_gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
        current = (py == (*data).cy) as ::core::ffi::c_int;
        absolute = hsize
            .wrapping_sub((*data).oy)
            .wrapping_add(py)
            .wrapping_add(1 as u_int);
        mode = window_copy_line_number_mode(wme.clone());
        if mode == WINDOW_COPY_LINE_NUMBERS_DEFAULT as ::core::ffi::c_int {
            if py < (*data).oy {
                line_number = (*data).oy.wrapping_sub(py);
            } else {
                line_number = py.wrapping_sub((*data).oy);
            }
        } else if mode == WINDOW_COPY_LINE_NUMBERS_ABSOLUTE as ::core::ffi::c_int
            || (mode == WINDOW_COPY_LINE_NUMBERS_HYBRID as ::core::ffi::c_int && current != 0)
        {
            line_number = absolute;
        } else if py > (*data).cy {
            line_number = py.wrapping_sub((*data).cy);
        } else {
            line_number = (*data).cy.wrapping_sub(py);
        }
        screen_write_cursormove(
            &mut *ctx,
            0 as ::core::ffi::c_int,
            py as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_nputs(
            &mut *ctx,
            width as ssize_t,
            &*if current != 0 {
                &raw mut cur_ln_gc
            } else {
                &raw mut ln_gc
            },
            |out| {
                let width = (width as ::core::ffi::c_int - 1 as ::core::ffi::c_int) as i32;
                if width < 0 {
                    write!(
                        out,
                        "{:<width$}",
                        (line_number) as u32,
                        width = width.unsigned_abs() as usize
                    )
                } else {
                    write!(
                        out,
                        "{:>width$}",
                        (line_number) as u32,
                        width = width as usize
                    )
                }?;
                out.write_all(b" ")
            },
        );
    }
    window_copy_write_one(
        wme.clone(),
        ctx,
        width,
        py,
        hsize.wrapping_sub((*data).oy).wrapping_add(py),
        content_sx,
        &mgc,
        &cgc,
        &mkgc,
        &clgc,
    );
    if py == 0 as u_int && (*s).rupper < (*s).rlower && (*data).hide_position == 0 {
        let position_format = options_get_string(
            &options_window
                .upgrade()
                .expect("live copy-mode window")
                .borrow_options_mut(),
            c"copy-mode-position-format",
        );
        value = position_format.as_ptr();
        if *value as ::core::ffi::c_int != '\0' as i32 {
            let expanded = format_expand_cstring(ft, value);
            if !expanded.is_empty() {
                screen_write_cursormove(
                    &mut *ctx,
                    width as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                format_draw(
                    ctx,
                    &gc,
                    content_sx,
                    expanded.as_ptr(),
                    ::core::ptr::null_mut::<style_ranges>(),
                    0 as ::core::ffi::c_int,
                );
            }
        }
    }
    if py == (*data).cy && (*data).cx >= content_sx {
        screen_write_cursormove(
            &mut *ctx,
            window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx)
                as ::core::ffi::c_int,
            py as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_putc(&mut *ctx, &grid_default_cell, '$' as i32 as u_char);
    }
    drop(ft_owner);
}
unsafe fn window_copy_write_lines(
    mut wme: refbox::Weak<window_mode_entry>,
    mut ctx: *mut screen_write_ctx,
    mut py: u_int,
    mut ny: u_int,
) {
    let mut yy: u_int = 0;
    yy = py;
    while yy < py.wrapping_add(ny) {
        window_copy_write_line(wme.clone(), ctx, yy);
        yy = yy.wrapping_add(1);
    }
}
unsafe fn window_copy_redraw_selection(mut wme: refbox::Weak<window_mode_entry>, mut old_y: u_int) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
    let mut new_y: u_int = 0;
    let mut start: u_int = 0;
    let mut end: u_int = 0;
    new_y = (*data).cy;
    if old_y <= new_y {
        start = old_y;
        end = new_y;
    } else {
        start = new_y;
        end = old_y;
    }
    if (*data).selflag as ::core::ffi::c_uint
        == SEL_WORD as ::core::ffi::c_int as ::core::ffi::c_uint
        && end < (*gd).sy.wrapping_add((*data).oy).wrapping_sub(1 as u_int)
    {
        end = end.wrapping_add(1);
    }
    window_copy_redraw_lines(
        wme.clone(),
        start,
        end.wrapping_sub(start).wrapping_add(1 as u_int),
    );
}
unsafe fn window_copy_redraw_lines(
    mut wme: refbox::Weak<window_mode_entry>,
    mut py: u_int,
    mut ny: u_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut i: u_int = 0;
    if window_copy_line_number_width(wme.clone()) != 0 as u_int {
        screen_write_start(&mut ctx, &raw mut (*data).screen);
        i = py;
        while i < py.wrapping_add(ny) {
            window_copy_write_line(wme.clone(), &raw mut ctx, i);
            i = i.wrapping_add(1);
        }
        screen_write_cursormove(
            &mut ctx,
            window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx)
                as ::core::ffi::c_int,
            (*data).cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_stop(&mut ctx);
        mode_pane_owner.request_redraw(true);
        return;
    }
    let res = {
        let scrollbar = &mode_pane_owner;
        scrollbar.scrollbar_overlay() && scrollbar.scrollbar_visible()
    };
    if res {
        screen_write_start(&mut ctx, &raw mut (*data).screen);
    } else {
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
    }
    i = py;
    while i < py.wrapping_add(ny) {
        window_copy_write_line(wme.clone(), &raw mut ctx, i);
        i = i.wrapping_add(1);
    }
    screen_write_cursormove(
        &mut ctx,
        window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx) as ::core::ffi::c_int,
        (*data).cy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_stop(&mut ctx);
    mode_pane_owner.redraw_scrollbar();
}
unsafe fn window_copy_redraw_screen(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    window_copy_redraw_lines(wme.clone(), 0 as u_int, (*data).screen.grid().sy);
}
unsafe fn window_copy_style_changed(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    if !(*data).screen.sel.is_none() {
        window_copy_set_selection(
            wme.clone(),
            0 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_synchronize_cursor_end(
    mut wme: refbox::Weak<window_mode_entry>,
    mut begin: ::core::ffi::c_int,
    mut no_reset: ::core::ffi::c_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    xx = (*data).cx;
    yy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    match (*data).selflag as ::core::ffi::c_uint {
        1 => {
            if !(no_reset != 0) {
                begin = 0 as ::core::ffi::c_int;
                if (*data).dy > yy || (*data).dy == yy && (*data).dx > xx {
                    window_copy_cursor_previous_word_pos(
                        wme.clone(),
                        window_copy_separators(&*data).expect("word separators"),
                        &raw mut xx,
                        &raw mut yy,
                    );
                    begin = 1 as ::core::ffi::c_int;
                    (*data).endselx = (*data).endselrx;
                    (*data).endsely = (*data).endselry;
                } else {
                    if xx >= window_copy_find_length(wme.clone(), yy)
                        || window_copy_in_set(
                            wme.clone(),
                            xx.wrapping_add(1 as u_int),
                            yy,
                            WHITESPACE,
                        ) == 0
                    {
                        window_copy_cursor_next_word_end_pos(
                            wme.clone(),
                            window_copy_separators(&*data).expect("word separators"),
                            &raw mut xx,
                            &raw mut yy,
                        );
                    }
                    (*data).selx = (*data).selrx;
                    (*data).sely = (*data).selry;
                }
            }
        }
        2 if !(no_reset != 0) => {
            begin = 0 as ::core::ffi::c_int;
            if (*data).dy > yy {
                xx = 0 as u_int;
                begin = 1 as ::core::ffi::c_int;
                (*data).endselx = (*data).endselrx;
                (*data).endsely = (*data).endselry;
            } else {
                if yy < (*data).endselry {
                    yy = (*data).endselry;
                }
                xx = window_copy_find_length(wme.clone(), yy);
                (*data).selx = (*data).selrx;
                (*data).sely = (*data).selry;
            }
        }
        _ => {}
    }
    if begin != 0 {
        (*data).selx = xx;
        (*data).sely = yy;
    } else {
        (*data).endselx = xx;
        (*data).endsely = yy;
    };
}
unsafe fn window_copy_synchronize_cursor(
    mut wme: refbox::Weak<window_mode_entry>,
    mut no_reset: ::core::ffi::c_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    match (*data).cursordrag as ::core::ffi::c_uint {
        1 => {
            window_copy_synchronize_cursor_end(wme.clone(), 0 as ::core::ffi::c_int, no_reset);
        }
        2 => {
            window_copy_synchronize_cursor_end(wme.clone(), 1 as ::core::ffi::c_int, no_reset);
        }
        _ => {}
    };
}
unsafe fn window_copy_update_cursor(
    mut wme: refbox::Weak<window_mode_entry>,
    mut cx: u_int,
    mut cy: u_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut old_cx: u_int = 0;
    let mut old_cy: u_int = 0;
    let mut py: u_int = 0;
    let mut width: u_int = 0;
    let mut content_sx: u_int = 0;
    let mut maxx: u_int = 0;
    let mut allow_onemore: ::core::ffi::c_int = 0;
    if (*data).rectflag == 0 && cy < (*s).grid().sy {
        allow_onemore =
            (!(*data).screen.sel.is_none() && (*data).rectflag != 0) as ::core::ffi::c_int;
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add(cy)
            .wrapping_sub((*data).oy);
        maxx = window_copy_cursor_limit(wme.clone(), py, allow_onemore);
        if cx > maxx {
            cx = maxx;
        }
    }
    old_cx = (*data).cx;
    old_cy = (*data).cy;
    (*data).cx = cx;
    (*data).cy = cy;
    if window_copy_line_numbers_active(wme.clone()) != 0 {
        width = window_copy_line_number_width(wme.clone());
        if !(*s).sel.is_none()
            || (*data).lineflag as ::core::ffi::c_uint
                != LINE_SEL_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
            || old_cy != (*data).cy
        {
            window_copy_redraw_screen(wme.clone());
            return;
        }
        if width >= (*s).grid().sx {
            content_sx = 1 as u_int;
        } else {
            content_sx = (*s).grid().sx.wrapping_sub(width);
        }
        if old_cx >= content_sx || (*data).cx >= content_sx {
            window_copy_redraw_screen(wme.clone());
            return;
        }
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
        screen_write_cursormove(
            &mut ctx,
            window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx)
                as ::core::ffi::c_int,
            (*data).cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_stop(&mut ctx);
        return;
    }
    if old_cy != (*data).cy && window_copy_cursor_line_active(wme.clone()) != 0 {
        window_copy_redraw_lines(wme.clone(), old_cy, 1 as u_int);
        window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
        return;
    }
    if old_cx == (*s).grid().sx {
        window_copy_redraw_lines(wme.clone(), old_cy, 1 as u_int);
    }
    if (*data).cx == (*s).grid().sx {
        window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
    } else {
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
        screen_write_cursormove(
            &mut ctx,
            window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx)
                as ::core::ffi::c_int,
            (*data).cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_stop(&mut ctx);
    };
}
unsafe fn window_copy_start_selection(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    (*data).selx = (*data).cx;
    (*data).sely = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).endselx = (*data).selx;
    (*data).endsely = (*data).sely;
    (*data).cursordrag = CURSORDRAG_ENDSEL;
    window_copy_set_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
}
unsafe fn window_copy_mouse_in_selection(
    mut wme: refbox::Weak<window_mode_entry>,
    mut x: u_int,
    mut y: u_int,
    mut on_start: *mut ::core::ffi::c_int,
    mut on_end: *mut ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut sx: u_int = (*data).screen.grid().sx;
    let mut sy: u_int = (*data).screen.grid().sy;
    let mut hsize: u_int = 0;
    let mut screeny: u_int = 0;
    let mut mx: u_int = 0;
    let mut my: u_int = 0;
    let mut selx: u_int = 0;
    let mut sely: u_int = 0;
    let mut endselx: u_int = 0;
    let mut endsely: u_int = 0;
    let mut cursorx: u_int = 0;
    let mut cursory: u_int = 0;
    let mut mpos: ::core::ffi::c_longlong = 0;
    let mut spos: ::core::ffi::c_longlong = 0;
    let mut epos: ::core::ffi::c_longlong = 0;
    let mut dstart: ::core::ffi::c_longlong = 0;
    let mut dend: ::core::ffi::c_longlong = 0;
    if !on_start.is_null() {
        *on_start = 0 as ::core::ffi::c_int;
    }
    if !on_end.is_null() {
        *on_end = 0 as ::core::ffi::c_int;
    }
    if (*data).screen.sel.is_none() {
        return 0 as ::core::ffi::c_int;
    }
    hsize = (*data).backing().grid().hsize;
    screeny = hsize.wrapping_sub((*data).oy);
    selx = window_copy_cursor_offset(wme.clone(), (*data).selx, sx);
    sely = (*data).sely.wrapping_sub(screeny);
    if (*data).sely >= screeny && sely < sy && x == selx && y == sely {
        if !on_start.is_null() {
            *on_start = 1 as ::core::ffi::c_int;
        }
        return 1 as ::core::ffi::c_int;
    }
    endselx = window_copy_cursor_offset(wme.clone(), (*data).endselx, sx);
    endsely = (*data).endsely.wrapping_sub(screeny);
    if (*data).endsely >= screeny && endsely < sy && x == endselx && y == endsely {
        if !on_end.is_null() {
            *on_end = 1 as ::core::ffi::c_int;
        }
        return 1 as ::core::ffi::c_int;
    }
    cursorx = window_copy_cursor_offset(wme.clone(), (*data).cx, sx);
    cursory = (*data).cy;
    if (x != cursorx || y != cursory) && screen_check_selection(&(*data).screen, x, y) == 0 {
        return 0 as ::core::ffi::c_int;
    }
    if !on_start.is_null() || !on_end.is_null() {
        mx = window_copy_cursor_unoffset(wme.clone(), x, sx);
        my = screeny.wrapping_add(y);
        mpos = my as ::core::ffi::c_longlong
            * sx.wrapping_add(1 as u_int) as ::core::ffi::c_longlong
            + mx as ::core::ffi::c_longlong;
        spos = (*data).sely as ::core::ffi::c_longlong
            * sx.wrapping_add(1 as u_int) as ::core::ffi::c_longlong
            + (*data).selx as ::core::ffi::c_longlong;
        epos = (*data).endsely as ::core::ffi::c_longlong
            * sx.wrapping_add(1 as u_int) as ::core::ffi::c_longlong
            + (*data).endselx as ::core::ffi::c_longlong;
        dstart = llabs(mpos - spos);
        dend = llabs(mpos - epos);
        if dstart <= dend {
            if !on_start.is_null() {
                *on_start = 1 as ::core::ffi::c_int;
            }
        } else if !on_end.is_null() {
            *on_end = 1 as ::core::ffi::c_int;
        }
    }
    1 as ::core::ffi::c_int
}
unsafe fn window_copy_adjust_selection(
    mut wme: refbox::Weak<window_mode_entry>,
    mut selx: *mut u_int,
    mut sely: *mut u_int,
) -> ::core::ffi::c_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut ty: u_int = 0;
    let mut relpos: ::core::ffi::c_int = 0;
    sx = *selx;
    sy = *sely;
    ty = (*data).backing().grid().hsize.wrapping_sub((*data).oy);
    if sy < ty {
        relpos = WINDOW_COPY_REL_POS_ABOVE as ::core::ffi::c_int;
        if (*data).rectflag == 0 {
            sx = 0 as u_int;
        }
        sy = 0 as u_int;
    } else if sy > ty.wrapping_add((*s).grid().sy).wrapping_sub(1 as u_int) {
        relpos = WINDOW_COPY_REL_POS_BELOW as ::core::ffi::c_int;
        if (*data).rectflag == 0 {
            sx = (*s).grid().sx.wrapping_sub(1 as u_int);
        }
        sy = (*s).grid().sy.wrapping_sub(1 as u_int);
    } else {
        relpos = WINDOW_COPY_REL_POS_ON_SCREEN as ::core::ffi::c_int;
        sy = sy.wrapping_sub(ty);
    }
    *selx = sx;
    *sely = sy;
    relpos
}
unsafe fn window_copy_update_selection(
    mut wme: refbox::Weak<window_mode_entry>,
    mut may_redraw: ::core::ffi::c_int,
    mut no_reset: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    if (*s).sel.is_none()
        && (*data).lineflag as ::core::ffi::c_uint
            == LINE_SEL_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return 0 as ::core::ffi::c_int;
    }
    window_copy_set_selection(wme.clone(), may_redraw, no_reset)
}
unsafe fn window_copy_update_selection_view(
    mut wme: refbox::Weak<window_mode_entry>,
    mut may_redraw: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut no_reset: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut selx: u_int = 0;
    let mut sely: u_int = 0;
    let mut endselx: u_int = 0;
    let mut endsely: u_int = 0;
    let mut changed: ::core::ffi::c_int = 0;
    if (*data).cursordrag as ::core::ffi::c_uint
        != CURSORDRAG_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return window_copy_update_selection(wme.clone(), may_redraw, no_reset);
    }
    selx = (*data).selx;
    sely = (*data).sely;
    endselx = (*data).endselx;
    endsely = (*data).endsely;
    changed = window_copy_update_selection(wme.clone(), may_redraw, 1 as ::core::ffi::c_int);
    (*data).selx = selx;
    (*data).sely = sely;
    (*data).endselx = endselx;
    (*data).endsely = endsely;
    changed
}
unsafe fn window_copy_set_selection(
    mut wme: refbox::Weak<window_mode_entry>,
    mut may_redraw: ::core::ffi::c_int,
    mut no_reset: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let options_window = mode_pane_owner.window_observer();
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
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut cy: u_int = 0;
    let mut endsx: u_int = 0;
    let mut endsy: u_int = 0;
    let mut clipx: u_int = 0;
    let mut startrelpos: ::core::ffi::c_int = 0;
    let mut endrelpos: ::core::ffi::c_int = 0;
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    window_copy_synchronize_cursor(wme.clone(), no_reset);
    sx = (*data).selx;
    sy = (*data).sely;
    startrelpos = window_copy_adjust_selection(wme.clone(), &raw mut sx, &raw mut sy);
    endsx = (*data).endselx;
    endsy = (*data).endsely;
    endrelpos = window_copy_adjust_selection(wme.clone(), &raw mut endsx, &raw mut endsy);
    if startrelpos == endrelpos
        && startrelpos != WINDOW_COPY_REL_POS_ON_SCREEN as ::core::ffi::c_int
    {
        screen_hide_selection(&mut *s);
        return 0 as ::core::ffi::c_int;
    }
    let mut ft_owner = format_create_defaults(
        None,
        None,
        None,
        (refbox::Weak::new()).clone(),
        Some(&mode_pane_owner),
    );
    ft = &raw mut *ft_owner;
    style_apply_with_options(
        &mut gc,
        c"copy-mode-selection-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    gc.flags = (gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    drop(ft_owner);
    clipx = window_copy_line_number_width(wme.clone());
    if clipx >= (*s).grid().sx {
        clipx = (*s).grid().sx.wrapping_sub(1 as u_int);
    }
    if window_copy_line_numbers_active(wme.clone()) != 0 {
        sx = window_copy_cursor_offset(wme.clone(), sx, (*s).grid().sx);
        endsx = window_copy_cursor_offset(wme.clone(), endsx, (*s).grid().sx);
    }
    screen_set_selection(
        &mut *s,
        sx,
        sy,
        endsx,
        endsy,
        (*data).rectflag as u_int,
        clipx,
        (*data).modekeys,
        &gc,
    );
    if (*data).rectflag != 0 && may_redraw != 0 {
        cy = (*data).cy;
        if (*data).cursordrag as ::core::ffi::c_uint
            == CURSORDRAG_ENDSEL as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if sy < cy {
                window_copy_redraw_lines(
                    wme.clone(),
                    sy,
                    cy.wrapping_sub(sy).wrapping_add(1 as u_int),
                );
            } else {
                window_copy_redraw_lines(
                    wme.clone(),
                    cy,
                    sy.wrapping_sub(cy).wrapping_add(1 as u_int),
                );
            }
        } else if endsy < cy {
            window_copy_redraw_lines(
                wme.clone(),
                endsy,
                cy.wrapping_sub(endsy).wrapping_add(1 as u_int),
            );
        } else {
            window_copy_redraw_lines(
                wme.clone(),
                cy,
                endsy.wrapping_sub(cy).wrapping_add(1 as u_int),
            );
        }
    }
    1 as ::core::ffi::c_int
}
unsafe fn window_copy_get_selection(mut wme: refbox::Weak<window_mode_entry>) -> Option<Vec<u8>> {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut buf = Vec::<u8>::new();
    let mut i: u_int = 0;
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut ex: u_int = 0;
    let mut ey: u_int = 0;
    let mut ey_last: u_int = 0;
    let mut firstsx: u_int = 0;
    let mut lastex: u_int = 0;
    let mut restex: u_int = 0;
    let mut restsx: u_int = 0;
    let mut selx: u_int = 0;
    let mut keys: ::core::ffi::c_int = 0;
    if (*data).screen.sel.is_none()
        && (*data).lineflag as ::core::ffi::c_uint
            == LINE_SEL_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return window_copy_match_at_cursor_bytes(data).map(|mut matched| {
            if let Some(first_nul) = matched.iter().position(|&byte| byte == 0) {
                matched.truncate(first_nul);
            }
            matched
        });
    }
    xx = (*data).endselx;
    yy = (*data).endsely;
    if yy < (*data).sely || yy == (*data).sely && xx < (*data).selx {
        sx = xx;
        sy = yy;
        ex = (*data).selx;
        ey = (*data).sely;
    } else {
        sx = (*data).selx;
        sy = (*data).sely;
        ex = xx;
        ey = yy;
    }
    ey_last = window_copy_find_length(wme.clone(), ey);
    if ex > ey_last {
        ex = ey_last;
    }
    xx = (*s).grid().sx;
    keys = options_get_number(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live window")
            .borrow_options_mut(),
        c"mode-keys",
    ) as ::core::ffi::c_int;
    if (*data).rectflag != 0 {
        if (*data).cursordrag as ::core::ffi::c_uint
            == CURSORDRAG_ENDSEL as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            selx = (*data).selx;
        } else {
            selx = (*data).endselx;
        }
        if selx < (*data).cx {
            if keys == MODEKEY_EMACS {
                lastex = (*data).cx;
                restex = (*data).cx;
            } else {
                lastex = (*data).cx.wrapping_add(1 as u_int);
                restex = (*data).cx.wrapping_add(1 as u_int);
            }
            firstsx = selx;
            restsx = selx;
        } else {
            lastex = selx.wrapping_add(1 as u_int);
            restex = selx.wrapping_add(1 as u_int);
            firstsx = (*data).cx;
            restsx = (*data).cx;
        }
    } else {
        if keys == MODEKEY_EMACS {
            lastex = ex;
        } else {
            lastex = ex.wrapping_add(1 as u_int);
        }
        restex = xx;
        firstsx = sx;
        restsx = 0 as u_int;
    }
    i = sy;
    while i <= ey {
        window_copy_copy_line(
            wme.clone(),
            &mut buf,
            i,
            if i == sy { firstsx } else { restsx },
            if i == ey { lastex } else { restex },
        );
        i = i.wrapping_add(1);
    }
    if buf.is_empty() {
        return None;
    }
    if (keys == MODEKEY_EMACS || lastex <= ey_last)
        && (!(grid_get_line((*data).backing().grid(), ey).flags as ::core::ffi::c_int)
            & GRID_LINE_WRAPPED
            != 0
            || lastex != ey_last)
    {
        buf.pop();
    }
    Some(buf)
}
unsafe fn window_copy_copy_buffer(
    mut wme: refbox::Weak<window_mode_entry>,
    prefix: Option<&CStr>,
    buf: Vec<u8>,
    mut set_paste: ::core::ffi::c_int,
    mut set_clip: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let prefix = prefix
        .filter(|_| set_paste != 0 && !buf.is_empty())
        .map(CStr::to_owned);
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut redraw: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if set_clip != 0
        && with_global_options(|oo| options_get_number(oo, c"set-clipboard"))
            != 0 as ::core::ffi::c_longlong
    {
        if window_copy_line_numbers_active(wme.clone()) != 0 && mode_pane_owner.needs_redraw(false)
        {
            redraw = PANE_REDRAW;
            mode_pane_owner.take_pending_redraw();
        }
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
        screen_write_setselection(&mut ctx, c"", &buf);
        screen_write_stop(&mut ctx);
        if redraw != 0 {
            mode_pane_owner.request_redraw(false);
        }
        events_fire_pane(c"pane-set-clipboard", mode_pane_owner.clone());
    }
    if set_paste != 0 {
        paste_add_owned(prefix, buf.into_boxed_slice());
    };
}
unsafe fn window_copy_pipe_run(
    mut wme: refbox::Weak<window_mode_entry>,
    s_owner: Option<&SessionRef>,
    cmd: Option<&CStr>,
) -> Option<Vec<u8>> {
    let mut job = refbox::Weak::new();
    let buf = window_copy_get_selection(wme.clone());
    let command_value;
    let mut cmd = cmd.filter(|cmd| !cmd.is_empty());
    if cmd.is_none() {
        command_value = with_global_options(|oo| {
            crate::src::options::options_get_string_optional(oo, c"copy-command")
        });
        cmd = command_value.as_deref();
    }
    if let Some(cmd) = cmd.filter(|cmd| !cmd.is_empty()) {
        job = job_run(
            Some(cmd),
            &Vec::new(),
            None,
            s_owner,
            None,
            None,
            None,
            None,
            JOB_NOWAIT,
            -(1 as ::core::ffi::c_int),
            -(1 as ::core::ffi::c_int),
        );
        if !job.is_empty() {
            bufferevent_write(job_get_event(&job), buf.as_deref().unwrap_or_default());
        }
    }
    buf
}
unsafe fn window_copy_pipe(
    mut wme: refbox::Weak<window_mode_entry>,
    s_owner: Option<&SessionRef>,
    cmd: Option<&CStr>,
) {
    let _ = window_copy_pipe_run(wme.clone(), s_owner, cmd);
}
unsafe fn window_copy_copy_pipe(
    mut wme: refbox::Weak<window_mode_entry>,
    s_owner: Option<&SessionRef>,
    prefix: Option<&CStr>,
    cmd: Option<&CStr>,
    mut set_paste: ::core::ffi::c_int,
    mut set_clip: ::core::ffi::c_int,
) {
    if let Some(buf) = window_copy_pipe_run(wme.clone(), s_owner, cmd) {
        window_copy_copy_buffer(wme.clone(), prefix, buf, set_paste, set_clip);
    }
}
unsafe fn window_copy_copy_selection(
    mut wme: refbox::Weak<window_mode_entry>,
    prefix: Option<&CStr>,
    mut set_paste: ::core::ffi::c_int,
    mut set_clip: ::core::ffi::c_int,
) {
    if let Some(buf) = window_copy_get_selection(wme.clone()) {
        window_copy_copy_buffer(wme.clone(), prefix, buf, set_paste, set_clip);
    }
}
unsafe fn window_copy_append_selection(mut wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut buf: Vec<u8>;
    let mut bufname: Option<CString> = None;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    buf = match window_copy_get_selection(wme.clone()) {
        Some(buf) => buf,
        None => return,
    };
    if with_global_options(|oo| options_get_number(oo, c"set-clipboard"))
        != 0 as ::core::ffi::c_longlong
    {
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
        screen_write_setselection(&mut ctx, c"", &buf);
        screen_write_stop(&mut ctx);
        events_fire_pane(c"pane-set-clipboard", mode_pane_owner.clone());
    }
    if let Some(pb) = paste_get_top(Some(&mut bufname)) {
        let buffer = pb.borrow();
        let bufdata = paste_buffer_data(&buffer).unwrap_or_default();
        let mut appended = bufdata.to_vec();
        appended.extend_from_slice(&buf);
        buf = appended;
    }
    let _ = paste_set_owned(buf.into_boxed_slice(), bufname.as_deref(), None);
}
unsafe fn window_copy_copy_line(
    mut wme: refbox::Weak<window_mode_entry>,
    buf: &mut Vec<u8>,
    mut sy: u_int,
    mut sx: u_int,
    mut ex: u_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut gd: *mut grid = (*data).backing_mut().grid_mut();
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
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut i: u_int = 0;
    let mut xx: u_int = 0;
    let mut wrapped: u_int = 0 as u_int;
    if sx > ex {
        return;
    }
    let gl = grid_get_line(&*gd, sy);
    if gl.flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0 && gl.cellsize as u_int <= (*gd).sx {
        wrapped = 1 as u_int;
    }
    if wrapped != 0 {
        xx = gl.cellsize as u_int;
    } else {
        xx = window_copy_find_length(wme.clone(), sy);
    }
    if ex > xx {
        ex = xx;
    }
    if sx > xx {
        sx = xx;
    }
    if sx < ex {
        i = sx;
        while i < ex {
            grid_get_cell(&*gd, i, sy, &mut gc);
            if !(gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0) {
                if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
                    utf8_set(&mut ud, '\t' as i32 as u_char);
                } else {
                    ud = utf8_copy(&gc.data);
                }
                if ud.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                    && gc.attr as ::core::ffi::c_int & GRID_ATTR_CHARSET != 0
                {
                    if let Some(acs) = tty_acs_get(None, true, ud.data[0]) {
                        let bytes = acs.to_bytes();
                        if bytes.len() <= ud.data.len() {
                            ud.size = bytes.len() as u_char;
                            ud.data[..bytes.len()].copy_from_slice(bytes);
                        }
                    }
                }
                buf.extend_from_slice(&ud.data[..ud.size as usize]);
            }
            i = i.wrapping_add(1);
        }
    }
    if wrapped == 0 || ex != xx {
        buf.push(b'\n');
    }
}
unsafe fn window_copy_clear_selection(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    screen_clear_selection(&mut (*data).screen);
    (*data).cursordrag = CURSORDRAG_NONE;
    (*data).lineflag = LINE_SEL_NONE;
    (*data).selflag = SEL_CHAR;
    py = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    px = window_copy_cursor_limit(wme.clone(), py, (*data).rectflag);
    if (*data).cx > px {
        window_copy_update_cursor(wme.clone(), px, (*data).cy);
    }
}
unsafe fn window_copy_in_set(
    mut wme: refbox::Weak<window_mode_entry>,
    mut px: u_int,
    mut py: u_int,
    set: &CStr,
) -> ::core::ffi::c_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    grid_in_set((*data).backing().grid(), px, py, set)
}
unsafe fn window_copy_find_length(
    mut wme: refbox::Weak<window_mode_entry>,
    mut py: u_int,
) -> u_int {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    grid_line_length((*data).backing().grid(), py)
}
unsafe fn window_copy_cursor_limit(
    mut wme: refbox::Weak<window_mode_entry>,
    mut py: u_int,
    mut allow_onemore: ::core::ffi::c_int,
) -> u_int {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    if allow_onemore != 0
        || options_get_number(
            &options_window
                .upgrade()
                .expect("live copy-mode window")
                .borrow_options_mut(),
            c"mode-keys",
        ) != MODEKEY_VI as ::core::ffi::c_longlong
    {
        return window_copy_find_length(wme.clone(), py);
    }
    grid_line_limit((*data).backing().grid(), py)
}
unsafe fn window_copy_cursor_start_of_line(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_start_of_line(&mut gr, 1 as ::core::ffi::c_int);
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_up(wme.clone(), hsize, (*data).oy, oldy, px, py);
}
unsafe fn window_copy_cursor_back_to_indentation(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_back_to_indentation(&mut gr);
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_up(wme.clone(), hsize, (*data).oy, oldy, px, py);
}
unsafe fn window_copy_cursor_end_of_line(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    if !(*data).screen.sel.is_none() && (*data).rectflag != 0 {
        grid_reader_cursor_end_of_line(&mut gr, 1 as ::core::ffi::c_int, 1 as ::core::ffi::c_int);
    } else {
        grid_reader_cursor_end_of_line(&mut gr, 1 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
    }
    (px, py) = grid_reader_get_cursor(&gr);
    if (*data).screen.sel.is_none() || (*data).rectflag == 0 {
        px = window_copy_cursor_limit(wme.clone(), py, 0 as ::core::ffi::c_int);
    }
    window_copy_acquire_cursor_down(
        wme.clone(),
        hsize,
        (*back_s).grid().sy,
        (*data).oy,
        oldy,
        px,
        py,
        0 as ::core::ffi::c_int,
    );
}
unsafe fn window_copy_other_end(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut selx: u_int = 0;
    let mut sely: u_int = 0;
    let mut cy: u_int = 0;
    let mut yy: u_int = 0;
    let mut hsize: u_int = 0;
    if (*s).sel.is_none()
        && (*data).lineflag as ::core::ffi::c_uint
            == LINE_SEL_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return;
    }
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_LEFT_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        (*data).lineflag = LINE_SEL_RIGHT_LEFT;
    } else if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_RIGHT_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        (*data).lineflag = LINE_SEL_LEFT_RIGHT;
    }
    match (*data).cursordrag as ::core::ffi::c_uint {
        0 | 2 => {
            (*data).cursordrag = CURSORDRAG_ENDSEL;
        }
        1 => {
            (*data).cursordrag = CURSORDRAG_SEL;
        }
        _ => {}
    }
    selx = (*data).endselx;
    sely = (*data).endsely;
    if (*data).cursordrag as ::core::ffi::c_uint
        == CURSORDRAG_SEL as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        selx = (*data).selx;
        sely = (*data).sely;
    }
    cy = (*data).cy;
    yy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).cx = selx;
    hsize = (*data).backing().grid().hsize;
    if sely < hsize.wrapping_sub((*data).oy) {
        (*data).oy = hsize.wrapping_sub(sely);
        (*data).cy = 0 as u_int;
    } else if sely > hsize.wrapping_sub((*data).oy).wrapping_add((*s).grid().sy) {
        (*data).oy = hsize
            .wrapping_sub(sely)
            .wrapping_add((*s).grid().sy)
            .wrapping_sub(1 as u_int);
        (*data).cy = (*s).grid().sy.wrapping_sub(1 as u_int);
    } else {
        (*data).cy = cy.wrapping_add(sely).wrapping_sub(yy);
    }
    yy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    hsize = window_copy_cursor_limit(wme.clone(), yy, (*data).rectflag);
    if (*data).cx > hsize {
        (*data).cx = hsize;
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_cursor_left(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_left(&mut gr, 1 as ::core::ffi::c_int);
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_up(wme.clone(), hsize, (*data).oy, oldy, px, py);
}
unsafe fn window_copy_cursor_right(
    mut wme: refbox::Weak<window_mode_entry>,
    mut all: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    let mut onemore: ::core::ffi::c_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    onemore = (options_get_number(
        &options_window
            .upgrade()
            .expect("live copy-mode window")
            .borrow_options_mut(),
        c"mode-keys",
    ) != MODEKEY_VI as ::core::ffi::c_longlong) as ::core::ffi::c_int;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_right(&mut gr, 1 as ::core::ffi::c_int, all, onemore);
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_down(
        wme.clone(),
        hsize,
        (*back_s).grid().sy,
        (*data).oy,
        oldy,
        px,
        py,
        0 as ::core::ffi::c_int,
    );
}
unsafe fn window_copy_cursor_up(
    mut wme: refbox::Weak<window_mode_entry>,
    mut scroll_only: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut norectsel: ::core::ffi::c_int = 0;
    norectsel = ((*data).screen.sel.is_none() || (*data).rectflag == 0) as ::core::ffi::c_int;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    ox = window_copy_find_length(wme.clone(), oy);
    if norectsel != 0 && (*data).cx != ox {
        (*data).lastcx = (*data).cx;
        (*data).lastsx = ox;
    }
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_LEFT_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
        && oy == (*data).sely
    {
        window_copy_other_end(wme.clone());
    }
    if scroll_only != 0
        && options_get_number(
            &options_window
                .upgrade()
                .expect("live copy-mode window")
                .borrow_options_mut(),
            c"mode-keys",
        ) == MODEKEY_VI as ::core::ffi::c_longlong
        && (*data).cy < (*s).grid().sy.wrapping_sub(1 as u_int)
    {
        window_copy_update_cursor(wme.clone(), (*data).cx, (*data).cy.wrapping_add(1 as u_int));
    }
    if scroll_only != 0 || (*data).cy == 0 as u_int {
        if norectsel != 0 {
            (*data).cx = (*data).lastcx;
        }
        window_copy_scroll_down(wme.clone(), 1 as u_int);
        if scroll_only != 0 {
            if (*data).cy == (*s).grid().sy.wrapping_sub(1 as u_int) {
                window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
            } else {
                window_copy_redraw_lines(wme.clone(), (*data).cy, 2 as u_int);
            }
        }
    } else {
        if norectsel != 0 {
            window_copy_update_cursor(
                wme.clone(),
                (*data).lastcx,
                (*data).cy.wrapping_sub(1 as u_int),
            );
        } else {
            window_copy_update_cursor(wme.clone(), (*data).cx, (*data).cy.wrapping_sub(1 as u_int));
        }
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        ) != 0
        {
            if (*data).cy == (*s).grid().sy.wrapping_sub(1 as u_int) {
                window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
            } else {
                window_copy_redraw_lines(wme.clone(), (*data).cy, 2 as u_int);
            }
        }
    }
    if norectsel != 0 {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        px = window_copy_find_length(wme.clone(), py);
        if (*data).cx >= (*data).lastsx && (*data).cx != px || (*data).cx > px {
            window_copy_update_cursor(wme.clone(), px, (*data).cy);
            if window_copy_update_selection(
                wme.clone(),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) != 0
            {
                window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
            }
        }
    }
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_LEFT_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        if (*data).rectflag != 0 {
            px = (*data).backing().grid().sx;
        } else {
            px = window_copy_find_length(wme.clone(), py);
        }
        window_copy_update_cursor(wme.clone(), px, (*data).cy);
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        ) != 0
        {
            window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
        }
    } else if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_RIGHT_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        window_copy_update_cursor(wme.clone(), 0 as u_int, (*data).cy);
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        ) != 0
        {
            window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
        }
    }
}
unsafe fn window_copy_cursor_down(
    mut wme: refbox::Weak<window_mode_entry>,
    mut scroll_only: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ox: u_int = 0;
    let mut oy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut norectsel: ::core::ffi::c_int = 0;
    norectsel = ((*data).screen.sel.is_none() || (*data).rectflag == 0) as ::core::ffi::c_int;
    oy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    ox = window_copy_find_length(wme.clone(), oy);
    if norectsel != 0 && (*data).cx != ox {
        (*data).lastcx = (*data).cx;
        (*data).lastsx = ox;
    }
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_RIGHT_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
        && oy == (*data).endsely
    {
        window_copy_other_end(wme.clone());
    }
    if scroll_only != 0
        && options_get_number(
            &options_window
                .upgrade()
                .expect("live copy-mode window")
                .borrow_options_mut(),
            c"mode-keys",
        ) == MODEKEY_VI as ::core::ffi::c_longlong
        && (*data).cy > 0 as u_int
    {
        window_copy_update_cursor(wme.clone(), (*data).cx, (*data).cy.wrapping_sub(1 as u_int));
    }
    if scroll_only != 0 || (*data).cy == (*s).grid().sy.wrapping_sub(1 as u_int) {
        if norectsel != 0 {
            (*data).cx = (*data).lastcx;
        }
        window_copy_scroll_up(wme.clone(), 1 as u_int);
        if scroll_only != 0 && (*data).cy > 0 as u_int {
            window_copy_redraw_lines(wme.clone(), (*data).cy.wrapping_sub(1 as u_int), 2 as u_int);
        }
    } else {
        if norectsel != 0 {
            window_copy_update_cursor(
                wme.clone(),
                (*data).lastcx,
                (*data).cy.wrapping_add(1 as u_int),
            );
        } else {
            window_copy_update_cursor(wme.clone(), (*data).cx, (*data).cy.wrapping_add(1 as u_int));
        }
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        ) != 0
        {
            window_copy_redraw_lines(wme.clone(), (*data).cy.wrapping_sub(1 as u_int), 2 as u_int);
        }
    }
    if norectsel != 0 {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        px = window_copy_find_length(wme.clone(), py);
        if (*data).cx >= (*data).lastsx && (*data).cx != px || (*data).cx > px {
            window_copy_update_cursor(wme.clone(), px, (*data).cy);
            if window_copy_update_selection(
                wme.clone(),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            ) != 0
            {
                window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
            }
        }
    }
    if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_LEFT_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        py = (*data)
            .backing()
            .grid()
            .hsize
            .wrapping_add((*data).cy)
            .wrapping_sub((*data).oy);
        if (*data).rectflag != 0 {
            px = (*data).backing().grid().sx;
        } else {
            px = window_copy_find_length(wme.clone(), py);
        }
        window_copy_update_cursor(wme.clone(), px, (*data).cy);
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        ) != 0
        {
            window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
        }
    } else if (*data).lineflag as ::core::ffi::c_uint
        == LINE_SEL_RIGHT_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        window_copy_update_cursor(wme.clone(), 0 as u_int, (*data).cy);
        if window_copy_update_selection(
            wme.clone(),
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        ) != 0
        {
            window_copy_redraw_lines(wme.clone(), (*data).cy, 1 as u_int);
        }
    }
}
unsafe fn window_copy_cursor_jump(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx.wrapping_add(1 as u_int);
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    if grid_reader_cursor_jump(&mut gr, &(&(*data).jumpchar)[0]) != 0 {
        (px, py) = grid_reader_get_cursor(&gr);
        window_copy_acquire_cursor_down(
            wme.clone(),
            hsize,
            (*back_s).grid().sy,
            (*data).oy,
            oldy,
            px,
            py,
            0 as ::core::ffi::c_int,
        );
    }
}
unsafe fn window_copy_cursor_jump_back(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_left(&mut gr, 0 as ::core::ffi::c_int);
    if grid_reader_cursor_jump_back(&mut gr, &(&(*data).jumpchar)[0]) != 0 {
        (px, py) = grid_reader_get_cursor(&gr);
        window_copy_acquire_cursor_up(wme.clone(), hsize, (*data).oy, oldy, px, py);
    }
}
unsafe fn window_copy_cursor_jump_to(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx.wrapping_add(2 as u_int);
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    if grid_reader_cursor_jump(&mut gr, &(&(*data).jumpchar)[0]) != 0 {
        grid_reader_cursor_left(&mut gr, 1 as ::core::ffi::c_int);
        (px, py) = grid_reader_get_cursor(&gr);
        window_copy_acquire_cursor_down(
            wme.clone(),
            hsize,
            (*back_s).grid().sy,
            (*data).oy,
            oldy,
            px,
            py,
            0 as ::core::ffi::c_int,
        );
    }
}
unsafe fn window_copy_cursor_jump_to_back(mut wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    let mut onemore: ::core::ffi::c_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    onemore = (options_get_number(
        &options_window
            .upgrade()
            .expect("live copy-mode window")
            .borrow_options_mut(),
        c"mode-keys",
    ) != MODEKEY_VI as ::core::ffi::c_longlong) as ::core::ffi::c_int;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_left(&mut gr, 0 as ::core::ffi::c_int);
    grid_reader_cursor_left(&mut gr, 0 as ::core::ffi::c_int);
    if grid_reader_cursor_jump_back(&mut gr, &(&(*data).jumpchar)[0]) != 0 {
        grid_reader_cursor_right(
            &mut gr,
            1 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            onemore,
        );
        (px, py) = grid_reader_get_cursor(&gr);
        window_copy_acquire_cursor_up(wme.clone(), hsize, (*data).oy, oldy, px, py);
    }
}
unsafe fn window_copy_cursor_next_word(
    mut wme: refbox::Weak<window_mode_entry>,
    separators: &CStr,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_next_word(&mut gr, separators);
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_down(
        wme.clone(),
        hsize,
        (*back_s).grid().sy,
        (*data).oy,
        oldy,
        px,
        py,
        0 as ::core::ffi::c_int,
    );
}
unsafe fn window_copy_cursor_next_word_end_pos(
    mut wme: refbox::Weak<window_mode_entry>,
    separators: &CStr,
    mut ppx: *mut u_int,
    mut ppy: *mut u_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    if options_get_number(
        &options_window
            .upgrade()
            .expect("live copy-mode window")
            .borrow_options_mut(),
        c"mode-keys",
    ) == MODEKEY_VI as ::core::ffi::c_longlong
    {
        if grid_reader_in_set(&gr, c"\t ") == 0 {
            grid_reader_cursor_right(
                &mut gr,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
        }
        grid_reader_cursor_next_word_end(&mut gr, separators);
        grid_reader_cursor_left(&mut gr, 1 as ::core::ffi::c_int);
    } else {
        grid_reader_cursor_next_word_end(&mut gr, separators);
    }
    (px, py) = grid_reader_get_cursor(&gr);
    *ppx = px;
    *ppy = py;
}
unsafe fn window_copy_cursor_next_word_end(
    mut wme: refbox::Weak<window_mode_entry>,
    separators: &CStr,
    mut no_reset: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    if options_get_number(
        &options_window
            .upgrade()
            .expect("live copy-mode window")
            .borrow_options_mut(),
        c"mode-keys",
    ) == MODEKEY_VI as ::core::ffi::c_longlong
    {
        if grid_reader_in_set(&gr, c"\t ") == 0 {
            grid_reader_cursor_right(
                &mut gr,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
        }
        grid_reader_cursor_next_word_end(&mut gr, separators);
        grid_reader_cursor_left(&mut gr, 1 as ::core::ffi::c_int);
    } else {
        grid_reader_cursor_next_word_end(&mut gr, separators);
    }
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_down(
        wme.clone(),
        hsize,
        (*back_s).grid().sy,
        (*data).oy,
        oldy,
        px,
        py,
        no_reset,
    );
}
unsafe fn window_copy_cursor_previous_word_pos(
    mut wme: refbox::Weak<window_mode_entry>,
    separators: &CStr,
    mut ppx: *mut u_int,
    mut ppy: *mut u_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut hsize: u_int = 0;
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_previous_word(
        &mut gr,
        separators,
        0 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    (px, py) = grid_reader_get_cursor(&gr);
    *ppx = px;
    *ppy = py;
}
unsafe fn window_copy_cursor_previous_word(
    mut wme: refbox::Weak<window_mode_entry>,
    separators: &CStr,
    mut already: ::core::ffi::c_int,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let options_window = mode_pane_owner.window_observer();
    let back_s = (*data).backing();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut oldy: u_int = 0;
    let mut hsize: u_int = 0;
    let mut stop_at_eol: ::core::ffi::c_int = 0;
    if options_get_number(
        &options_window
            .upgrade()
            .expect("live copy-mode window")
            .borrow_options_mut(),
        c"mode-keys",
    ) == MODEKEY_EMACS as ::core::ffi::c_longlong
    {
        stop_at_eol = 1 as ::core::ffi::c_int;
    } else {
        stop_at_eol = 0 as ::core::ffi::c_int;
    }
    px = (*data).cx;
    hsize = (*back_s).grid().hsize;
    py = hsize.wrapping_add((*data).cy).wrapping_sub((*data).oy);
    oldy = (*data).cy;
    let mut gr = grid_reader_start((*back_s).grid(), px, py);
    grid_reader_cursor_previous_word(&mut gr, separators, already, stop_at_eol);
    (px, py) = grid_reader_get_cursor(&gr);
    window_copy_acquire_cursor_up(wme.clone(), hsize, (*data).oy, oldy, px, py);
}
unsafe fn window_copy_cursor_prompt(
    mut wme: refbox::Weak<window_mode_entry>,
    mut direction: ::core::ffi::c_int,
    mut start_output: ::core::ffi::c_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = (*data).backing_mut();
    let mut gd: *mut grid = (*s).grid_mut();
    let mut end_line: u_int = 0;
    let mut line: u_int = (*gd)
        .hsize
        .wrapping_sub((*data).oy)
        .wrapping_add((*data).cy);
    let mut add: ::core::ffi::c_int = 0;
    let mut line_flag: ::core::ffi::c_int = 0;
    if start_output != 0 {
        line_flag = GRID_LINE_START_OUTPUT;
    } else {
        line_flag = GRID_LINE_START_PROMPT;
    }
    if direction == 0 as ::core::ffi::c_int {
        add = -(1 as ::core::ffi::c_int);
        end_line = 0 as u_int;
    } else {
        add = 1 as ::core::ffi::c_int;
        end_line = (*gd).hsize.wrapping_add((*gd).sy).wrapping_sub(1 as u_int);
    }
    if line == end_line {
        return;
    }
    loop {
        if line == end_line {
            return;
        }
        line = line.wrapping_add(add as u_int);
        if grid_get_line(&*gd, line).flags as ::core::ffi::c_int & line_flag != 0 {
            break;
        }
    }
    (*data).cx = 0 as u_int;
    if line > (*gd).hsize {
        (*data).cy = line.wrapping_sub((*gd).hsize);
        (*data).oy = 0 as u_int;
    } else {
        (*data).cy = 0 as u_int;
        (*data).oy = (*gd).hsize.wrapping_sub(line);
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_scroll_up(mut wme: refbox::Weak<window_mode_entry>, mut ny: u_int) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    if (*data).oy < ny {
        ny = (*data).oy;
    }
    if ny == 0 as u_int {
        return;
    }
    (*data).oy = (*data).oy.wrapping_sub(ny);
    mode_pane_owner.show_scrollbar();
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection_view(wme.clone(), 0 as ::core::ffi::c_int);
    if window_copy_cursor_line_active(wme.clone()) != 0 {
        window_copy_redraw_screen(wme.clone());
        return;
    }
    if window_copy_line_numbers_active(wme.clone()) != 0 {
        if window_copy_line_number_mode(wme.clone())
            != WINDOW_COPY_LINE_NUMBERS_ABSOLUTE as ::core::ffi::c_int
        {
            window_copy_redraw_screen(wme.clone());
            return;
        }
        screen_write_start(&mut ctx, &raw mut (*data).screen);
        screen_write_cursormove(
            &mut ctx,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_deleteline(&mut ctx, ny, 8 as u_int);
        window_copy_write_lines(
            wme.clone(),
            &raw mut ctx,
            (*s).grid().sy.wrapping_sub(ny),
            ny,
        );
        window_copy_write_line(wme.clone(), &raw mut ctx, 0 as u_int);
        if (*s).grid().sy > 1 as u_int {
            window_copy_write_line(wme.clone(), &raw mut ctx, 1 as u_int);
        }
        if (*s).grid().sy > 3 as u_int {
            window_copy_write_line(
                wme.clone(),
                &raw mut ctx,
                (*s).grid().sy.wrapping_sub(2 as u_int),
            );
        }
        if !(*s).sel.is_none() && (*s).grid().sy > ny {
            window_copy_write_line(
                wme.clone(),
                &raw mut ctx,
                (*s).grid().sy.wrapping_sub(ny).wrapping_sub(1 as u_int),
            );
        }
        screen_write_cursormove(
            &mut ctx,
            window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx)
                as ::core::ffi::c_int,
            (*data).cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_stop(&mut ctx);
        mode_pane_owner.request_redraw(true);
        return;
    }
    let res = {
        let scrollbar = &mode_pane_owner;
        scrollbar.scrollbar_overlay() && scrollbar.scrollbar_visible()
    };
    if res {
        screen_write_start(&mut ctx, &raw mut (*data).screen);
    } else {
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
    }
    screen_write_cursormove(
        &mut ctx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_deleteline(&mut ctx, ny, 8 as u_int);
    window_copy_write_lines(
        wme.clone(),
        &raw mut ctx,
        (*s).grid().sy.wrapping_sub(ny),
        ny,
    );
    window_copy_write_line(wme.clone(), &raw mut ctx, 0 as u_int);
    if (*s).grid().sy > 1 as u_int {
        window_copy_write_line(wme.clone(), &raw mut ctx, 1 as u_int);
    }
    if (*s).grid().sy > 3 as u_int {
        window_copy_write_line(
            wme.clone(),
            &raw mut ctx,
            (*s).grid().sy.wrapping_sub(2 as u_int),
        );
    }
    if !(*s).sel.is_none() && (*s).grid().sy > ny {
        window_copy_write_line(
            wme.clone(),
            &raw mut ctx,
            (*s).grid().sy.wrapping_sub(ny).wrapping_sub(1 as u_int),
        );
    }
    screen_write_cursormove(
        &mut ctx,
        window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx) as ::core::ffi::c_int,
        (*data).cy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_stop(&mut ctx);
    mode_pane_owner.redraw_scrollbar();
}
unsafe fn window_copy_scroll_down(mut wme: refbox::Weak<window_mode_entry>, mut ny: u_int) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut s: *mut screen = &raw mut (*data).screen;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    if ny > (*data).backing().grid().hsize {
        return;
    }
    if (*data).oy > (*data).backing().grid().hsize.wrapping_sub(ny) {
        ny = (*data).backing().grid().hsize.wrapping_sub((*data).oy);
    }
    if ny == 0 as u_int {
        return;
    }
    (*data).oy = (*data).oy.wrapping_add(ny);
    mode_pane_owner.show_scrollbar();
    if !(*data).searchmark.is_empty() && (*data).timeout == 0 {
        window_copy_search_marks(
            wme.clone(),
            ::core::ptr::null_mut::<screen>(),
            (*data).searchregex,
            1 as ::core::ffi::c_int,
        );
    }
    window_copy_update_selection_view(wme.clone(), 0 as ::core::ffi::c_int);
    if window_copy_cursor_line_active(wme.clone()) != 0 {
        window_copy_redraw_screen(wme.clone());
        return;
    }
    if window_copy_line_numbers_active(wme.clone()) != 0 {
        if window_copy_line_number_mode(wme.clone())
            != WINDOW_COPY_LINE_NUMBERS_ABSOLUTE as ::core::ffi::c_int
        {
            window_copy_redraw_screen(wme.clone());
            return;
        }
        screen_write_start(&mut ctx, &raw mut (*data).screen);
        screen_write_cursormove(
            &mut ctx,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_insertline(&mut ctx, ny, 8 as u_int);
        window_copy_write_lines(wme.clone(), &raw mut ctx, 0 as u_int, ny);
        if !(*s).sel.is_none() && (*s).grid().sy > ny {
            window_copy_write_line(wme.clone(), &raw mut ctx, ny);
        } else if ny == 1 as u_int {
            window_copy_write_line(wme.clone(), &raw mut ctx, 1 as u_int);
        }
        screen_write_cursormove(
            &mut ctx,
            window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx)
                as ::core::ffi::c_int,
            (*data).cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_stop(&mut ctx);
        mode_pane_owner.request_redraw(true);
        return;
    }
    let res = {
        let scrollbar = &mode_pane_owner;
        scrollbar.scrollbar_overlay() && scrollbar.scrollbar_visible()
    };
    if res {
        screen_write_start(&mut ctx, &raw mut (*data).screen);
    } else {
        screen_write_start_pane(
            &mut ctx,
            &mode_pane_owner,
            ::core::ptr::null_mut::<screen>(),
        );
    }
    screen_write_cursormove(
        &mut ctx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_insertline(&mut ctx, ny, 8 as u_int);
    window_copy_write_lines(wme.clone(), &raw mut ctx, 0 as u_int, ny);
    if !(*s).sel.is_none() && (*s).grid().sy > ny {
        window_copy_write_line(wme.clone(), &raw mut ctx, ny);
    } else if ny == 1 as u_int {
        window_copy_write_line(wme.clone(), &raw mut ctx, 1 as u_int);
    }
    screen_write_cursormove(
        &mut ctx,
        window_copy_cursor_offset(wme.clone(), (*data).cx, (*s).grid().sx) as ::core::ffi::c_int,
        (*data).cy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_stop(&mut ctx);
    mode_pane_owner.redraw_scrollbar();
}
unsafe fn window_copy_rectangle_set(
    mut wme: refbox::Weak<window_mode_entry>,
    mut rectflag: ::core::ffi::c_int,
) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    (*data).rectflag = rectflag;
    py = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    px = window_copy_cursor_limit(wme.clone(), py, (*data).rectflag);
    if (*data).cx > px {
        window_copy_update_cursor(wme.clone(), px, (*data).cy);
    }
    window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_move_mouse(mut m: *mut mouse_event) {
    let mouse_pane_owner;
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut data: *mut window_copy_mode_data = ::core::ptr::null_mut::<window_copy_mode_data>();
    mouse_pane_owner = cmd_mouse_pane(m, None, ::core::ptr::null_mut::<refbox::Weak<winlink>>());
    let Some(mouse_pane) = mouse_pane_owner.as_ref() else {
        return;
    };
    wme = mouse_pane.mode_entry();
    if !wme.is_alive() {
        return;
    }
    if !std::ptr::eq(wme.get_unchecked().mode, &window_copy_mode)
        && !std::ptr::eq(wme.get_unchecked().mode, &window_view_mode)
    {
        return;
    }
    if cmd_mouse_at(
        mouse_pane_owner.as_ref().expect("mouse pane was resolved"),
        m,
        &raw mut x,
        &raw mut y,
        0 as ::core::ffi::c_int,
    ) != 0 as ::core::ffi::c_int
    {
        return;
    }
    data = window_copy_data(wme.clone());
    x = window_copy_cursor_unoffset(wme.clone(), x, (*data).screen.grid().sx);
    window_copy_update_cursor(wme.clone(), x, y);
}
unsafe fn window_copy_install_drag_callbacks(client_owner: &ClientRef) {
    let _c: Option<ClientRef> = Some(client_owner.clone());
    let drag_client = std::rc::Rc::downgrade(client_owner);
    let update = Box::new(move |m: &mut mouse_event| {
        if let Some(owner) = drag_client.upgrade() {
            unsafe { window_copy_drag_update(&owner, m) }
        }
    });
    let drag_client = std::rc::Rc::downgrade(client_owner);
    let release = Box::new(move |m: &mut mouse_event| {
        if let Some(owner) = drag_client.upgrade() {
            unsafe { window_copy_drag_release(&owner, m) }
        }
    });
    client_owner.set_mouse_drag_callbacks(update, release);
}

pub unsafe fn window_copy_start_drag(client_owner: Option<&ClientRef>, mut m: *mut mouse_event) {
    let Some(client_owner) = client_owner else {
        return;
    };
    let mouse_pane_owner;
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut data: *mut window_copy_mode_data = ::core::ptr::null_mut::<window_copy_mode_data>();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut yg: u_int = 0;
    let mut inside_selection: ::core::ffi::c_int = 0;
    let mut on_start: ::core::ffi::c_int = 0;
    let mut on_end: ::core::ffi::c_int = 0;
    mouse_pane_owner = cmd_mouse_pane(m, None, ::core::ptr::null_mut::<refbox::Weak<winlink>>());
    let Some(mouse_pane) = mouse_pane_owner.as_ref() else {
        return;
    };
    wme = mouse_pane.mode_entry();
    if !wme.is_alive() {
        return;
    }
    if !std::ptr::eq(wme.get_unchecked().mode, &window_copy_mode)
        && !std::ptr::eq(wme.get_unchecked().mode, &window_view_mode)
    {
        return;
    }
    if cmd_mouse_at(
        mouse_pane_owner.as_ref().expect("mouse pane was resolved"),
        m,
        &raw mut x,
        &raw mut y,
        1 as ::core::ffi::c_int,
    ) != 0 as ::core::ffi::c_int
    {
        return;
    }
    window_copy_install_drag_callbacks(client_owner);
    data = window_copy_data(wme.clone());
    on_end = 0 as ::core::ffi::c_int;
    on_start = on_end;
    inside_selection =
        window_copy_mouse_in_selection(wme.clone(), x, y, &raw mut on_start, &raw mut on_end);
    x = window_copy_cursor_unoffset(wme.clone(), x, (*data).screen.grid().sx);
    yg = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add(y)
        .wrapping_sub((*data).oy);
    if on_start != 0
        || on_end != 0
        || inside_selection == 0
        || x < (*data).selrx
        || x > (*data).endselrx
        || yg != (*data).selry
    {
        (*data).lineflag = LINE_SEL_NONE;
        (*data).selflag = SEL_CHAR;
    }
    match (*data).selflag as ::core::ffi::c_uint {
        1 => {
            if (*data).separators.is_some() {
                window_copy_update_cursor(wme.clone(), x, y);
                window_copy_cursor_previous_word_pos(
                    wme.clone(),
                    window_copy_separators(&*data).expect("word separators"),
                    &raw mut x,
                    &raw mut y,
                );
                y = y.wrapping_sub((*data).backing().grid().hsize.wrapping_sub((*data).oy));
            }
            window_copy_update_cursor(wme.clone(), x, y);
        }
        2 => {
            window_copy_update_cursor(wme.clone(), 0 as u_int, y);
        }
        0 => {
            window_copy_update_cursor(wme.clone(), x, y);
            if inside_selection == 0 {
                window_copy_start_selection(wme.clone());
            } else {
                if on_start != 0 {
                    (*data).cursordrag = CURSORDRAG_SEL;
                } else if on_end != 0 {
                    (*data).cursordrag = CURSORDRAG_ENDSEL;
                }
                window_copy_update_selection(
                    wme.clone(),
                    1 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
            }
        }
        _ => {}
    }
    window_copy_redraw_screen(wme.clone());
    window_copy_drag_update(client_owner, m);
}
unsafe fn window_copy_drag_update(_client_owner: &ClientRef, mut m: *mut mouse_event) {
    let mouse_pane_owner;
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut data: *mut window_copy_mode_data = ::core::ptr::null_mut::<window_copy_mode_data>();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut old_cx: u_int = 0;
    let mut old_cy: u_int = 0;
    let tv = Duration::from_micros(WINDOW_COPY_DRAG_REPEAT_TIME as u64);
    mouse_pane_owner = cmd_mouse_pane(m, None, ::core::ptr::null_mut::<refbox::Weak<winlink>>());
    let Some(mouse_pane) = mouse_pane_owner.as_ref() else {
        return;
    };
    wme = mouse_pane.mode_entry();
    if !wme.is_alive() {
        return;
    }
    if !std::ptr::eq(wme.get_unchecked().mode, &window_copy_mode)
        && !std::ptr::eq(wme.get_unchecked().mode, &window_view_mode)
    {
        return;
    }
    data = window_copy_data(wme.clone());
    drop((*data).dragtimer.take());
    if cmd_mouse_at(
        mouse_pane_owner.as_ref().expect("mouse pane was resolved"),
        m,
        &raw mut x,
        &raw mut y,
        0 as ::core::ffi::c_int,
    ) != 0 as ::core::ffi::c_int
    {
        return;
    }
    x = window_copy_cursor_unoffset(wme.clone(), x, (*data).screen.grid().sx);
    old_cx = (*data).cx;
    old_cy = (*data).cy;
    window_copy_update_cursor(wme.clone(), x, y);
    if window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    ) != 0
    {
        window_copy_redraw_selection(wme.clone(), old_cy);
    }
    if old_cy != (*data).cy || old_cx == (*data).cx {
        if y == 0 as u_int {
            (*data).dragtimer = Some(window_copy_timer(wme.clone(), tv, window_copy_scroll_timer));
            window_copy_cursor_up(wme.clone(), 1 as ::core::ffi::c_int);
        } else if y == (*data).screen.grid().sy.wrapping_sub(1 as u_int) {
            (*data).dragtimer = Some(window_copy_timer(wme.clone(), tv, window_copy_scroll_timer));
            window_copy_cursor_down(wme.clone(), 1 as ::core::ffi::c_int);
        }
    }
}
unsafe fn window_copy_drag_release(client_owner: &ClientRef, mut m: *mut mouse_event) {
    let mouse_pane_owner;
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut data: *mut window_copy_mode_data = ::core::ptr::null_mut::<window_copy_mode_data>();
    mouse_pane_owner = cmd_mouse_pane(m, None, ::core::ptr::null_mut::<refbox::Weak<winlink>>());
    let Some(mouse_pane) = mouse_pane_owner.as_ref() else {
        return;
    };
    wme = mouse_pane.mode_entry();
    if !wme.is_alive() {
        return;
    }
    if !std::ptr::eq(wme.get_unchecked().mode, &window_copy_mode)
        && !std::ptr::eq(wme.get_unchecked().mode, &window_view_mode)
    {
        return;
    }
    data = window_copy_data(wme.clone());
    if window_copy_line_numbers_active(wme.clone()) != 0 {
        window_copy_drag_update(client_owner, m);
    }
    (*data).cursordrag = CURSORDRAG_NONE;
    drop((*data).dragtimer.take());
}
unsafe fn window_copy_jump_to_mark(mut wme: refbox::Weak<window_mode_entry>) {
    let mut data: *mut window_copy_mode_data = window_copy_data(wme.clone());
    let mut tmx: u_int = 0;
    let mut tmy: u_int = 0;
    tmx = (*data).cx;
    tmy = (*data)
        .backing()
        .grid()
        .hsize
        .wrapping_add((*data).cy)
        .wrapping_sub((*data).oy);
    (*data).cx = (*data).mx;
    if (*data).my < (*data).backing().grid().hsize {
        (*data).cy = 0 as u_int;
        (*data).oy = (*data).backing().grid().hsize.wrapping_sub((*data).my);
    } else {
        (*data).cy = (*data).my.wrapping_sub((*data).backing().grid().hsize);
        (*data).oy = 0 as u_int;
    }
    (*data).mx = tmx;
    (*data).my = tmy;
    (*data).showmark = 1 as ::core::ffi::c_int;
    window_copy_update_selection(
        wme.clone(),
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    window_copy_redraw_screen(wme.clone());
}
unsafe fn window_copy_acquire_cursor_up(
    mut wme: refbox::Weak<window_mode_entry>,
    mut hsize: u_int,
    mut oy: u_int,
    mut oldy: u_int,
    mut px: u_int,
    mut py: u_int,
) {
    let mut cy: u_int = 0;
    let mut yy: u_int = 0;
    let mut ny: u_int = 0;
    let mut nd: u_int = 0;
    yy = hsize.wrapping_sub(oy);
    if py < yy {
        ny = yy.wrapping_sub(py);
        cy = 0 as u_int;
        nd = 1 as u_int;
    } else {
        ny = 0 as u_int;
        cy = py.wrapping_sub(yy);
        nd = oldy.wrapping_sub(cy).wrapping_add(1 as u_int);
    }
    while ny > 0 as u_int {
        window_copy_cursor_up(wme.clone(), 1 as ::core::ffi::c_int);
        ny = ny.wrapping_sub(1);
    }
    window_copy_update_cursor(wme.clone(), px, cy);
    if window_copy_update_selection(
        wme.clone(),
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    ) != 0
    {
        window_copy_redraw_lines(wme.clone(), cy, nd);
    }
}
unsafe fn window_copy_acquire_cursor_down(
    mut wme: refbox::Weak<window_mode_entry>,
    mut hsize: u_int,
    mut sy: u_int,
    mut oy: u_int,
    mut oldy: u_int,
    mut px: u_int,
    mut py: u_int,
    mut no_reset: ::core::ffi::c_int,
) {
    let mut cy: u_int = 0;
    let mut yy: u_int = 0;
    let mut ny: u_int = 0;
    let mut nd: u_int = 0;
    cy = py.wrapping_sub(hsize).wrapping_add(oy);
    yy = sy.wrapping_sub(1 as u_int);
    if cy > yy {
        ny = cy.wrapping_sub(yy);
        oldy = yy;
        nd = 1 as u_int;
    } else {
        ny = 0 as u_int;
        nd = cy.wrapping_sub(oldy).wrapping_add(1 as u_int);
    }
    while ny > 0 as u_int {
        window_copy_cursor_down(wme.clone(), 1 as ::core::ffi::c_int);
        ny = ny.wrapping_sub(1);
    }
    if cy > yy {
        window_copy_update_cursor(wme.clone(), px, yy);
    } else {
        window_copy_update_cursor(wme.clone(), px, cy);
    }
    if window_copy_update_selection(wme.clone(), 1 as ::core::ffi::c_int, no_reset) != 0 {
        window_copy_redraw_lines(wme.clone(), oldy, nd);
    }
}

#[cfg(test)]
mod regex_cell_tests {
    use super::*;

    #[test]
    fn suffix_mapping_preserves_padding_and_partial_cells() {
        let cells = [
            Cow::Borrowed(&b"a"[..]),
            Cow::Borrowed(&b""[..]),
            Cow::Borrowed(&b"bc"[..]),
        ];
        for (text, expected) in [
            (&b"abc"[..], 0),
            (&b"bc"[..], 1),
            (&b"b"[..], 1),
            (&b"c"[..], 3),
            (&b""[..], 3),
            (&b"abcd"[..], 0),
        ] {
            assert_eq!(window_copy_find_cell_suffix(&cells, text), expected);
        }
        let wide = [Cow::Borrowed("漢".as_bytes()), Cow::Borrowed(&b""[..])];
        assert_eq!(window_copy_find_cell_suffix(&wide, "漢".as_bytes()), 1);
        assert_eq!(window_copy_find_cell_suffix(&wide[..1], &[0xe6]), 0);
        assert_eq!(window_copy_find_cell_suffix(&wide[..1], &[0xff]), 1);
    }

    #[test]
    fn cell_mapping_bounds_lines_and_stops_at_nul() {
        let mut gd = grid {
            sx: 3,
            sy: 2,
            ..grid::default()
        };
        gd.linedata.resize_with(2, grid_line::default);
        for (line, bytes) in gd.linedata.iter_mut().zip([b"abc", b"def"]) {
            line.cellsize = 3;
            line.celldata.resize_with(3, grid_cell_entry::default);
            for (cell, &byte) in line.celldata.iter_mut().zip(bytes) {
                cell.c2rust_unnamed = crate::src::shared::grid::grid_cell_entry_storage {
                    data: crate::src::shared::grid::grid_cell_entry_data {
                        attr: 0,
                        fg: 0,
                        bg: 0,
                        data: byte,
                    },
                };
            }
        }
        unsafe {
            let (mut x, mut y) = (1, 0);
            window_copy_cstrtocellpos(&gd, 5, &mut x, &mut y, b"def\0ignored");
            assert_eq!((x, y), (0, 1));
            window_copy_cstrtocellpos(&gd, 3, &mut x, &mut y, b"");
            assert_eq!((x, y), (0, 2));
            window_copy_cstrtocellpos(&gd, 3, &mut x, &mut y, b"a");
            assert_eq!((x, y), (0, 2));
        }
    }
}

#[cfg(test)]
mod backing_owner_tests {
    use super::*;
    use crate::src::grid::grid_set_cell;
    use crate::src::options::options_default;
    use crate::src::options::options_table;

    unsafe fn byte_at(s: &screen, x: u_int, y: u_int) -> u8 {
        let mut cell = grid_default_cell;
        grid_get_cell(s.grid(), x, y, &mut cell);
        cell.data.data[0]
    }

    #[test]
    fn reflowed_snapshot_owns_cells_independently_of_the_source() {
        unsafe {
            with_global_options(|oo| {
                options_default(
                    oo,
                    options_table
                        .iter()
                        .find(|entry| entry.name == Some(c"extended-keys"))
                        .unwrap(),
                );
            });
            let mut source = screen::empty();
            let mut hint = screen::empty();
            screen_init(&mut source, 8, 3, 10);
            screen_init(&mut hint, 4, 3, 0);
            for (x, byte) in b"ABCDEFGH".iter().enumerate() {
                let mut cell = grid_default_cell;
                cell.data.data[0] = *byte;
                grid_set_cell(source.grid_mut(), x as u_int, 0, &cell);
            }
            source.cx = 6;
            let (mut cx, mut cy) = (0, 0);
            let mut data = window_copy_mode_data::default();
            data.backing = Some(window_copy_clone_screen(
                &source,
                &hint,
                Some((&mut cx, &mut cy)),
                false,
            ));
            assert_eq!((cx, cy), (2, 1));
            assert_eq!(byte_at(data.backing(), 0, 0), b'A');
            assert_eq!(byte_at(data.backing(), 0, 1), b'E');
            assert_eq!((source.grid().sx, source.cx, source.cy), (8, 6, 0));
            assert_eq!(byte_at(&source, 4, 0), b'E');

            drop(source);
            assert_eq!(byte_at(data.backing(), 3, 1), b'H');
            drop(hint);
            drop(data);
        }
    }
}
