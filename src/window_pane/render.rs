//! Pane rendering operations. Model access ends before terminal or format callbacks.
use super::*;
use crate::src::format::{format_create, format_defaults};
use crate::src::grid::grid_default_cell;
use crate::src::prompt::prompt_draw;
use crate::src::screen_redraw::{redraw_draw_ctx, REDRAW_SCROLLBAR_LEFT, REDRAW_STATUS_TOP};
use crate::src::screen_write::{screen_write_init, screen_write_start, screen_write_stop};
use crate::src::shared::client::ClientRef;
use crate::src::shared::format::{FORMAT_NOJOBS, FORMAT_PANE};
use crate::src::shared::prompt::prompt_draw_data;
use crate::src::shared::redraw::redraw_span;
use crate::src::shared::screen_write::screen_write_ctx;
use crate::src::shared::style::style;
use crate::src::shared::tty::tty_style_ctx;
use crate::src::tty::{tty_cell, tty_cursor};
use crate::src::tty_draw::tty_draw_line;
use crate::src::window_copy::window_copy_get_current_offset;

pub(super) unsafe fn refresh_scrollbar_style(pane: &Rc<UnsafeCell<window_pane>>) {
    let mut updated = (*pane.get()).scrollbar_style;
    style_set_scrollbar_style_from_option(&mut updated, &pane.borrow_options_mut());
    (*pane.get()).scrollbar_style = updated;
}

/// Resolve the selected mode after releasing pane storage. The mode stack keeps
/// its screen alive until the bounded drawing or write operation finishes.
unsafe fn displayed_screen(pane: &Rc<UnsafeCell<window_pane>>) -> *mut screen {
    let mode = match &(*pane.get()).screen_source {
        PaneScreenSource::Base => return &raw mut (*pane.get()).base,
        PaneScreenSource::Mode(observer) => observer.clone(),
    };
    if !mode.is_alive() {
        return std::ptr::null_mut();
    }
    let display = mode
        .get_unchecked()
        .mode
        .display_screen
        .expect("mode display screen getter");
    display(mode)
}

pub(super) unsafe fn prepare_write(
    pane: &Rc<UnsafeCell<window_pane>>,
    context: &mut screen_write_ctx,
    requested: *mut screen,
) {
    let screen = if requested.is_null() {
        displayed_screen(pane)
    } else {
        requested
    };
    screen_write_init(context, screen);
    context.wp = Rc::downgrade(pane);
}

pub(super) unsafe fn is_obscured(pane: &Rc<UnsafeCell<window_pane>>) -> bool {
    let (sx, sy, xoff, yoff) = pane.geometry();
    let window = pane.window_observer().upgrade().expect("live pane parent");
    let (wsx, wsy) = window.logical_size();
    let outside = xoff < 0
        || yoff < 0
        || (xoff as u32).wrapping_add(sx) > wsx
        || (yoff as u32).wrapping_add(sy) > wsy;
    drop(window);
    outside
}

pub(super) unsafe fn alternate_screen_changed(pane: &Rc<UnsafeCell<window_pane>>, entered: bool) {
    let pending = !(*pane.get()).resize_queue.is_empty();
    if entered {
        window_pane_clear_resizes(&mut *pane.get(), std::ptr::null_mut());
        drop((*pane.get()).resize_timer.take());
    }
    let window = pane.window_observer().upgrade().expect("live pane parent");
    window.refit();
    if entered && (pending || !(*pane.get()).resize_queue.is_empty()) {
        let (sx, sy, _, _) = pane.geometry();
        window_pane_send_resize(&*pane.get(), sx, sy);
        window_pane_clear_resizes(&mut *pane.get(), std::ptr::null_mut());
    }
    server_redraw_window_borders(&window);
    drop(window);
}

fn apply_style(cell: &mut grid_cell, parsed: style) {
    if parsed.gc.fg != 8 {
        cell.fg = parsed.gc.fg;
    }
    if parsed.gc.bg != 8 {
        cell.bg = parsed.gc.bg;
    }
    if parsed.gc.us != 8 {
        cell.us = parsed.gc.us;
    }
    cell.attr |= parsed.gc.attr;
}

