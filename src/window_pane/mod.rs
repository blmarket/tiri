//! Pane implementation. Window state is accessed through the Window trait.
use crate::src::format::bytes::nullable_cstr;
use crate::src::server_client::ClientRegistry;
use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::window::WindowRef;
use crate::src::shared::window::WindowWeak;
use crate::src::window::*;
use std::os::fd::AsRawFd;
use std::time::Duration;
mod api;
mod border;
mod capture;
mod format;
mod input;
mod keys;
mod lifecycle;
mod mode_visuals;
mod model;
mod mouse;
pub(crate) mod observability;
mod process;
mod render;
mod sort;
mod spawning;
pub(crate) use format::format_without_pane;
pub use model::window_pane;
mod pane_sync;
use self::input::{input_parse_buffer, input_parse_pane};
use crate::src::arguments::args_has;
use crate::src::cmd::cmd_mouse_at;
use crate::src::cmd::queue::{cmdq_continue, cmdq_get_client};
use crate::src::compat::strtonum::strtonum;
use crate::src::events::{events_fire, events_fire_pane};
use crate::src::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_uint, event_payload_set_window,
};
use crate::src::ffi::libc::{fnmatch, gethostname, memset};
#[cfg(target_os = "linux")]
use crate::src::ffi::libc::{getpid, kill};
use crate::src::ffi::regex::RegexStorage;
#[cfg(target_os = "linux")]
use crate::src::ffi::utempter::utempter_remove_record;
use crate::src::file::{file_cancel, file_read_with_cmdq_wait_init};
use crate::src::format::bytes::write_cstr;
use crate::src::grid::grid_cells_look_equal;
use crate::src::grid::view::grid_view_string_cells_bytes;
use crate::src::input_keys::input_key_pane;
use crate::src::log::{fatal, fatalx, log_bytes, log_cstr, log_cstr_n, log_debug};
use crate::src::options::options_get_number;
use crate::src::options::options_owner_ptr;
use crate::src::prompt::{
    prompt_closed, prompt_create, prompt_free, prompt_incremental_start, prompt_key, prompt_mouse,
    prompt_set_options, prompt_type_string, prompt_update,
};
use crate::src::reactor::BufferEvent;
use crate::src::reactor::{
    bufferevent_disable, bufferevent_enable, bufferevent_new, bufferevent_write, evbuffer_drain,
    evbuffer_get_length, evbuffer_pullup,
};
use crate::src::screen::{screen_init, screen_resize, screen_set_default_cursor, screen_set_title};
pub use api::WindowPane;

use crate::src::server::{marked_pane, server_check_marked};
use crate::src::server_client::Client;
use crate::src::server_fn::{
    server_destroy_pane, server_kill_pane, server_redraw_window, server_redraw_window_borders,
    server_status_window,
};
use crate::src::shared::pane::window_pane_tree;
use crate::src::shared::prompt::prompt_create_data;
use crate::src::spawn::spawn_editor_finish;
use crate::src::status::status_at_line;
use crate::src::style::colour::{
    colour_palette_from_option, colour_palette_get, colour_palette_init, colour_totheme,
};
use crate::src::style::{style_ranges_init, style_set_scrollbar_style_from_option};
use crate::src::tmux::setblocking;
use crate::src::tty::tty_default_colours;
use crate::src::window_copy::{window_copy_mode, window_view_mode};
use tiri_buffer::SegmentedBuf;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

use crate::src::shared::abi::*;
use crate::src::shared::arguments::args;
use crate::src::shared::client::{client_file, client_file_cb};
use crate::src::shared::client::{CLIENT_DEAD, CLIENT_EXITED, CLIENT_UNATTACHEDFLAGS};
use crate::src::shared::colour::*;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::display::*;
use crate::src::shared::event::*;
use crate::src::shared::event::{EV_READ, EV_WRITE};
use crate::src::shared::grid::*;
use crate::src::shared::input::input_ctx;
use crate::src::shared::key::*;
use crate::src::shared::layout::layout_geometry;
use crate::src::shared::layout::*;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::mouse::{mouse_event, MOUSE_BUTTON_1, MOUSE_MASK_BUTTONS, MOUSE_MASK_DRAG};
use crate::src::shared::options::options;
use crate::src::shared::pane::{
    pane_output_data, window_pane_offset, window_pane_resize, window_pane_resizes, PANE_CHANGED,
    PANE_DESTROYED, PANE_EMPTY, PANE_EXITED, PANE_FOCUSED, PANE_INPUTOFF, PANE_REDRAW,
    PANE_REDRAWSCROLLBAR, PANE_SCROLLBARS_ALWAYS, PANE_SCROLLBARS_AUTOHIDE, PANE_SCROLLBARS_LEFT,
    PANE_SCROLLBARS_MODAL, PANE_STATUSREADY, PANE_STATUS_BOTTOM, PANE_STATUS_OFF, PANE_STATUS_TOP,
    PANE_STYLECHANGED, PANE_THEMECHANGED, PANE_UNSEENCHANGES,
};
use crate::src::shared::pane::{
    window_pane_history, window_pane_modes, window_pane_prompt, PaneScreenSource,
};
use crate::src::shared::posix_io::FNM_CASEFOLD;
use crate::src::shared::posix_terminal::winsize;
use crate::src::shared::prompt::*;
use crate::src::shared::prompt::{prompt_free_cb, prompt_result, PROMPT_CLOSE};
use crate::src::shared::screen::{screen, MODE_BRACKETPASTE, MODE_FOCUSON, MODE_THEME_UPDATES};
#[cfg(target_os = "linux")]
use crate::src::shared::signal::SIGCHLD;
use crate::src::shared::spawn::spawn_editor_state;
use crate::src::shared::status::status_prompt_input_cb;
use crate::src::shared::style::*;
pub use crate::src::shared::window::{
    window, window_mode, window_mode_entry, window_winlinks, windows, winlink, winlink_stack,
    winlinks,
};
use crate::src::shared::window::{WINDOW_MODE_NO_STACK, WINDOW_PANE_NO_MODE};
use libc::{REG_EXTENDED, REG_ICASE};

struct window_pane_input_data {
    item: Weak<UnsafeCell<cmdq_item>>,
    client: Option<ClientRef>,
    wp: u_int,
    file: Weak<UnsafeCell<client_file>>,
}

impl window_pane_input_data {
    fn into_callback(self: Box<Self>) -> client_file_cb {
        Some(Box::new(move |event| unsafe {
            window_pane_input_callback(
                &self,
                event.error,
                event.closed,
                event.buffer.expect("read callback buffer"),
            );
        }))
    }
}

impl Drop for window_pane_input_data {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            (client).release();
        }
    }
}

thread_local! {
    static WINDOW_PANES: std::cell::RefCell<window_pane_tree> = const {
        std::cell::RefCell::new(window_pane_tree {
            storage: std::collections::BTreeMap::new(),
        })
    };
    static NEXT_WINDOW_PANE_ID: std::cell::Cell<u_int> = const { std::cell::Cell::new(0) };
    static NEXT_ACTIVE_POINT: std::cell::Cell<u_int> = const { std::cell::Cell::new(0) };
}

/// The pane index owns the strong pane records. Each call borrows it only for
/// its own duration and returns owned values, so removed owners are dropped
/// by the caller after the borrow has ended.
impl window_pane_tree {
    /// An existing entry for the same id wins and is returned.
    pub fn insert(
        pane: Rc<std::cell::UnsafeCell<window_pane>>,
    ) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
        let id = unsafe { (*pane.get()).id };
        WINDOW_PANES.with_borrow_mut(|tree| match tree.storage.entry(id) {
            std::collections::btree_map::Entry::Occupied(entry) => Some(entry.get().clone()),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(pane);
                None
            }
        })
    }

    /// Removes and returns the indexed owner only if it is `pane_owner`.
    pub fn remove(
        pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    ) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
        let id = unsafe { (*pane_owner.get()).id };
        WINDOW_PANES.with_borrow_mut(|tree| {
            if !tree
                .storage
                .get(&id)
                .is_some_and(|owner| Rc::ptr_eq(owner, pane_owner))
            {
                return None;
            }
            tree.storage.remove(&id)
        })
    }

    pub fn find(id: u_int) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
        WINDOW_PANES.with_borrow(|tree| tree.storage.get(&id).cloned())
    }

    /// Retains every registered pane so callers may iterate while panes are
    /// destroyed.
    pub fn snapshot() -> Vec<Rc<std::cell::UnsafeCell<window_pane>>> {
        WINDOW_PANES.with_borrow(|tree| tree.storage.values().cloned().collect())
    }

    /// Reset the pane registry during fresh server startup.
    pub fn clear() {
        let removed = WINDOW_PANES.with_borrow_mut(|tree| std::mem::take(&mut tree.storage));
        // The borrow ended above, so dropping owners cannot re-enter the index.
        drop(removed);
    }

    fn alloc_id() -> u_int {
        NEXT_WINDOW_PANE_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        })
    }

    /// Returns the current activity point and advances it.
    pub fn next_active_point() -> u_int {
        NEXT_ACTIVE_POINT.with(|next| {
            let point = next.get();
            next.set(point.wrapping_add(1));
            point
        })
    }

    #[cfg(test)]
    pub(crate) fn peek_active_point() -> u_int {
        NEXT_ACTIVE_POINT.with(std::cell::Cell::get)
    }
}

