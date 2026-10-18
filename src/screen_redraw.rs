use crate::src::format::bytes::write_cstr;
use crate::src::format::format_create_defaults;
use crate::src::log::{fatalx, log_cstr, log_debug, log_get_level};
use crate::src::menu::{menu_height, menu_screen, menu_update, menu_width, menu_x, menu_y};
use crate::src::options::options_get_number;
use crate::src::options::OptionsScope;
use crate::src::server::{marked_pane, server_is_marked};
use crate::src::server_client::Client as _;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::borders::{
    CELL_LD, CELL_LR, CELL_LRD, CELL_LRU, CELL_LRUD, CELL_LU, CELL_RD, CELL_RU, CELL_UD, CELL_ULD,
    CELL_URD,
};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::{
    CLIENT_REDRAWBORDERS, CLIENT_REDRAWMENU, CLIENT_REDRAWOVERLAY, CLIENT_REDRAWSTATUS,
    CLIENT_REDRAWWINDOW, CLIENT_SUSPENDED, CLIENT_UTF8,
};
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::layout::*;
use crate::src::shared::limits::SIZE_MAX;
use crate::src::shared::pane::window_pane;
use crate::src::shared::pane::{
    PANE_BORDER_ARROWS, PANE_BORDER_BOTH, PANE_BORDER_COLOUR, PANE_SCROLLBARS_LEFT,
    PANE_STATUS_OFF, PANE_STATUS_TOP,
};
use crate::src::shared::redraw::{
    redraw_line, redraw_scene, redraw_span, redraw_span_data, redraw_span_type, redraw_spans,
    RedrawBorderSpan, RedrawStatusSpan,
};
use crate::src::shared::screen::CURSOR_MODES;
use crate::src::shared::session::SessionRef;
use crate::src::shared::tty::*;
use crate::src::shared::window::WindowRef;
use crate::src::status::{
    status_line_size, status_message_redraw, status_prompt_redraw, status_redraw,
};
use crate::src::style::style_apply_with_options;
use crate::src::text::utf8::utf8_set;
use crate::src::tty::tty_draw_line;
use crate::src::tty::{
    tty_cell, tty_check_overlay_range, tty_cursor, tty_puts, tty_reset, tty_sync_start,
    tty_update_mode,
};
use crate::src::window::windows;
use crate::src::window::Window as _;

use crate::src::window::WindowPane;
use crate::src::window_border::{window_get_border_cell, window_get_fill_cell};
use std::cell::RefCell;

pub const REDRAW_SPAN_MENU: redraw_span_type = 6;
pub const REDRAW_SPAN_SCROLLBAR: redraw_span_type = 5;
pub const REDRAW_SPAN_BORDER: redraw_span_type = 4;
pub const REDRAW_SPAN_STATUS: redraw_span_type = 3;
pub const REDRAW_SPAN_EMPTY: redraw_span_type = 2;
pub const REDRAW_SPAN_OUTSIDE: redraw_span_type = 1;
pub const REDRAW_SPAN_PANE: redraw_span_type = 0;

#[derive(Clone)]
#[repr(C)]
pub struct redraw_draw_ctx<'scene> {
    pub scene: &'scene redraw_scene,
    pub active: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub marked: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub status_lines: u_int,
    pub pane_lines: pane_lines,
    pub default_gc: grid_cell,
    pub flags: ::core::ffi::c_int,
}
#[derive(Clone, Default)]
pub struct redraw_build_cell {
    pub data: redraw_span_data,
}
#[repr(C)]
pub struct redraw_build_ctx<'a> {
    pub w: &'a WindowRef,
    pub ox: u_int,
    pub oy: u_int,
    pub sx: u_int,
    pub sy: u_int,
    pub ind: ::core::ffi::c_int,
    pub cells: &'a mut [redraw_build_cell],
}

pub const REDRAW_BORDER_IS_ARROW: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const REDRAW_SCROLLBAR_LEFT: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const REDRAW_SCROLLBAR_RIGHT: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const REDRAW_SCROLLBAR_OVERLAY: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const REDRAW_PANE: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const REDRAW_OUTSIDE: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const REDRAW_EMPTY: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const REDRAW_PANE_BORDER: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const REDRAW_PANE_STATUS: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const REDRAW_PANE_SCROLLBAR: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const REDRAW_STATUS: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const REDRAW_MENU: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const REDRAW_OVERLAY: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const REDRAW_ALL: ::core::ffi::c_int = 0x7fffffff as ::core::ffi::c_int;
pub const REDRAW_START_ISOLATE: &std::ffi::CStr = c"\u{2066}";
pub const REDRAW_END_ISOLATE: &std::ffi::CStr = c"\u{2069}";
thread_local! {
    static REDRAW_CELLS: RefCell<Vec<redraw_build_cell>> = const { RefCell::new(Vec::new()) };
}

struct RedrawCellScratch(Vec<redraw_build_cell>);

impl RedrawCellScratch {
    fn take() -> Self {
        REDRAW_CELLS.with(|cache| Self(std::mem::take(&mut *cache.borrow_mut())))
    }
}

