use crate::src::arguments::args_escape_cstring;
use crate::src::cfg::cfg_files;
use crate::src::cmd::cmd_mouse_pane;
use crate::src::cmd::queue::{
    cmdq_get_client, cmdq_get_event, cmdq_get_target_client, cmdq_merge_formats, cmdq_print,
};
use crate::src::compat::strtonum::strtonum;
use crate::src::environ::{environ_find, environ_iter};
use crate::src::ffi::libc::basename;
use crate::src::ffi::libm::{fabs, fmod};
use crate::src::ffi::regex::RegexStorage;
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::write_cstr;
use crate::src::format_draw::{format_trim_left_bytes, format_trim_right_bytes, format_width};
use crate::src::fuzzy::fuzzy_match_owned;
use crate::src::grid::{grid_get_cell, grid_line_length, grid_peek_line};
use crate::src::hyperlinks::hyperlinks_get;
use crate::src::job::{job_free, job_get_event, job_run};
use crate::src::log::{log_cstr, log_debug, log_get_level};
use crate::src::options::{
    options_get_number, options_get_string, options_is_array, options_parse_owned,
    options_to_cstring,
};
use crate::src::paste::{
    paste_buffer_created, paste_buffer_data, paste_buffer_name, paste_get_top,
    paste_make_sample_cstring,
};
use crate::src::reactor::{evbuffer_add, evbuffer_new, evbuffer_pullup, evbuffer_readline};
use crate::src::regsub::regsub_cstring;
use crate::src::server::{marked_pane, server_check_marked};
use crate::src::shared::window::WindowWeak;
use crate::src::window::Window as _;
use libc::{
    ctime_r, dirname, fnmatch, gethostname, getpid, getpwuid, getuid, localtime_r, memcmp, memcpy,
    memset, strcasecmp, strchr, strcmp, strcspn, strftime, strlen, strstr, strtod, time,
};

use crate::src::server_client::Client as _;
use crate::src::server_fn::server_status_client;
use crate::src::session::session_groups;
use crate::src::session::{sessions, Session};
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::WindowRef;
use crate::src::sort::{
    sort_get_clients, sort_get_panes_window, sort_get_sessions, sort_get_winlinks_session,
};
use crate::src::status::status_get_range;
use crate::src::style::colour::{
    colour_force_rgb, colour_format_escape_for_client, colour_parse_cstr,
};
use crate::src::text::utf8::{utf8_cstrhas, utf8_pad_cstring, utf8_set, utf8_tocstr_cstring};
use crate::src::tmux::{
    get_timer, getversion, global_environ, global_options, global_s_options, global_w_options,
    socket_path_cstr, start_time,
};
use crate::src::tty_features::tty_feature_present;
use crate::src::tty_term::tty_term_has_name;
use crate::src::window::{winlink_find_by_window, winlinks_minmax, winlinks_next};
use crate::src::window_buffer::window_buffer_mode;
use crate::src::window_client::window_client_mode;
use crate::src::window_pane::WindowPane as _;
use crate::src::window_tree::window_tree_mode;
use std::ffi::{CStr, CString};

