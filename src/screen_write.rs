use crate::src::format::bytes::format_message_with;
use crate::src::format_draw::format_draw;
use crate::src::grid::view::{
    grid_view_clear, grid_view_clear_history, grid_view_delete_cells, grid_view_delete_lines,
    grid_view_delete_lines_region, grid_view_get_cell, grid_view_insert_cells,
    grid_view_insert_lines, grid_view_insert_lines_region, grid_view_scroll_region_down,
    grid_view_scroll_region_up, grid_view_set_cell, grid_view_set_cells, grid_view_set_padding,
};
use crate::src::grid::{
    grid_cells_equal, grid_clear_history, grid_default_cell, grid_get_cell, grid_get_line,
    grid_get_line_mut,
};
use crate::src::log::{fatalx, log_bytes, log_debug, log_get_level};
use crate::src::options::options_get_number;
use crate::src::screen::{
    screen_alternate_off, screen_alternate_on, screen_check_selection, screen_mode_display,
    screen_reset_tabs, screen_select_cell,
};
use crate::src::shared::abi::ssize_t;
use crate::src::shared::abi::*;
use crate::src::shared::borders::{
    CELL_BORDERS, CELL_LD, CELL_LR, CELL_LU, CELL_RD, CELL_RU, CELL_UD, CELL_ULD, CELL_URD,
    SIMPLE_BORDERS,
};
use crate::src::shared::display::visible_range;
use crate::src::shared::grid::*;
use crate::src::shared::layout::*;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::menu::menu;
use crate::src::shared::options::options;
use crate::src::shared::pane::window_pane;
use crate::src::shared::screen::{
    screen, EXTENDED_KEY_MODES, MODE_CURSOR, MODE_INSERT, MODE_KEYS_EXTENDED, MODE_ORIGIN,
    MODE_SYNC, MODE_WRAP,
};
use crate::src::shared::screen_write::{
    screen_write_citem, screen_write_cline, screen_write_items, CLEAR,
};
use crate::src::shared::screen_write::{screen_write_ctx, screen_write_init_ctx_cb};
use crate::src::shared::style::*;
use crate::src::shared::tty::{
    tty_command_data, tty_ctx, tty_ctx_redraw_cb, tty_ctx_set_client_cb,
};
use crate::src::shared::tty::{
    TTY_CTX_CELL_INVALIDATE, TTY_CTX_INVISIBLE_PANES, TTY_CTX_OVERLAY_SYNC, TTY_CTX_PANE_OBSCURED,
    TTY_CTX_SYNC, TTY_CTX_WRAPPED,
};
use crate::src::shared::utf8::*;
use crate::src::text::utf8::{utf8_append, utf8_copy, utf8_fromcstr_vec, utf8_open, utf8_set};
use crate::src::text::utf8_combined::{
    hanguljamo_check_state, utf8_has_zwj, utf8_is_hangul_filler, utf8_is_vs, utf8_is_zwj,
    utf8_should_combine,
};
use crate::src::tmux::global_options;
use crate::src::tty::{
    tty_cmd_alignmenttest, tty_cmd_cell, tty_cmd_cells, tty_cmd_clearcharacter,
    tty_cmd_clearendofscreen, tty_cmd_clearscreen, tty_cmd_clearstartofscreen,
    tty_cmd_deletecharacter, tty_cmd_deleteline, tty_cmd_insertcharacter, tty_cmd_insertline,
    tty_cmd_rawstring, tty_cmd_redrawline, tty_cmd_reverseindex, tty_cmd_scrolldown,
    tty_cmd_scrollup, tty_cmd_setselection, tty_cmd_syncstart, tty_write,
};
use crate::src::tty_acs::{tty_acs_double_borders, tty_acs_heavy_borders, tty_acs_rounded_borders};
use crate::src::window::WindowPane;
use crate::src::window_visible::window_position_is_visible;
use std::ffi::CStr;

pub const PADDED_BORDERS: [::core::ffi::c_char; 14] =
    unsafe { ::core::mem::transmute::<[u8; 14], [::core::ffi::c_char; 14]>(*b"             \0") };
pub const SCREEN_WRITE_SYNC: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const SCREEN_WRITE_OBSCURED: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const SCREEN_WRITE_CHECKED_IF_OBSCURED: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
thread_local! {
    // Screen writing runs on the event-loop thread. Detached boxes preserve
    // FIFO reuse without owning raw pointers or a static initializer.
    static WRITE_ITEM_POOL: std::cell::RefCell<std::collections::VecDeque<Box<screen_write_citem>>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}
fn screen_write_get_citem() -> Box<screen_write_citem> {
    let mut item = WRITE_ITEM_POOL
        .with(|pool| pool.borrow_mut().pop_front())
        .unwrap_or_default();
    *item = Default::default();
    item
}
fn screen_write_free_citem(item: Box<screen_write_citem>) {
    // Screens may drop during thread teardown, after the pool is gone.
    let _ = WRITE_ITEM_POOL.try_with(|pool| pool.borrow_mut().push_back(item));
}
fn screen_write_current_item(ctx: &mut screen_write_ctx) -> &mut screen_write_citem {
    ctx.item
        .as_deref_mut()
        .expect("active screen write context")
}
fn screen_write_recycle_items(items: &mut screen_write_items) {
    while let Some(item) = items.pop_front() {
        screen_write_free_citem(item);
    }
}
unsafe fn screen_write_set_cursor(
    ctx: &mut screen_write_ctx,
    mut cx: ::core::ffi::c_int,
    mut cy: ::core::ffi::c_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let wp = write_pane;
    let mut s: *mut screen = ctx.screen_ptr();
    if cx != -(1 as ::core::ffi::c_int)
        && cx as u_int == (*s).cx
        && cy != -(1 as ::core::ffi::c_int)
        && cy as u_int == (*s).cy
    {
        return;
    }
    if cx != -(1 as ::core::ffi::c_int) {
        if cx as u_int > (*s).grid().sx {
            cx = (*s).grid().sx.wrapping_sub(1 as u_int) as ::core::ffi::c_int;
        }
        (*s).cx = cx as u_int;
    }
    if cy != -(1 as ::core::ffi::c_int) {
        if cy as u_int > (*s).grid().sy.wrapping_sub(1 as u_int) {
            cy = (*s).grid().sy.wrapping_sub(1 as u_int) as ::core::ffi::c_int;
        }
        (*s).cy = cy as u_int;
    }
    if wp.is_none() {
        return;
    }
    wp.expect("write pane").schedule_offset_update();
}
fn screen_write_redraw_cb(
    observer: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
) -> tty_ctx_redraw_cb {
    let observer = observer.clone();
    Some(Box::new(move |_| unsafe {
        if let Some(owner) = observer.upgrade() {
            use crate::src::window::WindowPane;
            owner.request_redraw(false);
        }
    }))
}
fn screen_write_set_client_cb(
    observer: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
) -> tty_ctx_set_client_cb {
    use crate::src::server_client::Client;
    let observer = observer.clone();
    Some(Box::new(move |ttyctx, client| unsafe {
        let Some(pane) = observer.upgrade() else {
            return 0;
        };
        client.prepare_pane_render(ttyctx, &pane)
    }))
}

unsafe fn screen_write_pane_is_obscured(ctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    let Some(pane) = ctx.wp.upgrade() else {
        return 0;
    };
    if ctx.flags & SCREEN_WRITE_CHECKED_IF_OBSCURED == 0 {
        ctx.flags |= SCREEN_WRITE_CHECKED_IF_OBSCURED;
        if pane.is_obscured() {
            ctx.flags |= SCREEN_WRITE_OBSCURED;
        }
    }
    (ctx.flags & SCREEN_WRITE_OBSCURED != 0) as i32
}
unsafe fn screen_write_should_draw_lines(ctx: &mut screen_write_ctx, y: u_int, ny: u_int) -> i32 {
    let screen = &*ctx.screen_ptr();
    let synchronized = screen.mode & MODE_SYNC != 0;
    let height = screen.grid().sy;
    match ctx.wp.upgrade() {
        Some(pane) => pane.should_draw_rows(synchronized, y, ny, height) as i32,
        None => (!synchronized) as i32,
    }
}
unsafe fn screen_write_should_draw_line(
    ctx: &mut screen_write_ctx,
    mut y: u_int,
) -> ::core::ffi::c_int {
    screen_write_should_draw_lines(ctx, y, 1 as u_int)
}
pub(crate) unsafe fn screen_write_initctx(
    ctx: &mut screen_write_ctx,
    ttyctx: &mut tty_ctx,
    mut is_sync: ::core::ffi::c_int,
    mut check_obscured: ::core::ffi::c_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut s: *mut screen = ctx.screen_ptr();
    *ttyctx = tty_ctx::default();
    ttyctx.sx = (*s).grid().sx;
    ttyctx.sy = (*s).grid().sy;
    ttyctx.ocx = (*s).cx;
    ttyctx.ocy = (*s).cy;
    ttyctx.orlower = (*s).rlower;
    ttyctx.orupper = (*s).rupper;
    if check_obscured != 0 && screen_write_pane_is_obscured(ctx) != 0 {
        ttyctx.flags |= TTY_CTX_PANE_OBSCURED;
    }
    ttyctx.style_ctx.defaults = grid_default_cell;
    ttyctx.style_ctx.hyperlinks = (*ctx.screen_ptr()).hyperlinks.clone();
    if let Some(callback) = ctx.init_ctx_cb.as_mut() {
        callback(ttyctx);
        if let Some((fg, bg)) = ttyctx
            .style_ctx
            .palette
            .with_palette(|palette| palette.map(|palette| (palette.fg, palette.bg)))
        {
            if ttyctx.style_ctx.defaults.fg == 8 as ::core::ffi::c_int {
                ttyctx.style_ctx.defaults.fg = fg;
            }
            if ttyctx.style_ctx.defaults.bg == 8 as ::core::ffi::c_int {
                ttyctx.style_ctx.defaults.bg = bg;
            }
        }
    } else {
        ttyctx.redraw_cb = screen_write_redraw_cb(&ctx.wp);
        if let Some(write_pane_value) = write_pane {
            (ttyctx.style_ctx.defaults, ttyctx.style_ctx.dim) = write_pane_value.default_colours();
            ttyctx.style_ctx.palette = crate::src::shared::tty::PaletteSource::Pane(
                std::rc::Rc::downgrade(write_pane_value),
            );
            ttyctx.set_client_cb = screen_write_set_client_cb(&ctx.wp);
        }
    }
    if !ctx.flags & SCREEN_WRITE_SYNC != 0 {
        if write_pane.is_some_and(|pane| pane.draws_inactive_screen()) {
            ttyctx.flags |= TTY_CTX_SYNC;
        } else {
            if write_pane.is_none() {
                ttyctx.flags |= TTY_CTX_OVERLAY_SYNC;
            }
            if is_sync != 0 {
                ttyctx.flags |= TTY_CTX_SYNC;
            }
        }
        tty_write(|tty, ctx| tty_cmd_syncstart(tty, ctx), ttyctx);
        ctx.flags |= SCREEN_WRITE_SYNC;
    }
}
pub unsafe fn screen_write_make_list(s: &mut screen) {
    if s.grid().sy == 0 {
        fatalx(|out| out.write_all(b"xcalloc: zero size"));
    }
    let rows = (0..s.grid().sy)
        .map(|_| screen_write_cline::default())
        .collect::<Vec<_>>()
        .into_boxed_slice();
    s.write_list = Some(rows);
}
pub unsafe fn screen_write_free_list(s: &mut screen) {
    let mut rows = s
        .write_list
        .take()
        .expect("screen write rows are initialized");
    for row in rows.iter_mut() {
        screen_write_recycle_items(&mut row.items);
    }
}