impl Drop for RedrawCellScratch {
    fn drop(&mut self) {
        self.0.clear();
        REDRAW_CELLS.with(|cache| {
            let mut cached = cache.borrow_mut();
            if self.0.capacity() > cached.capacity() {
                *cached = std::mem::take(&mut self.0);
            }
        });
    }
}
pub const REDRAW_ISOLATES: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const REDRAW_DEFAULT_SET: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const REDRAW_STATUS_TOP: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
fn redraw_flags_to_string(flags: ::core::ffi::c_int) -> std::ffi::CString {
    let mut names = Vec::new();
    for (flag, name) in [
        (REDRAW_STATUS, "status"),
        (REDRAW_PANE, "pane"),
        (REDRAW_PANE_BORDER, "border"),
        (REDRAW_PANE_STATUS, "pane-status"),
        (REDRAW_PANE_SCROLLBAR, "scrollbar"),
        (REDRAW_MENU, "menu"),
        (REDRAW_OVERLAY, "overlay"),
    ] {
        if flags & flag != 0 {
            names.push(name);
        }
    }
    if flags == REDRAW_ALL {
        names.push("all");
    }
    std::ffi::CString::new(names.join(" ")).expect("redraw flag names contain no NUL")
}
unsafe fn redraw_get_window_offset(c: &ClientRef) -> tty_window_view {
    let mut view = c.terminal_view();
    let (sx, sy) = c.terminal_size();
    view.sx = view.sx.max(sx);
    view.sy = view.sy.max(sy.wrapping_sub(status_line_size(c)));
    view
}
unsafe fn redraw_set_context(c: &ClientRef, bctx: &mut redraw_build_ctx) {
    let view = redraw_get_window_offset(c);
    bctx.ox = view.ox;
    bctx.oy = view.oy;
    bctx.sx = view.sx;
    bctx.sy = view.sy;
    bctx.ind = options_get_number(&bctx.w.borrow_options_mut(), c"pane-border-indicators")
        as ::core::ffi::c_int;
}
unsafe fn redraw_get_build_cell(
    mut bctx: *mut redraw_build_ctx,
    mut x: u_int,
    mut y: u_int,
) -> *mut redraw_build_cell {
    let index = y as usize * (*bctx).sx as usize + x as usize;
    &raw mut (*bctx).cells[index]
}
/// Whether scene cell (`x`, `y`) shows the window. A terminal larger than the
/// window shows only the window's size of the strip, as tmux shows a smaller
/// window, and the outside beyond it.
unsafe fn redraw_in_window(bctx: *mut redraw_build_ctx, x: u_int, y: u_int) -> bool {
    let (sx, sy) = (*bctx).w.size();
    x < sx && y < sy
}
/// Whatever a pane, its scrollbar or a separator does not cover is empty
/// within the window, such as the blank extent beside the last pane, and
/// outside beyond it.
unsafe fn redraw_reset_cell(mut bctx: *mut redraw_build_ctx, mut x: u_int, mut y: u_int) {
    let mut bc: *mut redraw_build_cell = redraw_get_build_cell(bctx, x, y);
    if redraw_in_window(bctx, x, y) {
        (*bc).data = redraw_span_data::Empty;
    } else {
        (*bc).data = redraw_span_data::Outside;
    }
}
unsafe fn redraw_window_to_scene(
    mut bctx: *mut redraw_build_ctx,
    mut wx: ::core::ffi::c_int,
    mut wy: ::core::ffi::c_int,
    mut x: *mut u_int,
    mut y: *mut u_int,
) -> ::core::ffi::c_int {
    let mut sx: ::core::ffi::c_int = 0;
    let mut sy: ::core::ffi::c_int = 0;
    if wx < 0 as ::core::ffi::c_int || wy < 0 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    let (width, height) = (*bctx).w.logical_size();
    if wx as u_int > width || wy as u_int > height {
        return 0 as ::core::ffi::c_int;
    }
    if wx < (*bctx).ox as ::core::ffi::c_int || wy < (*bctx).oy as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    sx = wx - (*bctx).ox as ::core::ffi::c_int;
    sy = wy - (*bctx).oy as ::core::ffi::c_int;
    if sx as u_int >= (*bctx).sx
        || sy as u_int >= (*bctx).sy
        || !redraw_in_window(bctx, sx as u_int, sy as u_int)
    {
        return 0 as ::core::ffi::c_int;
    }
    *x = sx as u_int;
    *y = sy as u_int;
    1 as ::core::ffi::c_int
}
unsafe fn redraw_pane_to_scene(
    mut bctx: *mut redraw_build_ctx,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut px: ::core::ffi::c_int,
    mut py: ::core::ffi::c_int,
    mut x: *mut u_int,
    mut y: *mut u_int,
) -> ::core::ffi::c_int {
    let mut wx: ::core::ffi::c_int = wp.geometry().2 + px;
    let mut wy: ::core::ffi::c_int = wp.geometry().3 + py;
    redraw_window_to_scene(bctx, wx, wy, x, y)
}
/// Whether the window's layout splits the colour of a separator between two
/// panes.
unsafe fn redraw_check_split_colours(w: &WindowRef) -> bool {
    w.layout().splits_separator_colours(w.pane_snapshot().len())
}

