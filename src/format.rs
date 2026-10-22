use crate::arguments::args_escape_cstring;
use crate::cfg::cfg_files;
use crate::cmd::cmd_mouse_pane;
use crate::cmd::queue::{
    cmdq_get_client, cmdq_get_event, cmdq_get_target_client, cmdq_merge_formats, cmdq_print,
};
use crate::compat::strtonum::strtonum;
use crate::environ::{environ_find, environ_iter};
use crate::ffi::fnmatch::fnmatch_matches;
use crate::ffi::regex::RegexStorage;
use crate::format::bytes::format_message_with;
use crate::format::bytes::write_cstr;
use crate::format_draw::{format_trim_left_bytes, format_trim_right_bytes, format_width};
use crate::fuzzy::fuzzy_match_owned;
use crate::grid::{grid_get_cell, grid_line_length, grid_peek_line};
use crate::hyperlinks::hyperlinks_get;
use crate::job::{job_free, job_get_event, job_run};
use crate::log::{log_cstr, log_debug, log_get_level};
use crate::options::options_get_string;
use crate::options::OptionsScope;
use crate::paste::{
    paste_buffer_created, paste_buffer_data, paste_buffer_name, paste_get_top,
    paste_make_sample_cstring,
};
use crate::reactor::{evbuffer_add, evbuffer_new, evbuffer_pullup, evbuffer_readline};
use crate::regsub::regsub_cstring;
use crate::server::{marked_pane, server_check_marked};
use crate::shared::window::WindowWeak;
use crate::window::Window as _;
use crate::winlink::winlink_window;
use libc::{ctime_r, dirname, gethostname, getpwuid, getuid, localtime_r, strftime};

use crate::server_client::Client as _;
use crate::server_fn::server_status_client;
use crate::session::session_groups;
use crate::session::{sessions, Session};
use crate::shared::client::ClientRef;
use crate::shared::session::SessionRef;
use crate::shared::window::WindowRef;
use crate::sort::{
    sort_get_clients, sort_get_panes_window, sort_get_sessions, sort_get_winlinks_session,
};
use crate::status::status_get_range;
use crate::style::colour::{colour_force_rgb, colour_format_escape_for_client, colour_parse_cstr};
use crate::text::utf8::{utf8_cstrhas, utf8_pad_cstring, utf8_set, utf8_tocstr_cstring};
use crate::tmux::{get_timer, getversion, global_environ, socket_path_cstr, start_time};
use crate::window::{winlink_find_by_window, winlink_next, winlinks_minmax};
use crate::window_buffer::window_buffer_mode;
use crate::window_client::window_client_mode;
use crate::window_pane::WindowPane as _;
use crate::window_tree::window_tree_mode;
use std::ffi::{CStr, CString};

