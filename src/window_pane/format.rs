//! Pane-owned lazy formatting. Values leave the pane as owned bytes or timestamps.
use super::*;
use crate::src::cmd::cmd_stringify_argv_cstring;
use crate::src::format::format_quote_shell_single;
use crate::src::format::C2RustUnnamed_43;
use crate::src::format::{format_tree, FormatValue, FORMAT_TYPE_PANE};
use crate::src::grid::grid_get_line;
use crate::src::grid::view::grid_view_get_cell;
use crate::src::names::parse_window_name_cstring;
use crate::src::osdep::{osdep_get_cwd, osdep_get_name_cstring};
use crate::src::shared::pane::{PANE_CMDRUNNING, PANE_STATUSDRAWN};
use crate::src::shared::screen::*;
use crate::src::style::colour::colour_format;
use crate::src::tmux::sig2name;
use libc::time;
use std::fmt::Write as _;
use std::os::fd::AsRawFd;
use std::os::unix::process::ExitStatusExt;

pub(super) unsafe fn format_value(
    owner: &Rc<UnsafeCell<window_pane>>,
    key: &CStr,
    context: &mut format_tree,
) -> Option<FormatValue> {
    if matches!(
        key.to_bytes(),
        b"mouse_word" | b"mouse_hyperlink" | b"mouse_line"
    ) {
        return mouse_value(owner, key, context).map(FormatValue::String);
    }
    if !context.wp.ptr_eq(&Rc::downgrade(owner)) {
        return None;
    }
    evaluate(key, context)
}

/// Preserve builtin defaults when the format context has no live pane.
/// No model can be accessed along this path.
pub(crate) unsafe fn format_without_pane(
    key: &CStr,
    context: &mut format_tree,
) -> Option<FormatValue> {
    assert!(context.wp.upgrade().is_none(), "absent pane format context");
    evaluate(key, context)
}