unsafe fn window_fire_pane_moved(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    old_w_owner: &WindowRef,
    mut old_idx: ::core::ffi::c_int,
    new_w_owner: &WindowRef,
    mut new_idx: ::core::ffi::c_int,
) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_pane(wp_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_pane(&mut ep, c"pane", Rc::clone(wp_owner));
    event_payload_set_window(&mut ep, c"window", WindowRef::clone(new_w_owner));
    event_payload_set_window(&mut ep, c"old_window", WindowRef::clone(old_w_owner));
    event_payload_set_window(&mut ep, c"new_window", WindowRef::clone(new_w_owner));
    if old_idx != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"old_window_index", old_idx);
    }
    if new_idx != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"window_index", new_idx);
        event_payload_set_int(&mut ep, c"new_window_index", new_idx);
    }
    events_fire(c"pane-moved", ep);
}

unsafe fn window_fire_pane_mode_changed(
    name: &CStr,
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    previous: Option<&CStr>,
    current: Option<&CStr>,
    mut entered: ::core::ffi::c_int,
) {
    let current: *const ::core::ffi::c_char = current.map_or(std::ptr::null(), CStr::as_ptr);
    let previous: *const ::core::ffi::c_char = previous.map_or(std::ptr::null(), CStr::as_ptr);
    let mut wp = wp_owner.get();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
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
    if !current.is_null() {
        event_payload_set_string(&mut ep, c"current_mode", |out| {
            write_cstr(out, nullable_cstr(current))
        });
    }
    if !previous.is_null() {
        event_payload_set_string(&mut ep, c"previous_mode", |out| {
            write_cstr(out, nullable_cstr(previous))
        });
    }
    event_payload_set_int(&mut ep, c"mode_entered", entered);
    events_fire(name, ep);
}

unsafe fn window_fire_pane_prompt(
    name: &CStr,
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    mut type_0: prompt_type,
) {
    let mut wp = wp_owner.get();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
        idx: 0,
    };
    let type_string = prompt_type_string(type_0);
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
    event_payload_set_string(&mut ep, c"prompt_type", |out| {
        out.write_all(type_string.to_bytes())
    });
    events_fire(name, ep);
}

unsafe fn window_pane_destroy_ready(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> ::core::ffi::c_int {
    let mut wp = wp_owner.get();
    if (*wp).pipe_fd.is_some()
        && (*wp)
            .pipe_event
            .with_ptr(|event| unsafe { evbuffer_get_length(&(*event).output) != 0 as size_t })
            .unwrap_or(false)
    {
        return 0 as ::core::ffi::c_int;
    }
    if (*wp)
        .fd
        .as_ref()
        .and_then(|fd| tiri_rt::unix::bytes_available(std::os::fd::AsFd::as_fd(fd)).ok())
        .is_some_and(|n| n > 0)
    {
        return 0 as ::core::ffi::c_int;
    }
    if !(*wp).flags & PANE_EXITED != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if (*wp).wait_item.strong_count() != 0 && !(*wp).flags & PANE_STATUSREADY != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if (*wp).editor.is_some() && !(*wp).flags & PANE_STATUSREADY != 0 {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}

unsafe fn window_pane_add_ref(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    from: &CStr,
) -> std::rc::Rc<std::cell::UnsafeCell<window_pane>> {
    let mut wp = wp_owner.get();
    let owner = Rc::clone(wp_owner);
    log_debug(format_args!(
        "retain pane %{} ({})",
        { (*wp).id },
        log_cstr(from)
    ));
    owner
}

unsafe fn window_pane_remove_ref(
    owner: std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    from: &CStr,
) {
    let wp = owner.get();
    log_debug(format_args!(
        "release pane %{} ({})",
        { (*wp).id },
        log_cstr(from)
    ));
    drop(owner);
}

unsafe fn window_pane_send_resize(wp: &window_pane, sx: u_int, sy: u_int) {
    let mut ws: winsize = winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    if wp.fd.is_none() {
        return;
    }
    log_debug(format_args!(
        "{}: %{} resize to {},{}",
        "window_pane_send_resize",
        { wp.id },
        { sx },
        { sy }
    ));
    memset(
        &raw mut ws as *mut ::core::ffi::c_void,
        0 as ::core::ffi::c_int,
        ::core::mem::size_of::<winsize>() as size_t,
    );
    ws.ws_col = sx as ::core::ffi::c_ushort;
    ws.ws_row = sy as ::core::ffi::c_ushort;
    let parent = wp.window.upgrade().expect("live pane parent");
    let (xpixel, ypixel) = parent.cell_size();
    drop(parent);
    ws.ws_xpixel = xpixel.wrapping_mul(ws.ws_col as u_int) as ::core::ffi::c_ushort;
    ws.ws_ypixel = ypixel.wrapping_mul(ws.ws_row as u_int) as ::core::ffi::c_ushort;
    if crate::src::shared::terminal::set_size(wp.fd.as_ref().map_or(-1, AsRawFd::as_raw_fd), &ws)
        == -(1 as ::core::ffi::c_int)
    {
        fatal(|out| out.write_all(b"ioctl failed"));
    }
}

unsafe fn window_pane_update_focus(wp_owner: Option<&Rc<std::cell::UnsafeCell<window_pane>>>) {
    let Some(pane) = wp_owner else {
        return;
    };
    if (*pane.get()).flags & PANE_EXITED != 0 {
        return;
    }
    let parent = pane.window_observer().upgrade().expect("live pane parent");
    let focused = parent.pane_is_focused(pane);
    pane.update_focus(focused);
    drop(parent);
}

unsafe fn window_pane_at_index(
    w: &WindowRef,
    idx: u_int,
) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
    w.pane_at_index(idx).upgrade()
}

unsafe fn window_pane_next_by_number(
    w: &WindowRef,
    pane: Option<&Rc<std::cell::UnsafeCell<window_pane>>>,
    n: u_int,
) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
    w.pane_by_number(pane, n, false).upgrade()
}

unsafe fn window_pane_previous_by_number(
    w: &WindowRef,
    pane: Option<&Rc<std::cell::UnsafeCell<window_pane>>>,
    n: u_int,
) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
    w.pane_by_number(pane, n, true).upgrade()
}

unsafe fn window_pane_index(pane: &Rc<std::cell::UnsafeCell<window_pane>>) -> Option<u32> {
    (*pane.get())
        .window_handle()?
        .pane_index(&Rc::downgrade(pane))
}

unsafe fn window_pane_printable_flags(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> std::ffi::CString {
    let mut wp = wp_owner.get();
    let window = (*wp).window_handle().expect("live window");
    let mut flags: [::core::ffi::c_char; 32] = [0; 32];
    let mut pos: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if window.active_pane().ptr_eq(&Rc::downgrade(wp_owner)) {
        let fresh12 = pos;
        pos += 1;
        flags[fresh12 as usize] = '*' as i32 as ::core::ffi::c_char;
    }
    if window.last_active_pane().ptr_eq(&Rc::downgrade(wp_owner)) {
        let fresh13 = pos;
        pos += 1;
        flags[fresh13 as usize] = '-' as i32 as ::core::ffi::c_char;
    }
    flags[pos as usize] = '\0' as i32 as ::core::ffi::c_char;
    std::ffi::CStr::from_ptr(flags.as_ptr()).to_owned()
}

unsafe fn window_pane_find_by_id_str(s: &CStr) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
    if s.to_bytes().first() != Some(&b'%') {
        return None;
    }
    let Ok(id) = strtonum(&s[1..], 0, UINT_MAX as i64) else {
        return None;
    };
    let id = id as u_int;
    window_pane_find_by_id(id)
}

unsafe fn window_pane_find_by_id(id: u_int) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
    window_pane_tree::find(id)
}

/// Retain a live pane for an operation through a nonowning model link.
/// Logical destruction invalidates observers even if another owner keeps the
/// allocation alive. This is not an accessor for final-drop cleanup.
unsafe fn window_pane_upgrade(
    pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    let owner = pane.upgrade()?;
    if (*owner.get()).flags & PANE_DESTROYED != 0 {
        return None;
    }
    Some(owner)
}

/// Resolve a live mode entry against its pane-owned stack before observing it.
unsafe fn window_pane_mode_weak(
    wme: refbox::Weak<window_mode_entry>,
) -> refbox::Weak<window_mode_entry> {
    let pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let storage = &(*pane_owner.get()).modes;
    storage
        .iter()
        .find(|entry| wme.is(entry))
        .expect("mode entry belongs to its pane")
        .downgrade()
}

unsafe fn window_pane_mode_push(
    wp: &mut window_pane,
    entry: refbox::RefBox<window_mode_entry>,
) -> refbox::Weak<window_mode_entry> {
    let storage = &mut wp.modes;
    let weak = entry.downgrade();
    storage.push(entry);
    weak
}