use crate::options::OPTIONS_TABLE_IS_HOOK;
use crate::shared::abi::*;
use crate::shared::client::CLIENT_UNATTACHEDFLAGS;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::environment::ENVIRON_HIDDEN;
use crate::shared::environment::{environ, environ_entry};
pub use crate::shared::format::{
    format_entry, format_entry_tree, format_job, format_job_tree, format_tree, format_type,
};
use crate::shared::format::{
    FORMAT_BASENAME, FORMAT_CHARACTER, FORMAT_CLIENTS, FORMAT_CLIENT_ENVIRON,
    FORMAT_CLIENT_TERMCAP, FORMAT_CLIENT_TERMFEAT, FORMAT_COLOUR, FORMAT_COLOUR_ESC_BG,
    FORMAT_COLOUR_ESC_FG, FORMAT_CYCLE, FORMAT_CYCLE_PERIOD, FORMAT_DIFFERENCE, FORMAT_DIRNAME,
    FORMAT_ENVIRON, FORMAT_EXPAND, FORMAT_EXPANDTIME, FORMAT_EXPAND_NOCYCLE, FORMAT_EXPAND_NOJOBS,
    FORMAT_EXPAND_TIME, FORMAT_FORCE, FORMAT_LENGTH, FORMAT_LITERAL, FORMAT_LOOP_LIMIT,
    FORMAT_MAX_PRECISION, FORMAT_MAX_REPEAT, FORMAT_MAX_REPEAT_SIZE, FORMAT_MAX_WIDTH,
    FORMAT_NOJOBS, FORMAT_NONE, FORMAT_NOT, FORMAT_NOT_NOT, FORMAT_OPTIONS, FORMAT_PANE,
    FORMAT_PANES, FORMAT_PRETTY, FORMAT_QUOTE_ARGUMENTS, FORMAT_QUOTE_SHELL, FORMAT_QUOTE_SHELL_SQ,
    FORMAT_QUOTE_STYLE, FORMAT_RELATIVE, FORMAT_REPEAT, FORMAT_SESSIONS, FORMAT_SESSION_NAME,
    FORMAT_STATUS, FORMAT_TIMESTRING, FORMAT_TIME_LIMIT, FORMAT_TIME_LOOP_CHECK, FORMAT_VERBOSE,
    FORMAT_WIDTH, FORMAT_WINDOW, FORMAT_WINDOWS, FORMAT_WINDOW_NAME,
};
use crate::shared::grid::*;
use crate::shared::job::JOB_NOWAIT;
use crate::shared::job::{job, job_update_callback, JobCompletion};
use crate::shared::pane::window_pane;
use crate::shared::paste::PasteBufferRef;
use crate::shared::posix_io::FNM_CASEFOLD;
use crate::shared::screen::screen;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::time::tm;
use crate::shared::tree::{RB_INF, RB_NEGINF};
use crate::shared::window::winlink;
use crate::shared::window::{WINDOW_SIZE_MANUAL, WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE};
use libc::{REG_EXTENDED, REG_ICASE};

