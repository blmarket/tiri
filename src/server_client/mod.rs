use crate::format::bytes::nullable_cstr;
use crate::options::OptionsScope;
use crate::session::Session as _;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::session::SessionRef;
use crate::shared::window::WindowRef;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::time::{Duration, SystemTime};
use std::{cell::RefCell, rc::Rc};
mod api;
mod client_overlay;
mod format;
mod model;
use crate::cfg::{cfg_client, cfg_finished, cfg_forget_client, start_cfg};
use crate::cmd::entries::wait_for::cmd_wait_for_client_lost;
use crate::cmd::parse::cmd_parse_from_argv;
use crate::cmd::queue::{
    cmdq_append, cmdq_error, cmdq_get_callback_owned, cmdq_get_client, cmdq_get_command,
    cmdq_get_error, cmdq_insert_after, cmdq_new, cmdq_next,
};
use crate::cmd::{cmd_list_all_have, cmd_log_argv};
use crate::compat::imsg::imsg_get_fd;
use crate::compat::imsg::msg_command;
use crate::control::{
    control_all_done, control_discard, control_discard_all, control_pane_offset, control_ready,
    control_reset_offsets, control_start, control_stop, control_write,
};
use crate::environ::{environ_create, environ_find, environ_put};
use crate::events::{events_fire, events_fire_client};
use crate::events_payload::{
    event_payload_create, event_payload_set_client, event_payload_set_int, event_payload_set_pane,
    event_payload_set_session, event_payload_set_target, event_payload_set_uint,
    event_payload_set_window,
};
use crate::file::{file_print, file_read_data, file_read_done, file_write_done, file_write_ready};
use crate::format::bytes::write_cstr;
use crate::format::{
    format_create, format_defaults, format_expand_cstring, format_expand_time_cstring,
    format_lost_client,
};
use crate::key_bindings::{key_bindings_dispatch, key_bindings_get, key_bindings_get_table};
use crate::key_string::key_string_format;
use crate::log::{fatal, log_cstr, log_debug, log_get_level, log_hex, log_pointer};
use crate::menu::{menu_close, menu_get_cursor, menu_key, menu_screen};
use crate::names::check_window_name;
use crate::options::{options_get_command, options_get_number, options_get_string};
use crate::proc::{
    bytes_of, imsg_string, proc_add_peer, proc_kill_peer, proc_remove_peer, proc_send,
};
use crate::prompt::prompt_free;
use crate::reactor::{
    bufferevent_enable, defer, evbuffer_add, evbuffer_get_length, evbuffer_pullup, evbuffer_readln,
};
use crate::resize::{recalculate_size, recalculate_sizes};
use crate::screen::screen_mode_display;
use crate::screen_redraw::{redraw_pane, redraw_pane_scrollbar, redraw_screen};
use crate::server::{current_time, server_add_accept, server_proc, server_update_socket};
use crate::server_fn::{
    server_check_unattached, server_redraw_window_borders, server_status_window,
};
pub use api::Client;
pub use client_overlay::Overlay;
use libc::access;
pub use model::client;

use crate::compat::imsg::imsg;
use crate::compat::imsg::*;
use crate::shared::abi::ssize_t;
use crate::shared::abi::*;
pub use crate::shared::client::overlay_check_cb;
use crate::shared::client::*;
use crate::shared::client::{
    CLIENT_ALLREDRAWFLAGS, CLIENT_ASSUMEPASTING, CLIENT_ATTACHED, CLIENT_BRACKETPASTING,
    CLIENT_CONTROL, CLIENT_CONTROL_NEWLAYOUTS, CLIENT_CONTROL_NOOUTPUT, CLIENT_CONTROL_PAUSEAFTER,
    CLIENT_CONTROL_WAITEXIT, CLIENT_DEAD, CLIENT_DOUBLECLICK, CLIENT_EXIT, CLIENT_EXITED,
    CLIENT_FOCUSED, CLIENT_IDENTIFIED, CLIENT_IGNORESIZE, CLIENT_NODETACHFLAGS,
    CLIENT_NO_DETACH_ON_DESTROY, CLIENT_PASTE_TIME_LIMIT, CLIENT_READONLY, CLIENT_REDRAWBORDERS,
    CLIENT_REDRAWMENU, CLIENT_REDRAWOVERLAY, CLIENT_REDRAWSCROLLBARS, CLIENT_REDRAWSTATUS,
    CLIENT_REDRAWWINDOW, CLIENT_REPEAT, CLIENT_STATUSFORCE, CLIENT_SUSPENDED, CLIENT_TERMINAL,
    CLIENT_TRIPLECLICK, CLIENT_UNATTACHEDFLAGS, CLIENT_UTF8,
};
use crate::shared::colour::{
    client_theme, COLOUR_FLAG_THEME, COLOUR_THEME_COUNT, THEME_DARK, THEME_LIGHT, THEME_UNKNOWN,
};
use crate::shared::command::unpack_argv;
use crate::shared::command::*;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::display::visible_ranges;
use crate::shared::environment::environ_entry;
use crate::shared::event::*;
use crate::shared::format::format_tree;
use crate::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use crate::shared::key::KEY_BINDING_REPEAT;
use crate::shared::key::*;
use crate::shared::key::{key_event, key_table, KeyBindingCommand};
use crate::shared::limits::UINT_MAX;
use crate::shared::mouse::{
    MOUSE_BUTTON_1, MOUSE_BUTTON_10, MOUSE_BUTTON_11, MOUSE_BUTTON_2, MOUSE_BUTTON_3,
    MOUSE_BUTTON_6, MOUSE_BUTTON_7, MOUSE_BUTTON_8, MOUSE_BUTTON_9, MOUSE_MASK_BUTTONS,
    MOUSE_MASK_CTRL, MOUSE_MASK_DRAG, MOUSE_MASK_META, MOUSE_MASK_SHIFT, MOUSE_WHEEL_DOWN,
    MOUSE_WHEEL_UP,
};
use crate::shared::pane::window_pane;
use crate::shared::pane::{
    window_pane_offset, PANE_SCROLLBARS_AUTOHIDE, PANE_SCROLLBARS_LEFT, PANE_SCROLLBARS_MODAL,
};
use crate::shared::posix_io::{_PATH_BSHELL, X_OK};
use crate::shared::screen::{
    ScreenMode, ALL_MOUSE_MODES, CURSOR_MODES, MODE_BRACKETPASTE, MODE_CURSOR, MODE_MOUSE_ALL,
    MODE_MOUSE_BUTTON, MODE_SYNC,
};
use crate::shared::tty::*;
use crate::shared::vis::{VIS_CSTYLE, VIS_NOSLASH, VIS_OCTAL};
use crate::shared::window::WINDOW_SIZE_LATEST;
use crate::shared::window::{window_mode_entry, winlink};
use crate::status::{
    status_at_line, status_free, status_get_range, status_init, status_line_size,
    status_message_clear, status_prompt_clear, status_prompt_cursor, status_prompt_key,
    status_timer_start,
};
use crate::style::colour::{
    colour_parse_cstr, colour_theme_option, colour_theme_terminal_colour, colour_totheme,
};
use crate::text::numbers::scan_u32;
use crate::text::utf8::{utf8_sanitize_cstring, utf8_stravisx_bytes};
use crate::tmux::{checkshell, find_home_cstr, setblocking};
use crate::tty::tty;
use crate::tty::tty_get_features;
use crate::tty::{
    tty_close, tty_cursor, tty_init, tty_invalidate, tty_margin_off, tty_open, tty_region_off,
    tty_repeat_requests, tty_reset, tty_resize, tty_send_requests, tty_set_path,
    tty_set_progress_bar, tty_set_size, tty_set_title, tty_start_tty, tty_stop_tty, tty_sync_end,
    tty_update_client_offset, tty_update_mode, tty_window_offset,
};
use crate::window::windows;
use crate::window::winlink_find_by_index;
use crate::window_copy::{window_copy_add, window_view_mode};
use crate::window_visible::{window_position_is_visible, window_visible_ranges};
use libc::EINTR;
use std::ffi::{CStr, CString};
use tiri_buffer::SegmentedBuf;

pub const _PATH_TTY: &std::ffi::CStr = c"/dev/tty";

impl client {
    /// Allocate a client on the server thread.
    ///
    /// # Safety
    /// The caller must serialize access to the global server model and perform
    /// client-loss cleanup before releasing a fully initialized client.
    fn new() -> ClientRef {
        std::rc::Rc::new(std::cell::RefCell::new(client::empty()))
    }
}

/// Own active clients in insertion order until explicit client-loss cleanup.
/// Each client retains its weak successor even after active removal.
pub struct ClientRegistry {
    ordered: Vec<ClientRef>,
}

thread_local! {
    static CLIENTS: std::cell::RefCell<ClientRegistry> = const {
        std::cell::RefCell::new(ClientRegistry {
            ordered: Vec::new(),
        })
    };
}

impl ClientRegistry {
    pub fn first() -> Option<ClientRef> {
        CLIENTS.with_borrow(|registry| registry.ordered.first().cloned())
    }

    /// Follow the client's own successor link; this deliberately does not
    /// borrow the registry so traversal survives removal of `current`.
    pub fn next(current: &ClientRef) -> Option<ClientRef> {
        current.borrow().registry_next.upgrade()
    }

    pub fn push_back(owner: ClientRef) {
        CLIENTS.with_borrow_mut(|registry| {
            assert!(
                !registry
                    .ordered
                    .iter()
                    .any(|current| std::rc::Rc::ptr_eq(current, &owner)),
                "client registered twice"
            );
            if let Some(previous) = registry.ordered.last() {
                previous.borrow_mut().registry_next = std::rc::Rc::downgrade(&owner);
            }
            owner.borrow_mut().registry_next = std::rc::Weak::new();
            registry.ordered.push(owner);
        })
    }

    /// Transfer the registry's strong reference to explicit cleanup, preserving
    /// the removed client's successor so an in-progress traversal can continue.
    pub fn remove(observer: &ClientWeak) -> Option<ClientRef> {
        CLIENTS.with_borrow_mut(|registry| {
            let index = registry.ordered.iter().position(|owner| {
                std::rc::Weak::ptr_eq(&std::rc::Rc::downgrade(owner), observer)
            })?;
            if index > 0 {
                let next = registry.ordered[index].borrow().registry_next.clone();
                registry.ordered[index - 1].borrow_mut().registry_next = next;
            }
            Some(registry.ordered.remove(index))
        })
    }
}

fn server_client_set_message(c: &mut client, message: Option<CString>) {
    c.message_string = message;
}

fn server_client_set_ttyname(c: &mut client, ttyname: Option<CString>) {
    c.ttyname = ttyname;
}

fn server_client_set_term_name(c: &mut client, term_name: Option<CString>) {
    c.term_name = term_name;
}

fn server_client_set_cwd(c: &mut client, cwd: Option<CString>) {
    c.cwd = cwd;
}

fn server_client_replace_title(c: &mut client, title: Option<CString>) {
    c.title = title;
}

fn server_client_replace_path(c: &mut client, path: Option<CString>) {
    c.path = path;
}

fn server_client_set_exit_session(c: &mut client, exit_session: Option<CString>) {
    c.exit_session = exit_session;
}

fn server_client_set_name(c: &mut client, name: Option<CString>) {
    c.name = name;
}

fn server_client_set_exit_message(c: &mut client, exit_message: Option<CString>) {
    c.exit_message = exit_message;
}

fn server_client_set_term_type(c: &mut client, term_type: Option<CString>) {
    c.term_type = term_type;
}

fn server_client_ensure_term_name(c: &mut client) {
    if c.term_name.as_deref().is_none_or(|name| name.is_empty()) {
        server_client_set_term_name(c, Some(CString::new("unknown").unwrap()));
    }
}

fn server_client_add_term_cap(c: &mut client, data: &CStr) {
    assert!(c.term_caps.len() < u_int::MAX as usize);
    c.term_caps.push(data.to_owned());
}

fn server_client_clear_term_caps(c: &mut client) {
    c.term_caps = Vec::new();
}

fn server_client_how_many() -> u_int {
    let mut n: u_int = 0;
    let mut registry_c_owner = ClientRegistry::first();
    while let Some(owner) = registry_c_owner {
        {
            let c = owner.borrow();
            if c.session_handle().is_some() && !c.flags & CLIENT_UNATTACHEDFLAGS as uint64_t != 0 {
                n = n.wrapping_add(1);
            }
        }
        registry_c_owner = ClientRegistry::next(&owner);
    }
    n
}
fn server_client_update_overlay_focus(owner: &ClientRef) {
    use crate::session::Session;
    use crate::window::Window;
    let Some(session) = owner.attached_session().upgrade() else {
        return;
    };
    let link = session.current_winlink();
    let window = winlink_window(&link);
    if let Some(window) = window {
        window.update_focus();
        drop(window);
    }
}

fn server_client_set_overlay(c_owner: &ClientRef, overlay: Overlay) {
    while c_owner.borrow().overlay.is_some() {
        server_client_clear_overlay(c_owner);
    }
    {
        let mut c = c_owner.borrow_mut();
        c.overlay_generation = c
            .overlay_generation
            .checked_add(1)
            .expect("overlay generation exhausted");
        if overlay.check.is_none() {
            c.tty.set_frozen(true);
        }
        if overlay.mode.is_none() {
            c.tty.set_cursor_hidden(true);
        }
        c.overlay = Some(overlay);
    }
    server_client_update_overlay_focus(c_owner);
    c_owner.request_redraw(CLIENT_ALLREDRAWFLAGS as u64);
}
fn server_client_clear_overlay(c_owner: &ClientRef) {
    let (overlay, generation) = {
        let mut c = c_owner.borrow_mut();
        let Some(overlay) = c.overlay.take() else {
            return;
        };
        c.overlay_generation = c
            .overlay_generation
            .checked_add(1)
            .expect("overlay generation exhausted");
        (overlay, c.overlay_generation)
    };
    drop(overlay);
    {
        let mut c = c_owner.borrow_mut();
        if c.overlay_generation == generation {
            c.tty.set_frozen(false);
            c.tty.set_cursor_hidden(false);
        }
    }
    server_client_update_overlay_focus(c_owner);
    c_owner.request_redraw(CLIENT_ALLREDRAWFLAGS as u64);
}
fn server_client_overlay_draw(c_owner: &ClientRef) {
    let generation = c_owner.borrow().overlay_generation;
    let Some(mut callback) = c_owner
        .borrow_mut()
        .overlay
        .as_mut()
        .and_then(|overlay| overlay.draw.take())
    else {
        return;
    };
    callback(c_owner);
    let mut c = c_owner.borrow_mut();
    if c.overlay_generation == generation {
        if let Some(overlay) = c.overlay.as_mut() {
            if overlay.draw.is_none() {
                overlay.draw = Some(callback);
            }
        }
    }
}

fn server_client_overlay_key(c_owner: &ClientRef, event: &mut key_event) -> Option<i32> {
    let generation = c_owner.borrow().overlay_generation;
    let mut callback = c_owner.borrow_mut().overlay.as_mut()?.key.take()?;
    let result = callback(c_owner, event);
    let mut c = c_owner.borrow_mut();
    if c.overlay_generation != generation {
        return Some(0);
    }
    if let Some(overlay) = c.overlay.as_mut() {
        if overlay.key.is_none() {
            overlay.key = Some(callback);
        }
    }
    Some(result)
}

fn server_client_overlay_mode(c_owner: &ClientRef) -> Option<(ScreenMode, u_int, u_int)> {
    let generation = c_owner.borrow().overlay_generation;
    let mut callback = c_owner.borrow_mut().overlay.as_mut()?.mode.take()?;
    let result = callback(c_owner);
    let mut c = c_owner.borrow_mut();
    if c.overlay_generation != generation {
        return None;
    }
    if let Some(overlay) = c.overlay.as_mut() {
        if overlay.mode.is_none() {
            overlay.mode = Some(callback);
        }
    }
    result
}

fn server_client_overlay_resize(c_owner: &ClientRef) {
    let generation = c_owner.borrow().overlay_generation;
    let Some(mut callback) = c_owner
        .borrow_mut()
        .overlay
        .as_mut()
        .and_then(|overlay| overlay.resize.take())
    else {
        return;
    };
    callback(c_owner);
    let mut c = c_owner.borrow_mut();
    if c.overlay_generation == generation {
        if let Some(overlay) = c.overlay.as_mut() {
            if overlay.resize.is_none() {
                overlay.resize = Some(callback);
            }
        }
    }
}

fn server_client_overlay_check(
    c_owner: &ClientRef,
    px: u_int,
    py: u_int,
    nx: u_int,
) -> Option<visible_ranges> {
    let generation = c_owner.borrow().overlay_generation;
    let mut callback = c_owner.borrow_mut().overlay.as_mut()?.check.take()?;
    let result = callback(c_owner, px, py, nx);
    let mut c = c_owner.borrow_mut();
    if c.overlay_generation != generation {
        return None;
    }
    if let Some(overlay) = c.overlay.as_mut() {
        if overlay.check.is_none() {
            overlay.check = Some(callback);
        }
    }
    Some(result)
}