pub(crate) unsafe fn screen_write_init(ctx: &mut screen_write_ctx, mut s: *mut screen) {
    // Callers provide initialized Rust storage and stop an active context before reuse.
    *ctx = screen_write_ctx::default();
    ctx.borrow_screen(s);
    if (*ctx.screen_ptr()).write_list.is_none() {
        screen_write_make_list(&mut *ctx.screen_ptr());
    }
    ctx.item = Some(screen_write_get_citem());
    ctx.scrolled = 0 as u_int;
    ctx.bg = 8 as u_int;
}
pub unsafe fn screen_write_start_pane(
    ctx: &mut screen_write_ctx,
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut s: *mut screen,
) {
    pane_owner.prepare_write(ctx, s);
    if log_get_level() != 0 {
        let (_, _, x, y) = pane_owner.geometry();
        log_debug(format_args!(
            "screen_write_start_pane: size {}x{}, pane %{} (at {},{})",
            (*ctx.screen_ptr()).grid().sx,
            (*ctx.screen_ptr()).grid().sy,
            pane_owner.id(),
            x,
            y
        ));
    }
}
pub unsafe fn screen_write_start_callback(
    ctx: &mut screen_write_ctx,
    mut s: *mut screen,
    mut cb: screen_write_init_ctx_cb,
) {
    screen_write_init(ctx, s);
    ctx.init_ctx_cb = cb;
    if log_get_level() != 0 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: size {}x{}, with callback",
            "screen_write_start_callback",
            { (*ctx.screen_ptr()).grid().sx },
            { (*ctx.screen_ptr()).grid().sy }
        ));
    }
}
pub unsafe fn screen_write_start(ctx: &mut screen_write_ctx, mut s: *mut screen) {
    screen_write_init(ctx, s);
    if log_get_level() != 0 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: size {}x{}, no pane",
            "screen_write_start",
            { (*ctx.screen_ptr()).grid().sx },
            { (*ctx.screen_ptr()).grid().sy }
        ));
    }
}
pub unsafe fn screen_write_stop(ctx: &mut screen_write_ctx) {
    screen_write_collect_end(ctx);
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_stop");
    screen_write_free_citem(ctx.item.take().expect("active screen write context"));
    ctx.target = Default::default();
}
pub unsafe fn screen_write_reset(ctx: &mut screen_write_ctx) {
    let mut s: *mut screen = ctx.screen_ptr();
    screen_reset_tabs(&mut *s);
    screen_write_scrollregion(ctx, 0 as u_int, (*s).grid().sy.wrapping_sub(1 as u_int));
    (*s).mode = MODE_CURSOR | MODE_WRAP;
    if options_get_number(global_options, c"extended-keys") == 2 as ::core::ffi::c_longlong {
        (*s).mode = (*s).mode & !EXTENDED_KEY_MODES | MODE_KEYS_EXTENDED;
    }
    screen_write_clearscreen(ctx, 8 as u_int);
    screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
}
pub unsafe fn screen_write_putc(ctx: &mut screen_write_ctx, gcp: &grid_cell, ch: u_char) {
    let mut gc = *gcp;
    utf8_set(&mut gc.data, ch);
    screen_write_cell(ctx, &gc);
}
/// Measure literal C-string bytes using the screen's display-width rules.
pub unsafe fn screen_write_strlen(msg: &CStr) -> size_t {
    let bytes = msg.to_bytes();
    let mut offset = 0;
    let mut size: size_t = 0;
    let mut ud = utf8_data::default();
    while let Some(&byte) = bytes.get(offset) {
        if byte > 0x7f && utf8_open(&mut ud, byte) == UTF8_MORE {
            offset += 1;
            if bytes.len() - offset < ud.size as usize - 1 {
                break;
            }
            let more = loop {
                let more = utf8_append(&mut ud, bytes[offset]);
                offset += 1;
                if more != UTF8_MORE {
                    break more;
                }
            };
            if more == UTF8_DONE {
                size = size.wrapping_add(ud.width as usize);
            }
        } else {
            if byte == b'\t' || (0x20..0x7f).contains(&byte) {
                size = size.wrapping_add(1);
            }
            offset += 1;
        }
    }
    size
}
pub unsafe fn screen_write_text(
    ctx: &mut screen_write_ctx,
    mut cx: u_int,
    mut width: u_int,
    mut lines: u_int,
    mut more: ::core::ffi::c_int,
    gcp: &grid_cell,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> ::core::ffi::c_int {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut cy: u_int = (*s).cy;
    let mut i: u_int = 0;
    let mut end: u_int = 0;
    let mut next: u_int = 0;
    let mut idx: u_int = 0 as u_int;
    let mut at: u_int = 0;
    let mut left: u_int = 0;
    let mut gc = *gcp;
    let cells = {
        let tmp = format_message_with(write);
        utf8_fromcstr_vec(tmp.as_c_str())
    };
    left = cx.wrapping_add(width).wrapping_sub((*s).cx);
    loop {
        at = 0 as u_int;
        end = idx;
        while cells[end as usize].size as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
            if cells[end as usize].size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                && cells[end as usize].data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int
                    == '\n' as i32
            {
                break;
            }
            if at.wrapping_add(cells[end as usize].width as u_int) > left {
                break;
            }
            at = at.wrapping_add(cells[end as usize].width as u_int);
            end = end.wrapping_add(1);
        }
        if cells[end as usize].size as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            next = end;
        } else if cells[end as usize].size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
            && cells[end as usize].data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int
                == '\n' as i32
            || (cells[end as usize].size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                && cells[end as usize].data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int
                    == ' ' as i32)
        {
            next = end.wrapping_add(1 as u_int);
        } else {
            i = end;
            while i > idx {
                if cells[i as usize].size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                    && cells[i as usize].data[0 as ::core::ffi::c_int as usize]
                        as ::core::ffi::c_int
                        == ' ' as i32
                {
                    break;
                }
                i = i.wrapping_sub(1);
            }
            if i != idx {
                next = i.wrapping_add(1 as u_int);
                end = i;
            } else {
                next = end;
            }
        }
        i = idx;
        while i < end {
            gc.data = utf8_copy(&cells[i as usize]);
            screen_write_cell(ctx, &gc);
            i = i.wrapping_add(1);
        }
        idx = next;
        if (*s).cy == cy.wrapping_add(lines).wrapping_sub(1 as u_int)
            || cells[idx as usize].size as ::core::ffi::c_int == 0 as ::core::ffi::c_int
        {
            break;
        }
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        left = width;
    }
    if (*s).cy == cy.wrapping_add(lines).wrapping_sub(1 as u_int)
        && (more == 0 || (*s).cx == cx.wrapping_add(width))
        || cells[idx as usize].size as ::core::ffi::c_int != 0 as ::core::ffi::c_int
    {
        return 0 as ::core::ffi::c_int;
    }
    if more == 0 || (*s).cx == cx.wrapping_add(width) {
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
    }
    1 as ::core::ffi::c_int
}