/*
 * This module is the stable facade for format handling.  The private
 * implementation modules below keep the translated C ABI and the existing
 * `crate::format::*` paths in one place while separating responsibilities:
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
    format_add, format_add_cstr, format_add_time, format_create, format_each, format_get_pane,
    format_log_debug, format_merge,
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

pub(super) unsafe fn format_plugin_pane(ft: &mut format_tree) -> Option<crate::plugin::PaneId> {
    let pane = <std::rc::Rc<std::cell::RefCell<window_pane>>>::from_observer(&ft.wp)?;
    Some(crate::plugin::PaneId(pane.id()))
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
    pub modifier: [u8; 3],
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

    fn name(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.modifier).expect("modifier name is NUL-terminated")
    }
}
pub struct format_expand_state<'a> {
    pub ft: &'a mut format_tree,
    pub loop_0: u_int,
    pub start_time: uint64_t,
    pub flags: ::core::ffi::c_int,
    pub time: time_t,
    pub tm: tm,
}

impl<'a> format_expand_state<'a> {
    /// Fresh expansion over `ft`; this is what the public entry points use.
    pub fn new(ft: &'a mut format_tree, flags: ::core::ffi::c_int) -> Self {
        format_expand_state {
            ft,
            loop_0: 0,
            start_time: unsafe { get_timer() },
            flags,
            time: 0,
            tm: unsafe { ::core::mem::zeroed() },
        }
    }

    /// Nested expansion over the same tree (job commands, time formats).
    pub fn child(&mut self, flags: ::core::ffi::c_int) -> format_expand_state<'_> {
        format_expand_state {
            ft: &mut *self.ft,
            loop_0: self.loop_0,
            start_time: self.start_time,
            flags: self.flags | flags,
            time: self.time,
            tm: self.tm,
        }
    }

    /// Nested expansion over a loop iteration's own tree.
    pub fn with_tree<'b>(
        &self,
        ft: &'b mut format_tree,
        flags: ::core::ffi::c_int,
    ) -> format_expand_state<'b> {
        format_expand_state {
            ft,
            loop_0: self.loop_0,
            start_time: self.start_time,
            flags: self.flags | flags,
            time: self.time,
            tm: self.tm,
        }
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
fn format_logging(ft: &format_tree) -> ::core::ffi::c_int {
    (log_get_level() != 0 as ::core::ffi::c_int || ft.flags & FORMAT_VERBOSE != 0)
        as ::core::ffi::c_int
}
unsafe fn format_log1(
    es: &format_expand_state<'_>,
    mut from: *const ::core::ffi::c_char,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let ft = &*es.ft;
    if format_logging(ft) == 0 {
        return;
    }
    let s = format_message_with(write);
    log_debug(format_args!(
        "{}: {}",
        log_cstr(CStr::from_ptr(from)),
        log_cstr(&s)
    ));
    if let Some(item) = ft.item.upgrade().filter(|_| ft.flags & FORMAT_VERBOSE != 0) {
        cmdq_print(&item, |out| {
            out.write_all(b"#")?;
            out.write_all(&b"          "[..(es.loop_0 as usize).min(10)])?;
            write_cstr(out, &*s)
        });
    }
}
pub unsafe fn format_create_defaults(
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    c_owner: Option<&ClientRef>,
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::RefCell<window_pane>>>,
) -> Box<format_tree> {
    let queue_client = cmdq_get_client(item_handle.map(|item| item.borrow()).as_deref());
    let mut owner = format_create(
        queue_client.as_ref(),
        item_handle,
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *owner;
    format_defaults(ft, c_owner, s_owner, wl.clone(), wp_owner);
    owner
}
pub unsafe fn format_create_from_state(
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
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
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> Box<format_tree> {
    let (tc, target) = {
        let item = item_handle.borrow();
        (
            cmdq_get_target_client(Some(&*item)),
            crate::cmd::queue::cmdq_get_target(&item).clone(),
        )
    };
    format_create_from_state(Some(item_handle), tc.as_ref(), &target)
}
pub unsafe fn format_defaults(
    ft: &mut format_tree,
    c_owner: Option<&ClientRef>,
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::RefCell<window_pane>>>,
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
            (wl.try_borrow().expect("live winlink").idx) as u32
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
        && !crate::shared::rc::same(
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
        ft.type_0 = FORMAT_TYPE_PANE;
    } else if wl.is_alive() {
        ft.type_0 = FORMAT_TYPE_WINDOW;
    } else if s_owner.is_some() {
        ft.type_0 = FORMAT_TYPE_SESSION;
    } else {
        ft.type_0 = FORMAT_TYPE_UNKNOWN;
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
        wl.try_borrow().ok().and_then(|link| {
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
        format_defaults_paste_buffer(ft, &pb);
    }
}
fn format_defaults_session(ft: &mut format_tree, s_owner: &SessionRef) {
    ft.s = std::rc::Rc::downgrade(s_owner);
}
fn format_defaults_client(ft: &mut format_tree, c_owner: &ClientRef) {
    let _c: Option<ClientRef> = Some(c_owner.clone());
    if ft.s.upgrade().is_none() {
        ft.s = c_owner.attached_session();
    }
    ft.c = std::rc::Rc::downgrade(c_owner);
}
pub fn format_defaults_window(ft: &mut format_tree, w_owner: Option<&WindowRef>) {
    ft.w = w_owner.map_or_else(WindowWeak::new, WindowRef::downgrade);
}
fn format_defaults_winlink(ft: &mut format_tree, mut wl: refbox::Weak<winlink>) {
    if ft.w.upgrade().is_none() {
        format_defaults_window(ft, winlink_window(&wl).as_ref());
    }
    ft.wl = wl;
}
pub unsafe fn format_defaults_pane(
    ft: &mut format_tree,
    wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
) {
    if ft.w.upgrade().is_none() {
        ft.w = wp_owner.window_observer();
    }
    ft.wp = std::rc::Rc::downgrade(wp_owner);
    wp_owner.add_mode_formats(ft);
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
    let separators =
        OptionsScope::GlobalSession.with_local(|oo| options_get_string(oo, c"word-separators"));
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
