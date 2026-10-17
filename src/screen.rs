use crate::src::grid::view::{grid_view_clear, grid_view_delete_lines};
use crate::src::grid::{
    grid_adjust_lines, grid_clear_lines, grid_create, grid_duplicate_lines, grid_empty_line,
    grid_reflow, grid_unwrap_position, grid_wrap_position,
};
use crate::src::hyperlinks::{hyperlinks_init, hyperlinks_reset};
use crate::src::log::{fatal, fatalx, log_debug};
use crate::src::options::options_get_number;
use crate::src::screen_write::{screen_write_free_list, screen_write_make_list};
use crate::src::shared::abi::*;
use crate::src::shared::display::*;
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::key::MODEKEY_EMACS;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::options::options;
pub use crate::src::shared::screen::{
    screen, screen_sel, ALL_MODES, EXTENDED_KEY_MODES, MODE_BRACKETPASTE, MODE_CRLF, MODE_CURSOR,
    MODE_CURSOR_BLINKING, MODE_CURSOR_BLINKING_SET, MODE_CURSOR_VERY_VISIBLE, MODE_FOCUSON,
    MODE_INSERT, MODE_KCURSOR, MODE_KEYS_EXTENDED, MODE_KEYS_EXTENDED_2, MODE_KKEYPAD,
    MODE_MOUSE_ALL, MODE_MOUSE_BUTTON, MODE_MOUSE_SGR, MODE_MOUSE_STANDARD, MODE_MOUSE_UTF8,
    MODE_ORIGIN, MODE_SYNC, MODE_THEME_UPDATES, MODE_WRAP,
};
use crate::src::style::style_apply;
use crate::src::text::utf8::utf8_copy;
use crate::src::tmux::{clean_name_cstring, global_options};
use std::collections::VecDeque;
use std::ffi::{CStr, CString};