fn server_client_check_nested(c: &client) -> ::core::ffi::c_int {
    let mut envent: Option<&environ_entry> = None;
    envent = environ_find(c.environ.as_deref().expect("environment"), c"TMUX");
    if envent.is_none()
        || envent
            .unwrap()
            .value
            .as_ref()
            .expect("environment value is present")
            .is_empty()
    {
        return 0 as ::core::ffi::c_int;
    }
    let ttyname = c.ttyname.as_deref().expect("client tty name");
    if crate::shared::pane::window_pane_tree::snapshot()
        .iter()
        .any(|pane| pane.matches_terminal_name(ttyname))
    {
        return 1;
    }
    0 as ::core::ffi::c_int
}
fn server_client_set_key_table(c_owner: &ClientRef, name: Option<&CStr>) {
    let default_name;
    let name = match name {
        Some(name) => name,
        None => {
            default_name = server_client_get_key_table(&c_owner.borrow());
            &default_name
        }
    };
    let old = c_owner.borrow_mut().keytable.take();
    drop(old);
    let table = key_bindings_get_table(name, 1);
    let mut c = c_owner.borrow_mut();
    c.keytable = table;
    c.keytable
        .as_ref()
        .expect("key table")
        .borrow_mut()
        .activity_time = SystemTime::now();
}
fn server_client_key_table_activity_diff(c: &client) -> uint64_t {
    c.activity_time
        .duration_since(
            c.keytable
                .as_ref()
                .expect("key table")
                .borrow()
                .activity_time,
        )
        .unwrap_or(Duration::MAX)
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn server_client_get_key_table(c: &client) -> std::ffi::CString {
    let Some(session) = c.session_handle() else {
        return c"root".to_owned();
    };
    let name = options_get_string(&session.borrow_options_mut(), c"key-table");
    if name.as_bytes().is_empty() {
        c"root".to_owned()
    } else {
        name
    }
}
fn server_client_is_default_key_table(c: &client, table: &key_table) -> ::core::ffi::c_int {
    (table.name == server_client_get_key_table(c)) as ::core::ffi::c_int
}
fn server_client_timer(
    client: &ClientRef,
    delay: Duration,
    callback: unsafe fn(&ClientRef),
) -> Timer {
    let observer = Rc::downgrade(client);
    Timer::new(delay, move || unsafe {
        if let Some(owner) = observer.upgrade() {
            callback(&owner);
        }
    })
    .expect("arm timer")
}
fn server_client_create(fd: OwnedFd) -> ClientRef {
    let mut i: u_int = 0;
    setblocking(fd.as_fd(), 0);
    let mut owner = client::new();
    let peer_observer = std::rc::Rc::downgrade(&owner);
    let dispatch: Box<dyn for<'a> FnMut(crate::shared::process::PeerMessage<'a>)> =
        Box::new(move |message| unsafe {
            if let Some(owner) = peer_observer.upgrade() {
                server_client_dispatch(&owner, message);
            }
        });
    let peer = unsafe { proc_add_peer(server_proc, fd, dispatch) };
    let keytable = key_bindings_get_table(c"root", 1);
    {
        let mut c = owner.borrow_mut();
        c.peer = peer;
        c.creation_time = SystemTime::now();
        c.activity_time = c.creation_time;
        c.environ = Some(environ_create());
        c.fd = None;
        c.out_fd = None;
        c.queue = Some(cmdq_new());
        tty_set_size(&mut c.tty, 80, 24, 0, 0);
        i = 0 as u_int;
        while i < COLOUR_THEME_COUNT as u_int {
            c.theme_colours[i as usize] = 8 as ::core::ffi::c_int;
            i = i.wrapping_add(1);
        }
        c.theme = THEME_UNKNOWN;
        let width = c.tty.size().0;
        status_init(&mut c.status, width);
        c.flags |= CLIENT_FOCUSED as uint64_t;
        c.keytable = keytable;
        c.click_wp = -(1 as ::core::ffi::c_int);
    }
    ClientRegistry::push_back(owner.clone());
    log_debug(format_args!(
        "new client {}",
        log_pointer(Rc::as_ptr(&owner))
    ));
    owner
}
fn server_client_open(owner: &ClientRef) -> Result<(), CString> {
    let (flags, ttyname) = {
        let c = owner.borrow();
        (c.flags, c.ttyname.clone())
    };
    if flags & CLIENT_CONTROL as uint64_t != 0 {
        return Ok(());
    }
    let client_name = ttyname.as_deref();
    if client_name == Some(c"/dev/tty")
        || [
            std::io::stdin().as_fd(),
            std::io::stdout().as_fd(),
            std::io::stderr().as_fd(),
        ]
        .into_iter()
        .any(|fd| {
            tiri_rt::unix::terminal_name(fd)
                .ok()
                .is_some_and(|name| Some(name.as_c_str()) == client_name)
        })
    {
        let mut message = b"can't use ".to_vec();
        message.extend_from_slice(
            ttyname
                .as_ref()
                .map_or(b"(null)".as_slice(), |name| name.to_bytes()),
        );
        return Err(CString::new(message).expect("terminal name contains no NUL"));
    }
    if flags & CLIENT_TERMINAL as uint64_t == 0 {
        return Err(c"not a terminal".to_owned());
    }
    tty_open(owner)?;
    server_client_update_theme_colours(Some(owner));
    Ok(())
}
fn server_client_attached_lost(c_owner: &ClientRef) {
    let mut s: Option<SessionRef> = None;
    log_debug(format_args!(
        "lost attached client {}",
        log_pointer(Rc::as_ptr(c_owner))
    ));
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        if window_owner.is_latest_client(c_owner) {
            let mut found: Option<ClientRef> = None;
            let mut registry_loop_0_owner = ClientRegistry::first();
            while let Some(loop_0) = registry_loop_0_owner {
                s = loop_0.borrow().session_handle();
                if !(Rc::ptr_eq(&loop_0, c_owner)
                    || s.is_none()
                    || winlink_window(&s.as_ref().expect("live session").current_winlink())
                        .is_none_or(|current| !WindowRef::ptr_eq(&current, &window_owner)))
                    && found.as_ref().is_none_or(|owner| {
                        loop_0.borrow().activity_time > owner.borrow().activity_time
                    })
                {
                    found = Some(loop_0.clone());
                }
                registry_loop_0_owner = ClientRegistry::next(&loop_0);
            }
            if let Some(found) = found {
                server_client_update_latest(&found);
            }
        }
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
}
fn server_client_fire_session_changed(c_owner: &ClientRef, old_owner: Option<&SessionRef>) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_client(Some(c_owner), 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_client(&mut ep, std::rc::Rc::clone(c_owner));
    if !fs.session_handle().is_none() {
        event_payload_set_session(
            &mut ep,
            c"session",
            fs.session_handle().expect("live session"),
        );
        event_payload_set_session(
            &mut ep,
            c"new_session",
            fs.session_handle().expect("live session"),
        );
    }
    if let Some(old) = old_owner {
        event_payload_set_session(&mut ep, c"old_session", old.clone());
    }
    if !fs.window_handle().is_none() {
        event_payload_set_window(
            &mut ep,
            c"window",
            WindowRef::clone((fs.window_handle().as_ref()).expect("live window")),
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
    } else if fs.idx != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"window_index", fs.idx);
    }
    if !fs.pane_handle().is_none() {
        event_payload_set_pane(
            &mut ep,
            c"pane",
            fs.pane_handle().expect("event target pane"),
        );
    }
    events_fire(c"client-session-changed", ep);
}
fn server_client_fire_resized(c_owner: &ClientRef, mut old_sx: u_int, mut old_sy: u_int) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_client(Some(c_owner), 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_client(&mut ep, std::rc::Rc::clone(c_owner));
    if !fs.session_handle().is_none() {
        event_payload_set_session(
            &mut ep,
            c"session",
            fs.session_handle().expect("live session"),
        );
    }
    if !fs.window_handle().is_none() {
        event_payload_set_window(
            &mut ep,
            c"window",
            WindowRef::clone((fs.window_handle().as_ref()).expect("live window")),
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
    } else if fs.idx != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"window_index", fs.idx);
    }
    if !fs.pane_handle().is_none() {
        event_payload_set_pane(
            &mut ep,
            c"pane",
            fs.pane_handle().expect("event target pane"),
        );
    }
    let (width, height) = c_owner.borrow().tty.size();
    event_payload_set_uint(&mut ep, c"width", width);
    event_payload_set_uint(&mut ep, c"height", height);
    event_payload_set_uint(&mut ep, c"old_width", old_sx);
    event_payload_set_uint(&mut ep, c"old_height", old_sy);
    events_fire(c"client-resized", ep);
}
fn server_client_set_session(c_owner: &ClientRef, s_owner: Option<&SessionRef>) {
    use crate::session::Session;
    use crate::window::Window;

    let old = c_owner.attached_session().upgrade();
    {
        let mut c = c_owner.borrow_mut();
        if let Some(target) = s_owner {
            if old
                .as_ref()
                .is_some_and(|old| !std::rc::Rc::ptr_eq(old, target))
            {
                c.last_session = c.session.clone();
            }
        } else {
            c.last_session = std::rc::Weak::new();
        }
        c.session = s_owner.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        c.flags |= CLIENT_FOCUSED as uint64_t;
    }
    if let Some(old) = old.as_ref() {
        let link = old.current_winlink();
        if link.is_alive() {
            let window = winlink_window(&link);
            if let Some(window) = window {
                window.update_focus();
                drop(window);
            }
        }
    }
    if let Some(session) = s_owner {
        {
            let link = session.current_winlink();
            let window = link
                .try_borrow()
                .expect("live winlink")
                .window_handle()
                .cloned()
                .expect("attached session window");
            window.set_latest_client(Some(c_owner));
        }
        recalculate_sizes();
        {
            let link = session.current_winlink();
            let window = winlink_window(&link).expect("attached session window");
            window.update_focus();
            drop(window);
        }
        session.on_attached();
        tty_update_client_offset(c_owner);
        status_timer_start(c_owner);
        server_client_fire_session_changed(c_owner, old.as_ref());
        c_owner.request_redraw(CLIENT_ALLREDRAWFLAGS as u64);
    }
    server_check_unattached();
    server_update_socket();
}

fn server_client_lost(client_owner: &ClientRef) {
    cfg_forget_client(client_owner);
    client_owner.borrow_mut().flags |= CLIENT_DEAD as uint64_t;
    server_client_clear_overlay(client_owner);
    status_prompt_clear(client_owner);
    status_message_clear(client_owner);
    let files = client_owner.file_handles();
    crate::file::client_files_interrupt(files, EINTR);
    let registry_owner =
        ClientRegistry::remove(&Rc::downgrade(client_owner)).expect("registered client owner");
    log_debug(format_args!(
        "lost client {}",
        log_pointer(Rc::as_ptr(client_owner))
    ));
    cmd_wait_for_client_lost(client_owner);
    cmdq_next(Some(client_owner));
    if client_owner.borrow().flags & CLIENT_ATTACHED as uint64_t != 0 {
        server_client_attached_lost(client_owner);
        events_fire_client(c"client-detached", std::rc::Rc::clone(client_owner));
    }
    let (named, flags) = {
        let c = client_owner.borrow();
        (c.name.is_some(), c.flags)
    };
    if named && flags & (CLIENT_CONTROL | CLIENT_TERMINAL) as uint64_t != 0 {
        events_fire_client(c"client-closed", std::rc::Rc::clone(client_owner));
    }
    if client_owner.borrow().flags & CLIENT_CONTROL as uint64_t != 0 {
        control_stop(client_owner);
    }
    if client_owner.borrow().flags & CLIENT_TERMINAL as uint64_t != 0 {
        tty_close(client_owner);
    }
    {
        let mut c = client_owner.borrow_mut();
        server_client_set_ttyname(&mut c, None);
        server_client_set_term_name(&mut c, None);
        server_client_set_term_type(&mut c, None);
        server_client_clear_term_caps(&mut c);
        status_free(&mut c.status);
    }
    client_owner.cancel_input_requests();
    let (timers, keytable, message_timer, prompt) = {
        let mut c = client_owner.borrow_mut();
        server_client_replace_title(&mut c, None);
        server_client_replace_path(&mut c, None);
        server_client_set_cwd(&mut c, None);
        server_client_set_exit_session(&mut c, None);
        server_client_set_exit_message(&mut c, None);
        let timers = [
            c.repeat_timer.take(),
            c.click_timer.take(),
            c.exit_timer.take(),
            c.cycle_timer.take(),
        ];
        let keytable = c.keytable.take();
        server_client_set_message(&mut c, None);
        (timers, keytable, c.message_timer.take(), c.prompt.take())
    };
    drop(timers);
    drop(keytable);
    drop(message_timer);
    if let Some(prompt) = prompt {
        prompt_free(&prompt.downgrade());
    }
    format_lost_client(client_owner);
    let environ = client_owner.borrow_mut().environ.take();
    drop(environ);
    let peer = std::mem::replace(&mut client_owner.borrow_mut().peer, refbox::Weak::new());
    unsafe { proc_remove_peer(&peer) };
    let (out_fd, fd) = {
        let mut c = client_owner.borrow_mut();
        (c.out_fd.take(), c.fd.take())
    };
    drop(out_fd);
    drop(fd);
    server_client_retire(registry_owner);
    server_add_accept(0 as ::core::ffi::c_int);
    recalculate_sizes();
    server_check_unattached();
    server_update_socket();
}
/// Like tmux's server_client_unref, free the client one loop turn later, so no
/// event callback on the stack can see its memory or queue go away. Remaining
/// holders, like tmux references, free it when they let go.
fn server_client_retire(registry_owner: ClientRef) {
    defer(move || {
        let holders = Rc::strong_count(&registry_owner) - 1;
        if holders != 0 {
            log_debug(format_args!(
                "client {} retained by {} references",
                log_pointer(Rc::as_ptr(&registry_owner)),
                holders
            ));
        }
        drop(registry_owner);
    });
}
fn server_client_suspend(c_owner: &ClientRef) {
    let (s, flags) = {
        let c = c_owner.borrow();
        (c.session_handle(), c.flags)
    };
    if s.is_none() || flags & CLIENT_UNATTACHEDFLAGS as uint64_t != 0 {
        return;
    }
    tty_stop_tty(c_owner);
    let peer = {
        let mut c = c_owner.borrow_mut();
        c.flags |= CLIENT_SUSPENDED as uint64_t;
        c.peer.clone()
    };
    proc_send(&peer, MSG_SUSPEND, None, &[]);
}
fn server_client_detach(c_owner: &ClientRef, mut msgtype: msgtype) {
    let mut c = c_owner.borrow_mut();
    let mut s: Option<SessionRef> = c.session_handle();
    if s.is_none() || c.flags & CLIENT_NODETACHFLAGS as uint64_t != 0 {
        return;
    }
    c.flags |= CLIENT_EXIT as uint64_t;
    c.exit_type = CLIENT_EXIT_DETACH;
    c.exit_msgtype = msgtype;
    server_client_set_exit_session(
        &mut c,
        Some(s.as_ref().expect("live session").name().clone()),
    );
}
fn server_client_exec(c_owner: &ClientRef, cmd: &CStr) {
    let mut s: Option<SessionRef> = c_owner.borrow().session_handle();
    if cmd.is_empty() {
        return;
    }
    let shell_session_value = if let Some(s_value) = s.as_ref() {
        options_get_string(&s_value.borrow_options_mut(), c"default-shell")
    } else {
        OptionsScope::GlobalSession.with_local(|oo| options_get_string(oo, c"default-shell"))
    };
    let mut shell: &CStr = &shell_session_value;
    if checkshell(Some(shell)) == 0 {
        shell = _PATH_BSHELL;
    }
    let cmd_bytes = cmd.to_bytes_with_nul();
    let shell_bytes = shell.to_bytes_with_nul();
    let mut msg = Vec::with_capacity(cmd_bytes.len() + shell_bytes.len());
    msg.extend_from_slice(cmd_bytes);
    msg.extend_from_slice(shell_bytes);
    c_owner.send_message(MSG_EXEC, None, &msg);
}

fn server_client_update_scrollbar_hover(
    client_owner: &ClientRef,
    mut type_0: ::core::ffi::c_int,
    mut px: ::core::ffi::c_int,
    mut py: ::core::ffi::c_int,
) {
    let Some(session_owner) = client_owner.borrow().session.upgrade() else {
        return;
    };
    let window_owner = (session_owner.current_winlink())
        .try_borrow()
        .expect("live winlink")
        .window_owner
        .as_ref()
        .expect("hover window")
        .clone();
    (|| {
        if type_0 != KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int {
            return;
        }
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|pane| pane.is_visible())
        {
            pane_owner.update_scrollbar_hover(px, py);
        }
    })();
    drop(window_owner);
}

fn server_client_check_mouse_in_pane(
    pane: &Rc<RefCell<window_pane>>,
    x: i32,
    y: i32,
    slider: &mut u32,
) -> key_code_mouse_location {
    pane.mouse_location(x, y, slider)
}

fn server_client_pane_window_id(pane: &Rc<RefCell<window_pane>>) -> u32 {
    let window = pane.window_observer().upgrade().expect("pane event window");
    let id = window.id();
    drop(window);
    id
}

