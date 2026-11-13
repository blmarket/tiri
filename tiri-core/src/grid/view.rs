use crate::grid::{
    grid_clear, grid_collect_history, grid_get_cell, grid_move_cells, grid_move_lines,
    grid_scroll_history, grid_scroll_history_region, grid_set_cell, grid_set_cells,
    grid_set_padding, grid_string_cells_bytes,
};
use crate::shared::abi::*;
use crate::shared::grid::*;
pub fn grid_view_get_cell(gd: &grid, px: u_int, py: u_int, gc: &mut grid_cell) {
    grid_get_cell(gd, px, gd.hsize.wrapping_add(py), gc);
}
pub fn grid_view_set_cell(gd: &mut grid, px: u_int, py: u_int, gc: &grid_cell) {
    grid_set_cell(gd, px, gd.hsize.wrapping_add(py), gc);
}
pub fn grid_view_set_padding(gd: &mut grid, px: u_int, py: u_int, bg: ::core::ffi::c_int) {
    grid_set_padding(gd, px, gd.hsize.wrapping_add(py), bg);
}
pub fn grid_view_set_cells(gd: &mut grid, px: u_int, py: u_int, gc: &grid_cell, bytes: &[u8]) {
    grid_set_cells(gd, px, gd.hsize.wrapping_add(py), gc, bytes);
}
pub fn grid_view_clear_history(gd: &mut grid, bg: u_int) {
    let visible = &gd.linedata[gd.hsize as usize..gd.hsize.wrapping_add(gd.sy) as usize];
    let last = visible
        .iter()
        .rposition(|line| line.cellused != 0)
        .map_or(0, |row| row as u_int + 1);
    if last == 0 {
        grid_view_clear(gd, 0, 0, gd.sx, gd.sy, bg);
        return;
    }
    for _ in 0..last {
        grid_collect_history(gd, 0);
        grid_scroll_history(gd, bg);
    }
    if last < gd.sy {
        grid_view_clear(gd, 0, 0, gd.sx, gd.sy.wrapping_sub(last), bg);
    }
    gd.hscrolled = 0;
}
pub fn grid_view_clear(gd: &mut grid, px: u_int, mut py: u_int, nx: u_int, ny: u_int, bg: u_int) {
    py = gd.hsize.wrapping_add(py);
    grid_clear(gd, px, py, nx, ny, bg);
}
pub fn grid_view_scroll_region_up(gd: &mut grid, mut rupper: u_int, mut rlower: u_int, bg: u_int) {
    if gd.flags & GRID_HISTORY != 0 {
        grid_collect_history(gd, 0);
        if rupper == 0 && rlower == gd.sy.wrapping_sub(1) {
            grid_scroll_history(gd, bg);
        } else {
            rupper = gd.hsize.wrapping_add(rupper);
            rlower = gd.hsize.wrapping_add(rlower);
            grid_scroll_history_region(gd, rupper, rlower, bg);
        }
    } else {
        rupper = gd.hsize.wrapping_add(rupper);
        rlower = gd.hsize.wrapping_add(rlower);
        grid_move_lines(
            gd,
            rupper,
            rupper.wrapping_add(1),
            rlower.wrapping_sub(rupper),
            bg,
        );
    };
}
pub fn grid_view_scroll_region_down(
    gd: &mut grid,
    mut rupper: u_int,
    mut rlower: u_int,
    bg: u_int,
) {
    rupper = gd.hsize.wrapping_add(rupper);
    rlower = gd.hsize.wrapping_add(rlower);
    grid_move_lines(
        gd,
        rupper.wrapping_add(1),
        rupper,
        rlower.wrapping_sub(rupper),
        bg,
    );
}
pub fn grid_view_insert_lines(gd: &mut grid, mut py: u_int, ny: u_int, bg: u_int) {
    py = gd.hsize.wrapping_add(py);
    let sy = gd.hsize.wrapping_add(gd.sy);
    grid_move_lines(
        gd,
        py.wrapping_add(ny),
        py,
        sy.wrapping_sub(py).wrapping_sub(ny),
        bg,
    );
}
pub fn grid_view_insert_lines_region(
    gd: &mut grid,
    mut rlower: u_int,
    mut py: u_int,
    ny: u_int,
    bg: u_int,
) {
    rlower = gd.hsize.wrapping_add(rlower);
    py = gd.hsize.wrapping_add(py);
    let ny2 = rlower.wrapping_add(1).wrapping_sub(py).wrapping_sub(ny);
    grid_move_lines(gd, rlower.wrapping_add(1).wrapping_sub(ny2), py, ny2, bg);
    let width = gd.sx;
    grid_clear(gd, 0, py.wrapping_add(ny2), width, ny.wrapping_sub(ny2), bg);
}
pub fn grid_view_delete_lines(gd: &mut grid, mut py: u_int, ny: u_int, bg: u_int) {
    py = gd.hsize.wrapping_add(py);
    let sy = gd.hsize.wrapping_add(gd.sy);
    grid_move_lines(
        gd,
        py,
        py.wrapping_add(ny),
        sy.wrapping_sub(py).wrapping_sub(ny),
        bg,
    );
    let width = gd.sx;
    grid_clear(gd, 0, sy.wrapping_sub(ny), width, ny, bg);
}
pub fn grid_view_delete_lines_region(
    gd: &mut grid,
    mut rlower: u_int,
    mut py: u_int,
    ny: u_int,
    bg: u_int,
) {
    rlower = gd.hsize.wrapping_add(rlower);
    py = gd.hsize.wrapping_add(py);
    let ny2 = rlower.wrapping_add(1).wrapping_sub(py).wrapping_sub(ny);
    grid_move_lines(gd, py, py.wrapping_add(ny), ny2, bg);
    let width = gd.sx;
    grid_clear(gd, 0, py.wrapping_add(ny2), width, ny.wrapping_sub(ny2), bg);
}
pub fn grid_view_insert_cells(gd: &mut grid, px: u_int, mut py: u_int, nx: u_int, bg: u_int) {
    py = gd.hsize.wrapping_add(py);
    let sx = gd.sx;
    if px >= sx.wrapping_sub(1) {
        grid_clear(gd, px, py, 1, 1, bg);
    } else {
        grid_move_cells(
            gd,
            px.wrapping_add(nx),
            px,
            py,
            sx.wrapping_sub(px).wrapping_sub(nx),
            bg,
        );
    };
}
pub fn grid_view_delete_cells(gd: &mut grid, px: u_int, mut py: u_int, nx: u_int, bg: u_int) {
    py = gd.hsize.wrapping_add(py);
    let sx = gd.sx;
    grid_move_cells(
        gd,
        px,
        px.wrapping_add(nx),
        py,
        sx.wrapping_sub(px).wrapping_sub(nx),
        bg,
    );
    grid_clear(gd, sx.wrapping_sub(nx), py, nx, 1, bg);
}
pub fn grid_view_string_cells_bytes(gd: &grid, py: u_int, nx: u_int) -> Vec<u8> {
    grid_string_cells_bytes(gd, 0, gd.hsize.wrapping_add(py), nx, None, 0, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::core::{grid_create, grid_default_cell};
    use crate::shared::colour::COLOUR_FLAG_RGB;

    fn row_labels(gd: &grid) -> Vec<u8> {
        (0..gd.hsize + gd.sy)
            .map(|row| {
                let mut cell = grid_default_cell;
                grid_get_cell(gd, 0, row, &mut cell);
                cell.data.data[0]
            })
            .collect()
    }

    #[test]
    fn visible_scroll_regions_preserve_history_and_outside_rows() {
        for history in [false, true] {
            let mut gd = grid_create(8, 4, 10);
            let mut cell = grid_default_cell;
            cell.data.data[0] = b'H';
            grid_set_cell(&mut gd, 0, 0, &cell);
            grid_scroll_history(&mut gd, 8);
            for (row, byte) in b"ABCD".iter().enumerate() {
                cell.data.data[0] = *byte;
                grid_view_set_cell(&mut gd, 0, row as u_int, &cell);
            }
            if !history {
                gd.flags &= !GRID_HISTORY;
            }
            grid_view_scroll_region_up(&mut gd, 1, 2, 8);
            assert_eq!(
                row_labels(&gd),
                if history {
                    b"HBAC D".as_slice()
                } else {
                    b"HAC D"
                }
            );
            let hsize = gd.hsize;
            grid_view_scroll_region_down(&mut gd, 1, 2, 8);
            assert_eq!(gd.hsize, hsize);
            assert_eq!(
                row_labels(&gd),
                if history {
                    b"HBA CD".as_slice()
                } else {
                    b"HA CD"
                }
            );
            let bytes = *b"ABCDEFGH";
            grid_view_set_cells(&mut gd, 0, 1, &grid_default_cell, &bytes);
            grid_view_insert_cells(&mut gd, 1, 1, 2, 8);
            grid_view_delete_cells(&mut gd, 1, 1, 2, 8);
            for (column, expected) in b"ABCDEF  ".iter().enumerate() {
                grid_view_get_cell(&gd, column as u_int, 1, &mut cell);
                assert_eq!(cell.data.data[0], *expected);
            }
            assert_eq!(row_labels(&gd)[0], b'H');
        }
    }

    #[test]
    fn clear_into_history_uses_last_used_row_and_preserves_empty_scroll_count() {
        let mut gd = grid_create(8, 4, 10);
        let mut cell = grid_default_cell;
        cell.data.data[0] = b'H';
        grid_set_cell(&mut gd, 0, 0, &cell);
        grid_scroll_history(&mut gd, 8);
        cell.data.data[0] = b'X';
        grid_view_set_cell(&mut gd, 0, 1, &cell);
        grid_view_clear_history(&mut gd, 8);
        assert_eq!(row_labels(&gd), b"H X    ");
        assert_eq!((gd.hsize, gd.hscrolled, gd.scroll_added), (3, 0, 3));
        gd.hscrolled = 2;
        let bg = (COLOUR_FLAG_RGB | 0x123456) as u_int;
        grid_view_clear_history(&mut gd, bg);
        assert_eq!((gd.hsize, gd.hscrolled, gd.scroll_added), (3, 2, 3));
        for row in 0..4 {
            grid_view_get_cell(&gd, 0, row, &mut cell);
            assert_eq!(cell.bg, bg as i32);
            assert_eq!(gd.linedata[gd.hsize as usize + row as usize].cellused, 0);
        }
    }
}