fn screen_free_titles(s: &mut screen) {
    s.titles = VecDeque::new();
}
pub unsafe fn screen_init(s: &mut screen, mut sx: u_int, mut sy: u_int, mut hlimit: u_int) {
    s.grid = Some(grid_create(sx, sy, hlimit));
    s.saved_grid = None;
    s.title = CString::default();
    s.titles = VecDeque::new();
    s.path = None;
    s.cstyle = SCREEN_CURSOR_DEFAULT;
    s.default_cstyle = SCREEN_CURSOR_DEFAULT;
    s.mode = MODE_CURSOR;
    s.default_mode = 0 as ::core::ffi::c_int;
    s.ccolour = -(1 as ::core::ffi::c_int);
    s.default_ccolour = -(1 as ::core::ffi::c_int);
    s.tabs = Vec::new();
    s.sel = None;
    s.write_list = None;
    s.hyperlinks = None;
    screen_reinit(s);
}
pub unsafe fn screen_reinit(s: &mut screen) {
    s.cx = 0 as u_int;
    s.cy = 0 as u_int;
    s.rupper = 0 as u_int;
    s.rlower = s.grid().sy.wrapping_sub(1 as u_int);
    s.mode = MODE_CURSOR | MODE_WRAP | s.mode & MODE_CRLF;
    if options_get_number(global_options(), c"extended-keys") == 2 as ::core::ffi::c_longlong {
        s.mode = s.mode & !EXTENDED_KEY_MODES | MODE_KEYS_EXTENDED;
    }
    if s.saved_grid.is_some() {
        screen_alternate_off(s, None, 0);
    }
    s.saved_cx = UINT_MAX as u_int;
    s.saved_cy = UINT_MAX as u_int;
    screen_reset_tabs(s);
    let hsize = s.grid().hsize;
    let sy = s.grid().sy;
    grid_clear_lines(s.grid_mut(), hsize, sy, 8);
    screen_clear_selection(s);
    screen_free_titles(s);
    screen_set_progress_bar(s, PROGRESS_BAR_HIDDEN, 0 as ::core::ffi::c_int);
    screen_reset_hyperlinks(s);
}
pub fn screen_reset_hyperlinks(s: &mut screen) {
    if let Some(table) = s.hyperlinks.as_ref() {
        hyperlinks_reset(table);
    } else {
        s.hyperlinks = Some(hyperlinks_init());
    }
}
impl Drop for screen {
    fn drop(&mut self) {
        if self.write_list.is_some() {
            unsafe { screen_write_free_list(self) };
        }
    }
}
pub unsafe fn screen_reset_tabs(s: &mut screen) {
    let bytes = (s.grid().sx as usize).div_ceil(8);
    s.tabs.clear();
    if s.tabs.try_reserve_exact(bytes).is_err() {
        fatal(|out| out.write_all(b"bit_alloc failed"));
    }
    s.tabs.resize(bytes, 0);
    let mut i: u_int = 8;
    while i < s.grid().sx {
        let fresh0 = &mut s.tabs[(i >> 3) as usize];
        *fresh0 = (*fresh0 as ::core::ffi::c_int | (1 as ::core::ffi::c_int) << (i & 0x7 as u_int))
            as bitstr_t;
        i = i.wrapping_add(8 as u_int);
    }
}
pub(crate) fn screen_share_hyperlinks(dst: &mut screen, src: &screen) {
    dst.hyperlinks = src.hyperlinks.clone();
}
pub(crate) unsafe fn screen_has_tab(s: &screen, column: u_int) -> bool {
    *s.tabs.get_unchecked(column as usize / 8) & (1 << (column % 8)) != 0
}
pub(crate) unsafe fn screen_set_tab(s: &mut screen, column: u_int, set: bool) {
    let byte = s.tabs.get_unchecked_mut(column as usize / 8);
    let mask = 1 << (column % 8);
    if set {
        *byte |= mask;
    } else {
        *byte &= !mask;
    }
}
pub(crate) fn screen_clear_tabs(s: &mut screen) {
    s.tabs.fill(0);
}
pub unsafe fn screen_set_default_cursor(s: &mut screen, mut oo: *mut options) {
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
    let mut c: ::core::ffi::c_int = 0;
    style_apply(
        &raw mut gc,
        oo,
        c"cursor-colour",
        ::core::ptr::null_mut::<format_tree>(),
    );
    s.default_ccolour = gc.fg;
    c = options_get_number(&*oo, c"cursor-style") as ::core::ffi::c_int;
    s.default_mode = 0 as ::core::ffi::c_int;
    screen_set_cursor_style(c as u_int, &mut s.default_cstyle, &mut s.default_mode);
}
pub fn screen_set_cursor_style(
    style: u_int,
    cstyle: &mut screen_cursor_style,
    mode: &mut ::core::ffi::c_int,
) {
    match style {
        0 => {
            *cstyle = SCREEN_CURSOR_DEFAULT;
        }
        1 => {
            *cstyle = SCREEN_CURSOR_BLOCK;
            *mode |= MODE_CURSOR_BLINKING;
        }
        2 => {
            *cstyle = SCREEN_CURSOR_BLOCK;
            *mode &= !MODE_CURSOR_BLINKING;
        }
        3 => {
            *cstyle = SCREEN_CURSOR_UNDERLINE;
            *mode |= MODE_CURSOR_BLINKING;
        }
        4 => {
            *cstyle = SCREEN_CURSOR_UNDERLINE;
            *mode &= !MODE_CURSOR_BLINKING;
        }
        5 => {
            *cstyle = SCREEN_CURSOR_BAR;
            *mode |= MODE_CURSOR_BLINKING;
        }
        6 => {
            *cstyle = SCREEN_CURSOR_BAR;
            *mode &= !MODE_CURSOR_BLINKING;
        }
        _ => {}
    };
}
pub fn screen_set_cursor_colour(s: &mut screen, colour: ::core::ffi::c_int) {
    s.ccolour = colour;
}
pub fn screen_set_title(
    s: &mut screen,
    title: &CStr,
    untrusted: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let Some(new_title) = clean_name_cstring(title, untrusted) else {
        return 0 as ::core::ffi::c_int;
    };
    s.title = new_title;
    1 as ::core::ffi::c_int
}
pub fn screen_set_path(s: &mut screen, path: &CStr) -> ::core::ffi::c_int {
    let untrusted: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
    let Some(new_path) = clean_name_cstring(path, untrusted) else {
        return 0 as ::core::ffi::c_int;
    };
    s.path = Some(new_path);
    1 as ::core::ffi::c_int
}
pub unsafe fn screen_push_title(s: &mut screen) {
    log_debug(format_args!(
        "{}: {}",
        "screen_push_title",
        s.titles.len() as u_int
    ));
    while s.titles.len() >= 10 {
        let Some(_title) = s.titles.pop_back() else {
            break;
        };
    }
    s.titles.push_front(s.title.clone());
}
pub unsafe fn screen_pop_title(s: &mut screen) {
    if s.titles.is_empty() {
        return;
    }
    log_debug(format_args!(
        "{}: {}",
        "screen_pop_title",
        s.titles.len() as u_int
    ));
    if let Some(title) = s.titles.pop_front() {
        s.title = title;
    }
}
pub fn screen_set_progress_bar(s: &mut screen, pbs: progress_bar_state, p: ::core::ffi::c_int) {
    s.progress_bar.state = pbs;
    if p >= 0 as ::core::ffi::c_int
        && pbs as ::core::ffi::c_uint
            != PROGRESS_BAR_INDETERMINATE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        s.progress_bar.progress = p;
    }
}
pub unsafe fn screen_resize_cursor(
    s: &mut screen,
    mut sx: u_int,
    mut sy: u_int,
    mut reflow: ::core::ffi::c_int,
    eat_empty: ::core::ffi::c_int,
    cursor: ::core::ffi::c_int,
) {
    let mut cx: u_int = s.cx;
    let mut cy: u_int = s.grid().hsize.wrapping_add(s.cy);
    let had_write_list = s.write_list.is_some();
    if had_write_list {
        screen_write_free_list(s);
    }
    log_debug(format_args!(
        "{}: new size {}x{}, now {}x{} (cursor {},{} = {},{})",
        "screen_resize_cursor",
        { sx },
        { sy },
        { s.grid().sx },
        { s.grid().sy },
        { s.cx },
        { s.cy },
        (cx) as u32,
        (cy) as u32
    ));
    if sx < 1 as u_int {
        sx = 1 as u_int;
    }
    if sy < 1 as u_int {
        sy = 1 as u_int;
    }
    if sx != s.grid().sx {
        s.grid_mut().sx = sx;
        screen_reset_tabs(s);
    } else {
        reflow = 0 as ::core::ffi::c_int;
    }
    if sy != s.grid().sy {
        cy = screen_resize_y(s, sy, eat_empty, cy);
    }
    if reflow != 0 {
        (cx, cy) = screen_reflow(s, sx, cx, cy, cursor);
    }
    if cy >= s.grid().hsize {
        s.cx = cx;
        s.cy = cy.wrapping_sub(s.grid().hsize);
    } else {
        s.cx = 0 as u_int;
        s.cy = 0 as u_int;
    }
    log_debug(format_args!(
        "{}: cursor finished at {},{} = {},{}",
        "screen_resize_cursor",
        { s.cx },
        { s.cy },
        (cx) as u32,
        (cy) as u32
    ));
    if had_write_list {
        screen_write_make_list(s);
    }
}
pub unsafe fn screen_resize(s: &mut screen, sx: u_int, sy: u_int, reflow: ::core::ffi::c_int) {
    screen_resize_cursor(
        s,
        sx,
        sy,
        reflow,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
}
unsafe fn screen_resize_y(
    s: &mut screen,
    sy: u_int,
    eat_empty: ::core::ffi::c_int,
    mut cy: u_int,
) -> u_int {
    let viewport_cy = s.cy;
    let gd = s.grid_mut();
    let mut needed: u_int = 0;
    let mut available: u_int = 0;
    let mut oldy: u_int = 0;
    let mut i: u_int = 0;
    if sy == 0 as u_int {
        fatalx(|out| out.write_all(b"zero size"));
    }
    oldy = gd.sy;
    if sy < oldy {
        needed = oldy.wrapping_sub(sy);
        if eat_empty != 0 {
            available = oldy.wrapping_sub(1 as u_int).wrapping_sub(viewport_cy);
            if available > 0 as u_int {
                if available > needed {
                    available = needed;
                }
                grid_view_delete_lines(gd, oldy.wrapping_sub(available), available, 8 as u_int);
            }
            needed = needed.wrapping_sub(available);
        }
        available = viewport_cy;
        if gd.flags & GRID_HISTORY != 0 {
            gd.hscrolled = gd.hscrolled.wrapping_add(needed);
            gd.hsize = gd.hsize.wrapping_add(needed);
        } else if needed > 0 as u_int && available > 0 as u_int {
            if available > needed {
                available = needed;
            }
            grid_view_delete_lines(gd, 0 as u_int, available, 8 as u_int);
            cy = cy.wrapping_sub(available);
        }
    }
    let lines = gd.hsize.wrapping_add(sy);
    grid_adjust_lines(gd, lines);
    if sy > oldy {
        needed = sy.wrapping_sub(oldy);
        available = gd.hscrolled;
        if gd.flags & GRID_HISTORY != 0 && available > 0 as u_int {
            if available > needed {
                available = needed;
            }
            gd.hscrolled = gd.hscrolled.wrapping_sub(available);
            gd.hsize = gd.hsize.wrapping_sub(available);
        } else {
            available = 0 as u_int;
        }
        needed = needed.wrapping_sub(available);
        i = gd.hsize.wrapping_add(sy).wrapping_sub(needed);
        while i < gd.hsize.wrapping_add(sy) {
            grid_empty_line(gd, i, 8 as u_int);
            i = i.wrapping_add(1);
        }
    }
    gd.sy = sy;
    s.rupper = 0 as u_int;
    s.rlower = sy.wrapping_sub(1 as u_int);
    cy
}
pub fn screen_set_selection(
    s: &mut screen,
    sx: u_int,
    sy: u_int,
    ex: u_int,
    ey: u_int,
    rectangle: u_int,
    clipx: u_int,
    modekeys: ::core::ffi::c_int,
    gc: &grid_cell,
) {
    let selection = screen_sel {
        hidden: 0,
        rectangle: rectangle as ::core::ffi::c_int,
        modekeys,
        sx,
        sy,
        ex,
        ey,
        clipx,
        cell: *gc,
    };
    if let Some(existing) = s.sel.as_mut() {
        **existing = selection;
    } else {
        s.sel = Some(Box::new(selection));
    }
}
pub fn screen_clear_selection(s: &mut screen) {
    drop(s.sel.take());
}
pub fn screen_hide_selection(s: &mut screen) {
    if let Some(selection) = s.sel.as_mut() {
        selection.hidden = 1;
    }
}
pub fn screen_check_selection(s: &screen, px: u_int, py: u_int) -> ::core::ffi::c_int {
    let Some(sel) = s.sel.as_ref() else {
        return 0;
    };
    let mut xx: u_int = 0;
    if sel.hidden != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if px < sel.clipx {
        return 0 as ::core::ffi::c_int;
    }
    if sel.rectangle != 0 {
        if sel.sy < sel.ey {
            if py < sel.sy || py > sel.ey {
                return 0 as ::core::ffi::c_int;
            }
        } else if sel.sy > sel.ey {
            if py > sel.sy || py < sel.ey {
                return 0 as ::core::ffi::c_int;
            }
        } else if py != sel.sy {
            return 0 as ::core::ffi::c_int;
        }
        if sel.ex < sel.sx {
            if px < sel.ex {
                return 0 as ::core::ffi::c_int;
            }
            if px > sel.sx {
                return 0 as ::core::ffi::c_int;
            }
        } else {
            if px < sel.sx {
                return 0 as ::core::ffi::c_int;
            }
            if px > sel.ex {
                return 0 as ::core::ffi::c_int;
            }
        }
    } else if sel.sy < sel.ey {
        if py < sel.sy || py > sel.ey {
            return 0 as ::core::ffi::c_int;
        }
        if py == sel.sy && px < sel.sx {
            return 0 as ::core::ffi::c_int;
        }
        if sel.modekeys == MODEKEY_EMACS {
            xx = if sel.ex == 0 as u_int {
                0 as u_int
            } else {
                sel.ex.wrapping_sub(1 as u_int)
            };
        } else {
            xx = sel.ex;
        }
        if py == sel.ey && px > xx {
            return 0 as ::core::ffi::c_int;
        }
    } else if sel.sy > sel.ey {
        if py > sel.sy || py < sel.ey {
            return 0 as ::core::ffi::c_int;
        }
        if py == sel.ey && px < sel.ex {
            return 0 as ::core::ffi::c_int;
        }
        if sel.modekeys == MODEKEY_EMACS {
            xx = sel.sx.wrapping_sub(1 as u_int);
        } else {
            xx = sel.sx;
        }
        if py == sel.sy && (sel.sx == 0 as u_int || px > xx) {
            return 0 as ::core::ffi::c_int;
        }
    } else {
        if py != sel.sy {
            return 0 as ::core::ffi::c_int;
        }
        if sel.ex < sel.sx {
            if sel.modekeys == MODEKEY_EMACS {
                xx = sel.sx.wrapping_sub(1 as u_int);
            } else {
                xx = sel.sx;
            }
            if px > xx || px < sel.ex {
                return 0 as ::core::ffi::c_int;
            }
        } else {
            if sel.modekeys == MODEKEY_EMACS {
                xx = if sel.ex == 0 as u_int {
                    0 as u_int
                } else {
                    sel.ex.wrapping_sub(1 as u_int)
                };
            } else {
                xx = sel.ex;
            }
            if px < sel.sx || px > xx {
                return 0 as ::core::ffi::c_int;
            }
        }
    }
    1 as ::core::ffi::c_int
}
pub fn screen_select_cell(s: &screen, src: &grid_cell) -> Option<grid_cell> {
    let selection = s.sel.as_ref()?;
    if selection.hidden != 0 {
        return None;
    }
    let mut dst = selection.cell;
    if dst.fg == 8 as ::core::ffi::c_int || dst.fg == 9 as ::core::ffi::c_int {
        dst.fg = src.fg;
    }
    if dst.bg == 8 as ::core::ffi::c_int || dst.bg == 9 as ::core::ffi::c_int {
        dst.bg = src.bg;
    }
    dst.data = utf8_copy(&src.data);
    dst.flags = src.flags;
    if dst.attr as ::core::ffi::c_int & GRID_ATTR_NOATTR != 0 {
        dst.attr = (dst.attr as ::core::ffi::c_int
            | src.attr as ::core::ffi::c_int & GRID_ATTR_CHARSET) as u_short;
    } else {
        dst.attr = (dst.attr as ::core::ffi::c_int | src.attr as ::core::ffi::c_int) as u_short;
    }
    Some(dst)
}
unsafe fn screen_reflow(
    s: &mut screen,
    new_x: u_int,
    mut cx: u_int,
    mut cy: u_int,
    cursor: ::core::ffi::c_int,
) -> (u_int, u_int) {
    let mut wx: u_int = 0;
    let mut wy: u_int = 0;
    if cursor != 0 {
        (wx, wy) = grid_wrap_position(s.grid(), cx, cy);
        log_debug(format_args!(
            "{}: cursor {},{} is {},{}",
            "screen_reflow",
            { cx },
            { cy },
            (wx) as u32,
            (wy) as u32
        ));
    }
    grid_reflow(s.grid_mut(), new_x);
    if cursor != 0 {
        (cx, cy) = grid_unwrap_position(s.grid(), wx, wy);
        log_debug(format_args!(
            "{}: new cursor is {},{}",
            "screen_reflow",
            { cx },
            { cy }
        ));
    } else {
        cx = 0 as u_int;
        cy = s.grid().hsize;
    };
    (cx, cy)
}
pub unsafe fn screen_alternate_on(
    s: &mut screen,
    gc: &grid_cell,
    cursor: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    if s.saved_grid.is_some() {
        return 0 as ::core::ffi::c_int;
    }
    sx = s.grid().sx;
    sy = s.grid().sy;
    s.saved_grid = Some(grid_create(sx, sy, 0));
    grid_duplicate_lines(
        s.saved_grid
            .as_deref_mut()
            .expect("saved grid was just created"),
        0 as u_int,
        s.grid.as_deref().expect("screen is initialized"),
        s.grid.as_deref().expect("screen is initialized").hsize,
        sy,
    );
    if cursor != 0 {
        s.saved_cx = s.cx;
        s.saved_cy = s.cy;
    }
    s.saved_cell = *gc;
    grid_view_clear(s.grid_mut(), 0 as u_int, 0 as u_int, sx, sy, 8 as u_int);
    s.saved_flags = s.grid().flags;
    s.grid_mut().flags &= !GRID_HISTORY;
    1 as ::core::ffi::c_int
}
pub unsafe fn screen_alternate_off(
    s: &mut screen,
    gc: Option<&mut grid_cell>,
    cursor: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut sx: u_int = s.grid().sx;
    let mut sy: u_int = s.grid().sy;
    if let Some(saved) = s.saved_grid.as_deref() {
        let (width, height) = (saved.sx, saved.sy);
        screen_resize(s, width, height, 0);
    }
    if cursor != 0 && s.saved_cx != UINT_MAX && s.saved_cy != UINT_MAX {
        s.cx = s.saved_cx;
        s.cy = s.saved_cy;
        if let Some(gc) = gc {
            *gc = s.saved_cell;
        }
    }
    if s.saved_grid.is_none() {
        if s.cx > s.grid().sx.wrapping_sub(1 as u_int) {
            s.cx = s.grid().sx.wrapping_sub(1 as u_int);
        }
        if s.cy > s.grid().sy.wrapping_sub(1 as u_int) {
            s.cy = s.grid().sy.wrapping_sub(1 as u_int);
        }
        return 0 as ::core::ffi::c_int;
    }
    let history = s.grid().hsize;
    let saved = s
        .saved_grid
        .as_deref()
        .expect("alternate screen has a saved grid");
    grid_duplicate_lines(
        s.grid.as_deref_mut().expect("screen is initialized"),
        history,
        saved,
        0 as u_int,
        saved.sy,
    );
    if s.saved_flags & GRID_HISTORY != 0 {
        s.grid_mut().flags |= GRID_HISTORY;
    }
    screen_resize(s, sx, sy, 1 as ::core::ffi::c_int);
    drop(s.saved_grid.take());
    if s.cx > s.grid().sx.wrapping_sub(1 as u_int) {
        s.cx = s.grid().sx.wrapping_sub(1 as u_int);
    }
    if s.cy > s.grid().sy.wrapping_sub(1 as u_int) {
        s.cy = s.grid().sy.wrapping_sub(1 as u_int);
    }
    1 as ::core::ffi::c_int
}
struct ScreenModeNames(i32);

impl std::fmt::Display for ScreenModeNames {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            return out.write_str("NONE");
        }
        if self.0 == ALL_MODES {
            return out.write_str("ALL");
        }
        const SCREEN_MODE_NAMES: &[(i32, &str)] = &[
            (MODE_CURSOR, "CURSOR"),
            (MODE_INSERT, "INSERT"),
            (MODE_KCURSOR, "KCURSOR"),
            (MODE_KKEYPAD, "KKEYPAD"),
            (MODE_WRAP, "WRAP"),
            (MODE_MOUSE_STANDARD, "MOUSE_STANDARD"),
            (MODE_MOUSE_BUTTON, "MOUSE_BUTTON"),
            (MODE_CURSOR_BLINKING, "CURSOR_BLINKING"),
            (MODE_CURSOR_VERY_VISIBLE, "CURSOR_VERY_VISIBLE"),
            (MODE_CURSOR_BLINKING_SET, "CURSOR_BLINKING_SET"),
            (MODE_MOUSE_UTF8, "MOUSE_UTF8"),
            (MODE_MOUSE_SGR, "MOUSE_SGR"),
            (MODE_BRACKETPASTE, "BRACKETPASTE"),
            (MODE_FOCUSON, "FOCUSON"),
            (MODE_MOUSE_ALL, "MOUSE_ALL"),
            (MODE_ORIGIN, "ORIGIN"),
            (MODE_CRLF, "CRLF"),
            (MODE_KEYS_EXTENDED, "KEYS_EXTENDED"),
            (MODE_KEYS_EXTENDED_2, "KEYS_EXTENDED_2"),
            (MODE_THEME_UPDATES, "THEME_UPDATES"),
            (MODE_SYNC, "SYNC"),
        ];
        let mut separator = "";
        for &(mask, name) in SCREEN_MODE_NAMES {
            if self.0 & mask != 0 {
                out.write_str(separator)?;
                out.write_str(name)?;
                separator = ",";
            }
        }
        Ok(())
    }
}

pub fn screen_mode_display(mode: i32) -> impl std::fmt::Display {
    ScreenModeNames(mode)
}