pub unsafe fn screen_write_puts(
    ctx: &mut screen_write_ctx,
    gcp: &grid_cell,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    screen_write_nputs(ctx, -1, gcp, write);
}
pub unsafe fn screen_write_nputs(
    ctx: &mut screen_write_ctx,
    maxlen: ssize_t,
    gcp: &grid_cell,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut gc = *gcp;
    let mut size: size_t = 0;
    let msg = format_message_with(write);
    let bytes = msg.as_bytes();
    let mut offset = 0;
    while let Some(&byte) = bytes.get(offset) {
        if byte > 0x7f && utf8_open(&mut gc.data, byte) == UTF8_MORE {
            offset += 1;
            if bytes.len() - offset < gc.data.size as usize - 1 {
                break;
            }
            let more = loop {
                let more = utf8_append(&mut gc.data, bytes[offset]);
                offset += 1;
                if more != UTF8_MORE {
                    break more;
                }
            };
            if more != UTF8_DONE {
                continue;
            }
            if maxlen > 0 && size.wrapping_add(gc.data.width as usize) > maxlen as usize {
                while size < maxlen as usize {
                    screen_write_putc(ctx, &gc, b' ');
                    size = size.wrapping_add(1);
                }
                break;
            }
            size = size.wrapping_add(gc.data.width as usize);
            screen_write_cell(ctx, &gc);
        } else {
            if maxlen > 0 && size.wrapping_add(1) > maxlen as usize {
                break;
            }
            if byte == 1 {
                gc.attr ^= GRID_ATTR_CHARSET as u_short;
            } else if byte == b'\n' {
                screen_write_linefeed(ctx, 0, 8);
                screen_write_carriagereturn(ctx);
            } else if byte == b'\t' || (0x20..0x7f).contains(&byte) {
                size = size.wrapping_add(1);
                screen_write_putc(ctx, &gc, byte);
            }
            offset += 1;
        }
    }
}
pub unsafe fn screen_write_fast_copy(
    ctx: &mut screen_write_ctx,
    src: &screen,
    mut px: u_int,
    mut py: u_int,
    mut nx: u_int,
    mut ny: u_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let wp = write_pane;
    let mut ttyctx = tty_ctx::default();
    let gd = src.grid();
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
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    let mut xoff: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut yoff: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if nx == 0 as u_int || ny == 0 as u_int {
        return;
    }
    if let Some(wp_value) = wp {
        xoff = wp_value.geometry().2;
        yoff = wp_value.geometry().3;
    }
    yy = py;
    while yy < py.wrapping_add(ny) {
        if yy >= gd.hsize.wrapping_add(gd.sy) {
            break;
        }
        (*s).cx = cx;
        screen_write_initctx(
            ctx,
            &mut ttyctx,
            0 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        WindowPane::visible_ranges(
            wp,
            (xoff as u_int).wrapping_add((*s).cx) as ::core::ffi::c_int,
            (*s).cy.wrapping_add(yoff as u_int) as ::core::ffi::c_int,
            nx,
            &mut r,
        );
        xx = px;
        while xx < px.wrapping_add(nx) {
            let gl = grid_get_line(gd, yy);
            let sgl = grid_get_line((*s).grid(), (*s).cy);
            if xx >= gl.cellsize as u_int && (*s).cx >= sgl.cellsize as u_int {
                break;
            }
            grid_get_cell(gd, xx, yy, &mut gc);
            if xx.wrapping_add(gc.data.width as u_int) > px.wrapping_add(nx) {
                break;
            }
            grid_view_set_cell((*s).grid_mut(), (*s).cx, (*s).cy, &gc);
            if !window_position_is_visible(&r, (xoff as u_int).wrapping_add((*s).cx)) {
                break;
            }
            ttyctx.flags &= TTY_CTX_OVERLAY_SYNC | TTY_CTX_SYNC;
            tty_write(|tty, ctx| tty_cmd_cell(tty, ctx, &*s, &gc), &mut ttyctx);
            ttyctx.ocx = ttyctx.ocx.wrapping_add(1);
            (*s).cx = (*s).cx.wrapping_add(1);
            xx = xx.wrapping_add(1);
        }
        (*s).cy = (*s).cy.wrapping_add(1);
        yy = yy.wrapping_add(1);
    }
    (*s).cx = cx;
    (*s).cy = cy;
}
fn screen_write_box_border_set(
    mut lines: box_lines,
    mut cell_type: ::core::ffi::c_int,
    gc: &mut grid_cell,
) {
    match lines as ::core::ffi::c_int {
        1 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            gc.data = utf8_copy(tty_acs_double_borders(cell_type));
        }
        2 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            gc.data = utf8_copy(tty_acs_heavy_borders(cell_type));
        }
        4 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            gc.data = utf8_copy(tty_acs_rounded_borders(cell_type));
        }
        3 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            utf8_set(&mut gc.data, SIMPLE_BORDERS[cell_type as usize] as u_char);
        }
        5 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            utf8_set(&mut gc.data, PADDED_BORDERS[cell_type as usize] as u_char);
        }
        0 | -1 => {
            gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
            utf8_set(&mut gc.data, CELL_BORDERS[cell_type as usize] as u_char);
        }
        _ => {}
    };
}
pub unsafe fn screen_write_hline(
    ctx: &mut screen_write_ctx,
    mut nx: u_int,
    mut left: ::core::ffi::c_int,
    mut right: ::core::ffi::c_int,
    mut lines: box_lines,
    border_gc: Option<&grid_cell>,
) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut gc = border_gc.copied().unwrap_or(grid_default_cell);
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut i: u_int = 0;
    cx = (*s).cx;
    cy = (*s).cy;
    gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
    if left != 0 {
        screen_write_box_border_set(lines, CELL_URD, &mut gc);
    } else {
        screen_write_box_border_set(lines, CELL_LR, &mut gc);
    }
    screen_write_cell(ctx, &gc);
    screen_write_box_border_set(lines, CELL_LR, &mut gc);
    i = 1 as u_int;
    while i < nx.wrapping_sub(1 as u_int) {
        screen_write_cell(ctx, &gc);
        i = i.wrapping_add(1);
    }
    if right != 0 {
        screen_write_box_border_set(lines, CELL_ULD, &mut gc);
    } else {
        screen_write_box_border_set(lines, CELL_LR, &mut gc);
    }
    screen_write_cell(ctx, &gc);
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_vline(
    ctx: &mut screen_write_ctx,
    mut ny: u_int,
    gcp: Option<&grid_cell>,
) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut gc = gcp.copied().unwrap_or(grid_default_cell);
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut i: u_int = 0;
    cx = (*s).cx;
    cy = (*s).cy;
    gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
    screen_write_putc(ctx, &gc, ('x' as i32) as u_char);
    i = 1 as u_int;
    while i < ny.wrapping_sub(1 as u_int) {
        screen_write_set_cursor(
            ctx,
            cx as ::core::ffi::c_int,
            cy.wrapping_add(i) as ::core::ffi::c_int,
        );
        screen_write_putc(ctx, &gc, 'x' as i32 as u_char);
        i = i.wrapping_add(1);
    }
    screen_write_set_cursor(
        ctx,
        cx as ::core::ffi::c_int,
        cy.wrapping_add(ny).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
    );
    screen_write_putc(ctx, &gc, ('x' as i32) as u_char);
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_menu(
    ctx: &mut screen_write_ctx,
    menu: &menu,
    choice: ::core::ffi::c_int,
    lines: box_lines,
    menu_gc: &grid_cell,
    border_gc: Option<&grid_cell>,
    choice_gc: &grid_cell,
) {
    let cx = (*ctx.screen_ptr()).cx;
    let cy = (*ctx.screen_ptr()).cy;
    let width = menu.width;
    let mut default_gc = *menu_gc;
    screen_write_box(
        ctx,
        width.wrapping_add(4),
        menu.count().wrapping_add(2),
        lines,
        border_gc,
        Some(menu.title.as_c_str()),
    );
    for (index, item) in menu.items.iter().enumerate() {
        let y = cy.wrapping_add(1).wrapping_add(index as u_int);
        let Some(name) = item.name.as_deref() else {
            screen_write_cursormove(ctx, cx as ::core::ffi::c_int, y as ::core::ffi::c_int, 0);
            screen_write_hline(ctx, width.wrapping_add(4), 1, 1, lines, border_gc);
            continue;
        };
        let disabled = name.to_bytes().first() == Some(&b'-');
        let gc = if choice >= 0 && index == choice as usize && !disabled {
            choice_gc
        } else {
            &default_gc
        };
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(1) as ::core::ffi::c_int,
            y as ::core::ffi::c_int,
            0,
        );
        for _ in 0..width.wrapping_add(2) {
            screen_write_putc(ctx, gc, b' ');
        }
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(2) as ::core::ffi::c_int,
            y as ::core::ffi::c_int,
            0,
        );
        if disabled {
            default_gc.attr |= GRID_ATTR_DIM as u_short;
            let name = CStr::from_bytes_with_nul(&name.to_bytes_with_nul()[1..])
                .expect("removing the disabled marker preserves the terminator");
            format_draw(
                ctx,
                &default_gc,
                width,
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
            );
            default_gc.attr &= !(GRID_ATTR_DIM as u_short);
        } else {
            format_draw(ctx, gc, width, name.as_ptr(), std::ptr::null_mut(), 0);
        }
    }
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_box(
    ctx: &mut screen_write_ctx,
    mut nx: u_int,
    mut ny: u_int,
    mut lines: box_lines,
    gcp: Option<&grid_cell>,
    title: Option<&CStr>,
) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut gc = gcp.copied().unwrap_or(grid_default_cell);
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut i: u_int = 0;
    cx = (*s).cx;
    cy = (*s).cy;
    gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
    gc.flags = (gc.flags as ::core::ffi::c_int | GRID_FLAG_NOPALETTE) as u_char;
    screen_write_box_border_set(lines, CELL_RD, &mut gc);
    screen_write_cell(ctx, &gc);
    screen_write_box_border_set(lines, CELL_LR, &mut gc);
    i = 1 as u_int;
    while i < nx.wrapping_sub(1 as u_int) {
        screen_write_cell(ctx, &gc);
        i = i.wrapping_add(1);
    }
    screen_write_box_border_set(lines, CELL_LD, &mut gc);
    screen_write_cell(ctx, &gc);
    screen_write_set_cursor(
        ctx,
        cx as ::core::ffi::c_int,
        cy.wrapping_add(ny).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
    );
    screen_write_box_border_set(lines, CELL_RU, &mut gc);
    screen_write_cell(ctx, &gc);
    screen_write_box_border_set(lines, CELL_LR, &mut gc);
    i = 1 as u_int;
    while i < nx.wrapping_sub(1 as u_int) {
        screen_write_cell(ctx, &gc);
        i = i.wrapping_add(1);
    }
    screen_write_box_border_set(lines, CELL_LU, &mut gc);
    screen_write_cell(ctx, &gc);
    screen_write_box_border_set(lines, CELL_UD, &mut gc);
    i = 1 as u_int;
    while i < ny.wrapping_sub(1 as u_int) {
        screen_write_set_cursor(
            ctx,
            cx as ::core::ffi::c_int,
            cy.wrapping_add(i) as ::core::ffi::c_int,
        );
        screen_write_cell(ctx, &gc);
        screen_write_set_cursor(
            ctx,
            cx.wrapping_add(nx).wrapping_sub(1 as u_int) as ::core::ffi::c_int,
            cy.wrapping_add(i) as ::core::ffi::c_int,
        );
        screen_write_cell(ctx, &gc);
        i = i.wrapping_add(1);
    }
    if let Some(title) = title {
        gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
        screen_write_cursormove(
            ctx,
            cx.wrapping_add(2 as u_int) as ::core::ffi::c_int,
            cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        format_draw(
            ctx,
            &gc,
            nx.wrapping_sub(4 as u_int),
            title.as_ptr(),
            ::core::ptr::null_mut::<style_ranges>(),
            0 as ::core::ffi::c_int,
        );
    }
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_preview(
    ctx: &mut screen_write_ctx,
    src: &screen,
    mut nx: u_int,
    mut ny: u_int,
) {
    let mut s: *mut screen = ctx.screen_ptr();
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
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    cx = (*s).cx;
    cy = (*s).cy;
    if src.mode & MODE_CURSOR != 0 {
        px = src.cx;
        if px < nx.wrapping_div(3 as u_int) {
            px = 0 as u_int;
        } else {
            px = px.wrapping_sub(nx.wrapping_div(3 as u_int));
        }
        if px.wrapping_add(nx) > src.grid().sx {
            if nx > src.grid().sx {
                px = 0 as u_int;
            } else {
                px = src.grid().sx.wrapping_sub(nx);
            }
        }
        py = src.cy;
        if py < ny.wrapping_div(3 as u_int) {
            py = 0 as u_int;
        } else {
            py = py.wrapping_sub(ny.wrapping_div(3 as u_int));
        }
        if py.wrapping_add(ny) > src.grid().sy {
            if ny > src.grid().sy {
                py = 0 as u_int;
            } else {
                py = src.grid().sy.wrapping_sub(ny);
            }
        }
    } else {
        px = 0 as u_int;
        py = 0 as u_int;
    }
    screen_write_fast_copy(ctx, src, px, src.grid().hsize.wrapping_add(py), nx, ny);
    if src.mode & MODE_CURSOR != 0 {
        grid_view_get_cell(src.grid(), src.cx, src.cy, &mut gc);
        gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_REVERSE) as u_short;
        screen_write_set_cursor(
            ctx,
            cx.wrapping_add(src.cx.wrapping_sub(px)) as ::core::ffi::c_int,
            cy.wrapping_add(src.cy.wrapping_sub(py)) as ::core::ffi::c_int,
        );
        screen_write_cell(ctx, &gc);
    }
}
pub unsafe fn screen_write_mode_set(ctx: &mut screen_write_ctx, mut mode: ::core::ffi::c_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    (*s).mode |= mode;
    if log_get_level() != 0 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: {}",
            "screen_write_mode_set",
            screen_mode_display(mode)
        ));
    }
}
pub unsafe fn screen_write_mode_clear(ctx: &mut screen_write_ctx, mut mode: ::core::ffi::c_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    (*s).mode &= !mode;
    if log_get_level() != 0 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: {}",
            "screen_write_mode_clear",
            screen_mode_display(mode)
        ));
    }
}
pub unsafe fn screen_write_end_sync(ctx: &mut screen_write_ctx) {
    let Some(pane) = ctx.wp.upgrade() else { return };
    if pane.is_synchronized() {
        screen_write_collect_flush(ctx, 0, "screen_write_end_sync");
    }
    pane.stop_sync();
}
pub unsafe fn screen_write_cursorup(ctx: &mut screen_write_ctx, mut ny: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    if ny == 0 as u_int {
        ny = 1 as u_int;
    }
    if cy < (*s).rupper {
        if ny > cy {
            ny = cy;
        }
    } else if ny > cy.wrapping_sub((*s).rupper) {
        ny = cy.wrapping_sub((*s).rupper);
    }
    if cx == (*s).grid().sx {
        cx = cx.wrapping_sub(1);
    }
    cy = cy.wrapping_sub(ny);
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_cursordown(ctx: &mut screen_write_ctx, mut ny: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    if ny == 0 as u_int {
        ny = 1 as u_int;
    }
    if cy > (*s).rlower {
        if ny > (*s).grid().sy.wrapping_sub(1 as u_int).wrapping_sub(cy) {
            ny = (*s).grid().sy.wrapping_sub(1 as u_int).wrapping_sub(cy);
        }
    } else if ny > (*s).rlower.wrapping_sub(cy) {
        ny = (*s).rlower.wrapping_sub(cy);
    }
    if cx == (*s).grid().sx {
        cx = cx.wrapping_sub(1);
    } else if ny == 0 as u_int {
        return;
    }
    cy = cy.wrapping_add(ny);
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_cursorright(ctx: &mut screen_write_ctx, mut nx: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    if nx == 0 as u_int {
        nx = 1 as u_int;
    }
    if nx > (*s).grid().sx.wrapping_sub(1 as u_int).wrapping_sub(cx) {
        nx = (*s).grid().sx.wrapping_sub(1 as u_int).wrapping_sub(cx);
    }
    if nx == 0 as u_int {
        return;
    }
    cx = cx.wrapping_add(nx);
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_cursorleft(ctx: &mut screen_write_ctx, mut nx: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    if nx == 0 as u_int {
        nx = 1 as u_int;
    }
    if nx > cx {
        nx = cx;
    }
    if nx == 0 as u_int {
        return;
    }
    cx = cx.wrapping_sub(nx);
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_backspace(ctx: &mut screen_write_ctx) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    if cx == 0 as u_int {
        if cy == 0 as u_int {
            return;
        }
        let gl = grid_get_line(
            (*s).grid(),
            (*s).grid().hsize.wrapping_add(cy).wrapping_sub(1 as u_int),
        );
        if gl.flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0 {
            cy = cy.wrapping_sub(1);
            cx = (*s).grid().sx.wrapping_sub(1 as u_int);
        }
    } else {
        cx = cx.wrapping_sub(1);
    }
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
}
fn screen_write_cell_is_single(gc: &grid_cell) -> ::core::ffi::c_int {
    (gc.data.width == 1
        && gc.data.size == 1
        && gc.data.data[0] >= 0x20
        && gc.data.data[0] != 0x7f
        && gc.flags as ::core::ffi::c_int & (GRID_FLAG_CLEARED | GRID_FLAG_PADDING | GRID_FLAG_TAB)
            == 0) as ::core::ffi::c_int
}
pub(crate) unsafe fn screen_write_redraw_line(
    ctx: &mut screen_write_ctx,
    ttyctx: &mut tty_ctx,
    mut yy: u_int,
    r: &mut Vec<visible_range>,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let wp = write_pane;
    let mut s: *mut screen = ctx.screen_ptr();
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
    let mut ngc: grid_cell = grid_cell {
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
    let mut cx: u_int = 0;
    let mut i: u_int = 0;
    let mut xoff: ::core::ffi::c_int = wp.expect("write pane").geometry().2;
    let mut yoff: ::core::ffi::c_int = wp.expect("write pane").geometry().3;
    WindowPane::visible_ranges(
        wp,
        xoff,
        (yoff as u_int).wrapping_add(yy) as ::core::ffi::c_int,
        sx,
        r,
    );
    i = 0 as u_int;
    while (i as usize) < r.len() {
        let ri = &r[i as usize];
        if !(ri.nx == 0 as u_int) {
            cx = ri.px.wrapping_sub(xoff as u_int);
            if !(cx >= sx) {
                if cx.wrapping_add(ri.nx) > sx {
                    ttyctx.data = tty_command_data::Count(sx.wrapping_sub(cx));
                } else {
                    ttyctx.data = tty_command_data::Count(ri.nx);
                }
                if !(ttyctx.data.count() == 0 as u_int) {
                    ttyctx.ocx = cx;
                    ttyctx.ocy = yy;
                    if ttyctx.data.count() != 1 as u_int {
                        tty_write(|tty, ctx| tty_cmd_redrawline(tty, ctx, &*s), ttyctx);
                    } else {
                        grid_view_get_cell((*s).grid(), cx, yy, &mut gc);
                        if screen_write_cell_is_single(&gc) == 0 {
                            tty_write(|tty, ctx| tty_cmd_redrawline(tty, ctx, &*s), ttyctx);
                        } else {
                            let cell =
                                if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_SELECTED != 0 {
                                    &gc
                                } else {
                                    if let Some(selected) = screen_select_cell(&*s, &gc) {
                                        ngc = selected;
                                    }
                                    &ngc
                                };
                            tty_write(|tty, ctx| tty_cmd_cell(tty, ctx, &*s, cell), ttyctx);
                        }
                    }
                }
            }
        }
        i = i.wrapping_add(1);
    }
}

unsafe fn screen_write_redraw_pane(ctx: &mut screen_write_ctx, ttyctx: &mut tty_ctx) {
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut yy: u_int = 0;
    yy = 0 as u_int;
    while yy < (*s).grid().sy {
        screen_write_redraw_line(ctx, ttyctx, yy, &mut r);
        yy = yy.wrapping_add(1);
    }
}
pub unsafe fn screen_write_alignmenttest(ctx: &mut screen_write_ctx) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
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
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    gc = grid_default_cell;
    utf8_set(&mut gc.data, 'E' as i32 as u_char);
    yy = 0 as u_int;
    while yy < (*s).grid().sy {
        xx = 0 as u_int;
        while xx < (*s).grid().sx {
            grid_view_set_cell(&mut *((*s).grid_mut()), xx, yy, &gc);
            xx = xx.wrapping_add(1);
        }
        yy = yy.wrapping_add(1);
    }
    screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
    (*s).rupper = 0 as u_int;
    (*s).rlower = (*s).grid().sy.wrapping_sub(1 as u_int);
    screen_write_collect_clear(ctx, 0 as u_int, (*s).grid().sy.wrapping_sub(1 as u_int));
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    if screen_write_should_draw_lines(ctx, 0 as u_int, (*s).grid().sy) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(|tty, ctx| tty_cmd_alignmenttest(tty, ctx), &mut ttyctx);
        return;
    }
    screen_write_redraw_pane(ctx, &mut ttyctx);
}
pub unsafe fn screen_write_insertcharacter(
    ctx: &mut screen_write_ctx,
    mut nx: u_int,
    mut bg: u_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    if nx == 0 as u_int {
        nx = 1 as u_int;
    }
    if nx > (*s).grid().sx.wrapping_sub((*s).cx) {
        nx = (*s).grid().sx.wrapping_sub((*s).cx);
    }
    if nx == 0 as u_int {
        return;
    }
    if (*s).cx > (*s).grid().sx.wrapping_sub(1 as u_int) {
        return;
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    grid_view_insert_cells((*s).grid_mut(), (*s).cx, (*s).cy, nx, bg);
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_insertcharacter");
    ttyctx.data = tty_command_data::Count(nx);
    if screen_write_should_draw_line(ctx, (*s).cy) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(
            |tty, ctx| tty_cmd_insertcharacter(tty, ctx, &*s),
            &mut ttyctx,
        );
        return;
    }
    screen_write_redraw_line(ctx, &mut ttyctx, (*s).cy, &mut r);
}
pub unsafe fn screen_write_deletecharacter(
    ctx: &mut screen_write_ctx,
    mut nx: u_int,
    mut bg: u_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    if nx == 0 as u_int {
        nx = 1 as u_int;
    }
    if nx > (*s).grid().sx.wrapping_sub((*s).cx) {
        nx = (*s).grid().sx.wrapping_sub((*s).cx);
    }
    if nx == 0 as u_int {
        return;
    }
    if (*s).cx > (*s).grid().sx.wrapping_sub(1 as u_int) {
        return;
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    grid_view_delete_cells((*s).grid_mut(), (*s).cx, (*s).cy, nx, bg);
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_deletecharacter");
    ttyctx.data = tty_command_data::Count(nx);
    if screen_write_should_draw_line(ctx, (*s).cy) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(
            |tty, ctx| tty_cmd_deletecharacter(tty, ctx, &*s),
            &mut ttyctx,
        );
        return;
    }
    screen_write_redraw_line(ctx, &mut ttyctx, (*s).cy, &mut r);
}
pub unsafe fn screen_write_clearcharacter(
    ctx: &mut screen_write_ctx,
    mut nx: u_int,
    mut bg: u_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    if nx == 0 as u_int {
        nx = 1 as u_int;
    }
    if nx > (*s).grid().sx.wrapping_sub((*s).cx) {
        nx = (*s).grid().sx.wrapping_sub((*s).cx);
    }
    if nx == 0 as u_int {
        return;
    }
    if (*s).cx > (*s).grid().sx.wrapping_sub(1 as u_int) {
        return;
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    grid_view_clear((*s).grid_mut(), (*s).cx, (*s).cy, nx, 1 as u_int, bg);
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_clearcharacter");
    ttyctx.data = tty_command_data::Count(nx);
    if screen_write_should_draw_line(ctx, (*s).cy) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(|tty, ctx| tty_cmd_clearcharacter(tty, ctx), &mut ttyctx);
        return;
    }
    screen_write_redraw_line(ctx, &mut ttyctx, (*s).cy, &mut r);
}
pub unsafe fn screen_write_insertline(ctx: &mut screen_write_ctx, mut ny: u_int, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut sy: u_int = (*s).grid().sy;
    if ny == 0 as u_int {
        ny = 1 as u_int;
    }
    if (*s).cy < (*s).rupper || (*s).cy > (*s).rlower {
        if ny > sy.wrapping_sub((*s).cy) {
            ny = sy.wrapping_sub((*s).cy);
        }
        if ny == 0 as u_int {
            return;
        }
        screen_write_initctx(
            ctx,
            &mut ttyctx,
            1 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
        );
        ttyctx.bg = bg;
        grid_view_insert_lines((*s).grid_mut(), (*s).cy, ny, bg);
        screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_insertline");
        ttyctx.data = tty_command_data::Count(ny);
        if screen_write_should_draw_lines(ctx, (*s).cy, sy.wrapping_sub((*s).cy)) == 0 {
            return;
        }
        if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
            tty_write(|tty, ctx| tty_cmd_insertline(tty, ctx, &*s), &mut ttyctx);
            return;
        }
        screen_write_redraw_pane(ctx, &mut ttyctx);
        return;
    }
    if ny > (*s).rlower.wrapping_add(1 as u_int).wrapping_sub((*s).cy) {
        ny = (*s).rlower.wrapping_add(1 as u_int).wrapping_sub((*s).cy);
    }
    if ny == 0 as u_int {
        return;
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    if (*s).cy < (*s).rupper || (*s).cy > (*s).rlower {
        grid_view_insert_lines((*s).grid_mut(), (*s).cy, ny, bg);
    } else {
        grid_view_insert_lines_region((*s).grid_mut(), (*s).rlower, (*s).cy, ny, bg);
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_insertline");
    ttyctx.data = tty_command_data::Count(ny);
    if screen_write_should_draw_lines(
        ctx,
        (*s).cy,
        (*s).rlower.wrapping_add(1 as u_int).wrapping_sub((*s).cy),
    ) == 0
    {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(|tty, ctx| tty_cmd_insertline(tty, ctx, &*s), &mut ttyctx);
        return;
    }
    screen_write_redraw_pane(ctx, &mut ttyctx);
}
pub unsafe fn screen_write_deleteline(ctx: &mut screen_write_ctx, mut ny: u_int, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut sy: u_int = (*s).grid().sy;
    let mut ry: u_int = 0;
    if ny == 0 as u_int {
        ny = 1 as u_int;
    }
    if (*s).cy < (*s).rupper || (*s).cy > (*s).rlower {
        if ny > sy.wrapping_sub((*s).cy) {
            ny = sy.wrapping_sub((*s).cy);
        }
        if ny == 0 as u_int {
            return;
        }
        screen_write_initctx(
            ctx,
            &mut ttyctx,
            1 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
        );
        ttyctx.bg = bg;
        grid_view_delete_lines((*s).grid_mut(), (*s).cy, ny, bg);
        screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_deleteline");
        ttyctx.data = tty_command_data::Count(ny);
        ry = (*s)
            .rlower
            .wrapping_add(1 as u_int)
            .wrapping_sub((*s).rupper);
        if screen_write_should_draw_lines(ctx, (*s).rupper, ry) == 0 {
            return;
        }
        if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
            tty_write(|tty, ctx| tty_cmd_deleteline(tty, ctx, &*s), &mut ttyctx);
            return;
        }
        screen_write_redraw_pane(ctx, &mut ttyctx);
        return;
    }
    ry = (*s).rlower.wrapping_add(1 as u_int).wrapping_sub((*s).cy);
    if ny > ry {
        ny = ry;
    }
    if ny == 0 as u_int {
        return;
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    if (*s).cy < (*s).rupper || (*s).cy > (*s).rlower {
        grid_view_delete_lines((*s).grid_mut(), (*s).cy, ny, bg);
    } else {
        grid_view_delete_lines_region((*s).grid_mut(), (*s).rlower, (*s).cy, ny, bg);
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_deleteline");
    ttyctx.data = tty_command_data::Count(ny);
    if screen_write_should_draw_lines(ctx, (*s).cy, ry) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(|tty, ctx| tty_cmd_deleteline(tty, ctx, &*s), &mut ttyctx);
        return;
    }
    screen_write_redraw_pane(ctx, &mut ttyctx);
}
pub unsafe fn screen_write_clearline(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut sx: u_int = (*s).grid().sx;
    let mut flags: u_int = 0;
    let gl = grid_get_line((*s).grid(), (*s).grid().hsize.wrapping_add((*s).cy));
    if gl.cellsize as ::core::ffi::c_int == 0 as ::core::ffi::c_int
        && (bg == 8 as u_int || bg == 9 as u_int)
    {
        return;
    }
    flags = (gl.flags as ::core::ffi::c_int & GRID_LINE_OSC133_FLAGS) as u_int;
    let od = gl.osc133_data;
    grid_view_clear((*s).grid_mut(), 0 as u_int, (*s).cy, sx, 1 as u_int, bg);
    let row = (*s).grid().hsize.wrapping_add((*s).cy);
    let gl = grid_get_line_mut((*s).grid_mut(), row);
    gl.flags = (gl.flags as u_int | flags) as u_short;
    gl.osc133_data = od;
    screen_write_collect_clear(ctx, (*s).cy, 1 as u_int);
    let ci = screen_write_current_item(ctx);
    ci.x = 0 as u_int;
    ci.used = sx;
    ci.type_0 = CLEAR;
    ci.bg = bg;
    (*s).write_rows_mut()[(*s).cy as usize]
        .items
        .push_back(ctx.item.take().expect("active screen write context"));
    ctx.item = Some(screen_write_get_citem());
}
pub unsafe fn screen_write_clearendofline(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut sx: u_int = (*s).grid().sx;
    if (*s).cx == 0 as u_int {
        screen_write_clearline(ctx, bg);
        return;
    }
    let gl = grid_get_line((*s).grid(), (*s).grid().hsize.wrapping_add((*s).cy));
    if (*s).cx > sx.wrapping_sub(1 as u_int)
        || (*s).cx >= gl.cellsize as u_int && (bg == 8 as u_int || bg == 9 as u_int)
    {
        return;
    }
    grid_view_clear(
        (*s).grid_mut(),
        (*s).cx,
        (*s).cy,
        sx.wrapping_sub((*s).cx),
        1 as u_int,
        bg,
    );
    let ci = screen_write_current_item(ctx);
    ci.x = (*s).cx;
    ci.used = sx.wrapping_sub((*s).cx);
    ci.type_0 = CLEAR;
    ci.bg = bg;
    screen_write_collect_insert(ctx);
}
pub unsafe fn screen_write_clearstartofline(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut sx: u_int = (*s).grid().sx;
    if (*s).cx >= sx.wrapping_sub(1 as u_int) {
        screen_write_clearline(ctx, bg);
        return;
    }
    if (*s).cx > sx.wrapping_sub(1 as u_int) {
        grid_view_clear((*s).grid_mut(), 0 as u_int, (*s).cy, sx, 1 as u_int, bg);
    } else {
        grid_view_clear(
            (*s).grid_mut(),
            0 as u_int,
            (*s).cy,
            (*s).cx.wrapping_add(1 as u_int),
            1 as u_int,
            bg,
        );
    }
    let ci = screen_write_current_item(ctx);
    ci.x = 0 as u_int;
    ci.used = (*s).cx.wrapping_add(1 as u_int);
    ci.type_0 = CLEAR;
    ci.bg = bg;
    screen_write_collect_insert(ctx);
}
pub unsafe fn screen_write_cursormove(
    ctx: &mut screen_write_ctx,
    mut px: ::core::ffi::c_int,
    mut py: ::core::ffi::c_int,
    mut origin: ::core::ffi::c_int,
) {
    let mut s: *mut screen = ctx.screen_ptr();
    if origin != 0 && py != -(1 as ::core::ffi::c_int) && (*s).mode & MODE_ORIGIN != 0 {
        if py as u_int > (*s).rlower.wrapping_sub((*s).rupper) {
            py = (*s).rlower as ::core::ffi::c_int;
        } else {
            py =
                (py as u_int).wrapping_add((*s).rupper) as ::core::ffi::c_int as ::core::ffi::c_int;
        }
    }
    if px != -(1 as ::core::ffi::c_int) && px as u_int > (*s).grid().sx.wrapping_sub(1 as u_int) {
        px = (*s).grid().sx.wrapping_sub(1 as u_int) as ::core::ffi::c_int;
    }
    if py != -(1 as ::core::ffi::c_int) && py as u_int > (*s).grid().sy.wrapping_sub(1 as u_int) {
        py = (*s).grid().sy.wrapping_sub(1 as u_int) as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: from {},{} to {},{}",
        "screen_write_cursormove",
        { (*s).cx },
        { (*s).cy },
        (px) as u32,
        (py) as u32
    ));
    screen_write_set_cursor(ctx, px, py);
}
pub unsafe fn screen_write_reverseindex(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut ry: u_int = 0;
    if (*s).cy != (*s).rupper {
        if (*s).cy > 0 as u_int {
            screen_write_set_cursor(
                ctx,
                -(1 as ::core::ffi::c_int),
                (*s).cy.wrapping_sub(1 as u_int) as ::core::ffi::c_int,
            );
        }
        return;
    }
    grid_view_scroll_region_down((*s).grid_mut(), (*s).rupper, (*s).rlower, bg);
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_reverseindex");
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    ry = (*s)
        .rlower
        .wrapping_add(1 as u_int)
        .wrapping_sub((*s).rupper);
    if screen_write_should_draw_lines(ctx, (*s).rupper, ry) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(|tty, ctx| tty_cmd_reverseindex(tty, ctx, &*s), &mut ttyctx);
        return;
    }
    screen_write_redraw_pane(ctx, &mut ttyctx);
}
pub unsafe fn screen_write_scrollregion(
    ctx: &mut screen_write_ctx,
    mut rupper: u_int,
    mut rlower: u_int,
) {
    let mut s: *mut screen = ctx.screen_ptr();
    if rupper > (*s).grid().sy.wrapping_sub(1 as u_int) {
        rupper = (*s).grid().sy.wrapping_sub(1 as u_int);
    }
    if rlower > (*s).grid().sy.wrapping_sub(1 as u_int) {
        rlower = (*s).grid().sy.wrapping_sub(1 as u_int);
    }
    if rupper >= rlower {
        return;
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_scrollregion");
    screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
    (*s).rupper = rupper;
    (*s).rlower = rlower;
}
pub unsafe fn screen_write_linefeed(
    ctx: &mut screen_write_ctx,
    mut wrapped: ::core::ffi::c_int,
    mut bg: u_int,
) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut rupper: u_int = (*s).rupper;
    let mut rlower: u_int = (*s).rlower;
    let row = (*s).grid().hsize.wrapping_add((*s).cy);
    let gl = grid_get_line_mut((*s).grid_mut(), row);
    if wrapped != 0 {
        gl.flags = (gl.flags as ::core::ffi::c_int | GRID_LINE_WRAPPED) as u_short;
    }
    log_debug(format_args!(
        "{}: at {},{} (region {}-{})",
        "screen_write_linefeed",
        { (*s).cx },
        { (*s).cy },
        (rupper) as u32,
        (rlower) as u32
    ));
    if bg != ctx.bg {
        screen_write_collect_flush(ctx, 1 as ::core::ffi::c_int, "screen_write_linefeed");
        ctx.bg = bg;
    }
    if (*s).cy != (*s).rlower {
        if (*s).cy < (*s).grid().sy.wrapping_sub(1 as u_int) {
            screen_write_set_cursor(
                ctx,
                -(1 as ::core::ffi::c_int),
                (*s).cy.wrapping_add(1 as u_int) as ::core::ffi::c_int,
            );
        }
        return;
    }
    grid_view_scroll_region_up((*s).grid_mut(), (*s).rupper, (*s).rlower, bg);
    screen_write_collect_scroll(ctx, bg);
    ctx.scrolled = ctx.scrolled.wrapping_add(1);
}
pub unsafe fn screen_write_scrollup(ctx: &mut screen_write_ctx, mut lines: u_int, mut bg: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut i: u_int = 0;
    if lines == 0 as u_int {
        lines = 1 as u_int;
    } else if lines
        > (*s)
            .rlower
            .wrapping_sub((*s).rupper)
            .wrapping_add(1 as u_int)
    {
        lines = (*s)
            .rlower
            .wrapping_sub((*s).rupper)
            .wrapping_add(1 as u_int);
    }
    if bg != ctx.bg {
        screen_write_collect_flush(ctx, 1 as ::core::ffi::c_int, "screen_write_scrollup");
        ctx.bg = bg;
    }
    i = 0 as u_int;
    while i < lines {
        grid_view_scroll_region_up((*s).grid_mut(), (*s).rupper, (*s).rlower, bg);
        screen_write_collect_scroll(ctx, bg);
        i = i.wrapping_add(1);
    }
    ctx.scrolled = ctx.scrolled.wrapping_add(lines);
}
pub unsafe fn screen_write_scrolldown(ctx: &mut screen_write_ctx, mut lines: u_int, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut i: u_int = 0;
    let mut ry: u_int = 0;
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    if lines == 0 as u_int {
        lines = 1 as u_int;
    } else if lines
        > (*s)
            .rlower
            .wrapping_sub((*s).rupper)
            .wrapping_add(1 as u_int)
    {
        lines = (*s)
            .rlower
            .wrapping_sub((*s).rupper)
            .wrapping_add(1 as u_int);
    }
    i = 0 as u_int;
    while i < lines {
        grid_view_scroll_region_down((*s).grid_mut(), (*s).rupper, (*s).rlower, bg);
        i = i.wrapping_add(1);
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_scrolldown");
    ttyctx.data = tty_command_data::Count(lines);
    ry = (*s)
        .rlower
        .wrapping_add(1 as u_int)
        .wrapping_sub((*s).rupper);
    if screen_write_should_draw_lines(ctx, (*s).rupper, ry) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 || write_pane.is_none() {
        tty_write(|tty, ctx| tty_cmd_scrolldown(tty, ctx, &*s), &mut ttyctx);
        return;
    }
    screen_write_redraw_pane(ctx, &mut ttyctx);
}
pub unsafe fn screen_write_carriagereturn(ctx: &mut screen_write_ctx) {
    screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int));
}
pub unsafe fn screen_write_clearendofscreen(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut sx: u_int = (*s).grid().sx;
    let mut sy: u_int = (*s).grid().sy;
    let mut y: u_int = 0;
    let mut i: u_int = 0;
    let mut xoff: u_int = 0;
    let mut yoff: u_int = 0;
    let mut ocx: u_int = 0;
    let mut ocy: u_int = 0;
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    if (*s).cx == 0 as u_int
        && (*s).cy == 0 as u_int
        && (*s).grid().flags & GRID_HISTORY != 0
        && write_pane.is_some()
        && write_pane
            .expect("write pane")
            .with_options_mut(|options| options_get_number(options, c"scroll-on-clear"))
            != 0
    {
        grid_view_clear_history((*s).grid_mut(), bg);
    } else {
        if (*s).cx <= sx.wrapping_sub(1 as u_int) {
            grid_view_clear(
                (*s).grid_mut(),
                (*s).cx,
                (*s).cy,
                sx.wrapping_sub((*s).cx),
                1 as u_int,
                bg,
            );
        }
        grid_view_clear(
            (*s).grid_mut(),
            0 as u_int,
            (*s).cy.wrapping_add(1 as u_int),
            sx,
            sy.wrapping_sub((*s).cy.wrapping_add(1 as u_int)),
            bg,
        );
    }
    screen_write_collect_clear(
        ctx,
        (*s).cy.wrapping_add(1 as u_int),
        sy.wrapping_sub((*s).cy.wrapping_add(1 as u_int)),
    );
    screen_write_collect_flush(
        ctx,
        0 as ::core::ffi::c_int,
        "screen_write_clearendofscreen",
    );
    if screen_write_should_draw_lines(ctx, (*s).cy, sy.wrapping_sub((*s).cy)) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 {
        tty_write(|tty, ctx| tty_cmd_clearendofscreen(tty, ctx), &mut ttyctx);
        return;
    }
    ocx = (*s).cx;
    ocy = (*s).cy;
    if let Some(write_pane_value) = write_pane {
        xoff = write_pane_value.geometry().2 as u_int;
        yoff = write_pane_value.geometry().3 as u_int;
    } else {
        xoff = 0 as u_int;
        yoff = 0 as u_int;
    }
    if (*s).cx <= sx.wrapping_sub(1 as u_int) {
        WindowPane::visible_ranges(
            write_pane,
            xoff.wrapping_add((*s).cx) as ::core::ffi::c_int,
            yoff.wrapping_add((*s).cy) as ::core::ffi::c_int,
            sx.wrapping_sub((*s).cx),
            &mut r,
        );
        i = 0 as u_int;
        while (i as usize) < r.len() {
            let ri = &r[i as usize];
            if !(ri.nx == 0 as u_int) {
                screen_write_collect_insert_clear(ctx, ri.px.wrapping_sub(xoff), ri.nx, bg);
            }
            i = i.wrapping_add(1);
        }
    }
    y = (*s).cy.wrapping_add(1 as u_int);
    while y < sy {
        screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, y as ::core::ffi::c_int);
        WindowPane::visible_ranges(
            write_pane,
            xoff as ::core::ffi::c_int,
            yoff.wrapping_add(y) as ::core::ffi::c_int,
            sx,
            &mut r,
        );
        i = 0 as u_int;
        while (i as usize) < r.len() {
            let ri = &r[i as usize];
            if !(ri.nx == 0 as u_int) {
                screen_write_collect_insert_clear(ctx, ri.px.wrapping_sub(xoff), ri.nx, bg);
            }
            i = i.wrapping_add(1);
        }
        y = y.wrapping_add(1);
    }
    screen_write_set_cursor(ctx, ocx as ::core::ffi::c_int, ocy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_clearstartofscreen(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut sx: u_int = (*s).grid().sx;
    let mut y: u_int = 0;
    let mut i: u_int = 0;
    let mut xoff: u_int = 0;
    let mut yoff: u_int = 0;
    let mut ocx: u_int = 0;
    let mut ocy: u_int = 0;
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    if (*s).cy > 0 as u_int {
        grid_view_clear((*s).grid_mut(), 0 as u_int, 0 as u_int, sx, (*s).cy, bg);
    }
    if (*s).cx > sx.wrapping_sub(1 as u_int) {
        grid_view_clear((*s).grid_mut(), 0 as u_int, (*s).cy, sx, 1 as u_int, bg);
    } else {
        grid_view_clear(
            (*s).grid_mut(),
            0 as u_int,
            (*s).cy,
            (*s).cx.wrapping_add(1 as u_int),
            1 as u_int,
            bg,
        );
    }
    screen_write_collect_clear(ctx, 0 as u_int, (*s).cy);
    screen_write_collect_flush(
        ctx,
        0 as ::core::ffi::c_int,
        "screen_write_clearstartofscreen",
    );
    if screen_write_should_draw_lines(ctx, 0 as u_int, (*s).cy.wrapping_add(1 as u_int)) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 {
        tty_write(|tty, ctx| tty_cmd_clearstartofscreen(tty, ctx), &mut ttyctx);
        return;
    }
    ocx = (*s).cx;
    ocy = (*s).cy;
    if let Some(write_pane_value) = write_pane {
        xoff = write_pane_value.geometry().2 as u_int;
        yoff = write_pane_value.geometry().3 as u_int;
    } else {
        xoff = 0 as u_int;
        yoff = 0 as u_int;
    }
    y = 0 as u_int;
    while y < (*s).cy {
        screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, y as ::core::ffi::c_int);
        WindowPane::visible_ranges(
            write_pane,
            xoff as ::core::ffi::c_int,
            yoff.wrapping_add(y) as ::core::ffi::c_int,
            sx,
            &mut r,
        );
        i = 0 as u_int;
        while (i as usize) < r.len() {
            let ri = &r[i as usize];
            if !(ri.nx == 0 as u_int) {
                screen_write_collect_insert_clear(ctx, ri.px.wrapping_sub(xoff), ri.nx, bg);
            }
            i = i.wrapping_add(1);
        }
        y = y.wrapping_add(1);
    }
    screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, (*s).cy as ::core::ffi::c_int);
    WindowPane::visible_ranges(
        write_pane,
        xoff as ::core::ffi::c_int,
        yoff.wrapping_add(ocy) as ::core::ffi::c_int,
        (*s).cx.wrapping_add(1 as u_int),
        &mut r,
    );
    i = 0 as u_int;
    while (i as usize) < r.len() {
        let ri = &r[i as usize];
        if !(ri.nx == 0 as u_int) {
            screen_write_collect_insert_clear(ctx, ri.px.wrapping_sub(xoff), ri.nx, bg);
        }
        i = i.wrapping_add(1);
    }
    screen_write_set_cursor(ctx, ocx as ::core::ffi::c_int, ocy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_clearscreen(ctx: &mut screen_write_ctx, mut bg: u_int) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    let mut sx: u_int = (*s).grid().sx;
    let mut sy: u_int = (*s).grid().sy;
    let mut y: u_int = 0;
    let mut i: u_int = 0;
    let mut xoff: u_int = 0;
    let mut yoff: u_int = 0;
    let mut ocx: u_int = 0;
    let mut ocy: u_int = 0;
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    ttyctx.bg = bg;
    if (*s).grid().flags & GRID_HISTORY != 0
        && write_pane.is_some()
        && write_pane
            .expect("write pane")
            .with_options_mut(|options| options_get_number(options, c"scroll-on-clear"))
            != 0
    {
        grid_view_clear_history((*s).grid_mut(), bg);
    } else {
        grid_view_clear((*s).grid_mut(), 0 as u_int, 0 as u_int, sx, sy, bg);
    }
    screen_write_collect_clear(ctx, 0 as u_int, sy);
    if screen_write_should_draw_lines(ctx, 0 as u_int, sy) == 0 {
        return;
    }
    if !ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 {
        tty_write(|tty, ctx| tty_cmd_clearscreen(tty, ctx), &mut ttyctx);
        return;
    }
    ocx = (*s).cx;
    ocy = (*s).cy;
    if let Some(write_pane_value) = write_pane {
        xoff = write_pane_value.geometry().2 as u_int;
        yoff = write_pane_value.geometry().3 as u_int;
    } else {
        xoff = 0 as u_int;
        yoff = 0 as u_int;
    }
    y = 0 as u_int;
    while y < sy {
        screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, y as ::core::ffi::c_int);
        WindowPane::visible_ranges(
            write_pane,
            xoff as ::core::ffi::c_int,
            yoff.wrapping_add(y) as ::core::ffi::c_int,
            sx,
            &mut r,
        );
        i = 0 as u_int;
        while (i as usize) < r.len() {
            let ri = &r[i as usize];
            if !(ri.nx == 0 as u_int) {
                screen_write_collect_insert_clear(ctx, ri.px.wrapping_sub(xoff), ri.nx, bg);
            }
            i = i.wrapping_add(1);
        }
        y = y.wrapping_add(1);
    }
    screen_write_set_cursor(ctx, ocx as ::core::ffi::c_int, ocy as ::core::ffi::c_int);
}
pub unsafe fn screen_write_clearhistory(ctx: &mut screen_write_ctx) {
    grid_clear_history(&mut *((*ctx.screen_ptr()).grid_mut()));
}
pub unsafe fn screen_write_fullredraw(ctx: &mut screen_write_ctx) {
    let mut ttyctx = tty_ctx::default();
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_fullredraw");
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if let Some(redraw_cb) = &ttyctx.redraw_cb {
        redraw_cb(&ttyctx);
    }
}
fn screen_write_collect_trim(
    items: &mut screen_write_items,
    x: u_int,
    used: u_int,
) -> (usize, bool) {
    let mut wrapped = false;
    let sx = x;
    let ex = x.wrapping_add(used).wrapping_sub(1);
    let mut index = 0;
    while let Some(ci) = items.get_mut(index) {
        let csx = ci.x;
        let cex = csx.wrapping_add(ci.used).wrapping_sub(1);
        if cex < sx {
            // Entirely before the replacement interval.
            index += 1;
        } else if csx > ex {
            return (index, wrapped);
        } else if csx >= sx && cex <= ex {
            if csx == 0 && ci.wrapped != 0 {
                wrapped = true;
            }
            screen_write_free_citem(items.remove(index).expect("linked write command"));
        } else if csx < sx && cex >= sx && cex <= ex {
            ci.used = sx.wrapping_sub(csx);
            index += 1;
        } else if cex > ex && csx >= sx && csx <= ex {
            ci.x = ex.wrapping_add(1);
            ci.used = cex.wrapping_sub(ex);
            return (index, wrapped);
        } else {
            let mut right = screen_write_get_citem();
            right.type_0 = ci.type_0;
            right.bg = ci.bg;
            right.gc = ci.gc;
            right.x = ex.wrapping_add(1);
            right.used = cex.wrapping_sub(ex);
            ci.used = sx.wrapping_sub(csx);
            items.insert(index + 1, right);
            return (index + 1, wrapped);
        }
    }
    (index, wrapped)
}
unsafe fn screen_write_collect_clear(ctx: &mut screen_write_ctx, mut y: u_int, mut n: u_int) {
    let mut i: u_int = 0;
    i = y;
    while i < y.wrapping_add(n) {
        let cl = &mut (*ctx.screen_ptr()).write_rows_mut()[i as usize];
        screen_write_recycle_items(&mut cl.items);
        i = i.wrapping_add(1);
    }
}
unsafe fn screen_write_collect_scroll(ctx: &mut screen_write_ctx, bg: u_int) {
    let s = &mut *ctx.screen_ptr();
    log_debug(format_args!(
        "{}: at {},{} (region {}-{})",
        "screen_write_collect_scroll", s.cx, s.cy, s.rupper, s.rlower
    ));
    let width = s.grid().sx;
    let (first, last) = (s.rupper as usize, s.rlower as usize);
    let rows = &mut s.write_rows_mut()[first..=last];
    screen_write_recycle_items(&mut rows[0].items);
    rows.rotate_left(1);
    let mut item = screen_write_get_citem();
    item.x = 0;
    item.used = width;
    item.type_0 = CLEAR;
    item.bg = bg;
    rows.last_mut()
        .expect("scroll region has at least one row")
        .items
        .push_back(item);
}
unsafe fn screen_write_collect_flush_scrolled(ctx: &mut screen_write_ctx) -> ::core::ffi::c_int {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let wp = write_pane;
    let mut s: *mut screen = ctx.screen_ptr();
    let mut ttyctx = tty_ctx::default();
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        1 as ::core::ffi::c_int,
    );
    if ttyctx.flags & TTY_CTX_PANE_OBSCURED != 0 && wp.is_some() {
        screen_write_redraw_pane(ctx, &mut ttyctx);
        return 0 as ::core::ffi::c_int;
    }
    if let Some(pane) = wp.filter(|pane| pane.scrollbar_visible() && pane.scrollbar_overlay()) {
        pane.request_redraw(false);
        return 0 as ::core::ffi::c_int;
    }
    log_debug(format_args!(
        "{}: scrolled {} (region {}-{})",
        "screen_write_collect_flush_scrolled",
        { ctx.scrolled },
        { (*s).rupper },
        { (*s).rlower }
    ));
    if ctx.scrolled
        > (*s)
            .rlower
            .wrapping_sub((*s).rupper)
            .wrapping_add(1 as u_int)
    {
        ctx.scrolled = (*s)
            .rlower
            .wrapping_sub((*s).rupper)
            .wrapping_add(1 as u_int);
    }
    if let Some(pane) = wp {
        let pane_bottom = (pane.geometry().3 as u_int).wrapping_add(pane.geometry().1);
        let window_sy = pane.window_size().1;
        if pane_bottom > window_sy {
            ttyctx.orlower = ttyctx
                .orlower
                .wrapping_sub(pane_bottom.wrapping_sub(window_sy));
        }
    }
    ttyctx.data = tty_command_data::Count(ctx.scrolled);
    ttyctx.bg = ctx.bg;
    tty_write(|tty, ctx| tty_cmd_scrollup(tty, ctx, &*s), &mut ttyctx);
    if wp.is_some() {
        write_pane_owner
            .as_ref()
            .expect("write pane")
            .redraw_scrollbar();
    }
    1 as ::core::ffi::c_int
}
unsafe fn screen_write_collect_flush_line(
    ctx: &mut screen_write_ctx,
    mut y: u_int,
    r: &mut Vec<visible_range>,
) -> u_int {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let wp = write_pane;
    let mut s: *mut screen = ctx.screen_ptr();
    let mut last: u_int = UINT_MAX;
    let mut items: u_int = 0 as u_int;
    let mut wsx: u_int = 0;
    let mut wsy: u_int = 0;
    let mut w_length: u_int = 0;
    let mut i: u_int = 0;
    let mut w_start: ::core::ffi::c_int = 0;
    let mut w_end: ::core::ffi::c_int = 0;
    let mut xoff: ::core::ffi::c_int = 0;
    let mut yoff: ::core::ffi::c_int = 0;
    let mut written: ::core::ffi::c_int = 0;
    let mut r_start: ::core::ffi::c_int = 0;
    let mut r_end: ::core::ffi::c_int = 0;
    let mut c_start: ::core::ffi::c_int = 0;
    let mut c_end: ::core::ffi::c_int = 0;
    let mut ttyctx = tty_ctx::default();
    if let Some(wp_value) = wp {
        wsx = wp_value.window_size().0;
        wsy = wp_value.window_size().1;
        xoff = wp_value.geometry().2;
        yoff = wp_value.geometry().3;
    } else {
        wsx = (*s).grid().sx;
        wsy = (*s).grid().sy;
        xoff = 0 as ::core::ffi::c_int;
        yoff = 0 as ::core::ffi::c_int;
    }
    if y.wrapping_add(yoff as u_int) >= wsy {
        return 0 as u_int;
    }
    WindowPane::visible_ranges(
        wp,
        0 as ::core::ffi::c_int,
        y.wrapping_add(yoff as u_int) as ::core::ffi::c_int,
        wsx,
        r,
    );
    let mut index = 0;
    loop {
        let cl = &(*s).write_rows()[y as usize];
        let Some(ci) = cl.items.get(index) else {
            break;
        };
        log_debug(format_args!(
            "collect list: x={} (last {}), y={}, used={}",
            { ci.x },
            (last) as u32,
            { y },
            { ci.used }
        ));
        if last != UINT_MAX && ci.x <= last {
            fatalx(|out| {
                write!(
                    out,
                    "collect list bad order: {} <= {}",
                    { ci.x },
                    (last) as u32
                )
            });
        }
        w_length = 0 as u_int;
        written = 0 as ::core::ffi::c_int;
        i = 0 as u_int;
        while (i as usize) < r.len() {
            let ri = &r[i as usize];
            if !(ri.nx == 0 as u_int) {
                r_start = ri.px as ::core::ffi::c_int;
                r_end = ri.px.wrapping_add(ri.nx) as ::core::ffi::c_int;
                c_start = ci.x as ::core::ffi::c_int;
                c_end = ci.x.wrapping_add(ci.used) as ::core::ffi::c_int;
                if !(c_start + xoff >= r_end || c_end + xoff <= r_start) {
                    if r_start > c_start + xoff {
                        w_start = r_start - xoff;
                    } else {
                        w_start = c_start;
                    }
                    if c_end + xoff > r_end {
                        w_end = r_end - xoff;
                    } else {
                        w_end = c_end;
                    }
                    if !(w_end <= w_start) {
                        w_length = (w_end - w_start) as u_int;
                        if !(w_length <= 0 as u_int) {
                            screen_write_set_cursor(ctx, w_start, y as ::core::ffi::c_int);
                            if ci.type_0 as ::core::ffi::c_uint
                                == CLEAR as ::core::ffi::c_int as ::core::ffi::c_uint
                            {
                                screen_write_initctx(
                                    ctx,
                                    &mut ttyctx,
                                    1 as ::core::ffi::c_int,
                                    0 as ::core::ffi::c_int,
                                );
                                ttyctx.bg = ci.bg;
                                ttyctx.data = tty_command_data::Count(w_length);
                                tty_write(|tty, ctx| tty_cmd_clearcharacter(tty, ctx), &mut ttyctx);
                            } else {
                                screen_write_initctx(
                                    ctx,
                                    &mut ttyctx,
                                    0 as ::core::ffi::c_int,
                                    0 as ::core::ffi::c_int,
                                );
                                if ci.wrapped != 0 {
                                    ttyctx.flags |= TTY_CTX_WRAPPED;
                                }
                                ttyctx.data = tty_command_data::Bytes(
                                    &cl.data
                                        [w_start as usize..w_start as usize + w_length as usize],
                                );
                                tty_write(
                                    |tty, ctx| tty_cmd_cells(tty, ctx, &*s, &ci.gc),
                                    &mut ttyctx,
                                );
                            }
                            items = items.wrapping_add(1);
                            written = 1 as ::core::ffi::c_int;
                        }
                    }
                }
            }
            i = i.wrapping_add(1);
        }
        if written != 0 {
            last = ci.x;
            screen_write_free_citem(
                (*s).write_rows_mut()[y as usize]
                    .items
                    .remove(index)
                    .expect("linked write command"),
            );
        } else {
            index += 1;
        }
    }
    items
}
unsafe fn screen_write_collect_flush(
    ctx: &mut screen_write_ctx,
    scroll_only: ::core::ffi::c_int,
    from: &str,
) {
    screen_write_collect_flush_with_ranges(ctx, scroll_only, from, &mut Vec::new());
}

