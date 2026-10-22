use super::control::{tty_check_codeset, tty_fake_bce};
use super::model::*;
use super::output::{
    terminal_term, tty_attributes, tty_cursor, tty_default_attributes, tty_margin_off, tty_putc,
    tty_putcode, tty_putcode_i, tty_putn, tty_region_off, tty_repeat_space, tty_update_mode,
};
use crate::grid::view::grid_view_get_cell;
use crate::grid::{grid_cells_look_equal, grid_default_cell, grid_get_line};
use crate::log::{fatalx, log_debug, log_get_level};
use crate::screen::screen_select_cell;
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::{ClientRef, CLIENT_UTF8};
use crate::shared::grid::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::screen::screen;
use crate::shared::tty::tty_style_ctx;
use crate::shared::tty::*;
use crate::tty::term::tty_term_has;

pub(super) type tty_draw_line_state = ::core::ffi::c_uint;
pub(super) const TTY_DRAW_LINE_DONE: tty_draw_line_state = 6;
pub(super) const TTY_DRAW_LINE_SAME: tty_draw_line_state = 5;
pub(super) const TTY_DRAW_LINE_EMPTY: tty_draw_line_state = 4;
pub(super) const TTY_DRAW_LINE_NEW2: tty_draw_line_state = 3;
pub(super) const TTY_DRAW_LINE_NEW1: tty_draw_line_state = 2;
pub(super) const TTY_DRAW_LINE_FLUSH: tty_draw_line_state = 1;
pub(super) const TTY_DRAW_LINE_FIRST: tty_draw_line_state = 0;
const TTY_DRAW_LINE_STATES: [&str; 7] = ["FIRST", "FLUSH", "NEW1", "NEW2", "EMPTY", "SAME", "DONE"];

