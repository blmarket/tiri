use crate::grid::{grid_get_cell, grid_in_set, grid_line_length, grid_line_limit};
use crate::shared::abi::*;
use crate::shared::grid::grid_reader;
use crate::shared::grid::*;
use std::ffi::CStr;
pub fn grid_reader_start(gd: &grid, cx: u_int, cy: u_int) -> grid_reader<'_> {
    grid_reader { gd, cx, cy }
}
pub fn grid_reader_get_cursor(gr: &grid_reader<'_>) -> (u_int, u_int) {
    (gr.cx, gr.cy)
}
pub fn grid_reader_line_length(gr: &grid_reader<'_>) -> u_int {
    grid_line_length(gr.gd, gr.cy)
}
pub fn grid_reader_cursor_right(
    gr: &mut grid_reader<'_>,
    mut wrap: ::core::ffi::c_int,
    mut all: ::core::ffi::c_int,
    mut onemore: ::core::ffi::c_int,
) {
    let mut px: u_int = 0;
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
    if all != 0 {
        px = gr.gd.sx;
    } else if onemore != 0 {
        px = grid_reader_line_length(gr);
    } else {
        px = grid_line_limit(gr.gd, gr.cy);
    }
    if wrap != 0
        && gr.cx >= px
        && gr.cy < gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int)
    {
        grid_reader_cursor_start_of_line(gr, 0 as ::core::ffi::c_int);
        grid_reader_cursor_down(gr);
    } else if gr.cx < px {
        gr.cx = gr.cx.wrapping_add(1);
        while gr.cx < px {
            grid_get_cell(gr.gd, gr.cx, gr.cy, &mut gc);
            if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
                break;
            }
            gr.cx = gr.cx.wrapping_add(1);
        }
    }
}
pub fn grid_reader_cursor_left(gr: &mut grid_reader<'_>, mut wrap: ::core::ffi::c_int) {
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
    while gr.cx > 0 as u_int {
        grid_get_cell(gr.gd, gr.cx, gr.cy, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        gr.cx = gr.cx.wrapping_sub(1);
    }
    if gr.cx == 0 as u_int
        && gr.cy > 0 as u_int
        && (wrap != 0
            || gr.gd.linedata[(gr.cy.wrapping_sub(1 as u_int)) as usize].flags
                as ::core::ffi::c_int
                & GRID_LINE_WRAPPED
                != 0)
    {
        grid_reader_cursor_up(gr);
        grid_reader_cursor_end_of_line(gr, 0 as ::core::ffi::c_int, 0 as ::core::ffi::c_int);
    } else if gr.cx > 0 as u_int {
        gr.cx = gr.cx.wrapping_sub(1);
    }
}
pub fn grid_reader_cursor_down(gr: &mut grid_reader<'_>) {
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
    if gr.cy < gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int) {
        gr.cy = gr.cy.wrapping_add(1);
    }
    while gr.cx > 0 as u_int {
        grid_get_cell(gr.gd, gr.cx, gr.cy, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        gr.cx = gr.cx.wrapping_sub(1);
    }
}
pub fn grid_reader_cursor_up(gr: &mut grid_reader<'_>) {
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
    if gr.cy > 0 as u_int {
        gr.cy = gr.cy.wrapping_sub(1);
    }
    while gr.cx > 0 as u_int {
        grid_get_cell(gr.gd, gr.cx, gr.cy, &mut gc);
        if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0 {
            break;
        }
        gr.cx = gr.cx.wrapping_sub(1);
    }
}
pub fn grid_reader_cursor_start_of_line(gr: &mut grid_reader<'_>, mut wrap: ::core::ffi::c_int) {
    if wrap != 0 {
        while gr.cy > 0 as u_int
            && gr.gd.linedata[(gr.cy.wrapping_sub(1 as u_int)) as usize].flags as ::core::ffi::c_int
                & GRID_LINE_WRAPPED
                != 0
        {
            gr.cy = gr.cy.wrapping_sub(1);
        }
    }
    gr.cx = 0 as u_int;
}
pub fn grid_reader_cursor_end_of_line(
    gr: &mut grid_reader<'_>,
    mut wrap: ::core::ffi::c_int,
    mut all: ::core::ffi::c_int,
) {
    let mut yy: u_int = 0;
    if wrap != 0 {
        yy = gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int);
        while gr.cy < yy
            && gr.gd.linedata[(gr.cy) as usize].flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0
        {
            gr.cy = gr.cy.wrapping_add(1);
        }
    }
    if all != 0 {
        gr.cx = gr.gd.sx;
    } else {
        gr.cx = grid_reader_line_length(gr);
    };
}
fn grid_reader_handle_wrap(
    gr: &mut grid_reader<'_>,
    xx: &mut u_int,
    yy: &mut u_int,
) -> ::core::ffi::c_int {
    while gr.cx > *xx {
        if gr.cy == *yy {
            return 0 as ::core::ffi::c_int;
        }
        grid_reader_cursor_start_of_line(gr, 0 as ::core::ffi::c_int);
        grid_reader_cursor_down(gr);
        if gr.gd.linedata[(gr.cy) as usize].flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0 {
            *xx = gr.gd.sx.wrapping_sub(1 as u_int);
        } else {
            *xx = grid_reader_line_length(gr);
        }
    }
    1 as ::core::ffi::c_int
}
pub fn grid_reader_in_set(gr: &grid_reader<'_>, set: &CStr) -> ::core::ffi::c_int {
    grid_in_set(gr.gd, gr.cx, gr.cy, set)
}
pub fn grid_reader_cursor_next_word(gr: &mut grid_reader<'_>, separators: &CStr) {
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    let mut width: u_int = 0;
    if gr.gd.linedata[(gr.cy) as usize].flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0 {
        xx = gr.gd.sx.wrapping_sub(1 as u_int);
    } else {
        xx = grid_reader_line_length(gr);
    }
    yy = gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int);
    if grid_reader_handle_wrap(gr, &mut xx, &mut yy) == 0 {
        return;
    }
    if grid_reader_in_set(gr, c"\t ") == 0 {
        if grid_reader_in_set(gr, separators) != 0 {
            loop {
                gr.cx = gr.cx.wrapping_add(1);
                if !(grid_reader_handle_wrap(gr, &mut xx, &mut yy) != 0
                    && grid_reader_in_set(gr, separators) != 0
                    && grid_reader_in_set(gr, c"\t ") == 0)
                {
                    break;
                }
            }
        } else {
            loop {
                gr.cx = gr.cx.wrapping_add(1);
                if !(grid_reader_handle_wrap(gr, &mut xx, &mut yy) != 0
                    && !(grid_reader_in_set(gr, separators) != 0
                        || grid_reader_in_set(gr, c"\t ") != 0))
                {
                    break;
                }
            }
        }
    }
    while grid_reader_handle_wrap(gr, &mut xx, &mut yy) != 0 && {
        width = grid_reader_in_set(gr, c"\t ") as u_int;
        width != 0
    } {
        gr.cx = gr.cx.wrapping_add(width);
    }
}
pub fn grid_reader_cursor_next_word_end(gr: &mut grid_reader<'_>, separators: &CStr) {
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    if gr.gd.linedata[(gr.cy) as usize].flags as ::core::ffi::c_int & GRID_LINE_WRAPPED != 0 {
        xx = gr.gd.sx.wrapping_sub(1 as u_int);
    } else {
        xx = grid_reader_line_length(gr);
    }
    yy = gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int);
    while grid_reader_handle_wrap(gr, &mut xx, &mut yy) != 0 {
        if grid_reader_in_set(gr, c"\t ") != 0 {
            gr.cx = gr.cx.wrapping_add(1);
        } else if grid_reader_in_set(gr, separators) != 0 {
            loop {
                gr.cx = gr.cx.wrapping_add(1);
                if !(grid_reader_handle_wrap(gr, &mut xx, &mut yy) != 0
                    && grid_reader_in_set(gr, separators) != 0
                    && grid_reader_in_set(gr, c"\t ") == 0)
                {
                    break;
                }
            }
            return;
        } else {
            loop {
                gr.cx = gr.cx.wrapping_add(1);
                if !(grid_reader_handle_wrap(gr, &mut xx, &mut yy) != 0
                    && !(grid_reader_in_set(gr, c"\t ") != 0
                        || grid_reader_in_set(gr, separators) != 0))
                {
                    break;
                }
            }
            return;
        }
    }
}
pub fn grid_reader_cursor_previous_word(
    gr: &mut grid_reader<'_>,
    separators: &CStr,
    mut already: ::core::ffi::c_int,
    mut stop_at_eol: ::core::ffi::c_int,
) {
    let mut oldx: ::core::ffi::c_int = 0;
    let mut oldy: ::core::ffi::c_int = 0;
    let mut at_eol: ::core::ffi::c_int = 0;
    let mut word_is_letters: ::core::ffi::c_int = 0;
    if already != 0 || grid_reader_in_set(gr, c"\t ") != 0 {
        loop {
            if gr.cx > 0 as u_int {
                gr.cx = gr.cx.wrapping_sub(1);
                if !(grid_reader_in_set(gr, c"\t ") == 0) {
                    continue;
                }
                word_is_letters = (grid_reader_in_set(gr, separators) == 0) as ::core::ffi::c_int;
                break;
            } else {
                if gr.cy == 0 as u_int {
                    return;
                }
                grid_reader_cursor_up(gr);
                grid_reader_cursor_end_of_line(
                    gr,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                if !(stop_at_eol != 0 && gr.cx > 0 as u_int) {
                    continue;
                }
                oldx = gr.cx as ::core::ffi::c_int;
                gr.cx = gr.cx.wrapping_sub(1);
                at_eol = grid_reader_in_set(gr, c"\t ");
                gr.cx = oldx as u_int;
                if !(at_eol != 0) {
                    continue;
                }
                word_is_letters = 0 as ::core::ffi::c_int;
                break;
            }
        }
    } else {
        word_is_letters = (grid_reader_in_set(gr, separators) == 0) as ::core::ffi::c_int;
    }
    loop {
        oldx = gr.cx as ::core::ffi::c_int;
        oldy = gr.cy as ::core::ffi::c_int;
        if gr.cx == 0 as u_int {
            if gr.cy == 0 as u_int
                || !(gr.gd.linedata[(gr.cy.wrapping_sub(1 as u_int)) as usize].flags
                    as ::core::ffi::c_int)
                    & GRID_LINE_WRAPPED
                    != 0
            {
                break;
            }
            grid_reader_cursor_up(gr);
            grid_reader_cursor_end_of_line(gr, 0 as ::core::ffi::c_int, 1 as ::core::ffi::c_int);
        }
        if gr.cx > 0 as u_int {
            gr.cx = gr.cx.wrapping_sub(1);
        }
        if !(grid_reader_in_set(gr, c"\t ") == 0
            && word_is_letters != grid_reader_in_set(gr, separators))
        {
            break;
        }
    }
    gr.cx = oldx as u_int;
    gr.cy = oldy as u_int;
}
fn grid_reader_cell_equals_data(gc: &grid_cell, ud: &utf8_data) -> bool {
    if gc.flags as i32 & GRID_FLAG_PADDING != 0 {
        return false;
    }
    if gc.flags as i32 & GRID_FLAG_TAB != 0 && ud.size == 1 && ud.data[0] == b'\t' {
        return true;
    }
    gc.data.size == ud.size && gc.data.data[..ud.size as usize] == ud.data[..ud.size as usize]
}
pub fn grid_reader_cursor_jump(gr: &mut grid_reader<'_>, jc: &utf8_data) -> ::core::ffi::c_int {
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
    let mut py: u_int = 0;
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    px = gr.cx;
    yy = gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int);
    py = gr.cy;
    while py <= yy {
        xx = grid_line_length(gr.gd, py);
        while px < xx {
            grid_get_cell(gr.gd, px, py, &mut gc);
            if grid_reader_cell_equals_data(&gc, jc) {
                gr.cx = px;
                gr.cy = py;
                return 1 as ::core::ffi::c_int;
            }
            px = px.wrapping_add(1);
        }
        if py == yy
            || gr.gd.linedata[(py) as usize].flags as ::core::ffi::c_int & GRID_LINE_WRAPPED == 0
        {
            return 0 as ::core::ffi::c_int;
        }
        px = 0 as u_int;
        py = py.wrapping_add(1);
    }
    0 as ::core::ffi::c_int
}
pub fn grid_reader_cursor_jump_back(
    gr: &mut grid_reader<'_>,
    jc: &utf8_data,
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
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut xx: u_int = 0;
    xx = gr.cx.wrapping_add(1 as u_int);
    py = gr.cy.wrapping_add(1 as u_int);
    while py > 0 as u_int {
        px = xx;
        while px > 0 as u_int {
            grid_get_cell(
                gr.gd,
                px.wrapping_sub(1 as u_int),
                py.wrapping_sub(1 as u_int),
                &mut gc,
            );
            if grid_reader_cell_equals_data(&gc, jc) {
                gr.cx = px.wrapping_sub(1 as u_int);
                gr.cy = py.wrapping_sub(1 as u_int);
                return 1 as ::core::ffi::c_int;
            }
            px = px.wrapping_sub(1);
        }
        if py == 1 as u_int
            || gr.gd.linedata[(py.wrapping_sub(2 as u_int)) as usize].flags as ::core::ffi::c_int
                & GRID_LINE_WRAPPED
                == 0
        {
            return 0 as ::core::ffi::c_int;
        }
        xx = grid_line_length(gr.gd, py.wrapping_sub(2 as u_int));
        py = py.wrapping_sub(1);
    }
    0 as ::core::ffi::c_int
}
pub fn grid_reader_cursor_back_to_indentation(gr: &mut grid_reader<'_>) {
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
    let mut py: u_int = 0;
    let mut xx: u_int = 0;
    let mut yy: u_int = 0;
    let mut oldx: u_int = 0;
    let mut oldy: u_int = 0;
    yy = gr.gd.hsize.wrapping_add(gr.gd.sy).wrapping_sub(1 as u_int);
    oldx = gr.cx;
    oldy = gr.cy;
    grid_reader_cursor_start_of_line(gr, 1 as ::core::ffi::c_int);
    py = gr.cy;
    while py <= yy {
        xx = grid_line_length(gr.gd, py);
        px = 0 as u_int;
        while px < xx {
            grid_get_cell(gr.gd, px, py, &mut gc);
            if (gc.data.size as ::core::ffi::c_int != 1 as ::core::ffi::c_int
                || gc.data.data[0] != b' ')
                && !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_TAB != 0
                && !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_PADDING != 0
            {
                gr.cx = px;
                gr.cy = py;
                return;
            }
            px = px.wrapping_add(1);
        }
        if !(gr.gd.linedata[(py) as usize].flags as ::core::ffi::c_int) & GRID_LINE_WRAPPED != 0 {
            break;
        }
        py = py.wrapping_add(1);
    }
    gr.cx = oldx;
    gr.cy = oldy;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{
        grid_create, grid_default_cell, grid_set_cell, grid_set_padding, grid_set_tab,
    };
    use crate::text::utf8::utf8_fromcstr_vec;

    fn sample_grid() -> Box<grid> {
        let mut owner = grid_create(8, 4, 0);
        for (row, text) in [c"ab   中,", c"  cd", c"  tail", c""].iter().enumerate() {
            let mut column = 0;
            for data in utf8_fromcstr_vec(text)
                .into_iter()
                .take_while(|cell| cell.size != 0)
            {
                let mut cell = grid_default_cell;
                cell.data = data;
                grid_set_cell(&mut owner, column, row as u_int, &cell);
                for offset in 1..data.width as u_int {
                    grid_set_padding(&mut owner, column + offset, row as u_int, 8);
                }
                column += data.width as u_int;
            }
        }
        let mut tab = grid_default_cell;
        grid_set_tab(&mut tab, 3);
        grid_set_cell(&mut owner, 2, 0, &tab);
        for column in 3..5 {
            grid_set_padding(&mut owner, column, 0, 8);
        }
        owner.linedata[0].flags |= GRID_LINE_WRAPPED as u_short;
        owner
    }

    #[test]
    fn borrowed_reader_keeps_padding_and_wrapped_line_boundaries() {
        let owner = sample_grid();
        let mut reader = grid_reader_start(&owner, 2, 0);
        grid_reader_cursor_right(&mut reader, 1, 0, 0);
        assert_eq!(grid_reader_get_cursor(&reader), (5, 0));
        grid_reader_cursor_right(&mut reader, 1, 0, 0);
        assert_eq!(grid_reader_get_cursor(&reader), (7, 0));
        grid_reader_cursor_right(&mut reader, 1, 0, 0);
        assert_eq!(grid_reader_get_cursor(&reader), (0, 1));
        grid_reader_cursor_left(&mut reader, 0);
        assert_eq!(grid_reader_get_cursor(&reader), (8, 0));
        grid_reader_cursor_end_of_line(&mut reader, 1, 0);
        assert_eq!(grid_reader_get_cursor(&reader), (4, 1));
        grid_reader_cursor_start_of_line(&mut reader, 1);
        assert_eq!(grid_reader_get_cursor(&reader), (0, 0));

        let mut independent = grid_reader_start(&owner, 6, 2);
        grid_reader_cursor_back_to_indentation(&mut independent);
        assert_eq!(grid_reader_get_cursor(&independent), (2, 2));
        assert_eq!(grid_reader_get_cursor(&reader), (0, 0));
    }

    #[test]
    fn borrowed_jump_targets_match_tabs_and_wide_cells_without_crossing_newlines() {
        let owner = sample_grid();
        let mut reader = grid_reader_start(&owner, 0, 0);
        for (text, expected) in [(c"\t", (2, 0)), (c"中", (5, 0)), (c"c", (2, 1))] {
            let target = utf8_fromcstr_vec(text)[0];
            assert_eq!(grid_reader_cursor_jump(&mut reader, &target), 1);
            assert_eq!(grid_reader_get_cursor(&reader), expected);
        }
        let target = utf8_fromcstr_vec(c"t")[0];
        assert_eq!(grid_reader_cursor_jump(&mut reader, &target), 0);
        assert_eq!(grid_reader_get_cursor(&reader), (2, 1));
        let target = utf8_fromcstr_vec(c"中")[0];
        assert_eq!(grid_reader_cursor_jump_back(&mut reader, &target), 1);
        assert_eq!(grid_reader_get_cursor(&reader), (5, 0));
        let target = utf8_fromcstr_vec(c"!")[0];
        assert_eq!(grid_reader_cursor_jump_back(&mut reader, &target), 0);
        assert_eq!(grid_reader_get_cursor(&reader), (5, 0));
    }
}
