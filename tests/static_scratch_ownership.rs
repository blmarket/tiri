use tiri::{
    grid::{grid_create, grid_default_cell, grid_set_cell, grid_string_cells_bytes},
    input_keys::input_key_get_mouse,
    shared::{
        grid::GRID_STRING_WITH_SEQUENCES,
        mouse::mouse_event,
        screen::{
            screen, ScreenMode, MODE_MOUSE_ALL, MODE_MOUSE_BUTTON, MODE_MOUSE_SGR, MODE_MOUSE_UTF8,
        },
    },
};

#[test]
fn mouse_encodings_preserve_tmux_bytes_and_independent_results() {
    unsafe {
        let mut screen = screen::default();
        screen.mode = MODE_MOUSE_BUTTON | MODE_MOUSE_SGR;
        let mut mouse = mouse_event {
            sgr_type: b'M' as u32,
            ..Default::default()
        };
        let first_bytes = input_key_get_mouse(ScreenMode::from(&screen), &mouse, 9, 19).unwrap();
        assert_eq!(first_bytes, b"\x1b[<0;10;20M");

        mouse.sgr_type = b' ' as u32;
        mouse.b = 3;
        assert_eq!(
            input_key_get_mouse(ScreenMode::from(&screen), &mouse, 222, 1000).unwrap(),
            b"\x1b[M#\xff\xff"
        );
        assert_eq!(first_bytes, b"\x1b[<0;10;20M");

        screen.mode |= MODE_MOUSE_UTF8;
        assert_eq!(
            input_key_get_mouse(ScreenMode::from(&screen), &mouse, 95, 2014).unwrap(),
            b"\x1b[M#\xc2\x80\xdf\xbf"
        );
        assert!(input_key_get_mouse(ScreenMode::from(&screen), &mouse, 2015, 0).is_none());

        screen.mode = MODE_MOUSE_BUTTON;
        mouse.b = 35;
        mouse.lb = 3;
        assert!(input_key_get_mouse(ScreenMode::from(&screen), &mouse, 0, 0).is_none());
        screen.mode = MODE_MOUSE_ALL;
        assert!(input_key_get_mouse(ScreenMode::from(&screen), &mouse, 0, 0).is_some());
        screen.mode = 0;
        assert!(input_key_get_mouse(ScreenMode::from(&screen), &mouse, 0, 0).is_none());
        assert_eq!(first_bytes, b"\x1b[<0;10;20M");
    }
}

#[test]
fn capture_attributes_continue_across_lines_but_not_between_captures() {
    unsafe {
        let mut grid = grid_create(1, 3, 0);
        let mut cell = grid_default_cell;
        cell.fg = 1;
        cell.data.data[0] = b'A';
        grid_set_cell(&mut grid, 0, 0, &cell);
        cell.data.data[0] = b'B';
        grid_set_cell(&mut grid, 0, 1, &cell);
        cell.fg = 4;
        cell.data.data[0] = b'C';
        grid_set_cell(&mut grid, 0, 2, &cell);
        let flags = GRID_STRING_WITH_SEQUENCES;
        let mut first = grid_default_cell;
        let mut second = grid_default_cell;

        assert_eq!(
            grid_string_cells_bytes(&grid, 0, 0, 1, Some(&mut first), flags, None),
            b"\x1b[31mA"
        );
        assert_eq!(
            grid_string_cells_bytes(&grid, 0, 2, 1, Some(&mut second), flags, None),
            b"\x1b[34mC"
        );
        assert_eq!(
            grid_string_cells_bytes(&grid, 0, 1, 1, Some(&mut first), flags, None),
            b"B"
        );
        let mut fresh = grid_default_cell;
        assert_eq!(
            grid_string_cells_bytes(&grid, 0, 1, 1, Some(&mut fresh), flags, None),
            b"\x1b[31mB"
        );
        assert_eq!(
            grid_string_cells_bytes(&grid, 0, 0, 1, None, flags, None),
            b"A"
        );
        drop(grid);
    }
}