unsafe fn evaluate(key: &CStr, context: &mut format_tree) -> Option<FormatValue> {
    match key.to_bytes() {
        b"pane_format" => Some(FormatValue::String(
            if context.type_0 == FORMAT_TYPE_PANE {
                c"1"
            } else {
                c"0"
            }
            .to_owned(),
        )),
        b"pane_start_command" => format_cb_start_command(context).map(FormatValue::String),
        b"pane_start_command_list" => {
            format_cb_start_command_list(context).map(FormatValue::String)
        }
        b"pane_start_path" => format_cb_start_path(context).map(FormatValue::String),
        b"pane_current_command" => format_cb_current_command(context).map(FormatValue::String),
        b"pane_current_path" => format_cb_current_path(context).map(FormatValue::String),
        b"history_bytes" => format_cb_history_bytes(context).map(FormatValue::String),
        b"history_all_bytes" => format_cb_history_all_bytes(context).map(FormatValue::String),
        b"pane_tabs" => format_cb_pane_tabs(context).map(FormatValue::String),
        b"pane_fg" => format_cb_pane_fg(context).map(FormatValue::String),
        b"pane_flags" => format_cb_pane_flags(context).map(FormatValue::String),
        b"pane_bg" => format_cb_pane_bg(context).map(FormatValue::String),
        b"pane_in_mode" => format_cb_pane_in_mode(context).map(FormatValue::String),
        b"pane_at_top" => format_cb_pane_at_top(context).map(FormatValue::String),
        b"pane_at_bottom" => format_cb_pane_at_bottom(context).map(FormatValue::String),
        b"cursor_character" => format_cb_cursor_character(context).map(FormatValue::String),
        b"cursor_colour" => format_cb_cursor_colour(context).map(FormatValue::String),
        b"alternate_on" => format_cb_alternate_on(context).map(FormatValue::String),
        b"alternate_saved_x" => format_cb_alternate_saved_x(context).map(FormatValue::String),
        b"alternate_saved_y" => format_cb_alternate_saved_y(context).map(FormatValue::String),
        b"bracket_paste_flag" => format_cb_bracket_paste_flag(context).map(FormatValue::String),
        b"cursor_flag" => format_cb_cursor_flag(context).map(FormatValue::String),
        b"cursor_shape" => format_cb_cursor_shape(context).map(FormatValue::String),
        b"cursor_very_visible" => format_cb_cursor_very_visible(context).map(FormatValue::String),
        b"cursor_x" => format_cb_cursor_x(context).map(FormatValue::String),
        b"cursor_y" => format_cb_cursor_y(context).map(FormatValue::String),
        b"cursor_blinking" => format_cb_cursor_blinking(context).map(FormatValue::String),
        b"history_added" => format_cb_history_added(context).map(FormatValue::String),
        b"history_collected" => format_cb_history_collected(context).map(FormatValue::String),
        b"history_generation" => format_cb_history_generation(context).map(FormatValue::String),
        b"history_limit" => format_cb_history_limit(context).map(FormatValue::String),
        b"history_size" => format_cb_history_size(context).map(FormatValue::String),
        b"insert_flag" => format_cb_insert_flag(context).map(FormatValue::String),
        b"keypad_cursor_flag" => format_cb_keypad_cursor_flag(context).map(FormatValue::String),
        b"keypad_flag" => format_cb_keypad_flag(context).map(FormatValue::String),
        b"mouse_all_flag" => format_cb_mouse_all_flag(context).map(FormatValue::String),
        b"mouse_any_flag" => format_cb_mouse_any_flag(context).map(FormatValue::String),
        b"mouse_button_flag" => format_cb_mouse_button_flag(context).map(FormatValue::String),
        b"mouse_sgr_flag" => format_cb_mouse_sgr_flag(context).map(FormatValue::String),
        b"mouse_standard_flag" => format_cb_mouse_standard_flag(context).map(FormatValue::String),
        b"mouse_utf8_flag" => format_cb_mouse_utf8_flag(context).map(FormatValue::String),
        b"origin_flag" => format_cb_origin_flag(context).map(FormatValue::String),
        b"synchronized_output_flag" => {
            format_cb_synchronized_output_flag(context).map(FormatValue::String)
        }
        b"pane_private_modes" => format_cb_pane_private_modes(context).map(FormatValue::String),
        b"pane_active" => format_cb_pane_active(context).map(FormatValue::String),
        b"pane_at_left" => format_cb_pane_at_left(context).map(FormatValue::String),
        b"pane_at_right" => format_cb_pane_at_right(context).map(FormatValue::String),
        b"pane_bottom" => format_cb_pane_bottom(context).map(FormatValue::String),
        b"pane_dead" => format_cb_pane_dead(context).map(FormatValue::String),
        b"pane_dead_signal" => format_cb_pane_dead_signal(context).map(FormatValue::String),
        b"pane_dead_status" => format_cb_pane_dead_status(context).map(FormatValue::String),
        b"pane_dead_time" => format_cb_pane_dead_time(context).map(FormatValue::Time),
        b"pane_last_output_time" => format_cb_pane_last_output_time(context).map(FormatValue::Time),
        b"pane_output_generation" => {
            format_cb_pane_output_generation(context).map(FormatValue::String)
        }
        b"pane_last_prompt_time" => format_cb_pane_last_prompt_time(context).map(FormatValue::Time),
        b"pane_command_start_time" => {
            format_cb_pane_command_start_time(context).map(FormatValue::Time)
        }
        b"pane_command_end_time" => format_cb_pane_command_end_time(context).map(FormatValue::Time),
        b"pane_command_running" => format_cb_pane_command_running(context).map(FormatValue::String),
        b"pane_command_duration" => {
            format_cb_pane_command_duration(context).map(FormatValue::String)
        }
        b"pane_command_status" => format_cb_pane_command_status(context).map(FormatValue::String),
        b"pane_height" => format_cb_pane_height(context).map(FormatValue::String),
        b"pane_id" => format_cb_pane_id(context).map(FormatValue::String),
        b"pane_index" => format_cb_pane_index(context).map(FormatValue::String),
        b"pane_input_off" => format_cb_pane_input_off(context).map(FormatValue::String),
        b"pane_unseen_changes" => format_cb_pane_unseen_changes(context).map(FormatValue::String),
        b"pane_key_mode" => format_cb_pane_key_mode(context).map(FormatValue::String),
        b"pane_last" => format_cb_pane_last(context).map(FormatValue::String),
        b"pane_left" => format_cb_pane_left(context).map(FormatValue::String),
        b"pane_marked" => format_cb_pane_marked(context).map(FormatValue::String),
        b"pane_marked_set" => format_cb_pane_marked_set(context).map(FormatValue::String),
        b"pane_mode" => format_cb_pane_mode(context).map(FormatValue::String),
        b"pane_path" => format_cb_pane_path(context).map(FormatValue::String),
        b"pane_pid" => format_cb_pane_pid(context).map(FormatValue::String),
        b"pane_pipe" => format_cb_pane_pipe(context).map(FormatValue::String),
        b"pane_pipe_pid" => format_cb_pane_pipe_pid(context).map(FormatValue::String),
        b"pane_pb_progress" => format_cb_pane_pb_progress(context).map(FormatValue::String),
        b"pane_pb_state" => format_cb_pane_pb_state(context).map(FormatValue::String),
        b"pane_right" => format_cb_pane_right(context).map(FormatValue::String),
        b"pane_search_string" => format_cb_pane_search_string(context).map(FormatValue::String),
        b"pane_synchronized" => format_cb_pane_synchronized(context).map(FormatValue::String),
        b"pane_title" => format_cb_pane_title(context).map(FormatValue::String),
        b"pane_top" => format_cb_pane_top(context).map(FormatValue::String),
        b"pane_tty" => format_cb_pane_tty(context).map(FormatValue::String),
        b"pane_width" => format_cb_pane_width(context).map(FormatValue::String),
        b"pane_x" => format_cb_pane_x(context).map(FormatValue::String),
        b"pane_y" => format_cb_pane_y(context).map(FormatValue::String),
        b"scroll_region_lower" => format_cb_scroll_region_lower(context).map(FormatValue::String),
        b"scroll_region_upper" => format_cb_scroll_region_upper(context).map(FormatValue::String),
        b"wrap_flag" => format_cb_wrap_flag(context).map(FormatValue::String),
        _ => None,
    }
}