unsafe fn window_pane_mode_remove(
    wp_value: &mut window_pane,
    wme: refbox::Weak<window_mode_entry>,
) -> Option<refbox::RefBox<window_mode_entry>> {
    let wp: *mut window_pane = wp_value as *mut _;
    let storage = &mut (*wp).modes;
    let index = storage.iter().position(|entry| wme.is(entry))?;
    Some(storage.remove(index))
}

unsafe fn window_pane_mode_promote(
    wp_value: &mut window_pane,
    wme: refbox::Weak<window_mode_entry>,
) {
    let wp: *mut window_pane = wp_value as *mut _;
    if (*wp).active_mode_entry() == wme {
        return;
    }
    let Some(entry) = window_pane_mode_remove(&mut *(wp), wme.clone()) else {
        return;
    };
    window_pane_mode_push(&mut *wp, entry);
}

unsafe fn window_pane_create(
    w_owner: &WindowRef,
    mut sx: u_int,
    mut sy: u_int,
    mut hlimit: u_int,
) -> Rc<std::cell::UnsafeCell<window_pane>> {
    let mut wp: *mut window_pane = ::core::ptr::null_mut::<window_pane>();
    let mut host: [::core::ffi::c_char; 65] = [0; 65];
    let owner = window_pane::new();
    wp = owner.get();
    (*wp).window = WindowRef::downgrade(w_owner).clone();
    (*wp).options = Some(crate::src::options::options_create_owned(Some(
        crate::src::options::OptionsScope::Window(WindowRef::downgrade(w_owner)),
    )));
    (*wp).flags = PANE_STYLECHANGED;
    (*wp).cmd_status = -(1 as ::core::ffi::c_int);
    (*wp).id = window_pane_tree::alloc_id();
    window_pane_tree::insert(owner.clone());
    (*wp).fd = None;
    (*wp).modes = window_pane_modes::default();
    (*wp).resize_queue = window_pane_resizes::default();
    (*wp).sx = sx;
    (*wp).sy = sy;
    (*wp).pipe_fd = None;
    (*wp).control_bg = -(1 as ::core::ffi::c_int);
    (*wp).control_fg = -(1 as ::core::ffi::c_int);
    style_set_scrollbar_style_from_option(
        &raw mut (*wp).scrollbar_style,
        options_owner_ptr(&mut (*wp).options).map_or(std::ptr::null_mut(), |options| options),
    );
    colour_palette_init(&mut (*wp).palette);
    colour_palette_from_option(
        Some(&mut (*wp).palette),
        options_owner_ptr(&mut (*wp).options).map_or(std::ptr::null_mut(), |options| options),
    );
    screen_init(&mut (*wp).base, sx, sy, hlimit);
    (*wp).screen_source = PaneScreenSource::Base;
    window_pane_default_cursor(&owner);
    screen_init(&mut (*wp).status_screen, 1 as u_int, 1 as u_int, 0 as u_int);
    style_ranges_init(&raw mut (*wp).border_status_line.ranges);
    if gethostname(
        &raw mut host as *mut ::core::ffi::c_char,
        ::core::mem::size_of::<[::core::ffi::c_char; 65]>() as size_t,
    ) == 0 as ::core::ffi::c_int
    {
        screen_set_title(
            &mut (*wp).base,
            CStr::from_ptr(host.as_ptr()),
            0 as ::core::ffi::c_int,
        );
    }
    owner
}

unsafe fn window_pane_wait_finish(wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let mut wp = wp_owner.get();
    let item_owner = std::mem::take(&mut (*wp).wait_item).upgrade();
    let Some(item_owner) = item_owner else { return };
    let item = item_owner.get();
    let _c: Option<ClientRef> = None;
    let mut retval: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if (*wp).flags & PANE_STATUSREADY != 0 {
        if (*wp).status & 0x7f as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            retval = ((*wp).status & 0xff00 as ::core::ffi::c_int) >> 8 as ::core::ffi::c_int;
        } else if (((*wp).status & 0x7f as ::core::ffi::c_int) + 1 as ::core::ffi::c_int)
            as ::core::ffi::c_schar as ::core::ffi::c_int
            >> 1 as ::core::ffi::c_int
            > 0 as ::core::ffi::c_int
        {
            retval = ((*wp).status & 0x7f as ::core::ffi::c_int) + 128 as ::core::ffi::c_int;
        }
    }
    let c_owner = cmdq_get_client((item).as_ref());
    if let Some(client) = c_owner.as_ref() {
        if client.attached_session().upgrade().is_none() {
            client.set_return_value(retval);
        }
    }
    cmdq_continue(&item_owner);
}

unsafe fn window_pane_free_modes(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    while !(*wp).modes.is_empty() {
        wme = (*wp).active_mode_entry();
        let entry =
            window_pane_mode_remove(&mut *(wp), wme.clone()).expect("mode entry is owned by pane");
        let next = (*wp).active_mode_entry();
        (*wp).screen_source = if !next.is_alive() {
            PaneScreenSource::Base
        } else {
            PaneScreenSource::Mode((*wp).active_mode_entry())
        };
        wme.get_unchecked()
            .mode
            .free
            .expect("non-null function pointer")(wme);
        drop(entry);
    }
    (*wp).screen_source = PaneScreenSource::Base;
}

unsafe fn window_pane_scrollbar_timer(owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    window_pane_scrollbar_hide(owner);
}

unsafe fn window_pane_scrollbar_auto_hide(wp: &window_pane) -> ::core::ffi::c_int {
    (((wp.window_handle().as_ref()).expect("live window")).scrollbar_mode()
        == PANE_SCROLLBARS_MODAL
        || ((wp.window_handle().as_ref()).expect("live window")).scrollbar_mode()
            == PANE_SCROLLBARS_AUTOHIDE) as ::core::ffi::c_int
}

unsafe fn window_pane_scrollbar_overlay_visible(wp: &window_pane) -> ::core::ffi::c_int {
    (window_pane_scrollbar_overlay(wp) != 0 && window_pane_scrollbar_visible(wp) != 0)
        as ::core::ffi::c_int
}

unsafe fn window_pane_scrollbar_redraw(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    if window_pane_scrollbar_visible(&*wp) == 0 {
        return;
    }
    if window_pane_scrollbar_overlay_visible(&*wp) != 0 {
        (*wp).flags |= PANE_REDRAW;
        return;
    }
    (*wp).flags |= PANE_REDRAWSCROLLBAR;
}

unsafe fn window_pane_scrollbar_redraw_visibility(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let wp = pane_owner.get();
    (*wp)
        .window_handle()
        .expect("pane window")
        .invalidate_scene();
    (*wp).flags |= PANE_REDRAW;
    server_redraw_window(((*wp).window_handle().as_ref()).expect("live window"));
}

unsafe fn window_pane_destroy(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    window_pane_wait_finish(pane_owner);
    spawn_editor_finish(pane_owner);
    let owner = window_pane_tree::remove(pane_owner).expect("registered pane owner");
    (*wp).flags |= PANE_DESTROYED;
    window_pane_clear_prompt(&owner);
    window_pane_free_modes(&owner);
    pane_owner.clear_sync_dirty();
    #[cfg(target_os = "linux")]
    if (*wp).fd.is_some() {
        utempter_remove_record((*wp).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd));
        kill(getpid(), SIGCHLD);
    }
    // Empty panes have stream buffers and an input parser without a PTY.
    std::mem::take(&mut (*wp).event).free();
    drop((*wp).fd.take());
    if let Some(ictx) = (*wp).ictx.take() {
        drop(ictx);
    }
    if (*wp).pipe_fd.is_some() {
        std::mem::take(&mut (*wp).pipe_event).free();
        drop((*wp).pipe_fd.take());
    }
    drop((*wp).resize_timer.take());
    drop((*wp).sync_timer.take());
    drop((*wp).sb_auto_timer.take());
    window_pane_clear_resizes(&mut *wp, ::core::ptr::null_mut::<window_pane_resize>());
    window_pane_remove_ref(owner, c"window_pane_destroy");
}

unsafe fn window_pane_read_callback(owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = owner.get();
    let (input, pipe, base, pipe_offset, has_pipe) = {
        let pane = &*owner.get();
        (
            pane.event.clone(),
            pane.pipe_event.clone(),
            pane.base_offset,
            pane.pipe_offset,
            pane.pipe_fd.is_some(),
        )
    };
    let mut wpo: *mut window_pane_offset = &raw mut (*wp).pipe_offset;
    let size = input.input_len().unwrap_or(0);
    if has_pipe {
        let data = input
            .with_ptr(|event| unsafe {
                pane_output_data(&mut (*event).input, base, &pipe_offset).to_vec()
            })
            .unwrap_or_default();
        let new_size = data.len();
        if new_size > 0 as size_t {
            let _ = pipe.write(&data);
            window_pane_update_used_data(owner, wpo, new_size);
        }
    }
    log_debug(format_args!("%{} has {} bytes", { (*wp).id }, { size }));
    let mut registry_c_owner = ClientRegistry::first();
    while let Some(client) = registry_c_owner {
        client.control_write_output(owner);
        registry_c_owner = ClientRegistry::next(&client);
    }
    input_parse_pane(owner);
    let _ = (*wp).event.with_ptr(|event| unsafe {
        bufferevent_disable(event, EV_READ as ::core::ffi::c_short);
    });
}