use crate::src::shared::abi::*;
use crate::src::shared::client::CLIENT_UNATTACHEDFLAGS;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::environment::ENVIRON_HIDDEN;
use crate::src::shared::environment::{environ, environ_entry};
pub use crate::src::shared::format::{
    format_entry, format_entry_tree, format_job, format_job_tree, format_tree, format_type,
};
use crate::src::shared::format::{
    FORMAT_BASENAME, FORMAT_CHARACTER, FORMAT_CLIENTS, FORMAT_CLIENT_ENVIRON,
    FORMAT_CLIENT_TERMCAP, FORMAT_CLIENT_TERMFEAT, FORMAT_COLOUR, FORMAT_COLOUR_ESC_BG,
    FORMAT_COLOUR_ESC_FG, FORMAT_CYCLE, FORMAT_CYCLE_PERIOD, FORMAT_DIFFERENCE, FORMAT_DIRNAME,
    FORMAT_ENVIRON, FORMAT_EXPAND, FORMAT_EXPANDTIME, FORMAT_EXPAND_NOCYCLE, FORMAT_EXPAND_NOJOBS,
    FORMAT_EXPAND_TIME, FORMAT_FORCE, FORMAT_LENGTH, FORMAT_LITERAL, FORMAT_LOOP_LIMIT,
    FORMAT_MAX_PRECISION, FORMAT_MAX_REPEAT, FORMAT_MAX_WIDTH, FORMAT_NOJOBS, FORMAT_NONE,
    FORMAT_NOT, FORMAT_NOT_NOT, FORMAT_OPTIONS, FORMAT_PANE, FORMAT_PANES, FORMAT_PRETTY,
    FORMAT_QUOTE_ARGUMENTS, FORMAT_QUOTE_SHELL, FORMAT_QUOTE_SHELL_SQ, FORMAT_QUOTE_STYLE,
    FORMAT_RELATIVE, FORMAT_REPEAT, FORMAT_SESSIONS, FORMAT_SESSION_NAME, FORMAT_STATUS,
    FORMAT_TIMESTRING, FORMAT_TIME_LIMIT, FORMAT_TIME_LOOP_CHECK, FORMAT_VERBOSE, FORMAT_WIDTH,
    FORMAT_WINDOW, FORMAT_WINDOWS, FORMAT_WINDOW_NAME,
};
use crate::src::shared::grid::*;
use crate::src::shared::job::JOB_NOWAIT;
use crate::src::shared::job::{job, job_update_callback, JobCompletion};
use crate::src::shared::key::key_event;
use crate::src::shared::mouse::mouse_event;
use crate::src::shared::options::OPTIONS_TABLE_IS_HOOK;
use crate::src::shared::options::{options, options_entry};
use crate::src::shared::pane::window_pane;
use crate::src::shared::paste::PasteBufferRef;
use crate::src::shared::posix_io::FNM_CASEFOLD;
use crate::src::shared::screen::screen;
use crate::src::shared::sort::sort_criteria;
use crate::src::shared::sort::*;
use crate::src::shared::time::tm;
use crate::src::shared::tree::{RB_INF, RB_NEGINF};
use crate::src::shared::window::winlink;
use crate::src::shared::window::{
    WINDOW_SIZE_MANUAL, WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE,
};
use libc::{REG_EXTENDED, REG_ICASE};

/*
 * This module is the stable facade for format handling.  The private
 * implementation modules below keep the translated C ABI and the existing
 * `crate::src::format::*` paths in one place while separating responsibilities:
 * tree stores the red-black trees and format-tree CRUD, jobs owns the shared
 * format-job cache and its callbacks, callbacks owns the default callback
 * table, and expression owns parsing and evaluation.  The facade remains the
 * dependency boundary for generated shared types, FFI declarations, and the
 * small logging/state helpers used by more than one group.
 */
pub mod bytes;
mod tree;
pub use tree::format_add_owned_cb;
use tree::*;
pub use tree::{
    format_add, format_add_cstr, format_add_time, format_create, format_create_owned,
    format_create_with_client, format_each, format_get_pane, format_log_debug, format_merge,
    format_owner_ptr,
};
mod jobs;
use jobs::*;
pub use jobs::{format_lost_client, format_tidy_jobs};
mod callbacks;
pub(crate) use callbacks::window_format_value;
pub use callbacks::FormatValue;
use callbacks::*;

pub(crate) fn format_is_builtin(key: &CStr) -> bool {
    format_table_get(key).is_some()
}

pub(super) unsafe fn format_plugin_pane(
    ft: *mut format_tree,
) -> Option<crate::src::plugin::PaneId> {
    let pane = <std::rc::Rc<std::cell::UnsafeCell<window_pane>>>::from_observer(&(*ft).wp)?;
    let id = crate::src::plugin::PaneId(pane.id());
    pane.release(c"plugin format context");
    Some(id)
}
mod expression;
pub use expression::format_expand_cstring;
pub(crate) use expression::format_pretty_time_cstring;
pub(crate) use expression::format_quote_shell_single;
pub(crate) use expression::format_single_from_state_cstring;
use expression::*;
pub(crate) use expression::{
    format_expand_time_cstring, format_single_cstring, format_single_from_target_cstring,
};
pub use expression::{format_skip, format_true};

pub struct format_modifier {
    pub modifier: [::core::ffi::c_char; 3],
    pub size: u_int,
    pub argv: Vec<std::ffi::CString>,
}

impl format_modifier {
    fn argc(&self) -> ::core::ffi::c_int {
        self.argv
            .len()
            .try_into()
            .expect("modifier argument count fits c_int")
    }