unsafe fn format_cb_start_command(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if wp.is_null() {
        return None;
    }
    cmd_stringify_argv_cstring(&(*wp).argv)
}

unsafe fn format_cb_start_command_list(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if wp.is_null() {
        return None;
    }
    if (*wp).argv.is_empty() {
        return Some(c"".to_owned());
    }
    let mut command = Vec::<u8>::new();
    for (i, arg) in (*wp).argv.iter().enumerate() {
        let quoted = format_quote_shell_single(arg.as_c_str());
        if i != 0 {
            command.push(b' ');
        }
        command.extend_from_slice(quoted.as_bytes());
    }
    Some(CString::new(command).expect("callback bytes contain no NUL"))
}

unsafe fn format_cb_start_path(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if wp.is_null() {
        return None;
    }
    if (*wp).cwd.is_none() {
        return Some(c"".to_owned());
    }
    (*wp).cwd.clone()
}

unsafe fn format_cb_current_command(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if wp.is_null() || (*wp).shell.is_none() {
        return None;
    }
    if let Some(cmd) = osdep_get_name_cstring((*wp).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd)) {
        let value = parse_window_name_cstring(cmd.as_c_str());
        return Some(value);
    }
    let argv = cmd_stringify_argv_cstring(&(*wp).argv);
    let source = argv
        .as_ref()
        .filter(|text| !text.as_bytes().is_empty())
        .map_or((*wp).shell.as_deref().unwrap(), |text| text.as_c_str());
    let value = parse_window_name_cstring(source);
    Some(value)
}

unsafe fn format_cb_current_path(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if wp.is_null() {
        return None;
    }
    osdep_get_cwd((*wp).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd))
}