unsafe fn redraw_mark_pane_inside(
    mut bctx: *mut redraw_build_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let wp = wp_owner;
    let mut bc: *mut redraw_build_cell = ::core::ptr::null_mut::<redraw_build_cell>();
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    py = 0 as u_int;
    while py < wp.geometry().1 {
        px = 0 as u_int;
        while px < wp.geometry().0 {
            if !(redraw_pane_to_scene(
                bctx,
                wp,
                px as ::core::ffi::c_int,
                py as ::core::ffi::c_int,
                &raw mut x,
                &raw mut y,
            ) == 0)
            {
                bc = redraw_get_build_cell(bctx, x, y);
                (*bc).data = redraw_span_data::Pane(Default::default());
                (*bc).data.pane_mut().wp = std::rc::Rc::downgrade(wp);
                (*bc).data.pane_mut().px = px;
                (*bc).data.pane_mut().py = py;
            }
            px = px.wrapping_add(1);
        }
        py = py.wrapping_add(1);
    }
}
unsafe fn redraw_mark_pane_scrollbar(
    mut bctx: *mut redraw_build_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut sb_w: ::core::ffi::c_int,
    mut sb_left: ::core::ffi::c_int,
    mut overlay: ::core::ffi::c_int,
) {
    let wp = wp_owner;
    let mut bc: *mut redraw_build_cell = ::core::ptr::null_mut::<redraw_build_cell>();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut wx: ::core::ffi::c_int = 0;
    let mut wy: ::core::ffi::c_int = 0;
    let mut sx: ::core::ffi::c_int = 0;
    let mut ex: ::core::ffi::c_int = 0;
    let mut sy: u_int = 0;
    if sb_w == 0 as ::core::ffi::c_int {
        return;
    }
    if overlay != 0 && sb_left != 0 {
        sx = wp.geometry().2;
        ex = sx + sb_w - 1 as ::core::ffi::c_int;
    } else if overlay != 0 {
        ex = wp.geometry().2 + wp.geometry().0 as ::core::ffi::c_int - 1 as ::core::ffi::c_int;
        sx = ex - sb_w + 1 as ::core::ffi::c_int;
    } else if sb_left != 0 {
        sx = wp.geometry().2 - sb_w;
        ex = wp.geometry().2 - 1 as ::core::ffi::c_int;
    } else {
        sx = wp.geometry().2 + wp.geometry().0 as ::core::ffi::c_int;
        ex = sx + sb_w - 1 as ::core::ffi::c_int;
    }
    sy = 0 as u_int;
    while sy < wp.geometry().1 {
        wy = wp.geometry().3 + sy as ::core::ffi::c_int;
        wx = sx;
        while wx <= ex {
            if !(redraw_window_to_scene(bctx, wx, wy, &raw mut x, &raw mut y) == 0) {
                bc = redraw_get_build_cell(bctx, x, y);
                (*bc).data = redraw_span_data::Scrollbar(Default::default());
                (*bc).data.scrollbar_mut().wp = std::rc::Rc::downgrade(wp);
                (*bc).data.scrollbar_mut().y = sy;
                (*bc).data.scrollbar_mut().height = wp.geometry().1;
                if sb_left != 0 {
                    (*bc).data.scrollbar_mut().flags |= REDRAW_SCROLLBAR_LEFT;
                } else {
                    (*bc).data.scrollbar_mut().flags |= REDRAW_SCROLLBAR_RIGHT;
                }
                if overlay != 0 {
                    (*bc).data.scrollbar_mut().flags |= REDRAW_SCROLLBAR_OVERLAY;
                }
            }
            wx += 1;
        }
        sy = sy.wrapping_add(1);
    }
}
fn redraw_data_has_pane(
    data: &redraw_span_data,
    pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
) -> bool {
    let border = data.border();
    border.left_wp.ptr_eq(pane) || border.right_wp.ptr_eq(pane)
}
/// Mark the separator cell at window position (`wx`, `wy`) beside `wp`:
/// `after` when the cell precedes the pane, on its left or above it.
unsafe fn redraw_mark_border_cell(
    mut bctx: *mut redraw_build_ctx,
    mut wx: ::core::ffi::c_int,
    mut wy: ::core::ffi::c_int,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    after: bool,
) {
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if redraw_window_to_scene(bctx, wx, wy, &raw mut x, &raw mut y) == 0 {
        return;
    }
    let bc = redraw_get_build_cell(bctx, x, y);
    match (*bc).data.kind() {
        REDRAW_SPAN_OUTSIDE | REDRAW_SPAN_EMPTY => {
            (*bc).data = redraw_span_data::Border(Default::default())
        }
        REDRAW_SPAN_BORDER => {}
        _ => return,
    }
    let border = (*bc).data.border_mut();
    if after {
        border.right_wp = std::rc::Rc::downgrade(wp);
    } else {
        border.left_wp = std::rc::Rc::downgrade(wp);
    }
}
/// Mark the arrow cells on the second row of the separators at `left` and
/// `right` beside `wp`.
unsafe fn redraw_mark_border_arrows(
    mut bctx: *mut redraw_build_ctx,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut left: ::core::ffi::c_int,
    mut right: ::core::ffi::c_int,
) {
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if (*bctx).ind != PANE_BORDER_ARROWS && (*bctx).ind != PANE_BORDER_BOTH {
        return;
    }
    let (_, sy, _, yoff) = wp.geometry();
    let wy = yoff + 1;
    if wy >= yoff + sy as ::core::ffi::c_int {
        return;
    }
    for wx in [left, right] {
        if redraw_window_to_scene(bctx, wx, wy, &raw mut x, &raw mut y) != 0 {
            let bc = redraw_get_build_cell(bctx, x, y);
            if (*bc).data.kind() == REDRAW_SPAN_BORDER {
                (*bc).data.border_mut().flags |= REDRAW_BORDER_IS_ARROW;
            }
        }
    }
}
/// Mark the separators around `wp`. The columns beside it are as tall as the
/// pane: the one before it when a column precedes the pane, and the one after
/// it while that column is within the extent. The rows above and below it span
/// the pane and both columns, within the window; a pane as tall as the window,
/// as every strip pane is, has none.
unsafe fn redraw_mark_pane_borders(
    mut bctx: *mut redraw_build_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut sb_w: ::core::ffi::c_int,
    mut sb_left: ::core::ffi::c_int,
) {
    let wp = wp_owner;
    let (sx, sy, xoff, yoff) = wp.geometry();
    let mut left = xoff - 1;
    let mut right = xoff + sx as ::core::ffi::c_int;
    if sb_w != 0 {
        if sb_left != 0 {
            left -= sb_w;
        } else {
            right += sb_w;
        }
    }
    let (extent, height) = (*bctx).w.logical_size();
    let extent = extent as ::core::ffi::c_int;
    for (mark, wx) in [(left >= 0, left), (right <= extent, right)] {
        if mark {
            for wy in yoff..yoff + sy as ::core::ffi::c_int {
                redraw_mark_border_cell(bctx, wx, wy, wp, wx == left);
            }
        }
    }
    for wy in [yoff - 1, yoff + sy as ::core::ffi::c_int] {
        if wy < 0 || wy >= height as ::core::ffi::c_int {
            continue;
        }
        for wx in left.max(0)..=right.min(extent) {
            redraw_mark_border_cell(bctx, wx, wy, wp, wy < yoff);
        }
    }
    redraw_mark_border_arrows(bctx, wp, left, right);
}
/// The columns `wp`'s scrollbar takes, whether it is on the left, and whether
/// it overlays the pane.
unsafe fn redraw_scrollbar_extent(
    bctx: *mut redraw_build_ctx,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) -> (::core::ffi::c_int, ::core::ffi::c_int, ::core::ffi::c_int) {
    let mut sb_w: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut sb_left: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut overlay: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if wp.scrollbar_visible() {
        overlay = wp.scrollbar_overlay() as i32;
        if overlay != 0 {
            sb_w = wp.scrollbar_width() + wp.scrollbar_pad();
            if sb_w > wp.geometry().0 as ::core::ffi::c_int {
                sb_w = wp.scrollbar_width();
                if sb_w > wp.geometry().0 as ::core::ffi::c_int {
                    sb_w = wp.geometry().0 as ::core::ffi::c_int;
                }
            }
        } else {
            sb_w = wp.scrollbar_width() + wp.scrollbar_pad();
        }
    }
    if sb_w != 0 as ::core::ffi::c_int && ((*bctx).w).scrollbar_position() == PANE_SCROLLBARS_LEFT {
        sb_left = 1 as ::core::ffi::c_int;
    }
    (sb_w, sb_left, overlay)
}
unsafe fn redraw_mark_pane(
    mut bctx: *mut redraw_build_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let (sb_w, sb_left, overlay) = redraw_scrollbar_extent(bctx, wp_owner);
    redraw_mark_pane_inside(bctx, wp_owner);
    redraw_mark_pane_borders(
        bctx,
        wp_owner,
        if overlay != 0 {
            0 as ::core::ffi::c_int
        } else {
            sb_w
        },
        sb_left,
    );
    redraw_mark_pane_scrollbar(bctx, wp_owner, sb_w, sb_left, overlay);
}
/// The line a separator cell draws, from the separator cells it joins above,
/// below, left and right. A lone cell is part of a vertical separator.
fn redraw_border_type(up: bool, down: bool, left: bool, right: bool) -> ::core::ffi::c_int {
    match (up, down, left, right) {
        (true, true, true, true) => CELL_LRUD,
        (true, true, false, true) => CELL_URD,
        (true, true, true, false) => CELL_ULD,
        (false, true, true, true) => CELL_LRD,
        (true, false, true, true) => CELL_LRU,
        (false, true, false, true) => CELL_RD,
        (false, true, true, false) => CELL_LD,
        (true, false, false, true) => CELL_RU,
        (true, false, true, false) => CELL_LU,
        (false, false, false, false) => CELL_UD,
        (false, false, _, _) => CELL_LR,
        _ => CELL_UD,
    }
}
unsafe fn redraw_is_border(bctx: *mut redraw_build_ctx, x: u_int, y: u_int) -> bool {
    x < (*bctx).sx
        && y < (*bctx).sy
        && (*redraw_get_build_cell(bctx, x, y)).data.kind() == REDRAW_SPAN_BORDER
}
/// Give every separator cell its line, joining the separator cells beside it.
unsafe fn redraw_set_border_types(bctx: *mut redraw_build_ctx) {
    for y in 0..(*bctx).sy {
        for x in 0..(*bctx).sx {
            if !redraw_is_border(bctx, x, y) {
                continue;
            }
            let cell_type = redraw_border_type(
                y > 0 && redraw_is_border(bctx, x, y - 1),
                redraw_is_border(bctx, x, y + 1),
                x > 0 && redraw_is_border(bctx, x - 1, y),
                redraw_is_border(bctx, x + 1, y),
            );
            (*redraw_get_build_cell(bctx, x, y))
                .data
                .border_mut()
                .cell_type = cell_type;
        }
    }
}
/// Mark `wp`'s status row: the separator cells of the row above it (or below
/// it) from two columns in to its right edge, each keeping the line it draws.
unsafe fn redraw_mark_border_status(
    bctx: *mut redraw_build_ctx,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let pane_status = wp.border_status();
    if pane_status == PANE_STATUS_OFF {
        return;
    }
    let (sx, sy, xoff, yoff) = wp.geometry();
    let wy = if pane_status == PANE_STATUS_TOP {
        yoff - 1
    } else {
        yoff + sy as ::core::ffi::c_int
    };
    let (sb_w, sb_left, overlay) = redraw_scrollbar_extent(bctx, wp);
    let mut right = xoff + sx as ::core::ffi::c_int;
    if overlay == 0 && sb_left == 0 {
        right += sb_w;
    }
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    for (offset, wx) in (xoff + 2..right).enumerate() {
        if redraw_window_to_scene(bctx, wx, wy, &raw mut x, &raw mut y) == 0 {
            continue;
        }
        let bc = redraw_get_build_cell(bctx, x, y);
        if (*bc).data.kind() != REDRAW_SPAN_BORDER {
            continue;
        }
        let cell_type = (*bc).data.border().cell_type;
        (*bc).data = redraw_span_data::Status(RedrawStatusSpan {
            wp: std::rc::Rc::downgrade(wp),
            offset: offset as u_int,
            cell_type,
        });
    }
}
/// When the layout says so, a separator between two panes splits its colour:
/// a vertical one at half the window's height, a horizontal one at half its
/// width.
unsafe fn redraw_mark_split_colours(mut bctx: *mut redraw_build_ctx) {
    let mut bc: *mut redraw_build_cell = ::core::ptr::null_mut::<redraw_build_cell>();
    let mut sd: *mut redraw_span_data = ::core::ptr::null_mut::<redraw_span_data>();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut wy: u_int = 0;
    if (*bctx).ind != PANE_BORDER_COLOUR && (*bctx).ind != PANE_BORDER_BOTH {
        return;
    }
    if !redraw_check_split_colours((*bctx).w) {
        return;
    }
    y = 0 as u_int;
    while y < (*bctx).sy {
        x = 0 as u_int;
        while x < (*bctx).sx {
            bc = redraw_get_build_cell(bctx, x, y);
            if !((*bc).data.kind() as ::core::ffi::c_uint
                != REDRAW_SPAN_BORDER as ::core::ffi::c_int as ::core::ffi::c_uint)
            {
                sd = &raw mut (*bc).data;
                wy = (*bctx).oy.wrapping_add(y);
                let (position, size) = if (*sd).border().cell_type == CELL_LR {
                    ((*bctx).ox.wrapping_add(x), (*bctx).w.logical_size().0)
                } else {
                    (wy, (*bctx).w.logical_size().1)
                };
                if (*sd).border().left_wp.strong_count() != 0
                    && (*sd).border().right_wp.strong_count() != 0
                {
                    if position <= size.wrapping_div(2 as u_int) {
                        (*sd).border_mut().style_wp = (*sd).border().left_wp.clone();
                    } else {
                        (*sd).border_mut().style_wp = (*sd).border().right_wp.clone();
                    }
                }
            }
            x = x.wrapping_add(1);
        }
        y = y.wrapping_add(1);
    }
}
/// Frame a window smaller than the scene, as tmux does: the column after it
/// and the row below it are separator cells, `│` down the right edge, `─`
/// along the bottom and `┘` where they meet. A frame cell beside a pane is
/// that pane's border; the separators inside the window do not join it.
unsafe fn redraw_mark_window_frame(bctx: *mut redraw_build_ctx) {
    let (sx, sy) = (*bctx).w.size();
    let right = sx < (*bctx).sx;
    let bottom = sy < (*bctx).sy;
    if right {
        for y in 0..sy.min((*bctx).sy) {
            redraw_mark_frame_cell(bctx, (sx, y), Some((sx - 1, y)), CELL_UD);
        }
    }
    if bottom {
        for x in 0..sx.min((*bctx).sx) {
            redraw_mark_frame_cell(bctx, (x, sy), Some((x, sy - 1)), CELL_LR);
        }
    }
    if right && bottom {
        redraw_mark_frame_cell(bctx, (sx, sy), None, CELL_LU);
    }
}
/// Make scene cell `at` a frame cell drawing `cell_type`, the border of the
/// pane at scene cell `inside` if there is one.
unsafe fn redraw_mark_frame_cell(
    bctx: *mut redraw_build_ctx,
    (x, y): (u_int, u_int),
    inside: Option<(u_int, u_int)>,
    cell_type: ::core::ffi::c_int,
) {
    let owner = inside
        .map(|(ix, iy)| &(*redraw_get_build_cell(bctx, ix, iy)).data)
        .and_then(|data| match data {
            redraw_span_data::Pane(pane) => Some(pane.wp.clone()),
            _ => None,
        })
        .unwrap_or_default();
    (*redraw_get_build_cell(bctx, x, y)).data = redraw_span_data::Border(RedrawBorderSpan {
        left_wp: owner,
        cell_type,
        ..Default::default()
    });
}
unsafe fn redraw_mark_menu(bctx: *mut redraw_build_ctx) {
    let Some(owner) = (*bctx).w.menu_observer() else {
        return;
    };
    let md = owner.try_borrow_mut().expect("live unborrowed menu");
    let observer = owner.clone();
    for py in 0..menu_height(&md) {
        for px in 0..menu_width(&md) {
            let (mut x, mut y) = (0, 0);
            if redraw_window_to_scene(
                bctx,
                menu_x(&md).wrapping_add(px) as ::core::ffi::c_int,
                menu_y(&md).wrapping_add(py) as ::core::ffi::c_int,
                &mut x,
                &mut y,
            ) != 0
            {
                let cell = &mut *redraw_get_build_cell(bctx, x, y);
                cell.data = redraw_span_data::Menu(crate::src::shared::redraw::RedrawMenuSpan {
                    md: observer.clone(),
                    px,
                    py,
                });
            }
        }
    }
}
fn redraw_compare_data(a: &redraw_build_cell, b: &redraw_build_cell) -> bool {
    use redraw_span_data::*;
    match (&a.data, &b.data) {
        (Pane(a), Pane(b)) => a.wp.ptr_eq(&b.wp) && a.py == b.py && a.px.wrapping_add(1) == b.px,
        (Border(a), Border(b)) => a == b && a.flags & REDRAW_BORDER_IS_ARROW == 0,
        (Status(a), Status(b)) => {
            a.wp.ptr_eq(&b.wp) && a.offset.wrapping_add(1) == b.offset && a.cell_type == b.cell_type
        }
        (Scrollbar(a), Scrollbar(b)) => a == b,
        (Menu(a), Menu(b)) => a.md == b.md && a.py == b.py && a.px.wrapping_add(1) == b.px,
        (Outside, Outside) | (Empty, Empty) => true,
        _ => false,
    }
}
unsafe fn redraw_build_cells<'a>(
    mut bctx: *mut redraw_build_ctx<'a>,
    cells: &'a mut Vec<redraw_build_cell>,
) {
    let mut ncells: size_t = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if (*bctx).sx != 0 as u_int
        && (*bctx).sy as ::core::ffi::c_ulong
            > SIZE_MAX.wrapping_div((*bctx).sx as ::core::ffi::c_ulong)
    {
        fatalx(|out| {
            write_cstr(out, c"redraw_build_cells")?;
            out.write_all(b": too many cells")
        });
    }
    ncells = ((*bctx).sx as size_t).wrapping_mul((*bctx).sy as size_t);
    if ncells > cells.len() {
        if cells.try_reserve_exact(ncells - cells.len()).is_err() {
            fatalx(|out| {
                write_cstr(out, c"redraw_build_cells")?;
                out.write_all(b": too many cells")
            });
        }
        cells.resize_with(ncells, redraw_build_cell::default);
    }
    (*bctx).cells = &mut cells[..ncells];
    y = 0 as u_int;
    while y < (*bctx).sy {
        x = 0 as u_int;
        while x < (*bctx).sx {
            redraw_reset_cell(bctx, x, y);
            x = x.wrapping_add(1);
        }
        y = y.wrapping_add(1);
    }
    for pane_owner in (*bctx)
        .w
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .rev()
    {
        redraw_mark_pane(bctx, &pane_owner);
    }
    redraw_set_border_types(bctx);
    redraw_mark_split_colours(bctx);
    redraw_mark_window_frame(bctx);
    for pane_owner in (*bctx)
        .w
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
    {
        redraw_mark_border_status(bctx, &pane_owner);
    }
    redraw_mark_menu(bctx);
}
unsafe fn redraw_make_scene(client_owner: &ClientRef) -> Option<Box<redraw_scene>> {
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let mut s: Option<SessionRef> = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let window_owner = s
        .as_ref()
        .expect("live session")
        .current_winlink()
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("current window");
    let mut bctx: redraw_build_ctx = redraw_build_ctx {
        w: &window_owner,
        ox: 0,
        oy: 0,
        sx: 0,
        sy: 0,
        ind: 0,
        cells: &mut [],
    };
    let mut bc: *mut redraw_build_cell = ::core::ptr::null_mut::<redraw_build_cell>();
    let mut last: *mut redraw_build_cell = ::core::ptr::null_mut::<redraw_build_cell>();
    let mut type_0: redraw_span_type = REDRAW_SPAN_PANE;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut x0: u_int = 0;
    if c.as_ref().expect("live client").flags() & CLIENT_SUSPENDED as uint64_t != 0 {
        drop(window_owner);
        return None;
    }
    redraw_set_context(client_owner, &mut bctx);
    let mut cells = RedrawCellScratch::take();
    log_debug(format_args!(
        "{}: building @{} scene ({}x{} {},{}; generation {})",
        log_cstr(
            (c.as_ref().expect("live client").name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        { window_owner.id() },
        { bctx.sx },
        { bctx.sy },
        { bctx.ox },
        { bctx.oy },
        window_owner.scene_generation() as ::core::ffi::c_ulonglong
    ));
    redraw_build_cells(&raw mut bctx, &mut cells.0);
    let mut scene = Box::new(redraw_scene {
        c: std::rc::Rc::downgrade(client_owner),
        w: WindowRef::downgrade(&window_owner).clone(),
        lines: Box::default(),
        generation: window_owner.scene_generation(),
        sx: bctx.sx,
        sy: bctx.sy,
        ox: bctx.ox,
        oy: bctx.oy,
    });
    if bctx.sy == 0 {
        fatalx(|out| out.write_all(b"xcalloc: zero size"));
    }
    let lines = ::std::iter::repeat_with(redraw_line::default)
        .take(bctx.sy as usize)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    scene.lines = lines;
    y = 0 as u_int;
    while y < bctx.sy {
        let line = &mut scene.lines[y as usize];
        x = 0 as u_int;
        while x < bctx.sx {
            x0 = x;
            last = redraw_get_build_cell(&raw mut bctx, x, y);
            x = x.wrapping_add(1);
            while x < bctx.sx {
                bc = redraw_get_build_cell(&raw mut bctx, x, y);
                if !redraw_compare_data(&*last, &*bc) {
                    break;
                }
                last = bc;
                x = x.wrapping_add(1);
            }
            bc = redraw_get_build_cell(&raw mut bctx, x0, y);
            type_0 = (*bc).data.kind();
            line[type_0 as usize].push(Box::new(redraw_span {
                x: x0,
                width: x.wrapping_sub(x0),
                data: (*bc).data.clone(),
            }));
        }
        y = y.wrapping_add(1);
    }
    log_debug(format_args!(
        "{}: finished building @{} scene",
        log_cstr(
            (c.as_ref().expect("live client").name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        { window_owner.id() }
    ));
    drop(window_owner);
    Some(scene)
}

pub unsafe fn redraw_invalidate_all_scenes() {
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        window_owner.invalidate_scene();
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
}
unsafe fn redraw_get_scene(client_owner: &ClientRef) -> Option<Box<redraw_scene>> {
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let window_owner = (c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade()
        .expect("live session")
        .current_winlink())
    .get_unchecked()
    .window_handle()
    .cloned()
    .expect("current window");
    let tty_window_view { ox, oy, sx, sy, .. } = redraw_get_window_offset(client_owner);
    let scene = client_owner.take_redraw_scene();
    let reason = match scene.as_deref() {
        None => Some("missing"),
        Some(scene) if !scene.w.ptr_eq(&WindowRef::downgrade(&window_owner)) => {
            Some("window changed")
        }
        Some(scene) if scene.generation != window_owner.scene_generation() => {
            Some("generation changed")
        }
        Some(scene) if scene.ox != ox || scene.oy != oy => Some("offset changed"),
        Some(scene) if scene.sx != sx || scene.sy != sy => Some("size changed"),
        Some(_) => None,
    };
    let result = if let Some(reason) = reason {
        log_debug(format_args!(
            "{}: @{} scene invalid: {}",
            log_cstr(
                c.as_ref()
                    .expect("live client")
                    .name()
                    .as_deref()
                    .unwrap_or(c"(null)")
            ),
            window_owner.id(),
            reason
        ));
        drop(scene);
        redraw_make_scene(client_owner)
    } else {
        scene
    };
    drop(window_owner);
    result
}

unsafe fn redraw_draw_pane_span(
    dctx: &mut redraw_draw_ctx<'_>,
    span: &redraw_span,
    mut x: u_int,
    mut y: u_int,
    mut n: u_int,
) {
    let Some(pane) = span.data.pane().wp.upgrade() else {
        return;
    };
    let Some(client) = dctx.scene.c.upgrade() else {
        return;
    };
    pane.draw_line(
        &client,
        (
            span.data.pane().px.wrapping_add(x.wrapping_sub(span.x)),
            span.data.pane().py,
        ),
        n,
        (x, y),
        false,
    );
}
unsafe fn redraw_get_default_border_style(
    dctx: &mut redraw_draw_ctx<'_>,
    mut gc: *mut grid_cell,
    mut pane_lines: *mut pane_lines,
) {
    let scene = dctx.scene;
    let Some(client_owner) = scene.c.upgrade() else {
        return;
    };
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let mut s: Option<SessionRef> = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let Some(window_owner) = scene.w.upgrade() else {
        return;
    };
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut dgc: *mut grid_cell = &mut dctx.default_gc;
    if !dctx.flags & REDRAW_DEFAULT_SET != 0 {
        let mut ft_owner = format_create_defaults(
            None,
            c.as_ref(),
            s.as_ref(),
            (s.as_ref().expect("live session").current_winlink()).clone(),
            None,
        );
        ft = &raw mut *ft_owner;
        style_apply_with_options(
            &mut *dgc,
            c"pane-border-style",
            Some(&mut *ft),
            &OptionsScope::Window(WindowRef::downgrade(&window_owner)),
        );
        drop(ft_owner);
        dctx.pane_lines = window_owner.pane_border_lines();
        dctx.flags |= REDRAW_DEFAULT_SET;
    }
    *gc = *dgc;
    *pane_lines = dctx.pane_lines;
    drop(window_owner);
}
fn redraw_get_pane_for_border_style(
    active: Option<&std::rc::Weak<std::cell::UnsafeCell<window_pane>>>,
    span: &redraw_span,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    let redraw_span_data::Border(border) = &span.data else {
        return None;
    };
    if let Some(owner) = border.style_wp.upgrade() {
        return Some(owner);
    }
    if let Some(active) = active {
        if redraw_data_has_pane(&span.data, active) {
            if let Some(owner) = active.upgrade() {
                return Some(owner);
            }
        }
    }
    border
        .left_wp
        .upgrade()
        .or_else(|| border.right_wp.upgrade())
}
unsafe fn redraw_draw_border_arrow(
    dctx: &mut redraw_draw_ctx<'_>,
    span: &redraw_span,
    mut gc: *mut grid_cell,
) {
    let active = &dctx.active;
    let mut ch: ::core::ffi::c_char = 0;
    if span.data.kind() as ::core::ffi::c_uint
        != REDRAW_SPAN_BORDER as ::core::ffi::c_int as ::core::ffi::c_uint
        || active.strong_count() == 0
    {
        return;
    }
    if !span.data.border().flags & REDRAW_BORDER_IS_ARROW != 0 {
        return;
    }
    if span.data.border().left_wp.ptr_eq(active) {
        ch = ',' as i32 as ::core::ffi::c_char;
    } else if span.data.border().right_wp.ptr_eq(active) {
        ch = '+' as i32 as ::core::ffi::c_char;
    } else {
        return;
    }
    utf8_set(&mut (*gc).data, ch as u_char);
    (*gc).attr = ((*gc).attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
}
unsafe fn redraw_draw_border_span(
    dctx: &mut redraw_draw_ctx<'_>,
    span: &redraw_span,
    mut x: u_int,
    mut y: u_int,
    mut n: u_int,
) {
    let scene = dctx.scene;
    let Some(client_owner) = scene.c.upgrade() else {
        return;
    };
    let _c: Option<ClientRef> = Some(client_owner.clone());
    let Some(window_owner) = scene.w.upgrade() else {
        return;
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
    let mut pane_lines: pane_lines = PANE_LINES_SINGLE;
    let mut i: u_int = 0;
    let border = span.data.kind() == REDRAW_SPAN_BORDER;
    let mut border_pane_owner = None;
    let mut isolates: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if border {
        border_pane_owner = redraw_get_pane_for_border_style(Some(&dctx.active), span);
    }
    if let Some(owner) = border_pane_owner.as_ref() {
        gc = owner.border_style(&client_owner);
        owner.border_cell(span.data.border().cell_type, &mut gc);
    } else {
        redraw_get_default_border_style(dctx, &raw mut gc, &raw mut pane_lines);
        if border {
            window_get_border_cell(None, pane_lines, span.data.border().cell_type, &mut gc);
        } else {
            let inside = (span.data.kind() == REDRAW_SPAN_EMPTY) as ::core::ffi::c_int;
            window_get_fill_cell(&window_owner, inside, &raw mut gc);
        }
    }
    if border && dctx.marked.strong_count() != 0 && redraw_data_has_pane(&span.data, &dctx.marked) {
        gc.attr = (gc.attr as ::core::ffi::c_int ^ GRID_ATTR_REVERSE) as u_short;
    }
    redraw_draw_border_arrow(dctx, span, &raw mut gc);
    if border && dctx.flags & REDRAW_ISOLATES != 0 {
        isolates = 1 as ::core::ffi::c_int;
    }
    {
        let terminal = &(client_owner);
        tty_cursor(terminal, x, y)
    };
    if isolates != 0 {
        {
            let terminal = &(client_owner);
            tty_puts(terminal, REDRAW_END_ISOLATE)
        };
    }
    i = 0 as u_int;
    while i < n {
        tty_cell(&client_owner, &gc, None);
        i = i.wrapping_add(1);
    }
    if isolates != 0 {
        {
            let terminal = &(client_owner);
            tty_puts(terminal, REDRAW_START_ISOLATE)
        };
    }
}
unsafe fn redraw_draw_scrollbar_span(
    dctx: &mut redraw_draw_ctx<'_>,
    span: &redraw_span,
    mut x: u_int,
    mut y: u_int,
    mut n: u_int,
) {
    let Some(pane_owner) = span.data.scrollbar().wp.upgrade() else {
        return;
    };
    let Some(client_owner) = dctx.scene.c.upgrade() else {
        return;
    };
    pane_owner.draw_scrollbar(&client_owner, span, x, y, n);
}
unsafe fn redraw_draw_status_span(
    dctx: &mut redraw_draw_ctx<'_>,
    span: &redraw_span,
    x: u_int,
    y: u_int,
    n: u_int,
) {
    let Some(pane) = span.data.status().wp.upgrade() else {
        return;
    };
    let Some(client) = dctx.scene.c.upgrade() else {
        return;
    };
    pane.draw_line(
        &client,
        (
            span.data
                .status()
                .offset
                .wrapping_add(x.wrapping_sub(span.x)),
            0,
        ),
        n,
        (x, y),
        true,
    );
}
unsafe fn redraw_draw_menu_span(
    dctx: &mut redraw_draw_ctx<'_>,
    span: &redraw_span,
    x: u_int,
    y: u_int,
    n: u_int,
) {
    let data = span.data.menu();
    let md = match data.md.try_borrow_mut() {
        Ok(md) => md,
        Err(refbox::BorrowError::Dropped) => return,
        Err(refbox::BorrowError::Borrowed) => panic!("menu already borrowed during redraw"),
    };
    if md.closed {
        return;
    }
    let scene = dctx.scene;
    let Some(client_owner) = scene.c.upgrade() else {
        return;
    };
    let px = data.px.wrapping_add(x.wrapping_sub(span.x));
    {
        let terminal = &(client_owner);
        tty_draw_line(terminal, menu_screen(&md), px, data.py, n, x, y, None);
    };
}
unsafe fn redraw_draw_span(dctx: &mut redraw_draw_ctx<'_>, span: &redraw_span, mut y: u_int) {
    let scene = dctx.scene;
    let data = &span.data;
    let type_0: redraw_span_type = data.kind();
    let Some(client_owner) = scene.c.upgrade() else {
        return;
    };
    if type_0 == REDRAW_SPAN_STATUS
        && data
            .status()
            .wp
            .upgrade()
            .is_none_or(|owner| !owner.has_new_status())
    {
        return;
    }
    if span.width == 0 {
        return;
    }
    let ranges = tty_check_overlay_range(&client_owner, span.x, y, span.width);
    for range in ranges.storage.iter().take(ranges.used as usize) {
        if range.nx == 0 {
            continue;
        }
        let (x, n) = (range.px, range.nx);
        match type_0 as ::core::ffi::c_uint {
            REDRAW_SPAN_PANE => {
                redraw_draw_pane_span(dctx, span, x, y, n);
            }
            REDRAW_SPAN_STATUS => {
                redraw_draw_status_span(dctx, span, x, y, n);
            }
            REDRAW_SPAN_BORDER | REDRAW_SPAN_OUTSIDE | REDRAW_SPAN_EMPTY => {
                redraw_draw_border_span(dctx, span, x, y, n);
            }
            REDRAW_SPAN_SCROLLBAR => {
                redraw_draw_scrollbar_span(dctx, span, x, y, n);
            }
            REDRAW_SPAN_MENU => {
                redraw_draw_menu_span(dctx, span, x, y, n);
            }
            _ => {}
        }
    }
}
unsafe fn redraw_draw_pane_lines(
    dctx: &mut redraw_draw_ctx<'_>,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    flags: ::core::ffi::c_int,
) {
    let wp = wp_owner;
    let scene = dctx.scene;
    let top = (wp.geometry().3 - scene.oy as i32).max(0);
    let bottom = (wp.geometry().3 + wp.geometry().1 as i32 - scene.oy as i32)
        .max(0)
        .min(scene.sy as i32);
    for y in top..bottom {
        let line = &scene.lines[y as usize];
        let cy = if dctx.flags & REDRAW_STATUS_TOP != 0 {
            dctx.status_lines.wrapping_add(y as u_int)
        } else {
            y as u_int
        };
        if flags & REDRAW_PANE != 0 {
            for span in line[REDRAW_SPAN_PANE as usize].iter() {
                if span.data.pane().wp.ptr_eq(&std::rc::Rc::downgrade(wp)) {
                    redraw_draw_span(dctx, span, cy);
                }
            }
        }
        if flags & REDRAW_PANE_SCROLLBAR != 0 {
            for span in line[REDRAW_SPAN_SCROLLBAR as usize].iter() {
                if span.data.scrollbar().wp.ptr_eq(&std::rc::Rc::downgrade(wp)) {
                    redraw_draw_span(dctx, span, cy);
                }
            }
        }
    }
}
unsafe fn redraw_draw_lines(dctx: &mut redraw_draw_ctx<'_>, flags: ::core::ffi::c_int) {
    let scene = dctx.scene;
    let masks = [
        REDRAW_PANE,
        REDRAW_OUTSIDE,
        REDRAW_EMPTY,
        REDRAW_PANE_STATUS,
        REDRAW_PANE_BORDER,
        REDRAW_PANE_SCROLLBAR,
        REDRAW_MENU,
    ];
    for (y, line) in scene.lines.iter().enumerate() {
        let cy = if dctx.flags & REDRAW_STATUS_TOP != 0 {
            dctx.status_lines.wrapping_add(y as u_int)
        } else {
            y as u_int
        };
        for (spans, mask) in line.iter().zip(masks) {
            if flags != REDRAW_ALL && flags & mask == 0 {
                continue;
            }
            for span in spans.iter() {
                redraw_draw_span(dctx, span, cy);
            }
        }
    }
}
unsafe fn redraw_draw_menu_lines(dctx: &mut redraw_draw_ctx<'_>) {
    let scene = dctx.scene;
    for (y, line) in scene.lines.iter().enumerate() {
        let cy = if dctx.flags & REDRAW_STATUS_TOP != 0 {
            dctx.status_lines.wrapping_add(y as u_int)
        } else {
            y as u_int
        };
        for span in line[REDRAW_SPAN_MENU as usize].iter() {
            redraw_draw_span(dctx, span, cy);
        }
    }
}
/// The scene row of `wp`'s status line, when it has one in view.
unsafe fn redraw_pane_status_line(
    dctx: &redraw_draw_ctx<'_>,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) -> Option<u_int> {
    let scene = dctx.scene;
    let pane_status = wp.border_status();
    if pane_status == PANE_STATUS_OFF {
        return None;
    }
    let (_, sy, _, yoff) = wp.geometry();
    let wy = if pane_status == PANE_STATUS_TOP {
        yoff - 1
    } else {
        yoff + sy as ::core::ffi::c_int
    };
    if wy < 0 || (wy as u_int) < scene.oy || wy as u_int >= scene.oy.wrapping_add(scene.sy) {
        return None;
    }
    Some((wy as u_int).wrapping_sub(scene.oy))
}
/// The width of `wp`'s status line in view, its row's status spans and the
/// first of them that is `wp`'s.
unsafe fn redraw_pane_status_width<'scene>(
    dctx: &redraw_draw_ctx<'scene>,
    wp: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) -> Option<(u_int, &'scene redraw_spans, usize)> {
    let y = redraw_pane_status_line(dctx, wp)?;
    let spans = &dctx.scene.lines[y as usize][REDRAW_SPAN_STATUS as usize];
    let observer = std::rc::Rc::downgrade(wp);
    let mut width = 0;
    let mut first_index = spans.len();
    for (index, span) in spans.iter().enumerate() {
        if span.data.status().wp.ptr_eq(&observer) {
            if first_index == spans.len() {
                first_index = index;
            }
            width = width.max(span.data.status().offset.wrapping_add(span.width));
        }
    }
    Some((width, spans, first_index))
}
/// The line under column `x` of a status line whose spans are `spans` from
/// `span_index`, which moves forward as `x` does. A column without a span
/// draws a horizontal line.
pub fn redraw_get_status_border_cell_type(
    spans: &redraw_spans,
    span_index: &mut usize,
    x: u_int,
) -> ::core::ffi::c_int {
    let mut index = *span_index;
    if index >= spans.len() || spans[index].data.kind() != REDRAW_SPAN_STATUS {
        return CELL_LR;
    }
    let pane = &spans[index].data.status().wp;
    while index < spans.len() {
        let span = spans[index].as_ref();
        if span.data.status().wp.ptr_eq(pane) {
            let start = span.data.status().offset;
            if x >= start && x < start.wrapping_add(span.width) {
                *span_index = index;
                return span.data.status().cell_type;
            }
            if start > x {
                *span_index = index;
                return CELL_LR;
            }
        }
        index += 1;
    }
    *span_index = spans.len();
    CELL_LR
}
unsafe fn redraw_set_draw_context(scene: &redraw_scene) -> Option<redraw_draw_ctx<'_>> {
    let window_owner = scene.w.upgrade()?;
    let client_owner = scene.c.upgrade()?;
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let s = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let mut dctx = redraw_draw_ctx {
        scene,
        active: window_owner.active_pane(),
        marked: std::rc::Weak::new(),
        status_lines: status_line_size(c.as_ref().expect("live client")),
        pane_lines: PANE_LINES_SINGLE,
        default_gc: grid_cell::default(),
        flags: 0,
    };
    if server_is_marked(
        s.as_ref(),
        (s.as_ref().expect("live session").current_winlink()).clone(),
        marked_pane.pane_handle().as_ref(),
    ) != 0
    {
        dctx.marked = marked_pane.wp.clone();
    }
    if options_get_number(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"status-position",
    ) == 0
    {
        dctx.flags |= REDRAW_STATUS_TOP;
    }
    if c.as_ref().expect("live client").flags() & CLIENT_UTF8 as uint64_t != 0 && {
        client_owner.borrow_terminal().has(TTYC_BIDI)
    } {
        dctx.flags |= REDRAW_ISOLATES;
    }
    Some(dctx)
}
unsafe fn redraw_draw_pane_prompt(
    dctx: &mut redraw_draw_ctx<'_>,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    wp_owner.draw_prompt(dctx);
}
unsafe fn redraw_draw(
    client_owner: &ClientRef,
    pane_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
    mut flags: ::core::ffi::c_int,
) {
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let s = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let mut redraw = 0;
    if c.as_ref().expect("live client").flags() & CLIENT_SUSPENDED as uint64_t != 0 {
        return;
    }
    if flags & REDRAW_STATUS != 0 {
        if c.as_ref()
            .expect("live client")
            .status_message_text()
            .0
            .is_some()
        {
            redraw = status_message_redraw(&c.clone().expect("live client"));
        } else if c
            .as_ref()
            .expect("live client")
            .prompt_observer()
            .is_alive()
        {
            redraw = status_prompt_redraw(&c.clone().expect("live client"));
        } else {
            redraw = status_redraw(&c.clone().expect("live client"));
        }
        if redraw == 0 && !(flags == REDRAW_ALL) {
            flags &= !REDRAW_STATUS;
            if flags == 0 as ::core::ffi::c_int {
                return;
            }
        }
    }
    if log_get_level() != 0 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: starting @{} redraw ({})",
            log_cstr(
                (c.as_ref().expect("live client").name())
                    .as_deref()
                    .unwrap_or(c"(null)")
            ),
            {
                (((s.as_ref().expect("live session").current_winlink())
                    .get_unchecked()
                    .window_handle()
                    .as_ref())
                .expect("live window"))
                .id()
            },
            log_cstr(&redraw_flags_to_string(flags))
        ));
    }
    let Some(scene) = redraw_get_scene(client_owner) else {
        return;
    };
    redraw_draw_scene(client_owner, pane_owner, flags, &scene);
    client_owner.restore_redraw_scene(scene);
}

unsafe fn redraw_draw_scene(
    client_owner: &ClientRef,
    pane_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
    mut flags: ::core::ffi::c_int,
    scene: &redraw_scene,
) {
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let wp = pane_owner;
    let Some(window_owner) = scene.w.upgrade() else {
        return;
    };
    let mut _s: Option<SessionRef> = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let mut i: u_int = 0;
    let mut y: u_int = 0;
    let mut lines: u_int = 0;
    let Some(mut dctx) = redraw_set_draw_context(scene) else {
        drop(window_owner);
        return;
    };
    if let Some(menu) = window_owner.menu_observer() {
        menu_update(&mut menu.try_borrow_mut().expect("live unborrowed menu"));
    }
    if flags & (REDRAW_PANE_BORDER | REDRAW_PANE_STATUS) != 0 {
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            pane_owner.reset_border_cache();
        }
    }
    if flags & REDRAW_PANE_STATUS != 0 {
        let mut redraw = false;
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            pane_owner.set_new_status(flags == REDRAW_ALL);
            if let Some((width, status_spans, first_status_span)) =
                redraw_pane_status_width(&dctx, &pane_owner)
            {
                if width != 0
                    && pane_owner.make_status(client_owner, width, status_spans, first_status_span)
                {
                    pane_owner.set_new_status(true);
                    redraw = true;
                }
            }
        }
        if !redraw && flags != REDRAW_ALL {
            flags &= !REDRAW_PANE_STATUS;
            if flags == 0 {
                drop(window_owner);
                return;
            }
        }
    }
    if flags & REDRAW_PANE != 0 {
        if let Some(pane) = pane_owner {
            pane.stop_sync();
            pane.clear_sync_dirty();
        } else {
            for pane in window_owner
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
            {
                pane.stop_sync();
                pane.clear_sync_dirty();
            }
        }
    }
    {
        let terminal = &(client_owner);
        tty_sync_start(terminal);
        tty_update_mode(
            terminal,
            terminal.borrow_terminal().mode() & !CURSOR_MODES,
            None,
        );
    };
    if let Some(wp) = wp {
        redraw_draw_pane_lines(&mut dctx, wp, flags);
    } else {
        redraw_draw_lines(&mut dctx, flags);
    }
    if flags & REDRAW_PANE != 0 {
        if let Some(wp) = wp {
            redraw_draw_pane_prompt(&mut dctx, wp);
        } else {
            for pane_owner in window_owner
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
            {
                redraw_draw_pane_prompt(&mut dctx, &pane_owner);
            }
        }
    }
    if window_owner.menu_observer().is_some() && flags & REDRAW_MENU != 0 {
        redraw_draw_menu_lines(&mut dctx);
    }
    if flags & REDRAW_STATUS != 0 {
        lines = dctx.status_lines;
        if c.as_ref()
            .expect("live client")
            .status_message_text()
            .0
            .is_some()
            || c.as_ref()
                .expect("live client")
                .prompt_observer()
                .is_alive()
        {
            lines = if lines == 0 as u_int {
                1 as u_int
            } else {
                lines
            };
        }
        if dctx.flags & REDRAW_STATUS_TOP != 0 {
            y = 0 as u_int;
        } else {
            y = client_owner.terminal_size().1.wrapping_sub(lines);
        }
        i = 0;
        while i < lines {
            let width = client_owner.terminal_size().0;
            let ranges = tty_check_overlay_range(client_owner, 0, y.wrapping_add(i), width);
            for range in ranges.storage.iter().take(ranges.used as usize) {
                if range.nx != 0 {
                    client_owner.draw_status_line(i, range.px, range.nx, y.wrapping_add(i));
                }
            }
            i = i.wrapping_add(1);
        }
    }
    if flags & REDRAW_OVERLAY != 0 {
        client_owner.draw_overlay();
    }
    {
        let terminal = &(client_owner);
        tty_reset(terminal)
    };
    log_debug(format_args!(
        "{}: finished @{} redraw",
        log_cstr(
            (c.as_ref().expect("live client").name())
                .as_deref()
                .unwrap_or(c"(null)")
        ),
        { (window_owner).id() }
    ));
    drop(window_owner);
}
pub unsafe fn redraw_screen(client_owner: &ClientRef) {
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if c.as_ref().expect("live client").flags() & CLIENT_REDRAWWINDOW as uint64_t != 0 {
        if c.as_ref().expect("live client").flags() & CLIENT_REDRAWOVERLAY as uint64_t != 0 {
            redraw_draw(client_owner, None, REDRAW_ALL);
        } else {
            redraw_draw(client_owner, None, REDRAW_ALL & !REDRAW_OVERLAY);
        }
    } else {
        if c.as_ref().expect("live client").flags() & CLIENT_REDRAWBORDERS as uint64_t != 0 {
            flags |= REDRAW_PANE_BORDER | REDRAW_PANE_STATUS;
        }
        if c.as_ref().expect("live client").flags() & CLIENT_REDRAWSTATUS as uint64_t != 0 {
            flags |= REDRAW_STATUS | REDRAW_PANE_STATUS;
        }
        if c.as_ref().expect("live client").flags() & CLIENT_REDRAWOVERLAY as uint64_t != 0 {
            flags |= REDRAW_OVERLAY;
        }
        if c.as_ref().expect("live client").flags() & CLIENT_REDRAWMENU as uint64_t != 0 {
            flags |= REDRAW_MENU;
        }
        if (c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .expect("live session")
            .current_winlink())
        .get_unchecked()
        .window_handle()
        .as_ref()
        .expect("current window")
        .menu_observer()
        .is_some()
        {
            flags |= REDRAW_MENU;
        }
        if flags != 0 as ::core::ffi::c_int {
            redraw_draw(client_owner, None, flags);
        }
    };
}
pub unsafe fn redraw_pane(
    client_owner: &ClientRef,
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let mut c: Option<ClientRef> = Some(client_owner.clone());
    redraw_draw(
        client_owner,
        Some(pane_owner),
        REDRAW_PANE | REDRAW_PANE_SCROLLBAR,
    );
    if (c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade()
        .expect("live session")
        .current_winlink())
    .get_unchecked()
    .window_handle()
    .as_ref()
    .expect("current window")
    .menu_observer()
    .is_some()
    {
        redraw_draw(client_owner, None, REDRAW_MENU);
    }
}
pub unsafe fn redraw_pane_scrollbar(
    client_owner: &ClientRef,
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    redraw_draw(client_owner, Some(pane_owner), REDRAW_PANE_SCROLLBAR);
}