pub(super) unsafe fn default_colours(pane: &Rc<UnsafeCell<window_pane>>) -> (grid_cell, u32) {
    if (*pane.get()).flags & PANE_STYLECHANGED != 0 {
        log_debug(format_args!("%{}: style changed", pane.id()));
        (*pane.get()).flags &= !PANE_STYLECHANGED;
        let mut context =
            format_create(None, None, (FORMAT_PANE | pane.id()) as i32, FORMAT_NOJOBS);
        format_defaults(&mut *context, None, None, refbox::Weak::new(), Some(pane));
        for (key, active) in [(c"window-active-style", true), (c"window-style", false)] {
            let (fg, bg) = {
                let palette = pane.borrow_palette();
                (palette.fg, palette.bg)
            };
            let mut cell = grid_cell {
                fg,
                bg,
                ..grid_default_cell
            };
            {
                let state = &mut *pane.get();
                if active {
                    state.cached_active_gc = cell;
                } else {
                    state.cached_gc = cell;
                }
            }
            let parsed =
                crate::src::style::style_resolve_with_options(key, Some(&mut context), |visit| {
                    visit(&mut pane.borrow_options_mut());
                })
                .unwrap_or_else(|| {
                    let mut style = style::default();
                    crate::src::style::style_set(&mut style, &grid_default_cell);
                    style
                });
            apply_style(&mut cell, parsed);
            let state = &mut *pane.get();
            if active {
                state.cached_active_gc = cell;
                state.cached_active_dim = parsed.dim as u32;
            } else {
                state.cached_gc = cell;
                state.cached_dim = parsed.dim as u32;
            }
        }
        drop(context);
    }
    let window = pane.window_observer().upgrade().expect("live pane parent");
    let active = window.active_pane().ptr_eq(&Rc::downgrade(pane));
    drop(window);
    let state = &*pane.get();
    let mut cell = grid_default_cell;
    cell.fg = if active && state.cached_active_gc.fg != 8 {
        state.cached_active_gc.fg
    } else {
        state.cached_gc.fg
    };
    cell.bg = if active && state.cached_active_gc.bg != 8 {
        state.cached_active_gc.bg
    } else {
        state.cached_gc.bg
    };
    (
        cell,
        if active {
            state.cached_active_dim
        } else {
            state.cached_dim
        },
    )
}

pub(super) unsafe fn draw_line(
    pane: &Rc<UnsafeCell<window_pane>>,
    client: &ClientRef,
    source: (u32, u32),
    width: u32,
    destination: (u32, u32),
    status: bool,
) {
    let (defaults, dim) = if status {
        (grid_default_cell, 0)
    } else {
        pane.default_colours()
    };
    let target = if status {
        &raw mut (*pane.get()).status_screen
    } else {
        displayed_screen(pane)
    };
    let selected = std::mem::take(&mut *target);
    let context = tty_style_ctx {
        defaults,
        dim,
        palette: crate::src::shared::tty::PaletteSource::Pane(Rc::downgrade(pane)),
        hyperlinks: selected.hyperlinks.clone(),
    };
    let (px, py) = source;
    let (x, y) = destination;
    let available = selected.grid().sx.saturating_sub(px);
    if available != 0 {
        tty_draw_line(
            client,
            &selected,
            px,
            py,
            width.min(available),
            x,
            y,
            (!status).then_some(&context),
        );
    }
    *target = selected;
}