unsafe fn format_cb_history_bytes(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    let mut gd: *mut grid = ::core::ptr::null_mut::<grid>();
    let mut size: size_t = 0 as size_t;
    let mut i: u_int = 0;
    let mut value = None;
    if wp.is_null() {
        return None;
    }
    gd = (*wp).base.grid_mut();
    i = 0 as u_int;
    while i < (*gd).hsize.wrapping_add((*gd).sy) {
        let gl = grid_get_line(&*gd, i);
        size = (size as ::core::ffi::c_ulong).wrapping_add(
            (gl.cellsize as usize).wrapping_mul(::core::mem::size_of::<grid_cell_entry>() as usize)
                as ::core::ffi::c_ulong,
        ) as size_t as size_t;
        size = (size as ::core::ffi::c_ulong).wrapping_add(
            (gl.extdsize as usize).wrapping_mul(::core::mem::size_of::<grid_extd_entry>() as usize)
                as ::core::ffi::c_ulong,
        ) as size_t as size_t;
        i = i.wrapping_add(1);
    }
    size = (size as ::core::ffi::c_ulong).wrapping_add(
        ((*gd).hsize.wrapping_add((*gd).sy) as usize)
            .wrapping_mul(::core::mem::size_of::<grid_line>() as usize)
            as ::core::ffi::c_ulong,
    ) as size_t as size_t;
    value = Some(
        CString::new(format!("{}", (size) as usize)).expect("formatted numbers contain no NUL"),
    );
    value
}

unsafe fn format_cb_history_all_bytes(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    let mut gd: *mut grid = ::core::ptr::null_mut::<grid>();
    let mut i: u_int = 0;
    let mut lines: u_int = 0;
    let mut cells: u_int = 0 as u_int;
    let mut extended_cells: u_int = 0 as u_int;
    let mut value = None;
    if wp.is_null() {
        return None;
    }
    gd = (*wp).base.grid_mut();
    lines = (*gd).hsize.wrapping_add((*gd).sy);
    i = 0 as u_int;
    while i < lines {
        let gl = grid_get_line(&*gd, i);
        cells = cells.wrapping_add(gl.cellsize as u_int);
        extended_cells = extended_cells.wrapping_add(gl.extdsize);
        i = i.wrapping_add(1);
    }
    value = Some(
        CString::new(format!(
            "{},{},{},{},{},{}",
            (lines) as u32,
            ((lines as usize).wrapping_mul(::core::mem::size_of::<grid_line>() as usize)) as usize,
            (cells) as u32,
            ((cells as usize).wrapping_mul(::core::mem::size_of::<grid_cell_entry>() as usize))
                as usize,
            (extended_cells) as u32,
            ((extended_cells as usize)
                .wrapping_mul(::core::mem::size_of::<grid_extd_entry>() as usize))
                as usize
        ))
        .expect("formatted numbers contain no NUL"),
    );
    value
}

unsafe fn format_cb_pane_tabs(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    let mut i: u_int = 0;
    if wp.is_null() {
        return None;
    }
    let mut tabs = String::new();
    i = 0 as u_int;
    while i < (*wp).base.grid().sx {
        if crate::src::screen::screen_has_tab(&(*wp).base, i) {
            if !tabs.is_empty() {
                tabs.push(',');
            }
            write!(&mut tabs, "{i}").expect("writing to a String cannot fail");
        }
        i = i.wrapping_add(1);
    }
    if tabs.is_empty() {
        return None;
    }
    Some(CString::new(tabs).expect("callback bytes contain no NUL"))
}

unsafe fn format_cb_pane_fg(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
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
    if wp.is_null() {
        return None;
    }
    gc = tty_default_colours(format_pane_owner.as_ref().expect("live pane")).0;
    Some(colour_format(gc.fg))
}

unsafe fn format_cb_pane_flags(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(window_pane_printable_flags(
            format_pane_owner.as_ref().expect("live pane"),
        ));
    }
    None
}

unsafe fn format_cb_pane_bg(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
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
    if wp.is_null() {
        return None;
    }
    gc = tty_default_colours(format_pane_owner.as_ref().expect("live pane")).0;
    Some(colour_format(gc.bg))
}

