use super::*;
use crate::options::options_get_number;
use crate::server_client::ClientRegistry;
use crate::session::Session;
use crate::shared::client::client_handle;
use crate::shared::client::ClientRef;
use crate::shared::rc::same;
use crate::shared::session::SessionRef;
use crate::shared::window::WindowRef;
use crate::winlink::winlink_window;
use std::ffi::{CStr, CString};
use std::rc::Rc;

fn format_cb_host(_ft: &mut format_tree) -> Option<CString> {
    Some(tiri_rt::unix::host_name().unwrap_or_default())
}
fn format_cb_host_short(_ft: &mut format_tree) -> Option<CString> {
    let host = tiri_rt::unix::host_name().unwrap_or_default();
    let host = host.as_bytes();
    let short = host.split(|&byte| byte == b'.').next().unwrap_or(host);
    Some(CString::new(short).expect("hostname contains no NUL"))
}
fn format_cb_pid(_ft: &mut format_tree) -> Option<CString> {
    let mut value = None;
    value = Some(
        CString::new(format!("{}", std::process::id())).expect("formatted numbers contain no NUL"),
    );
    value
}
fn format_cb_session_attached_list(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_attached_list", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_alert(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_alert", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_alerts(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_alerts", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_stack(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_stack", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_window_stack_index(ft: &mut format_tree) -> Option<CString> {
    let link = ft.winlink_handle();
    let owner = link.try_borrow().ok()?.session.upgrade()?;
    let mut context = format_tree::default();
    context.s = Rc::downgrade(&owner);
    context.wl = link;
    match owner.format_value(c"window_stack_index", &mut context)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("window_stack_index is a string builtin"),
    }
}
fn format_cb_window_linked_sessions_list(ft: &mut format_tree) -> Option<CString> {
    let link = ft.winlink_handle();
    if !link.is_alive() {
        return None;
    }
    let window = winlink_window(&link).expect("linked window");
    let mut names = Vec::new();
    let mut cursor = window.next_winlink(None);
    while cursor.is_alive() {
        let session = cursor.try_borrow().expect("live winlink").session.upgrade();
        if let Some(session) = session {
            if !names.is_empty() {
                names.push(b',');
            }
            names.extend_from_slice(session.name().as_bytes());
        }
        cursor = window.next_winlink(Some(cursor));
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

fn format_cb_window_active_sessions(ft: &mut format_tree) -> Option<CString> {
    let link = ft.winlink_handle();
    if !link.is_alive() {
        return None;
    }
    let window = winlink_window(&link).expect("linked window");
    let mut count = 0u32;
    let mut cursor = window.next_winlink(None);
    while cursor.is_alive() {
        let session = cursor.try_borrow().expect("live winlink").session.upgrade();
        if let Some(session) = session {
            if session.current_winlink() == cursor {
                count = count.wrapping_add(1);
            }
        }
        cursor = window.next_winlink(Some(cursor));
    }
    Some(CString::new(count.to_string()).expect("formatted number contains no NUL"))
}

fn format_cb_window_active_sessions_list(ft: &mut format_tree) -> Option<CString> {
    let link = ft.winlink_handle();
    if !link.is_alive() {
        return None;
    }
    let window = winlink_window(&link).expect("linked window");
    let mut names = Vec::new();
    let mut cursor = window.next_winlink(None);
    while cursor.is_alive() {
        let session = cursor.try_borrow().expect("live winlink").session.upgrade();
        if let Some(session) = session {
            if session.current_winlink() == cursor {
                if !names.is_empty() {
                    names.push(b',');
                }
                names.extend_from_slice(session.name().as_bytes());
            }
        }
        cursor = window.next_winlink(Some(cursor));
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

fn format_cb_window_active_clients(ft: &mut format_tree) -> Option<CString> {
    let link = ft.winlink_handle();
    if !link.is_alive() {
        return None;
    }
    let window = winlink_window(&link);
    let mut count = 0u32;
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if let Some(session) = client.attached_session().upgrade() {
            if same(
                window.as_ref(),
                winlink_window(&session.current_winlink()).as_ref(),
            ) {
                count = count.wrapping_add(1);
            }
        }
        cursor = ClientRegistry::next(&client);
    }
    Some(CString::new(count.to_string()).expect("formatted number contains no NUL"))
}

fn format_cb_window_active_clients_list(ft: &mut format_tree) -> Option<CString> {
    let link = ft.winlink_handle();
    if !link.is_alive() {
        return None;
    }
    let window = winlink_window(&link);
    let mut names = Vec::new();
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if let Some(session) = client.attached_session().upgrade() {
            if same(
                window.as_ref(),
                winlink_window(&session.current_winlink()).as_ref(),
            ) {
                if !names.is_empty() {
                    names.push(b',');
                }
                names.extend_from_slice(client.name().as_deref().expect("client name").to_bytes());
            }
        }
        cursor = ClientRegistry::next(&client);
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

fn format_cb_window_layout(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let legacy = client_handle(&ft.client)
        .as_ref()
        .is_some_and(|client| client.uses_legacy_layout_format());
    let value = window.layout_string(legacy, false);
    drop(window);
    value
}

fn format_cb_window_visible_layout(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let legacy = client_handle(&ft.client)
        .as_ref()
        .is_some_and(|client| client.uses_legacy_layout_format());
    let value = window.layout_string(legacy, true);
    drop(window);
    value
}

/// The name of the window's layout, as `select-layout` takes it.
fn format_cb_window_layout_name(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let name = window.layout().name().to_owned();
    drop(window);
    Some(name)
}

fn pane_format_value(key: &CStr, context: &mut format_tree) -> Option<FormatValue> {
    match context.wp.upgrade() {
        Some(pane) => pane.format_value(key, context),
        None => crate::window_pane::format_without_pane(key, context),
    }
}

fn pane_format_string(key: &CStr, ft: &mut format_tree) -> Option<CString> {
    match pane_format_value(key, ft)? {
        FormatValue::String(value) => Some(value),
        _ => unreachable!("string builtin value"),
    }
}

fn pane_format_time(key: &CStr, ft: &mut format_tree) -> Option<time_t> {
    match pane_format_value(key, ft)? {
        FormatValue::Time(value) => Some(value),
        _ => unreachable!("time builtin value"),
    }
}

fn format_cb_start_command(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_start_command", ft)
}
fn format_cb_start_command_list(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_start_command_list", ft)
}
fn format_cb_start_path(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_start_path", ft)
}
fn format_cb_current_command(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_current_command", ft)
}
fn format_cb_current_path(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_current_path", ft)
}
fn format_cb_history_bytes(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_bytes", ft)
}
fn format_cb_history_all_bytes(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_all_bytes", ft)
}
fn format_cb_pane_tabs(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_tabs", ft)
}
fn format_cb_pane_fg(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_fg", ft)
}
fn format_cb_pane_flags(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_flags", ft)
}

fn format_cb_pane_bg(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_bg", ft)
}
fn format_cb_session_group_list(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_group_list", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_group_attached_list(ft: &mut format_tree) -> Option<CString> {
    match ft
        .s
        .upgrade()?
        .format_value(c"session_group_attached_list", ft)?
    {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_pane_in_mode(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_in_mode", ft)
}
fn format_cb_pane_at_top(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_at_top", ft)
}
fn format_cb_pane_at_bottom(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_at_bottom", ft)
}

fn format_cb_cursor_character(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_character", ft)
}
fn format_cb_cursor_colour(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_colour", ft)
}
fn format_cb_mouse_word(ft: &mut format_tree) -> Option<CString> {
    if ft.m.valid == 0 {
        return None;
    }
    let pane = cmd_mouse_pane(&ft.m, None, None)?;
    match pane.format_value(c"mouse_word", ft)? {
        FormatValue::String(value) => Some(value),
        _ => unreachable!("mouse builtin string"),
    }
}
fn format_cb_mouse_hyperlink(ft: &mut format_tree) -> Option<CString> {
    if ft.m.valid == 0 {
        return None;
    }
    let pane = cmd_mouse_pane(&ft.m, None, None)?;
    match pane.format_value(c"mouse_hyperlink", ft)? {
        FormatValue::String(value) => Some(value),
        _ => unreachable!("mouse builtin string"),
    }
}
fn format_cb_mouse_line(ft: &mut format_tree) -> Option<CString> {
    if ft.m.valid == 0 {
        return None;
    }
    let pane = cmd_mouse_pane(&ft.m, None, None)?;
    match pane.format_value(c"mouse_line", ft)? {
        FormatValue::String(value) => Some(value),
        _ => unreachable!("mouse builtin string"),
    }
}
fn format_cb_mouse_status_line(ft: &mut format_tree) -> Option<CString> {
    let format_client_owner = ft.c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let mut value = None;
    let mut y: u_int = 0;
    if ft.m.valid == 0 {
        return None;
    }
    if format_client.is_none()
        || !format_client
            .as_ref()
            .expect("live client")
            .terminal_started()
    {
        return None;
    }
    if ft.m.statusat == 0 as ::core::ffi::c_int && ft.m.y < ft.m.statuslines {
        y = ft.m.y;
    } else if ft.m.statusat > 0 as ::core::ffi::c_int && ft.m.y >= ft.m.statusat as u_int {
        y = ft.m.y.wrapping_sub(ft.m.statusat as u_int);
    } else {
        return None;
    }
    value =
        Some(CString::new(format!("{}", (y) as u32)).expect("formatted numbers contain no NUL"));
    value
}
fn format_cb_mouse_status_range(ft: &mut format_tree) -> Option<CString> {
    let format_client_owner = ft.c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if ft.m.valid == 0 {
        return None;
    }
    if format_client.is_none()
        || !format_client
            .as_ref()
            .expect("live client")
            .terminal_started()
    {
        return None;
    }
    if ft.m.statusat == 0 as ::core::ffi::c_int && ft.m.y < ft.m.statuslines {
        x = ft.m.x;
        y = ft.m.y;
    } else if ft.m.statusat > 0 as ::core::ffi::c_int && ft.m.y >= ft.m.statusat as u_int {
        x = ft.m.x;
        y = ft.m.y.wrapping_sub(ft.m.statusat as u_int);
    } else {
        return None;
    }
    let sr = status_get_range(format_client.as_ref().expect("live client"), x, y)?;
    match sr.type_0 as ::core::ffi::c_uint {
        0 => return None,
        1 => {
            return Some(c"left".to_owned());
        }
        2 => {
            return Some(c"right".to_owned());
        }
        3 => {
            return Some(c"pane".to_owned());
        }
        4 => {
            return Some(c"window".to_owned());
        }
        5 => {
            return Some(c"session".to_owned());
        }
        6 => {
            return Some(
                CStr::from_bytes_until_nul(&sr.string)
                    .expect("range string is terminated")
                    .to_owned(),
            );
        }
        7 => {
            return Some(c"control".to_owned());
        }
        _ => {}
    }
    None
}
fn format_cb_alternate_on(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"alternate_on", ft)
}
fn format_cb_alternate_saved_x(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"alternate_saved_x", ft)
}
fn format_cb_alternate_saved_y(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"alternate_saved_y", ft)
}
fn format_cb_bracket_paste_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"bracket_paste_flag", ft)
}
fn format_cb_buffer_name(ft: &mut format_tree) -> Option<CString> {
    Some(paste_buffer_name(&*ft.pb.as_ref()?.try_borrow()?).to_owned())
}
fn format_cb_buffer_sample(ft: &mut format_tree) -> Option<CString> {
    Some(paste_make_sample_cstring(&*ft.pb.as_ref()?.try_borrow()?))
}
fn format_cb_buffer_full(ft: &mut format_tree) -> Option<CString> {
    let buffer = ft.pb.as_ref()?.try_borrow()?;
    let bytes = paste_buffer_data(&buffer)?;
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    Some(CString::new(&bytes[..end]).expect("bounded buffer contains no NUL"))
}
fn format_cb_buffer_size(ft: &mut format_tree) -> Option<CString> {
    let buffer = ft.pb.as_ref()?.try_borrow()?;
    Some(CString::new(buffer.size().to_string()).expect("formatted numbers contain no NUL"))
}
fn format_cb_client_cell_height(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_cell_height", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_cell_width(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_cell_width", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_colours(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_colours", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_control_mode(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_control_mode", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_discarded(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_discarded", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_flags(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_flags", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_height(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_height", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_key_table(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_key_table", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_last_session(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_last_session", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_name(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_name", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_pid(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_pid", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_prefix(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_prefix", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_readonly(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_readonly", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_session(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_session", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_termfeatures(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_termfeatures", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_termname(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_termname", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_termtype(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_termtype", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_tty(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_tty", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_uid(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_uid", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_user(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_user", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_utf8(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_utf8", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_width(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_width", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_written(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_written", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_theme(ft: &mut format_tree) -> Option<CString> {
    match ft.c.upgrade()?.format_value(c"client_theme", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_config_files(_ft: &mut format_tree) -> Option<CString> {
    let mut paths = Vec::<u8>::new();
    for (index, path) in cfg_files().iter().enumerate() {
        if index != 0 {
            paths.push(b',');
        }
        paths.extend_from_slice(path.as_bytes());
    }
    Some(CString::new(paths).expect("callback bytes contain no NUL"))
}
fn format_cb_cursor_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_flag", ft)
}
fn format_cb_cursor_shape(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_shape", ft)
}
fn format_cb_cursor_very_visible(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_very_visible", ft)
}
fn format_cb_cursor_x(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_x", ft)
}
fn format_cb_cursor_y(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_y", ft)
}
fn format_cb_cursor_blinking(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"cursor_blinking", ft)
}
fn format_cb_history_added(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_added", ft)
}
fn format_cb_history_collected(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_collected", ft)
}
fn format_cb_history_generation(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_generation", ft)
}
fn format_cb_history_limit(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_limit", ft)
}
fn format_cb_history_size(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"history_size", ft)
}
fn format_cb_insert_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"insert_flag", ft)
}
fn format_cb_keypad_cursor_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"keypad_cursor_flag", ft)
}
fn format_cb_keypad_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"keypad_flag", ft)
}
fn format_cb_mouse_all_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"mouse_all_flag", ft)
}
fn format_cb_mouse_any_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"mouse_any_flag", ft)
}
fn format_cb_mouse_button_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"mouse_button_flag", ft)
}
fn format_cb_mouse_pane(ft: &mut format_tree) -> Option<CString> {
    if ft.m.valid == 0 {
        return None;
    }
    let pane = cmd_mouse_pane(&ft.m, None, None)?;
    Some(CString::new(format!("%{}", pane.id())).expect("numeric pane identity"))
}
fn format_cb_mouse_sgr_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"mouse_sgr_flag", ft)
}
fn format_cb_mouse_standard_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"mouse_standard_flag", ft)
}
fn format_cb_mouse_utf8_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"mouse_utf8_flag", ft)
}
fn format_cb_mouse_x(ft: &mut format_tree) -> Option<CString> {
    if ft.m.valid == 0 {
        return None;
    }
    let pane = cmd_mouse_pane(&ft.m, None, None);
    if let Some(position) = pane.and_then(|pane| pane.mouse_position(&ft.m, false)) {
        return Some(CString::new(position.0.to_string()).expect("numeric mouse position"));
    }
    if ft
        .c
        .upgrade()
        .is_some_and(|client| client.terminal_started())
    {
        let mouse = &ft.m;
        let coordinate = if mouse.statusat == 0 && mouse.y < mouse.statuslines
            || (mouse.statusat > 0 && mouse.y >= mouse.statusat as u32)
        {
            Some(mouse.x)
        } else {
            None
        };
        return coordinate.map(|coordinate| {
            CString::new(coordinate.to_string()).expect("numeric mouse position")
        });
    }
    None
}
fn format_cb_mouse_y(ft: &mut format_tree) -> Option<CString> {
    if ft.m.valid == 0 {
        return None;
    }
    let pane = cmd_mouse_pane(&ft.m, None, None);
    if let Some(position) = pane.and_then(|pane| pane.mouse_position(&ft.m, false)) {
        return Some(CString::new(position.1.to_string()).expect("numeric mouse position"));
    }
    if ft
        .c
        .upgrade()
        .is_some_and(|client| client.terminal_started())
    {
        let mouse = &ft.m;
        let coordinate = if mouse.statusat == 0 && mouse.y < mouse.statuslines {
            Some(mouse.y)
        } else if mouse.statusat > 0 && mouse.y >= mouse.statusat as u32 {
            Some(mouse.y.wrapping_sub(mouse.statusat as u32))
        } else {
            None
        };
        return coordinate.map(|coordinate| {
            CString::new(coordinate.to_string()).expect("numeric mouse position")
        });
    }
    None
}
fn format_cb_next_session_id(_ft: &mut format_tree) -> Option<CString> {
    Some(
        CString::new(format!("${}", crate::session::sessions::next_id()))
            .expect("formatted numbers contain no NUL"),
    )
}
fn format_cb_origin_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"origin_flag", ft)
}
fn format_cb_synchronized_output_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"synchronized_output_flag", ft)
}
fn format_cb_pane_private_modes(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_private_modes", ft)
}
fn format_cb_pane_active(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_active", ft)
}
fn format_cb_pane_at_left(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_at_left", ft)
}
fn format_cb_pane_at_right(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_at_right", ft)
}
fn format_cb_pane_bottom(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_bottom", ft)
}
fn format_cb_pane_dead(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_dead", ft)
}
fn format_cb_pane_dead_signal(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_dead_signal", ft)
}
fn format_cb_pane_dead_status(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_dead_status", ft)
}
fn format_cb_pane_dead_time(ft: &mut format_tree) -> Option<time_t> {
    pane_format_time(c"pane_dead_time", ft)
}
fn format_cb_pane_last_output_time(ft: &mut format_tree) -> Option<time_t> {
    pane_format_time(c"pane_last_output_time", ft)
}
fn format_cb_pane_output_generation(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_output_generation", ft)
}
fn format_cb_pane_last_prompt_time(ft: &mut format_tree) -> Option<time_t> {
    pane_format_time(c"pane_last_prompt_time", ft)
}
fn format_cb_pane_command_start_time(ft: &mut format_tree) -> Option<time_t> {
    pane_format_time(c"pane_command_start_time", ft)
}
fn format_cb_pane_command_end_time(ft: &mut format_tree) -> Option<time_t> {
    pane_format_time(c"pane_command_end_time", ft)
}
fn format_cb_pane_command_running(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_command_running", ft)
}
fn format_cb_pane_command_duration(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_command_duration", ft)
}
fn format_cb_pane_command_status(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_command_status", ft)
}
fn format_cb_pane_format(ft: &mut format_tree) -> Option<CString> {
    if ft.type_0 as ::core::ffi::c_uint
        == FORMAT_TYPE_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}
