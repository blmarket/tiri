use crate::src::hyperlinks::hyperlinks_get;
use crate::src::log::{fatalx, log_debug};
use crate::src::server::current_time;
use crate::src::shared::abi::*;
pub use crate::src::shared::colour::{COLOUR_FLAG_256, COLOUR_FLAG_RGB, COLOUR_FLAG_THEME};
use crate::src::shared::grid::*;
pub use crate::src::shared::limits::UINT_MAX;
pub use crate::src::shared::screen::screen;
use crate::src::style::colour::{colour_split_rgb, colour_theme_terminal_colour};
use crate::src::text::utf8::{
    utf8_build_one, utf8_cstrhas, utf8_from_data, utf8_has_whitespace, utf8_set, utf8_to_data,
};
use crate::src::tmux::start_time;
use std::ffi::CStr;

pub static grid_default_cell: grid_cell = grid_cell {
    data: utf8_data {
        data: [
            ' ' as i32 as u_char,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
        have: 0 as u_char,
        size: 1 as u_char,
        width: 1 as u_char,
    },
    attr: 0 as u_short,
    flags: 0 as u_char,
    fg: 8 as ::core::ffi::c_int,
    bg: 8 as ::core::ffi::c_int,
    us: 8 as ::core::ffi::c_int,
    link: 0 as u_int,
};
static grid_padding_cell: grid_cell = grid_cell {
    data: utf8_data {
        data: [
            '!' as i32 as u_char,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
        have: 0 as u_char,
        size: 0 as u_char,
        width: 0 as u_char,
    },
    attr: 0 as u_short,
    flags: GRID_FLAG_PADDING as u_char,
    fg: 8 as ::core::ffi::c_int,
    bg: 8 as ::core::ffi::c_int,
    us: 8 as ::core::ffi::c_int,
    link: 0 as u_int,
};
static grid_cleared_cell: grid_cell = grid_cell {
    data: utf8_data {
        data: [
            ' ' as i32 as u_char,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
        have: 0 as u_char,
        size: 1 as u_char,
        width: 1 as u_char,
    },
    attr: 0 as u_short,
    flags: GRID_FLAG_CLEARED as u_char,
    fg: 8 as ::core::ffi::c_int,
    bg: 8 as ::core::ffi::c_int,
    us: 8 as ::core::ffi::c_int,
    link: 0 as u_int,
};
static grid_cleared_entry: grid_cell_entry = grid_cell_entry {
    c2rust_unnamed: grid_cell_entry_storage {
        data: grid_cell_entry_data {
            attr: 0 as u_char,
            fg: 8 as u_char,
            bg: 8 as u_char,
            data: ' ' as i32 as u_char,
        },
    },
    flags: GRID_FLAG_CLEARED as u_char,
};
fn grid_store_cell(entry: &mut grid_cell_entry, cell: &grid_cell, byte: u_char) {
    entry.flags = (cell.flags as i32 & !GRID_FLAG_CLEARED) as u_char;
    if cell.fg & COLOUR_FLAG_256 != 0 {
        entry.flags |= GRID_FLAG_FG256 as u_char;
    }
    if cell.bg & COLOUR_FLAG_256 != 0 {
        entry.flags |= GRID_FLAG_BG256 as u_char;
    }
    entry.c2rust_unnamed.data = grid_cell_entry_data {
        fg: (cell.fg & 0xff) as u_char,
        bg: (cell.bg & 0xff) as u_char,
        attr: cell.attr as u_char,
        data: byte,
    };
}
fn grid_need_extended_cell(entry: &grid_cell_entry, cell: &grid_cell) -> bool {
    entry.flags as i32 & GRID_FLAG_EXTENDED != 0
        || cell.attr > 0xff
        || cell.data.size > 1
        || cell.data.width > 1
        || cell.fg & (COLOUR_FLAG_RGB | COLOUR_FLAG_THEME) != 0
        || cell.bg & (COLOUR_FLAG_RGB | COLOUR_FLAG_THEME) != 0
        || cell.us != 8
        || cell.link != 0
        || cell.flags as i32 & GRID_FLAG_TAB != 0
}
fn grid_get_extended_cell(line: &mut grid_line, column: usize, flags: i32) {
    let count = line.extdsize.wrapping_add(1);
    line.extddata
        .resize_with(count as usize, grid_extd_entry::default);
    line.extdsize = count;
    let entry = &mut line.celldata[column];
    entry.c2rust_unnamed.offset = count.wrapping_sub(1);
    entry.flags = (flags | GRID_FLAG_EXTENDED) as u_char;
}
unsafe fn grid_extended_cell<'a>(
    line: &'a mut grid_line,
    column: usize,
    cell: &grid_cell,
) -> &'a mut grid_extd_entry {
    let flags = cell.flags as i32 & !GRID_FLAG_CLEARED;
    let entry = &line.celldata[column];
    if entry.flags as i32 & GRID_FLAG_EXTENDED == 0 {
        grid_get_extended_cell(line, column, flags);
    } else if entry.c2rust_unnamed.offset >= line.extdsize {
        fatalx(|out| out.write_all(b"offset too big"));
    }
    line.flags |= GRID_LINE_EXTENDED as u_short;
    if cell.link != 0 {
        line.flags |= GRID_LINE_HYPERLINK as u_short;
    }
    let mut packed = 0;
    if cell.flags as i32 & GRID_FLAG_TAB != 0 {
        packed = cell.data.width as utf8_char;
    } else {
        utf8_from_data(&cell.data, &mut packed);
    }
    let offset = line.celldata[column].c2rust_unnamed.offset;
    let extended = &mut line.extddata[offset as usize];
    *extended = grid_extd_entry {
        data: packed,
        attr: cell.attr,
        flags: flags as u_char,
        fg: cell.fg,
        bg: cell.bg,
        us: cell.us,
        link: cell.link,
    };
    extended
}
unsafe fn grid_compact_line(gl: &mut grid_line) {
    if gl.extdsize == 0 {
        return;
    }
    let cells = &mut gl.celldata[..gl.cellsize as usize];
    let count = cells
        .iter()
        .filter(|cell| cell.flags as i32 & GRID_FLAG_EXTENDED != 0)
        .count();
    if count == 0 {
        gl.extddata.clear();
        gl.extdsize = 0;
        return;
    }
    let mut compact = GridArray::default();
    compact.resize_with(count, grid_extd_entry::default);
    let mut index = 0;
    for cell in cells {
        if cell.flags as i32 & GRID_FLAG_EXTENDED != 0 {
            compact[index] = gl.extddata[cell.c2rust_unnamed.offset as usize];
            cell.c2rust_unnamed.offset = index as u_int;
            index += 1;
        }
    }
    gl.extddata = compact;
    gl.extdsize = count as u_int;
}
pub fn grid_get_line(gd: &grid, line: u_int) -> &grid_line {
    &gd.linedata[line as usize]
}
pub fn grid_get_line_mut(gd: &mut grid, line: u_int) -> &mut grid_line {
    &mut gd.linedata[line as usize]
}
pub unsafe fn grid_line_time(gl: &grid_line) -> time_t {
    if gl.time == 0 {
        return 0;
    }
    crate::src::shared::time::unix_seconds(start_time) + gl.time as time_t - 1
}
unsafe fn grid_line_set_time(gl: &mut grid_line) {
    gl.time = if current_time == 0 {
        0
    } else {
        (current_time as __time_t - crate::src::shared::time::unix_seconds(start_time) + 1) as u_int
    };
}
pub fn grid_adjust_lines(gd: &mut grid, lines: u_int) {
    gd.linedata.resize_with(lines as usize, grid_line::default);
}
unsafe fn grid_clear_cell(line: &mut grid_line, px: u_int, bg: u_int, moved: bool) {
    let column = px as usize;
    let entry = &mut line.celldata[column];
    let old_offset = entry.c2rust_unnamed.offset;
    let had_extended = entry.flags as i32 & GRID_FLAG_EXTENDED != 0;
    *entry = grid_cleared_entry;
    if !moved && had_extended && old_offset < line.extdsize {
        entry.flags |= GRID_FLAG_EXTENDED as u_char;
        entry.c2rust_unnamed.offset = old_offset;
        let extended = grid_extended_cell(line, column, &grid_cleared_cell);
        if bg != 8 {
            extended.bg = bg as i32;
        }
    } else if bg != 8 {
        if bg & (COLOUR_FLAG_RGB | COLOUR_FLAG_THEME) as u_int != 0 {
            let flags = entry.flags as i32;
            grid_get_extended_cell(line, column, flags);
            grid_extended_cell(line, column, &grid_cleared_cell).bg = bg as i32;
        } else {
            if bg & COLOUR_FLAG_256 as u_int != 0 {
                entry.flags |= GRID_FLAG_BG256 as u_char;
            }
            entry.c2rust_unnamed.data.bg = bg as u_char;
        }
    }
}
fn grid_check_y(gd: &grid, from: &str, py: u_int) -> i32 {
    if py >= gd.hsize.wrapping_add(gd.sy) {
        log_debug(format_args!("{}: y out of range: {}", from, py));
        return -1;
    }
    0
}
pub fn grid_cells_look_equal(gc1: &grid_cell, gc2: &grid_cell) -> bool {
    gc1.fg == gc2.fg
        && gc1.bg == gc2.bg
        && gc1.attr == gc2.attr
        && (gc1.flags as i32 & !GRID_FLAG_CLEARED) == (gc2.flags as i32 & !GRID_FLAG_CLEARED)
        && gc1.link == gc2.link
}
pub fn grid_cells_equal(gc1: &grid_cell, gc2: &grid_cell) -> bool {
    grid_cells_look_equal(gc1, gc2)
        && gc1.data.width == gc2.data.width
        && gc1.data.size == gc2.data.size
        && gc1.data.data[..gc1.data.size as usize] == gc2.data.data[..gc2.data.size as usize]
}
pub fn grid_set_tab(gc: &mut grid_cell, width: u_int) {
    gc.data.data.fill(0);
    gc.flags |= GRID_FLAG_TAB as u_char;
    gc.flags &= !GRID_FLAG_PADDING as u_char;
    gc.data.have = width as u_char;
    gc.data.size = gc.data.have;
    gc.data.width = gc.data.size;
    gc.data.data[..gc.data.size as usize].fill(b' ');
}
fn grid_free_line(gd: &mut grid, py: u_int) {
    gd.linedata[py as usize] = grid_line::default();
}
pub fn grid_free_lines(gd: &mut grid, py: u_int, ny: u_int) {
    for row in py..py.wrapping_add(ny) {
        grid_free_line(gd, row);
    }
}
pub fn grid_create(sx: u_int, sy: u_int, hlimit: u_int) -> Box<grid> {
    let mut owner = Box::new(grid::default());
    owner.sx = sx;
    owner.sy = sy;
    if hlimit != 0 {
        owner.flags = GRID_HISTORY;
    }
    owner.hlimit = hlimit;
    owner.linedata.resize_with(sy as usize, grid_line::default);
    owner
}
pub unsafe fn grid_compare(ga: &grid, gb: &grid) -> i32 {
    if ga.sx != gb.sx || ga.sy != gb.sy {
        return 1;
    }
    let mut a = grid_cell::default();
    let mut b = grid_cell::default();
    for row in 0..ga.sy {
        let a_line = &ga.linedata[row as usize];
        let b_line = &gb.linedata[row as usize];
        if a_line.cellsize != b_line.cellsize {
            return 1;
        }
        for column in 0..a_line.cellsize as u_int {
            grid_get_cell(ga, column, row, &mut a);
            grid_get_cell(gb, column, row, &mut b);
            if !grid_cells_equal(&a, &b) {
                return 1;
            }
        }
    }
    0
}

fn grid_trim_history(gd: &mut grid, ny: u_int) {
    grid_free_lines(gd, 0, ny);
    let live = gd.hsize.wrapping_add(gd.sy) as usize;
    gd.linedata[..live].rotate_left(ny as usize);
}
pub fn grid_collect_history(gd: &mut grid, all: i32) {
    if gd.hsize == 0 || gd.hsize < gd.hlimit {
        return;
    }
    let count = if all != 0 {
        gd.hsize.wrapping_sub(gd.hlimit)
    } else {
        gd.hlimit / 10
    }
    .max(1)
    .min(gd.hsize);
    grid_trim_history(gd, count);
    gd.hsize = gd.hsize.wrapping_sub(count);
    gd.scroll_collected = gd.scroll_collected.wrapping_add(count);
    gd.hscrolled = gd.hscrolled.min(gd.hsize);
}
pub fn grid_remove_history(gd: &mut grid, ny: u_int) {
    if ny > gd.hsize {
        return;
    }
    let start = gd.hsize.wrapping_add(gd.sy).wrapping_sub(ny);
    for offset in 0..ny {
        grid_free_line(gd, start.wrapping_add(offset));
    }
    gd.hsize = gd.hsize.wrapping_sub(ny);
}
pub unsafe fn grid_scroll_history(gd: &mut grid, bg: u_int) {
    let end = gd.hsize.wrapping_add(gd.sy);
    gd.linedata
        .resize_with(end.wrapping_add(1) as usize, grid_line::default);
    grid_empty_line(gd, end, bg);
    gd.hscrolled = gd.hscrolled.wrapping_add(1);
    let history = &mut gd.linedata[gd.hsize as usize];
    grid_compact_line(history);
    grid_line_set_time(history);
    gd.hsize = gd.hsize.wrapping_add(1);
    gd.scroll_added = gd.scroll_added.wrapping_add(1);
}
pub fn grid_clear_history(gd: &mut grid) {
    grid_trim_history(gd, gd.hsize);
    gd.hscrolled = 0;
    gd.hsize = 0;
    gd.scroll_generation = gd.scroll_generation.wrapping_add(1);
    gd.linedata.resize_with(gd.sy as usize, grid_line::default);
}
pub unsafe fn grid_scroll_history_region(gd: &mut grid, upper: u_int, lower: u_int, bg: u_int) {
    let end = gd.hsize.wrapping_add(gd.sy);
    gd.linedata
        .resize_with(end.wrapping_add(1) as usize, grid_line::default);
    let history = gd.hsize as usize;
    gd.linedata[history..=end as usize].rotate_right(1);
    let upper = upper.wrapping_add(1);
    let lower = lower.wrapping_add(1);
    gd.linedata.swap(history, upper as usize);
    grid_line_set_time(&mut gd.linedata[history]);
    gd.linedata[upper as usize..=lower as usize].rotate_left(1);
    grid_empty_line(gd, lower, bg);
    gd.hscrolled = gd.hscrolled.wrapping_add(1);
    gd.hsize = gd.hsize.wrapping_add(1);
    gd.scroll_added = gd.scroll_added.wrapping_add(1);
}
unsafe fn grid_expand_line(gd: &mut grid, py: u_int, mut width: u_int, bg: u_int) {
    let line = &mut gd.linedata[py as usize];
    if width <= line.cellsize as u_int {
        return;
    }
    if width < gd.sx / 4 {
        width = gd.sx / 4;
    } else if width < gd.sx / 2 {
        width = gd.sx / 2;
    } else if width < gd.sx {
        width = gd.sx;
    }
    line.celldata
        .resize_with(width as usize, grid_cell_entry::default);
    for column in line.cellsize as u_int..width {
        grid_clear_cell(line, column, bg, false);
    }
    line.cellsize = width as u_short;
}
pub unsafe fn grid_empty_line(gd: &mut grid, py: u_int, bg: u_int) {
    gd.linedata[py as usize] = grid_line::default();
    if bg != 8 && bg != 9 {
        grid_expand_line(gd, py, gd.sx, bg);
    }
}
pub unsafe fn grid_peek_line(gd: &grid, py: u_int) -> Option<&grid_line> {
    if grid_check_y(gd, "grid_peek_line", py) != 0 {
        return None;
    }
    Some(&gd.linedata[py as usize])
}
unsafe fn grid_get_cell1(gl: &grid_line, px: u_int, gc: &mut grid_cell) {
    let entry = &gl.celldata[px as usize];
    if entry.flags as i32 & GRID_FLAG_EXTENDED != 0 {
        let offset = entry.c2rust_unnamed.offset;
        if offset >= gl.extdsize {
            *gc = grid_default_cell;
        } else {
            let extended = &gl.extddata[offset as usize];
            gc.flags = extended.flags;
            gc.attr = extended.attr;
            gc.fg = extended.fg;
            gc.bg = extended.bg;
            gc.us = extended.us;
            gc.link = extended.link;
            if gc.flags as i32 & GRID_FLAG_TAB != 0 {
                grid_set_tab(gc, extended.data);
            } else {
                utf8_to_data(extended.data, &mut gc.data);
            }
        }
        return;
    }
    let data = entry.c2rust_unnamed.data;
    gc.flags = (entry.flags as i32 & !(GRID_FLAG_FG256 | GRID_FLAG_BG256)) as u_char;
    gc.attr = data.attr as u_short;
    gc.fg = data.fg as i32;
    if entry.flags as i32 & GRID_FLAG_FG256 != 0 {
        gc.fg |= COLOUR_FLAG_256;
    }
    gc.bg = data.bg as i32;
    if entry.flags as i32 & GRID_FLAG_BG256 != 0 {
        gc.bg |= COLOUR_FLAG_256;
    }
    gc.us = 8;
    utf8_set(&mut gc.data, data.data);
    gc.link = 0;
}
pub unsafe fn grid_get_cell(gd: &grid, px: u_int, py: u_int, gc: &mut grid_cell) {
    if grid_check_y(gd, "grid_get_cell", py) != 0
        || px >= gd.linedata[py as usize].cellsize as u_int
    {
        *gc = grid_default_cell;
    } else {
        grid_get_cell1(&gd.linedata[py as usize], px, gc);
    }
}
pub unsafe fn grid_set_cell(gd: &mut grid, px: u_int, py: u_int, gc: &grid_cell) {
    if grid_check_y(gd, "grid_set_cell", py) != 0 {
        return;
    }
    grid_expand_line(gd, py, px.wrapping_add(1), 8);
    let line = &mut gd.linedata[py as usize];
    if px.wrapping_add(1) > line.cellused as u_int {
        line.cellused = px.wrapping_add(1) as u_short;
    }
    let column = px as usize;
    if grid_need_extended_cell(&line.celldata[column], gc) {
        grid_extended_cell(line, column, gc);
    } else {
        grid_store_cell(&mut line.celldata[column], gc, gc.data.data[0]);
    }
}
pub unsafe fn grid_set_padding(gd: &mut grid, px: u_int, py: u_int, bg: i32) {
    let mut cell = grid_padding_cell;
    cell.bg = bg;
    grid_set_cell(gd, px, py, &cell);
}
pub unsafe fn grid_set_cells(gd: &mut grid, px: u_int, py: u_int, gc: &grid_cell, bytes: &[u8]) {
    if grid_check_y(gd, "grid_set_cells", py) != 0 {
        return;
    }
    let end = (px as usize).wrapping_add(bytes.len());
    grid_expand_line(gd, py, end as u_int, 8);
    let line = &mut gd.linedata[py as usize];
    if end > line.cellused as usize {
        line.cellused = end as u_short;
    }
    for (index, &byte) in bytes.iter().enumerate() {
        let column = px.wrapping_add(index as u_int) as usize;
        if grid_need_extended_cell(&line.celldata[column], gc) {
            grid_extended_cell(line, column, gc).data = utf8_build_one(byte as u_char);
        } else {
            grid_store_cell(&mut line.celldata[column], gc, byte as u_char);
        }
    }
}
pub unsafe fn grid_clear(gd: &mut grid, px: u_int, py: u_int, nx: u_int, ny: u_int, bg: u_int) {
    if nx == 0 || ny == 0 {
        return;
    }
    if px == 0 && nx == gd.sx {
        grid_clear_lines(gd, py, ny, bg);
        return;
    }
    if grid_check_y(gd, "grid_clear", py) != 0
        || grid_check_y(gd, "grid_clear", py.wrapping_add(ny).wrapping_sub(1)) != 0
    {
        return;
    }
    for row in py..py.wrapping_add(ny) {
        let width = gd.sx.min(gd.linedata[row as usize].cellsize as u_int);
        let mut count = nx;
        if bg == 8 || bg == 9 {
            if px > width {
                continue;
            }
            if px.wrapping_add(nx) > width {
                count = width.wrapping_sub(px);
            }
        }
        grid_expand_line(gd, row, px.wrapping_add(count), 8);
        let line = &mut gd.linedata[row as usize];
        for column in px..px.wrapping_add(count) {
            grid_clear_cell(line, column, bg, false);
        }
    }
}
pub unsafe fn grid_clear_lines(gd: &mut grid, py: u_int, ny: u_int, bg: u_int) {
    if ny == 0
        || grid_check_y(gd, "grid_clear_lines", py) != 0
        || grid_check_y(gd, "grid_clear_lines", py.wrapping_add(ny).wrapping_sub(1)) != 0
    {
        return;
    }
    for row in py..py.wrapping_add(ny) {
        grid_empty_line(gd, row, bg);
    }
    if py != 0 {
        gd.linedata[(py - 1) as usize].flags &= !GRID_LINE_WRAPPED as u_short;
    }
}
pub unsafe fn grid_move_lines(gd: &mut grid, dy: u_int, py: u_int, ny: u_int, bg: u_int) {
    if ny == 0 || py == dy {
        return;
    }
    let source_end = py.wrapping_add(ny);
    let destination_end = dy.wrapping_add(ny);
    for row in [
        py,
        source_end.wrapping_sub(1),
        dy,
        destination_end.wrapping_sub(1),
    ] {
        if grid_check_y(gd, "grid_move_lines", row) != 0 {
            return;
        }
    }
    if dy != 0 {
        gd.linedata[(dy - 1) as usize].flags &= !GRID_LINE_WRAPPED as u_short;
    }
    let lines = &mut gd.linedata;
    if dy < py {
        for offset in 0..ny as usize {
            lines[dy as usize + offset] = std::mem::take(&mut lines[py as usize + offset]);
        }
    } else {
        for offset in (0..ny as usize).rev() {
            lines[dy as usize + offset] = std::mem::take(&mut lines[py as usize + offset]);
        }
    }
    for row in py..source_end {
        if row < dy || row >= destination_end {
            grid_empty_line(gd, row, bg);
        }
    }
    if py != 0 && (py < dy || py >= destination_end) {
        gd.linedata[(py - 1) as usize].flags &= !GRID_LINE_WRAPPED as u_short;
    }
}
pub unsafe fn grid_move_cells(
    gd: &mut grid,
    dx: u_int,
    px: u_int,
    py: u_int,
    nx: u_int,
    bg: u_int,
) {
    if nx == 0 || px == dx || grid_check_y(gd, "grid_move_cells", py) != 0 {
        return;
    }
    grid_expand_line(gd, py, px.wrapping_add(nx), 8);
    grid_expand_line(gd, py, dx.wrapping_add(nx), 8);
    let line = &mut gd.linedata[py as usize];
    line.celldata
        .copy_within(px as usize..px as usize + nx as usize, dx as usize);
    if dx.wrapping_add(nx) > line.cellused as u_int {
        line.cellused = dx.wrapping_add(nx) as u_short;
    }
    for column in px..px.wrapping_add(nx) {
        if column < dx || column >= dx.wrapping_add(nx) {
            grid_clear_cell(line, column, bg, true);
        }
    }
}
unsafe fn grid_string_cells_fg(gc: &grid_cell, values: &mut [i32; 64]) -> usize {
    let colour = gc.fg;
    if colour & COLOUR_FLAG_THEME != 0 {
        let terminal = colour_theme_terminal_colour((colour & 0xff) as u_int);
        values[0] = if terminal == 8 { 39 } else { terminal + 30 };
        1
    } else if colour & COLOUR_FLAG_256 != 0 {
        values[..3].copy_from_slice(&[38, 5, colour & 0xff]);
        3
    } else if colour & COLOUR_FLAG_RGB != 0 {
        let (r, g, b) = colour_split_rgb(colour);
        values[..5].copy_from_slice(&[38, 2, r as i32, g as i32, b as i32]);
        5
    } else {
        values[0] = match colour {
            0..=7 => colour + 30,
            8 => 39,
            90..=97 => colour,
            _ => return 0,
        };
        1
    }
}

unsafe fn grid_string_cells_bg(gc: &grid_cell, values: &mut [i32; 64]) -> usize {
    let colour = gc.bg;
    if colour & COLOUR_FLAG_THEME != 0 {
        let terminal = colour_theme_terminal_colour((colour & 0xff) as u_int);
        values[0] = if terminal == 8 { 49 } else { terminal + 40 };
        1
    } else if colour & COLOUR_FLAG_256 != 0 {
        values[..3].copy_from_slice(&[48, 5, colour & 0xff]);
        3
    } else if colour & COLOUR_FLAG_RGB != 0 {
        let (r, g, b) = colour_split_rgb(colour);
        values[..5].copy_from_slice(&[48, 2, r as i32, g as i32, b as i32]);
        5
    } else {
        values[0] = match colour {
            0..=7 => colour + 40,
            8 => 49,
            90..=97 => colour + 10,
            _ => return 0,
        };
        1
    }
}

unsafe fn grid_string_cells_us(gc: &grid_cell, values: &mut [i32; 64]) -> usize {
    let colour = gc.us;
    if colour & COLOUR_FLAG_THEME != 0 {
        let terminal = colour_theme_terminal_colour((colour & 0xff) as u_int);
        if terminal == 8 {
            values[0] = 59;
            1
        } else {
            values[..3].copy_from_slice(&[58, 5, terminal]);
            3
        }
    } else if colour & COLOUR_FLAG_256 != 0 {
        values[..3].copy_from_slice(&[58, 5, colour & 0xff]);
        3
    } else if colour & COLOUR_FLAG_RGB != 0 {
        let (r, g, b) = colour_split_rgb(colour);
        values[..5].copy_from_slice(&[58, 2, r as i32, g as i32, b as i32]);
        5
    } else {
        0
    }
}
/// A bounded, NUL-terminated scratch buffer with tmux's strlcat truncation.
struct GridStringBuffer<'a> {
    storage: &'a mut [u8],
    len: usize,
}
impl<'a> GridStringBuffer<'a> {
    fn new(storage: &'a mut [u8]) -> Self {
        let mut buffer = Self { storage, len: 0 };
        buffer.clear();
        buffer
    }
    fn clear(&mut self) {
        self.len = 0;
        if let Some(first) = self.storage.first_mut() {
            *first = 0;
        }
    }
    fn append(&mut self, bytes: &[u8]) {
        if self.storage.is_empty() {
            return;
        }
        let count = bytes.len().min(self.storage.len() - 1 - self.len);
        self.storage[self.len..self.len + count].copy_from_slice(&bytes[..count]);
        self.len += count;
        self.storage[self.len] = 0;
    }
    fn as_bytes(&self) -> &[u8] {
        &self.storage[..self.len]
    }
}
impl std::fmt::Write for GridStringBuffer<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.append(text.as_bytes());
        Ok(())
    }
}
fn grid_string_cells_add_code(
    buf: &mut GridStringBuffer<'_>,
    s: &[i32],
    newc: &[i32],
    oldc: &[i32],
    flags: i32,
) {
    use std::fmt::Write;
    let reset = s.first() == Some(&0);
    if newc.is_empty() || (!reset && newc == oldc) || (reset && matches!(newc[0], 39 | 49)) {
        return;
    }
    buf.append(if flags & GRID_STRING_ESCAPE_SEQUENCES != 0 {
        b"\\033["
    } else {
        b"\x1b["
    });
    for (index, value) in newc.iter().enumerate() {
        if index != 0 {
            buf.append(b";");
        }
        write!(buf, "{value}").expect("bounded buffer formatting succeeds");
    }
    buf.append(b"m");
}
fn grid_string_cells_add_hyperlink(
    buf: &mut GridStringBuffer<'_>,
    id: &CStr,
    uri: &CStr,
    flags: i32,
) -> bool {
    let id = id.to_bytes();
    let uri = uri.to_bytes();
    if uri.len().wrapping_add(id.len()).wrapping_add(17) >= buf.storage.len() {
        return false;
    }
    let escaped = flags & GRID_STRING_ESCAPE_SEQUENCES != 0;
    buf.append(if escaped { b"\\033]8;" } else { b"\x1b]8;" });
    if !id.is_empty() {
        buf.append(b"id=");
        buf.append(id);
    }
    buf.append(b";");
    buf.append(uri);
    buf.append(if escaped { b"\\033\\\\" } else { b"\x1b\\" });
    true
}
unsafe fn grid_string_cells_code(
    lastgc: &grid_cell,
    gc: &grid_cell,
    buf: &mut GridStringBuffer<'_>,
    flags: i32,
    sc: Option<&screen>,
    has_link: &mut bool,
) {
    use std::fmt::Write;
    let mut oldc = [0; 64];
    let mut newc = [0; 64];
    let mut codes = [0; 128];
    let mut count = 0;
    let attr = gc.attr as i32;
    let mut lastattr = lastgc.attr as i32;
    const ATTRS: &[(i32, i32)] = &[
        (GRID_ATTR_BRIGHT, 1),
        (GRID_ATTR_DIM, 2),
        (GRID_ATTR_ITALICS, 3),
        (GRID_ATTR_UNDERSCORE, 4),
        (GRID_ATTR_BLINK, 5),
        (GRID_ATTR_REVERSE, 7),
        (GRID_ATTR_HIDDEN, 8),
        (GRID_ATTR_STRIKETHROUGH, 9),
        (GRID_ATTR_UNDERSCORE_2, 42),
        (GRID_ATTR_UNDERSCORE_3, 43),
        (GRID_ATTR_UNDERSCORE_4, 44),
        (GRID_ATTR_UNDERSCORE_5, 45),
        (GRID_ATTR_OVERLINE, 53),
    ];
    if ATTRS
        .iter()
        .any(|&(mask, _)| !attr & mask != 0 && lastattr & mask != 0)
        || (lastgc.us != 8 && gc.us == 8)
    {
        codes[count] = 0;
        count += 1;
        lastattr &= GRID_ATTR_CHARSET;
    }
    for &(mask, code) in ATTRS {
        if attr & mask != 0 && lastattr & mask == 0 {
            codes[count] = code;
            count += 1;
        }
    }
    let codes = &codes[..count];
    let escaped = flags & GRID_STRING_ESCAPE_SEQUENCES != 0;
    buf.clear();
    if !codes.is_empty() {
        buf.append(if escaped { b"\\033[" } else { b"\x1b[" });
        for (index, &code) in codes.iter().enumerate() {
            if index != 0 {
                buf.append(b";");
            }
            if code < 10 {
                write!(buf, "{code}").expect("bounded buffer formatting succeeds");
            } else {
                write!(buf, "{}:{}", code / 10, code % 10)
                    .expect("bounded buffer formatting succeeds");
            }
        }
        buf.append(b"m");
    }
    let nnewc = grid_string_cells_fg(gc, &mut newc);
    let noldc = grid_string_cells_fg(lastgc, &mut oldc);
    grid_string_cells_add_code(buf, codes, &newc[..nnewc], &oldc[..noldc], flags);
    let nnewc = grid_string_cells_bg(gc, &mut newc);
    let noldc = grid_string_cells_bg(lastgc, &mut oldc);
    grid_string_cells_add_code(buf, codes, &newc[..nnewc], &oldc[..noldc], flags);
    let nnewc = grid_string_cells_us(gc, &mut newc);
    let noldc = grid_string_cells_us(lastgc, &mut oldc);
    grid_string_cells_add_code(buf, codes, &newc[..nnewc], &oldc[..noldc], flags);
    if attr & GRID_ATTR_CHARSET != 0 && lastattr & GRID_ATTR_CHARSET == 0 {
        buf.append(if escaped { b"\\016" } else { b"\x0e" });
    }
    if attr & GRID_ATTR_CHARSET == 0 && lastattr & GRID_ATTR_CHARSET != 0 {
        buf.append(if escaped { b"\\017" } else { b"\x0f" });
    }
    if lastgc.link != gc.link {
        if let Some(table) = sc.and_then(|screen| screen.hyperlinks.as_ref()) {
            if let Some(link) = hyperlinks_get(table, gc.link) {
                *has_link =
                    grid_string_cells_add_hyperlink(buf, &link.internal_id, &link.uri, flags);
            } else if *has_link {
                grid_string_cells_add_hyperlink(buf, c"", c"", flags);
                *has_link = false;
            }
        }
    }
}
/// The caller initializes `lastgc` to `grid_default_cell` once per capture
/// and retains it across lines so unchanged attributes are not emitted again.
pub unsafe fn grid_string_cells_bytes(
    gd: &grid,
    px: u_int,
    py: u_int,
    nx: u_int,
    mut lastgc: Option<&mut grid_cell>,
    flags: i32,
    screen: Option<&screen>,
) -> Vec<u8> {
    let mut cell = grid_cell::default();
    let mut buf = Vec::with_capacity(128);
    let mut code_storage = [0; 8192];
    let mut code = GridStringBuffer::new(&mut code_storage);
    let mut has_link = false;
    let Some(line) = grid_peek_line(gd, py) else {
        return buf;
    };
    let end = if flags & GRID_STRING_EMPTY_CELLS != 0 {
        line.cellsize
    } else {
        line.cellused
    } as u_int;
    for column in px..px.wrapping_add(nx) {
        if column >= end {
            break;
        }
        grid_get_cell(gd, column, py, &mut cell);
        if cell.flags as i32 & GRID_FLAG_PADDING != 0 {
            continue;
        }
        if let Some(last) = lastgc
            .as_deref_mut()
            .filter(|_| flags & GRID_STRING_WITH_SEQUENCES != 0)
        {
            grid_string_cells_code(last, &cell, &mut code, flags, screen, &mut has_link);
            buf.extend_from_slice(code.as_bytes());
            *last = cell;
        }
        let data: &[u8] = if cell.flags as i32 & GRID_FLAG_TAB != 0 {
            b"\t"
        } else if flags & GRID_STRING_ESCAPE_SEQUENCES != 0
            && cell.data.size == 1
            && cell.data.data[0] == b'\\'
        {
            b"\\\\"
        } else {
            &cell.data.data[..cell.data.size as usize]
        };
        buf.extend_from_slice(data);
    }
    if has_link {
        grid_string_cells_add_hyperlink(&mut code, c"", c"", flags);
        buf.extend_from_slice(code.as_bytes());
    }
    if flags & GRID_STRING_TRIM_SPACES != 0 {
        while buf.last() == Some(&b' ') {
            buf.pop();
        }
    }
    buf
}
pub fn grid_duplicate_lines(dst: &mut grid, dy: u_int, src: &grid, sy: u_int, mut ny: u_int) {
    if dy.wrapping_add(ny) > dst.hsize.wrapping_add(dst.sy) {
        ny = dst.hsize.wrapping_add(dst.sy).wrapping_sub(dy);
    }
    if sy.wrapping_add(ny) > src.hsize.wrapping_add(src.sy) {
        ny = src.hsize.wrapping_add(src.sy).wrapping_sub(sy);
    }
    for offset in 0..ny {
        dst.linedata[dy.wrapping_add(offset) as usize] =
            src.linedata[sy.wrapping_add(offset) as usize].clone();
    }
}
fn grid_reflow_dead(line: &mut grid_line) {
    *line = grid_line::default();
    line.flags = GRID_LINE_DEAD as u_short;
}
fn grid_reflow_add(gd: &mut grid, count: u_int) -> u_int {
    let first = gd.sy;
    gd.sy = gd.sy.wrapping_add(count);
    gd.linedata.resize_with(gd.sy as usize, grid_line::default);
    first
}
fn grid_reflow_move(gd: &mut grid, from: &mut grid_line) -> u_int {
    let to = grid_reflow_add(gd, 1);
    gd.linedata[to as usize] = std::mem::take(from);
    grid_reflow_dead(from);
    to
}
unsafe fn grid_reflow_join(
    target: &mut grid,
    gd: &mut grid,
    sx: u_int,
    yy: u_int,
    mut width: u_int,
    already: bool,
) {
    let to = if already {
        target.sy.wrapping_sub(1)
    } else {
        grid_reflow_move(target, &mut gd.linedata[yy as usize])
    };
    let mut at = target.linedata[to as usize].cellused as u_int;
    let mut lines: u_int = 0;
    let mut from = None;
    let mut want = 0;
    let mut wrapped = true;
    let mut cell = grid_cell::default();
    while yy.wrapping_add(1).wrapping_add(lines) != gd.hsize.wrapping_add(gd.sy) {
        let row = yy.wrapping_add(1).wrapping_add(lines);
        let line = &gd.linedata[row as usize];
        if line.flags as i32 & GRID_LINE_WRAPPED == 0 {
            wrapped = false;
        }
        if line.cellused == 0 {
            if !wrapped {
                break;
            }
            lines = lines.wrapping_add(1);
            continue;
        }
        grid_get_cell1(line, 0, &mut cell);
        if width.wrapping_add(cell.data.width as u_int) > sx {
            break;
        }
        width = width.wrapping_add(cell.data.width as u_int);
        grid_set_cell(target, at, to, &cell);
        at = at.wrapping_add(1);
        from = Some(row);
        want = 1;
        while want < line.cellused as u_int {
            grid_get_cell1(line, want, &mut cell);
            if width.wrapping_add(cell.data.width as u_int) > sx {
                break;
            }
            width = width.wrapping_add(cell.data.width as u_int);
            grid_set_cell(target, at, to, &cell);
            at = at.wrapping_add(1);
            want = want.wrapping_add(1);
        }
        lines = lines.wrapping_add(1);
        if !wrapped || want != line.cellused as u_int || width == sx {
            break;
        }
    }
    let Some(from) = from.filter(|_| lines != 0) else {
        return;
    };
    let left = (gd.linedata[from as usize].cellused as u_int).wrapping_sub(want);
    if left != 0 {
        grid_move_cells(gd, 0, want, yy.wrapping_add(lines), left, 8);
        let from = &mut gd.linedata[from as usize];
        from.cellused = left as u_short;
        from.cellsize = from.cellused;
        from.celldata
            .resize_with(from.cellsize as usize, grid_cell_entry::default);
        lines = lines.wrapping_sub(1);
    } else if !wrapped {
        target.linedata[to as usize].flags &= !GRID_LINE_WRAPPED as u_short;
    }
    for row in yy.wrapping_add(1)..yy.wrapping_add(1).wrapping_add(lines) {
        grid_reflow_dead(&mut gd.linedata[row as usize]);
    }
    if gd.hscrolled > to.wrapping_add(lines) {
        gd.hscrolled = gd.hscrolled.wrapping_sub(lines);
    } else if gd.hscrolled > to {
        gd.hscrolled = to;
    }
}
unsafe fn grid_reflow_split(target: &mut grid, gd: &mut grid, sx: u_int, yy: u_int, at: u_int) {
    let source = &mut gd.linedata[yy as usize];
    let used = source.cellused as u_int;
    let flags = source.flags as i32;
    let mut cell = grid_cell::default();
    let lines = if flags & GRID_LINE_EXTENDED == 0 {
        1_u32.wrapping_add(used.wrapping_sub(1) / sx)
    } else {
        let mut lines: u_int = 2;
        let mut width: u_int = 0;
        for column in at..used {
            grid_get_cell1(source, column, &mut cell);
            if width.wrapping_add(cell.data.width as u_int) > sx {
                lines = lines.wrapping_add(1);
                width = 0;
            }
            width = width.wrapping_add(cell.data.width as u_int);
        }
        lines
    };
    let first = grid_reflow_add(target, lines);
    let mut row = first.wrapping_add(1);
    let mut width: u_int = 0;
    let mut column: u_int = 0;
    for index in at..used {
        grid_get_cell1(source, index, &mut cell);
        if width.wrapping_add(cell.data.width as u_int) > sx {
            target.linedata[row as usize].flags |= GRID_LINE_WRAPPED as u_short;
            row = row.wrapping_add(1);
            width = 0;
            column = 0;
        }
        width = width.wrapping_add(cell.data.width as u_int);
        grid_set_cell(target, column, row, &cell);
        column = column.wrapping_add(1);
    }
    if flags & GRID_LINE_WRAPPED != 0 {
        target.linedata[row as usize].flags |= GRID_LINE_WRAPPED as u_short;
    }
    source.cellused = at as u_short;
    source.cellsize = source.cellused;
    source
        .celldata
        .resize_with(source.cellsize as usize, grid_cell_entry::default);
    source.flags |= GRID_LINE_WRAPPED as u_short;
    target.linedata[first as usize] = std::mem::take(source);
    grid_reflow_dead(source);
    if yy <= gd.hscrolled {
        gd.hscrolled = gd.hscrolled.wrapping_add(lines.wrapping_sub(1));
    }
    if width < sx && flags & GRID_LINE_WRAPPED != 0 {
        grid_reflow_join(target, gd, sx, yy, width, true);
    }
}
pub unsafe fn grid_reflow(gd: &mut grid, sx: u_int) {
    let mut target = grid_create(gd.sx, 0, 0);
    let mut cell = grid_cell::default();
    for row in 0..gd.hsize.wrapping_add(gd.sy) {
        let line = &mut gd.linedata[row as usize];
        if line.flags as i32 & GRID_LINE_DEAD != 0 {
            continue;
        }
        let mut at = 0;
        let width = if line.flags as i32 & GRID_LINE_EXTENDED == 0 {
            let width = line.cellused as u_int;
            at = width.min(sx);
            width
        } else {
            let mut width: u_int = 0;
            for column in 0..line.cellused as u_int {
                grid_get_cell1(line, column, &mut cell);
                if at == 0 && width.wrapping_add(cell.data.width as u_int) > sx {
                    at = column;
                }
                width = width.wrapping_add(cell.data.width as u_int);
            }
            width
        };
        if width == sx {
            grid_reflow_move(&mut target, line);
        } else if width > sx {
            grid_reflow_split(&mut target, gd, sx, row, at);
        } else if line.flags as i32 & GRID_LINE_WRAPPED != 0 {
            grid_reflow_join(&mut target, gd, sx, row, width, false);
        } else {
            grid_reflow_move(&mut target, line);
        }
    }
    if target.sy < gd.sy {
        let extra = gd.sy.wrapping_sub(target.sy);
        grid_reflow_add(&mut target, extra);
    }
    gd.hsize = target.sy.wrapping_sub(gd.sy);
    gd.hscrolled = gd.hscrolled.min(gd.hsize);
    gd.linedata = std::mem::take(&mut target.linedata);
    gd.scroll_generation = gd.scroll_generation.wrapping_add(1);
}
pub fn grid_wrap_position(gd: &grid, px: u_int, py: u_int) -> (u_int, u_int) {
    let mut ax: u_int = 0;
    let mut ay: u_int = 0;
    for line in &gd.linedata[..py as usize] {
        if line.flags as i32 & GRID_LINE_WRAPPED != 0 {
            ax = ax.wrapping_add(line.cellused as u_int);
        } else {
            ax = 0;
            ay = ay.wrapping_add(1);
        }
    }
    if px >= gd.linedata[py as usize].cellused as u_int {
        ax = UINT_MAX;
    } else {
        ax = ax.wrapping_add(px);
    }
    (ax, ay)
}
pub fn grid_unwrap_position(gd: &grid, mut wx: u_int, wy: u_int) -> (u_int, u_int) {
    let mut row: u_int = 0;
    let mut ay: u_int = 0;
    let end = gd.hsize.wrapping_add(gd.sy).wrapping_sub(1);
    while row < end {
        if ay == wy {
            break;
        }
        if gd.linedata[row as usize].flags as i32 & GRID_LINE_WRAPPED == 0 {
            ay = ay.wrapping_add(1);
        }
        row = row.wrapping_add(1);
    }
    if wx == UINT_MAX {
        while row < end && gd.linedata[row as usize].flags as i32 & GRID_LINE_WRAPPED != 0 {
            row = row.wrapping_add(1);
        }
        wx = gd.linedata[row as usize].cellused as u_int;
    } else {
        while gd.linedata[row as usize].flags as i32 & GRID_LINE_WRAPPED != 0 {
            let used = gd.linedata[row as usize].cellused as u_int;
            if wx < used {
                break;
            }
            wx = wx.wrapping_sub(used);
            row = row.wrapping_add(1);
        }
    }
    (wx, row)
}
pub unsafe fn grid_line_length(gd: &grid, mut py: u_int) -> u_int {
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
    let mut px: u_int = 0;
    px = gd.linedata[py as usize].cellsize as u_int;
    if px > gd.sx {
        px = gd.sx;
    }
    while px > 0 as u_int {
        grid_get_cell(gd, px.wrapping_sub(1 as u_int), py, &mut gc);
        if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0
            || gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int
            || gc.data.data[0] != b' '
        {
            break;
        }
        px = px.wrapping_sub(1);
    }
    px
}
pub unsafe fn grid_line_limit(gd: &grid, mut py: u_int) -> u_int {
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
    let mut px: u_int = 0;
    px = grid_line_length(gd, py);
    if px == 0 as u_int {
        return 0 as u_int;
    }
    px = px.wrapping_sub(1);
    while px > 0 as u_int {
        grid_get_cell(gd, px, py, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        px = px.wrapping_sub(1);
    }
    px
}
pub unsafe fn grid_in_set(
    gd: &grid,
    mut px: u_int,
    mut py: u_int,
    set: &CStr,
) -> ::core::ffi::c_int {
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
    let mut pxx: u_int = 0;
    let mut has_tab: ::core::ffi::c_int = 0;
    let mut has_space: ::core::ffi::c_int = 0;
    has_tab = set.to_bytes().contains(&b'\t') as ::core::ffi::c_int;
    has_space = set.to_bytes().contains(&b' ') as ::core::ffi::c_int;
    grid_get_cell(gd, px, py, &mut gc);
    if gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0 {
        if has_tab == 0 && has_space == 0 {
            return 0 as ::core::ffi::c_int;
        }
        pxx = px;
        loop {
            pxx = pxx.wrapping_sub(1);
            grid_get_cell(gd, pxx, py, &mut tmp_gc);
            if !(pxx > 0 as u_int && tmp_gc.flags as ::core::ffi::c_int & GRID_FLAG_PADDING != 0) {
                break;
            }
        }
        if (has_tab != 0 || has_space != 0)
            && tmp_gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0
            || has_space != 0 && utf8_has_whitespace(&tmp_gc.data) != 0
        {
            return (tmp_gc.data.width as u_int).wrapping_sub(px.wrapping_sub(pxx))
                as ::core::ffi::c_int;
        }
        return 0 as ::core::ffi::c_int;
    }
    if (has_tab != 0 || has_space != 0) && gc.flags as ::core::ffi::c_int & GRID_FLAG_TAB != 0 {
        return gc.data.width as ::core::ffi::c_int;
    }
    if has_space != 0 && utf8_has_whitespace(&gc.data) != 0 {
        return if gc.data.width as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            1 as ::core::ffi::c_int
        } else {
            gc.data.width as ::core::ffi::c_int
        };
    }
    utf8_cstrhas(set, &gc.data) as ::core::ffi::c_int
}
struct GridFlagNames {
    flags: i32,
    names: &'static [(i32, &'static str)],
}
impl std::fmt::Display for GridFlagNames {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut separator = "";
        for &(mask, name) in self.names {
            if self.flags & mask != 0 {
                out.write_str(separator)?;
                out.write_str(name)?;
                separator = ",";
            }
        }
        if separator.is_empty() {
            out.write_str("NONE")?;
        }
        Ok(())
    }
}
pub fn grid_line_flags_display(flags: i32) -> impl std::fmt::Display {
    GridFlagNames {
        flags,
        names: &[
            (GRID_LINE_WRAPPED, "WRAPPED"),
            (GRID_LINE_EXTENDED, "EXTENDED"),
            (GRID_LINE_DEAD, "DEAD"),
            (GRID_LINE_START_PROMPT, "START_PROMPT"),
            (GRID_LINE_SECOND_PROMPT, "SECOND_PROMPT"),
            (GRID_LINE_START_COMMAND, "START_COMMAND"),
            (GRID_LINE_START_OUTPUT, "START_OUTPUT"),
            (GRID_LINE_END_OUTPUT, "END_OUTPUT"),
            (GRID_LINE_HYPERLINK, "HYPERLINK"),
        ],
    }
}
pub fn grid_cell_flags_display(flags: i32) -> impl std::fmt::Display {
    GridFlagNames {
        flags,
        names: &[
            (GRID_FLAG_FG256, "FG256"),
            (GRID_FLAG_BG256, "BG256"),
            (GRID_FLAG_PADDING, "PADDING"),
            (GRID_FLAG_EXTENDED, "EXTENDED"),
            (GRID_FLAG_SELECTED, "SELECTED"),
            (GRID_FLAG_CLEARED, "CLEARED"),
            (GRID_FLAG_TAB, "TAB"),
            (GRID_FLAG_NOPALETTE, "NOPALETTE"),
        ],
    }
}
pub fn grid_cell_attr_display(attr: i32) -> impl std::fmt::Display {
    GridFlagNames {
        flags: attr,
        names: &[
            (GRID_ATTR_CHARSET, "CHARSET"),
            (GRID_ATTR_BRIGHT, "BRIGHT"),
            (GRID_ATTR_DIM, "DIM"),
            (GRID_ATTR_UNDERSCORE, "UNDERSCORE"),
            (GRID_ATTR_BLINK, "BLINK"),
            (GRID_ATTR_REVERSE, "REVERSE"),
            (GRID_ATTR_HIDDEN, "HIDDEN"),
            (GRID_ATTR_ITALICS, "ITALICS"),
            (GRID_ATTR_STRIKETHROUGH, "STRIKETHROUGH"),
            (GRID_ATTR_UNDERSCORE_2, "UNDERSCORE_2"),
            (GRID_ATTR_UNDERSCORE_3, "UNDERSCORE_3"),
            (GRID_ATTR_UNDERSCORE_4, "UNDERSCORE_4"),
            (GRID_ATTR_UNDERSCORE_5, "UNDERSCORE_5"),
            (GRID_ATTR_OVERLINE, "OVERLINE"),
        ],
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;

    #[test]
    fn sequence_buffers_preserve_resets_charset_and_truncated_prefixes() {
        unsafe {
            let mut previous = grid_default_cell;
            previous.attr = (GRID_ATTR_BRIGHT | GRID_ATTR_CHARSET) as u_short;
            previous.fg = COLOUR_FLAG_RGB | 0xabcdef;
            previous.us = COLOUR_FLAG_RGB | 0x010203;
            let mut cell = grid_default_cell;
            cell.attr = (GRID_ATTR_UNDERSCORE_2 | GRID_ATTR_OVERLINE) as u_short;
            cell.fg = previous.fg;
            for (flags, expected) in [
                (0, b"\x1b[0;4:2;5:3m\x1b[38;2;171;205;239m\x0f".as_slice()),
                (
                    GRID_STRING_ESCAPE_SEQUENCES,
                    b"\\033[0;4:2;5:3m\\033[38;2;171;205;239m\\017".as_slice(),
                ),
            ] {
                let mut storage = [0; 128];
                let mut out = GridStringBuffer::new(&mut storage);
                let mut has_link = false;
                grid_string_cells_code(&previous, &cell, &mut out, flags, None, &mut has_link);
                assert_eq!(out.as_bytes(), expected);
                grid_string_cells_code(&cell, &cell, &mut out, flags, None, &mut has_link);
                assert!(out.as_bytes().is_empty());
            }
            let mut storage = [99; 5];
            let mut out = GridStringBuffer::new(&mut storage);
            grid_string_cells_code(&previous, &cell, &mut out, 0, None, &mut false);
            assert_eq!(out.as_bytes(), b"\x1b[0;");
            assert_eq!(storage[4], 0);
        }
    }

    #[test]
    fn hyperlink_sequences_keep_capacity_guard_and_existing_prefix() {
        let mut storage = [0; 32];
        let mut out = GridStringBuffer::new(&mut storage);
        out.append(b"prefix");
        assert!(!grid_string_cells_add_hyperlink(
            &mut out,
            c"abcdefghijklmn",
            c"x",
            0
        ));
        assert_eq!(out.as_bytes(), b"prefix");
        assert!(grid_string_cells_add_hyperlink(
            &mut out,
            c"a",
            c"https://x/",
            GRID_STRING_ESCAPE_SEQUENCES
        ));
        assert_eq!(out.as_bytes(), b"prefix\\033]8;id=a;https://x/\\03");
        assert_eq!(storage[31], 0);
    }

    #[test]
    fn captures_preserve_last_code_when_closing_hyperlinks() {
        unsafe {
            let table = crate::src::hyperlinks::HyperlinksRef::new();
            let link = crate::src::hyperlinks::hyperlinks_put(&table, c"uri", Some(c"id"));
            let mut screen = screen::empty();
            screen.hyperlinks = Some(table.clone());
            let mut gd = grid_create(2, 1, 0);
            let mut cell = grid_default_cell;
            cell.link = link;
            utf8_set(&mut cell.data, b'A');
            grid_set_cell(&mut gd, 0, 0, &cell);
            let mut previous = grid_default_cell;
            assert_eq!(
                grid_string_cells_bytes(
                    &gd,
                    0,
                    0,
                    1,
                    Some(&mut previous),
                    GRID_STRING_WITH_SEQUENCES,
                    Some(&screen)
                ),
                b"\x1b]8;id=id;uri\x1b\\A\x1b]8;id=id;uri\x1b\\\x1b]8;;\x1b\\"
            );
            utf8_set(&mut cell.data, b'B');
            grid_set_cell(&mut gd, 1, 0, &cell);
            previous = grid_default_cell;
            assert_eq!(
                grid_string_cells_bytes(
                    &gd,
                    0,
                    0,
                    2,
                    Some(&mut previous),
                    GRID_STRING_WITH_SEQUENCES,
                    Some(&screen)
                ),
                b"\x1b]8;id=id;uri\x1b\\AB\x1b]8;;\x1b\\"
            );
            assert_eq!(previous.link, link);
        }
    }

    #[test]
    fn colour_parameters_keep_theme_priority_and_default_underline_semantics() {
        unsafe {
            for (colour, fg, bg, us) in [
                (8, &[39][..], &[49][..], &[][..]),
                (9, &[][..], &[][..], &[][..]),
                (91, &[91][..], &[101][..], &[][..]),
                (
                    COLOUR_FLAG_256 | 17,
                    &[38, 5, 17][..],
                    &[48, 5, 17][..],
                    &[58, 5, 17][..],
                ),
                (
                    COLOUR_FLAG_RGB | 0x123456,
                    &[38, 2, 18, 52, 86][..],
                    &[48, 2, 18, 52, 86][..],
                    &[58, 2, 18, 52, 86][..],
                ),
                (
                    COLOUR_FLAG_THEME | COLOUR_FLAG_RGB | COLOUR_FLAG_256 | 4,
                    &[32][..],
                    &[42][..],
                    &[58, 5, 2][..],
                ),
                (COLOUR_FLAG_THEME | 255, &[39][..], &[49][..], &[59][..]),
            ] {
                let cell = grid_cell {
                    fg: colour,
                    bg: colour,
                    us: colour,
                    ..grid_default_cell
                };
                let mut values = [-1; 64];
                let count = grid_string_cells_fg(&cell, &mut values);
                assert_eq!(&values[..count], fg);
                let count = grid_string_cells_bg(&cell, &mut values);
                assert_eq!(&values[..count], bg);
                let count = grid_string_cells_us(&cell, &mut values);
                assert_eq!(&values[..count], us);
            }
        }
    }

    #[test]
    fn flag_descriptions_keep_order_unknown_bits_and_independent_values() {
        let first = grid_line_flags_display(GRID_LINE_WRAPPED | GRID_LINE_START_PROMPT);
        let empty = grid_line_flags_display(1 << 30);
        let all = grid_line_flags_display(-1);
        assert_eq!(
            format!("{first}|{empty}|{first}"),
            "WRAPPED,START_PROMPT|NONE|WRAPPED,START_PROMPT"
        );
        assert_eq!(all.to_string(), "WRAPPED,EXTENDED,DEAD,START_PROMPT,SECOND_PROMPT,START_COMMAND,START_OUTPUT,END_OUTPUT,HYPERLINK");
        let cells = grid_cell_flags_display(GRID_FLAG_PADDING | GRID_FLAG_TAB);
        let all_cells = grid_cell_flags_display(-1);
        assert_eq!(
            all_cells.to_string(),
            "FG256,BG256,PADDING,EXTENDED,SELECTED,CLEARED,TAB,NOPALETTE"
        );
        assert_eq!(cells.to_string(), "PADDING,TAB");
        assert_eq!(grid_cell_flags_display(0).to_string(), "NONE");
        assert_eq!(grid_cell_attr_display(GRID_ATTR_NOATTR).to_string(), "NONE");
        assert_eq!(grid_cell_attr_display(-1).to_string(), "CHARSET,BRIGHT,DIM,UNDERSCORE,BLINK,REVERSE,HIDDEN,ITALICS,STRIKETHROUGH,UNDERSCORE_2,UNDERSCORE_3,UNDERSCORE_4,UNDERSCORE_5,OVERLINE");
    }

    #[test]
    fn borrowed_bulk_writes_keep_embedded_nuls_bytes_and_empty_extent() {
        unsafe {
            let mut owner = grid_create(16, 3, 0);
            let bytes = [0, b'A', 0xff];
            let mut cell = grid_default_cell;
            for row in 0..2 {
                if row == 1 {
                    cell.fg = COLOUR_FLAG_RGB | 0x123456;
                    cell.link = 7;
                }
                grid_set_cells(&mut owner, 1, row, &cell, &bytes);
                assert_eq!(owner.linedata[row as usize].cellsize, 8);
                assert_eq!(owner.linedata[row as usize].cellused, 4);
                for (index, byte) in bytes.iter().enumerate() {
                    let mut actual = grid_default_cell;
                    grid_get_cell(&owner, 1 + index as u_int, row, &mut actual);
                    assert_eq!(
                        (actual.data.size, actual.data.width, actual.data.data[0]),
                        (1, 1, (*byte))
                    );
                    assert_eq!((actual.fg, actual.link), (cell.fg, cell.link));
                }
            }
            grid_set_cells(&mut owner, 3, 2, &cell, &[]);
            assert_eq!(
                (owner.linedata[2].cellsize, owner.linedata[2].cellused),
                (4, 3)
            );
            assert_eq!(owner.linedata[2].extdsize, 0);
            grid_set_cells(&mut owner, 0, 3, &cell, &bytes);
            assert_eq!(owner.linedata.len(), 3);
        }
    }

    #[test]
    fn overlapping_cell_moves_keep_extended_colours_and_clear_source_only() {
        unsafe {
            let bg = (COLOUR_FLAG_RGB | 0xabcdef) as u_int;
            for (destination, source, expected) in [(0, 1, b"BCDE "), (1, 0, b" ABCD")] {
                let mut owner = grid_create(8, 1, 0);
                for (column, byte) in b"ABCDE".iter().enumerate() {
                    let mut cell = grid_default_cell;
                    cell.fg = COLOUR_FLAG_RGB | *byte as i32;
                    utf8_set(&mut cell.data, *byte);
                    grid_set_cell(&mut owner, column as u_int, 0, &cell);
                }
                grid_move_cells(&mut owner, destination, source, 0, 4, bg);
                for (column, byte) in expected.iter().enumerate() {
                    let mut actual = grid_default_cell;
                    grid_get_cell(&owner, column as u_int, 0, &mut actual);
                    assert_eq!(actual.data.data[0], *byte);
                    if *byte == b' ' {
                        assert_eq!(actual.bg, bg as i32);
                    } else {
                        assert_eq!(actual.fg, COLOUR_FLAG_RGB | *byte as i32);
                    }
                }
            }
            let mut owner = grid_create(16, 1, 0);
            grid_clear(&mut owner, 5, 0, 3, 1, 8);
            assert_eq!(owner.linedata[0].cellsize, 0);
            grid_clear(&mut owner, 5, 0, 3, 1, bg);
            assert_eq!(
                (owner.linedata[0].cellsize, owner.linedata[0].cellused),
                (16, 0)
            );
            let mut actual = grid_default_cell;
            grid_get_cell(&owner, 7, 0, &mut actual);
            assert_eq!(actual.bg, bg as i32);
            grid_get_cell(&owner, 8, 0, &mut actual);
            assert_eq!(actual.bg, 8);
        }
    }

    #[test]
    fn clearing_extended_cells_preserves_moved_entries_and_reuses_live_slots() {
        unsafe {
            let mut line = grid_line {
                cellsize: 2,
                ..Default::default()
            };
            line.celldata.resize_with(2, grid_cell_entry::default);
            let mut cell = grid_default_cell;
            cell.fg = COLOUR_FLAG_RGB | 0xabcdef;
            cell.link = 7;
            utf8_set(&mut cell.data, b'X');
            grid_extended_cell(&mut line, 0, &cell);
            line.celldata[1] = line.celldata[0];
            grid_clear_cell(&mut line, 0, 8, true);
            let mut actual = grid_default_cell;
            grid_get_cell1(&line, 1, &mut actual);
            assert_same_cell(&actual, &cell);
            assert_eq!(line.extdsize, 1);
            assert!(!grid_need_extended_cell(
                &line.celldata[0],
                &grid_default_cell
            ));

            grid_clear_cell(&mut line, 1, 9, false);
            grid_get_cell1(&line, 1, &mut actual);
            assert_eq!(
                (actual.data.data[0], actual.fg, actual.bg, actual.link),
                (b' ', 8, 9, 0)
            );
            assert_eq!(line.extdsize, 1);
            assert_ne!(line.flags as i32 & GRID_LINE_HYPERLINK, 0);

            let bg = (COLOUR_FLAG_RGB | 0x123456) as u_int;
            grid_clear_cell(&mut line, 0, bg, false);
            assert_eq!(line.extdsize, 2);
            grid_clear_cell(&mut line, 0, bg, false);
            assert_eq!(line.extdsize, 2);
            line.celldata[0].c2rust_unnamed.offset = u_int::MAX;
            grid_clear_cell(&mut line, 0, bg, false);
            assert_eq!(line.extdsize, 3);
            grid_get_cell1(&line, 0, &mut actual);
            assert_eq!((actual.data.data[0], actual.bg), (b' ', bg as i32));
        }
    }

    #[test]
    fn line_compaction_remaps_live_offsets_and_releases_unused_entries() {
        unsafe {
            let mut line = grid_line {
                cellsize: 5,
                extdsize: 6,
                flags: GRID_LINE_EXTENDED as u_short,
                ..Default::default()
            };
            line.celldata.resize_with(6, grid_cell_entry::default);
            line.extddata.resize_with(6, grid_extd_entry::default);
            for (index, extended) in line.extddata.iter_mut().enumerate() {
                extended.fg = 10 + index as i32;
            }
            for (column, offset) in [(0, 4), (2, 1), (3, 4), (5, 0)] {
                line.celldata[column].flags = GRID_FLAG_EXTENDED as u_char;
                line.celldata[column].c2rust_unnamed.offset = offset;
            }
            grid_compact_line(&mut line);
            assert_eq!(line.extdsize, 3);
            assert_eq!(
                line.extddata
                    .iter()
                    .map(|entry| entry.fg)
                    .collect::<Vec<_>>(),
                [14, 11, 14]
            );
            for (column, offset) in [(0, 0), (2, 1), (3, 2)] {
                let actual = line.celldata[column].c2rust_unnamed.offset;
                assert_eq!(actual, offset);
                line.celldata[column].flags = 0;
            }
            grid_compact_line(&mut line);
            assert_eq!(line.extdsize, 0);
            assert!(line.extddata.is_empty());
            assert_eq!(line.flags, GRID_LINE_EXTENDED as u_short);
        }
    }

    #[test]
    fn borrowed_history_operations_keep_thresholds_counts_and_spare_rows() {
        unsafe {
            let mut owner = labeled_grid();
            for _ in 0..4 {
                grid_scroll_history(&mut owner, 8);
            }
            assert_eq!(labels(&mut owner), b"ABCD    ");
            assert_eq!(
                (owner.hsize, owner.hscrolled, owner.scroll_added),
                (4, 4, 4)
            );
            owner.hlimit = 4;
            grid_collect_history(&mut owner, 1);
            assert_eq!(labels(&mut owner), b"BCD    ");
            assert_eq!(
                (owner.hsize, owner.hscrolled, owner.scroll_collected),
                (3, 3, 1)
            );
            owner.hlimit = 1;
            grid_collect_history(&mut owner, 1);
            assert_eq!(labels(&mut owner), b"D    ");
            assert_eq!(
                (owner.hsize, owner.hscrolled, owner.scroll_collected),
                (1, 1, 3)
            );
            grid_remove_history(&mut owner, 2);
            assert_eq!(owner.hsize, 1);
            grid_remove_history(&mut owner, 1);
            assert_eq!(labels(&mut owner), b"D   ");
            assert_eq!(owner.hscrolled, 1);
            assert_eq!(owner.linedata.len(), 8);
            grid_clear_history(&mut owner);
            assert_eq!(owner.linedata.len(), 4);
            assert_eq!(
                (owner.hsize, owner.hscrolled, owner.scroll_generation),
                (0, 0, 1)
            );
            assert_eq!((owner.scroll_added, owner.scroll_collected), (4, 3));
        }
    }

    #[test]
    fn borrowed_line_lookup_covers_history_and_rejects_spare_storage() {
        unsafe {
            let mut owner = grid_create(8, 2, 10);
            grid_scroll_history(&mut owner, 8);
            grid_adjust_lines(&mut owner, 4);
            owner.linedata[0].time = 1;
            owner.linedata[1].time = 9;
            owner.linedata[3].time = 77;
            let history = grid_peek_line(&owner, 0).unwrap();
            assert_eq!(history.time, 1);
            assert_eq!(
                grid_line_time(history),
                crate::src::shared::time::unix_seconds(start_time)
            );
            let visible = grid_peek_line(&owner, 1).unwrap();
            assert_eq!(
                grid_line_time(visible),
                crate::src::shared::time::unix_seconds(start_time) + 8
            );
            assert_eq!(grid_line_time(grid_peek_line(&owner, 2).unwrap()), 0);
            assert!(grid_peek_line(&owner, 3).is_none());
            assert!(grid_peek_line(&owner, u_int::MAX).is_none());
        }
    }

    #[test]
    fn cell_comparison_preserves_ignored_fields_and_exact_glyph_bytes() {
        let original = grid_default_cell;
        let mut changed = original;
        changed.flags |= GRID_FLAG_CLEARED as u_char;
        changed.us = 99;
        changed.data.have = 31;
        changed.data.data[31] = 0xff;
        assert!(grid_cells_equal(&original, &changed));
        changed.data.width = 2;
        assert!(grid_cells_look_equal(&original, &changed));
        assert!(!grid_cells_equal(&original, &changed));
        changed.data.width = original.data.width;
        changed.data.data[0] = b'x';
        assert!(grid_cells_look_equal(&original, &changed));
        assert!(!grid_cells_equal(&original, &changed));
        for mutate in [
            |cell: &mut grid_cell| cell.fg += 1,
            |cell: &mut grid_cell| cell.bg += 1,
            |cell: &mut grid_cell| cell.attr ^= 1,
            |cell: &mut grid_cell| cell.flags ^= GRID_FLAG_PADDING as u_char,
            |cell: &mut grid_cell| cell.link += 1,
        ] {
            let mut changed = original;
            mutate(&mut changed);
            assert!(!grid_cells_look_equal(&original, &changed));
            assert!(!grid_cells_equal(&original, &changed));
        }
    }

    #[test]
    fn grid_comparison_keeps_allocated_line_sizes_and_physical_row_origin() {
        unsafe {
            let mut a = grid_create(2, 1, 10);
            let mut b = grid_create(2, 1, 10);
            assert_eq!(grid_compare(&a, &b), 0);
            grid_set_cell(&mut a, 0, 0, &grid_default_cell);
            assert_eq!(grid_compare(&a, &b), 1);
            grid_set_cell(&mut b, 0, 0, &grid_default_cell);
            assert_eq!(grid_compare(&a, &b), 0);

            grid_scroll_history(&mut a, 8);
            grid_scroll_history(&mut b, 8);
            let mut cell = grid_default_cell;
            cell.data.data[0] = b'X';
            grid_set_cell(&mut a, 0, 1, &cell);
            assert_eq!(grid_compare(&a, &b), 0);
            grid_set_cell(&mut a, 0, 0, &cell);
            assert_eq!(grid_compare(&a, &b), 1);
            grid_set_cell(&mut b, 0, 0, &cell);
            a.linedata[0].time = 17;
            a.linedata[0].flags ^= GRID_LINE_WRAPPED as u_short;
            assert_eq!(grid_compare(&a, &b), 0);
            b.sx = 3;
            assert_eq!(grid_compare(&a, &b), 1);
        }
    }

    fn assert_same_cell(actual: &grid_cell, expected: &grid_cell) {
        assert_eq!(actual.data.data, expected.data.data);
        assert_eq!(
            (actual.data.have, actual.data.size, actual.data.width),
            (expected.data.have, expected.data.size, expected.data.width),
        );
        assert_eq!(
            (
                actual.flags,
                actual.attr,
                actual.fg,
                actual.bg,
                actual.us,
                actual.link
            ),
            (
                expected.flags,
                expected.attr,
                expected.fg,
                expected.bg,
                expected.us,
                expected.link
            ),
        );
    }

    #[test]
    fn borrowed_cell_reads_cover_defaults_compact_extended_and_tab_storage() {
        unsafe {
            let mut owner = grid_create(16, 1, 0);
            let default = grid_default_cell;
            let mut actual = grid_cell::default();
            for (x, y) in [(0, 0), (u_int::MAX, 0), (0, u_int::MAX)] {
                grid_get_cell(&owner, x, y, &mut actual);
                assert_same_cell(&actual, &default);
            }

            let mut compact = default;
            utf8_set(&mut compact.data, b'A');
            compact.fg = COLOUR_FLAG_256 | 99;
            compact.bg = COLOUR_FLAG_256 | 17;
            compact.attr = 3;
            grid_set_cell(&mut owner, 0, 0, &compact);
            grid_get_cell(&owner, 0, 0, &mut actual);
            assert_same_cell(&actual, &compact);

            let mut extended = default;
            extended.data = crate::src::text::utf8::utf8_fromcstr_vec(c"🦀")[0];
            extended.fg = COLOUR_FLAG_RGB | 0xabcdef;
            extended.us = COLOUR_FLAG_RGB | 0x123456;
            extended.link = 7;
            grid_set_cell(&mut owner, 1, 0, &extended);
            grid_get_cell(&owner, 1, 0, &mut actual);
            assert_same_cell(&actual, &extended);

            let mut tab = default;
            grid_set_tab(&mut tab, 3);
            grid_set_cell(&mut owner, 3, 0, &tab);
            grid_get_cell(&owner, 3, 0, &mut actual);
            assert_same_cell(&actual, &tab);

            owner.linedata[0].celldata[1].c2rust_unnamed.offset = u_int::MAX;
            grid_get_cell(&owner, 1, 0, &mut actual);
            assert_same_cell(&actual, &default);
        }
    }

    #[test]
    fn borrowed_line_queries_keep_wide_space_and_tab_padding_widths() {
        unsafe {
            let mut owner = grid_create(8, 1, 0);
            let mut cell = grid_default_cell;
            grid_set_tab(&mut cell, 3);
            grid_set_cell(&mut owner, 2, 0, &cell);
            grid_set_padding(&mut owner, 3, 0, 8);
            grid_set_padding(&mut owner, 4, 0, 8);
            assert_eq!(grid_line_length(&owner, 0), 5);
            assert_eq!(grid_line_limit(&owner, 0), 2);
            for (x, expected) in [(2, 3), (3, 2), (4, 1)] {
                assert_eq!(grid_in_set(&owner, x, 0, c"\t"), expected);
                assert_eq!(grid_in_set(&owner, x, 0, c" "), expected);
                assert_eq!(grid_in_set(&owner, x, 0, c""), 0);
            }

            cell = grid_default_cell;
            cell.data = crate::src::text::utf8::utf8_fromcstr_vec(c"　")[0];
            grid_set_cell(&mut owner, 5, 0, &cell);
            grid_set_padding(&mut owner, 6, 0, 8);
            assert_eq!(grid_line_length(&owner, 0), 7);
            assert_eq!(grid_line_limit(&owner, 0), 5);
            assert_eq!(grid_in_set(&owner, 5, 0, c" "), 2);
            assert_eq!(grid_in_set(&owner, 6, 0, c" "), 1);
            assert_eq!(grid_in_set(&owner, 6, 0, c"\t"), 0);
            assert_eq!(grid_in_set(&owner, 5, 0, c"　"), 1);
        }
    }

    unsafe fn labeled_grid() -> Box<grid> {
        let mut owner = grid_create(8, 4, 1);
        for (y, byte) in b"ABCD".iter().enumerate() {
            let mut cell = grid_default_cell;
            cell.data.data[0] = *byte;
            cell.fg = COLOUR_FLAG_RGB | (*byte as i32);
            grid_set_cell(&mut owner, 0, y as u_int, &cell);
        }
        owner
    }
    unsafe fn labels(owner: &mut grid) -> Vec<u8> {
        (0..owner.hsize + owner.sy)
            .map(|y| {
                let mut cell = grid_default_cell;
                grid_get_cell(&*owner, 0, y, &mut cell);
                cell.data.data[0]
            })
            .collect()
    }

    #[test]
    fn borrowed_line_moves_preserve_overlap_flags_and_fill_backgrounds() {
        unsafe {
            for bg in [8, (COLOUR_FLAG_RGB | 0x123456) as u_int] {
                for (destination, source, count, expected, wrapped) in [
                    (0, 1, 3, b"BCD ", [true, true, true, false]),
                    (1, 0, 3, b" ABC", [false, false, true, true]),
                    (2, 0, 1, b" BAD", [false, false, true, true]),
                    (0, 2, 1, b"CB D", [true, false, false, true]),
                    (4, 0, 1, b"ABCD", [true; 4]),
                    (0, 4, 1, b"ABCD", [true; 4]),
                    (0, 3, 2, b"ABCD", [true; 4]),
                    (0, 3, 0, b"ABCD", [true; 4]),
                ] {
                    let mut owner = labeled_grid();
                    for line in owner.linedata.iter_mut() {
                        line.flags |= GRID_LINE_WRAPPED as u_short;
                    }
                    grid_move_lines(&mut owner, destination, source, count, bg);
                    assert_eq!(labels(&mut owner), expected);
                    for (row, &is_wrapped) in wrapped.iter().enumerate() {
                        assert_eq!(
                            owner.linedata[row].flags as i32 & GRID_LINE_WRAPPED != 0,
                            is_wrapped,
                        );
                        if expected[row] == b' ' {
                            let mut cell = grid_default_cell;
                            grid_get_cell(&owner, 0, row as u_int, &mut cell);
                            assert_eq!(cell.bg, bg as i32);
                        }
                    }
                }
            }
            let mut owner = labeled_grid();
            for line in owner.linedata.iter_mut() {
                line.flags |= GRID_LINE_WRAPPED as u_short;
            }
            grid_clear_lines(&mut owner, 1, 2, 8);
            assert_eq!(labels(&mut owner), b"A  D");
            assert!(owner.linedata[..3]
                .iter()
                .all(|line| line.flags as i32 & GRID_LINE_WRAPPED == 0));
            assert_ne!(owner.linedata[3].flags as i32 & GRID_LINE_WRAPPED, 0);
        }
    }

    #[test]
    fn wrapped_positions_keep_line_ends_empty_rows_and_history_coordinates() {
        let mut gd = grid_create(8, 5, 10);
        gd.sy = 3;
        gd.hsize = 2;
        for (row, (used, wrapped)) in [(3, true), (2, false), (0, false), (4, true), (1, false)]
            .into_iter()
            .enumerate()
        {
            gd.linedata[row].cellused = used;
            if wrapped {
                gd.linedata[row].flags |= GRID_LINE_WRAPPED as u_short;
            }
        }
        for (position, wrapped) in [
            ((2, 0), (2, 0)),
            ((0, 1), (3, 0)),
            ((1, 1), (4, 0)),
            ((2, 3), (2, 2)),
            ((0, 4), (4, 2)),
        ] {
            assert_eq!(grid_wrap_position(&gd, position.0, position.1), wrapped);
            assert_eq!(grid_unwrap_position(&gd, wrapped.0, wrapped.1), position);
        }
        for (position, wrapped, end) in [
            ((3, 0), (UINT_MAX, 0), (2, 1)),
            ((99, 1), (UINT_MAX, 0), (2, 1)),
            ((0, 2), (UINT_MAX, 1), (0, 2)),
            ((4, 3), (UINT_MAX, 2), (1, 4)),
        ] {
            assert_eq!(grid_wrap_position(&gd, position.0, position.1), wrapped);
            assert_eq!(grid_unwrap_position(&gd, wrapped.0, wrapped.1), end);
        }
        assert_eq!(grid_unwrap_position(&gd, 99, 99), (99, 4));
        gd.linedata[2].flags |= GRID_LINE_WRAPPED as u_short;
        assert_eq!(grid_unwrap_position(&gd, 2, 1), (2, 3));
    }

    #[test]
    fn reflow_splits_and_joins_wrapped_rows_with_metadata_and_scroll_position() {
        unsafe {
            for extended in [false, true] {
                let mut gd = grid_create(5, 4, 20);
                gd.sy = 3;
                gd.hsize = 1;
                gd.hscrolled = 1;
                let mut cell = grid_default_cell;
                if extended {
                    cell.fg = COLOUR_FLAG_RGB | 0x123456;
                }
                for (row, text) in [b"ABCDE".as_slice(), b"FGHIJ", b"KL", b"Z"]
                    .iter()
                    .enumerate()
                {
                    for (column, byte) in text.iter().enumerate() {
                        utf8_set(&mut cell.data, *byte);
                        grid_set_cell(&mut gd, column as u_int, row as u_int, &cell);
                    }
                    gd.linedata[row].time = 77 + row as u_int;
                    if row < 2 {
                        gd.linedata[row].flags |= GRID_LINE_WRAPPED as u_short;
                    }
                }
                gd.linedata[0].flags |= GRID_LINE_START_PROMPT as u_short;
                gd.linedata[0].osc133_data.prompt_col = 2;
                grid_reflow(&mut gd, 3);
                assert_eq!(
                    (gd.sx, gd.sy, gd.hsize, gd.hscrolled, gd.scroll_generation),
                    (5, 3, 2, 2, 1)
                );
                for (row, text) in [b"ABC".as_slice(), b"DEF", b"GHI", b"JKL", b"Z"]
                    .iter()
                    .enumerate()
                {
                    assert_eq!(gd.linedata[row].cellused as usize, text.len());
                    assert_eq!(
                        gd.linedata[row].flags as i32 & GRID_LINE_WRAPPED != 0,
                        row < 3
                    );
                    for (column, byte) in text.iter().enumerate() {
                        let mut actual = grid_default_cell;
                        grid_get_cell(&gd, column as u_int, row as u_int, &mut actual);
                        assert_eq!((actual.data.data[0], actual.fg), (*byte, cell.fg));
                    }
                }
                gd.sx = 3;
                grid_reflow(&mut gd, 8);
                assert_eq!((gd.hsize, gd.hscrolled, gd.scroll_generation), (0, 0, 2));
                assert_eq!(gd.linedata[0].cellused, 8);
                assert_eq!(gd.linedata[1].cellused, 4);
                assert_eq!(gd.linedata[0].time, 77);
                assert_eq!(gd.linedata[0].osc133_data.prompt_col, 2);
                assert_ne!(gd.linedata[0].flags as i32 & GRID_LINE_START_PROMPT, 0);
                for (column, byte) in b"IJKL".iter().enumerate() {
                    grid_get_cell(&gd, column as u_int, 1, &mut cell);
                    assert_eq!(cell.data.data[0], *byte);
                }
            }
        }
    }

    #[test]
    fn reflow_keeps_wide_cells_padding_and_links_when_joining_a_split_tail() {
        unsafe {
            let mut gd = grid_create(3, 2, 10);
            let mut cell = grid_default_cell;
            cell.data = crate::src::text::utf8::utf8_fromcstr_vec(c"漢")[0];
            cell.link = 7;
            grid_set_cell(&mut gd, 0, 0, &cell);
            grid_set_padding(&mut gd, 1, 0, COLOUR_FLAG_RGB | 0x123456);
            let mut ascii = grid_default_cell;
            utf8_set(&mut ascii.data, b'X');
            grid_set_cell(&mut gd, 2, 0, &ascii);
            utf8_set(&mut ascii.data, b'Y');
            grid_set_cell(&mut gd, 0, 1, &ascii);
            gd.linedata[0].flags |= GRID_LINE_WRAPPED as u_short;
            grid_reflow(&mut gd, 2);
            assert_eq!((gd.linedata[0].cellused, gd.linedata[1].cellused), (2, 2));
            assert_eq!(gd.linedata[1].flags as i32 & GRID_LINE_WRAPPED, 0);
            let mut actual = grid_default_cell;
            grid_get_cell(&gd, 0, 0, &mut actual);
            assert_same_cell(&actual, &cell);
            grid_get_cell(&gd, 1, 0, &mut actual);
            assert_eq!(actual.data.width, 0);
            assert_ne!(actual.flags as i32 & GRID_FLAG_PADDING, 0);
            grid_get_cell(&gd, 0, 1, &mut actual);
            assert_eq!(actual.data.data[0], b'X');
            grid_get_cell(&gd, 1, 1, &mut actual);
            assert_eq!(actual.data.data[0], b'Y');
            gd.sx = 2;
            grid_reflow(&mut gd, 4);
            assert_eq!((gd.linedata[0].cellused, gd.linedata[1].cellused), (4, 0));
            grid_get_cell(&gd, 0, 0, &mut actual);
            assert_same_cell(&actual, &cell);
            grid_get_cell(&gd, 3, 0, &mut actual);
            assert_eq!(actual.data.data[0], b'Y');
        }
    }

    #[test]
    fn line_duplication_clips_both_grids_and_preserves_independent_metadata() {
        unsafe {
            let mut src = labeled_grid();
            let mut dst = labeled_grid();
            src.linedata[1].time = 77;
            src.linedata[1].flags |=
                GRID_LINE_WRAPPED as u_short | GRID_LINE_START_PROMPT as u_short;
            src.linedata[1].osc133_data.prompt_col = 3;
            grid_duplicate_lines(&mut dst, 3, &src, 1, 3);
            assert_eq!(labels(&mut dst), b"ABCB");
            assert_eq!(dst.linedata[3].time, 77);
            assert_eq!(dst.linedata[3].flags, src.linedata[1].flags);
            assert_eq!(dst.linedata[3].osc133_data.prompt_col, 3);
            src.linedata[1].extddata[0].fg = 8;
            let mut cell = grid_default_cell;
            grid_get_cell(&dst, 0, 3, &mut cell);
            assert_eq!(cell.fg, COLOUR_FLAG_RGB | b'B' as i32);
            grid_duplicate_lines(&mut dst, 0, &src, 3, 3);
            assert_eq!(labels(&mut dst), b"DBCB");
            grid_duplicate_lines(&mut dst, 4, &src, 4, 0);
            assert_eq!(labels(&mut dst), b"DBCB");
            drop(src);
            grid_get_cell(&dst, 0, 3, &mut cell);
            assert_eq!(cell.data.data[0], b'B');
        }
    }

    #[test]
    fn line_moves_history_rotation_and_deep_clones_keep_independent_cells() {
        unsafe {
            let mut owner = labeled_grid();
            let mut copy = grid_create(8, 4, 0);
            grid_duplicate_lines(&mut copy, 0, &owner, 0, 4);
            assert_ne!(
                owner.linedata[0].celldata.as_ptr(),
                copy.linedata[0].celldata.as_ptr()
            );
            assert_ne!(
                owner.linedata[0].extddata.as_ptr(),
                copy.linedata[0].extddata.as_ptr()
            );
            grid_move_lines(&mut owner, 0, 1, 3, 8);
            assert_eq!(labels(&mut owner), b"BCD ");
            grid_move_lines(&mut owner, 1, 0, 3, 8);
            assert_eq!(labels(&mut owner), b" BCD");
            assert_eq!(labels(&mut copy), b"ABCD");
            drop(owner);
            assert_eq!(labels(&mut copy), b"ABCD");

            let mut owner = labeled_grid();
            grid_scroll_history_region(&mut owner, 1, 2, 8);
            assert_eq!(labels(&mut owner), b"BAC D");
            grid_collect_history(&mut owner, 0);
            assert_eq!(labels(&mut owner), b"AC D");
            let mut cell = grid_default_cell;
            cell.fg = COLOUR_FLAG_RGB | 0x123456;
            cell.data.data[0] = b'X';
            for _ in 0..8 {
                grid_set_cell(&mut owner, 0, 0, &cell);
            }
            grid_scroll_history(&mut owner, 8);
            assert_eq!(labels(&mut owner), b"XC D ");
            assert_eq!(
                owner.linedata[0].extddata.len(),
                owner.linedata[0].extdsize as usize
            );
            grid_clear_history(&mut owner);
            assert_eq!(labels(&mut owner), b"C D ");
            assert_eq!(owner.linedata.len(), 4);
        }
    }
}