unsafe fn window_pane_error_callback(owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = owner.get();
    log_debug(format_args!("%{} error", { (*wp).id }));
    (*wp).flags |= PANE_EXITED;
    if window_pane_destroy_ready(owner) != 0 {
        server_destroy_pane(owner, 1);
    }
}

unsafe fn window_pane_set_event(wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let mut wp = wp_owner.get();
    let read_observer = std::rc::Rc::downgrade(wp_owner);
    let error_observer = read_observer.clone();
    setblocking(
        (*wp).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
        0 as ::core::ffi::c_int,
    );
    let stream = bufferevent_new(
        (*wp).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
        bufferevent_data_callback(move |_| unsafe {
            if let Some(owner) = read_observer.upgrade() {
                window_pane_read_callback(&owner);
            }
        }),
        None,
        bufferevent_event_callback(move |_, _| unsafe {
            if let Some(owner) = error_observer.upgrade() {
                window_pane_error_callback(&owner);
            }
        }),
    );
    if stream.is_null() {
        fatalx(|out| out.write_all(b"out of memory"));
    }
    (*wp).event = crate::src::reactor::StreamHandle::from_ptr(stream);
    (*wp).ictx = Some(input_init(
        Some(wp_owner),
        stream,
        crate::src::shared::input::InputPalette::Pane(std::rc::Rc::downgrade(wp_owner)),
        None,
    ));
    let _ = (*wp).event.with_ptr(|event| unsafe {
        bufferevent_enable(event, (EV_READ | EV_WRITE) as ::core::ffi::c_short);
    });
}

fn window_pane_clear_resizes(wp: &mut window_pane, mut except: *mut window_pane_resize) {
    wp.clear_resizes_except(except);
}

unsafe fn window_pane_resize(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    sx: u_int,
    sy: u_int,
) {
    let wp = pane_owner.get();
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
        idx: 0,
    };
    if sx == (*wp).sx && sy == (*wp).sy {
        return;
    }
    pane_owner.stop_sync();
    let old_sx = (*wp).sx;
    let old_sy = (*wp).sy;
    (*wp).push_resize(window_pane_resize {
        sx,
        sy,
        osx: old_sx,
        osy: old_sy,
    });
    (*wp).sx = sx;
    (*wp).sy = sy;
    log_debug(format_args!(
        "{}: %{} resize {}x{}",
        "window_pane_resize",
        { (*wp).id },
        { sx },
        { sy }
    ));
    let reflow = (*wp).base.saved_grid.is_none() as ::core::ffi::c_int;
    screen_resize(&mut (*wp).base, sx, sy, reflow);
    wme = (*wp).active_mode_entry();
    if wme.is_alive() && wme.get_unchecked().mode.resize.is_some() {
        wme.get_unchecked()
            .mode
            .resize
            .expect("non-null function pointer")(wme, sx, sy);
    }
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_pane(pane_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(pane_owner));
    event_payload_set_window(
        &mut ep,
        c"window",
        WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_uint(&mut ep, c"width", sx);
    event_payload_set_uint(&mut ep, c"height", sy);
    event_payload_set_uint(&mut ep, c"old_width", old_sx);
    event_payload_set_uint(&mut ep, c"old_height", old_sy);
    events_fire(c"pane-resized", ep);
}

unsafe fn window_pane_set_mode(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    source_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
    mode: &'static window_mode,
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    mut fs: *mut cmd_find_state,
    mut args: *mut args,
) -> ::core::ffi::c_int {
    let wp = pane_owner.get();
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mode_screen: *mut screen;
    let mut name: *const ::core::ffi::c_char = mode.name.as_ptr();
    let mut oname: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if let Some(active) = (*wp).active_mode() {
        if std::ptr::eq(active, mode) {
            return 1 as ::core::ffi::c_int;
        }
        if active.flags & WINDOW_MODE_NO_STACK != 0 {
            window_pane_reset_mode(pane_owner);
        }
    }
    if let Some(active) = (*wp).active_mode() {
        oname = active.name.as_ptr();
    }
    let existing = (*wp)
        .modes
        .iter()
        .find(|entry| std::ptr::eq(entry.get_unchecked().mode, mode))
        .map_or_else(refbox::Weak::new, refbox::RefBox::downgrade);
    if existing.is_alive() {
        wme = existing.clone();
        window_pane_mode_promote(&mut *(wp), wme.clone());
        mode_screen = wme
            .get_unchecked()
            .mode
            .display_screen
            .expect("mode display screen getter")(wme.clone());
    } else {
        // The pane owns the entry; callbacks observe it through its weak identity.
        let entry = refbox::RefBox::new(window_mode_entry {
            wp: std::rc::Rc::downgrade(pane_owner),
            swp: source_owner.map(std::rc::Rc::downgrade).unwrap_or_default(),
            mode,
            boxed_data: None,
            data_owner: None,
            prefix: 1,
            kill: 0,
        });
        wme = window_pane_mode_push(&mut *wp, entry);
        mode_screen =
            wme.get_unchecked()
                .mode
                .init
                .expect("non-null function pointer")(wme.clone(), item_handle, fs, args);
        if mode_screen.is_null() {
            drop(
                window_pane_mode_remove(&mut *(wp), wme.clone())
                    .expect("mode entry is owned by pane"),
            );
            return 1 as ::core::ffi::c_int;
        }
    }
    wme.get_mut_unchecked().kill = if !args.is_null() {
        args_has(args, 'k' as i32 as u_char)
    } else {
        0 as ::core::ffi::c_int
    };
    assert!(!mode_screen.is_null(), "active mode has a screen");
    (*wp).screen_source = PaneScreenSource::Mode((*wp).active_mode_entry());
    (*wp).flags |= PANE_REDRAW | PANE_REDRAWSCROLLBAR | PANE_CHANGED;
    ((*wp).window_handle().as_ref())
        .expect("live window")
        .refit();
    server_redraw_window_borders(((*wp).window_handle().as_ref()).expect("live window"));
    server_status_window(((*wp).window_handle().as_ref()).expect("live window"));
    window_fire_pane_mode_changed(
        c"pane-mode-entered",
        pane_owner,
        nullable_cstr(oname),
        nullable_cstr(name),
        1 as ::core::ffi::c_int,
    );
    window_fire_pane_mode_changed(
        c"pane-mode-changed",
        pane_owner,
        nullable_cstr(oname),
        nullable_cstr(name),
        1 as ::core::ffi::c_int,
    );
    0 as ::core::ffi::c_int
}

unsafe fn window_pane_reset_mode(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut next: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut kill_0: ::core::ffi::c_int = 0;
    let mut name: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut p: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if (*wp).modes.is_empty() {
        return;
    }
    wme = (*wp).active_mode_entry();
    p = wme.get_unchecked().mode.name.as_ptr();
    kill_0 = wme.get_unchecked().kill;
    let entry =
        window_pane_mode_remove(&mut *(wp), wme.clone()).expect("mode entry is owned by pane");
    next = (*wp).active_mode_entry();
    (*wp).screen_source = if !next.is_alive() {
        PaneScreenSource::Base
    } else {
        PaneScreenSource::Mode((*wp).active_mode_entry())
    };
    wme.get_unchecked()
        .mode
        .free
        .expect("non-null function pointer")(wme);
    drop(entry);
    next = (*wp).active_mode_entry();
    (*wp).screen_source = if !next.is_alive() {
        PaneScreenSource::Base
    } else {
        PaneScreenSource::Mode((*wp).active_mode_entry())
    };
    if !next.is_alive() {
        (*wp).flags &= !PANE_UNSEENCHANGES;
        log_debug(format_args!("{}: no next mode", "window_pane_reset_mode"));
    } else {
        log_debug(format_args!(
            "{}: next mode is {}",
            "window_pane_reset_mode",
            crate::src::log::log_bytes(next.get_unchecked().mode.name.to_bytes())
        ));
        assert!(!(*wp).screen_ptr().is_null(), "restored mode has a screen");
        if next.get_unchecked().mode.resize.is_some() {
            next.get_unchecked()
                .mode
                .resize
                .expect("non-null function pointer")(next.clone(), (*wp).sx, (*wp).sy);
        }
    }
    name = if !next.is_alive() {
        ::core::ptr::null::<::core::ffi::c_char>()
    } else {
        next.get_unchecked().mode.name.as_ptr()
    };
    (*wp).flags |= PANE_REDRAW | PANE_REDRAWSCROLLBAR | PANE_CHANGED;
    ((*wp).window_handle().as_ref())
        .expect("live window")
        .refit();
    server_redraw_window_borders(((*wp).window_handle().as_ref()).expect("live window"));
    server_status_window(((*wp).window_handle().as_ref()).expect("live window"));
    window_fire_pane_mode_changed(
        c"pane-mode-exited",
        pane_owner,
        nullable_cstr(p),
        nullable_cstr(name),
        0 as ::core::ffi::c_int,
    );
    window_fire_pane_mode_changed(
        c"pane-mode-changed",
        pane_owner,
        nullable_cstr(p),
        nullable_cstr(name),
        0 as ::core::ffi::c_int,
    );
    if kill_0 != 0 {
        server_kill_pane(pane_owner);
    }
}