pub(super) unsafe fn draw_scrollbar(
    pane_owner: &Rc<UnsafeCell<window_pane>>,
    client_owner: &ClientRef,
    span: &redraw_span,
    mut x: u32,
    mut y: u32,
    mut n: u32,
) {
    let wp = pane_owner.get();
    let sb_style = (*wp).scrollbar_style;
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
    let mut slgc: grid_cell = grid_cell {
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
    let mut pad_gc: grid_cell = grid_cell {
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
    let mut gcp: *mut grid_cell = ::core::ptr::null_mut::<grid_cell>();
    let mut pct_view: ::core::ffi::c_double = 0.;
    let mut total_height: u_int = 0;
    let mut slider_h: u_int = 0;
    let mut slider_y: u_int = 0;
    let mut sb_h: u_int = span.data.scrollbar().height;
    let mut sb_y: u_int = span.data.scrollbar().y;
    let mut i: u_int = 0;
    let mut off: u_int = 0;
    let mut sb_w: u_int = 0;
    let mut sb_pad: u_int = 0;
    if window_pane_mode(&*wp) == WINDOW_PANE_NO_MODE {
        total_height = {
            let screen = &*displayed_screen(pane_owner);
            screen.grid().sy.wrapping_add(screen.grid().hsize)
        };
        if total_height == 0 as u_int {
            return;
        }
        pct_view = sb_h as ::core::ffi::c_double / total_height as ::core::ffi::c_double;
        slider_h = (sb_h as ::core::ffi::c_double * pct_view) as u_int;
        slider_y = sb_h.wrapping_sub(slider_h);
    } else {
        if (*wp).modes.is_empty() {
            return;
        }
        let Some((cm_y, cm_size)) = window_copy_get_current_offset(pane_owner) else {
            return;
        };
        total_height = (cm_size as u_int).wrapping_add(sb_h);
        if total_height == 0 as u_int {
            return;
        }
        pct_view = sb_h as ::core::ffi::c_double / total_height as ::core::ffi::c_double;
        slider_h = (sb_h as ::core::ffi::c_double * pct_view) as u_int;
        slider_y = (sb_h.wrapping_add(1 as u_int) as ::core::ffi::c_double
            * (cm_y as ::core::ffi::c_double / total_height as ::core::ffi::c_double))
            as u_int;
    }
    if slider_h < 1 as u_int {
        slider_h = 1 as u_int;
    }
    if slider_y >= sb_h {
        slider_y = sb_h.wrapping_sub(1 as u_int);
    }
    (*wp).sb_slider_y = slider_y;
    (*wp).sb_slider_h = slider_h;
    gc = sb_style.gc;
    slgc = gc;
    slgc.fg = gc.bg;
    slgc.bg = gc.fg;
    pad_gc = pane_owner.default_colours().0;
    sb_w = sb_style.width as u_int;
    sb_pad = sb_style.pad as u_int;
    off = x.wrapping_sub(span.x);
    {
        let terminal = &(client_owner);
        tty_cursor(terminal, x, y)
    };
    let mut current_block_40: u64;
    i = 0 as u_int;
    while i < n {
        if span.data.scrollbar().flags & REDRAW_SCROLLBAR_LEFT != 0 {
            if off.wrapping_add(i) >= sb_w && off.wrapping_add(i) < sb_w.wrapping_add(sb_pad) {
                tty_cell(client_owner, &pad_gc, None);
                current_block_40 = 3437258052017859086;
            } else {
                current_block_40 = 7828949454673616476;
            }
        } else if off.wrapping_add(i) < sb_pad {
            tty_cell(client_owner, &pad_gc, None);
            current_block_40 = 3437258052017859086;
        } else {
            current_block_40 = 7828949454673616476;
        }
        if current_block_40 == 7828949454673616476 {
            if sb_y >= slider_y && sb_y < slider_y.wrapping_add(slider_h) {
                gcp = &raw mut slgc;
            } else {
                gcp = &raw mut gc;
            }
            tty_cell(client_owner, &*gcp, None);
        }
        i = i.wrapping_add(1);
    }
}

pub(super) unsafe fn draw_prompt(
    wp_owner: &Rc<UnsafeCell<window_pane>>,
    dctx: &mut redraw_draw_ctx<'_>,
) {
    let wp = wp_owner.get();
    let prompt = (*wp).prompt.as_ref().map(|prompt| prompt.downgrade());
    let (pane_width, pane_height, xoff, yoff) = wp_owner.geometry();
    let scene = dctx.scene;
    let Some(client_owner) = scene.c.upgrade() else {
        return;
    };
    let _c: Option<ClientRef> = Some(client_owner.clone());
    let mut screen: screen = screen::empty();
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    let mut ox: ::core::ffi::c_int = scene.ox as ::core::ffi::c_int;
    let mut oy: ::core::ffi::c_int = scene.oy as ::core::ffi::c_int;
    let mut sx: ::core::ffi::c_int = scene.sx as ::core::ffi::c_int;
    let mut sy: ::core::ffi::c_int = scene.sy as ::core::ffi::c_int;
    let mut line: ::core::ffi::c_int = 0;
    let mut cy: ::core::ffi::c_int = 0;
    let mut px: ::core::ffi::c_int = 0;
    let mut offset: ::core::ffi::c_int = 0;
    let mut width: ::core::ffi::c_int = 0;
    let mut wy: ::core::ffi::c_int = 0;
    if prompt.is_none() || pane_width == 0 as u_int || pane_height == 0 as u_int {
        return;
    }
    if !dctx.flags & REDRAW_STATUS_TOP != 0 {
        wy = yoff + pane_height as ::core::ffi::c_int - 1 as ::core::ffi::c_int;
    } else {
        wy = yoff;
    }
    if wy < oy || wy >= oy + sy {
        return;
    }
    line = wy - oy;
    if dctx.flags & REDRAW_STATUS_TOP != 0 {
        cy = dctx.status_lines.wrapping_add(line as u_int) as ::core::ffi::c_int;
    } else {
        cy = line;
    }
    if xoff + pane_width as ::core::ffi::c_int <= ox || xoff >= ox + sx {
        return;
    }
    if xoff < ox {
        offset = ox - xoff;
        px = 0 as ::core::ffi::c_int;
    } else {
        offset = 0 as ::core::ffi::c_int;
        px = xoff - ox;
    }
    width = pane_width.wrapping_sub(offset as u_int) as ::core::ffi::c_int;
    if px + width > sx {
        width = sx - px;
    }
    screen_init(&mut screen, pane_width, 1 as u_int, 0 as u_int);
    screen_write_start(&mut ctx, &raw mut screen);
    let pdd = prompt_draw_data {
        area_x: 0 as u_int,
        area_width: pane_width,
        prompt_line: 0 as u_int,
    };
    let prompt_cx = prompt_draw(
        &prompt
            .as_ref()
            .expect("active prompt")
            .try_borrow_mut()
            .expect("unborrowed prompt"),
        &mut ctx,
        pdd,
    );
    (*wp).prompt_cx = prompt_cx;
    screen_write_stop(&mut ctx);
    {
        let terminal = &(client_owner);
        tty_draw_line(
            terminal,
            &screen,
            offset as u_int,
            0 as u_int,
            width as u_int,
            px as u_int,
            cy as u_int,
            None,
        );
    };
    drop(screen);
}

pub(super) unsafe fn visible_ranges(
    base_wp: Option<&Rc<UnsafeCell<window_pane>>>,
    mut px: ::core::ffi::c_int,
    py: ::core::ffi::c_int,
    mut width: u_int,
    ranges: &mut Vec<visible_range>,
) {
    ranges.clear();
    if py < 0 || width == 0 {
        return;
    }
    if px < 0 {
        if -px as u_int >= width {
            return;
        }
        width = width.wrapping_sub(-px as u_int);
        px = 0;
    }
    let Some(base_wp) = base_wp else {
        ranges.push(visible_range {
            px: px as u_int,
            nx: width,
        });
        return;
    };

    let window = base_wp
        .window_observer()
        .upgrade()
        .expect("live pane parent");
    let window_size = window.logical_size();
    drop(window);
    if py as u_int >= window_size.1 || px as u_int >= window_size.0 {
        return;
    }
    if (px as u_int).wrapping_add(width) > window_size.0 {
        width = window_size.0.wrapping_sub(px as u_int);
    }
    ranges.push(visible_range {
        px: px as u_int,
        nx: width,
    });
}