fn format_cb_pane_height(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_height", ft)
}
fn format_cb_pane_id(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_id", ft)
}
fn format_cb_pane_index(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_index", ft)
}
fn format_cb_pane_input_off(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_input_off", ft)
}
fn format_cb_pane_unseen_changes(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_unseen_changes", ft)
}
fn format_cb_pane_key_mode(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_key_mode", ft)
}
fn format_cb_pane_last(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_last", ft)
}

fn format_cb_pane_left(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_left", ft)
}
fn format_cb_pane_marked(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_marked", ft)
}
fn format_cb_pane_marked_set(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_marked_set", ft)
}
fn format_cb_pane_mode(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_mode", ft)
}
fn format_cb_pane_path(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_path", ft)
}
fn format_cb_pane_pid(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_pid", ft)
}
fn format_cb_pane_pipe(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_pipe", ft)
}
fn format_cb_pane_pipe_pid(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_pipe_pid", ft)
}
fn format_cb_pane_pb_progress(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_pb_progress", ft)
}
fn format_cb_pane_pb_state(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_pb_state", ft)
}
fn format_cb_pane_right(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_right", ft)
}
fn format_cb_pane_search_string(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_search_string", ft)
}
fn format_cb_pane_synchronized(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_synchronized", ft)
}
fn format_cb_pane_title(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_title", ft)
}
fn format_cb_pane_top(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_top", ft)
}
fn format_cb_pane_tty(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_tty", ft)
}
fn format_cb_pane_unzoomed_height(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_unzoomed_height", ft)
}