unsafe fn screen_write_collect_flush_with_ranges(
    ctx: &mut screen_write_ctx,
    scroll_only: ::core::ffi::c_int,
    from: &str,
    r: &mut Vec<visible_range>,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut current_block: u64;
    let mut s: *mut screen = ctx.screen_ptr();
    let wp = write_pane;
    let mut y: u_int = 0;
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut items: u_int = 0 as u_int;
    if !(wp.is_some() && wp.expect("write pane").output_suppressed()) {
        if (*s).mode & MODE_SYNC != 0 {
            if ctx.scrolled != 0 as u_int {
                screen_write_should_draw_lines(
                    ctx,
                    (*s).rupper,
                    (*s).rlower
                        .wrapping_add(1 as u_int)
                        .wrapping_sub((*s).rupper),
                );
            }
            y = 0 as u_int;
            while y < (*s).grid().sy {
                let cl = &(*s).write_rows()[y as usize];
                if !cl.items.is_empty() {
                    screen_write_should_draw_line(ctx, y);
                }
                y = y.wrapping_add(1);
            }
        } else {
            if ctx.scrolled != 0 as u_int {
                if screen_write_collect_flush_scrolled(ctx) == 0 {
                    current_block = 9123463459983562265;
                } else {
                    ctx.scrolled = 0 as u_int;
                    current_block = 8236137900636309791;
                }
            } else {
                current_block = 8236137900636309791;
            }
            match current_block {
                9123463459983562265 => {}
                _ => {
                    ctx.bg = 8 as u_int;
                    if scroll_only != 0 {
                        return;
                    }
                    cx = (*s).cx;
                    cy = (*s).cy;
                    y = 0 as u_int;
                    while y < (*s).grid().sy {
                        items = items.wrapping_add(screen_write_collect_flush_line(ctx, y, r));
                        y = y.wrapping_add(1);
                    }
                    (*s).cx = cx;
                    (*s).cy = cy;
                    log_debug(format_args!(
                        "{}: flushed {} items ({})",
                        "screen_write_collect_flush",
                        (items) as u32,
                        from
                    ));
                    return;
                }
            }
        }
    }
    y = 0 as u_int;
    while y < (*s).grid().sy {
        let cl = &mut (*s).write_rows_mut()[y as usize];
        while let Some(item) = cl.items.pop_front() {
            screen_write_free_citem(item);
        }
        y = y.wrapping_add(1);
    }
    ctx.scrolled = 0 as u_int;
    ctx.bg = 8 as u_int;
}
unsafe fn screen_write_collect_insert(ctx: &mut screen_write_ctx) {
    let mut item = ctx.item.take().expect("active screen write context");
    let s = &mut *ctx.screen_ptr();
    let row = s.cy as usize;
    let items = &mut s.write_rows_mut()[row].items;
    let (before, wrapped) = screen_write_collect_trim(items, item.x, item.used);
    if wrapped {
        item.wrapped = 1;
    }
    items.insert(before, item);
    ctx.item = Some(screen_write_get_citem());
}
unsafe fn screen_write_collect_insert_clear(
    ctx: &mut screen_write_ctx,
    mut px: u_int,
    mut nx: u_int,
    mut bg: u_int,
) {
    let ci = screen_write_current_item(ctx);
    if nx != 0 as u_int {
        ci.x = px;
        ci.used = nx;
        ci.type_0 = CLEAR;
        ci.bg = bg;
        screen_write_collect_insert(ctx);
    }
}
unsafe fn screen_write_clear_cell(gd: &mut grid, px: u_int, py: u_int) {
    let mut gc = grid_default_cell;
    grid_view_get_cell(gd, px, py, &mut gc);
    let bg = gc.bg;
    gc = grid_default_cell;
    gc.bg = bg;
    grid_view_set_cell(gd, px, py, &gc);
}
unsafe fn screen_write_insert_clears(ctx: &mut screen_write_ctx, mut px: u_int, mut nx: u_int) {
    let mut s: *mut screen = ctx.screen_ptr();
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
    let mut xx: u_int = 0;
    let mut start: u_int = px;
    let mut n: u_int = 0;
    let mut bg: ::core::ffi::c_int = 8 as ::core::ffi::c_int;
    xx = px;
    while xx < px.wrapping_add(nx) {
        grid_view_get_cell((*s).grid(), xx, (*s).cy, &mut gc);
        if xx == start {
            bg = gc.bg;
        } else if gc.bg != bg {
            n = xx.wrapping_sub(start);
            log_debug(format_args!(
                "{}: from {}, size {}",
                "screen_write_insert_clears",
                (start) as u32,
                (n) as u32
            ));
            screen_write_collect_insert_clear(ctx, start, n, bg as u_int);
            start = xx;
            bg = gc.bg;
        }
        xx = xx.wrapping_add(1);
    }
    log_debug(format_args!(
        "{}: from {}, size {}",
        "screen_write_insert_clears",
        (start) as u32,
        (xx.wrapping_sub(start)) as u32
    ));
    screen_write_collect_insert_clear(ctx, start, xx.wrapping_sub(start), bg as u_int);
}
pub unsafe fn screen_write_collect_end(ctx: &mut screen_write_ctx) {
    let mut s: *mut screen = ctx.screen_ptr();
    let row = (*s).cy as usize;
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
    let mut xx: u_int = 0;
    let mut bx: u_int = 0 as u_int;
    let mut bnx: u_int = 0 as u_int;
    let (start, used, collected_cell) = {
        let item = screen_write_current_item(ctx);
        if item.used == 0 {
            return;
        }
        item.x = (*s).cx;
        (item.x, item.used, item.gc)
    };
    screen_write_collect_insert(ctx);
    let cl = &(*s).write_rows()[row];
    log_debug(format_args!(
        "{}: {} {} (at {},{})",
        "screen_write_collect_end",
        { used },
        log_bytes(&cl.data[start as usize..start as usize + used as usize]),
        { (*s).cx },
        { (*s).cy }
    ));
    if (*s).cx != 0 as u_int {
        xx = (*s).cx;
        while xx > 0 as u_int {
            grid_view_get_cell((*s).grid(), xx, (*s).cy, &mut gc);
            if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
                break;
            }
            screen_write_clear_cell((*s).grid_mut(), xx, (*s).cy);
            log_debug(format_args!(
                "{}: padding erased (before) at {} (cx {})",
                "screen_write_collect_end",
                (xx) as u32,
                { (*s).cx }
            ));
            xx = xx.wrapping_sub(1);
        }
        if xx != (*s).cx {
            if xx == 0 as u_int {
                grid_view_get_cell((*s).grid(), 0 as u_int, (*s).cy, &mut gc);
            }
            if gc.data.width as ::core::ffi::c_int > 1 as ::core::ffi::c_int
                || gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0
            {
                screen_write_clear_cell((*s).grid_mut(), xx, (*s).cy);
                log_debug(format_args!(
                    "{}: padding erased (before) at {} (cx {})",
                    "screen_write_collect_end",
                    (xx) as u32,
                    { (*s).cx }
                ));
            }
            bx = xx;
            bnx = (*s).cx.wrapping_sub(xx);
        }
    }
    let cl = &(*s).write_rows()[row];
    let bytes = &cl.data[start as usize..start as usize + used as usize];
    grid_view_set_cells(
        (*s).grid.as_deref_mut().expect("screen is initialized"),
        (*s).cx,
        (*s).cy,
        &collected_cell,
        bytes,
    );
    if bnx != 0 as u_int {
        screen_write_insert_clears(ctx, bx, bnx);
    }
    screen_write_set_cursor(
        ctx,
        (*s).cx.wrapping_add(used) as ::core::ffi::c_int,
        -(1 as ::core::ffi::c_int),
    );
    xx = (*s).cx;
    while xx < (*s).grid().sx {
        grid_view_get_cell((*s).grid(), xx, (*s).cy, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        screen_write_clear_cell((*s).grid_mut(), xx, (*s).cy);
        log_debug(format_args!(
            "{}: padding erased (after) at {} (cx {})",
            "screen_write_collect_end",
            (xx) as u32,
            { (*s).cx }
        ));
        xx = xx.wrapping_add(1);
    }
    if xx != (*s).cx {
        screen_write_insert_clears(ctx, (*s).cx, xx.wrapping_sub((*s).cx));
    }
}
pub unsafe fn screen_write_collect_add(ctx: &mut screen_write_ctx, gc: &grid_cell) {
    let mut s: *mut screen = ctx.screen_ptr();
    let mut sx: u_int = (*s).grid().sx;
    let mut collect: ::core::ffi::c_int = 0;
    collect = 1 as ::core::ffi::c_int;
    if gc.data.width as ::core::ffi::c_int != 1 as ::core::ffi::c_int
        || gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int
        || gc.data.data[0] as ::core::ffi::c_int >= 0x7f as ::core::ffi::c_int
        || (gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0)
        || (gc.attr as ::core::ffi::c_int & GRID_ATTR_CHARSET != 0)
        || (!(*s).mode & MODE_WRAP != 0)
        || ((*s).mode & MODE_INSERT != 0)
        || (!(*s).sel.is_none())
    {
        collect = 0 as ::core::ffi::c_int;
    }
    if collect == 0 {
        screen_write_collect_end(ctx);
        screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_collect_add");
        screen_write_cell(ctx, gc);
        return;
    }
    if (*s).cx > sx.wrapping_sub(1 as u_int)
        || screen_write_current_item(ctx).used > sx.wrapping_sub(1 as u_int).wrapping_sub((*s).cx)
    {
        screen_write_collect_end(ctx);
    }
    if (*s).cx > sx.wrapping_sub(1 as u_int) {
        log_debug(format_args!(
            "{}: wrapped at {},{}",
            "screen_write_collect_add",
            { (*s).cx },
            { (*s).cy }
        ));
        screen_write_current_item(ctx).wrapped = 1 as ::core::ffi::c_int;
        screen_write_linefeed(ctx, 1 as ::core::ffi::c_int, 8 as u_int);
        screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int));
    }
    let ci = screen_write_current_item(ctx);
    if ci.used == 0 {
        ci.gc = *gc;
    }
    let width = (*s).grid().sx as usize;
    let row = &mut (*s).write_rows_mut()[(*s).cy as usize];
    if row.data.is_empty() {
        if width == 0 {
            fatalx(|out| out.write_all(b"xmalloc: zero size"));
        }
        row.data.resize_with(width, || 0);
    }
    let used = ci.used;
    ci.used = used.wrapping_add(1);
    row.data[(*s).cx.wrapping_add(used) as usize] = gc.data.data[0];
}
pub unsafe fn screen_write_cell(ctx: &mut screen_write_ctx, gc: &grid_cell) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let wp = write_pane;
    let ud = &gc.data;
    let mut tmp_gc: grid_cell = grid_cell {
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
    let mut now_gc: grid_cell = grid_cell {
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
    let mut ttyctx = tty_ctx::default();
    let mut sx: u_int = (*s).grid().sx;
    let mut sy: u_int = (*s).grid().sy;
    let mut width: u_int = ud.width as u_int;
    let mut xx: u_int = 0;
    let mut not_wrap: u_int = 0;
    let mut i: u_int = 0;
    let mut n: u_int = 0;
    let mut vis: u_int = 0;
    let mut selected: ::core::ffi::c_int = 0;
    let mut skip: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
    let mut redraw: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut yoff: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut xoff: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
        return;
    }
    if screen_write_combine(ctx, gc) != 0 as ::core::ffi::c_int {
        return;
    }
    screen_write_collect_flush(ctx, 1 as ::core::ffi::c_int, "screen_write_cell");
    if !(*s).mode & MODE_WRAP != 0
        && width > 1 as u_int
        && (width > sx || (*s).cx != sx && (*s).cx > sx.wrapping_sub(width))
    {
        return;
    }
    if (*s).mode & MODE_INSERT != 0 {
        grid_view_insert_cells((*s).grid_mut(), (*s).cx, (*s).cy, width, 8 as u_int);
        skip = 0 as ::core::ffi::c_int;
    }
    if (*s).mode & MODE_WRAP != 0 && (*s).cx > sx.wrapping_sub(width) {
        log_debug(format_args!(
            "{}: wrapped at {},{}",
            "screen_write_cell",
            { (*s).cx },
            { (*s).cy }
        ));
        screen_write_linefeed(ctx, 1 as ::core::ffi::c_int, 8 as u_int);
        screen_write_set_cursor(ctx, 0 as ::core::ffi::c_int, -(1 as ::core::ffi::c_int));
        screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_cell");
    }
    if (*s).cx > sx.wrapping_sub(width) || (*s).cy > sy.wrapping_sub(1 as u_int) {
        return;
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let gl = grid_get_line((*s).grid(), (*s).grid().hsize.wrapping_add((*s).cy));
    if gl.flags as ::core::ffi::c_int & GRID_LINE_EXTENDED != 0 {
        grid_view_get_cell((*s).grid(), (*s).cx, (*s).cy, &mut now_gc);
        if screen_write_overwrite((*s).grid_mut(), (*s).cx, (*s).cy, &now_gc, width) != 0 {
            redraw = 1 as ::core::ffi::c_int;
            skip = 0 as ::core::ffi::c_int;
        }
    }
    xx = (*s).cx.wrapping_add(1 as u_int);
    while xx < (*s).cx.wrapping_add(width) {
        log_debug(format_args!(
            "{}: new padding at {},{}",
            "screen_write_cell",
            (xx) as u32,
            { (*s).cy }
        ));
        grid_view_set_padding((*s).grid_mut(), xx, (*s).cy, gc.bg);
        skip = 0 as ::core::ffi::c_int;
        xx = xx.wrapping_add(1);
    }
    if skip != 0 {
        let gl = grid_get_line((*s).grid(), (*s).grid().hsize.wrapping_add((*s).cy));
        if (*s).cx >= gl.cellsize as u_int {
            skip = grid_cells_equal(gc, &grid_default_cell) as ::core::ffi::c_int;
        } else {
            let gce = &gl.celldata[(*s).cx as usize];
            if gce.flags as ::core::ffi::c_int & GRID_FLAG_EXTENDED != 0
                || (gc.flags as ::core::ffi::c_int != gce.flags as ::core::ffi::c_int)
                || (gc.attr as ::core::ffi::c_int
                    != gce.c2rust_unnamed.data.attr as ::core::ffi::c_int)
                || (gc.fg != gce.c2rust_unnamed.data.fg as ::core::ffi::c_int)
                || (gc.bg != gce.c2rust_unnamed.data.bg as ::core::ffi::c_int)
                || (gc.data.width as ::core::ffi::c_int != 1 as ::core::ffi::c_int)
                || (gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int)
                || (gce.c2rust_unnamed.data.data as ::core::ffi::c_int
                    != gc.data.data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int)
            {
                skip = 0 as ::core::ffi::c_int;
            }
        }
    }
    selected = screen_check_selection(&*s, (*s).cx, (*s).cy);
    if selected != 0 && !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_SELECTED != 0 {
        tmp_gc = *gc;
        tmp_gc.flags = (tmp_gc.flags as ::core::ffi::c_int | GRID_FLAG_SELECTED) as u_char;
        grid_view_set_cell((*s).grid_mut(), (*s).cx, (*s).cy, &tmp_gc);
    } else if selected == 0 && gc.flags as ::core::ffi::c_int & GRID_FLAG_SELECTED != 0 {
        tmp_gc = *gc;
        tmp_gc.flags = (tmp_gc.flags as ::core::ffi::c_int & !GRID_FLAG_SELECTED) as u_char;
        grid_view_set_cell((*s).grid_mut(), (*s).cx, (*s).cy, &tmp_gc);
    } else if skip == 0 {
        grid_view_set_cell((*s).grid_mut(), (*s).cx, (*s).cy, gc);
    }
    if selected != 0 {
        skip = 0 as ::core::ffi::c_int;
    }
    if let Some(wp_value) = wp {
        xoff = wp_value.geometry().2;
        yoff = wp_value.geometry().3;
    }
    WindowPane::visible_ranges(
        wp,
        (xoff as u_int).wrapping_add((*s).cx) as ::core::ffi::c_int,
        (*s).cy.wrapping_add(yoff as u_int) as ::core::ffi::c_int,
        width,
        &mut r,
    );
    not_wrap = ((*s).mode & MODE_WRAP == 0) as ::core::ffi::c_int as u_int;
    if (*s).cx <= sx.wrapping_sub(not_wrap).wrapping_sub(width) {
        screen_write_set_cursor(
            ctx,
            (*s).cx.wrapping_add(width) as ::core::ffi::c_int,
            -(1 as ::core::ffi::c_int),
        );
    } else {
        screen_write_set_cursor(
            ctx,
            sx.wrapping_sub(not_wrap) as ::core::ffi::c_int,
            -(1 as ::core::ffi::c_int),
        );
    }
    if (*s).mode & MODE_INSERT != 0 {
        // tmux's insert path uses the ranges left by the nested flush when
        // deciding whether to paint this cell. Preserve that observable
        // behavior explicitly, with caller-owned storage instead of a pane's
        // shared scratch buffer.
        screen_write_collect_flush_with_ranges(ctx, 0, "screen_write_cell", &mut r);
        ttyctx.data = tty_command_data::Count(width);
        if screen_write_should_draw_line(ctx, (*s).cy) != 0 {
            tty_write(
                |tty, ctx| tty_cmd_insertcharacter(tty, ctx, &*s),
                &mut ttyctx,
            );
        }
    }
    if skip != 0 || screen_write_should_draw_line(ctx, (*s).cy) == 0 {
        return;
    }
    if redraw != 0 && wp.is_some() {
        screen_write_redraw_line(ctx, &mut ttyctx, (*s).cy, &mut r);
        return;
    }
    if selected != 0 {
        if let Some(selected) = screen_select_cell(&*s, gc) {
            tmp_gc = selected;
        }
    } else {
        tmp_gc = *gc;
    }
    i = 0 as u_int;
    vis = 0 as u_int;
    while (i as usize) < r.len() {
        vis = vis.wrapping_add(r[i as usize].nx);
        i = i.wrapping_add(1);
    }
    if vis >= width {
        if screen_write_should_draw_line(ctx, (*s).cy) != 0 {
            tty_write(|tty, ctx| tty_cmd_cell(tty, ctx, &*s, &tmp_gc), &mut ttyctx);
        }
        return;
    }
    utf8_set(&mut tmp_gc.data, ' ' as i32 as u_char);
    if screen_write_should_draw_line(ctx, (*s).cy) == 0 {
        return;
    }
    i = 0 as u_int;
    while (i as usize) < r.len() {
        let ri = &r[i as usize];
        if !(ri.nx == 0 as u_int) {
            n = 0 as u_int;
            while n < ri.nx {
                ttyctx.ocx =
                    (ri.px as ::core::ffi::c_int - xoff + n as ::core::ffi::c_int) as u_int;
                tty_write(|tty, ctx| tty_cmd_cell(tty, ctx, &*s, &tmp_gc), &mut ttyctx);
                n = n.wrapping_add(1);
            }
        }
        i = i.wrapping_add(1);
    }
}
unsafe fn screen_write_combine(ctx: &mut screen_write_ctx, gc: &grid_cell) -> ::core::ffi::c_int {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut r = Vec::new();
    let mut s: *mut screen = ctx.screen_ptr();
    let wp = write_pane;
    let ud = &gc.data;
    let mut oo: *mut options = global_options;
    let mut i: u_int = 0;
    let mut n: u_int = 0;
    let mut cx: u_int = (*s).cx;
    let mut cy: u_int = (*s).cy;
    let mut vis: u_int = 0;
    let mut last: grid_cell = grid_cell {
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
    let mut ttyctx = tty_ctx::default();
    let mut force_wide: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut zero_width: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut xoff: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut yoff: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if utf8_is_hangul_filler(ud) != 0 {
        return 1 as ::core::ffi::c_int;
    }
    if utf8_is_zwj(ud) != 0 {
        zero_width = 1 as ::core::ffi::c_int;
    } else if utf8_is_vs(ud) != 0 {
        zero_width = 1 as ::core::ffi::c_int;
        if options_get_number(oo, c"variation-selector-always-wide") != 0 {
            force_wide = 1 as ::core::ffi::c_int;
        }
    } else if ud.width as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
        zero_width = 1 as ::core::ffi::c_int;
    }
    if (ud.size as ::core::ffi::c_int) < 2 as ::core::ffi::c_int || cx == 0 as u_int {
        return zero_width;
    }
    log_debug(format_args!(
        "{}: character {} at {},{} (width {})",
        "screen_write_combine",
        log_bytes(&ud.data[..ud.size as usize]),
        (cx) as u32,
        (cy) as u32,
        (ud.width as ::core::ffi::c_int) as u32
    ));
    n = 1 as u_int;
    grid_view_get_cell((*s).grid(), cx.wrapping_sub(n), cy, &mut last);
    if cx != 1 as u_int && last.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
        n = 2 as u_int;
        grid_view_get_cell((*s).grid(), cx.wrapping_sub(n), cy, &mut last);
    }
    if n != last.data.width as u_int || last.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
        return zero_width;
    }
    if zero_width == 0 {
        match hanguljamo_check_state(&last.data, ud) as ::core::ffi::c_uint {
            3 => return 1 as ::core::ffi::c_int,
            1 => return 0 as ::core::ffi::c_int,
            0 => {
                if utf8_should_combine(&last.data, ud) != 0
                    || (utf8_should_combine(ud, &last.data) != 0)
                {
                    force_wide = 1 as ::core::ffi::c_int;
                } else if utf8_has_zwj(&last.data) == 0 {
                    return 0 as ::core::ffi::c_int;
                }
            }
            _ => {}
        }
    }
    if (last.data.size as ::core::ffi::c_int + ud.size as ::core::ffi::c_int) as usize
        > ::core::mem::size_of::<[u_char; 32]>() as usize
    {
        return zero_width;
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_combine");
    log_debug(format_args!(
        "{}: {} -> {} at {},{} (offset {}, width {})",
        "screen_write_combine",
        log_bytes(&ud.data[..ud.size as usize]),
        log_bytes(&last.data.data[..last.data.size as usize]),
        (cx.wrapping_sub(n)) as u32,
        (cy) as u32,
        (n) as u32,
        (last.data.width as ::core::ffi::c_int) as u32
    ));
    let size = last.data.size as usize;
    let combined_size = size + ud.size as usize;
    last.data.data[size..combined_size].copy_from_slice(&ud.data[..ud.size as usize]);
    last.data.size = combined_size as u_char;
    if last.data.width as ::core::ffi::c_int == 1 as ::core::ffi::c_int && force_wide != 0 {
        last.data.width = 2 as u_char;
        n = 2 as u_int;
        cx = cx.wrapping_add(1);
    } else {
        force_wide = 0 as ::core::ffi::c_int;
    }
    grid_view_set_cell((*s).grid_mut(), cx.wrapping_sub(n), cy, &last);
    if force_wide != 0 {
        grid_view_set_padding((*s).grid_mut(), cx.wrapping_sub(1 as u_int), cy, last.bg);
    }
    if let Some(wp_value) = wp {
        xoff = wp_value.geometry().2;
        yoff = wp_value.geometry().3;
    }
    WindowPane::visible_ranges(
        wp,
        (xoff as u_int).wrapping_add(cx).wrapping_sub(n) as ::core::ffi::c_int,
        cy.wrapping_add(yoff as u_int) as ::core::ffi::c_int,
        n,
        &mut r,
    );
    i = 0 as u_int;
    vis = 0 as u_int;
    while (i as usize) < r.len() {
        vis = vis.wrapping_add(r[i as usize].nx);
        i = i.wrapping_add(1);
    }
    if vis < n {
        return 1 as ::core::ffi::c_int;
    }
    screen_write_set_cursor(
        ctx,
        cx.wrapping_sub(n) as ::core::ffi::c_int,
        cy as ::core::ffi::c_int,
    );
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if force_wide != 0 {
        ttyctx.flags |= TTY_CTX_CELL_INVALIDATE;
    }
    if screen_write_should_draw_line(ctx, cy) != 0 {
        tty_write(|tty, ctx| tty_cmd_cell(tty, ctx, &*s, &last), &mut ttyctx);
    }
    screen_write_set_cursor(ctx, cx as ::core::ffi::c_int, cy as ::core::ffi::c_int);
    1 as ::core::ffi::c_int
}
unsafe fn screen_write_overwrite(
    gd: &mut grid,
    cx: u_int,
    cy: u_int,
    gc: &grid_cell,
    mut width: u_int,
) -> ::core::ffi::c_int {
    let mut tmp_gc: grid_cell = grid_cell {
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
    let mut xx: u_int = 0;
    let mut done: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
        xx = cx.wrapping_add(1 as u_int);
        loop {
            xx = xx.wrapping_sub(1);
            if !(xx > 0 as u_int) {
                break;
            }
            grid_view_get_cell(gd, xx, cy, &mut tmp_gc);
            if !(tmp_gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
                break;
            }
            log_debug(format_args!(
                "{}: padding at {},{}",
                "screen_write_overwrite",
                (xx) as u32,
                { cy }
            ));
            screen_write_clear_cell(gd, xx, cy);
        }
        log_debug(format_args!(
            "{}: character at {},{}",
            "screen_write_overwrite",
            (xx) as u32,
            { cy }
        ));
        screen_write_clear_cell(gd, xx, cy);
        done = 1 as ::core::ffi::c_int;
    }
    if width != 1 as u_int
        || gc.data.width as ::core::ffi::c_int != 1 as ::core::ffi::c_int
        || gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0
    {
        xx = cx.wrapping_add(width).wrapping_sub(1 as u_int);
        loop {
            xx = xx.wrapping_add(1);
            if !(xx < gd.sx) {
                break;
            }
            grid_view_get_cell(gd, xx, cy, &mut tmp_gc);
            if !(tmp_gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
                break;
            }
            log_debug(format_args!(
                "{}: overwrite at {},{}",
                "screen_write_overwrite",
                (xx) as u32,
                { cy }
            ));
            screen_write_clear_cell(gd, xx, cy);
            done = 1 as ::core::ffi::c_int;
        }
    }
    done
}
pub unsafe fn screen_write_setselection(ctx: &mut screen_write_ctx, clip: &CStr, data: &[u8]) {
    let mut ttyctx = tty_ctx::default();
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    ttyctx.data = tty_command_data::Selection { clip, data };
    // tty_write consumes the borrowed payload synchronously.
    tty_write(|tty, ctx| tty_cmd_setselection(tty, ctx), &mut ttyctx);
}
pub unsafe fn screen_write_rawstring(
    ctx: &mut screen_write_ctx,
    data: &[u8],
    mut allow_invisible_panes: ::core::ffi::c_int,
) {
    let mut ttyctx = tty_ctx::default();
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if allow_invisible_panes != 0 {
        ttyctx.flags |= TTY_CTX_INVISIBLE_PANES;
    }
    ttyctx.data = tty_command_data::Bytes(data);
    // tty_write consumes the borrowed payload synchronously.
    tty_write(|tty, ctx| tty_cmd_rawstring(tty, ctx), &mut ttyctx);
}
pub unsafe fn screen_write_alternateon(
    ctx: &mut screen_write_ctx,
    gc: &grid_cell,
    cursor: ::core::ffi::c_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut ttyctx = tty_ctx::default();
    let _wp = write_pane;
    if write_pane_owner
        .as_ref()
        .is_some_and(|pane| !pane.alternate_screen_allowed())
    {
        return;
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_alternateon");
    if screen_alternate_on(&mut *ctx.screen_ptr(), gc, cursor) == 0 {
        return;
    }
    if let Some(pane) = write_pane_owner.as_ref() {
        pane.alternate_screen_changed(true);
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if let Some(redraw_cb) = &ttyctx.redraw_cb {
        redraw_cb(&ttyctx);
    }
}
pub unsafe fn screen_write_alternateoff(
    ctx: &mut screen_write_ctx,
    gc: &mut grid_cell,
    cursor: ::core::ffi::c_int,
) {
    let write_pane_owner = ctx.wp.upgrade();
    let write_pane = write_pane_owner.as_ref();
    let mut ttyctx = tty_ctx::default();
    let _wp = write_pane;
    if write_pane_owner
        .as_ref()
        .is_some_and(|pane| !pane.alternate_screen_allowed())
    {
        return;
    }
    screen_write_collect_flush(ctx, 0 as ::core::ffi::c_int, "screen_write_alternateoff");
    if screen_alternate_off(&mut *ctx.screen_ptr(), Some(gc), cursor) == 0 {
        return;
    }
    if let Some(pane) = write_pane_owner.as_ref() {
        pane.alternate_screen_changed(false);
    }
    screen_write_initctx(
        ctx,
        &mut ttyctx,
        1 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if let Some(redraw_cb) = &ttyctx.redraw_cb {
        redraw_cb(&ttyctx);
    }
}

#[cfg(test)]
mod write_ctx_tests {
    use super::*;
    use crate::src::shared::colour::colour_palette;
    use std::{cell::Cell, rc::Rc};

    struct DropCounter(Rc<Cell<usize>>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn restarting_a_stopped_context_releases_its_previous_callback() {
        unsafe {
            let mut s = screen::empty();
            s.grid = Some(crate::src::grid::grid_create(8, 2, 0));
            let mut ctx = screen_write_ctx::default();
            let drops = Rc::new(Cell::new(0));
            for pass in 0..3 {
                let owner = DropCounter(drops.clone());
                screen_write_start_callback(
                    &mut ctx,
                    &mut s,
                    Some(Box::new(move |_| {
                        let _ = &owner;
                    })),
                );
                assert_eq!(drops.get(), pass);
                assert!(ctx.item.is_some());
                assert_eq!(ctx.bg, 8);
                screen_write_collect_add(&mut ctx, &grid_default_cell);
                screen_write_stop(&mut ctx);
                assert!(ctx.item.is_none());
                assert_eq!(drops.get(), pass);
            }
            // Restarting without a callback must also release the previous owner.
            screen_write_start(&mut ctx, &mut s);
            assert_eq!(drops.get(), 3);
            assert!(ctx.init_ctx_cb.is_none());
            screen_write_stop(&mut ctx);
            assert_eq!(s.cx, 3);
        }
    }

    #[test]
    fn callback_defaults_and_palette_fallback_share_the_owned_style_cell() {
        unsafe {
            let mut s = screen::empty();
            s.grid = Some(crate::src::grid::grid_create(8, 2, 0));
            let palette = refbox::RefBox::new(colour_palette {
                fg: 3,
                bg: 4,
                ..Default::default()
            });
            let palette_observer = palette.downgrade();
            let mut ctx = screen_write_ctx {
                flags: SCREEN_WRITE_SYNC,
                init_ctx_cb: Some(Box::new(move |ttyctx| {
                    ttyctx.style_ctx.defaults.fg = 7;
                    ttyctx.style_ctx.palette =
                        crate::src::shared::tty::PaletteSource::Popup(palette_observer.clone());
                })),
                ..Default::default()
            };
            ctx.borrow_screen(&raw mut s);
            let mut ttyctx = tty_ctx::default();
            screen_write_initctx(&mut ctx, &mut ttyctx, 0, 0);
            assert_eq!(
                (ttyctx.style_ctx.defaults.fg, ttyctx.style_ctx.defaults.bg),
                (7, 4)
            );
            let previous = std::hint::black_box(ttyctx);
            palette.try_borrow_mut().unwrap().bg = 6;
            ttyctx = tty_ctx::default();
            screen_write_initctx(&mut ctx, &mut ttyctx, 0, 0);
            assert_eq!(
                (ttyctx.style_ctx.defaults.fg, ttyctx.style_ctx.defaults.bg),
                (7, 6)
            );
            assert_eq!(
                (
                    previous.style_ctx.defaults.fg,
                    previous.style_ctx.defaults.bg
                ),
                (7, 4)
            );
        }
    }

    #[test]
    fn resetting_a_terminal_context_drops_both_owned_callbacks() {
        unsafe {
            let mut s = screen::empty();
            s.grid = Some(crate::src::grid::grid_create(8, 2, 0));
            s.cx = 3;
            s.cy = 1;
            s.rlower = 1;
            let mut ctx = screen_write_ctx {
                flags: SCREEN_WRITE_SYNC,
                ..Default::default()
            };
            ctx.borrow_screen(&raw mut s);
            let drops = Rc::new(Cell::new(0));
            let mut ttyctx = tty_ctx::default();
            for reset in 1..=3 {
                let redraw_owner = DropCounter(drops.clone());
                ttyctx.redraw_cb = Some(Box::new(move |_| {
                    let _ = &redraw_owner;
                }));
                let client_owner = DropCounter(drops.clone());
                ttyctx.set_client_cb = Some(Box::new(move |_, _| {
                    let _ = &client_owner;
                    0
                }));
                screen_write_initctx(&mut ctx, &mut ttyctx, 0, 0);
                assert_eq!(drops.get(), reset * 2);
                assert_eq!((ttyctx.sx, ttyctx.sy, ttyctx.ocx, ttyctx.ocy), (8, 2, 3, 1));
                assert!(grid_cells_equal(
                    &ttyctx.style_ctx.defaults,
                    &grid_default_cell
                ));
                assert!(ttyctx.set_client_cb.is_none());
            }
        }
    }
}

#[cfg(test)]
mod write_cell_tests {
    use super::*;
    use crate::src::grid::grid_create;

    #[test]
    fn overwriting_wide_cells_clears_surrounding_padding_and_preserves_backgrounds() {
        unsafe {
            for (cx, width, cleared) in [
                (1, 3, vec![0, 1, 4]),
                (0, 1, vec![1]),
                (3, 1, vec![4]),
                (2, 3, vec![]),
            ] {
                let mut gd = grid_create(8, 1, 0);
                for x in 0..8 {
                    let mut cell = grid_default_cell;
                    cell.data.data[0] = b'A' + x as u8;
                    cell.bg = 100 + x as i32;
                    grid_view_set_cell(&mut gd, x, 0, &cell);
                }
                for x in [0, 3] {
                    let mut cell = grid_default_cell;
                    cell.data.data[..3].copy_from_slice("漢".as_bytes());
                    cell.data.size = 3;
                    cell.data.width = 2;
                    cell.bg = 200 + x as i32;
                    grid_view_set_cell(&mut gd, x, 0, &cell);
                    grid_view_set_padding(&mut gd, x + 1, 0, cell.bg);
                }
                let before: Vec<_> = (0..8)
                    .map(|x| {
                        let mut cell = grid_default_cell;
                        grid_view_get_cell(&gd, x, 0, &mut cell);
                        cell
                    })
                    .collect();
                assert_eq!(
                    screen_write_overwrite(&mut gd, cx, 0, &before[cx as usize], width),
                    (!cleared.is_empty()) as i32,
                );
                for x in 0..8 {
                    let mut after = grid_default_cell;
                    grid_view_get_cell(&gd, x, 0, &mut after);
                    let mut expected = before[x as usize];
                    if cleared.contains(&x) {
                        let bg = expected.bg;
                        expected = grid_default_cell;
                        expected.bg = bg;
                    }
                    assert!(
                        grid_cells_equal(&after, &expected),
                        "cursor {cx}, width {width}, cell {x}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod write_row_tests {
    use super::*;

    #[test]
    fn scrolling_a_region_moves_rows_and_resize_rebuilds_the_owned_slice() {
        unsafe {
            let mut s = screen::empty();
            s.grid = Some(crate::src::grid::grid_create(4, 5, 0));
            s.rupper = 1;
            s.rlower = 3;
            screen_write_make_list(&mut s);
            let mut allocations = Vec::new();
            for (index, row) in s.write_rows_mut().iter_mut().enumerate() {
                row.data.resize_with(4, || b'A' + index as u8);
                allocations.push(row.data.as_ptr());
                let mut item = screen_write_get_citem();
                item.bg = index as u_int;
                row.items.push_back(item);
            }
            let mut ctx = screen_write_ctx::default();
            ctx.borrow_screen(&raw mut s);
            screen_write_collect_scroll(&mut ctx, 99);
            allocations[1..=3].rotate_left(1);
            for (index, &expected) in [0, 2, 3, 99, 4].iter().enumerate() {
                let row = &mut s.write_rows_mut()[index];
                assert_eq!(row.data.as_ptr(), allocations[index]);
                let item = row.items.front().unwrap();
                assert_eq!(item.bg, expected);
                assert!(row.items.get(1).is_none());
            }
            assert_eq!(s.write_rows()[3].data[0], b'B');
            let item = s.write_rows()[3].items.front().unwrap();
            assert_eq!((item.x, item.used, item.type_0), (0, 4, CLEAR));

            s.rupper = 2;
            s.rlower = 2;
            screen_write_collect_scroll(&mut ctx, 100);
            assert_eq!(s.write_rows()[2].data.as_ptr(), allocations[2]);
            assert_eq!(s.write_rows()[2].items.front().unwrap().bg, 100);

            crate::src::screen::screen_resize(&mut s, 6, 3, 0);
            assert_eq!(s.write_rows().len(), 3);
            assert!(s
                .write_rows()
                .iter()
                .all(|row| row.data.is_empty() && row.items.is_empty()));
            screen_write_free_list(&mut s);
            assert!(s.write_list.is_none());

            // Moving and dropping a screen also releases its still-owned rows.
            screen_write_make_list(&mut s);
            s.write_rows_mut()[0].data.resize_with(6, || 1);
            s.write_rows_mut()[0]
                .items
                .push_back(screen_write_get_citem());
            let moved = s;
            drop(moved);
        }
    }

    #[test]
    fn scroll_moves_text_owners_and_tears_down_rows() {
        unsafe {
            let mut s: screen = screen::empty();
            s.grid = Some(crate::src::grid::grid_create(4, 3, 0));
            s.rupper = 0;
            s.rlower = 2;
            screen_write_make_list(&mut s);
            let mut pointers = Vec::new();
            for y in 0..3 {
                let row = &mut s.write_rows_mut()[y];
                row.data.resize_with(4, || b'A' + y as u8);
                pointers.push(row.data.as_ptr());
            }
            let mut ctx: screen_write_ctx = Default::default();
            ctx.borrow_screen(&raw mut s);
            for _ in 0..4 {
                screen_write_collect_scroll(&mut ctx, 8);
                pointers.rotate_left(1);
                for (y, pointer) in pointers.iter().enumerate() {
                    assert_eq!(s.write_rows()[y].data.as_ptr(), *pointer);
                }
            }
            // Split and trim queued commands without losing ownership or wrap
            // state, then reuse the detached nodes through the same pool.
            screen_write_recycle_items(&mut s.write_rows_mut()[0].items);
            ctx.item = Some(screen_write_get_citem());
            for (x, used, wrapped) in [(0, 10, 1), (3, 3, 0)] {
                let item = screen_write_current_item(&mut ctx);
                item.x = x;
                item.used = used;
                item.wrapped = wrapped;
                screen_write_collect_insert(&mut ctx);
            }
            let mut intervals = Vec::new();
            let mut index = 0;
            while let Some(item) = s.write_rows()[0].items.get(index) {
                intervals.push((item.x, item.used, item.wrapped));
                index += 1;
            }
            assert_eq!(intervals, [(0, 3, 1), (3, 3, 0), (6, 4, 0)]);
            let items = &mut s.write_rows_mut()[0].items;
            let (tail, wrapped) = screen_write_collect_trim(items, 0, 8);
            assert!(wrapped);
            assert_eq!(
                (items.get(tail).unwrap().x, items.get(tail).unwrap().used),
                (8, 2)
            );
            screen_write_collect_trim(items, 9, 1);
            assert_eq!(
                (items.get(tail).unwrap().x, items.get(tail).unwrap().used),
                (8, 1)
            );
            screen_write_collect_trim(items, 0, 20);
            assert!(items.is_empty());
            screen_write_free_citem(ctx.item.take().unwrap());
            screen_write_free_list(&mut s);
            assert!(s.write_list.is_none());
        }
    }
}