unsafe fn format_cb_pane_in_mode(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let pane = &*format_pane_owner?.get();
    let count = pane.modes.len();
    Some(CString::new(format!("{count}")).expect("formatted numbers contain no NUL"))
}

unsafe fn format_cb_pane_at_top(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    let mut flag: ::core::ffi::c_int = 0;
    let mut value = None;
    if wp.is_null() {
        return None;
    }
    let top = i32::from(window_pane_get_pane_status(&*wp) == PANE_STATUS_TOP);
    flag = ((*wp).yoff == top) as ::core::ffi::c_int;
    value =
        Some(CString::new(format!("{}", (flag) as i32)).expect("formatted numbers contain no NUL"));
    value
}

unsafe fn format_cb_pane_at_bottom(ft: *mut format_tree) -> Option<CString> {
    let pane = (*ft).wp.upgrade()?;
    let window = pane.window_observer().upgrade().expect("pane window");
    let (_, height, _, y) = pane.geometry();
    let status = pane.border_status();
    let bottom = window.logical_size().1 as i32 - i32::from(status == PANE_STATUS_BOTTOM);
    Some(
        if y + height as i32 == bottom {
            c"1"
        } else {
            c"0"
        }
        .to_owned(),
    )
}

unsafe fn format_cb_cursor_character(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
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
    let mut value = None;
    if wp.is_null() {
        return None;
    }
    grid_view_get_cell((*wp).base.grid(), (*wp).base.cx, (*wp).base.cy, &mut gc);
    if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
        value = Some(
            CString::new(std::slice::from_raw_parts(
                (&raw mut gc.data.data as *mut u_char).cast::<u8>(),
                libc::strnlen(
                    (&raw mut gc.data.data as *mut u_char).cast(),
                    gc.data.size as ::core::ffi::c_int as usize,
                ),
            ))
            .expect("bounded character contains no NUL"),
        );
    }
    value
}

unsafe fn format_cb_cursor_colour(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if wp.is_null() || (*wp).screen_ptr().is_null() {
        return None;
    }
    if (*(*wp).screen_ptr()).ccolour != -(1 as ::core::ffi::c_int) {
        return Some(colour_format((*(*wp).screen_ptr()).ccolour));
    }
    Some(colour_format((*(*wp).screen_ptr()).default_ccolour))
}