unsafe fn window_pane_reset_mode_all(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    while !(*wp).modes.is_empty() {
        window_pane_reset_mode(pane_owner);
    }
}

fn window_pane_prompt_input_callback(
    data: &refbox::Weak<window_pane_prompt>,
    input: Option<&CStr>,
    key: prompt_key_result,
) -> prompt_result {
    let (client, callback) = {
        let Ok(mut state) = data.try_borrow_mut() else {
            return PROMPT_CLOSE;
        };
        (state.c.upgrade(), state.inputcb.take())
    };
    let Some(mut callback) = callback else {
        return PROMPT_CLOSE;
    };
    let result = callback(client.as_ref(), input, key);
    if let Ok(mut state) = data.try_borrow_mut() {
        state.inputcb = Some(callback);
    }
    result
}

unsafe fn window_pane_prompt_free_callback(
    data: &refbox::RefBox<crate::src::shared::pane::window_pane_prompt>,
) {
    let (wp_id, callback, inputcb) = {
        let mut state = data.try_borrow_mut().expect("prompt cleanup record");
        (state.wp_id, state.freecb.take(), state.inputcb.take())
    };
    let lookup_wp_owner = window_pane_find_by_id(wp_id);
    let wp = lookup_wp_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !wp.is_null() && (*wp).prompt_data.is(data) {
        (*wp).prompt_data = refbox::Weak::new();
    }
    if let Some(callback) = callback {
        callback();
    }
    drop(inputcb);
}

unsafe fn window_pane_set_prompt(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    client_owner: Option<&ClientRef>,
    mut fs: *mut cmd_find_state,
    msg: &CStr,
    input: Option<&CStr>,
    mut inputcb: status_prompt_input_cb,
    mut freecb: prompt_free_cb,
    mut flags: ::core::ffi::c_int,
    mut type_0: prompt_type,
) {
    let mut input: *const ::core::ffi::c_char = input.map_or(std::ptr::null(), CStr::as_ptr);
    let wp = pane_owner.get();
    let session_owner = client_owner.and_then(|owner| owner.attached_session().upgrade());
    let mut pd = prompt_create_data::default();
    window_pane_clear_prompt(pane_owner);
    let wpp = refbox::RefBox::new(window_pane_prompt {
        wp_id: (*wp).id,
        c: client_owner.map(Rc::downgrade).unwrap_or_default(),
        inputcb,
        freecb,
        type_0,
    });
    prompt_set_options(&mut pd, session_owner.as_ref());
    pd.fs = fs.as_ref();
    pd.prompt = msg;
    pd.input = if input.is_null() {
        None
    } else {
        Some(CStr::from_ptr(input))
    };
    pd.type_0 = type_0;
    pd.flags = flags;
    let input_data = wpp.downgrade();
    let identity = input_data.clone();
    pd.inputcb = Some(Box::new(move |s, key| {
        window_pane_prompt_input_callback(&input_data, s, key)
    }));
    let free_data = wpp;
    pd.freecb = Some(Box::new(move || unsafe {
        window_pane_prompt_free_callback(&free_data)
    }));
    let prompt = prompt_create(pd);
    let prompt_observer = prompt.downgrade();
    (*wp).prompt = Some(prompt);
    let prompt = prompt_observer;
    (*wp).prompt_data = identity;
    (*wp).flags |= PANE_REDRAW;
    prompt_incremental_start(&prompt);
    window_fire_pane_prompt(c"pane-prompt-opened", pane_owner, type_0);
}

unsafe fn window_pane_clear_prompt(owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = owner.get();
    let prompt = (*wp).prompt.take();
    let wpp = (*wp).prompt_data.clone();
    let mut type_0: prompt_type = PROMPT_TYPE_INVALID;
    if let Some(prompt) = prompt {
        if !wpp.is_empty() {
            type_0 = wpp
                .try_borrow_mut()
                .expect("live pane prompt record")
                .type_0;
        }
        prompt_free(&prompt.downgrade());
        (*wp).flags |= PANE_REDRAW;
        if !(*wp).flags & PANE_DESTROYED != 0 {
            window_fire_pane_prompt(c"pane-prompt-closed", owner, type_0);
        }
    }
}

fn window_pane_has_prompt(wp: &window_pane) -> ::core::ffi::c_int {
    wp.prompt.is_some() as ::core::ffi::c_int
}

unsafe fn window_pane_update_prompt(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    msg: &CStr,
    input: Option<&CStr>,
) {
    let input: *const ::core::ffi::c_char = input.map_or(std::ptr::null(), CStr::as_ptr);
    let wp = pane_owner.get();
    if (*wp).prompt.is_some() {
        prompt_update(
            &mut (*wp)
                .prompt
                .as_ref()
                .expect("active prompt")
                .try_borrow_mut()
                .expect("unborrowed prompt"),
            msg,
            (!input.is_null()).then(|| CStr::from_ptr(input)),
        );
        (*wp).flags |= PANE_REDRAW;
    }
}

unsafe fn window_pane_prompt_key(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    client_owner: Option<&ClientRef>,
    mut key: key_code,
    mut m: *mut mouse_event,
) -> prompt_key_result {
    let mut wp = pane_owner.get();
    let prompt = (*wp).prompt.as_ref().map(|prompt| prompt.downgrade());
    let wpp = (*wp).prompt_data.clone();
    let mut result: prompt_key_result = PROMPT_KEY_NOT_HANDLED;
    let mut wp_id: u_int = (*wp).id;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut py: u_int = 0;
    let mut redraw: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let Some(prompt) = prompt else {
        return PROMPT_KEY_NOT_HANDLED;
    };
    if !wpp.is_empty() {
        wpp.try_borrow_mut().expect("live prompt callback record").c =
            client_owner.map(Rc::downgrade).unwrap_or_default();
    }
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
    {
        if m.is_null()
            || (*m).b & MOUSE_MASK_BUTTONS as u_int != MOUSE_BUTTON_1 as u_int
            || (*m).b & MOUSE_MASK_DRAG as u_int != 0
            || (*m).b & MOUSE_MASK_BUTTONS as u_int == 3 as u_int
            || cmd_mouse_at(
                pane_owner,
                m,
                &raw mut x,
                &raw mut y,
                0 as ::core::ffi::c_int,
            ) != 0 as ::core::ffi::c_int
        {
            result = PROMPT_KEY_NOT_HANDLED;
        } else {
            if client_owner.is_some_and(|owner| status_at_line(owner) == 0) {
                py = 0 as u_int;
            } else {
                py = (*wp).sy.wrapping_sub(1 as u_int);
            }
            if y == py {
                result = prompt_mouse(
                    &mut prompt.try_borrow_mut().expect("live unborrowed prompt"),
                    x,
                    0 as u_int,
                    (*wp).sx,
                    Some(&mut redraw),
                );
            } else {
                result = PROMPT_KEY_NOT_HANDLED;
            }
        }
    } else {
        result = prompt_key(&prompt, key, &mut redraw);
    }
    let lookup_wp_owner = window_pane_find_by_id(wp_id);
    wp = lookup_wp_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if wp.is_null() {
        return result;
    }
    if !wpp.is_empty() && (*wp).prompt_data == wpp {
        wpp.try_borrow_mut().expect("live prompt callback record").c = Weak::new();
    }
    if (*wp)
        .prompt
        .as_ref()
        .is_some_and(|current| prompt.is(current))
        && (result as ::core::ffi::c_uint
            == PROMPT_KEY_CLOSE as ::core::ffi::c_int as ::core::ffi::c_uint
            || prompt_closed(&prompt.try_borrow_mut().expect("live unborrowed prompt")) != 0)
    {
        window_pane_clear_prompt(lookup_wp_owner.as_ref().expect("live prompt pane"));
    }
    if redraw != 0
        || !(*wp)
            .prompt
            .as_ref()
            .is_some_and(|current| prompt.is(current))
    {
        (*wp).flags |= PANE_REDRAW;
    }
    result
}

unsafe fn window_pane_copy_paste(
    owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    bytes: &[u8],
) {
    let wp = owner.get();
    let window_owner = owner.window_observer().upgrade().expect("pane window");
    for pane_owner in window_owner
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
    {
        let loop_0 = pane_owner.get();
        if loop_0 != wp
            && (*loop_0).modes.is_empty()
            && (*loop_0).fd.is_some()
            && !(*loop_0).flags & PANE_INPUTOFF != 0
            && options_get_number(
                options_owner_ptr(&mut (*loop_0).options)
                    .map_or(std::ptr::null_mut(), |options| options),
                c"synchronize-panes",
            ) != 0
        {
            log_debug(format_args!(
                "{}: {}",
                "window_pane_copy_paste",
                log_cstr_n(bytes)
            ));
            let _ = (*loop_0).event.with_ptr(|event| unsafe {
                bufferevent_write(event, bytes);
            });
        }
    }
    drop(window_owner);
}