    fn arg(&self, index: usize) -> &CStr {
        &self.argv[index]
    }
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct format_expand_state {
    pub ft: *mut format_tree,
    pub loop_0: u_int,
    pub start_time: uint64_t,
    pub flags: ::core::ffi::c_int,
    pub time: time_t,
    pub tm: tm,
}

impl Default for format_expand_state {
    fn default() -> Self {
        // SAFETY: every field is an integer or a nullable pointer.
        unsafe { ::core::mem::zeroed() }
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_43 {
    pub mode: ::core::ffi::c_int,
    pub number: ::core::ffi::c_int,
}
pub const LESS_THAN_EQUAL: C2RustUnnamed_44 = 10;
pub const LESS_THAN: C2RustUnnamed_44 = 9;
pub const GREATER_THAN_EQUAL: C2RustUnnamed_44 = 8;
pub const GREATER_THAN: C2RustUnnamed_44 = 7;
pub const NOT_EQUAL: C2RustUnnamed_44 = 6;
pub const EQUAL: C2RustUnnamed_44 = 5;
pub const MODULUS: C2RustUnnamed_44 = 4;
pub const DIVIDE: C2RustUnnamed_44 = 3;
pub const MULTIPLY: C2RustUnnamed_44 = 2;
pub const SUBTRACT: C2RustUnnamed_44 = 1;
pub const ADD: C2RustUnnamed_44 = 0;
pub type C2RustUnnamed_44 = ::core::ffi::c_uint;

pub use ::libc::REG_NOSUB;
pub const INT64_MAX: ::core::ffi::c_long = 9223372036854775807 as ::core::ffi::c_long;

static mut sort_crit: sort_criteria = sort_criteria {
    order: SORT_ACTIVITY,
    reversed: 0,
    order_seq: &[],
};
const format_upper: [Option<&CStr>; 26] = [
    None,
    None,
    None,
    Some(c"pane_id"),
    None,
    Some(c"window_flags"),
    None,
    Some(c"host"),
    Some(c"window_index"),
    None,
    None,
    None,
    None,
    None,
    None,
    Some(c"pane_index"),
    None,
    None,
    Some(c"session_name"),
    Some(c"pane_title"),
    None,
    None,
    Some(c"window_name"),
    None,
    None,
    None,
];
const format_lower: [Option<&CStr>; 26] = [
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some(c"host_short"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];
#[inline]
unsafe fn format_logging(mut ft: *mut format_tree) -> ::core::ffi::c_int {
    (log_get_level() != 0 as ::core::ffi::c_int || (*ft).flags & FORMAT_VERBOSE != 0)
        as ::core::ffi::c_int
}
unsafe fn format_log1(
    mut es: *mut format_expand_state,
    mut from: *const ::core::ffi::c_char,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut ft: *mut format_tree = (*es).ft;
    if format_logging(ft) == 0 {
        return;
    }
    let s = format_message_with(write);
    log_debug(format_args!(
        "{}: {}",
        log_cstr(CStr::from_ptr(from)),
        log_cstr(&s)
    ));
    if let Some(item) = (*ft)
        .item
        .upgrade()
        .filter(|_| (*ft).flags & FORMAT_VERBOSE != 0)
    {
        cmdq_print(&item, |out| {
            out.write_all(b"#")?;
            out.write_all(&b"          "[..((*es).loop_0 as usize).min(10)])?;
            write_cstr(out, &*s)
        });
    }
}
unsafe fn format_copy_state(
    mut to: *mut format_expand_state,
    mut from: *mut format_expand_state,
    mut flags: ::core::ffi::c_int,
) {
    (*to).ft = (*from).ft;
    (*to).loop_0 = (*from).loop_0;
    (*to).time = (*from).time;
    memcpy(
        &raw mut (*to).tm as *mut ::core::ffi::c_void,
        &raw mut (*from).tm as *const ::core::ffi::c_void,
        ::core::mem::size_of::<tm>() as size_t,
    );
    (*to).flags = (*from).flags | flags;
    (*to).start_time = (*from).start_time;
}
pub unsafe fn format_create_defaults(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    c_owner: Option<&ClientRef>,
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
) -> Box<format_tree> {
    let item = item_handle.map_or(std::ptr::null_mut(), |item| item.get());
    let queue_client = cmdq_get_client((item).as_ref());
    let mut owner;
    if !item.is_null() {
        owner = format_create_with_client(
            queue_client.as_ref(),
            item_handle,
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
    } else {
        owner = format_create(None, item_handle, FORMAT_NONE, 0 as ::core::ffi::c_int);
    }
    let ft = &raw mut *owner;
    format_defaults(ft, c_owner, s_owner, wl.clone(), wp_owner);
    owner
}
pub unsafe fn format_create_from_state(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    c_owner: Option<&ClientRef>,
    fs: &cmd_find_state,
) -> Box<format_tree> {
    format_create_defaults(
        item_handle,
        c_owner,
        fs.s.upgrade().as_ref(),
        (fs.winlink_handle()).clone(),
        fs.wp.upgrade().as_ref(),
    )
}
pub unsafe fn format_create_from_target(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> Box<format_tree> {
    let item = item_handle.get();
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    format_create_from_state(
        Some(item_handle),
        tc.as_ref(),
        &*crate::src::cmd::queue::cmdq_get_target_mut(&mut *item),
    )
}
pub unsafe fn format_defaults(
    mut ft: *mut format_tree,
    c_owner: Option<&ClientRef>,
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
) {
    let mut c: Option<ClientRef> = c_owner.cloned();
    if let Some(name) = c.as_ref().and_then(|c| c.name()) {
        log_debug(format_args!("{}: c={}", "format_defaults", log_cstr(&name)));
    } else {
        log_debug(format_args!("{}: c=none", "format_defaults"));
    }
    if let Some(s) = s_owner {
        log_debug(format_args!("{}: s=${}", "format_defaults", s.id()));
    } else {
        log_debug(format_args!("{}: s=none", "format_defaults"));
    }
    if wl.is_alive() {
        log_debug(format_args!(
            "{}: wl={}",
            "format_defaults",
            (wl.get_unchecked().idx) as u32
        ));
    } else {
        log_debug(format_args!("{}: wl=none", "format_defaults"));
    }
    if let Some(wp) = wp_owner {
        log_debug(format_args!("{}: wp=%{}", "format_defaults", wp.id()));
    } else {
        log_debug(format_args!("{}: wp=none", "format_defaults"));
    }
    if !c.is_none()
        && s_owner.is_some()
        && !crate::src::shared::rc::same(
            c.as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .as_ref(),
            s_owner,
        )
    {
        log_debug(format_args!(
            "{}: session does not match",
            "format_defaults"
        ));
    }
    if wp_owner.is_some() {
        (*ft).type_0 = FORMAT_TYPE_PANE;
    } else if wl.is_alive() {
        (*ft).type_0 = FORMAT_TYPE_WINDOW;
    } else if s_owner.is_some() {
        (*ft).type_0 = FORMAT_TYPE_SESSION;
    } else {
        (*ft).type_0 = FORMAT_TYPE_UNKNOWN;
    }
    let session_owner = s_owner
        .cloned()
        .or_else(|| c_owner.and_then(|owner| owner.attached_session().upgrade()));
    if !wl.is_alive() {
        wl = session_owner
            .as_ref()
            .map_or_else(refbox::Weak::new, |owner| owner.current_winlink());
    }
    let pane_owner = wp_owner.cloned().or_else(|| {
        wl.try_borrow_mut().ok().and_then(|link| {
            link.window_owner
                .as_ref()
                .and_then(|owner| owner.active_pane().upgrade())
        })
    });
    if let Some(client) = c_owner {
        format_defaults_client(ft, client);
    }
    if let Some(session) = session_owner.as_ref() {
        format_defaults_session(ft, session);
    }
    if wl.is_alive() {
        format_defaults_winlink(ft, wl.clone());
    }
    if let Some(pane) = pane_owner.as_ref() {
        format_defaults_pane(ft, pane);
    }
    if let Some(pb) = paste_get_top(None) {
        format_defaults_paste_buffer(&mut *ft, &pb);
    }
}
unsafe fn format_defaults_session(mut ft: *mut format_tree, s_owner: &SessionRef) {
    (*ft).s = std::rc::Rc::downgrade(s_owner);
}
unsafe fn format_defaults_client(mut ft: *mut format_tree, c_owner: &ClientRef) {
    let _c: Option<ClientRef> = Some(c_owner.clone());
    if (*ft).s.upgrade().is_none() {
        (*ft).s = c_owner.attached_session();
    }
    (*ft).c = std::rc::Rc::downgrade(c_owner);
}
pub unsafe fn format_defaults_window(mut ft: *mut format_tree, w_owner: Option<&WindowRef>) {
    (*ft).w = w_owner.map_or_else(WindowWeak::new, WindowRef::downgrade);
}
unsafe fn format_defaults_winlink(mut ft: *mut format_tree, mut wl: refbox::Weak<winlink>) {
    if (*ft).w.upgrade().is_none() {
        format_defaults_window(ft, wl.get_unchecked().window_owner.as_ref());
    }
    (*ft).wl = wl;
}
pub unsafe fn format_defaults_pane(
    mut ft: *mut format_tree,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    if (*ft).w.upgrade().is_none() {
        (*ft).w = wp_owner.window_observer();
    }
    (*ft).wp = std::rc::Rc::downgrade(wp_owner);
    wp_owner.add_mode_formats(&mut *ft);
}
pub fn format_defaults_paste_buffer(ft: &mut format_tree, pb: &PasteBufferRef) {
    ft.pb = Some(pb.clone());
}
fn format_is_word_separator(ws: &CStr, gc: &grid_cell) -> bool {
    if utf8_cstrhas(ws, &gc.data) {
        return true;
    }
    if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
        return true;
    }
    gc.data.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int && gc.data.data[0] == b' '
}
pub unsafe fn format_grid_word(gd: &grid, x: u_int, y: u_int) -> Option<CString> {
    format_grid_word_cstring(gd, x, y)
}
pub(crate) unsafe fn format_grid_word_cstring(
    gd: &grid,
    mut x: u_int,
    mut y: u_int,
) -> Option<CString> {
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
    let mut ud: Vec<utf8_data> = Vec::new();
    let mut end: u_int = 0;
    let mut found: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut s = None;
    let separators = options_get_string(global_s_options, c"word-separators");
    let ws = separators.as_c_str();
    loop {
        grid_get_cell(gd, x, y, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0
            && format_is_word_separator(ws, &gc)
        {
            found = 1 as ::core::ffi::c_int;
            break;
        } else {
            if x == 0 as u_int {
                if y == 0 as u_int {
                    break;
                }
                let gl = grid_peek_line(gd, y.wrapping_sub(1 as u_int))?;
                if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
                    break;
                }
                y = y.wrapping_sub(1);
                x = grid_line_length(gd, y);
                if x == 0 as u_int {
                    break;
                }
            }
            x = x.wrapping_sub(1);
        }
    }
    loop {
        if found != 0 {
            end = grid_line_length(gd, y);
            if end == 0 as u_int || x == end.wrapping_sub(1 as u_int) {
                if y == gd.hsize.wrapping_add(gd.sy).wrapping_sub(1 as u_int) {
                    break;
                }
                let gl = grid_peek_line(gd, y)?;
                if !(gl.flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
                    break;
                }
                y = y.wrapping_add(1);
                x = 0 as u_int;
            } else {
                x = x.wrapping_add(1);
            }
        }
        found = 1 as ::core::ffi::c_int;
        grid_get_cell(gd, x, y, &mut gc);
        if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
            continue;
        }
        if format_is_word_separator(ws, &gc) {
            break;
        }
        ud.push(gc.data);
    }
    if !ud.is_empty() {
        ud.push(utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        });
        s = Some(utf8_tocstr_cstring(&ud));
    }
    s
}
pub unsafe fn format_grid_line(gd: &grid, y: u_int) -> Option<CString> {
    format_grid_line_cstring(gd, y)
}
pub(crate) unsafe fn format_grid_line_cstring(gd: &grid, mut y: u_int) -> Option<CString> {
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
    let mut ud: Vec<utf8_data> = Vec::new();
    let mut x: u_int = 0;
    let mut s = None;
    x = 0 as u_int;
    while x < grid_line_length(gd, y) {
        grid_get_cell(gd, x, y, &mut gc);
        if !(gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0) {
            if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
                let mut tab = gc.data;
                utf8_set(&mut tab, '\t' as i32 as u_char);
                ud.push(tab);
            } else {
                ud.push(gc.data);
            }
        }
        x = x.wrapping_add(1);
    }
    if !ud.is_empty() {
        ud.push(utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        });
        s = Some(utf8_tocstr_cstring(&ud));
        drop(ud);
    }
    s
}
pub(crate) unsafe fn format_grid_hyperlink_cstring(
    gd: &grid,
    mut x: u_int,
    mut y: u_int,
    s: &screen,
) -> Option<CString> {
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
    loop {
        grid_get_cell(gd, x, y, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        if x == 0 as u_int {
            return None;
        }
        x = x.wrapping_sub(1);
    }
    if s.hyperlinks.is_none() || gc.link == 0 as u_int {
        return None;
    }
    hyperlinks_get(s.hyperlinks.as_ref()?, gc.link).map(|link| link.uri.clone())
}

pub const FORMAT_TYPE_PANE: format_type = 3;

pub const FORMAT_TYPE_WINDOW: format_type = 2;

pub const FORMAT_TYPE_SESSION: format_type = 1;

pub const FORMAT_TYPE_UNKNOWN: format_type = 0;