unsafe fn format_cb_alternate_on(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.saved_grid.is_some() {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_alternate_saved_x(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.saved_cx) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_alternate_saved_y(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.saved_cy) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_bracket_paste_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() && !(*format_pane).screen_ptr().is_null() {
        if (*(*format_pane).screen_ptr()).mode & MODE_BRACKETPASTE != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_cursor_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_CURSOR != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_cursor_shape(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() && !(*format_pane).screen_ptr().is_null() {
        match (*(*format_pane).screen_ptr()).cstyle as ::core::ffi::c_uint {
            1 => {
                return Some(c"block".to_owned());
            }
            2 => {
                return Some(c"underline".to_owned());
            }
            3 => {
                return Some(c"bar".to_owned());
            }
            _ => {
                return Some(c"default".to_owned());
            }
        }
    }
    None
}

unsafe fn format_cb_cursor_very_visible(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() && !(*format_pane).screen_ptr().is_null() {
        if (*(*format_pane).screen_ptr()).mode & MODE_CURSOR_VERY_VISIBLE != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_cursor_x(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.cx) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_cursor_y(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.cy) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_cursor_blinking(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() && !(*format_pane).screen_ptr().is_null() {
        if (*(*format_pane).screen_ptr()).mode & MODE_CURSOR_BLINKING != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_history_added(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!(
                "{}",
                ((*format_pane).base.grid().scroll_added) as u32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_history_collected(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return Some(
            CString::new(format!("{}", ((*wp).base.grid().scroll_collected) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_history_generation(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return Some(
            CString::new(format!("{}", ((*wp).base.grid().scroll_generation) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_history_limit(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.grid().hlimit) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_history_size(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.grid().hsize) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_insert_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_INSERT != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_keypad_cursor_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_KCURSOR != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_keypad_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_KKEYPAD != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_mouse_all_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_MOUSE_ALL != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_mouse_any_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & ALL_MOUSE_MODES != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_mouse_button_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_MOUSE_BUTTON != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_mouse_sgr_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_MOUSE_SGR != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_mouse_standard_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_MOUSE_STANDARD != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_mouse_utf8_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_MOUSE_UTF8 != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_origin_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_ORIGIN != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_synchronized_output_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_SYNC != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_private_modes(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    static mut table: [C2RustUnnamed_43; 14] = [
        C2RustUnnamed_43 {
            mode: MODE_KCURSOR,
            number: 1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_ORIGIN,
            number: 6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_WRAP,
            number: 7 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_CURSOR_BLINKING,
            number: 12 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_CURSOR,
            number: 25 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_MOUSE_STANDARD,
            number: 1000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_MOUSE_BUTTON,
            number: 1002 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_MOUSE_ALL,
            number: 1003 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_FOCUSON,
            number: 1004 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_MOUSE_UTF8,
            number: 1005 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_MOUSE_SGR,
            number: 1006 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_BRACKETPASTE,
            number: 2004 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_SYNC,
            number: 2026 as ::core::ffi::c_int,
        },
        C2RustUnnamed_43 {
            mode: MODE_THEME_UPDATES,
            number: 2031 as ::core::ffi::c_int,
        },
    ];
    let mut mode: ::core::ffi::c_int = 0;
    let mut value = String::new();
    let mut i: u_int = 0;
    if format_pane.is_null() {
        return None;
    }
    mode = (*format_pane).base.mode;
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[C2RustUnnamed_43; 14]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_43>() as usize)
    {
        if !(!mode & table[i as usize].mode != 0)
            && !(table[i as usize].mode == MODE_CURSOR_BLINKING
                && !mode & MODE_CURSOR_BLINKING_SET != 0)
        {
            if !value.is_empty() {
                value.push(',');
            }
            write!(&mut value, "{}", table[i as usize].number)
                .expect("writing to a String cannot fail");
        }
        i = i.wrapping_add(1);
    }
    let value = std::ffi::CString::new(value).expect("mode numbers have no NUL bytes");
    Some(value)
}

unsafe fn format_cb_pane_active(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if format_pane
            == (((*format_pane).window_handle().as_ref()).expect("live window"))
                .active_pane()
                .upgrade()
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get())
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_at_left(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).xoff == 0 as ::core::ffi::c_int {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_at_right(ft: *mut format_tree) -> Option<CString> {
    let pane = (*ft).wp.upgrade()?;
    Some(
        if pane
            .window_observer()
            .upgrade()
            .is_none_or(|window| window.step_pane(Some(&(*ft).wp), false).upgrade().is_none())
        {
            c"1"
        } else {
            c"0"
        }
        .to_owned(),
    )
}

unsafe fn format_cb_pane_bottom(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return Some(
            CString::new(format!(
                "{}",
                ((*wp).yoff + (*wp).sy as ::core::ffi::c_int - 1 as ::core::ffi::c_int) as i32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_dead(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        if (*wp).fd.is_none() && (*wp).exit_status.is_some() {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_dead_signal(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return (*wp)
            .exit_status
            .and_then(|status| status.signal())
            .map(sig2name);
    }
    None
}

unsafe fn format_cb_pane_dead_status(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return (*wp)
            .exit_status
            .and_then(|status| status.code())
            .map(|code| {
                CString::new(format!("{}", code)).expect("formatted numbers contain no NUL")
            });
    }
    None
}

unsafe fn format_cb_pane_dead_time(mut ft: *mut format_tree) -> Option<time_t> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        if (*wp).flags & PANE_STATUSDRAWN != 0 {
            return Some(crate::src::shared::time::unix_seconds((*wp).dead_time));
        }
        return None;
    }
    None
}

unsafe fn format_cb_pane_last_output_time(mut ft: *mut format_tree) -> Option<time_t> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() && (*wp).last_output_time != 0 as time_t {
        return Some((*wp).last_output_time as __time_t as time_t);
    }
    None
}

unsafe fn format_cb_pane_output_generation(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut value: ::core::ffi::c_ulonglong = 0;
    if !format_pane.is_null() {
        value = (*format_pane).output_generation as ::core::ffi::c_ulonglong;
        return Some(
            CString::new(format!("{}", (value) as u64)).expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_last_prompt_time(mut ft: *mut format_tree) -> Option<time_t> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() && (*wp).last_prompt_time != 0 as time_t {
        return Some((*wp).last_prompt_time as __time_t as time_t);
    }
    None
}

unsafe fn format_cb_pane_command_start_time(mut ft: *mut format_tree) -> Option<time_t> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() && (*wp).cmd_start_time != 0 as time_t {
        return Some((*wp).cmd_start_time as __time_t as time_t);
    }
    None
}

unsafe fn format_cb_pane_command_end_time(mut ft: *mut format_tree) -> Option<time_t> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() && (*wp).cmd_end_time != 0 as time_t {
        return Some((*wp).cmd_end_time as __time_t as time_t);
    }
    None
}

unsafe fn format_cb_pane_command_running(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return Some(
            CString::new(format!(
                "{}",
                (((*wp).flags & PANE_CMDRUNNING != 0) as ::core::ffi::c_int) as i32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_command_duration(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    let mut end: time_t = 0;
    if wp.is_null() || (*wp).cmd_start_time == 0 as time_t {
        return None;
    }
    if (*wp).flags & PANE_CMDRUNNING != 0 {
        end = time(::core::ptr::null_mut::<time_t>());
    } else {
        end = (*wp).cmd_end_time;
    }
    if end < (*wp).cmd_start_time {
        end = (*wp).cmd_start_time;
    }
    Some(
        CString::new(format!(
            "{}",
            ((end - (*wp).cmd_start_time) as ::core::ffi::c_longlong) as i64
        ))
        .expect("formatted numbers contain no NUL"),
    )
}

unsafe fn format_cb_pane_command_status(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() && (*wp).cmd_status != -(1 as ::core::ffi::c_int) {
        return Some(
            CString::new(format!("{}", ((*wp).cmd_status) as i32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_height(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).sy) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_id(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("%{}", ((*format_pane).id) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_index(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut idx: u_int = 0;
    if !format_pane.is_null()
        && window_pane_index(format_pane_owner.as_ref().expect("live pane"))
            .map(|value| {
                idx = value;
            })
            .is_some()
    {
        return Some(
            CString::new(format!("{}", (idx) as u32)).expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_input_off(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).flags & PANE_INPUTOFF != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_unseen_changes(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).flags & PANE_UNSEENCHANGES != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_key_mode(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() && !(*format_pane).screen_ptr().is_null() {
        match (*(*format_pane).screen_ptr()).mode & EXTENDED_KEY_MODES {
            MODE_KEYS_EXTENDED => {
                return Some(c"Ext 1".to_owned());
            }
            MODE_KEYS_EXTENDED_2 => {
                return Some(c"Ext 2".to_owned());
            }
            _ => {
                return Some(c"VT10x".to_owned());
            }
        }
    }
    None
}

unsafe fn format_cb_pane_last(ft: *mut format_tree) -> Option<CString> {
    let pane = (*ft).wp.upgrade()?;
    let Some(window) = pane.window_observer().upgrade() else {
        return Some(c"0".to_owned());
    };
    let is_last = window.last_active_pane().ptr_eq(&Rc::downgrade(&pane));
    let value = if is_last { c"1" } else { c"0" }.to_owned();
    drop(window);
    Some(value)
}

unsafe fn format_cb_pane_left(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).xoff) as i32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_marked(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if server_check_marked() != 0
            && marked_pane
                .pane_handle()
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get())
                == format_pane
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_marked_set(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if server_check_marked() != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_mode(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let pane = &*format_pane_owner?.get();
    pane.active_mode().map(|mode| mode.name.to_owned())
}

unsafe fn format_cb_pane_path(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            (*format_pane)
                .base
                .path
                .clone()
                .unwrap_or_else(|| c"".to_owned()),
        );
    }
    None
}

unsafe fn format_cb_pane_pid(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() && (*format_pane).fd.is_some() {
        return Some(
            CString::new(format!(
                "{}",
                ((*format_pane).pid as ::core::ffi::c_long) as ::core::ffi::c_long
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_pipe(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).pipe_fd.is_some() {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_pipe_pid(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut value = None;
    if !format_pane.is_null() && (*format_pane).pipe_fd.is_some() {
        value = Some(
            CString::new(format!(
                "{}",
                ((*format_pane).pipe_pid as ::core::ffi::c_long) as ::core::ffi::c_long
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    value
}

unsafe fn format_cb_pane_pb_progress(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut value = None;
    if !format_pane.is_null() {
        value = Some(
            CString::new(format!(
                "{}",
                ((*format_pane).base.progress_bar.progress) as i32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    value
}

unsafe fn format_cb_pane_pb_state(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        match (*format_pane).base.progress_bar.state as ::core::ffi::c_uint {
            0 => {
                return Some(c"hidden".to_owned());
            }
            1 => {
                return Some(c"normal".to_owned());
            }
            2 => {
                return Some(c"error".to_owned());
            }
            3 => {
                return Some(c"indeterminate".to_owned());
            }
            4 => {
                return Some(c"paused".to_owned());
            }
            _ => {}
        }
    }
    None
}

unsafe fn format_cb_pane_right(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wp: *mut window_pane = format_pane;
    if !wp.is_null() {
        return Some(
            CString::new(format!(
                "{}",
                ((*wp).xoff + (*wp).sx as ::core::ffi::c_int - 1 as ::core::ffi::c_int) as i32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_search_string(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).searchstr.is_none() {
            return Some(c"".to_owned());
        }
        return (*format_pane).searchstr.clone();
    }
    None
}

unsafe fn format_cb_pane_synchronized(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if options_get_number(&(*format_pane).borrow_options_mut(), c"synchronize-panes") != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_pane_title(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some((*format_pane).base.title.clone());
    }
    None
}

unsafe fn format_cb_pane_top(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).yoff) as i32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_tty(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some((*format_pane).tty().to_owned());
    }
    None
}

unsafe fn format_cb_pane_width(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).sx) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_x(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).xoff) as i32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_pane_y(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).yoff) as i32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_scroll_region_lower(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.rlower) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_scroll_region_upper(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_pane).base.rupper) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_wrap_flag(mut ft: *mut format_tree) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_pane = format_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_pane.is_null() {
        if (*format_pane).base.mode & MODE_WRAP != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn mouse_value(
    owner: &Rc<UnsafeCell<window_pane>>,
    key: &CStr,
    context: &mut format_tree,
) -> Option<CString> {
    let (x, y) = owner.mouse_position(&context.m, false)?;
    let has_modes = !(*owner.get()).modes.is_empty();
    if has_modes {
        if window_pane_mode(&*owner.get()) == WINDOW_PANE_NO_MODE {
            return None;
        }
        return match key.to_bytes() {
            b"mouse_word" => crate::src::window_copy::window_copy_get_word_cstring(owner, x, y),
            b"mouse_hyperlink" => {
                crate::src::window_copy::window_copy_get_hyperlink_cstring(owner, x, y)
            }
            b"mouse_line" => crate::src::window_copy::window_copy_get_line_cstring(owner, y),
            _ => None,
        };
    }
    let pane = &*owner.get();
    let grid = pane.base.grid();
    let row = grid.hsize.wrapping_add(y);
    match key.to_bytes() {
        b"mouse_word" => crate::src::format::format_grid_word_cstring(grid, x, row),
        b"mouse_hyperlink" => {
            crate::src::format::format_grid_hyperlink_cstring(grid, x, row, &*pane.screen_ptr())
        }
        b"mouse_line" => crate::src::format::format_grid_line_cstring(grid, row),
        _ => None,
    }
}