fn server_client_check_mouse(client_owner: &ClientRef, event: &mut key_event) -> key_code {
    let mut selected_pane = None;
    let mut last_pane = None;
    let Some(session_owner) = client_owner.borrow().session.upgrade() else {
        return KEYC_UNKNOWN;
    };
    let s = Some(session_owner.clone());
    let window_owner = (s.as_ref().expect("live session").current_winlink())
        .try_borrow()
        .expect("live winlink")
        .window_owner
        .as_ref()
        .expect("mouse window")
        .clone();
    let result = (|| {
        let mut current_block: u64;
        let mut fwl: refbox::Weak<winlink> = refbox::Weak::new();
        let mut x: u_int = 0;
        let mut y: u_int = 0;
        let mut px: u_int = 0;
        let mut py: u_int = 0;
        let mut n: u_int = 0;
        let mut sl_mpos: u_int = 0 as u_int;
        let mut b: u_int = 0;
        let mut bn: u_int = 0;
        let mut ignore: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
        let mut key: key_code = 0;
        let mut type_0: key_code_type = KEYC_TYPE_NOTYPE;
        let mut loc: key_code_mouse_location = KEYC_MOUSE_LOCATION_NOWHERE;
        log_debug(format_args!(
            "{} mouse {:02x} at {},{} (last {},{}) ({})",
            log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)")),
            { event.m.b },
            { event.m.x },
            { event.m.y },
            { event.m.lx },
            { event.m.ly },
            { client_owner.borrow().mouse_drag_flag }
        ));
        if client_owner.borrow().mouse_last_pane != -(1 as ::core::ffi::c_int) {
            last_pane = Rc::<RefCell<window_pane>>::find_by_id(
                client_owner.borrow().mouse_last_pane as u_int,
            );
            if let Some(pane) = last_pane.as_ref() {
                log_debug(format_args!(
                    "{} mouse last pane %{}",
                    log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)")),
                    { pane.id() }
                ));
            }
        }
        if event.key == KEYC_DOUBLECLICK as ::core::ffi::c_ulong as key_code {
            type_0 = KEYC_TYPE_DOUBLECLICK;
            x = event.m.x;
            y = event.m.y;
            b = event.m.b;
            ignore = 1 as ::core::ffi::c_int;
            log_debug(format_args!(
                "double-click at {},{}",
                (x) as u32,
                (y) as u32
            ));
        } else if event.m.sgr_type != ' ' as i32 as u_int
            && event.m.sgr_b & MOUSE_MASK_DRAG as u_int != 0
            && event.m.sgr_b & MOUSE_MASK_BUTTONS as u_int == 3 as u_int
            || event.m.sgr_type == ' ' as i32 as u_int
                && event.m.b & MOUSE_MASK_DRAG as u_int != 0
                && event.m.b & MOUSE_MASK_BUTTONS as u_int == 3 as u_int
                && event.m.lb & MOUSE_MASK_BUTTONS as u_int == 3 as u_int
        {
            type_0 = KEYC_TYPE_MOUSEMOVE;
            x = event.m.x;
            y = event.m.y;
            b = 0 as u_int;
            log_debug(format_args!("move at {},{}", (x) as u32, (y) as u32));
        } else if event.m.b & MOUSE_MASK_DRAG as u_int != 0 {
            type_0 = KEYC_TYPE_MOUSEDRAG;
            if client_owner.borrow().mouse_drag_flag != 0 {
                x = event.m.x;
                y = event.m.y;
                b = event.m.b;
                if x == event.m.lx && y == event.m.ly {
                    return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                }
                if client_owner.borrow().mouse_drag_status != 0 {
                    x = client_owner.borrow().mouse_drag_x;
                    y = client_owner.borrow().mouse_drag_y;
                }
                log_debug(format_args!("drag update at {},{}", (x) as u32, (y) as u32));
            } else {
                x = event.m.lx;
                y = event.m.ly;
                b = event.m.lb;
                client_owner.borrow_mut().mouse_drag_status = 0;
                log_debug(format_args!("drag start at {},{}", (x) as u32, (y) as u32));
            }
        } else if event.m.b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_UP as u_int
            || event.m.b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_DOWN as u_int
        {
            if event.m.b & MOUSE_MASK_BUTTONS as u_int == MOUSE_WHEEL_UP as u_int {
                type_0 = KEYC_TYPE_WHEELUP;
            } else {
                type_0 = KEYC_TYPE_WHEELDOWN;
            }
            x = event.m.x;
            y = event.m.y;
            b = event.m.b;
            log_debug(format_args!("wheel at {},{}", (x) as u32, (y) as u32));
        } else if event.m.b & MOUSE_MASK_BUTTONS as u_int == 3 as u_int {
            type_0 = KEYC_TYPE_MOUSEUP;
            x = event.m.x;
            y = event.m.y;
            b = event.m.lb;
            if event.m.sgr_type == 'm' as i32 as u_int {
                b = event.m.sgr_b;
            }
            log_debug(format_args!("up at {},{}", (x) as u32, (y) as u32));
        } else {
            if client_owner.borrow().flags & CLIENT_DOUBLECLICK as uint64_t != 0 {
                let timer = client_owner.borrow_mut().click_timer.take();
                drop(timer);
                client_owner.borrow_mut().flags &= !CLIENT_DOUBLECLICK as uint64_t;
                type_0 = KEYC_TYPE_SECONDCLICK;
                x = event.m.x;
                y = event.m.y;
                b = event.m.b;
                log_debug(format_args!(
                    "second-click at {},{}",
                    (x) as u32,
                    (y) as u32
                ));
                client_owner.borrow_mut().flags |= CLIENT_TRIPLECLICK as uint64_t;
                current_block = 16799951812150840583;
            } else if client_owner.borrow().flags & CLIENT_TRIPLECLICK as uint64_t != 0 {
                let timer = client_owner.borrow_mut().click_timer.take();
                drop(timer);
                client_owner.borrow_mut().flags &= !CLIENT_TRIPLECLICK as uint64_t;
                type_0 = KEYC_TYPE_TRIPLECLICK;
                x = event.m.x;
                y = event.m.y;
                b = event.m.b;
                log_debug(format_args!(
                    "triple-click at {},{}",
                    (x) as u32,
                    (y) as u32
                ));
                current_block = 5614288427414743461;
            } else {
                current_block = 16799951812150840583;
            }
            match current_block {
                5614288427414743461 => {}
                _ => {
                    if type_0 as ::core::ffi::c_uint
                        == KEYC_TYPE_NOTYPE as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        type_0 = KEYC_TYPE_MOUSEDOWN;
                        x = event.m.x;
                        y = event.m.y;
                        b = event.m.b;
                        log_debug(format_args!("down at {},{}", (x) as u32, (y) as u32));
                        client_owner.borrow_mut().flags |= CLIENT_DOUBLECLICK as uint64_t;
                    }
                }
            }
        }
        if type_0 as ::core::ffi::c_uint
            == KEYC_TYPE_NOTYPE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
        }
        event.m.s = s.as_ref().expect("live session").id() as ::core::ffi::c_int;
        event.m.w = -(1 as ::core::ffi::c_int);
        event.m.wp = -(1 as ::core::ffi::c_int);
        event.m.ignore = ignore;
        event.m.statusat = status_at_line(client_owner);
        event.m.statuslines = status_line_size(client_owner);
        if event.m.statusat != -(1 as ::core::ffi::c_int)
            && y >= event.m.statusat as u_int
            && y < (event.m.statusat as u_int).wrapping_add(event.m.statuslines)
        {
            if type_0 == KEYC_TYPE_MOUSEDRAG && client_owner.borrow().mouse_drag_flag == 0 {
                client_owner.borrow_mut().mouse_drag_status = 1;
            }
            if let Some(sr) =
                status_get_range(client_owner, x, y.wrapping_sub(event.m.statusat as u_int))
            {
                match sr.type_0 as ::core::ffi::c_uint {
                    0 => return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code,
                    1 => {
                        log_debug(format_args!("mouse range: left"));
                        loc = KEYC_MOUSE_LOCATION_STATUS_LEFT;
                    }
                    2 => {
                        log_debug(format_args!("mouse range: right"));
                        loc = KEYC_MOUSE_LOCATION_STATUS_RIGHT;
                    }
                    3 => {
                        if Rc::<RefCell<window_pane>>::find_by_id(sr.argument).is_none() {
                            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                        }
                        event.m.wp = sr.argument as ::core::ffi::c_int;
                        log_debug(format_args!("mouse range: pane %{}", (event.m.wp) as u32));
                        loc = KEYC_MOUSE_LOCATION_STATUS;
                    }
                    4 => {
                        fwl = s.as_ref().expect("live session").with_winlinks(|links| {
                            winlink_find_by_index(links, sr.argument as ::core::ffi::c_int)
                        });
                        if !fwl.is_alive() {
                            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                        }
                        event.m.w = ((winlink_window(&fwl).as_ref()).expect("live window")).id()
                            as ::core::ffi::c_int;
                        log_debug(format_args!("mouse range: window @{}", (event.m.w) as u32));
                        loc = KEYC_MOUSE_LOCATION_STATUS;
                    }
                    5 => {
                        if crate::session::sessions::find_by_id(sr.argument).is_none() {
                            return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
                        }
                        event.m.s = sr.argument as ::core::ffi::c_int;
                        log_debug(format_args!("mouse range: session ${}", (event.m.s) as u32));
                        loc = KEYC_MOUSE_LOCATION_STATUS;
                    }
                    6 => {
                        log_debug(format_args!("mouse range: user"));
                        loc = KEYC_MOUSE_LOCATION_STATUS;
                    }
                    7 => {
                        n = sr.argument;
                        log_debug(format_args!("mouse range: control {}", (n) as u32));
                        loc = (KEYC_MOUSE_LOCATION_CONTROL0 as ::core::ffi::c_int as u_int)
                            .wrapping_add(n)
                            as key_code_mouse_location;
                    }
                    _ => {}
                }
            } else {
                loc = KEYC_MOUSE_LOCATION_STATUS_DEFAULT;
            }
        }
        if loc as ::core::ffi::c_uint
            == KEYC_MOUSE_LOCATION_NOWHERE as ::core::ffi::c_int as ::core::ffi::c_uint
            && client_owner.borrow().mouse_scrolling_flag != 0
        {
            if let Some(pane) = last_pane.as_ref() {
                loc = KEYC_MOUSE_LOCATION_SCROLLBAR_SLIDER;
                event.m.wp = pane.id() as ::core::ffi::c_int;
                event.m.w = server_client_pane_window_id(pane) as ::core::ffi::c_int;
            }
        } else if loc as ::core::ffi::c_uint
            == KEYC_MOUSE_LOCATION_NOWHERE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            px = x;
            if event.m.statusat == 0 as ::core::ffi::c_int && y >= event.m.statuslines {
                py = y.wrapping_sub(event.m.statuslines);
            } else if event.m.statusat > 0 as ::core::ffi::c_int && y >= event.m.statusat as u_int {
                py = (event.m.statusat - 1 as ::core::ffi::c_int) as u_int;
            } else {
                py = y;
            }
            let view = tty_window_offset(&client_owner.borrow().tty);
            event.m.ox = view.ox;
            event.m.oy = view.oy;
            let (sx, sy) = (view.sx, view.sy);
            log_debug(format_args!(
                "mouse window @{} at {},{} ({}x{})",
                { (window_owner).id() },
                { event.m.ox },
                { event.m.oy },
                { sx },
                { sy }
            ));
            if px > sx || py > sy {
                server_client_update_scrollbar_hover(
                    client_owner,
                    type_0 as ::core::ffi::c_int,
                    -(1 as ::core::ffi::c_int),
                    -(1 as ::core::ffi::c_int),
                );
                return KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
            }
            px = px.wrapping_add(event.m.ox);
            py = py.wrapping_add(event.m.oy);
            server_client_update_scrollbar_hover(
                client_owner,
                type_0 as ::core::ffi::c_int,
                px as ::core::ffi::c_int,
                py as ::core::ffi::c_int,
            );
            if type_0 as ::core::ffi::c_uint
                == KEYC_TYPE_MOUSEDRAG as ::core::ffi::c_int as ::core::ffi::c_uint
                && last_pane.is_some()
            {
                selected_pane = last_pane.clone();
            } else {
                let hit_window_owner = &window_owner;
                selected_pane = hit_window_owner.pane_at(px, py).upgrade();
            }
            match &selected_pane {
                None => {
                    loc = KEYC_MOUSE_LOCATION_EMPTY;
                    event.m.w = (window_owner).id() as ::core::ffi::c_int;
                    log_debug(format_args!(
                        "mouse {},{} on empty area",
                        (x) as u32,
                        (y) as u32
                    ));
                }
                Some(pane_owner) => {
                    loc = server_client_check_mouse_in_pane(
                        pane_owner,
                        px as ::core::ffi::c_int,
                        py as ::core::ffi::c_int,
                        &mut sl_mpos,
                    );
                    if loc as ::core::ffi::c_uint
                        == KEYC_MOUSE_LOCATION_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        log_debug(format_args!(
                            "mouse {},{} on pane %{}",
                            (x) as u32,
                            (y) as u32,
                            { pane_owner.id() }
                        ));
                    } else if loc as ::core::ffi::c_uint
                        == KEYC_MOUSE_LOCATION_BORDER as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        if let Some(range) = pane_owner.status_range(px, py) {
                            n = range.argument;
                            loc = (KEYC_MOUSE_LOCATION_CONTROL0 as ::core::ffi::c_int as u_int)
                                .wrapping_add(n)
                                as key_code_mouse_location;
                        }
                        log_debug(format_args!("mouse on pane %{} border", {
                            pane_owner.id()
                        }));
                    } else if loc as ::core::ffi::c_uint
                        == KEYC_MOUSE_LOCATION_SCROLLBAR_UP as ::core::ffi::c_int
                            as ::core::ffi::c_uint
                        || loc as ::core::ffi::c_uint
                            == KEYC_MOUSE_LOCATION_SCROLLBAR_SLIDER as ::core::ffi::c_int
                                as ::core::ffi::c_uint
                        || loc as ::core::ffi::c_uint
                            == KEYC_MOUSE_LOCATION_SCROLLBAR_DOWN as ::core::ffi::c_int
                                as ::core::ffi::c_uint
                    {
                        log_debug(format_args!("mouse on pane %{} scrollbar", {
                            pane_owner.id()
                        }));
                    }
                    event.m.wp = pane_owner.id() as ::core::ffi::c_int;
                    event.m.w = server_client_pane_window_id(pane_owner) as ::core::ffi::c_int;
                }
            }
        } else {
            server_client_update_scrollbar_hover(
                client_owner,
                type_0 as ::core::ffi::c_int,
                -(1 as ::core::ffi::c_int),
                -(1 as ::core::ffi::c_int),
            );
        }
        if type_0 as ::core::ffi::c_uint
            == KEYC_TYPE_MOUSEDOWN as ::core::ffi::c_int as ::core::ffi::c_uint
            || type_0 as ::core::ffi::c_uint
                == KEYC_TYPE_SECONDCLICK as ::core::ffi::c_int as ::core::ffi::c_uint
            || type_0 as ::core::ffi::c_uint
                == KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if type_0 as ::core::ffi::c_uint
                != KEYC_TYPE_MOUSEDOWN as ::core::ffi::c_int as ::core::ffi::c_uint
                && (event.m.b != client_owner.borrow().click_button
                    || loc as ::core::ffi::c_uint
                        != client_owner.borrow().click_loc as key_code_mouse_location
                            as ::core::ffi::c_uint
                    || event.m.wp != client_owner.borrow().click_wp)
            {
                type_0 = KEYC_TYPE_MOUSEDOWN;
                log_debug(format_args!(
                    "click sequence reset at {},{}",
                    (x) as u32,
                    (y) as u32
                ));
                client_owner.borrow_mut().flags &= !CLIENT_TRIPLECLICK as uint64_t;
                client_owner.borrow_mut().flags |= CLIENT_DOUBLECLICK as uint64_t;
            }
            if type_0 as ::core::ffi::c_uint
                != KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_uint
                && KEYC_CLICK_TIMEOUT != 0 as ::core::ffi::c_int
            {
                client_owner.borrow_mut().click_event = event.m;
                client_owner.borrow_mut().click_button = event.m.b;
                client_owner.borrow_mut().click_loc = loc as ::core::ffi::c_int;
                client_owner.borrow_mut().click_wp = event.m.wp;
                log_debug(format_args!("click timer started"));
                let timeout = Duration::from_millis(KEYC_CLICK_TIMEOUT as u64);
                let timer = client_owner.borrow_mut().click_timer.take();
                drop(timer);
                let timer = server_client_timer(client_owner, timeout, server_client_click_timer);
                client_owner.borrow_mut().click_timer = Some(timer);
            }
        }
        key = KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code;
        if type_0 as ::core::ffi::c_uint
            != KEYC_TYPE_MOUSEDRAG as ::core::ffi::c_int as ::core::ffi::c_uint
            && type_0 as ::core::ffi::c_uint
                != KEYC_TYPE_WHEELUP as ::core::ffi::c_int as ::core::ffi::c_uint
            && type_0 as ::core::ffi::c_uint
                != KEYC_TYPE_WHEELDOWN as ::core::ffi::c_int as ::core::ffi::c_uint
            && type_0 as ::core::ffi::c_uint
                != KEYC_TYPE_DOUBLECLICK as ::core::ffi::c_int as ::core::ffi::c_uint
            && type_0 as ::core::ffi::c_uint
                != KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_uint
            && client_owner.borrow().mouse_drag_flag != 0 as ::core::ffi::c_int
        {
            let release = client_owner.borrow_mut().mouse_drag_release.take();
            if let Some(release) = release {
                release(&mut event.m);
            }
            client_owner.borrow_mut().mouse_drag_update = None;
            client_owner.borrow_mut().mouse_drag_release = None;
            client_owner.borrow_mut().mouse_scrolling_flag = 0 as ::core::ffi::c_int;
            type_0 = KEYC_TYPE_MOUSEDRAGEND;
            client_owner.borrow_mut().mouse_drag_flag = 0 as ::core::ffi::c_int;
            client_owner.borrow_mut().mouse_slider_mpos = -(1 as ::core::ffi::c_int);
            client_owner.borrow_mut().mouse_last_pane = -(1 as ::core::ffi::c_int);
        }
        if type_0 as ::core::ffi::c_uint
            == KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_uint
            && loc as ::core::ffi::c_uint
                == KEYC_MOUSE_LOCATION_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            key = KEYC_MOUSEMOVE_PANE as ::core::ffi::c_ulong as key_code;
            if let Some(pane_owner) = selected_pane.as_ref().filter(|pane_owner| {
                !window_owner
                    .active_pane()
                    .ptr_eq(&Rc::downgrade(pane_owner))
                    && options_get_number(
                        &s.as_ref().expect("live session").borrow_options_mut(),
                        c"focus-follows-mouse",
                    ) != 0
            }) {
                WindowRef::clone(&(window_owner)).redraw_active_switch(selected_pane.as_ref());
                WindowRef::clone(&(window_owner)).select_pane(pane_owner, true);
                server_redraw_window_borders(&(window_owner));
                server_status_window(&(window_owner));
            }
        }
        if type_0 as ::core::ffi::c_uint
            == KEYC_TYPE_MOUSEDRAG as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if client_owner.borrow().mouse_drag_update.is_some() {
                key = KEYC_DRAGGING as ::core::ffi::c_ulong as key_code;
            }
            if client_owner.borrow().mouse_drag_flag == 0 as ::core::ffi::c_int {
                if client_owner.borrow().mouse_drag_status != 0 {
                    client_owner.borrow_mut().mouse_drag_x = x;
                    client_owner.borrow_mut().mouse_drag_y = y;
                } else {
                    client_owner.borrow_mut().mouse_drag_x = px;
                    client_owner.borrow_mut().mouse_drag_y = py;
                }
            }
            client_owner.borrow_mut().mouse_drag_flag =
                (b & MOUSE_MASK_BUTTONS as u_int).wrapping_add(1 as u_int) as ::core::ffi::c_int;
            if client_owner.borrow().mouse_drag_status == 0 && last_pane.is_none() {
                let hit_window_owner = &window_owner;
                selected_pane = hit_window_owner.pane_at(px, py).upgrade();
                last_pane = selected_pane.clone();
                if let Some(selected_pane_value) = selected_pane.as_ref() {
                    client_owner.borrow_mut().mouse_last_pane =
                        selected_pane_value.id() as ::core::ffi::c_int;
                }
            }
            if client_owner.borrow().mouse_scrolling_flag == 0 as ::core::ffi::c_int
                && loc as ::core::ffi::c_uint
                    == KEYC_MOUSE_LOCATION_SCROLLBAR_SLIDER as ::core::ffi::c_int
                        as ::core::ffi::c_uint
            {
                client_owner.borrow_mut().mouse_scrolling_flag = 1 as ::core::ffi::c_int;
                if event.m.statusat == 0 as ::core::ffi::c_int {
                    client_owner.borrow_mut().mouse_slider_mpos =
                        sl_mpos.wrapping_add(event.m.statuslines) as ::core::ffi::c_int;
                } else {
                    client_owner.borrow_mut().mouse_slider_mpos = sl_mpos as ::core::ffi::c_int;
                }
            }
        }
        if key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code {
            if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_1 as u_int {
                bn = 1 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_2 as u_int {
                bn = 2 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_3 as u_int {
                bn = 3 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_6 as u_int {
                bn = 6 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_7 as u_int {
                bn = 7 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_8 as u_int {
                bn = 8 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_9 as u_int {
                bn = 9 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_10 as u_int {
                bn = 10 as u_int;
            } else if b & MOUSE_MASK_BUTTONS as u_int == MOUSE_BUTTON_11 as u_int {
                bn = 11 as u_int;
            } else {
                bn = 0 as u_int;
            }
            key = ((type_0 as ::core::ffi::c_ulonglong) << 32 as ::core::ffi::c_int
                | (bn as ::core::ffi::c_ulonglong) << KEYC_MOUSE_BUTTON_SHIFT
                | (loc as ::core::ffi::c_ulonglong) << KEYC_MOUSE_LOCATION_SHIFT)
                as key_code;
        }
        if b & MOUSE_MASK_META as u_int != 0 {
            key |= KEYC_META;
        }
        if b & MOUSE_MASK_CTRL as u_int != 0 {
            key |= KEYC_CTRL;
        }
        if b & MOUSE_MASK_SHIFT as u_int != 0 {
            key |= KEYC_SHIFT;
        }
        if log_get_level() != 0 as ::core::ffi::c_int {
            let key_string = key_string_format(key, true);
            log_debug(format_args!("mouse key is {}", log_cstr(&key_string)));
        }
        key
    })();
    drop(window_owner);
    result
}
fn server_client_update_theme_colours(c_owner: Option<&ClientRef>) {
    let mut theme: client_theme = THEME_UNKNOWN;
    let mut i: u_int = 0;
    let mut colour: ::core::ffi::c_int = 0;
    let mut option: ::core::ffi::c_int = 0;
    let Some(owner) = c_owner else {
        return;
    };
    option = OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"theme"))
        as ::core::ffi::c_int;
    if option == 1 as ::core::ffi::c_int {
        i = 0 as u_int;
        while i < COLOUR_THEME_COUNT as u_int {
            owner.borrow_mut().theme_colours[i as usize] = colour_theme_terminal_colour(i);
            i = i.wrapping_add(1);
        }
        return;
    }
    let mut ft_owner = format_create(c_owner, None, FORMAT_NONE, FORMAT_NOJOBS);
    let ft = &mut *ft_owner;
    format_defaults(ft, c_owner, None, (refbox::Weak::new()).clone(), None);
    let (client_theme, reported_bg) = {
        let c = owner.borrow();
        (c.theme, c.tty.reported_colours().1)
    };
    theme = client_theme;
    if theme as ::core::ffi::c_uint == THEME_UNKNOWN as ::core::ffi::c_int as ::core::ffi::c_uint {
        theme = colour_totheme(reported_bg);
    }
    if option == 2 as ::core::ffi::c_int {
        theme = THEME_LIGHT;
    } else if option == 3 as ::core::ffi::c_int {
        theme = THEME_DARK;
    }
    i = 0 as u_int;
    while i < COLOUR_THEME_COUNT as u_int {
        owner.borrow_mut().theme_colours[i as usize] = 8 as ::core::ffi::c_int;
        if let Some(name) = colour_theme_option(i, theme) {
            let value = OptionsScope::GlobalServer.with_local(|oo| options_get_string(oo, name));
            let expanded = format_expand_cstring(ft, &value);
            colour = colour_parse_cstr(expanded.as_c_str()).unwrap_or(-1);
            if !(colour == -(1 as ::core::ffi::c_int) || colour & COLOUR_FLAG_THEME != 0) {
                owner.borrow_mut().theme_colours[i as usize] = colour;
            }
        }
        i = i.wrapping_add(1);
    }
    drop(ft_owner);
}
fn server_client_is_bracket_paste(c: &mut client, mut key: key_code) -> ::core::ffi::c_int {
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_PASTE_START as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
    {
        c.flags = (c.flags as ::core::ffi::c_ulonglong | CLIENT_BRACKETPASTING) as uint64_t;
        c.paste_time = current_time();
        log_debug(format_args!(
            "{}: bracket paste on",
            log_cstr(c.name.as_deref().unwrap_or(c"(null)"))
        ));
        return 0 as ::core::ffi::c_int;
    }
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_PASTE_END as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
    {
        c.flags = (c.flags as ::core::ffi::c_ulonglong & !CLIENT_BRACKETPASTING) as uint64_t;
        log_debug(format_args!(
            "{}: bracket paste off",
            log_cstr(c.name.as_deref().unwrap_or(c"(null)"))
        ));
        return 0 as ::core::ffi::c_int;
    }
    (c.flags as ::core::ffi::c_ulonglong & CLIENT_BRACKETPASTING != 0) as ::core::ffi::c_int
}
fn server_client_is_assume_paste(c: &mut client) -> ::core::ffi::c_int {
    let mut s: Option<SessionRef> = (*c).session_handle();
    let mut t: ::core::ffi::c_int = 0;
    if c.flags as ::core::ffi::c_ulonglong & CLIENT_BRACKETPASTING != 0 {
        return 0 as ::core::ffi::c_int;
    }
    t = options_get_number(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"assume-paste-time",
    ) as ::core::ffi::c_int;
    if t == 0 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    if c.tty.has(TTYC_ENBP) {
        return 0 as ::core::ffi::c_int;
    }
    let elapsed = c.activity_time.duration_since(c.last_activity_time);
    if elapsed
        .is_ok_and(|elapsed| elapsed.as_secs() == 0 && elapsed < Duration::from_millis(t as u64))
    {
        if c.flags as ::core::ffi::c_ulonglong & CLIENT_ASSUMEPASTING != 0 {
            return 1 as ::core::ffi::c_int;
        }
        c.flags = (c.flags as ::core::ffi::c_ulonglong | CLIENT_ASSUMEPASTING) as uint64_t;
        c.paste_time = current_time();
        log_debug(format_args!(
            "{}: assume paste on",
            log_cstr(c.name.as_deref().unwrap_or(c"(null)"))
        ));
        return 0 as ::core::ffi::c_int;
    }
    if c.flags as ::core::ffi::c_ulonglong & CLIENT_ASSUMEPASTING != 0 {
        c.flags = (c.flags as ::core::ffi::c_ulonglong & !CLIENT_ASSUMEPASTING) as uint64_t;
        log_debug(format_args!(
            "{}: assume paste off",
            log_cstr(c.name.as_deref().unwrap_or(c"(null)"))
        ));
    }
    0 as ::core::ffi::c_int
}
fn server_client_update_latest(c_owner: &ClientRef) {
    use crate::session::Session;
    use crate::window::Window;

    let Some(session) = c_owner.attached_session().upgrade() else {
        return;
    };
    let link = session.current_winlink();
    let window = winlink_window(&link).expect("attached session window");
    if !window.set_latest_client(Some(c_owner)) {
        drop(window);
        return;
    }
    let use_latest = options_get_number(&window.borrow_options_mut(), c"window-size")
        == WINDOW_SIZE_LATEST as i64;
    if use_latest {
        recalculate_size(&window, 0);
    }
    drop(window);
    events_fire_client(c"client-active", c_owner.clone());
}

fn server_client_repeat_time(c: &client, bd: &KeyBindingCommand) -> u_int {
    let mut s: Option<SessionRef> = (*c).session_handle();
    let mut repeat: u_int = 0;
    let mut initial: u_int = 0;
    if !bd.flags & KEY_BINDING_REPEAT != 0 {
        return 0 as u_int;
    }
    repeat = options_get_number(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"repeat-time",
    ) as u_int;
    if repeat == 0 as u_int {
        return 0 as u_int;
    }
    if !c.flags & CLIENT_REPEAT as uint64_t != 0 || bd.key != c.last_key {
        initial = options_get_number(
            &s.as_ref().expect("live session").borrow_options_mut(),
            c"initial-repeat-time",
        ) as u_int;
        if initial != 0 as u_int {
            repeat = initial;
        }
    }
    repeat
}
fn server_client_handle_dead_key(pane: Option<&Rc<RefCell<window_pane>>>, key: key_code) -> i32 {
    pane.is_some_and(|pane| pane.close_after_key(key)) as i32
}
unsafe fn server_client_key_callback(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut owned: QueuedKeyEvent,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut current_block: u64;
    let event: &mut key_event = &mut owned.0;
    let mut key: key_code = event.key;
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut target_pane = None;
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut table: std::rc::Rc<std::cell::RefCell<key_table>>;
    let mut table_owner;
    let mut first: std::rc::Rc<std::cell::RefCell<key_table>>;
    let mut bd: Option<KeyBindingCommand> = None;
    let mut repeat: u_int = 0;
    let mut flags: uint64_t = 0;
    let mut prefix_delay: uint64_t = 0;
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut key0: key_code = 0;
    let mut prefix: key_code = 0;
    let mut prefix2: key_code = 0;
    let c_owner = event.resolve_client(|| cmdq_get_client(Some(&*item)));
    let Some(c_owner) = c_owner else {
        return CmdOutcome::Normal;
    };
    s = c_owner.borrow().session_handle();
    if let Some(session) = s
        .as_ref()
        .filter(|_| c_owner.borrow().flags & CLIENT_UNATTACHEDFLAGS as uint64_t == 0)
    {
        wl = session.current_winlink();
        let activity_time = {
            let mut c = c_owner.borrow_mut();
            c.last_activity_time = c.activity_time;
            c.activity_time = SystemTime::now();
            c.activity_time
        };
        session.update_activity(Some(activity_time));
        event.m.valid = 0 as ::core::ffi::c_int;
        if key == KEYC_MOUSE as ::core::ffi::c_ulong as key_code
            || key == KEYC_DOUBLECLICK as ::core::ffi::c_ulong as key_code
        {
            if c_owner.borrow().flags & CLIENT_READONLY as uint64_t != 0 {
                current_block = 1578459965781631232;
            } else {
                key = server_client_check_mouse(&c_owner, event);
                if key == KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code {
                    current_block = 1578459965781631232;
                } else {
                    event.m.valid = 1 as ::core::ffi::c_int;
                    event.m.key = key;
                    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                        == KEYC_DRAGGING as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                    {
                        let update = c_owner.borrow_mut().mouse_drag_update.take();
                        if let Some(mut update) = update {
                            update(&mut event.m);
                            let mut c = c_owner.borrow_mut();
                            if c.mouse_drag_update.is_none() {
                                c.mouse_drag_update = Some(update);
                            }
                        }
                        current_block = 1578459965781631232;
                    } else {
                        event.key = key;
                        current_block = 4495394744059808450;
                    }
                }
            }
        } else {
            current_block = 4495394744059808450;
        }
        match current_block {
            1578459965781631232 => {}
            _ => {
                let mouse_fs = if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                    == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                    || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                        >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                            << 32 as ::core::ffi::c_int
                        && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                            <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int
                                as ::core::ffi::c_ulonglong)
                                << 32 as ::core::ffi::c_int
                {
                    cmd_find_state::find_from_mouse(&event.m)
                } else {
                    None
                };
                fs = mouse_fs.unwrap_or_else(|| {
                    cmd_find_state::find_from_client(Some(&c_owner), 0 as ::core::ffi::c_int)
                        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int))
                });
                target_pane = fs.pane_handle();
                if (key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                    == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                    || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                        >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                            << 32 as ::core::ffi::c_int
                        && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                            <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int
                                as ::core::ffi::c_ulonglong)
                                << 32 as ::core::ffi::c_int)
                    && options_get_number(&session.borrow_options_mut(), c"mouse") == 0
                {
                    current_block = 15469183920764600035;
                } else {
                    let bracket_paste =
                        server_client_is_bracket_paste(&mut c_owner.borrow_mut(), key) != 0;
                    if bracket_paste
                        || (!(key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                            == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                            || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                                >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int
                                    as ::core::ffi::c_ulonglong)
                                    << 32 as ::core::ffi::c_int
                                && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                                    <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int
                                        as ::core::ffi::c_ulonglong)
                                        << 32 as ::core::ffi::c_int)
                            && key != KEYC_FOCUS_IN as ::core::ffi::c_ulong as key_code
                            && key != KEYC_FOCUS_OUT as ::core::ffi::c_ulong as key_code
                            && !(key as ::core::ffi::c_ulonglong) & KEYC_SENT != 0
                            && server_client_is_assume_paste(&mut c_owner.borrow_mut()) != 0)
                    {
                        current_block = 3436715649514806935;
                    } else if key == KEYC_FOCUS_IN as ::core::ffi::c_ulong as key_code
                        || key == KEYC_FOCUS_OUT as ::core::ffi::c_ulong as key_code
                    {
                        current_block = 15469183920764600035;
                    } else {
                        if server_client_is_default_key_table(
                            &c_owner.borrow(),
                            &c_owner
                                .borrow()
                                .keytable
                                .as_ref()
                                .expect("key table")
                                .borrow(),
                        ) != 0
                            && target_pane.is_some()
                            && {
                                wme = target_pane.as_ref().expect("key pane").mode_entry();
                                wme.is_alive()
                            }
                            && wme
                                .try_borrow()
                                .expect("live mode entry")
                                .mode
                                .key_table
                                .is_some()
                        {
                            let key_table = wme
                                .try_borrow()
                                .expect("live mode entry")
                                .mode
                                .key_table
                                .expect("non-null function pointer");
                            table_owner =
                                key_bindings_get_table(key_table(wme), 1 as ::core::ffi::c_int);
                        } else {
                            table_owner = c_owner.borrow().keytable.clone();
                        }
                        table = table_owner.as_ref().expect("key table").clone();
                        first = table.clone();
                        '_table_changed: loop {
                            prefix = options_get_number(&session.borrow_options_mut(), c"prefix")
                                as key_code;
                            prefix2 = options_get_number(&session.borrow_options_mut(), c"prefix2")
                                as key_code;
                            key0 = (key as ::core::ffi::c_ulonglong
                                & (KEYC_MASK_KEY | KEYC_MASK_MODIFIERS))
                                as key_code;
                            if (key0
                                == prefix as ::core::ffi::c_ulonglong
                                    & (KEYC_MASK_KEY | KEYC_MASK_MODIFIERS)
                                || key0
                                    == prefix2 as ::core::ffi::c_ulonglong
                                        & (KEYC_MASK_KEY | KEYC_MASK_MODIFIERS))
                                && table.borrow().name.as_c_str() != c"prefix"
                            {
                                server_client_set_key_table(&c_owner, Some(c"prefix"));
                                c_owner.borrow_mut().flags |= CLIENT_REDRAWSTATUS as u64;
                                current_block = 1578459965781631232;
                                break;
                            } else {
                                flags = c_owner.borrow().flags;
                                loop {
                                    match &target_pane {
                                        None => {
                                            log_debug(format_args!(
                                                "key table {} (no pane)",
                                                log_cstr(&table.borrow().name)
                                            ));
                                        }
                                        Some(target_pane) => {
                                            log_debug(format_args!(
                                                "key table {} (pane %{})",
                                                log_cstr(&table.borrow().name),
                                                { target_pane.id() }
                                            ));
                                        }
                                    }
                                    if c_owner.borrow().flags & CLIENT_REPEAT as uint64_t != 0 {
                                        log_debug(format_args!("currently repeating"));
                                    }
                                    bd = key_bindings_get(&table.borrow(), key0)
                                        .map(|bd| bd.command());
                                    prefix_delay = OptionsScope::GlobalServer
                                        .with_local(|oo| options_get_number(oo, c"prefix-timeout"))
                                        as uint64_t;
                                    if prefix_delay > 0 as uint64_t
                                        && table.borrow().name.as_c_str() == c"prefix"
                                        && server_client_key_table_activity_diff(&c_owner.borrow())
                                            > prefix_delay
                                    {
                                        if bd.is_some()
                                            && c_owner.borrow().flags & CLIENT_REPEAT as uint64_t
                                                != 0
                                            && bd.as_ref().unwrap().flags & KEY_BINDING_REPEAT != 0
                                        {
                                            log_debug(format_args!(
                                                "prefix timeout ignored, repeat is active"
                                            ));
                                        } else {
                                            log_debug(format_args!("prefix timeout exceeded"));
                                            server_client_set_key_table(
                                                &c_owner,
                                                nullable_cstr(::core::ptr::null::<
                                                    ::core::ffi::c_char,
                                                >(
                                                )),
                                            );
                                            table_owner = c_owner.borrow().keytable.clone();
                                            table =
                                                table_owner.as_ref().expect("key table").clone();
                                            first = table.clone();
                                            c_owner.borrow_mut().flags |=
                                                CLIENT_REDRAWSTATUS as u64;
                                            continue '_table_changed;
                                        }
                                    }
                                    if let Some(bd_value) = bd.as_ref() {
                                        if c_owner.borrow().flags & CLIENT_REPEAT as uint64_t != 0
                                            && !bd_value.flags & KEY_BINDING_REPEAT != 0
                                        {
                                            current_block = 5891011138178424807;
                                            break;
                                        } else {
                                            current_block = 13763002826403452995;
                                            break;
                                        }
                                    } else if key0 != KEYC_ANY as ::core::ffi::c_ulong as key_code {
                                        key0 = KEYC_ANY as ::core::ffi::c_ulong as key_code;
                                    } else {
                                        if key
                                            == KEYC_MOUSEMOVE_PANE as ::core::ffi::c_ulong
                                                as key_code
                                            || key
                                                == KEYC_MOUSEMOVE_STATUS as ::core::ffi::c_ulong
                                                    as key_code
                                            || key
                                                == KEYC_MOUSEMOVE_STATUS_LEFT
                                                    as ::core::ffi::c_ulong
                                                    as key_code
                                            || key
                                                == KEYC_MOUSEMOVE_STATUS_RIGHT
                                                    as ::core::ffi::c_ulong
                                                    as key_code
                                            || key
                                                == KEYC_MOUSEMOVE_STATUS_DEFAULT
                                                    as ::core::ffi::c_ulong
                                                    as key_code
                                            || key
                                                == KEYC_MOUSEMOVE_BORDER as ::core::ffi::c_ulong
                                                    as key_code
                                        {
                                            current_block = 15469183920764600035;
                                            break '_table_changed;
                                        }
                                        log_debug(format_args!(
                                            "not found in key table {}",
                                            log_cstr(&table.borrow().name)
                                        ));
                                        if server_client_is_default_key_table(
                                            &c_owner.borrow(),
                                            &table.borrow(),
                                        ) == 0
                                            || c_owner.borrow().flags & CLIENT_REPEAT as uint64_t
                                                != 0
                                        {
                                            current_block = 13484060386966298149;
                                            break;
                                        } else {
                                            current_block = 11796148217846552555;
                                            break;
                                        }
                                    }
                                }
                                match current_block {
                                    11796148217846552555 => {
                                        if !std::rc::Rc::ptr_eq(&first, &table)
                                            && !flags & CLIENT_REPEAT as uint64_t != 0
                                        {
                                            current_block = 10435735846551762309;
                                            break;
                                        } else {
                                            current_block = 15469183920764600035;
                                            break;
                                        }
                                    }
                                    13763002826403452995 => {
                                        log_debug(format_args!(
                                            "found in key table {}",
                                            log_cstr(&table.borrow().name)
                                        ));
                                        let bd = bd.take().expect("matched binding");
                                        repeat = server_client_repeat_time(&c_owner.borrow(), &bd);
                                        if repeat != 0 as u_int {
                                            c_owner.borrow_mut().flags |= CLIENT_REPEAT as uint64_t;
                                            c_owner.borrow_mut().last_key = bd.key;
                                            let timeout = Duration::from_millis(repeat as u64);
                                            let timer = c_owner.borrow_mut().repeat_timer.take();
                                            drop(timer);
                                            let timer = server_client_timer(
                                                &c_owner,
                                                timeout,
                                                server_client_repeat_timer,
                                            );
                                            c_owner.borrow_mut().repeat_timer = Some(timer);
                                        } else {
                                            c_owner.borrow_mut().flags &=
                                                !CLIENT_REPEAT as uint64_t;
                                            server_client_set_key_table(
                                                &c_owner,
                                                nullable_cstr(::core::ptr::null::<
                                                    ::core::ffi::c_char,
                                                >(
                                                )),
                                            );
                                        }
                                        c_owner.borrow_mut().flags |= CLIENT_REDRAWSTATUS as u64;
                                        key_bindings_dispatch(
                                            bd,
                                            Some(item_handle),
                                            Some(&c_owner),
                                            Some(&*event),
                                            &fs,
                                        );
                                        current_block = 1578459965781631232;
                                        break;
                                    }
                                    13484060386966298149 => {
                                        log_debug(format_args!("trying in root table"));
                                        server_client_set_key_table(
                                            &c_owner,
                                            nullable_cstr(
                                                ::core::ptr::null::<::core::ffi::c_char>(),
                                            ),
                                        );
                                        table_owner = c_owner.borrow().keytable.clone();
                                        table = table_owner.as_ref().expect("key table").clone();
                                        if c_owner.borrow().flags & CLIENT_REPEAT as uint64_t != 0 {
                                            first = table.clone();
                                        }
                                        c_owner.borrow_mut().flags &= !CLIENT_REPEAT as uint64_t;
                                        c_owner.borrow_mut().flags |= CLIENT_REDRAWSTATUS as u64;
                                    }
                                    _ => {
                                        log_debug(format_args!(
                                            "found in key table {} (not repeating)",
                                            log_cstr(&table.borrow().name)
                                        ));
                                        server_client_set_key_table(
                                            &c_owner,
                                            nullable_cstr(
                                                ::core::ptr::null::<::core::ffi::c_char>(),
                                            ),
                                        );
                                        table_owner = c_owner.borrow().keytable.clone();
                                        table = table_owner.as_ref().expect("key table").clone();
                                        first = table.clone();
                                        c_owner.borrow_mut().flags &= !CLIENT_REPEAT as uint64_t;
                                        c_owner.borrow_mut().flags |= CLIENT_REDRAWSTATUS as u64;
                                    }
                                }
                            }
                        }
                        match current_block {
                            1578459965781631232 => {}
                            15469183920764600035 => {}
                            _ => {
                                server_client_set_key_table(&c_owner, None);
                                c_owner.borrow_mut().flags |= CLIENT_REDRAWSTATUS as u64;
                                current_block = 1578459965781631232;
                            }
                        }
                    }
                    match current_block {
                        1578459965781631232 => {}
                        15469183920764600035 => {}
                        _ => {
                            if c_owner.borrow().flags & CLIENT_READONLY as uint64_t != 0 {
                                current_block = 1578459965781631232;
                            } else {
                                if let Some(bytes) = event.bytes_ptr_len() {
                                    target_pane
                                        .as_ref()
                                        .expect("paste target pane")
                                        .paste(key, bytes);
                                }
                                key = KEYC_NONE as ::core::ffi::c_ulong as key_code;
                                current_block = 1578459965781631232;
                            }
                        }
                    }
                }
                match current_block {
                    1578459965781631232 => {}
                    _ => {
                        let pane_owner = target_pane.as_ref();
                        if !(server_client_handle_dead_key(pane_owner, key) != 0)
                            && !(c_owner.borrow().flags & CLIENT_READONLY as uint64_t != 0)
                        {
                            if let Some(pane_owner) = pane_owner {
                                pane_owner.key(Some(&c_owner), wl.clone(), key, Some(&mut event.m));
                            }
                        }
                    }
                }
            }
        }
    }
    if !s.is_none() && key != KEYC_FOCUS_OUT as ::core::ffi::c_ulong as key_code {
        server_client_update_latest(&c_owner);
    }
    CmdOutcome::Normal
}

