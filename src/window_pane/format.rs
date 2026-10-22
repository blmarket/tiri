//! Pane-owned lazy formatting. Values leave the pane as owned bytes or timestamps.
use super::*;
use crate::cmd::cmd_stringify_argv_cstring;
use crate::format::format_quote_shell_single;
use crate::format::C2RustUnnamed_43;
use crate::format::{format_tree, FormatValue, FORMAT_TYPE_PANE};
use crate::grid::grid_get_line;
use crate::grid::view::grid_view_get_cell;
use crate::names::parse_window_name_cstring;
use crate::osdep::{osdep_get_cwd, osdep_get_name_cstring};
use crate::shared::pane::{PANE_CMDRUNNING, PANE_STATUSDRAWN};
use crate::shared::screen::*;
use crate::style::colour::colour_format;
use crate::tmux::sig2name;
use std::fmt::Write as _;
use std::os::unix::process::ExitStatusExt;

pub(super) fn format_value(
    owner: &Rc<RefCell<window_pane>>,
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
pub(crate) fn format_without_pane(key: &CStr, context: &mut format_tree) -> Option<FormatValue> {
    assert!(context.wp.upgrade().is_none(), "absent pane format context");
    evaluate(key, context)
}

fn evaluate(key: &CStr, context: &mut format_tree) -> Option<FormatValue> {
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
        b"pane_unzoomed_height" => {
            format_cb_pane_unzoomed_size(context, false).map(FormatValue::String)
        }
        b"pane_unzoomed_width" => {
            format_cb_pane_unzoomed_size(context, true).map(FormatValue::String)
        }
        b"pane_width" => format_cb_pane_width(context).map(FormatValue::String),
        b"pane_x" => format_cb_pane_x(context).map(FormatValue::String),
        b"pane_y" => format_cb_pane_y(context).map(FormatValue::String),
        b"pane_zoomed_flag" => format_cb_pane_zoomed_flag(context).map(FormatValue::String),
        b"scroll_region_lower" => format_cb_scroll_region_lower(context).map(FormatValue::String),
        b"scroll_region_upper" => format_cb_scroll_region_upper(context).map(FormatValue::String),
        b"wrap_flag" => format_cb_wrap_flag(context).map(FormatValue::String),
        _ => None,
    }
}

fn format_cb_start_command(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    cmd_stringify_argv_cstring(&wp.argv)
}

fn format_cb_start_command_list(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.argv.is_empty() {
        return Some(c"".to_owned());
    }
    let mut command = Vec::<u8>::new();
    for (i, arg) in wp.argv.iter().enumerate() {
        let quoted = format_quote_shell_single(arg.as_c_str());
        if i != 0 {
            command.push(b' ');
        }
        command.extend_from_slice(quoted.as_bytes());
    }
    Some(CString::new(command).expect("callback bytes contain no NUL"))
}

fn format_cb_start_path(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.cwd.is_none() {
        return Some(c"".to_owned());
    }
    wp.cwd.clone()
}

fn format_cb_current_command(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    wp.shell.as_ref()?;
    if let Some(cmd) = wp
        .fd
        .as_ref()
        .and_then(|fd| osdep_get_name_cstring(std::os::fd::AsFd::as_fd(fd)))
    {
        let value = parse_window_name_cstring(cmd.as_c_str());
        return Some(value);
    }
    let argv = cmd_stringify_argv_cstring(&wp.argv);
    let source = argv
        .as_ref()
        .filter(|text| !text.as_bytes().is_empty())
        .map_or(wp.shell.as_deref().unwrap(), |text| text.as_c_str());
    let value = parse_window_name_cstring(source);
    Some(value)
}

fn format_cb_current_path(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    osdep_get_cwd(std::os::fd::AsFd::as_fd(wp.fd.as_ref()?))
}

fn format_cb_history_bytes(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    let mut size: size_t = 0 as size_t;
    let mut i: u_int = 0;
    let mut value = None;
    let gd = wp.base.grid();
    i = 0 as u_int;
    while i < gd.hsize.wrapping_add(gd.sy) {
        let gl = grid_get_line(gd, i);
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
        (gd.hsize.wrapping_add(gd.sy) as usize)
            .wrapping_mul(::core::mem::size_of::<grid_line>() as usize)
            as ::core::ffi::c_ulong,
    ) as size_t as size_t;
    value = Some(
        CString::new(format!("{}", (size) as usize)).expect("formatted numbers contain no NUL"),
    );
    value
}