fn format_cb_pane_unzoomed_width(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_unzoomed_width", ft)
}

fn format_cb_pane_width(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_width", ft)
}

fn format_cb_pane_zoomed_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_zoomed_flag", ft)
}
fn format_cb_pane_x(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_x", ft)
}
fn format_cb_pane_y(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"pane_y", ft)
}
fn format_cb_scroll_region_lower(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"scroll_region_lower", ft)
}
fn format_cb_scroll_region_upper(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"scroll_region_upper", ft)
}
fn format_cb_server_sessions(_ft: &mut format_tree) -> Option<CString> {
    let mut s: Option<SessionRef> = None;
    let mut n: u_int = 0 as u_int;
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        n = n.wrapping_add(1);
        s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
        s = s_owner.clone();
    }
    Some(CString::new(format!("{}", (n) as u32)).expect("formatted numbers contain no NUL"))
}
fn format_cb_session_active(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_active", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_activity_flag(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_activity_flag", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_bell_flag(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_bell_flag", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_silence_flag(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_silence_flag", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_attached(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_attached", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_format(ft: &mut format_tree) -> Option<CString> {
    if ft.type_0 as ::core::ffi::c_uint
        == FORMAT_TYPE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}
fn format_cb_session_group(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_group", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_group_attached(ft: &mut format_tree) -> Option<CString> {
    match ft
        .s
        .upgrade()?
        .format_value(c"session_group_attached", ft)?
    {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_group_many_attached(ft: &mut format_tree) -> Option<CString> {
    match ft
        .s
        .upgrade()?
        .format_value(c"session_group_many_attached", ft)?
    {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_group_size(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_group_size", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_grouped(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_grouped", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_id(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_id", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_many_attached(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_many_attached", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_marked(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_marked", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_name(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_name", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_path(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_path", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_windows(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"session_windows", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_socket_path(_ft: &mut format_tree) -> Option<CString> {
    Some(socket_path_cstr().to_owned())
}
fn format_cb_version(_ft: &mut format_tree) -> Option<CString> {
    Some(getversion().to_owned())
}
fn format_cb_sixel_support(_ft: &mut format_tree) -> Option<CString> {
    Some(c"0".to_owned())
}
fn format_cb_active_window_index(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"active_window_index", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_last_window_index(ft: &mut format_tree) -> Option<CString> {
    match ft.s.upgrade()?.format_value(c"last_window_index", ft)? {
        FormatValue::String(value) => Some(value),
        FormatValue::Time(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_window_active(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        let session_owner = (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .session
            .upgrade()?;
        if ft.winlink_handle() == session_owner.current_winlink() {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_activity_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        if (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .flags
            & WINLINK_ACTIVITY
            != 0
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_bell_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        if (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .flags
            & WINLINK_BELL
            != 0
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_bigger(ft: &mut format_tree) -> Option<CString> {
    let format_client_owner = ft.c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let c = format_client.as_ref()?;
    let view = c.terminal_view();
    Some(if view.bigger { c"1" } else { c"0" }.to_owned())
}
fn format_cb_window_cell_height(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value =
        CString::new(window.cell_size().1.to_string()).expect("formatted number contains no NUL");
    drop(window);
    Some(value)
}

fn format_cb_window_cell_width(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value =
        CString::new(window.cell_size().0.to_string()).expect("formatted number contains no NUL");
    drop(window);
    Some(value)
}

fn format_cb_window_end_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        let session_owner = (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .session
            .upgrade()?;
        if ft.winlink_handle()
            == session_owner.with_winlinks(|links| winlinks_minmax(links, RB_INF))
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_flags(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        return Some(crate::window::window_printable_flags(
            (ft.winlink_handle()).clone(),
            true,
        ));
    }
    None
}
fn format_cb_window_format(ft: &mut format_tree) -> Option<CString> {
    if ft.type_0 as ::core::ffi::c_uint
        == FORMAT_TYPE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}
fn format_cb_window_height(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value =
        CString::new(window.size().1.to_string()).expect("formatted number contains no NUL");
    drop(window);
    Some(value)
}

fn format_cb_window_manual_height(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let manual =
        options_get_number(&window.borrow_options_mut(), c"window-size") == WINDOW_SIZE_MANUAL as _;
    let value = if manual {
        CString::new(window.manual_size().1.to_string()).expect("formatted number")
    } else {
        c"".to_owned()
    };
    drop(window);
    Some(value)
}
fn format_cb_window_id(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value = CString::new(format!("@{}", window.id())).expect("formatted ID contains no NUL");
    drop(window);
    Some(value)
}

fn format_cb_window_index(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        return Some(
            CString::new(format!("{}", {
                (ft.winlink_handle())
                    .try_borrow()
                    .expect("live winlink")
                    .idx
            }))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}
fn format_cb_window_last_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        let session_owner = (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .session
            .upgrade()?;
        if ft.winlink_handle() == session_owner.last_winlink() {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_linked(ft: &mut format_tree) -> Option<CString> {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s: Option<SessionRef> = None;
    let mut found: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if ft.winlink_handle().is_alive() {
        let mut s_owner = sessions::first();
        s = s_owner.clone();
        while !s.is_none() {
            wl = s_owner
                .as_ref()
                .expect("session registry entry")
                .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
            while wl.is_alive() {
                if same(
                    winlink_window(&wl).as_ref(),
                    winlink_window(&ft.winlink_handle()).as_ref(),
                ) {
                    if found != 0 {
                        return Some(c"1".to_owned());
                    }
                    found = 1 as ::core::ffi::c_int;
                }
                wl = winlink_next(&wl);
            }
            s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
            s = s_owner.clone();
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_linked_sessions(ft: &mut format_tree) -> Option<CString> {
    let mut sg = crate::shared::session::SessionGroupWeak::new();
    let mut s: Option<SessionRef> = None;
    let mut n: u_int = 0 as u_int;
    if !ft.winlink_handle().is_alive() {
        return None;
    }
    let window = winlink_window(&ft.winlink_handle())
        .expect("linked window")
        .clone();
    sg = session_groups::minmax();
    while sg.is_alive() {
        let group_members = crate::session::session_group_members(&sg);
        s = group_members.first().cloned();
        if group_members.first().is_some_and(|session| {
            session.with_winlinks(|links| winlink_find_by_window(links, &window).is_alive())
        }) {
            n = n.wrapping_add(1);
        }
        sg = session_groups::next(&sg);
    }
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        if !crate::session::session_groups::for_session(&Rc::downgrade(
            s_owner.as_ref().expect("registered session"),
        ))
        .is_alive()
            && s_owner
                .as_ref()
                .expect("registered session")
                .with_winlinks(|links| winlink_find_by_window(links, &window).is_alive())
        {
            n = n.wrapping_add(1);
        }
        s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
        s = s_owner.clone();
    }
    drop(window);
    Some(CString::new(format!("{}", (n) as u32)).expect("formatted numbers contain no NUL"))
}
fn format_cb_window_marked_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        if server_check_marked() != 0
            && marked_pane.with_borrow(|marked| marked.winlink_handle()) == ft.winlink_handle()
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

fn format_cb_window_name(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value = window.name();
    drop(window);
    Some(value)
}

fn format_cb_window_offset_x(ft: &mut format_tree) -> Option<CString> {
    let format_client_owner = ft.c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let c = format_client.as_ref()?;
    let view = c.terminal_view();
    view.bigger
        .then(|| CString::new(view.ox.to_string()).expect("formatted number contains no NUL"))
}
fn format_cb_window_offset_y(ft: &mut format_tree) -> Option<CString> {
    let format_client_owner = ft.c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let c = format_client.as_ref()?;
    let view = c.terminal_view();
    view.bigger
        .then(|| CString::new(view.oy.to_string()).expect("formatted number contains no NUL"))
}
fn format_cb_window_panes(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value = CString::new(window.pane_snapshot().len().to_string())
        .expect("formatted number contains no NUL");
    drop(window);
    Some(value)
}

fn format_cb_window_raw_flags(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        return Some(crate::window::window_printable_flags(
            (ft.winlink_handle()).clone(),
            false,
        ));
    }
    None
}
fn format_cb_window_silence_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        if (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .flags
            & WINLINK_SILENCE
            != 0
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_start_flag(ft: &mut format_tree) -> Option<CString> {
    if ft.winlink_handle().is_alive() {
        let session_owner = (ft.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .session
            .upgrade()?;
        if ft.winlink_handle()
            == session_owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF))
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}
fn format_cb_window_width(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let value =
        CString::new(window.size().0.to_string()).expect("formatted number contains no NUL");
    drop(window);
    Some(value)
}

fn format_cb_window_zoomed_flag(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let zoomed = window.zoomed_pane().strong_count() != 0;
    drop(window);
    Some(if zoomed { c"1" } else { c"0" }.to_owned())
}

fn format_cb_window_manual_width(ft: &mut format_tree) -> Option<CString> {
    let window = ft.w.upgrade()?;
    let manual =
        options_get_number(&window.borrow_options_mut(), c"window-size") == WINDOW_SIZE_MANUAL as _;
    let value = if manual {
        CString::new(window.manual_size().0.to_string()).expect("formatted number")
    } else {
        c"".to_owned()
    };
    drop(window);
    Some(value)
}

fn format_cb_wrap_flag(ft: &mut format_tree) -> Option<CString> {
    pane_format_string(c"wrap_flag", ft)
}
fn format_cb_buffer_created(ft: &mut format_tree) -> Option<time_t> {
    Some(paste_buffer_created(&*ft.pb.as_ref()?.try_borrow()?) as __time_t as time_t)
}
fn format_cb_client_activity(ft: &mut format_tree) -> Option<time_t> {
    match ft.c.upgrade()?.format_value(c"client_activity", ft)? {
        FormatValue::Time(value) => Some(value),
        FormatValue::String(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_client_created(ft: &mut format_tree) -> Option<time_t> {
    match ft.c.upgrade()?.format_value(c"client_created", ft)? {
        FormatValue::Time(value) => Some(value),
        FormatValue::String(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_activity(ft: &mut format_tree) -> Option<time_t> {
    match ft.s.upgrade()?.format_value(c"session_activity", ft)? {
        FormatValue::Time(value) => Some(value),
        FormatValue::String(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_created(ft: &mut format_tree) -> Option<time_t> {
    match ft.s.upgrade()?.format_value(c"session_created", ft)? {
        FormatValue::Time(value) => Some(value),
        FormatValue::String(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_session_last_attached(ft: &mut format_tree) -> Option<time_t> {
    match ft.s.upgrade()?.format_value(c"session_last_attached", ft)? {
        FormatValue::Time(value) => Some(value),
        FormatValue::String(_) => unreachable!("builtin value type"),
    }
}
fn format_cb_start_time(_ft: &mut format_tree) -> Option<time_t> {
    Some(crate::shared::time::unix_seconds(start_time()))
}
fn format_cb_window_activity(ft: &mut format_tree) -> Option<time_t> {
    let window = ft.w.upgrade()?;
    let value = crate::shared::time::unix_seconds(window.activity_time());
    drop(window);
    Some(value)
}

fn format_cb_buffer_mode_format(_ft: &mut format_tree) -> Option<CString> {
    Some(
        window_buffer_mode
            .default_format
            .expect("mode default format")
            .to_owned(),
    )
}
fn format_cb_client_mode_format(_ft: &mut format_tree) -> Option<CString> {
    Some(
        window_client_mode
            .default_format
            .expect("mode default format")
            .to_owned(),
    )
}
fn format_cb_tree_mode_format(_ft: &mut format_tree) -> Option<CString> {
    Some(
        window_tree_mode
            .default_format
            .expect("mode default format")
            .to_owned(),
    )
}
fn format_cb_uid(_ft: &mut format_tree) -> Option<CString> {
    Some(
        CString::new(format!("{}", unsafe { getuid() } as ::core::ffi::c_long))
            .expect("formatted numbers contain no NUL"),
    )
}
fn format_cb_user(_ft: &mut format_tree) -> Option<CString> {
    static CACHED: std::sync::OnceLock<CString> = std::sync::OnceLock::new();
    if let Some(name) = CACHED.get() {
        return Some(name.clone());
    }
    let name = unsafe {
        let pw = getpwuid(getuid());
        if pw.is_null() {
            return None;
        }
        CStr::from_ptr((*pw).pw_name).to_owned()
    };
    let _ = CACHED.set(name);
    CACHED.get().cloned()
}
pub enum FormatValue {
    String(CString),
    Time(time_t),
}
pub(super) enum FormatCallback {
    String(fn(&mut format_tree) -> Option<CString>),
    Time(fn(&mut format_tree) -> Option<time_t>),
}
pub(super) struct FormatTableEntry {
    pub key: &'static CStr,
    callback: FormatCallback,
}
impl FormatTableEntry {
    pub fn get(&self, ft: &mut format_tree) -> Option<FormatValue> {
        use crate::server_client::Client;
        use crate::session::Session;
        use crate::window::{Window, WindowPane};
        if model_key(self.key, b"session_") {
            if let Some(owner) = ft.s.upgrade() {
                return owner.format_value(self.key, ft);
            }
        } else if model_key(self.key, b"window_") {
            if let Some(owner) = ft.w.upgrade() {
                let result = owner.format_value(self.key, ft);
                drop(owner);
                return result;
            }
        } else if model_key(self.key, b"pane_") {
            if let Some(owner) = ft.wp.upgrade() {
                return owner.format_value(self.key, ft);
            }
        } else if model_key(self.key, b"client_") {
            if let Some(owner) = ft.c.upgrade() {
                return owner.format_value(self.key, ft);
            }
        }
        self.evaluate(ft)
    }

    fn evaluate(&self, ft: &mut format_tree) -> Option<FormatValue> {
        match self.callback {
            FormatCallback::String(cb) => cb(ft).map(FormatValue::String),
            FormatCallback::Time(cb) => cb(ft).map(FormatValue::Time),
        }
    }
}

fn model_key(key: &CStr, prefix: &[u8]) -> bool {
    key.to_bytes().starts_with(prefix) && !key.to_bytes().ends_with(b"_mode_format")
}

pub(crate) fn window_format_value(
    owner: &WindowRef,
    key: &CStr,
    context: &mut format_tree,
) -> Option<FormatValue> {
    if !context.w.ptr_eq(&WindowRef::downgrade(owner)) || !model_key(key, b"window_") {
        return None;
    }
    format_table_get(key)?.evaluate(context)
}

pub(super) static FORMAT_TABLE: [FormatTableEntry; 211] = [
    FormatTableEntry {
        key: c"active_window_index",
        callback: FormatCallback::String(format_cb_active_window_index),
    },
    FormatTableEntry {
        key: c"alternate_on",
        callback: FormatCallback::String(format_cb_alternate_on),
    },
    FormatTableEntry {
        key: c"alternate_saved_x",
        callback: FormatCallback::String(format_cb_alternate_saved_x),
    },
    FormatTableEntry {
        key: c"alternate_saved_y",
        callback: FormatCallback::String(format_cb_alternate_saved_y),
    },
    FormatTableEntry {
        key: c"bracket_paste_flag",
        callback: FormatCallback::String(format_cb_bracket_paste_flag),
    },
    FormatTableEntry {
        key: c"buffer_created",
        callback: FormatCallback::Time(format_cb_buffer_created),
    },
    FormatTableEntry {
        key: c"buffer_full",
        callback: FormatCallback::String(format_cb_buffer_full),
    },
    FormatTableEntry {
        key: c"buffer_mode_format",
        callback: FormatCallback::String(format_cb_buffer_mode_format),
    },
    FormatTableEntry {
        key: c"buffer_name",
        callback: FormatCallback::String(format_cb_buffer_name),
    },
    FormatTableEntry {
        key: c"buffer_sample",
        callback: FormatCallback::String(format_cb_buffer_sample),
    },
    FormatTableEntry {
        key: c"buffer_size",
        callback: FormatCallback::String(format_cb_buffer_size),
    },
    FormatTableEntry {
        key: c"client_activity",
        callback: FormatCallback::Time(format_cb_client_activity),
    },
    FormatTableEntry {
        key: c"client_cell_height",
        callback: FormatCallback::String(format_cb_client_cell_height),
    },
    FormatTableEntry {
        key: c"client_cell_width",
        callback: FormatCallback::String(format_cb_client_cell_width),
    },
    FormatTableEntry {
        key: c"client_colours",
        callback: FormatCallback::String(format_cb_client_colours),
    },
    FormatTableEntry {
        key: c"client_control_mode",
        callback: FormatCallback::String(format_cb_client_control_mode),
    },
    FormatTableEntry {
        key: c"client_created",
        callback: FormatCallback::Time(format_cb_client_created),
    },
    FormatTableEntry {
        key: c"client_discarded",
        callback: FormatCallback::String(format_cb_client_discarded),
    },
    FormatTableEntry {
        key: c"client_flags",
        callback: FormatCallback::String(format_cb_client_flags),
    },
    FormatTableEntry {
        key: c"client_height",
        callback: FormatCallback::String(format_cb_client_height),
    },
    FormatTableEntry {
        key: c"client_key_table",
        callback: FormatCallback::String(format_cb_client_key_table),
    },
    FormatTableEntry {
        key: c"client_last_session",
        callback: FormatCallback::String(format_cb_client_last_session),
    },
    FormatTableEntry {
        key: c"client_mode_format",
        callback: FormatCallback::String(format_cb_client_mode_format),
    },
    FormatTableEntry {
        key: c"client_name",
        callback: FormatCallback::String(format_cb_client_name),
    },
    FormatTableEntry {
        key: c"client_pid",
        callback: FormatCallback::String(format_cb_client_pid),
    },
    FormatTableEntry {
        key: c"client_prefix",
        callback: FormatCallback::String(format_cb_client_prefix),
    },
    FormatTableEntry {
        key: c"client_readonly",
        callback: FormatCallback::String(format_cb_client_readonly),
    },
    FormatTableEntry {
        key: c"client_session",
        callback: FormatCallback::String(format_cb_client_session),
    },
    FormatTableEntry {
        key: c"client_termfeatures",
        callback: FormatCallback::String(format_cb_client_termfeatures),
    },
    FormatTableEntry {
        key: c"client_termname",
        callback: FormatCallback::String(format_cb_client_termname),
    },
    FormatTableEntry {
        key: c"client_termtype",
        callback: FormatCallback::String(format_cb_client_termtype),
    },
    FormatTableEntry {
        key: c"client_theme",
        callback: FormatCallback::String(format_cb_client_theme),
    },
    FormatTableEntry {
        key: c"client_tty",
        callback: FormatCallback::String(format_cb_client_tty),
    },
    FormatTableEntry {
        key: c"client_uid",
        callback: FormatCallback::String(format_cb_client_uid),
    },
    FormatTableEntry {
        key: c"client_user",
        callback: FormatCallback::String(format_cb_client_user),
    },
    FormatTableEntry {
        key: c"client_utf8",
        callback: FormatCallback::String(format_cb_client_utf8),
    },
    FormatTableEntry {
        key: c"client_width",
        callback: FormatCallback::String(format_cb_client_width),
    },
    FormatTableEntry {
        key: c"client_written",
        callback: FormatCallback::String(format_cb_client_written),
    },
    FormatTableEntry {
        key: c"config_files",
        callback: FormatCallback::String(format_cb_config_files),
    },
    FormatTableEntry {
        key: c"cursor_blinking",
        callback: FormatCallback::String(format_cb_cursor_blinking),
    },
    FormatTableEntry {
        key: c"cursor_character",
        callback: FormatCallback::String(format_cb_cursor_character),
    },
    FormatTableEntry {
        key: c"cursor_colour",
        callback: FormatCallback::String(format_cb_cursor_colour),
    },
    FormatTableEntry {
        key: c"cursor_flag",
        callback: FormatCallback::String(format_cb_cursor_flag),
    },
    FormatTableEntry {
        key: c"cursor_shape",
        callback: FormatCallback::String(format_cb_cursor_shape),
    },
    FormatTableEntry {
        key: c"cursor_very_visible",
        callback: FormatCallback::String(format_cb_cursor_very_visible),
    },
    FormatTableEntry {
        key: c"cursor_x",
        callback: FormatCallback::String(format_cb_cursor_x),
    },
    FormatTableEntry {
        key: c"cursor_y",
        callback: FormatCallback::String(format_cb_cursor_y),
    },
    FormatTableEntry {
        key: c"history_added",
        callback: FormatCallback::String(format_cb_history_added),
    },
    FormatTableEntry {
        key: c"history_all_bytes",
        callback: FormatCallback::String(format_cb_history_all_bytes),
    },
    FormatTableEntry {
        key: c"history_bytes",
        callback: FormatCallback::String(format_cb_history_bytes),
    },
    FormatTableEntry {
        key: c"history_collected",
        callback: FormatCallback::String(format_cb_history_collected),
    },
    FormatTableEntry {
        key: c"history_generation",
        callback: FormatCallback::String(format_cb_history_generation),
    },
    FormatTableEntry {
        key: c"history_limit",
        callback: FormatCallback::String(format_cb_history_limit),
    },
    FormatTableEntry {
        key: c"history_size",
        callback: FormatCallback::String(format_cb_history_size),
    },
    FormatTableEntry {
        key: c"host",
        callback: FormatCallback::String(format_cb_host),
    },
    FormatTableEntry {
        key: c"host_short",
        callback: FormatCallback::String(format_cb_host_short),
    },
    FormatTableEntry {
        key: c"insert_flag",
        callback: FormatCallback::String(format_cb_insert_flag),
    },
    FormatTableEntry {
        key: c"keypad_cursor_flag",
        callback: FormatCallback::String(format_cb_keypad_cursor_flag),
    },
    FormatTableEntry {
        key: c"keypad_flag",
        callback: FormatCallback::String(format_cb_keypad_flag),
    },
    FormatTableEntry {
        key: c"last_window_index",
        callback: FormatCallback::String(format_cb_last_window_index),
    },
    FormatTableEntry {
        key: c"mouse_all_flag",
        callback: FormatCallback::String(format_cb_mouse_all_flag),
    },
    FormatTableEntry {
        key: c"mouse_any_flag",
        callback: FormatCallback::String(format_cb_mouse_any_flag),
    },
    FormatTableEntry {
        key: c"mouse_button_flag",
        callback: FormatCallback::String(format_cb_mouse_button_flag),
    },
    FormatTableEntry {
        key: c"mouse_hyperlink",
        callback: FormatCallback::String(format_cb_mouse_hyperlink),
    },
    FormatTableEntry {
        key: c"mouse_line",
        callback: FormatCallback::String(format_cb_mouse_line),
    },
    FormatTableEntry {
        key: c"mouse_pane",
        callback: FormatCallback::String(format_cb_mouse_pane),
    },
    FormatTableEntry {
        key: c"mouse_sgr_flag",
        callback: FormatCallback::String(format_cb_mouse_sgr_flag),
    },
    FormatTableEntry {
        key: c"mouse_standard_flag",
        callback: FormatCallback::String(format_cb_mouse_standard_flag),
    },
    FormatTableEntry {
        key: c"mouse_status_line",
        callback: FormatCallback::String(format_cb_mouse_status_line),
    },
    FormatTableEntry {
        key: c"mouse_status_range",
        callback: FormatCallback::String(format_cb_mouse_status_range),
    },
    FormatTableEntry {
        key: c"mouse_utf8_flag",
        callback: FormatCallback::String(format_cb_mouse_utf8_flag),
    },
    FormatTableEntry {
        key: c"mouse_word",
        callback: FormatCallback::String(format_cb_mouse_word),
    },
    FormatTableEntry {
        key: c"mouse_x",
        callback: FormatCallback::String(format_cb_mouse_x),
    },
    FormatTableEntry {
        key: c"mouse_y",
        callback: FormatCallback::String(format_cb_mouse_y),
    },
    FormatTableEntry {
        key: c"next_session_id",
        callback: FormatCallback::String(format_cb_next_session_id),
    },
    FormatTableEntry {
        key: c"origin_flag",
        callback: FormatCallback::String(format_cb_origin_flag),
    },
    FormatTableEntry {
        key: c"pane_active",
        callback: FormatCallback::String(format_cb_pane_active),
    },
    FormatTableEntry {
        key: c"pane_at_bottom",
        callback: FormatCallback::String(format_cb_pane_at_bottom),
    },
    FormatTableEntry {
        key: c"pane_at_left",
        callback: FormatCallback::String(format_cb_pane_at_left),
    },
    FormatTableEntry {
        key: c"pane_at_right",
        callback: FormatCallback::String(format_cb_pane_at_right),
    },
    FormatTableEntry {
        key: c"pane_at_top",
        callback: FormatCallback::String(format_cb_pane_at_top),
    },
    FormatTableEntry {
        key: c"pane_bg",
        callback: FormatCallback::String(format_cb_pane_bg),
    },
    FormatTableEntry {
        key: c"pane_bottom",
        callback: FormatCallback::String(format_cb_pane_bottom),
    },
    FormatTableEntry {
        key: c"pane_command_duration",
        callback: FormatCallback::String(format_cb_pane_command_duration),
    },
    FormatTableEntry {
        key: c"pane_command_end_time",
        callback: FormatCallback::Time(format_cb_pane_command_end_time),
    },
    FormatTableEntry {
        key: c"pane_command_running",
        callback: FormatCallback::String(format_cb_pane_command_running),
    },
    FormatTableEntry {
        key: c"pane_command_start_time",
        callback: FormatCallback::Time(format_cb_pane_command_start_time),
    },
    FormatTableEntry {
        key: c"pane_command_status",
        callback: FormatCallback::String(format_cb_pane_command_status),
    },
    FormatTableEntry {
        key: c"pane_current_command",
        callback: FormatCallback::String(format_cb_current_command),
    },
    FormatTableEntry {
        key: c"pane_current_path",
        callback: FormatCallback::String(format_cb_current_path),
    },
    FormatTableEntry {
        key: c"pane_dead",
        callback: FormatCallback::String(format_cb_pane_dead),
    },
    FormatTableEntry {
        key: c"pane_dead_signal",
        callback: FormatCallback::String(format_cb_pane_dead_signal),
    },
    FormatTableEntry {
        key: c"pane_dead_status",
        callback: FormatCallback::String(format_cb_pane_dead_status),
    },
    FormatTableEntry {
        key: c"pane_dead_time",
        callback: FormatCallback::Time(format_cb_pane_dead_time),
    },
    FormatTableEntry {
        key: c"pane_fg",
        callback: FormatCallback::String(format_cb_pane_fg),
    },
    FormatTableEntry {
        key: c"pane_flags",
        callback: FormatCallback::String(format_cb_pane_flags),
    },
    FormatTableEntry {
        key: c"pane_format",
        callback: FormatCallback::String(format_cb_pane_format),
    },
    FormatTableEntry {
        key: c"pane_height",
        callback: FormatCallback::String(format_cb_pane_height),
    },
    FormatTableEntry {
        key: c"pane_id",
        callback: FormatCallback::String(format_cb_pane_id),
    },
    FormatTableEntry {
        key: c"pane_in_mode",
        callback: FormatCallback::String(format_cb_pane_in_mode),
    },
    FormatTableEntry {
        key: c"pane_index",
        callback: FormatCallback::String(format_cb_pane_index),
    },
    FormatTableEntry {
        key: c"pane_input_off",
        callback: FormatCallback::String(format_cb_pane_input_off),
    },
    FormatTableEntry {
        key: c"pane_key_mode",
        callback: FormatCallback::String(format_cb_pane_key_mode),
    },
    FormatTableEntry {
        key: c"pane_last",
        callback: FormatCallback::String(format_cb_pane_last),
    },
    FormatTableEntry {
        key: c"pane_last_output_time",
        callback: FormatCallback::Time(format_cb_pane_last_output_time),
    },
    FormatTableEntry {
        key: c"pane_last_prompt_time",
        callback: FormatCallback::Time(format_cb_pane_last_prompt_time),
    },
    FormatTableEntry {
        key: c"pane_left",
        callback: FormatCallback::String(format_cb_pane_left),
    },
    FormatTableEntry {
        key: c"pane_marked",
        callback: FormatCallback::String(format_cb_pane_marked),
    },
    FormatTableEntry {
        key: c"pane_marked_set",
        callback: FormatCallback::String(format_cb_pane_marked_set),
    },
    FormatTableEntry {
        key: c"pane_mode",
        callback: FormatCallback::String(format_cb_pane_mode),
    },
    FormatTableEntry {
        key: c"pane_output_generation",
        callback: FormatCallback::String(format_cb_pane_output_generation),
    },
    FormatTableEntry {
        key: c"pane_path",
        callback: FormatCallback::String(format_cb_pane_path),
    },
    FormatTableEntry {
        key: c"pane_pb_progress",
        callback: FormatCallback::String(format_cb_pane_pb_progress),
    },
    FormatTableEntry {
        key: c"pane_pb_state",
        callback: FormatCallback::String(format_cb_pane_pb_state),
    },
    FormatTableEntry {
        key: c"pane_pid",
        callback: FormatCallback::String(format_cb_pane_pid),
    },
    FormatTableEntry {
        key: c"pane_pipe",
        callback: FormatCallback::String(format_cb_pane_pipe),
    },
    FormatTableEntry {
        key: c"pane_pipe_pid",
        callback: FormatCallback::String(format_cb_pane_pipe_pid),
    },
    FormatTableEntry {
        key: c"pane_private_modes",
        callback: FormatCallback::String(format_cb_pane_private_modes),
    },
    FormatTableEntry {
        key: c"pane_right",
        callback: FormatCallback::String(format_cb_pane_right),
    },
    FormatTableEntry {
        key: c"pane_search_string",
        callback: FormatCallback::String(format_cb_pane_search_string),
    },
    FormatTableEntry {
        key: c"pane_start_command",
        callback: FormatCallback::String(format_cb_start_command),
    },
    FormatTableEntry {
        key: c"pane_start_command_list",
        callback: FormatCallback::String(format_cb_start_command_list),
    },
    FormatTableEntry {
        key: c"pane_start_path",
        callback: FormatCallback::String(format_cb_start_path),
    },
    FormatTableEntry {
        key: c"pane_synchronized",
        callback: FormatCallback::String(format_cb_pane_synchronized),
    },
    FormatTableEntry {
        key: c"pane_tabs",
        callback: FormatCallback::String(format_cb_pane_tabs),
    },
    FormatTableEntry {
        key: c"pane_title",
        callback: FormatCallback::String(format_cb_pane_title),
    },
    FormatTableEntry {
        key: c"pane_top",
        callback: FormatCallback::String(format_cb_pane_top),
    },
    FormatTableEntry {
        key: c"pane_tty",
        callback: FormatCallback::String(format_cb_pane_tty),
    },
    FormatTableEntry {
        key: c"pane_unseen_changes",
        callback: FormatCallback::String(format_cb_pane_unseen_changes),
    },
    FormatTableEntry {
        key: c"pane_unzoomed_height",
        callback: FormatCallback::String(format_cb_pane_unzoomed_height),
    },
    FormatTableEntry {
        key: c"pane_unzoomed_width",
        callback: FormatCallback::String(format_cb_pane_unzoomed_width),
    },
    FormatTableEntry {
        key: c"pane_width",
        callback: FormatCallback::String(format_cb_pane_width),
    },
    FormatTableEntry {
        key: c"pane_x",
        callback: FormatCallback::String(format_cb_pane_x),
    },
    FormatTableEntry {
        key: c"pane_y",
        callback: FormatCallback::String(format_cb_pane_y),
    },
    FormatTableEntry {
        key: c"pane_zoomed_flag",
        callback: FormatCallback::String(format_cb_pane_zoomed_flag),
    },
    FormatTableEntry {
        key: c"pid",
        callback: FormatCallback::String(format_cb_pid),
    },
    FormatTableEntry {
        key: c"scroll_region_lower",
        callback: FormatCallback::String(format_cb_scroll_region_lower),
    },
    FormatTableEntry {
        key: c"scroll_region_upper",
        callback: FormatCallback::String(format_cb_scroll_region_upper),
    },
    FormatTableEntry {
        key: c"server_sessions",
        callback: FormatCallback::String(format_cb_server_sessions),
    },
    FormatTableEntry {
        key: c"session_active",
        callback: FormatCallback::String(format_cb_session_active),
    },
    FormatTableEntry {
        key: c"session_activity",
        callback: FormatCallback::Time(format_cb_session_activity),
    },
    FormatTableEntry {
        key: c"session_activity_flag",
        callback: FormatCallback::String(format_cb_session_activity_flag),
    },
    FormatTableEntry {
        key: c"session_alert",
        callback: FormatCallback::String(format_cb_session_alert),
    },
    FormatTableEntry {
        key: c"session_alerts",
        callback: FormatCallback::String(format_cb_session_alerts),
    },
    FormatTableEntry {
        key: c"session_attached",
        callback: FormatCallback::String(format_cb_session_attached),
    },
    FormatTableEntry {
        key: c"session_attached_list",
        callback: FormatCallback::String(format_cb_session_attached_list),
    },
    FormatTableEntry {
        key: c"session_bell_flag",
        callback: FormatCallback::String(format_cb_session_bell_flag),
    },
    FormatTableEntry {
        key: c"session_created",
        callback: FormatCallback::Time(format_cb_session_created),
    },
    FormatTableEntry {
        key: c"session_format",
        callback: FormatCallback::String(format_cb_session_format),
    },
    FormatTableEntry {
        key: c"session_group",
        callback: FormatCallback::String(format_cb_session_group),
    },
    FormatTableEntry {
        key: c"session_group_attached",
        callback: FormatCallback::String(format_cb_session_group_attached),
    },
    FormatTableEntry {
        key: c"session_group_attached_list",
        callback: FormatCallback::String(format_cb_session_group_attached_list),
    },
    FormatTableEntry {
        key: c"session_group_list",
        callback: FormatCallback::String(format_cb_session_group_list),
    },
    FormatTableEntry {
        key: c"session_group_many_attached",
        callback: FormatCallback::String(format_cb_session_group_many_attached),
    },
    FormatTableEntry {
        key: c"session_group_size",
        callback: FormatCallback::String(format_cb_session_group_size),
    },
    FormatTableEntry {
        key: c"session_grouped",
        callback: FormatCallback::String(format_cb_session_grouped),
    },
    FormatTableEntry {
        key: c"session_id",
        callback: FormatCallback::String(format_cb_session_id),
    },
    FormatTableEntry {
        key: c"session_last_attached",
        callback: FormatCallback::Time(format_cb_session_last_attached),
    },
    FormatTableEntry {
        key: c"session_many_attached",
        callback: FormatCallback::String(format_cb_session_many_attached),
    },
    FormatTableEntry {
        key: c"session_marked",
        callback: FormatCallback::String(format_cb_session_marked),
    },
    FormatTableEntry {
        key: c"session_name",
        callback: FormatCallback::String(format_cb_session_name),
    },
    FormatTableEntry {
        key: c"session_path",
        callback: FormatCallback::String(format_cb_session_path),
    },
    FormatTableEntry {
        key: c"session_silence_flag",
        callback: FormatCallback::String(format_cb_session_silence_flag),
    },
    FormatTableEntry {
        key: c"session_stack",
        callback: FormatCallback::String(format_cb_session_stack),
    },
    FormatTableEntry {
        key: c"session_windows",
        callback: FormatCallback::String(format_cb_session_windows),
    },
    FormatTableEntry {
        key: c"sixel_support",
        callback: FormatCallback::String(format_cb_sixel_support),
    },
    FormatTableEntry {
        key: c"socket_path",
        callback: FormatCallback::String(format_cb_socket_path),
    },
    FormatTableEntry {
        key: c"start_time",
        callback: FormatCallback::Time(format_cb_start_time),
    },
    FormatTableEntry {
        key: c"synchronized_output_flag",
        callback: FormatCallback::String(format_cb_synchronized_output_flag),
    },
    FormatTableEntry {
        key: c"tree_mode_format",
        callback: FormatCallback::String(format_cb_tree_mode_format),
    },
    FormatTableEntry {
        key: c"uid",
        callback: FormatCallback::String(format_cb_uid),
    },
    FormatTableEntry {
        key: c"user",
        callback: FormatCallback::String(format_cb_user),
    },
    FormatTableEntry {
        key: c"version",
        callback: FormatCallback::String(format_cb_version),
    },
    FormatTableEntry {
        key: c"window_active",
        callback: FormatCallback::String(format_cb_window_active),
    },
    FormatTableEntry {
        key: c"window_active_clients",
        callback: FormatCallback::String(format_cb_window_active_clients),
    },
    FormatTableEntry {
        key: c"window_active_clients_list",
        callback: FormatCallback::String(format_cb_window_active_clients_list),
    },
    FormatTableEntry {
        key: c"window_active_sessions",
        callback: FormatCallback::String(format_cb_window_active_sessions),
    },
    FormatTableEntry {
        key: c"window_active_sessions_list",
        callback: FormatCallback::String(format_cb_window_active_sessions_list),
    },
    FormatTableEntry {
        key: c"window_activity",
        callback: FormatCallback::Time(format_cb_window_activity),
    },
    FormatTableEntry {
        key: c"window_activity_flag",
        callback: FormatCallback::String(format_cb_window_activity_flag),
    },
    FormatTableEntry {
        key: c"window_bell_flag",
        callback: FormatCallback::String(format_cb_window_bell_flag),
    },
    FormatTableEntry {
        key: c"window_bigger",
        callback: FormatCallback::String(format_cb_window_bigger),
    },
    FormatTableEntry {
        key: c"window_cell_height",
        callback: FormatCallback::String(format_cb_window_cell_height),
    },
    FormatTableEntry {
        key: c"window_cell_width",
        callback: FormatCallback::String(format_cb_window_cell_width),
    },
    FormatTableEntry {
        key: c"window_end_flag",
        callback: FormatCallback::String(format_cb_window_end_flag),
    },
    FormatTableEntry {
        key: c"window_flags",
        callback: FormatCallback::String(format_cb_window_flags),
    },
    FormatTableEntry {
        key: c"window_format",
        callback: FormatCallback::String(format_cb_window_format),
    },
    FormatTableEntry {
        key: c"window_height",
        callback: FormatCallback::String(format_cb_window_height),
    },
    FormatTableEntry {
        key: c"window_id",
        callback: FormatCallback::String(format_cb_window_id),
    },
    FormatTableEntry {
        key: c"window_index",
        callback: FormatCallback::String(format_cb_window_index),
    },
    FormatTableEntry {
        key: c"window_last_flag",
        callback: FormatCallback::String(format_cb_window_last_flag),
    },
    FormatTableEntry {
        key: c"window_layout",
        callback: FormatCallback::String(format_cb_window_layout),
    },
    FormatTableEntry {
        key: c"window_layout_name",
        callback: FormatCallback::String(format_cb_window_layout_name),
    },
    FormatTableEntry {
        key: c"window_linked",
        callback: FormatCallback::String(format_cb_window_linked),
    },
    FormatTableEntry {
        key: c"window_linked_sessions",
        callback: FormatCallback::String(format_cb_window_linked_sessions),
    },
    FormatTableEntry {
        key: c"window_linked_sessions_list",
        callback: FormatCallback::String(format_cb_window_linked_sessions_list),
    },
    FormatTableEntry {
        key: c"window_manual_height",
        callback: FormatCallback::String(format_cb_window_manual_height),
    },
    FormatTableEntry {
        key: c"window_manual_width",
        callback: FormatCallback::String(format_cb_window_manual_width),
    },
    FormatTableEntry {
        key: c"window_marked_flag",
        callback: FormatCallback::String(format_cb_window_marked_flag),
    },
    FormatTableEntry {
        key: c"window_name",
        callback: FormatCallback::String(format_cb_window_name),
    },
    FormatTableEntry {
        key: c"window_offset_x",
        callback: FormatCallback::String(format_cb_window_offset_x),
    },
    FormatTableEntry {
        key: c"window_offset_y",
        callback: FormatCallback::String(format_cb_window_offset_y),
    },
    FormatTableEntry {
        key: c"window_panes",
        callback: FormatCallback::String(format_cb_window_panes),
    },
    FormatTableEntry {
        key: c"window_raw_flags",
        callback: FormatCallback::String(format_cb_window_raw_flags),
    },
    FormatTableEntry {
        key: c"window_silence_flag",
        callback: FormatCallback::String(format_cb_window_silence_flag),
    },
    FormatTableEntry {
        key: c"window_stack_index",
        callback: FormatCallback::String(format_cb_window_stack_index),
    },
    FormatTableEntry {
        key: c"window_start_flag",
        callback: FormatCallback::String(format_cb_window_start_flag),
    },
    FormatTableEntry {
        key: c"window_visible_layout",
        callback: FormatCallback::String(format_cb_window_visible_layout),
    },
    FormatTableEntry {
        key: c"window_width",
        callback: FormatCallback::String(format_cb_window_width),
    },
    FormatTableEntry {
        key: c"window_zoomed_flag",
        callback: FormatCallback::String(format_cb_window_zoomed_flag),
    },
    FormatTableEntry {
        key: c"wrap_flag",
        callback: FormatCallback::String(format_cb_wrap_flag),
    },
];
pub(super) fn format_table_get(key: &CStr) -> Option<&'static FormatTableEntry> {
    FORMAT_TABLE
        .binary_search_by(|entry| entry.key.cmp(key))
        .ok()
        .map(|index| &FORMAT_TABLE[index])
}

#[cfg(test)]
mod owned_callback_tests {
    use super::*;

    #[test]
    fn buffer_formats_preserve_missing_empty_and_logical_binary_lengths() {
        let owner = refbox::RefBox::new(crate::shared::paste::paste_buffer::empty());
        let buffer = crate::shared::paste::PasteBufferRef::observe(&owner);
        {
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &mut *ft_owner;
            ft.pb = Some(buffer.clone());
            assert!(paste_buffer_data(&buffer.borrow()).is_none());
            assert!(format_cb_buffer_full(ft).is_none());
            assert_eq!(format_cb_buffer_size(ft).unwrap().as_c_str(), c"0");

            buffer.borrow_mut().data = Some(Box::default());
            assert_eq!(paste_buffer_data(&buffer.borrow()), Some(&b""[..]));
            assert_eq!(format_cb_buffer_full(ft).unwrap().as_c_str(), c"");

            buffer.borrow_mut().data = Some(b"A\xff\0B".to_vec().into_boxed_slice());
            {
                let value = buffer.borrow();
                let bytes = paste_buffer_data(&value).unwrap();
                assert_eq!(bytes, b"A\xff\0B");
                assert_eq!(bytes.as_ptr(), value.data.as_ref().unwrap().as_ptr());
            }
            assert_eq!(format_cb_buffer_full(ft).unwrap().as_bytes(), b"A\xff");
            assert_eq!(format_cb_buffer_size(ft).unwrap().as_c_str(), c"4");

            buffer.borrow_mut().data = Some(b"A".to_vec().into_boxed_slice());
            assert_eq!(format_cb_buffer_full(ft).unwrap().as_c_str(), c"A");
            assert_eq!(format_cb_buffer_size(ft).unwrap().as_c_str(), c"1");
            ft.pb = None;
            drop(ft_owner);
        }
    }

    #[test]
    fn builtin_lookup_preserves_sorted_keys_and_missing_results() {
        assert!(FORMAT_TABLE
            .windows(2)
            .all(|pair| pair[0].key < pair[1].key));
        for entry in &FORMAT_TABLE {
            assert!(std::ptr::eq(format_table_get(entry.key).unwrap(), entry));
        }
        assert!(format_table_get(c"not_a_builtin").is_none());
        let mut ft_owner = format_create(None, None, 0, 0);
        let ft = &mut *ft_owner;
        for key in [
            c"buffer_full",
            c"client_created",
            c"session_created",
            c"pane_dead_time",
        ] {
            assert!(format_table_get(key).unwrap().get(ft).is_none());
        }
        drop(ft_owner);
    }
}