fn tty_draw_line_clear(
    client: &ClientRef,
    mut px: u_int,
    mut py: u_int,
    mut nx: u_int,
    defaults: &grid_cell,
    mut bg: u_int,
    mut wrapped: ::core::ffi::c_int,
) {
    if nx == 0 as u_int {
        return;
    }
    if !client.clips_terminal_output()
        && wrapped == 0
        && nx >= 10 as u_int
        && tty_fake_bce(&*client.borrow_terminal(), defaults, bg) == 0
    {
        if px.wrapping_add(nx) >= terminal_value!(client, sx)
            && tty_term_has(&terminal_term(client), TTYC_EL) != 0
        {
            tty_cursor(client, px, py);
            tty_putcode(client, TTYC_EL);
            return;
        }
        if px == 0 as u_int && tty_term_has(&terminal_term(client), TTYC_EL1) != 0 {
            tty_cursor(client, px.wrapping_add(nx).wrapping_sub(1 as u_int), py);
            tty_putcode(client, TTYC_EL1);
            return;
        }
        if tty_term_has(&terminal_term(client), TTYC_ECH) != 0 {
            tty_cursor(client, px, py);
            tty_putcode_i(client, TTYC_ECH, nx as ::core::ffi::c_int);
            return;
        }
    }
    if px != 0 as u_int || wrapped == 0 {
        tty_cursor(client, px, py);
    }
    if nx == 1 as u_int {
        tty_putc(client, ' ' as i32 as u_char);
    } else if nx == 2 as u_int {
        tty_putn(client, b"  ", 2 as u_int);
    } else {
        tty_repeat_space(client, nx);
    };
}
fn tty_draw_line_get_empty(gc: &grid_cell, last: &grid_cell, mut nx: u_int) -> u_int {
    let mut empty: u_int = 0 as u_int;
    if gc.data.width as u_int > nx {
        empty = nx;
    } else if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0
        || (gc.data.width as ::core::ffi::c_int == 0 as ::core::ffi::c_int)
    {
        empty = 1 as u_int;
    } else if gc.flags as ::core::ffi::c_int & GRID_FLAG_SELECTED != 0 {
        empty = 0 as u_int;
    } else if gc.bg == last.bg
        && gc.attr as ::core::ffi::c_int == 0 as ::core::ffi::c_int
        && gc.link == 0 as u_int
    {
        if gc.flags as ::core::ffi::c_int & GRID_FLAG_CLEARED != 0 {
            empty = 1 as u_int;
        } else if gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
            empty = gc.data.width as u_int;
        } else if gc.data.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
            && gc.data.data[0] as ::core::ffi::c_int == ' ' as i32
        {
            empty = 1 as u_int;
        }
    }
    empty
}
pub fn tty_draw_line(
    client: &ClientRef,
    s: &screen,
    mut px: u_int,
    mut py: u_int,
    mut nx: u_int,
    mut atx: u_int,
    mut aty: u_int,
    style_ctx: Option<&tty_style_ctx>,
) {
    let mut current_block: u64;
    let gd = s.grid();
    let mut converted = grid_cell::default();
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
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    let mut last_i: u_int = 0;
    let mut cx: u_int = 0;
    let mut ex: u_int = 0;
    let mut width: u_int = 0;
    let mut cellsize: u_int = 0;
    let mut bg: u_int = 0;
    let mut flags: ::core::ffi::c_int = 0;
    let mut empty: ::core::ffi::c_int = 0;
    let mut wrapped: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut buf = [0u8; 1000];
    let mut len: size_t = 0;
    let mut current_state: tty_draw_line_state = TTY_DRAW_LINE_FIRST;
    let mut next_state: tty_draw_line_state = TTY_DRAW_LINE_FIRST;
    let default_style_ctx = tty_style_ctx {
        defaults: grid_default_cell,
        hyperlinks: s.hyperlinks.clone(),
        ..Default::default()
    };
    let style_ctx = style_ctx.unwrap_or(&default_style_ctx);
    let defaults = &style_ctx.defaults;
    log_debug(format_args!(
        "{}: px={} py={} nx={} atx={} aty={}",
        "tty_draw_line",
        { px },
        { py },
        { nx },
        { atx },
        { aty }
    ));
    if atx >= terminal_value!(client, sx) {
        return;
    }
    if atx.wrapping_add(nx) >= terminal_value!(client, sx) {
        nx = terminal_value!(client, sx).wrapping_sub(atx);
    }
    if nx == 0 as u_int {
        return;
    }
    cellsize = grid_get_line(gd, gd.hsize.wrapping_add(py)).cellsize as u_int;
    if s.grid().sx > cellsize {
        ex = cellsize;
    } else {
        ex = s.grid().sx;
    }
    log_debug(format_args!(
        "{}: drawing {}-{},{} (end {}) at {},{}; defaults: fg={}, bg={}",
        "tty_draw_line",
        { px },
        { px.wrapping_add(nx) },
        { py },
        (ex) as u32,
        { atx },
        { aty },
        { defaults.fg },
        { defaults.bg }
    ));
    flags = terminal_value!(client, flags) & TTY_NOCURSOR;
    terminal_set!(client, flags, |=, TTY_NOCURSOR);
    tty_update_mode(client, terminal_value!(client, mode), Some(s.into()));
    tty_region_off(client);
    tty_margin_off(client);
    last = grid_default_cell;
    last.bg = defaults.bg;
    tty_default_attributes(client, 8 as u_int, Some(style_ctx));
    cx = 0 as u_int;
    i = px;
    while i < px.wrapping_add(nx) {
        grid_view_get_cell(gd, i, py, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        cx = cx.wrapping_add(1);
        i = i.wrapping_add(1);
    }
    if cx != 0 as u_int {
        i = px.wrapping_add(1 as u_int);
        while i > 0 as u_int {
            grid_view_get_cell(gd, i.wrapping_sub(1 as u_int), py, &mut gc);
            if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
                break;
            }
            i = i.wrapping_sub(1);
        }
        if i == 0 as u_int {
            bg = defaults.bg as u_int;
        } else {
            bg = gc.bg as u_int;
            if gc.flags as ::core::ffi::c_int & GRID_FLAG_SELECTED != 0 {
                ngc = gc;
                if let Some(selected) = screen_select_cell(s, &gc) {
                    ngc = selected;
                    bg = ngc.bg as u_int;
                }
            }
        }
        tty_attributes(client, &last, Some(style_ctx));
        log_debug(format_args!(
            "{}: clearing {} padding cells",
            "tty_draw_line",
            (cx) as u32
        ));
        tty_draw_line_clear(client, atx, aty, cx, defaults, bg, 0 as ::core::ffi::c_int);
        if cx == ex {
            current_block = 15064833524635049977;
        } else {
            atx = atx.wrapping_add(cx);
            px = px.wrapping_add(cx);
            nx = nx.wrapping_sub(cx);
            current_block = 16799951812150840583;
        }
    } else {
        current_block = 16799951812150840583;
    }
    if current_block == 16799951812150840583 {
        if py != 0 as u_int
            && atx == 0 as u_int
            && terminal_value!(client, cx) >= terminal_value!(client, sx)
            && terminal_value!(client, cy) != UINT_MAX
            && terminal_value!(client, cy).wrapping_add(1) == aty
            && nx == terminal_value!(client, sx)
        {
            let gl = grid_get_line(gd, gd.hsize.wrapping_add(py).wrapping_sub(1 as u_int));
            if gl.flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0 {
                wrapped = 1 as ::core::ffi::c_int;
            }
        }
        i = 0 as u_int;
        last_i = i;
        len = 0 as size_t;
        width = 0 as u_int;
        current_state = TTY_DRAW_LINE_FIRST;
        loop {
            let mut gcp: &grid_cell;
            if i == nx {
                empty = 0 as ::core::ffi::c_int;
                next_state = TTY_DRAW_LINE_DONE;
                gcp = &grid_default_cell;
            } else {
                if i > nx {
                    fatalx(|out| write!(out, "position {} > width {}", (i) as u32, { nx }));
                }
                if px >= ex || i >= ex.wrapping_sub(px) {
                    empty = nx.wrapping_sub(i) as ::core::ffi::c_int;
                    gcp = &grid_default_cell;
                } else {
                    grid_view_get_cell(gd, px.wrapping_add(i), py, &mut gc);
                    empty = tty_draw_line_get_empty(&gc, &last, nx.wrapping_sub(i))
                        as ::core::ffi::c_int;
                    if empty != 0 as ::core::ffi::c_int {
                        gcp = &gc;
                    } else {
                        converted =
                            tty_check_codeset(client.flags() & CLIENT_UTF8 as u64 != 0, &gc);
                        gcp = &converted;
                        if gcp.flags as ::core::ffi::c_int & GRID_FLAG_SELECTED != 0 {
                            ngc = *gcp;
                            if let Some(selected) = screen_select_cell(s, gcp) {
                                ngc = selected;
                                gcp = &ngc;
                            }
                        }
                    }
                }
                if empty != 0 as ::core::ffi::c_int {
                    next_state = TTY_DRAW_LINE_EMPTY;
                } else if current_state as ::core::ffi::c_uint
                    == TTY_DRAW_LINE_FIRST as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    next_state = TTY_DRAW_LINE_SAME;
                } else if grid_cells_look_equal(gcp, &last) {
                    if gcp.data.size as usize
                        > (::core::mem::size_of::<[::core::ffi::c_char; 1000]>() as usize)
                            .wrapping_sub(len as usize)
                    {
                        next_state = TTY_DRAW_LINE_FLUSH;
                    } else {
                        next_state = TTY_DRAW_LINE_SAME;
                    }
                } else if current_state as ::core::ffi::c_uint
                    == TTY_DRAW_LINE_NEW1 as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    next_state = TTY_DRAW_LINE_NEW2;
                } else {
                    next_state = TTY_DRAW_LINE_NEW1;
                }
            }
            if log_get_level() != 0 as ::core::ffi::c_int {
                log_debug(format_args!(
                    "{}: cell {} empty {}, bg {}; state: current {}, next {}",
                    "tty_draw_line",
                    (px.wrapping_add(i)) as u32,
                    (empty) as u32,
                    (gcp.bg) as u32,
                    TTY_DRAW_LINE_STATES[current_state as usize],
                    TTY_DRAW_LINE_STATES[next_state as usize]
                ));
            }
            if next_state as ::core::ffi::c_uint != current_state as ::core::ffi::c_uint {
                if current_state as ::core::ffi::c_uint
                    == TTY_DRAW_LINE_EMPTY as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    tty_attributes(client, &last, Some(style_ctx));
                    tty_draw_line_clear(
                        client,
                        atx.wrapping_add(last_i),
                        aty,
                        i.wrapping_sub(last_i),
                        defaults,
                        last.bg as u_int,
                        wrapped,
                    );
                    wrapped = 0 as ::core::ffi::c_int;
                } else if next_state as ::core::ffi::c_uint
                    != TTY_DRAW_LINE_SAME as ::core::ffi::c_int as ::core::ffi::c_uint
                    && len != 0 as size_t
                {
                    tty_attributes(client, &last, Some(style_ctx));
                    if atx.wrapping_add(i).wrapping_sub(width) != 0 as u_int || wrapped == 0 {
                        tty_cursor(client, atx.wrapping_add(i).wrapping_sub(width), aty);
                    }
                    if !(last.attr as ::core::ffi::c_int) & GRID_ATTR_CHARSET != 0 {
                        tty_putn(client, &buf[..len], width);
                    } else {
                        j = 0 as u_int;
                        while (j as size_t) < len {
                            tty_putc(client, buf[j as usize] as u_char);
                            j = j.wrapping_add(1);
                        }
                    }
                    len = 0 as size_t;
                    width = 0 as u_int;
                    wrapped = 0 as ::core::ffi::c_int;
                }
                last_i = i;
            }
            if next_state as ::core::ffi::c_uint
                != TTY_DRAW_LINE_EMPTY as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                let data = &gcp.data;
                let size = data.size as usize;
                buf[len..len + size].copy_from_slice(&data.data[..size]);
                len += size;
                width = width.wrapping_add(data.width as u_int);
            }
            if next_state as ::core::ffi::c_uint
                == TTY_DRAW_LINE_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                break;
            }
            current_state = next_state;
            last = *gcp;
            if empty != 0 as ::core::ffi::c_int {
                i = i.wrapping_add(empty as u_int);
            } else {
                i = i.wrapping_add(gcp.data.width as u_int);
            }
        }
    }
    terminal_set!(client, flags, =, terminal_value!(client, flags) & !TTY_NOCURSOR | flags);
    tty_update_mode(client, terminal_value!(client, mode), Some(s.into()));
}
