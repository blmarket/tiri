use super::*;
use crate::src::grid::{grid_create, grid_default_cell, grid_get_line_mut, grid_set_cells};

#[test]
fn screen_tail_keeps_history_and_joins_only_soft_wrapped_lines() {
    unsafe {
        let mut grid = grid_create(8, 6, 100);
        let cell = grid_default_cell;
        grid_set_cells(&mut grid, 0, 0, &cell, b"history");
        grid_set_cells(&mut grid, 0, 1, &cell, b"visible");
        grid_set_cells(&mut grid, 0, 2, &cell, b"abcdefgh");
        grid_set_cells(&mut grid, 0, 3, &cell, b"ij");
        grid_get_line_mut(&mut grid, 2).flags |= GRID_LINE_WRAPPED as u16;
        grid.hsize = 1;
        grid.sy = 5;
        assert_eq!(
            screen_text(&grid, ScreenSource::Visible, 10),
            c"visible\nabcdefgh\nij"
        );
        assert_eq!(
            screen_text(&grid, ScreenSource::Recent, 10),
            c"history\nvisible\nabcdefgh\nij"
        );
        assert_eq!(
            screen_text(&grid, ScreenSource::RecentUnwrapped, 1),
            c"abcdefghij"
        );
        assert_eq!(screen_text(&grid, ScreenSource::Recent, 1), c"ij");
        assert_eq!(screen_text(&grid, ScreenSource::Recent, 0), c"");
    }
}

#[test]
fn cursor_observation_preserves_terminal_shape_and_blink() {
    let mut screen = screen::default();
    assert_eq!(cursor_shape(&screen), 0);
    for (shape, steady) in [
        (SCREEN_CURSOR_BLOCK, 2),
        (SCREEN_CURSOR_UNDERLINE, 4),
        (SCREEN_CURSOR_BAR, 6),
    ] {
        screen.cstyle = shape;
        screen.mode = 0;
        assert_eq!(cursor_shape(&screen), steady);
        screen.mode = MODE_CURSOR_BLINKING;
        assert_eq!(cursor_shape(&screen), steady - 1);
    }
}
