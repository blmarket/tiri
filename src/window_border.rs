use crate::format::{format_add, format_create, format_defaults, format_expand_cstring};
use crate::format_draw::format_draw;
use crate::grid::grid_default_cell;
use crate::grid::view::grid_view_get_cell;
use crate::options::options_get_string;
use crate::screen::screen_init;
use crate::screen_write::{screen_write_start, screen_write_stop};
use crate::shared::abi::*;
use crate::shared::borders::{CELL_BORDERS, SIMPLE_BORDERS};
use crate::shared::format::{FORMAT_NOJOBS, FORMAT_WINDOW};
use crate::shared::grid::*;
use crate::shared::layout::*;
use crate::shared::screen::screen;
use crate::shared::style::style_ranges;
use crate::shared::window::WindowRef;
use crate::text::utf8::{utf8_copy, utf8_set};
use crate::tty::{tty_acs_double_borders, tty_acs_heavy_borders, tty_acs_rounded_borders};
use crate::window::Window as _;

/// Render `fill-character` into the cell that fills whatever no pane, scrollbar
/// or separator covers, inside the window's extent or outside it.
pub(crate) unsafe fn window_render_fill_cell(
    w_owner: &WindowRef,
    inside: bool,
) -> Option<grid_cell> {
    let mut s: screen = screen::empty();
    let mut new_gc: grid_cell = grid_cell {
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
    let mut ft_owner = format_create(
        None,
        None,
        (FORMAT_WINDOW | (w_owner).id()) as ::core::ffi::c_int,
        FORMAT_NOJOBS,
    );
    let ft = &mut *ft_owner;
    format_defaults(
        ft,
        None,
        None,
        (refbox::Weak::new()).clone(),
        w_owner.active_pane().upgrade().as_ref(),
    );
    format_add(ft, c"is_inside", |out| write!(out, "{}", (inside) as i32));
    format_add(ft, c"is_outside", |out| {
        write!(out, "{}", { (!inside) as ::core::ffi::c_int })
    });
    let value = options_get_string(&w_owner.borrow_options_mut(), c"fill-character");
    let expanded = format_expand_cstring(ft, value.as_ptr());
    drop(ft_owner);
    screen_init(&mut s, 1 as u_int, 1 as u_int, 0 as u_int);
    let mut ctx = screen_write_start(&mut s);
    format_draw(
        &mut ctx,
        &grid_default_cell,
        1 as u_int,
        expanded.as_ptr(),
        ::core::ptr::null_mut::<style_ranges>(),
        0 as ::core::ffi::c_int,
    );
    screen_write_stop(ctx);
    grid_view_get_cell(s.grid(), 0 as u_int, 0 as u_int, &mut new_gc);
    drop(s);
    (new_gc.data.width == 1).then_some(new_gc)
}

unsafe fn window_copy_fill_cell(gc: &mut grid_cell, fill: &grid_cell) {
    gc.data = utf8_copy(&fill.data);
    gc.attr = (gc.attr as ::core::ffi::c_int | fill.attr as ::core::ffi::c_int) as u_short;
    gc.flags = (gc.flags as ::core::ffi::c_int | fill.flags as ::core::ffi::c_int) as u_char;
    if fill.fg != 8 as ::core::ffi::c_int {
        gc.fg = fill.fg;
    }
    if fill.bg != 8 as ::core::ffi::c_int {
        gc.bg = fill.bg;
    }
    if fill.us != 8 as ::core::ffi::c_int {
        gc.us = fill.us;
    }
}
pub unsafe fn window_get_fill_cell(
    owner: &WindowRef,
    inside: ::core::ffi::c_int,
    gc: &mut grid_cell,
) {
    let fill = owner.fill_cell(inside != 0);
    window_copy_fill_cell(gc, &fill);
}

/// Set `gc` to a separator glyph in the `pane_lines` style. Every separator is
/// vertical; `index` is the pane's number for the number style.
pub fn window_get_border_cell(
    index: Option<u32>,
    mut pane_lines: pane_lines,
    cell_type: ::core::ffi::c_int,
    gc: &mut grid_cell,
) {
    match pane_lines as ::core::ffi::c_uint {
        4 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            if let Some(index) = index {
                utf8_set(
                    &mut gc.data,
                    ('0' as i32 as u_int).wrapping_add(index.wrapping_rem(10 as u_int)) as u_char,
                );
            } else {
                utf8_set(&mut gc.data, '*' as i32 as u_char);
            }
        }
        1 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            gc.data = utf8_copy(tty_acs_double_borders(cell_type));
        }
        2 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            gc.data = utf8_copy(tty_acs_heavy_borders(cell_type));
        }
        7 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            gc.data = utf8_copy(tty_acs_rounded_borders(cell_type));
        }
        3 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            utf8_set(&mut gc.data, SIMPLE_BORDERS[cell_type as usize] as u_char);
        }
        6 | 5 => {
            gc.attr = (gc.attr as ::core::ffi::c_int & !GRID_ATTR_CHARSET) as u_short;
            utf8_set(&mut gc.data, ' ' as i32 as u_char);
        }
        _ => {
            gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
            utf8_set(&mut gc.data, CELL_BORDERS[cell_type as usize] as u_char);
        }
    };
}