struct QueuedKeyEvent(Box<key_event>, Option<ClientRef>);

fn server_client_handle_menu_key(owner: &ClientRef, event: &key_event) -> ::core::ffi::c_int {
    let (window, menu) = {
        let session = owner.borrow().session_handle().expect("live session");
        let retained = winlink_window(&session.current_winlink()).expect("current window");
        (
            crate::shared::window::WindowRef::downgrade(&retained),
            retained.menu_observer(),
        )
    };
    let Some(menu) = menu else { return 0 };
    let mut new_event = event.metadata_snapshot();
    if event.key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || event.key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && event.key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
    {
        let m = &mut new_event.m;
        m.statusat = status_at_line(owner);
        m.statuslines = status_line_size(owner);
        let tty_window_view { ox, oy, .. } = tty_window_offset(&owner.borrow().tty);
        m.x = m.x.wrapping_add(ox);
        if m.statusat == 0 as ::core::ffi::c_int {
            if m.y < m.statuslines {
                m.y = UINT_MAX as u_int;
                m.x = m.y;
            } else {
                m.y = m.y.wrapping_sub(m.statuslines).wrapping_add(oy);
            }
        } else if m.statusat > 0 as ::core::ffi::c_int && m.y >= m.statusat as u_int {
            m.y = UINT_MAX as u_int;
            m.x = m.y;
        } else {
            m.y = m.y.wrapping_add(oy);
        }
    }
    if menu_key(Some(owner), &menu, &new_event) == 1 {
        menu_close(&window, Some(&menu));
    }
    1 as ::core::ffi::c_int
}
fn server_client_handle_key0(
    owner: &ClientRef,
    mut owned: Box<key_event>,
    after_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    next: Option<&mut std::rc::Weak<std::cell::RefCell<cmdq_item>>>,
) -> ::core::ffi::c_int {
    let event: &mut key_event = &mut owned;
    let mut s: Option<SessionRef> = owner.borrow().session_handle();
    let item_allocation;
    if s.is_none() || owner.borrow().flags & CLIENT_UNATTACHEDFLAGS as uint64_t != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if event.key == KEYC_REPORT_LIGHT_THEME as ::core::ffi::c_ulong as key_code {
        server_client_report_theme(owner, THEME_LIGHT);
        return 0 as ::core::ffi::c_int;
    }
    if event.key == KEYC_REPORT_DARK_THEME as ::core::ffi::c_ulong as key_code {
        server_client_report_theme(owner, THEME_DARK);
        return 0 as ::core::ffi::c_int;
    }
    if !owner.borrow().flags & CLIENT_READONLY as uint64_t != 0 {
        if !owner.borrow().message_string.is_none() {
            if owner.borrow().message_ignore_keys != 0 {
                return 0 as ::core::ffi::c_int;
            }
            status_message_clear(owner);
        }
        if let Some(result) = server_client_overlay_key(owner, event) {
            match result {
                0 => return 0 as ::core::ffi::c_int,
                1 => {
                    server_client_clear_overlay(owner);
                    return 0 as ::core::ffi::c_int;
                }
                _ => {}
            }
        }
        server_client_clear_overlay(owner);
        let active_pane_owner =
            ((winlink_window(&s.as_ref().expect("live session").current_winlink()).as_ref())
                .expect("live window"))
            .active_pane()
            .upgrade();
        if server_client_handle_dead_key(active_pane_owner.as_ref(), event.key) != 0 {
            return 0 as ::core::ffi::c_int;
        }
        if server_client_handle_menu_key(owner, event) != 0 {
            return 0 as ::core::ffi::c_int;
        }
        if owner.borrow().prompt.is_some() {
            match status_prompt_key(owner, event.key, Some(&event.m)) as ::core::ffi::c_uint {
                1 | 2 => return 0 as ::core::ffi::c_int,
                _ => {}
            }
        }
        let prompt_window = winlink_window(&s.as_ref().expect("live session").current_winlink())
            .expect("prompt window");
        let mut prompt_pane = prompt_window.active_pane().upgrade();
        if !prompt_pane
            .as_ref()
            .is_some_and(|pane| pane.prompt_position(true).is_some())
        {
            prompt_pane = prompt_window
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
                .find(|pane| pane.prompt_position(true).is_some() && pane.is_visible());
        }
        drop(prompt_window);
        if let Some(pane_owner) =
            prompt_pane.filter(|pane| pane.prompt_position(true).is_some() && pane.is_visible())
        {
            match pane_owner.prompt_key(Some(owner), event.key, Some(&mut event.m))
                as ::core::ffi::c_uint
            {
                1..=3 => return 0 as ::core::ffi::c_int,
                0 if (event.key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                    == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                    || event.key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                        >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int
                            as ::core::ffi::c_ulonglong)
                            << 32 as ::core::ffi::c_int
                        && event.key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                            <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int
                                as ::core::ffi::c_ulonglong)
                                << 32 as ::core::ffi::c_int) =>
                {
                    return 0 as ::core::ffi::c_int;
                }
                _ => {}
            }
        }
    }
    let client_owner = if after_handle.is_none() {
        None
    } else {
        event.client = std::rc::Rc::downgrade(owner);
        Some(owner.clone())
    };
    let queued_event = QueuedKeyEvent(owned, client_owner);
    item_allocation = cmdq_get_callback_owned(
        c"server_client_key_callback",
        Some(Box::new(move |_item| unsafe {
            server_client_key_callback(_item, queued_event)
        })),
    );
    if let Some(after) = after_handle {
        let inserted = cmdq_insert_after(after, item_allocation);
        if let Some(next) = next {
            *next = inserted;
        }
        return 1;
    }
    cmdq_append(Some(owner), item_allocation);
    1 as ::core::ffi::c_int
}
fn server_client_handle_key(owner: &ClientRef, event: Box<key_event>) -> ::core::ffi::c_int {
    server_client_handle_key0(owner, event, None, None)
}
fn server_client_handle_key_after(
    owner: &ClientRef,
    event: Box<key_event>,
    after_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    next: Option<&mut std::rc::Weak<std::cell::RefCell<cmdq_item>>>,
) -> ::core::ffi::c_int {
    server_client_handle_key0(owner, event, after_handle, next)
}
fn server_client_loop() {
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        server_client_check_window_resize(&window_owner);
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            pane_owner.notify_style_changed();
        }
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
    let mut registry_c_owner = ClientRegistry::first();
    while let Some(client_owner) = registry_c_owner {
        server_client_check_exit(&client_owner, 0 as ::core::ffi::c_int);
        let session = client_owner.borrow().session_handle();
        if session.is_some_and(|session| session.current_winlink().is_alive()) {
            server_client_check_modes(&client_owner);
            server_client_check_redraw(&client_owner);
            server_client_reset_state(&client_owner);
        }
        registry_c_owner = ClientRegistry::next(&client_owner);
    }
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            pane_owner.finish_cycle();
        }
        check_window_name(&window_owner);
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            pane_owner.send_theme_update();
        }
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
}
fn server_client_check_window_resize(owner: &WindowRef) {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if owner.pending_resize().is_none() {
        return;
    }
    wl = owner.next_winlink(None);
    while wl.is_alive() {
        if wl
            .try_borrow()
            .expect("live winlink")
            .session
            .upgrade()
            .is_some_and(|owner| owner.is_attached() && owner.current_winlink() == wl)
        {
            break;
        }
        wl = owner.next_winlink(Some(wl.clone()));
    }
    if !wl.is_alive() {
        return;
    }
    log_debug(format_args!(
        "{}: resizing window @{}",
        "server_client_check_window_resize",
        { (owner).id() }
    ));
    owner.apply_pending_resize();
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PromptCursor {
    mode: ::core::ffi::c_int,
    cx: u_int,
    cy: u_int,
}

fn server_client_prompt_cursor(
    owner: &ClientRef,
    pane: &Rc<RefCell<window_pane>>,
    mut cursor: PromptCursor,
) -> Option<PromptCursor> {
    let mut r = Vec::new();
    let (px, py) = pane.prompt_position(status_at_line(owner) == 0)?;
    cursor.mode &= !MODE_CURSOR;
    let tty_window_view { ox, oy, sx, sy, .. } = owner.terminal_view();
    if px < ox as ::core::ffi::c_int
        || px > ox.wrapping_add(sx) as ::core::ffi::c_int
        || py < oy as ::core::ffi::c_int
        || py > oy.wrapping_add(sy) as ::core::ffi::c_int
    {
        return Some(cursor);
    }
    cursor.cx = (px as u_int).wrapping_sub(ox);
    cursor.cy = (py as u_int).wrapping_sub(oy);
    window_visible_ranges(
        Some(pane),
        cursor.cx as ::core::ffi::c_int,
        cursor.cy as ::core::ffi::c_int,
        1 as u_int,
        &mut r,
    );
    if window_position_is_visible(&r, cursor.cx) {
        if status_at_line(owner) == 0 as ::core::ffi::c_int {
            cursor.cy = cursor.cy.wrapping_add(status_line_size(owner));
        }
        cursor.mode |= MODE_CURSOR;
    }
    Some(cursor)
}

fn server_client_reset_state(client_owner: &ClientRef) {
    let Some(session_owner) = client_owner.borrow().session.upgrade() else {
        return;
    };
    let current_link = session_owner.current_winlink();
    let Ok(link) = current_link.try_borrow() else {
        return;
    };
    let window_owner = link
        .window_owner
        .as_ref()
        .expect("current link window")
        .clone();
    drop(link);
    (|| {
        let mut r = Vec::new();
        let active_owner = window_owner.active_pane().upgrade();
        let mut s: Option<ScreenMode> = None;

        let mut mode: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
        let mut cursor: ::core::ffi::c_int = 0;
        let mut pane_mode: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
        let mut cx: u_int = 0 as u_int;
        let mut cy: u_int = 0 as u_int;
        let mut prompt: u_int = 0 as u_int;
        let mut sb_w: u_int = 0;
        if client_owner.borrow().flags & (CLIENT_CONTROL | CLIENT_SUSPENDED) as uint64_t != 0 {
            return;
        }
        let blocked = client_owner.borrow_mut().tty.suspend_block();
        let menu_owner = window_owner.menu_observer();
        if client_owner
            .borrow()
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.draw.is_some())
        {
            if let Some((overlay_screen, overlay_cx, overlay_cy)) =
                server_client_overlay_mode(client_owner)
            {
                s = Some(overlay_screen);
                cx = overlay_cx;
                cy = overlay_cy;
            }
        } else if let Some(menu) = menu_owner.as_ref() {
            let menu_borrow = menu.try_borrow_mut().expect("live unborrowed menu");
            let md = &*menu_borrow;
            (cx, cy) = menu_get_cursor(md);
            s = Some(ScreenMode::from(menu_screen(md)));
        } else if let Some(active) = active_owner
            .as_ref()
            .filter(|_| client_owner.borrow().prompt.is_none())
        {
            s = Some(active.screen_mode(true).expect("displayed pane screen"));
        } else {
            s = Some(ScreenMode::from(
                &*client_owner.borrow_mut().status.active_screen(),
            ));
        }
        if let Some(s) = s {
            mode = s.mode;
        }
        if log_get_level() != 0 as ::core::ffi::c_int {
            log_debug(format_args!(
                "{}: client {} mode {}",
                "server_client_reset_state",
                log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)")),
                screen_mode_display(mode)
            ));
        }
        {
            let terminal = &(client_owner);
            tty_region_off(terminal);
            tty_margin_off(terminal);
        };
        if client_owner.borrow().prompt.is_some() {
            prompt = 1 as u_int;
            (cx, cy) = status_prompt_cursor(client_owner);
        } else if active_owner.is_some()
            && !client_owner
                .borrow()
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.draw.is_some())
        {
            if window_owner.menu_observer().is_some() {
                let tty_window_view { ox, oy, sx, sy, .. } =
                    tty_window_offset(&client_owner.borrow().tty);
                if cx < ox || cx >= ox.wrapping_add(sx) || cy < oy || cy >= oy.wrapping_add(sy) {
                    mode &= !MODE_CURSOR;
                } else {
                    cx = cx.wrapping_sub(ox);
                    cy = cy.wrapping_sub(oy);
                    if status_at_line(client_owner) == 0 as ::core::ffi::c_int {
                        cy = cy.wrapping_add(status_line_size(client_owner));
                    }
                }
                prompt = 1 as u_int;
            } else {
                if let Some(cursor) = server_client_prompt_cursor(
                    client_owner,
                    active_owner.as_ref().expect("active pane"),
                    PromptCursor { mode, cx, cy },
                ) {
                    prompt = 1;
                    mode = cursor.mode;
                    cx = cursor.cx;
                    cy = cursor.cy;
                }
            }
            if prompt == 0 {
                let s = s.expect("active pane screen mode");
                let pane = active_owner.as_ref().expect("active pane");
                let (pane_width, _, pane_x, pane_y) = pane.geometry();
                let scrollbar = &pane;
                cursor = 0 as ::core::ffi::c_int;
                pane_mode = s.mode;
                let tty_window_view { ox, oy, sx, sy, .. } =
                    tty_window_offset(&client_owner.borrow().tty);
                if pane_x + s.cx as ::core::ffi::c_int >= ox as ::core::ffi::c_int
                    && pane_x + s.cx as ::core::ffi::c_int
                        <= ox as ::core::ffi::c_int + sx as ::core::ffi::c_int
                    && pane_y + s.cy as ::core::ffi::c_int >= oy as ::core::ffi::c_int
                    && pane_y + s.cy as ::core::ffi::c_int
                        <= oy as ::core::ffi::c_int + sy as ::core::ffi::c_int
                {
                    cursor = 1 as ::core::ffi::c_int;
                    cx = (pane_x + s.cx as ::core::ffi::c_int - ox as ::core::ffi::c_int) as u_int;
                    cy = (pane_y + s.cy as ::core::ffi::c_int - oy as ::core::ffi::c_int) as u_int;
                    window_visible_ranges(
                        active_owner.as_ref(),
                        cx as ::core::ffi::c_int,
                        cy as ::core::ffi::c_int,
                        1 as u_int,
                        &mut r,
                    );
                    if !window_position_is_visible(&r, cx) {
                        cursor = 0 as ::core::ffi::c_int;
                    }
                    if scrollbar.scrollbar_overlay() && scrollbar.scrollbar_visible() {
                        sb_w = scrollbar.scrollbar_width() as u_int;
                        if sb_w > pane_width {
                            sb_w = pane_width;
                        }
                        if sb_w != 0 as u_int
                            && (window_owner).scrollbar_position() == PANE_SCROLLBARS_LEFT
                        {
                            if s.cx < sb_w {
                                cursor = 0 as ::core::ffi::c_int;
                            }
                        } else if sb_w != 0 as u_int && s.cx >= pane_width.wrapping_sub(sb_w) {
                            cursor = 0 as ::core::ffi::c_int;
                        }
                    }
                    if status_at_line(client_owner) == 0 as ::core::ffi::c_int {
                        cy = cy.wrapping_add(status_line_size(client_owner));
                    }
                }
                if cursor == 0 {
                    mode &= !MODE_CURSOR;
                }
            }
        } else if !client_owner
            .borrow()
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.mode.is_some())
            || s.is_none()
        {
            mode &= !MODE_CURSOR;
        }
        if !pane_mode & MODE_SYNC != 0 {
            log_debug(format_args!(
                "{}: cursor to {},{}",
                "server_client_reset_state",
                (cx) as u32,
                (cy) as u32
            ));
            {
                let terminal = &(client_owner);
                tty_cursor(terminal, cx, cy)
            };
        } else {
            mode &= !CURSOR_MODES;
            mode |= client_owner.borrow().tty.mode() & CURSOR_MODES;
            s = None;
        }
        if options_get_number(&session_owner.borrow_options_mut(), c"mouse") != 0 {
            if !client_owner
                .borrow()
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.draw.is_some())
                && window_owner.menu_observer().is_none()
            {
                mode &= !ALL_MOUSE_MODES;
                for pane_owner in window_owner
                    .pane_snapshot()
                    .iter()
                    .filter_map(std::rc::Weak::upgrade)
                {
                    if pane_owner
                        .screen_mode(true)
                        .expect("displayed pane screen")
                        .mode
                        & MODE_MOUSE_ALL
                        != 0
                    {
                        mode |= MODE_MOUSE_ALL;
                    }
                }
            }
            if options_get_number(&session_owner.borrow_options_mut(), c"focus-follows-mouse") != 0
                || (window_owner).scrollbar_mode() == PANE_SCROLLBARS_MODAL
                || (window_owner).scrollbar_mode() == PANE_SCROLLBARS_AUTOHIDE
            {
                mode |= MODE_MOUSE_ALL;
            } else if !mode & MODE_MOUSE_ALL != 0 {
                mode |= MODE_MOUSE_BUTTON;
            }
        }
        if !client_owner
            .borrow()
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.draw.is_some())
            && prompt != 0
        {
            mode &= !MODE_BRACKETPASTE;
        }
        {
            let terminal = &(client_owner);
            tty_update_mode(terminal, mode, s);
            tty_reset(terminal);
            tty_sync_end(terminal);
        };
        client_owner.borrow_mut().tty.resume_block(blocked);
    })();
    drop(window_owner);
}
fn server_client_repeat_timer(owner: &ClientRef) {
    if owner.flags() & CLIENT_REPEAT as uint64_t != 0 {
        server_client_set_key_table(owner, None);
        owner.update_flags(CLIENT_REDRAWSTATUS as u64, CLIENT_REPEAT as uint64_t);
    }
}
fn server_client_click_timer(owner: &ClientRef) {
    log_debug(format_args!("click timer expired"));
    let (flags, click_event) = {
        let c = owner.borrow();
        (c.flags, c.click_event)
    };
    if flags & CLIENT_TRIPLECLICK as uint64_t != 0 {
        let event = key_event::new(
            KEYC_DOUBLECLICK as ::core::ffi::c_ulong as key_code,
            click_event,
            None,
        );
        server_client_handle_key(owner, event);
    }
    owner.borrow_mut().flags &= !(CLIENT_DOUBLECLICK | CLIENT_TRIPLECLICK) as uint64_t;
}
fn server_client_start_exit_timer(owner: &ClientRef) {
    let timeout = Duration::from_secs(10);
    if owner.borrow().exit_timer.is_none() {
        let timer = server_client_timer(owner, timeout, server_client_exit_timer);
        owner.borrow_mut().exit_timer = Some(timer);
    }
}
fn server_client_exit_timer(owner: &ClientRef) {
    let timer = owner.borrow_mut().exit_timer.take();
    drop(timer);
    let flags = owner.flags();
    if flags & (CLIENT_DEAD | CLIENT_SUSPENDED) as uint64_t != 0 {
        return;
    }
    if flags & CLIENT_EXITED as uint64_t != 0 {
        log_debug(format_args!(
            "{}: {} took too long to exit",
            "server_client_exit_timer",
            log_cstr(owner.borrow().name.as_deref().unwrap_or(c"(null)"))
        ));
        server_client_lost(owner);
    } else if flags & CLIENT_EXIT as uint64_t != 0 {
        log_debug(format_args!(
            "{}: {} took too long to flush",
            "server_client_exit_timer",
            log_cstr(owner.borrow().name.as_deref().unwrap_or(c"(null)"))
        ));
        server_client_check_exit(owner, 1 as ::core::ffi::c_int);
    }
}
fn server_client_check_exit(client_owner: &ClientRef, force: ::core::ffi::c_int) {
    let flags = client_owner.borrow().flags;
    if flags & (CLIENT_DEAD | CLIENT_EXITED) as uint64_t != 0 {
        return;
    }
    if !flags & CLIENT_EXIT as uint64_t != 0 {
        return;
    }
    if flags & CLIENT_CONTROL as uint64_t != 0 {
        if force != 0 {
            control_discard_all(client_owner);
        } else {
            control_discard(client_owner);
            if control_all_done(client_owner) == 0 {
                server_client_start_exit_timer(client_owner);
                return;
            }
        }
    }
    if force == 0 && crate::file::client_files_has_pending_data(&client_owner.borrow().files) {
        server_client_start_exit_timer(client_owner);
        return;
    }
    let timer = {
        let mut c = client_owner.borrow_mut();
        c.flags |= CLIENT_EXITED as uint64_t;
        c.exit_timer.take()
    };
    drop(timer);
    server_client_start_exit_timer(client_owner);
    let c = client_owner.borrow();
    match c.exit_type as ::core::ffi::c_uint {
        0 => {
            let mut data = Vec::from(c.retval.to_ne_bytes());
            if !c.exit_message.is_none() {
                data.extend_from_slice(
                    (c.exit_message)
                        .as_deref()
                        .expect("string is present")
                        .to_bytes_with_nul(),
                );
            }
            proc_send(&c.peer, MSG_EXIT, None, &data);
        }
        1 => {
            proc_send(&c.peer, MSG_SHUTDOWN, None, &[]);
        }
        2 => {
            proc_send(
                &c.peer,
                c.exit_msgtype,
                None,
                c.exit_session
                    .as_deref()
                    .expect("detach sets the exit session")
                    .to_bytes_with_nul(),
            );
        }
        _ => {}
    };
}
fn server_client_redraw_timer() {
    log_debug(format_args!("redraw timer fired"));
}
fn server_client_check_modes(client_owner: &ClientRef) {
    let Some(session_owner) = client_owner.borrow().session.upgrade() else {
        return;
    };
    let current_link = session_owner.current_winlink();
    let Ok(link) = current_link.try_borrow() else {
        return;
    };
    let window_owner = link
        .window_owner
        .as_ref()
        .expect("current link window")
        .clone();
    drop(link);
    (|| {
        if client_owner.borrow().flags & (CLIENT_CONTROL | CLIENT_SUSPENDED) as uint64_t != 0 {
            return;
        }
        if !client_owner.borrow().flags & CLIENT_REDRAWSTATUS as uint64_t != 0 {
            return;
        }
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            let wme = pane_owner.mode_entry();
            let update = if wme.is_alive() {
                wme.try_borrow()
                    .expect("mode update not borrowed")
                    .mode
                    .update
            } else {
                None
            };
            if let Some(update) = update {
                update(wme);
            }
        }
    })();
    drop(window_owner);
}
fn server_client_any_pane_redraw(c: &client, window: &WindowRef) -> bool {
    if c.flags & CLIENT_REDRAWWINDOW as uint64_t != 0 {
        return true;
    }
    window
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .any(|pane| pane.needs_redraw(false) || pane.needs_redraw(true))
}
fn server_client_check_redraw(client_owner: &ClientRef) {
    let Some(session_owner) = client_owner.borrow().session.upgrade() else {
        return;
    };
    let current_link = session_owner.current_winlink();
    let Ok(link) = current_link.try_borrow() else {
        return;
    };
    let window_owner = link
        .window_owner
        .as_ref()
        .expect("current link window")
        .clone();
    drop(link);
    (|| {
        let s = Some(session_owner.clone());
        let mut needed: ::core::ffi::c_int = 0;
        let mut mode: ::core::ffi::c_int = client_owner.borrow().tty.mode();
        let timeout = Duration::from_micros(1000);
        thread_local! {
            static REDRAW_TIMER: std::cell::RefCell<Option<Timer>> = const {
                std::cell::RefCell::new(None)
            };
        }
        let mut n: size_t = 0;
        if client_owner.borrow().flags & (CLIENT_CONTROL | CLIENT_SUSPENDED) as uint64_t != 0 {
            return;
        }
        if client_owner.borrow().flags & CLIENT_ALLREDRAWFLAGS as uint64_t != 0 {
            log_debug(format_args!(
                "{}: redraw{}{}{}{}{}",
                log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)")),
                log_cstr(
                    if client_owner.borrow().flags & CLIENT_REDRAWWINDOW as uint64_t != 0 {
                        c" window"
                    } else {
                        c""
                    }
                ),
                log_cstr(
                    if client_owner.borrow().flags & CLIENT_REDRAWSTATUS as uint64_t != 0 {
                        c" status"
                    } else {
                        c""
                    }
                ),
                log_cstr(
                    if client_owner.borrow().flags & CLIENT_REDRAWBORDERS as uint64_t != 0 {
                        c" borders"
                    } else {
                        c""
                    }
                ),
                log_cstr(
                    if client_owner.borrow().flags & CLIENT_REDRAWOVERLAY as uint64_t != 0 {
                        c" overlay"
                    } else {
                        c""
                    }
                ),
                log_cstr(
                    if client_owner.borrow().flags & CLIENT_REDRAWMENU as uint64_t != 0 {
                        c" menu"
                    } else {
                        c""
                    }
                )
            ));
        }
        needed = 0 as ::core::ffi::c_int;
        if client_owner.borrow().flags as ::core::ffi::c_ulonglong
            & (CLIENT_ALLREDRAWFLAGS as ::core::ffi::c_ulonglong | CLIENT_REDRAWSCROLLBARS)
            != 0
            || (server_client_any_pane_redraw(&client_owner.borrow(), &window_owner))
        {
            needed = 1 as ::core::ffi::c_int;
        }
        if needed == 0 {
            client_owner.borrow_mut().flags &= !CLIENT_STATUSFORCE as uint64_t;
            return;
        }
        n = client_owner.borrow().tty.pending_output();
        if n != 0 as size_t || client_owner.borrow().tty.is_blocked() {
            if n != 0 as size_t {
                log_debug(format_args!(
                    "{}: redraw deferred ({} left)",
                    log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)")),
                    (n) as usize
                ));
            } else {
                log_debug(format_args!(
                    "{}: redraw deferred (blocked)",
                    log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)"))
                ));
            }
            if REDRAW_TIMER.with_borrow(Option::is_none) {
                log_debug(format_args!("redraw timer started"));
                REDRAW_TIMER.set(Some(
                    Timer::new(timeout, || {
                        drop(REDRAW_TIMER.take());
                        server_client_redraw_timer();
                    })
                    .expect("arm timer"),
                ));
            }
            for pane_owner in window_owner
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
            {
                if pane_owner.needs_redraw(false) {
                    client_owner.borrow_mut().flags |= CLIENT_REDRAWWINDOW as uint64_t;
                    break;
                }
                if pane_owner.needs_redraw(true) {
                    let mut c = client_owner.borrow_mut();
                    c.flags =
                        (c.flags as ::core::ffi::c_ulonglong | CLIENT_REDRAWSCROLLBARS) as uint64_t;
                }
            }
            return;
        }
        log_debug(format_args!(
            "{}: redraw needed",
            log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)"))
        ));
        let saved = client_owner.borrow_mut().tty.begin_redraw();
        if !client_owner.borrow().flags & CLIENT_REDRAWWINDOW as uint64_t != 0 {
            for pane_owner in window_owner
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
            {
                if pane_owner.needs_redraw(false) {
                    log_debug(format_args!(
                        "{}: redraw pane %{}",
                        "server_client_check_redraw",
                        { pane_owner.id() }
                    ));
                    redraw_pane(client_owner, &pane_owner);
                } else if pane_owner.needs_redraw(true)
                    || client_owner.borrow().flags as ::core::ffi::c_ulonglong
                        & CLIENT_REDRAWSCROLLBARS
                        != 0
                {
                    log_debug(format_args!(
                        "{}: redraw scrollbar %{}",
                        "server_client_check_redraw",
                        { pane_owner.id() }
                    ));
                    redraw_pane_scrollbar(client_owner, &pane_owner);
                }
            }
        }
        if client_owner.borrow().flags & CLIENT_ALLREDRAWFLAGS as uint64_t != 0 {
            if options_get_number(
                &s.as_ref().expect("live session").borrow_options_mut(),
                c"set-titles",
            ) != 0
            {
                server_client_set_title(client_owner);
                server_client_set_path(client_owner);
            }
            server_client_set_progress_bar(client_owner);
            redraw_screen(client_owner);
        }
        client_owner.borrow_mut().tty.restore_redraw_cursor(&saved);
        {
            let terminal = &(client_owner);
            tty_update_mode(terminal, mode, None)
        };
        client_owner.borrow_mut().tty.end_redraw(saved);
        {
            let mut c = client_owner.borrow_mut();
            c.flags = (c.flags as ::core::ffi::c_ulonglong
                & !(CLIENT_ALLREDRAWFLAGS as ::core::ffi::c_ulonglong
                    | CLIENT_REDRAWSCROLLBARS
                    | CLIENT_STATUSFORCE as ::core::ffi::c_ulonglong))
                as uint64_t;
            c.redraw = c.tty.pending_output();
        }
        log_debug(format_args!(
            "{}: redraw added {} bytes",
            log_cstr(client_owner.borrow().name.as_deref().unwrap_or(c"(null)")),
            { client_owner.borrow().redraw }
        ));
    })();
    drop(window_owner);
}
fn server_client_set_title(client_owner: &ClientRef) {
    let mut template_session_value: Option<std::ffi::CString> = None;

    let Some(session_owner) = client_owner.borrow().session.upgrade() else {
        return;
    };
    let s = Some(session_owner.clone());
    template_session_value = Some(options_get_string(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"set-titles-string",
    ));
    let template = template_session_value.as_deref().expect("option snapshot");
    let mut ft_owner = crate::format::format_create(
        Some(client_owner),
        None,
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(
        ft,
        Some(client_owner),
        None,
        (refbox::Weak::new()).clone(),
        None,
    );
    let title = format_expand_time_cstring(ft, template);
    if client_owner
        .borrow()
        .title
        .as_ref()
        .is_none_or(|old| old != &title)
    {
        server_client_replace_title(&mut client_owner.borrow_mut(), Some(title.clone()));
        {
            let terminal = &(client_owner);
            tty_set_title(terminal, &title)
        };
    }
    drop(ft_owner);
}
/// Acquire the current active pane for an immediate client operation.
/// Raw relationship fields are accessed only while resolving the handle.
fn server_client_active_pane(c: &client) -> Option<std::rc::Rc<std::cell::RefCell<window_pane>>> {
    let session_owner = c.session.upgrade()?;
    let current = session_owner.current_winlink();
    let link = current.try_borrow().ok()?;
    link.window_owner.as_ref()?.active_pane().upgrade()
}

fn server_client_set_path(owner: &ClientRef) {
    let Some(active_owner) = server_client_active_pane(&owner.borrow()) else {
        return;
    };
    let path = active_owner.screen_path();
    if owner.borrow().path.as_deref() != Some(path.as_c_str()) {
        server_client_replace_path(&mut owner.borrow_mut(), Some(path.clone()));
        {
            let terminal = &(owner);
            tty_set_path(terminal, &path)
        };
    }
}
fn server_client_set_progress_bar(owner: &ClientRef) {
    let Some(active_owner) = server_client_active_pane(&owner.borrow()) else {
        return;
    };
    let pane_progress = active_owner.screen_progress();
    {
        let mut state = owner.borrow_mut();
        if pane_progress.state == state.progress_bar.state
            && pane_progress.progress == state.progress_bar.progress
        {
            return;
        }
        state.progress_bar = pane_progress;
    }
    {
        let terminal = &(owner);
        tty_set_progress_bar(terminal, &pane_progress)
    };
}

unsafe fn server_client_dispatch(
    owner: &ClientRef,
    message: crate::shared::process::PeerMessage<'_>,
) {
    let mut current_block: u64;
    let mut datalen: ssize_t = 0;
    let mut s: Option<SessionRef> = None;
    let mut old_sx: u_int = 0;
    let mut old_sy: u_int = 0;
    if owner.borrow().flags & CLIENT_DEAD as uint64_t != 0 {
        return;
    }
    let imsg = match message {
        crate::shared::process::PeerMessage::Disconnected => {
            server_client_lost(owner);
            return;
        }
        crate::shared::process::PeerMessage::Message(imsg) => imsg,
    };
    datalen = imsg.data.len() as ssize_t;
    match imsg.hdr.type_0 {
        MSG_IDENTIFY_CLIENTPID
        | MSG_IDENTIFY_CWD
        | MSG_IDENTIFY_ENVIRON
        | MSG_IDENTIFY_FEATURES
        | MSG_IDENTIFY_FLAGS
        | MSG_IDENTIFY_LONGFLAGS
        | MSG_IDENTIFY_STDIN
        | MSG_IDENTIFY_STDOUT
        | MSG_IDENTIFY_TERM
        | MSG_IDENTIFY_TERMINFO
        | MSG_IDENTIFY_TTYNAME
        | MSG_IDENTIFY_DONE => {
            if server_client_dispatch_identify(owner, imsg) != 0 as ::core::ffi::c_int {
                current_block = 13639960948656484833;
            } else {
                current_block = 14945149239039849694;
            }
        }
        MSG_COMMAND => {
            if server_client_dispatch_command(owner, imsg) != 0 as ::core::ffi::c_int {
                current_block = 13639960948656484833;
            } else {
                current_block = 14945149239039849694;
            }
        }
        MSG_RESIZE => {
            if datalen != 0 as ssize_t {
                current_block = 13639960948656484833;
            } else if owner.borrow().flags & CLIENT_CONTROL as uint64_t != 0 {
                current_block = 14945149239039849694;
            } else {
                server_client_update_latest(owner);
                (old_sx, old_sy) = owner.borrow().tty.size();
                tty_resize(owner);
                {
                    let terminal = &(owner);
                    tty_repeat_requests(terminal, 0 as ::core::ffi::c_int)
                };
                recalculate_sizes();
                if !owner
                    .borrow()
                    .overlay
                    .as_ref()
                    .is_some_and(|overlay| overlay.resize.is_some())
                {
                    server_client_clear_overlay(owner);
                } else {
                    server_client_overlay_resize(owner);
                }
                owner.borrow_mut().flags |= CLIENT_ALLREDRAWFLAGS as u64;
                if !owner.borrow().session_handle().is_none() {
                    server_client_fire_resized(owner, old_sx, old_sy);
                }
                current_block = 14945149239039849694;
            }
        }
        MSG_EXITING => {
            if datalen != 0 as ssize_t {
                current_block = 13639960948656484833;
            } else {
                server_client_set_session(owner, None);
                recalculate_sizes();
                tty_close(owner);
                let peer = owner.borrow().peer.clone();
                proc_send(&peer, MSG_EXITED, None, &[]);
                current_block = 14945149239039849694;
            }
        }
        MSG_WAKEUP | MSG_UNLOCK => {
            if datalen != 0 as ssize_t {
                current_block = 13639960948656484833;
            } else if owner.borrow().flags & CLIENT_SUSPENDED as uint64_t == 0 {
                current_block = 14945149239039849694;
            } else {
                owner.borrow_mut().flags &= !CLIENT_SUSPENDED as uint64_t;
                if owner.borrow().fd.is_none() || owner.borrow().session_handle().is_none() {
                    current_block = 14945149239039849694;
                } else {
                    let activity_time = SystemTime::now();
                    s = {
                        let mut c = owner.borrow_mut();
                        c.activity_time = activity_time;
                        c.session_handle()
                    };
                    tty_start_tty(owner);
                    owner.borrow_mut().flags |= CLIENT_ALLREDRAWFLAGS as u64;
                    recalculate_sizes();
                    if let Some(s_value) = s.as_ref() {
                        s_value.update_activity(Some(activity_time));
                    }
                    current_block = 14945149239039849694;
                }
            }
        }
        MSG_SHELL => {
            if datalen != 0 as ssize_t
                || (server_client_dispatch_shell(&owner.borrow()) != 0 as ::core::ffi::c_int)
            {
                current_block = 13639960948656484833;
            } else {
                current_block = 14945149239039849694;
            }
        }
        MSG_WRITE_READY => {
            if file_write_ready(owner, imsg) != 0 as ::core::ffi::c_int {
                current_block = 13639960948656484833;
            } else {
                current_block = 14945149239039849694;
            }
        }
        MSG_WRITE_DONE => {
            if file_write_done(owner, imsg) != 0 as ::core::ffi::c_int {
                current_block = 13639960948656484833;
            } else {
                current_block = 14945149239039849694;
            }
        }
        MSG_READ => {
            if file_read_data(owner, imsg) != 0 as ::core::ffi::c_int {
                current_block = 13639960948656484833;
            } else {
                current_block = 14945149239039849694;
            }
        }
        MSG_READ_DONE if file_read_done(owner, imsg) != 0 as ::core::ffi::c_int => {
            current_block = 13639960948656484833;
        }
        _ => {
            current_block = 14945149239039849694;
        }
    }
    match current_block {
        14945149239039849694 => (),
        _ => {
            log_debug(format_args!(
                "client {} invalid message type {}",
                log_pointer(Rc::as_ptr(owner)),
                (imsg.hdr.type_0) as i32
            ));
            proc_kill_peer(&owner.borrow().peer);
        }
    }
}
fn server_client_read_only(item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>) -> CmdOutcome {
    cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
    CmdOutcome::Error
}
fn server_client_default_command(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let c_owner = cmdq_get_client(Some(&*item)).expect("live client");
    let new_item_allocation;
    let cmdlist = OptionsScope::GlobalServer.with_local(|oo| options_get_command(oo));
    if c_owner.borrow().flags & CLIENT_READONLY as uint64_t != 0
        && cmd_list_all_have(&cmdlist.borrow()) == 0
    {
        new_item_allocation = cmdq_get_callback_owned(
            c"server_client_read_only",
            Some(Box::new(server_client_read_only)),
        );
    } else {
        new_item_allocation = cmdq_get_command(&cmdlist, None);
    }
    cmdq_insert_after(item_handle, new_item_allocation);
    CmdOutcome::Normal
}
fn server_client_command_done(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let c_owner = cmdq_get_client(Some(&*item)).expect("live client");
    let flags = c_owner.borrow().flags;
    if !flags & CLIENT_ATTACHED as uint64_t != 0 {
        c_owner.borrow_mut().flags |= CLIENT_EXIT as uint64_t;
    } else if !flags & CLIENT_EXIT as uint64_t != 0 {
        if flags & CLIENT_CONTROL as uint64_t != 0 {
            control_ready(&c_owner);
        }
        {
            let terminal = &c_owner;
            tty_send_requests(terminal)
        };
    }
    CmdOutcome::Normal
}
fn server_client_dispatch_command(owner: &ClientRef, imsg: &mut imsg) -> ::core::ffi::c_int {
    let mut current_block: u64;
    let mut data: msg_command = msg_command { argc: 0 };
    let mut len: size_t = 0;
    let mut argv = Vec::new();
    let mut argc: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut cause: Option<CString> = None;
    let mut new_item_allocation = None;
    if owner.borrow().flags & CLIENT_EXIT as uint64_t != 0 {
        return 0 as ::core::ffi::c_int;
    }
    let command_size = ::core::mem::size_of::<msg_command>();
    if imsg.data.len() < command_size {
        return -(1 as ::core::ffi::c_int);
    }
    let argc_bytes: [u8; ::core::mem::size_of::<::core::ffi::c_int>()] =
        imsg.data[..command_size].try_into().unwrap();
    data.argc = ::core::ffi::c_int::from_ne_bytes(argc_bytes);
    let trailing = &mut imsg.data[command_size..];
    len = trailing.len();
    if len > 0 && trailing[len - 1] != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    let unpacked = if len == 0 {
        unpack_argv(&mut [], data.argc)
    } else {
        unpack_argv(trailing, data.argc)
    };
    if let Ok(decoded) = unpacked {
        argv = decoded;
        argc = ::core::ffi::c_int::try_from(argv.len()).expect("argv length exceeds c_int");
        if argc == 0 as ::core::ffi::c_int {
            new_item_allocation = Some(cmdq_get_callback_owned(
                c"server_client_default_command",
                Some(Box::new(server_client_default_command)),
            ));
            current_block = 13472856163611868459;
        } else {
            cmd_log_argv(&argv, c"cmd_unpack_argv");
            let mut pr = cmd_parse_from_argv(&argv);
            match pr.status as ::core::ffi::c_uint {
                0 => {
                    cause = pr.error;
                    current_block = 12680788052841528405;
                }
                _ => {
                    if owner.borrow().flags & CLIENT_READONLY as uint64_t != 0
                        && cmd_list_all_have(
                            &pr.cmdlist.as_ref().expect("parsed command list").borrow(),
                        ) == 0
                    {
                        new_item_allocation = Some(cmdq_get_callback_owned(
                            c"server_client_read_only",
                            Some(Box::new(server_client_read_only)),
                        ));
                    } else {
                        new_item_allocation = Some(cmdq_get_command(
                            pr.cmdlist.as_ref().expect("successful command parse"),
                            None,
                        ));
                    }
                    drop(pr.cmdlist.take());
                    current_block = 13472856163611868459;
                }
            }
        }
        match current_block {
            12680788052841528405 => {}
            _ => {
                cmdq_append(
                    Some(owner),
                    new_item_allocation.take().expect("parsed command item"),
                );
                cmdq_append(
                    Some(owner),
                    cmdq_get_callback_owned(
                        c"server_client_command_done",
                        Some(Box::new(server_client_command_done)),
                    ),
                );
                return 0 as ::core::ffi::c_int;
            }
        }
    } else {
        cause = Some(CString::new("command too long").unwrap());
    }
    cmdq_append(
        Some(owner),
        cmdq_get_error(cause.as_deref().expect("command error")),
    );
    owner.borrow_mut().flags |= CLIENT_EXIT as uint64_t;
    0 as ::core::ffi::c_int
}
unsafe fn server_client_dispatch_identify(
    owner: &ClientRef,
    imsg: &mut imsg,
) -> ::core::ffi::c_int {
    let mut datalen: size_t = 0;
    let mut flags: ::core::ffi::c_int = 0;
    let mut feat: ::core::ffi::c_int = 0;
    let mut longflags: uint64_t = 0;
    if owner.borrow().flags & CLIENT_IDENTIFIED as uint64_t != 0 {
        return -(1 as ::core::ffi::c_int);
    }
    datalen = imsg.data.len();
    match imsg.hdr.type_0 {
        MSG_IDENTIFY_FEATURES => {
            if datalen != ::core::mem::size_of::<::core::ffi::c_int>() as usize {
                return -(1 as ::core::ffi::c_int);
            }
            feat = ::core::ffi::c_int::from_ne_bytes(
                imsg.data[..].try_into().expect("checked length"),
            );
            owner.borrow_mut().term_features |= feat;
            log_debug(format_args!(
                "client {} IDENTIFY_FEATURES {}",
                log_pointer(Rc::as_ptr(owner)),
                log_cstr(&tty_get_features(feat))
            ));
        }
        MSG_IDENTIFY_FLAGS => {
            if datalen != ::core::mem::size_of::<::core::ffi::c_int>() as usize {
                return -(1 as ::core::ffi::c_int);
            }
            flags = ::core::ffi::c_int::from_ne_bytes(
                imsg.data[..].try_into().expect("checked length"),
            );
            owner.borrow_mut().flags |= flags as uint64_t;
            log_debug(format_args!(
                "client {} IDENTIFY_FLAGS {}",
                log_pointer(Rc::as_ptr(owner)),
                log_hex(((flags) as u32) as u64)
            ));
        }
        MSG_IDENTIFY_LONGFLAGS => {
            if datalen != ::core::mem::size_of::<uint64_t>() as usize {
                return -(1 as ::core::ffi::c_int);
            }
            longflags = uint64_t::from_ne_bytes(imsg.data[..].try_into().expect("checked length"));
            owner.borrow_mut().flags |= longflags;
            log_debug(format_args!(
                "client {} IDENTIFY_LONGFLAGS {}",
                log_pointer(Rc::as_ptr(owner)),
                log_hex(longflags as ::core::ffi::c_ulonglong)
            ));
        }
        MSG_IDENTIFY_TERM => {
            let Some(data) = imsg_string(imsg) else {
                return -(1 as ::core::ffi::c_int);
            };
            server_client_set_term_name(&mut owner.borrow_mut(), Some(data.to_owned()));
            log_debug(format_args!(
                "client {} IDENTIFY_TERM {}",
                log_pointer(Rc::as_ptr(owner)),
                log_cstr(data)
            ));
        }
        MSG_IDENTIFY_TERMINFO => {
            let Some(data) = imsg_string(imsg) else {
                return -(1 as ::core::ffi::c_int);
            };
            server_client_add_term_cap(&mut owner.borrow_mut(), data);
            log_debug(format_args!(
                "client {} IDENTIFY_TERMINFO {}",
                log_pointer(Rc::as_ptr(owner)),
                log_cstr(data)
            ));
        }
        MSG_IDENTIFY_TTYNAME => {
            let Some(data) = imsg_string(imsg) else {
                return -(1 as ::core::ffi::c_int);
            };
            server_client_set_ttyname(&mut owner.borrow_mut(), Some(data.to_owned()));
            log_debug(format_args!(
                "client {} IDENTIFY_TTYNAME {}",
                log_pointer(Rc::as_ptr(owner)),
                log_cstr(data)
            ));
        }
        MSG_IDENTIFY_CWD => {
            let Some(data) = imsg_string(imsg) else {
                return -(1 as ::core::ffi::c_int);
            };
            if access(data.as_ptr(), X_OK) == 0 as ::core::ffi::c_int {
                server_client_set_cwd(&mut owner.borrow_mut(), Some(data.to_owned()));
            } else {
                if let Some(home) = find_home_cstr() {
                    server_client_set_cwd(&mut owner.borrow_mut(), Some(home.to_owned()));
                } else {
                    server_client_set_cwd(
                        &mut owner.borrow_mut(),
                        Some(CString::new("/").unwrap()),
                    );
                }
            }
            log_debug(format_args!(
                "client {} IDENTIFY_CWD {}",
                log_pointer(Rc::as_ptr(owner)),
                log_cstr(data)
            ));
        }
        MSG_IDENTIFY_STDIN => {
            if datalen != 0 as size_t {
                return -(1 as ::core::ffi::c_int);
            }
            owner.borrow_mut().fd = imsg_get_fd(imsg);
            log_debug(format_args!(
                "client {} IDENTIFY_STDIN {}",
                log_pointer(Rc::as_ptr(owner)),
                owner.borrow().fd.as_ref().map_or(-1, AsRawFd::as_raw_fd)
            ));
        }
        MSG_IDENTIFY_STDOUT => {
            if datalen != 0 as size_t {
                return -(1 as ::core::ffi::c_int);
            }
            owner.borrow_mut().out_fd = imsg_get_fd(imsg);
            log_debug(format_args!(
                "client {} IDENTIFY_STDOUT {}",
                log_pointer(Rc::as_ptr(owner)),
                owner
                    .borrow()
                    .out_fd
                    .as_ref()
                    .map_or(-1, AsRawFd::as_raw_fd)
            ));
        }
        MSG_IDENTIFY_ENVIRON => {
            let Some(data) = imsg_string(imsg) else {
                return -(1 as ::core::ffi::c_int);
            };
            if data.to_bytes().contains(&b'=') {
                environ_put(
                    owner
                        .borrow_mut()
                        .environ
                        .as_deref_mut()
                        .expect("environment"),
                    data,
                    0 as ::core::ffi::c_int,
                );
            }
            log_debug(format_args!(
                "client {} IDENTIFY_ENVIRON {}",
                log_pointer(Rc::as_ptr(owner)),
                log_cstr(data)
            ));
        }
        MSG_IDENTIFY_CLIENTPID => {
            if datalen != ::core::mem::size_of::<pid_t>() as usize {
                return -(1 as ::core::ffi::c_int);
            }
            let pid_bytes: [u8; ::core::mem::size_of::<pid_t>()] =
                imsg.data[..].try_into().unwrap();
            owner.borrow_mut().pid = pid_t::from_ne_bytes(pid_bytes);
            log_debug(format_args!(
                "client {} IDENTIFY_CLIENTPID {}",
                log_pointer(Rc::as_ptr(owner)),
                owner.borrow().pid as ::core::ffi::c_long
            ));
        }
        _ => {}
    }
    if imsg.hdr.type_0 != MSG_IDENTIFY_DONE {
        return 0 as ::core::ffi::c_int;
    }
    owner.borrow_mut().flags |= CLIENT_IDENTIFIED as uint64_t;
    server_client_ensure_term_name(&mut owner.borrow_mut());
    if owner
        .borrow()
        .ttyname
        .as_ref()
        .is_some_and(|name| !name.is_empty())
    {
        let name = (owner.borrow().ttyname)
            .as_deref()
            .expect("string is present")
            .to_owned();
        server_client_set_name(&mut owner.borrow_mut(), Some(name));
    } else {
        let name = CString::new(format!(
            "client-{}",
            owner.borrow().pid as ::core::ffi::c_long
        ))
        .unwrap();
        server_client_set_name(&mut owner.borrow_mut(), Some(name));
    }
    log_debug(format_args!(
        "client {} name is {}",
        log_pointer(Rc::as_ptr(owner)),
        log_cstr(owner.borrow().name.as_deref().unwrap_or(c"(null)"))
    ));
    if owner.borrow().flags & CLIENT_CONTROL as uint64_t != 0 {
        control_start(owner);
    } else if owner.borrow().fd.is_some() {
        if tty_init(owner) != 0 as ::core::ffi::c_int {
            let fd = owner.borrow_mut().fd.take();
            drop(fd);
        } else {
            tty_resize(owner);
            owner.borrow_mut().flags |= CLIENT_TERMINAL as uint64_t;
        }
        let out_fd = owner.borrow_mut().out_fd.take();
        drop(out_fd);
    }
    if owner.borrow().flags & (CLIENT_CONTROL | CLIENT_TERMINAL) as uint64_t != 0 {
        events_fire_client(c"client-created", std::rc::Rc::clone(owner));
    }
    if owner.borrow().flags as ::core::ffi::c_ulonglong
        & (CLIENT_BRACKETPASTING | CLIENT_ASSUMEPASTING)
        != 0
        && current_time() - owner.borrow().paste_time > CLIENT_PASTE_TIME_LIMIT as time_t
    {
        log_debug(format_args!(
            "{}: paste time limit exceeded",
            log_cstr(owner.borrow().name.as_deref().unwrap_or(c"(null)"))
        ));
        let mut c = owner.borrow_mut();
        c.flags = (c.flags as ::core::ffi::c_ulonglong
            & !(CLIENT_BRACKETPASTING | CLIENT_ASSUMEPASTING)) as uint64_t;
    }
    if !owner.borrow().flags & CLIENT_EXIT as uint64_t != 0
        && !cfg_finished()
        && ClientRegistry::first().is_some_and(|first| Rc::ptr_eq(&first, owner))
    {
        start_cfg();
    }
    0 as ::core::ffi::c_int
}
fn server_client_dispatch_shell(c: &client) -> ::core::ffi::c_int {
    let shell_value =
        OptionsScope::GlobalSession.with_local(|oo| options_get_string(oo, c"default-shell"));
    let mut shell: &CStr = &shell_value;
    if checkshell(Some(shell)) == 0 {
        shell = _PATH_BSHELL;
    }
    proc_send(&c.peer, MSG_SHELL, None, shell.to_bytes_with_nul());
    proc_kill_peer(&c.peer);
    0 as ::core::ffi::c_int
}
/// Copy the selected directory while any observed startup client is retained.
fn server_client_get_cwd(c: Option<&ClientRef>, s: Option<&SessionRef>) -> Option<CString> {
    use crate::session::Session;
    if let Some(client) = c {
        return client.cwd(s);
    }
    if !cfg_finished() {
        if let Some(owner) = cfg_client() {
            return owner.borrow().cwd.clone();
        }
    }
    if let Some(session) = s {
        if let Some(cwd) = session.cwd() {
            return Some(cwd);
        }
    }
    Some(find_home_cstr().unwrap_or(c"/").to_owned())
}
fn server_client_control_flags(c_owner: &ClientRef, next: &CStr) -> uint64_t {
    if next == c"pause-after" {
        c_owner.borrow_mut().pause_age = 0 as u_int;
        return 0x100000000 as uint64_t;
    }
    if let Some((pause_age, _)) = next
        .to_bytes()
        .strip_prefix(b"pause-after=")
        .and_then(scan_u32)
    {
        c_owner.borrow_mut().pause_age = pause_age.wrapping_mul(1000 as u_int);
        return 0x100000000 as uint64_t;
    }
    if next == c"no-output" {
        return 0x4000000 as uint64_t;
    }
    if next == c"wait-exit" {
        return 0x200000000 as uint64_t;
    }
    if next == c"new-layouts" {
        return 0x800000000 as uint64_t;
    }
    0 as uint64_t
}
fn server_client_set_flags(c_owner: &ClientRef, flags: &CStr) {
    let mut flag: uint64_t = 0;
    for token in flags.to_bytes().split(|&b| b == b',') {
        let (not, name) = match token.strip_prefix(b"!") {
            Some(name) => (true, name),
            None => (false, token),
        };
        let next = CString::new(name).expect("a flag from a C string contains no NUL");
        if c_owner.flags() & CLIENT_CONTROL as uint64_t != 0 {
            flag = server_client_control_flags(c_owner, &next);
        } else {
            flag = 0 as uint64_t;
        }
        if name == b"read-only" {
            flag = CLIENT_READONLY as uint64_t;
        } else if name == b"ignore-size" {
            flag = CLIENT_IGNORESIZE as uint64_t;
        } else if name == b"no-detach-on-destroy" {
            flag = CLIENT_NO_DETACH_ON_DESTROY as uint64_t;
        }
        if flag == 0 as uint64_t {
            continue;
        }
        log_debug(format_args!(
            "client {} set flag {}",
            log_cstr(c_owner.name().as_deref().unwrap_or(c"(null)")),
            log_cstr(&next)
        ));
        {
            let mut c = c_owner.borrow_mut();
            if not {
                if c.flags & CLIENT_READONLY as uint64_t != 0 {
                    flag &= !CLIENT_READONLY as uint64_t;
                }
                c.flags &= !flag;
            } else {
                c.flags |= flag;
            }
        }
        if flag == CLIENT_CONTROL_NOOUTPUT as uint64_t {
            control_reset_offsets(c_owner);
        }
    }
    c_owner.send_message(MSG_FLAGS, None, bytes_of(&c_owner.flags()));
}
fn server_client_get_flags(c: &client) -> CString {
    let mut s: Vec<u8> = Vec::new();
    if c.flags & CLIENT_ATTACHED as uint64_t != 0 {
        s.extend_from_slice(b"attached,");
    }
    if c.flags & CLIENT_FOCUSED as uint64_t != 0 {
        s.extend_from_slice(b"focused,");
    }
    if c.flags & CLIENT_CONTROL as uint64_t != 0 {
        s.extend_from_slice(b"control-mode,");
    }
    if c.flags & CLIENT_IGNORESIZE as uint64_t != 0 {
        s.extend_from_slice(b"ignore-size,");
    }
    if c.flags as ::core::ffi::c_ulonglong & CLIENT_NO_DETACH_ON_DESTROY != 0 {
        s.extend_from_slice(b"no-detach-on-destroy,");
    }
    if c.flags & CLIENT_CONTROL_NOOUTPUT as uint64_t != 0 {
        s.extend_from_slice(b"no-output,");
    }
    if c.flags as ::core::ffi::c_ulonglong & CLIENT_CONTROL_WAITEXIT != 0 {
        s.extend_from_slice(b"wait-exit,");
    }
    if c.flags as ::core::ffi::c_ulonglong & CLIENT_CONTROL_NEWLAYOUTS != 0 {
        s.extend_from_slice(b"new-layouts,");
    }
    if c.flags as ::core::ffi::c_ulonglong & CLIENT_CONTROL_PAUSEAFTER != 0 {
        s.extend_from_slice(
            format!("pause-after={},", c.pause_age.wrapping_div(1000 as u_int)).as_bytes(),
        );
    }
    if c.flags & CLIENT_READONLY as uint64_t != 0 {
        s.extend_from_slice(b"read-only,");
    }
    if c.flags & CLIENT_SUSPENDED as uint64_t != 0 {
        s.extend_from_slice(b"suspended,");
    }
    if c.flags & CLIENT_UTF8 as uint64_t != 0 {
        s.extend_from_slice(b"UTF-8,");
    }
    s.pop();
    CString::new(s).expect("client flag names contain no NUL")
}

fn server_client_remove_pane(wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>) {
    let pane_id = wp_owner.id();
    let mut registry_c_owner = ClientRegistry::first();
    while let Some(owner) = registry_c_owner {
        let update = {
            let mut c = owner.borrow_mut();
            if c.mouse_last_pane == pane_id as ::core::ffi::c_int {
                c.mouse_last_pane = -(1 as ::core::ffi::c_int);
                c.mouse_scrolling_flag = 0 as ::core::ffi::c_int;
                c.mouse_drag_update.take()
            } else {
                None
            }
        };
        drop(update);
        registry_c_owner = ClientRegistry::next(&owner);
    }
}
fn server_client_print(
    client_owner: Option<&ClientRef>,
    mut parse: ::core::ffi::c_int,
    mut evb: &mut SegmentedBuf,
) {
    let escaped;
    let msg: &CStr = if parse == 0 {
        let input = evbuffer_pullup(evb, -1).map_or(&[][..], |bytes| &*bytes);
        let mut bytes = utf8_stravisx_bytes(input, VIS_OCTAL | VIS_CSTYLE | VIS_NOSLASH);
        bytes.push(0);
        escaped = bytes;
        CStr::from_bytes_until_nul(&escaped).expect("escaped message is NUL-terminated")
    } else {
        if evbuffer_pullup(evb, -1).is_some_and(|bytes| bytes.last() != Some(&0)) {
            evbuffer_add(evb, b"\0");
        }
        evbuffer_pullup(evb, -1).map_or(c"", |bytes| {
            CStr::from_bytes_until_nul(bytes).expect("print buffer is NUL-terminated")
        })
    };
    log_debug(format_args!("{}: {}", "server_client_print", log_cstr(msg)));
    if let Some(client) = client_owner {
        let flags = client.flags();
        let view_session = client
            .borrow()
            .session_handle()
            .filter(|_| flags & CLIENT_CONTROL as uint64_t == 0);
        if let Some(session) = view_session {
            let pane_owner = winlink_window(&session.current_winlink())
                .as_ref()
                .expect("live window")
                .active_pane()
                .upgrade()
                .expect("view-mode pane");
            // SAFETY (set_mode, window_copy_add): `pane_owner` is the live active
            // pane of the client's session and no pane or client borrow is held
            // across these calls. Entering view mode first gives window_copy_add
            // the copy-mode data it requires.
            if !pane_owner.is_mode(&window_view_mode) {
                pane_owner.set_mode(None, &window_view_mode, None, None, None);
            }
            if parse != 0 {
                while let Some(line) = evbuffer_readln(evb) {
                    unsafe {
                        window_copy_add(&pane_owner, 1 as ::core::ffi::c_int, |out| {
                            write_cstr(
                                out,
                                CStr::from_bytes_until_nul(&line).expect("readln appends a NUL"),
                            )
                        })
                    };
                }
                let size = evbuffer_get_length(&*(evb));
                if size != 0 as size_t {
                    let rest = evbuffer_pullup(evb, -1).expect("nonempty buffer");
                    unsafe {
                        window_copy_add(&pane_owner, 1 as ::core::ffi::c_int, |out| {
                            out.write_all(rest)
                        })
                    };
                }
            } else {
                unsafe {
                    window_copy_add(&pane_owner, 0 as ::core::ffi::c_int, |out| {
                        write_cstr(out, msg)
                    })
                };
            }
        } else {
            if !flags & CLIENT_UTF8 as uint64_t != 0 {
                let sanitized = utf8_sanitize_cstring(msg);
                if flags & CLIENT_CONTROL as uint64_t != 0 {
                    // SAFETY: no client borrow is held, so the control state is
                    // not aliased while the line is queued.
                    control_write(client, |out| write_cstr(out, &*sanitized));
                } else {
                    file_print(client_owner, |out| {
                        write_cstr(out, &*sanitized)?;
                        out.write_all(b"\n")
                    });
                }
            } else if flags & CLIENT_CONTROL as uint64_t != 0 {
                // SAFETY: no client borrow is held, so the control state is not
                // aliased while the line is queued.
                control_write(client, |out| write_cstr(out, msg));
            } else {
                file_print(client_owner, |out| {
                    write_cstr(out, msg)?;
                    out.write_all(b"\n")
                });
            }
        }
    }
}
fn server_client_report_theme(c_owner: &ClientRef, mut theme: client_theme) {
    let mut old: client_theme = c_owner.borrow().theme;
    if theme as ::core::ffi::c_uint == THEME_LIGHT as ::core::ffi::c_int as ::core::ffi::c_uint {
        c_owner.borrow_mut().theme = THEME_LIGHT;
        events_fire_client(c"client-light-theme", std::rc::Rc::clone(c_owner));
    } else {
        c_owner.borrow_mut().theme = THEME_DARK;
        events_fire_client(c"client-dark-theme", std::rc::Rc::clone(c_owner));
    }
    if c_owner.borrow().theme as ::core::ffi::c_uint != old as ::core::ffi::c_uint {
        server_client_update_theme_colours(Some(c_owner));
        {
            let terminal = &(c_owner);
            if terminal.borrow_terminal().is_opened() {
                tty_invalidate(terminal);
            }
        };
        c_owner.borrow_mut().flags |= CLIENT_ALLREDRAWFLAGS as u64;
    }
    {
        let terminal = &(c_owner);
        tty_repeat_requests(terminal, 1 as ::core::ffi::c_int)
    };
}

#[cfg(test)]
mod key_event_owner_tests {
    use super::key_event;

    #[test]
    fn absent_and_empty_bytes_remain_distinct_in_snapshots() {
        let mouse = Default::default();
        let absent = key_event::new(1, mouse, None);
        let empty = key_event::new(1, mouse, Some(Vec::new()));
        assert!(absent.bytes_ptr_len().is_none());
        assert_eq!(empty.bytes_ptr_len(), Some([].as_slice()));
        assert!(empty.metadata_snapshot().bytes.is_none());
    }
}

impl Drop for client {
    fn drop(&mut self) {
        log_debug(format_args!("free client {}", log_pointer(self)));
        drop(self.queue.take());
        assert!(
            crate::file::client_files_is_empty(&self.files),
            "client file index still contains live records at client teardown"
        );
    }
}

/// Clients for queue tests elsewhere, built without the server.
#[cfg(test)]
pub(crate) mod queue_test_clients {
    use super::*;

    /// A client owning only a command queue.
    pub(crate) fn with_queue() -> ClientRef {
        let owner = client::new();
        owner.borrow_mut().queue = Some(cmdq_new());
        owner
    }

    /// Attach or detach without the server's side effects.
    pub(crate) fn attach(owner: &ClientRef, session: Option<&SessionRef>) {
        owner.borrow_mut().set_session(session);
    }

    pub(crate) fn return_value(owner: &ClientRef) -> i32 {
        owner.borrow().retval
    }
}

#[cfg(test)]
mod lost_queue_tests {
    use super::*;
    use crate::shared::command::CmdOutcome;
    use std::cell::Cell;

    struct SetOnDrop(Rc<Cell<bool>>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    /// Queue a suffix item that records whether it ran or was released.
    fn append_suffix(owner: &ClientRef, ran: &Rc<Cell<bool>>, released: &Rc<Cell<bool>>) {
        let ran = ran.clone();
        let capture = SetOnDrop(released.clone());
        cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"suffix",
                Some(Box::new(move |_| {
                    let _capture = &capture;
                    ran.set(true);
                    CmdOutcome::Normal
                })),
            ),
        );
    }

    #[test]
    fn dropping_a_lost_client_drops_its_suspended_future() {
        let owner = queue_test_clients::with_queue();
        let (tx, rx) = tiri_rt::oneshot::channel::<()>();
        let (body_ran, future_released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let (body, guard) = (body_ran.clone(), SetOnDrop(future_released.clone()));
        let head = cmdq_append(
            Some(&owner),
            cmdq_get_callback_owned(
                c"suspended head",
                Some(Box::new(move |_| {
                    CmdOutcome::Suspend(Box::pin(async move {
                        let _guard = guard;
                        let _ = rx.await;
                        body.set(true);
                    }))
                })),
            ),
        );
        let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        append_suffix(&owner, &ran, &released);
        assert_eq!(cmdq_next(Some(&owner)), 0);
        owner.borrow_mut().flags |= CLIENT_DEAD as uint64_t;
        assert_eq!(cmdq_next(Some(&owner)), 0);

        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
        assert!(future_released.get());
        assert!(tx.is_closed());
        assert!(tx.send(()).is_err());
        assert!(!body_ran.get());
        assert!(released.get());
        assert!(!ran.get());
    }

    /// Queue a head that suspends on `future` built from a strong client
    /// reference, then release the test's own reference.
    fn suspend_holding_client<F: std::future::Future<Output = ()> + 'static>(
        future: impl FnOnce(ClientRef) -> F + 'static,
    ) -> (
        std::rc::Weak<std::cell::RefCell<client>>,
        std::rc::Weak<std::cell::RefCell<cmdq_item>>,
    ) {
        let owner = queue_test_clients::with_queue();
        let held = owner.clone();
        let head = cmdq_append(
            Some(&owner),
            cmdq_get_callback_owned(
                c"client-holding head",
                Some(Box::new(move |_| {
                    CmdOutcome::Suspend(Box::pin(future(held)))
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(&owner)), 0);
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_some(), "the future holds the client");
        (client, head)
    }

    #[test]
    fn a_completed_future_may_release_its_own_client() {
        let (tx, mut rx) = tiri_rt::oneshot::channel::<()>();
        let ran = Rc::new(Cell::new(false));
        let body = ran.clone();
        let (client, head) = suspend_holding_client(move |held| {
            std::future::poll_fn(move |cx| {
                let _ = &held;
                std::future::Future::poll(std::pin::Pin::new(&mut rx), cx).map(|_| body.set(true))
            })
        });
        tx.send(()).unwrap();
        assert!(ran.get());
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
    }

    #[test]
    fn a_body_may_release_its_own_client_while_polled() {
        let (tx, rx) = tiri_rt::oneshot::channel::<()>();
        let ran = Rc::new(Cell::new(false));
        let body = ran.clone();
        let (client, head) = suspend_holding_client(move |held| async move {
            let _ = rx.await;
            let client = Rc::downgrade(&held);
            drop(held);
            assert!(client.upgrade().is_none(), "the queue is freed mid-poll");
            body.set(true);
        });
        tx.send(()).unwrap();
        assert!(ran.get());
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
    }

    #[test]
    fn dropping_a_lost_client_cancels_a_file_backed_wait() {
        use tiri_rt::Runtime as _;
        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        {
            let owner = queue_test_clients::with_queue();
            let cancelled = Rc::new(Cell::new(false));
            let mut capture = Some(SetOnDrop(cancelled.clone()));
            let file = Rc::new(Cell::new(std::rc::Weak::new()));
            let observed_file = file.clone();
            let head = cmdq_append(
                Some(&owner),
                cmdq_get_callback_owned(
                    c"file-backed head",
                    Some(Box::new(move |_| {
                        let wait = crate::file::file_read_wait(None, c"/dev/null", |file| {
                            observed_file.set(file);
                            let capture = capture.take();
                            Some(Box::new(move |_| {
                                let _capture = &capture;
                                panic!("a dropped wait must not deliver progress")
                            }))
                        });
                        CmdOutcome::Suspend(Box::pin(async move {
                            let _ = wait.await;
                            panic!("a dropped wait must not complete")
                        }))
                    })),
                ),
            );
            let (ran, released) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
            append_suffix(&owner, &ran, &released);
            assert_eq!(
                cmdq_next(Some(&owner)),
                0,
                "local completion must be deferred"
            );
            assert!(head.upgrade().is_some());

            owner.borrow_mut().flags |= CLIENT_DEAD as uint64_t;
            drop(owner);
            assert!(head.upgrade().is_none());
            assert!(cancelled.get());
            assert!(released.get());
            assert!(!ran.get());
            let file = file.take();
            crate::reactor::shutdown_runtime(runtime);
            assert!(file.upgrade().is_none());
        }
    }

    /// Queue a suspended head whose future records its release, then mark the
    /// client lost the way `server_client_lost` does before retiring it.
    fn lose_with_suspended_head(
        owner: &ClientRef,
    ) -> (std::rc::Weak<std::cell::RefCell<cmdq_item>>, Rc<Cell<bool>>) {
        let released = Rc::new(Cell::new(false));
        let guard = SetOnDrop(released.clone());
        let head = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"suspended head",
                Some(Box::new(move |_| {
                    CmdOutcome::Suspend(Box::pin(async move {
                        let _guard = guard;
                        std::future::pending::<()>().await;
                    }))
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(owner)), 0);
        owner.borrow_mut().flags |= CLIENT_DEAD as uint64_t;
        assert_eq!(cmdq_next(Some(owner)), 0);
        (head, released)
    }

    #[test]
    fn a_client_retired_inside_a_callback_is_freed_from_dispatch() {
        use tiri_rt::{Handle as _, Runtime as _};
        let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
        let owner = queue_test_clients::with_queue();
        let client = Rc::downgrade(&owner);
        let inside = client.clone();
        let returned = Rc::new(Cell::new(false));
        let mark = returned.clone();
        let _task = runtime
            .handle()
            .spawn(async move {
                server_client_retire(owner);
                assert!(inside.upgrade().is_some(), "freed inside the callback");
                mark.set(true);
            })
            .unwrap();
        runtime.poll(Some(std::time::Duration::ZERO)).unwrap();
        assert!(returned.get());
        assert!(client.upgrade().is_none());
    }

    #[test]
    fn retiring_a_lost_client_cancels_its_suspended_item_from_dispatch() {
        use tiri_rt::Runtime as _;
        let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
        {
            let owner = queue_test_clients::with_queue();
            let (head, released) = lose_with_suspended_head(&owner);
            let client = Rc::downgrade(&owner);
            server_client_retire(owner);
            assert!(client.upgrade().is_some());
            assert!(head.upgrade().is_some());
            assert!(!released.get(), "cancelled inline");
            runtime.poll(Some(std::time::Duration::ZERO)).unwrap();
            assert!(client.upgrade().is_none());
            assert!(head.upgrade().is_none());
            assert!(released.get());
        }
    }

    #[test]
    fn runtime_shutdown_releases_a_pending_retirement() {
        use tiri_rt::Runtime as _;
        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        {
            let owner = queue_test_clients::with_queue();
            let (head, released) = lose_with_suspended_head(&owner);
            let client = Rc::downgrade(&owner);
            server_client_retire(owner);
            crate::reactor::shutdown_runtime(runtime);
            assert!(client.upgrade().is_none());
            assert!(head.upgrade().is_none());
            assert!(released.get());
        }
    }
}