fn format_cb_history_all_bytes(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    let mut i: u_int = 0;
    let mut lines: u_int = 0;
    let mut cells: u_int = 0 as u_int;
    let mut extended_cells: u_int = 0 as u_int;
    let mut value = None;
    let gd = wp.base.grid();
    lines = gd.hsize.wrapping_add(gd.sy);
    i = 0 as u_int;
    while i < lines {
        let gl = grid_get_line(gd, i);
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

fn format_cb_pane_tabs(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    let mut i: u_int = 0;
    let mut tabs = String::new();
    i = 0 as u_int;
    while i < wp.base.grid().sx {
        if crate::screen::screen_has_tab(&wp.base, i) {
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

fn format_cb_pane_fg(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    Some(colour_format(pane.default_colours().0.fg))
}

fn format_cb_pane_flags(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    Some(window_pane_printable_flags(&pane))
}

fn format_cb_pane_bg(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    Some(colour_format(pane.default_colours().0.bg))
}

fn format_cb_pane_in_mode(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.wp.upgrade()?;
    let pane = owner.borrow();
    let count = pane.modes.len();
    Some(CString::new(format!("{count}")).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_at_top(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let top = i32::from(window_pane_get_pane_status(&pane) == PANE_STATUS_TOP);
    let flag = i32::from(pane.borrow().yoff == top);
    Some(CString::new(format!("{flag}")).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_at_bottom(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
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

fn format_cb_cursor_character(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
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
    grid_view_get_cell(wp.base.grid(), wp.base.cx, wp.base.cy, &mut gc);
    if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
        let data = &gc.data.data[..gc.data.size as usize];
        let len = data
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(data.len());
        value = Some(CString::new(&data[..len]).expect("bounded character contains no NUL"));
    }
    value
}

/// Display state of the format pane's displayed screen, copied out of its owner.
fn format_cb_cursor_colour(ft: &mut format_tree) -> Option<CString> {
    let screen = ft.wp.upgrade()?.screen_mode(true)?;
    if screen.ccolour != -(1 as ::core::ffi::c_int) {
        return Some(colour_format(screen.ccolour));
    }
    Some(colour_format(screen.default_ccolour))
}

fn format_cb_alternate_on(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.saved_grid.is_some() {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_alternate_saved_x(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { wp.base.saved_cx }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_alternate_saved_y(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.saved_cy) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_bracket_paste_flag(ft: &mut format_tree) -> Option<CString> {
    let screen = ft.wp.upgrade()?.screen_mode(true)?;
    if screen.mode & MODE_BRACKETPASTE != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_cursor_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_CURSOR != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_cursor_shape(ft: &mut format_tree) -> Option<CString> {
    let screen = ft.wp.upgrade()?.screen_mode(true)?;
    match screen.cstyle as ::core::ffi::c_uint {
        1 => Some(c"block".to_owned()),
        2 => Some(c"underline".to_owned()),
        3 => Some(c"bar".to_owned()),
        _ => Some(c"default".to_owned()),
    }
}

fn format_cb_cursor_very_visible(ft: &mut format_tree) -> Option<CString> {
    let screen = ft.wp.upgrade()?.screen_mode(true)?;
    if screen.mode & MODE_CURSOR_VERY_VISIBLE != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_cursor_x(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.base.cx) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_cursor_y(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.base.cy) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_cursor_blinking(ft: &mut format_tree) -> Option<CString> {
    let screen = ft.wp.upgrade()?.screen_mode(true)?;
    if screen.mode & MODE_CURSOR_BLINKING != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_history_added(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.grid().scroll_added) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_history_collected(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.grid().scroll_collected) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_history_generation(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.grid().scroll_generation) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_history_limit(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.grid().hlimit) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_history_size(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.grid().hsize) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_insert_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_INSERT != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_keypad_cursor_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_KCURSOR != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_keypad_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_KKEYPAD != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_mouse_all_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_MOUSE_ALL != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_mouse_any_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & ALL_MOUSE_MODES != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_mouse_button_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_MOUSE_BUTTON != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_mouse_sgr_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_MOUSE_SGR != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_mouse_standard_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_MOUSE_STANDARD != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_mouse_utf8_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_MOUSE_UTF8 != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_origin_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_ORIGIN != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_synchronized_output_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_SYNC != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_pane_private_modes(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    static table: [C2RustUnnamed_43; 14] = [
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
    mode = wp.base.mode;
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

fn format_cb_pane_active(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let window = pane.window_observer().upgrade().expect("live window");
    let active = window.active_pane().ptr_eq(&ft.wp);
    Some(if active { c"1" } else { c"0" }.to_owned())
}

fn format_cb_pane_at_left(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.xoff == 0 as ::core::ffi::c_int {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_pane_at_right(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    Some(
        if pane.window_observer().upgrade().is_none_or(|window| {
            window.zoomed_pane().ptr_eq(&ft.wp)
                || window.step_pane(Some(&ft.wp), false).upgrade().is_none()
        }) {
            c"1"
        } else {
            c"0"
        }
        .to_owned(),
    )
}

fn format_cb_pane_bottom(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", {
            (wp.yoff + wp.sy as ::core::ffi::c_int - 1 as ::core::ffi::c_int)
        }))
        .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_pane_dead(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.fd.is_none() && wp.exit_status.is_some() {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_pane_dead_signal(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    wp.exit_status
        .and_then(|status| status.signal())
        .map(sig2name)
}

fn format_cb_pane_dead_status(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    wp.exit_status
        .and_then(|status| status.code())
        .map(|code| CString::new(format!("{}", code)).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_dead_time(ft: &mut format_tree) -> Option<time_t> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.flags & PANE_STATUSDRAWN != 0 {
        return Some(crate::shared::time::unix_seconds(wp.dead_time));
    }
    None
}

fn format_cb_pane_last_output_time(ft: &mut format_tree) -> Option<time_t> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.last_output_time != 0 as time_t {
        return Some(wp.last_output_time as __time_t as time_t);
    }
    None
}

fn format_cb_pane_output_generation(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    let mut value: ::core::ffi::c_ulonglong = 0;
    value = wp.output_generation as ::core::ffi::c_ulonglong;
    Some(CString::new(format!("{}", (value) as u64)).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_last_prompt_time(ft: &mut format_tree) -> Option<time_t> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.last_prompt_time != 0 as time_t {
        return Some(wp.last_prompt_time as __time_t as time_t);
    }
    None
}

fn format_cb_pane_command_start_time(ft: &mut format_tree) -> Option<time_t> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.cmd_start_time != 0 as time_t {
        return Some(wp.cmd_start_time as __time_t as time_t);
    }
    None
}

fn format_cb_pane_command_end_time(ft: &mut format_tree) -> Option<time_t> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.cmd_end_time != 0 as time_t {
        return Some(wp.cmd_end_time as __time_t as time_t);
    }
    None
}

fn format_cb_pane_command_running(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", {
            ((wp.flags & PANE_CMDRUNNING != 0) as ::core::ffi::c_int)
        }))
        .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_pane_command_duration(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    let mut end: time_t = 0;
    if wp.cmd_start_time == 0 as time_t {
        return None;
    }
    if wp.flags & PANE_CMDRUNNING != 0 {
        end = crate::shared::time::unix_seconds(std::time::SystemTime::now());
    } else {
        end = wp.cmd_end_time;
    }
    if end < wp.cmd_start_time {
        end = wp.cmd_start_time;
    }
    Some(
        CString::new(format!(
            "{}",
            ((end - wp.cmd_start_time) as ::core::ffi::c_longlong) as i64
        ))
        .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_pane_command_status(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.cmd_status != -(1 as ::core::ffi::c_int) {
        return Some(
            CString::new(format!("{}", { (wp.cmd_status) }))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

fn format_cb_pane_height(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.sy) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_id(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("%{}", { (wp.id) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_index(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let idx = window_pane_index(&pane)?;
    Some(CString::new(format!("{idx}")).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_input_off(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.flags & PANE_INPUTOFF != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_pane_unseen_changes(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.flags & PANE_UNSEENCHANGES != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_pane_key_mode(ft: &mut format_tree) -> Option<CString> {
    let screen = ft.wp.upgrade()?.screen_mode(true)?;
    match screen.mode & EXTENDED_KEY_MODES {
        MODE_KEYS_EXTENDED => Some(c"Ext 1".to_owned()),
        MODE_KEYS_EXTENDED_2 => Some(c"Ext 2".to_owned()),
        _ => Some(c"VT10x".to_owned()),
    }
}

fn format_cb_pane_last(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let Some(window) = pane.window_observer().upgrade() else {
        return Some(c"0".to_owned());
    };
    let is_last = window.last_active_pane().ptr_eq(&Rc::downgrade(&pane));
    let value = if is_last { c"1" } else { c"0" }.to_owned();
    drop(window);
    Some(value)
}

fn format_cb_pane_left(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.xoff) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_marked(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let marked = server_check_marked() != 0
        && marked_pane
            .with_borrow(|marked| marked.pane_handle())
            .is_some_and(|owner| Rc::ptr_eq(&owner, &pane));
    Some(if marked { c"1" } else { c"0" }.to_owned())
}

fn format_cb_pane_marked_set(ft: &mut format_tree) -> Option<CString> {
    ft.wp.upgrade()?;
    Some(
        if server_check_marked() != 0 {
            c"1"
        } else {
            c"0"
        }
        .to_owned(),
    )
}

fn format_cb_pane_mode(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.wp.upgrade()?;
    let pane = owner.borrow();
    pane.active_mode().map(|mode| mode.name.to_owned())
}

fn format_cb_pane_path(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(wp.base.path.clone().unwrap_or_else(|| c"".to_owned()))
}

fn format_cb_pane_pid(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.fd.is_some() {
        return Some(
            CString::new(format!(
                "{}",
                (wp.pid as ::core::ffi::c_long) as ::core::ffi::c_long
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

fn format_cb_pane_pipe(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.pipe_fd.is_some() {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_pane_pipe_pid(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    let mut value = None;
    if wp.pipe_fd.is_some() {
        value = Some(
            CString::new(format!(
                "{}",
                (wp.pipe_pid as ::core::ffi::c_long) as ::core::ffi::c_long
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    value
}

fn format_cb_pane_pb_progress(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let progress = pane.borrow().base.progress_bar.progress;
    Some(CString::new(format!("{}", { progress })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_pb_state(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    match wp.base.progress_bar.state as ::core::ffi::c_uint {
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
    None
}

fn format_cb_pane_right(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", {
            (wp.xoff + wp.sx as ::core::ffi::c_int - 1 as ::core::ffi::c_int)
        }))
        .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_pane_search_string(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.searchstr.is_none() {
        return Some(c"".to_owned());
    }
    wp.searchstr.clone()
}

fn format_cb_pane_synchronized(ft: &mut format_tree) -> Option<CString> {
    let format_pane_owner = ft.wp.upgrade();
    if let Some(format_pane) = format_pane_owner.as_ref() {
        if options_get_number(&format_pane.borrow_options_mut(), c"synchronize-panes") != 0 {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

fn format_cb_pane_title(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(wp.base.title.clone())
}

fn format_cb_pane_top(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.yoff) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_tty(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(wp.tty().to_owned())
}

/// The pane's width, or with `width` false its height, in its place in the
/// layout with the window unzoomed.
fn format_cb_pane_unzoomed_size(ft: &mut format_tree, width: bool) -> Option<CString> {
    let (sx, sy) = ft.wp.upgrade()?.unzoomed_size()?;
    Some(
        CString::new(format!("{}", if width { sx } else { sy }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_pane_zoomed_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let window = pane.window_observer().upgrade()?;
    let zoomed = window.zoomed_pane().ptr_eq(&Rc::downgrade(&pane));
    Some(if zoomed { c"1" } else { c"0" }.to_owned())
}

fn format_cb_pane_width(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.sx) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_x(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.xoff) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_pane_y(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(CString::new(format!("{}", { (wp.yoff) })).expect("formatted numbers contain no NUL"))
}

fn format_cb_scroll_region_lower(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.rlower) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_scroll_region_upper(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    Some(
        CString::new(format!("{}", { (wp.base.rupper) }))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_wrap_flag(ft: &mut format_tree) -> Option<CString> {
    let pane = ft.wp.upgrade()?;
    let wp = pane.borrow();
    if wp.base.mode & MODE_WRAP != 0 {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn mouse_value(
    owner: &Rc<RefCell<window_pane>>,
    key: &CStr,
    context: &mut format_tree,
) -> Option<CString> {
    let (x, y) = owner.mouse_position(&context.m, false)?;
    let has_modes = !owner.borrow().modes.is_empty();
    if has_modes {
        if window_pane_mode(&owner.borrow()) == WINDOW_PANE_NO_MODE {
            return None;
        }
        return match key.to_bytes() {
            b"mouse_word" => crate::window_copy::window_copy_get_word_cstring(owner, x, y),
            b"mouse_hyperlink" => {
                crate::window_copy::window_copy_get_hyperlink_cstring(owner, x, y)
            }
            b"mouse_line" => crate::window_copy::window_copy_get_line_cstring(owner, y),
            _ => None,
        };
    }
    let pane = owner.borrow();
    let grid = pane.base.grid();
    let row = grid.hsize.wrapping_add(y);
    match key.to_bytes() {
        b"mouse_word" => crate::format::format_grid_word_cstring(grid, x, row),
        b"mouse_hyperlink" => pane
            .with_screen(|screen| {
                crate::format::format_grid_hyperlink_cstring(grid, x, row, screen)
            })
            .flatten(),
        b"mouse_line" => crate::format::format_grid_line_cstring(grid, row),
        _ => None,
    }
}