unsafe fn window_pane_copy_key(
    owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    key: key_code,
) {
    let wp = owner.get();
    let window_owner = owner.window_observer().upgrade().expect("pane window");
    for pane_owner in window_owner
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
    {
        let loop_0 = pane_owner.get();
        if loop_0 != wp
            && (*loop_0).modes.is_empty()
            && (*loop_0).fd.is_some()
            && !(*loop_0).flags & PANE_INPUTOFF != 0
            && options_get_number(
                options_owner_ptr(&mut (*loop_0).options)
                    .map_or(std::ptr::null_mut(), |options| options),
                c"synchronize-panes",
            ) != 0
        {
            input_key_pane(&pane_owner, key, ::core::ptr::null_mut::<mouse_event>());
        }
    }
    drop(window_owner);
}

unsafe fn window_pane_paste(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut key: key_code,
    bytes: &[u8],
) {
    let wp = pane_owner.get();
    if !(*wp).modes.is_empty() {
        return;
    }
    if (*wp).fd.is_none() || (*wp).flags & PANE_INPUTOFF != 0 {
        return;
    }
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
        == (KEYC_TYPE_FUNCTION as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
            << 32 as ::core::ffi::c_int
        && (key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
            == KEYC_PASTE_START as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
            || key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                == KEYC_PASTE_END as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong)
        && !(*(*wp).screen_ptr()).mode & MODE_BRACKETPASTE != 0
    {
        return;
    }
    log_debug(format_args!(
        "{}: {}",
        "window_pane_paste",
        log_cstr_n(bytes)
    ));
    let _ = (*wp).event.with_ptr(|event| unsafe {
        bufferevent_write(event, bytes);
    });
    if options_get_number(
        options_owner_ptr(&mut (*wp).options).map_or(std::ptr::null_mut(), |options| options),
        c"synchronize-panes",
    ) != 0
    {
        window_pane_copy_paste(pane_owner, bytes);
    }
}

unsafe fn window_pane_key(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    client_owner: Option<&ClientRef>,
    mut wl: refbox::Weak<winlink>,
    mut key: key_code,
    mut m: *mut mouse_event,
) -> ::core::ffi::c_int {
    let wp = pane_owner.get();
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    if (key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int)
        && m.is_null()
    {
        return -(1 as ::core::ffi::c_int);
    }
    wme = (*wp).active_mode_entry();
    if wme.is_alive() {
        if key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            == (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
        {
            return 0 as ::core::ffi::c_int;
        }
        if let (Some(callback), Some(client)) = (wme.get_unchecked().mode.key, client_owner) {
            key &= !KEYC_MASK_FLAGS;
            callback(wme, client, wl, key, m);
        }
        return 0 as ::core::ffi::c_int;
    }
    if (*wp).fd.is_none() || (*wp).flags & PANE_INPUTOFF != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if input_key_pane(pane_owner, key, m) != 0 as ::core::ffi::c_int {
        return -(1 as ::core::ffi::c_int);
    }
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
    {
        return 0 as ::core::ffi::c_int;
    }
    if options_get_number(
        options_owner_ptr(&mut (*wp).options).map_or(std::ptr::null_mut(), |options| options),
        c"synchronize-panes",
    ) != 0
    {
        window_pane_copy_key(pane_owner, key);
    }
    0 as ::core::ffi::c_int
}

fn window_pane_exited(wp: &window_pane) -> ::core::ffi::c_int {
    (wp.fd.is_none() || wp.flags & PANE_EXITED != 0) as ::core::ffi::c_int
}

unsafe fn window_pane_search(
    wp: &window_pane,
    term: &CStr,
    mut regex: ::core::ffi::c_int,
    mut ignore: ::core::ffi::c_int,
) -> u_int {
    let grid = wp.base.grid();
    let mut regex_storage = RegexStorage::default();
    let mut i: u_int = 0;
    let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut found: ::core::ffi::c_int = 0;
    let glob = if regex == 0 {
        if ignore != 0 {
            flags |= FNM_CASEFOLD;
        }
        let term = term.to_bytes();
        let mut pattern = Vec::with_capacity(term.len() + 2);
        pattern.push(b'*');
        pattern.extend_from_slice(term);
        pattern.push(b'*');
        Some(CString::new(pattern).expect("search term contains no NUL"))
    } else {
        if ignore != 0 {
            flags |= REG_ICASE;
        }
        None
    };
    let regex_owner = if regex != 0 {
        let Ok(regex) = regex_storage.compile(term, flags | REG_EXTENDED) else {
            return 0;
        };
        Some(regex)
    } else {
        None
    };
    i = 0 as u_int;
    while i < grid.sy {
        let mut line = grid_view_string_cells_bytes(grid, i, grid.sx);
        if let Some(nul) = line.iter().position(|&byte| byte == 0) {
            line.truncate(nul);
        }
        while let Some(&last) = line.last() {
            if ::libc::isspace(last as ::core::ffi::c_int) == 0 {
                break;
            }
            line.pop();
        }
        line.push(0);
        log_debug(format_args!(
            "{}: {}",
            "window_pane_search",
            log_bytes(&line)
        ));
        if regex == 0 {
            found = (fnmatch(
                glob.as_ref()
                    .expect("nonregex search has a pattern")
                    .as_ptr(),
                line.as_ptr().cast::<::core::ffi::c_char>(),
                flags,
            ) == 0 as ::core::ffi::c_int) as ::core::ffi::c_int;
        } else {
            found = regex_owner
                .as_ref()
                .unwrap()
                .is_match(CStr::from_bytes_with_nul(&line).unwrap()) as i32;
        }
        if found != 0 {
            break;
        }
        i = i.wrapping_add(1);
    }
    drop(regex_owner);
    if i == grid.sy {
        return 0 as u_int;
    }
    i.wrapping_add(1 as u_int)
}

unsafe fn window_pane_full_size_offset(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> (::core::ffi::c_int, ::core::ffi::c_int, u_int, u_int) {
    let wp = pane_owner.get();
    let sb_w = if window_pane_scrollbar_reserve(&*wp) != 0 {
        ((*wp).scrollbar_style.width + (*wp).scrollbar_style.pad) as u_int
    } else {
        0
    };
    let xoff = if (*wp)
        .window_handle()
        .expect("pane parent")
        .scrollbar_position()
        == PANE_SCROLLBARS_LEFT
    {
        ((*wp).xoff as u_int).wrapping_sub(sb_w) as ::core::ffi::c_int
    } else {
        (*wp).xoff
    };
    (xoff, (*wp).yoff, (*wp).sx.wrapping_add(sb_w), (*wp).sy)
}

unsafe fn window_pane_stack_push(
    stack: *mut window_pane_history,
    pane: Option<&Rc<UnsafeCell<window_pane>>>,
) {
    if let Some(pane) = pane {
        crate::src::shared::pane::pane_history_push(&mut *stack, Rc::downgrade(pane));
    }
}

unsafe fn window_pane_stack_remove(
    stack: *mut window_pane_history,
    pane: Option<&Rc<UnsafeCell<window_pane>>>,
) {
    if let Some(pane) = pane {
        crate::src::shared::pane::pane_history_remove(&mut *stack, &Rc::downgrade(pane));
    }
}

unsafe fn window_pane_input_callback(
    cdata: &window_pane_input_data,
    error: ::core::ffi::c_int,
    closed: bool,
    buffer: &mut SegmentedBuf,
) {
    let client = cdata.client.as_ref().expect("pane input client");
    let buf = evbuffer_pullup(buffer, -1).map_or(std::ptr::null_mut(), |bytes| bytes.as_mut_ptr());
    let len = evbuffer_get_length(buffer);
    let lookup_wp_owner = window_pane_find_by_id(cdata.wp);
    let wp = lookup_wp_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if wp.is_null() {
        client.request_exit(1);
    }
    let file = cdata.file.upgrade();
    if let Some(cancelled) = file
        .as_ref()
        .filter(|_| !closed && (wp.is_null() || client.is_dead() || error != 0))
    {
        file_cancel(&mut *cancelled.get());
    } else if file.is_none() || closed || error != 0 {
        cmdq_continue(
            &cdata
                .item
                .upgrade()
                .expect("file wait retains command item"),
        );
    } else {
        let bytes: &[u8] = if buf.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(buf, len)
        };
        input_parse_buffer(lookup_wp_owner.as_ref().expect("live pane"), bytes);
    }
    evbuffer_drain(buffer, len);
}

unsafe fn window_pane_start_input(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> Result<::core::ffi::c_int, std::ffi::CString> {
    let item = item_handle.get();
    let mut wp = wp_owner.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    if (*wp).flags & PANE_EMPTY == 0 {
        return Err(c"pane is not empty".to_owned());
    }
    if c.as_ref().expect("live client").flags() & (CLIENT_DEAD | CLIENT_EXITED) as uint64_t != 0 {
        return Ok(1);
    }
    if !c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade()
        .is_none()
    {
        return Ok(1);
    }
    // The file initializer publishes its weak identity before dispatch. The
    // callback then owns the box, including its retained client reference.
    let mut cdata = Box::new(window_pane_input_data {
        item: std::rc::Rc::downgrade(item_handle),
        client: c.clone(),
        wp: (*wp).id,
        file: Weak::new(),
    });
    file_read_with_cmdq_wait_init(
        c_owner.as_ref(),
        c"-",
        move |file| {
            cdata.file = file;
            cdata.into_callback()
        },
        item_handle,
        None,
    );
    Ok(0)
}

unsafe fn window_pane_update_used_data(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    mut wpo: *mut window_pane_offset,
    mut size: size_t,
) {
    let mut wp = wp_owner.get();
    let used: size_t = (*wpo).used.wrapping_sub((*wp).base_offset);
    let Some(available) = (*wp)
        .event
        .with_ptr(|event| unsafe { evbuffer_get_length(&(*event).input) })
    else {
        return;
    };
    if size > available.saturating_sub(used) {
        size = available.saturating_sub(used);
    }
    (*wpo).used = (*wpo).used.wrapping_add(size);
}

unsafe fn window_pane_default_cursor(wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let mut wp = wp_owner.get();
    screen_set_default_cursor(
        &mut *(*wp).screen_ptr(),
        options_owner_ptr(&mut (*wp).options).map_or(std::ptr::null_mut(), |options| options),
    );
}

unsafe fn window_pane_mode(wp: &window_pane) -> ::core::ffi::c_int {
    if let Some(active) = wp.active_mode() {
        if std::ptr::eq(active, &window_copy_mode) {
            return 1 as ::core::ffi::c_int;
        }
        if std::ptr::eq(active, &window_view_mode) {
            return 2 as ::core::ffi::c_int;
        }
    }
    0 as ::core::ffi::c_int
}

unsafe fn window_pane_show_scrollbar(wp: &window_pane) -> ::core::ffi::c_int {
    if wp.base.saved_grid.is_some() {
        return 0;
    }
    let window = wp.window_handle().expect("live window");
    let mode = window.scrollbar_mode();
    (mode == PANE_SCROLLBARS_ALWAYS
        || mode == PANE_SCROLLBARS_AUTOHIDE
        || mode == PANE_SCROLLBARS_MODAL && window_pane_mode(wp) != WINDOW_PANE_NO_MODE) as _
}

unsafe fn window_pane_scrollbar_reserve(wp: &window_pane) -> ::core::ffi::c_int {
    if window_pane_show_scrollbar(wp) == 0 {
        return 0 as ::core::ffi::c_int;
    }
    (((wp.window_handle().as_ref()).expect("live window")).scrollbar_mode()
        == PANE_SCROLLBARS_ALWAYS) as ::core::ffi::c_int
}

unsafe fn window_pane_scrollbar_overlay(wp: &window_pane) -> ::core::ffi::c_int {
    if window_pane_show_scrollbar(wp) == 0 {
        return 0 as ::core::ffi::c_int;
    }
    window_pane_scrollbar_auto_hide(wp)
}

unsafe fn window_pane_scrollbar_visible(wp: &window_pane) -> ::core::ffi::c_int {
    if window_pane_show_scrollbar(wp) == 0 {
        return 0 as ::core::ffi::c_int;
    }
    if window_pane_scrollbar_auto_hide(wp) == 0 {
        return 1 as ::core::ffi::c_int;
    }
    wp.sb_auto_visible
}

unsafe fn window_pane_scrollbar_start_timer(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    let mut delay: u_int = 0;
    if window_pane_scrollbar_auto_hide(&*wp) == 0 || (*wp).sb_auto_visible == 0 {
        return;
    }
    delay = (((*wp).window_handle().as_ref()).expect("live window"))
        .with_options_mut(|options| options_get_number(options, c"pane-scrollbars-timeout"))
        as u_int;
    let timeout = Duration::from_millis(delay as u64);
    drop((*wp).sb_auto_timer.take());
    let observer = std::rc::Rc::downgrade(pane_owner);
    (*wp).sb_auto_timer = Some(
        Timer::new(timeout, move || unsafe {
            if let Some(owner) = observer.upgrade() {
                window_pane_scrollbar_timer(&owner);
            }
        })
        .expect("arm timer"),
    );
}

unsafe fn window_pane_scrollbar_show(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    let mut changed: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if window_pane_scrollbar_auto_hide(&*wp) == 0 {
        return;
    }
    if window_pane_show_scrollbar(&*wp) == 0 {
        return;
    }
    if (*wp).sb_auto_visible == 0 {
        (*wp).sb_auto_visible = 1 as ::core::ffi::c_int;
        changed = 1 as ::core::ffi::c_int;
    }
    drop((*wp).sb_auto_timer.take());

    window_pane_scrollbar_start_timer(pane_owner);

    if changed != 0 {
        window_pane_scrollbar_redraw_visibility(pane_owner);
    }
}

unsafe fn window_pane_scrollbar_hide(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    drop((*wp).sb_auto_timer.take());
    if (*wp).sb_auto_visible == 0 {
        return;
    }
    (*wp).sb_auto_visible = 0 as ::core::ffi::c_int;
    window_pane_scrollbar_redraw_visibility(pane_owner);
}

unsafe fn window_pane_get_bg(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> ::core::ffi::c_int {
    let mut c: ::core::ffi::c_int = 0;
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
    c = window_pane_get_bg_control_client(wp_owner);
    if c == -(1 as ::core::ffi::c_int) {
        defaults = tty_default_colours(wp_owner).0;
        if defaults.bg == 8 as ::core::ffi::c_int || defaults.bg == 9 as ::core::ffi::c_int {
            c = window_get_bg_client(wp_owner);
        } else {
            c = defaults.bg;
        }
    }
    c
}

unsafe fn window_get_bg_client(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> ::core::ffi::c_int {
    let mut wp = wp_owner.get();
    let mut loop_0: Option<ClientRef> = None;
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
                    .contains_window(&WindowRef::clone(
                        ((*wp).window_handle().as_ref()).expect("live window"),
                    )) as i32)
                    == 0)
            && !(loop_0.as_ref().expect("live client").borrow_terminal().bg
                == -(1 as ::core::ffi::c_int))
        {
            return loop_0.as_ref().expect("live client").borrow_terminal().bg;
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    -(1 as ::core::ffi::c_int)
}

unsafe fn window_pane_get_bg_control_client(
    pane: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> ::core::ffi::c_int {
    let colour = (*pane.get()).control_bg;
    if colour == -1 {
        return -1;
    }
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if client.is_control() {
            return colour;
        }
        cursor = ClientRegistry::next(&client);
    }
    -1
}

unsafe fn window_pane_get_fg(
    wp_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> ::core::ffi::c_int {
    let mut wp = wp_owner.get();
    let mut loop_0: Option<ClientRef> = None;
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
                    .contains_window(&WindowRef::clone(
                        ((*wp).window_handle().as_ref()).expect("live window"),
                    )) as i32)
                    == 0)
            && !(loop_0.as_ref().expect("live client").borrow_terminal().fg
                == -(1 as ::core::ffi::c_int))
        {
            return loop_0.as_ref().expect("live client").borrow_terminal().fg;
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    -(1 as ::core::ffi::c_int)
}

unsafe fn window_pane_get_fg_control_client(
    pane: &Rc<std::cell::UnsafeCell<window_pane>>,
) -> ::core::ffi::c_int {
    let colour = (*pane.get()).control_fg;
    if colour == -1 {
        return -1;
    }
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if client.is_control() {
            return colour;
        }
        cursor = ClientRegistry::next(&client);
    }
    -1
}

unsafe fn window_pane_get_theme(
    wp_owner: Option<&Rc<std::cell::UnsafeCell<window_pane>>>,
) -> client_theme {
    let mut wp = wp_owner.map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut loop_0: Option<ClientRef> = None;
    let mut found_light: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut found_dark: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if wp.is_null() {
        return THEME_UNKNOWN;
    }
    let window = (*wp).window_handle().expect("live window");
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
        {
            match loop_0.as_ref().expect("live client").terminal_theme() as ::core::ffi::c_uint {
                1 => {
                    found_light = 1 as ::core::ffi::c_int;
                }
                2 => {
                    found_dark = 1 as ::core::ffi::c_int;
                }
                _ => {}
            }
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    drop(window);
    if found_dark != 0 && found_light == 0 {
        return THEME_DARK;
    }
    if found_light != 0 && found_dark == 0 {
        return THEME_LIGHT;
    }
    colour_totheme(window_pane_get_bg(wp_owner.expect("pane checked above")))
}

unsafe fn window_pane_send_theme_update(pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    let mut theme: client_theme = THEME_UNKNOWN;
    if window_pane_exited(&*wp) != 0 {
        return;
    }
    if !(*wp).flags & PANE_THEMECHANGED != 0 {
        return;
    }
    if !(*(*wp).screen_ptr()).mode & MODE_THEME_UPDATES != 0 {
        return;
    }
    theme = window_pane_get_theme(Some(pane_owner));
    if theme as ::core::ffi::c_uint == (*wp).last_theme as ::core::ffi::c_uint {
        return;
    }
    (*wp).last_theme = theme;
    (*wp).flags &= !PANE_THEMECHANGED;
    match theme as ::core::ffi::c_uint {
        1 => {
            log_debug(format_args!(
                "{}: %{} light theme",
                "window_pane_send_theme_update",
                { (*wp).id }
            ));
            let _ = (*wp)
                .event
                .with_ptr(|event| unsafe { bufferevent_write(event, b"\x1B[?997;2n") });
        }
        2 => {
            log_debug(format_args!(
                "{}: %{} dark theme",
                "window_pane_send_theme_update",
                { (*wp).id }
            ));
            let _ = (*wp)
                .event
                .with_ptr(|event| unsafe { bufferevent_write(event, b"\x1B[?997;1n") });
        }
        0 => {
            log_debug(format_args!(
                "{}: %{} unknown theme",
                "window_pane_send_theme_update",
                { (*wp).id }
            ));
        }
        _ => {}
    };
}

unsafe fn window_pane_status_get_range(
    pane_owner: &Rc<std::cell::UnsafeCell<window_pane>>,
    x: u_int,
    y: u_int,
) -> Option<style_range> {
    let wp = pane_owner.get();
    let pane_status = window_pane_get_pane_status(&*wp);
    let line = if pane_status == PANE_STATUS_TOP {
        ((*wp).yoff - 1) as u_int
    } else if pane_status == PANE_STATUS_BOTTOM {
        ((*wp).yoff as u_int).wrapping_add((*wp).sy)
    } else {
        0
    };
    if pane_status == PANE_STATUS_OFF || line != y {
        return None;
    }
    let x = x.wrapping_sub((*wp).xoff as u_int).wrapping_sub(2);
    (*wp)
        .border_status_line
        .ranges
        .as_slice()
        .iter()
        .find(|range| x >= range.start && x < range.end)
        .map(|range| **range)
}

unsafe fn window_pane_get_pane_status(wp: &window_pane) -> ::core::ffi::c_int {
    wp.window_handle()
        .expect("pane window")
        .pane_border_status()
}

unsafe fn window_pane_get_pane_lines(wp: &window_pane) -> pane_lines {
    wp.window_handle().expect("pane window").pane_border_lines()
}

impl Drop for window_pane {
    fn drop(&mut self) {
        log_debug(format_args!("pane %{} freed", { self.id }));
        // Logical destroy normally takes the parser first; direct owner
        // destruction must still drop it before the screens it writes to.
        drop(self.ictx.take());
    }
}

pub(crate) use input::{input_complete_request, input_free_request, input_request_matches};
pub use input::{
    input_csi_type, input_esc_type, input_init, input_parse_screen, input_pending,
    input_reply_clipboard, input_request_reply, input_reset, input_set_buffer_size,
    input_table_entry, InputRequestReply, INPUT_BUF_START, INPUT_CSI_CBT, INPUT_CSI_CNL,
    INPUT_CSI_CPL, INPUT_CSI_CUB, INPUT_CSI_CUD, INPUT_CSI_CUF, INPUT_CSI_CUP, INPUT_CSI_CUU,
    INPUT_CSI_DA, INPUT_CSI_DA_TWO, INPUT_CSI_DCH, INPUT_CSI_DECSCUSR, INPUT_CSI_DECSTBM,
    INPUT_CSI_DL, INPUT_CSI_DSR, INPUT_CSI_DSR_PRIVATE, INPUT_CSI_ECH, INPUT_CSI_ED, INPUT_CSI_EL,
    INPUT_CSI_HPA, INPUT_CSI_ICH, INPUT_CSI_IL, INPUT_CSI_MODOFF, INPUT_CSI_MODSET,
    INPUT_CSI_QUERY, INPUT_CSI_QUERY_PRIVATE, INPUT_CSI_RCP, INPUT_CSI_REP, INPUT_CSI_RM,
    INPUT_CSI_RM_PRIVATE, INPUT_CSI_SCP, INPUT_CSI_SD, INPUT_CSI_SGR, INPUT_CSI_SM,
    INPUT_CSI_SM_GRAPHICS, INPUT_CSI_SM_PRIVATE, INPUT_CSI_SU, INPUT_CSI_TBC, INPUT_CSI_VPA,
    INPUT_CSI_WINOPS, INPUT_CSI_XDA, INPUT_DISCARD, INPUT_END_BEL, INPUT_END_ST, INPUT_ESC_DECALN,
    INPUT_ESC_DECKPAM, INPUT_ESC_DECKPNM, INPUT_ESC_DECRC, INPUT_ESC_DECSC, INPUT_ESC_HTS,
    INPUT_ESC_IND, INPUT_ESC_NEL, INPUT_ESC_RI, INPUT_ESC_RIS, INPUT_ESC_SCSG0_OFF,
    INPUT_ESC_SCSG0_ON, INPUT_ESC_SCSG1_OFF, INPUT_ESC_SCSG1_ON, INPUT_ESC_ST, INPUT_LAST,
    INPUT_REQUEST_TIMEOUT,
};

#[cfg(test)]
mod pane_prompt_data_tests {
    use super::*;

    use crate::src::text::utf8::utf8_fromcstr_vec;
    use std::cell::Cell;

    fn data(wp_id: u_int) -> refbox::RefBox<crate::src::shared::pane::window_pane_prompt> {
        refbox::RefBox::new(window_pane_prompt {
            wp_id,
            c: Weak::new(),
            inputcb: None,
            freecb: None,
            type_0: PROMPT_TYPE_COMMAND,
        })
    }

    fn attach(
        data: refbox::RefBox<crate::src::shared::pane::window_pane_prompt>,
    ) -> refbox::RefBox<crate::src::shared::prompt::prompt> {
        let pr = refbox::RefBox::new(prompt {
            flags: PROMPT_SINGLE,
            buffer: utf8_fromcstr_vec(c""),
            ..Default::default()
        });
        let input = data.downgrade();
        pr.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |text, key| {
            window_pane_prompt_input_callback(&input, text, key)
        }));
        let freed = data;
        pr.try_borrow_mut().unwrap().freecb = Some(Box::new(move || unsafe {
            window_pane_prompt_free_callback(&freed)
        }));
        pr
    }

    #[test]
    fn callback_replacement_keeps_new_pane_data_and_releases_the_old_record() {
        unsafe {
            let pane = window_pane::new();
            let wp = pane.get();
            (*wp).id = u_int::MAX - 1;
            // This fixture exercises cleanup without firing pane hook events.
            (*wp).flags = PANE_DESTROYED;
            assert!(window_pane_tree::insert(pane.clone()).is_none());
            let old_data = data((*wp).id);
            let old_weak = old_data.downgrade();
            let old_prompt = attach(old_data);
            let old_data = old_weak.clone();
            let replacement = data((*wp).id);
            let replacement_weak = replacement.downgrade();
            let replacement_prompt = attach(replacement);
            let replacement = replacement_weak.clone();
            let next_data = replacement.clone();
            let replacement_observer = replacement_prompt.downgrade();
            let mut next_prompt = Some(replacement_prompt);
            let callback_pane = Rc::downgrade(&pane);
            old_data.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |_, text, kind| {
                let pane = callback_pane.upgrade().expect("prompt pane");
                let wp = pane.get();
                assert_eq!(text, Some(c"x"));
                assert_eq!(kind, PROMPT_KEY_CLOSE);
                window_pane_clear_prompt(&pane);
                (*wp).prompt = next_prompt.take();
                (*wp).prompt_data = next_data.clone();
                PROMPT_CLOSE
            }));
            let frees = Rc::new(Cell::new(0));
            let count = frees.clone();
            old_data.try_borrow_mut().unwrap().freecb =
                Some(Box::new(move || count.set(count.get() + 1)));
            let old_observer = old_prompt.downgrade();
            (*wp).prompt = Some(old_prompt);
            (*wp).prompt_data = old_weak.clone();
            assert_eq!(
                window_pane_prompt_key(&pane, None, b'x' as key_code, std::ptr::null_mut()),
                PROMPT_KEY_CLOSE
            );
            assert_eq!(frees.get(), 1);
            assert_eq!((*wp).prompt_data, replacement_weak);
            assert!(replacement_observer.is((*wp).prompt.as_ref().unwrap()));
            assert!(!old_observer.is_alive());
            drop(old_data);
            assert!(!old_weak.is_alive());
            window_pane_clear_prompt(&pane);
            assert!((*wp).prompt_data.is_empty());
            assert!(!replacement_observer.is_alive());
            drop(replacement);
            assert!(!replacement_weak.is_alive());
            assert_eq!(window_pane_tree::remove(&pane).unwrap().get(), wp);
        }
    }
}
