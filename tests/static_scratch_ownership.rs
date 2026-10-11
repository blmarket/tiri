use hmux::src::{
    grid::{grid_create, grid_default_cell, grid_set_cell, grid_string_cells_bytes},
    input_keys::input_key_get_mouse,
    shared::{
        grid::{grid_cell, GRID_ATTR_CHARSET, GRID_FLAG_TAB, GRID_STRING_WITH_SEQUENCES},
        mouse::mouse_event,
        screen::{screen, MODE_MOUSE_ALL, MODE_MOUSE_BUTTON, MODE_MOUSE_SGR, MODE_MOUSE_UTF8},
    },
    tty::tty_check_codeset,
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
        let mut first = [0; 40];
        let len = input_key_get_mouse(&mut screen, &mouse, 9, 19, &mut first).unwrap();
        let first_bytes = std::slice::from_raw_parts(first.as_ptr().cast::<u8>(), len);
        assert_eq!(first_bytes, b"\x1b[<0;10;20M");

        // A legacy release cannot be converted to SGR, even when SGR is enabled.
        mouse.sgr_type = b' ' as u32;
        mouse.b = 3;
        let mut second = [0; 40];
        let len = input_key_get_mouse(&mut screen, &mouse, 222, 1000, &mut second).unwrap();
        assert_eq!(
            std::slice::from_raw_parts(second.as_ptr().cast::<u8>(), len),
            b"\x1b[M#\xff\xff"
        );
        assert_eq!(first_bytes, b"\x1b[<0;10;20M");

        screen.mode |= MODE_MOUSE_UTF8;
        let len = input_key_get_mouse(&mut screen, &mouse, 95, 2014, &mut second).unwrap();
        assert_eq!(
            std::slice::from_raw_parts(second.as_ptr().cast::<u8>(), len),
            b"\x1b[M#\xc2\x80\xdf\xbf"
        );
        assert!(input_key_get_mouse(&mut screen, &mouse, 2015, 0, &mut second).is_none());

        // Suppressed motion and disabled mouse reporting produce no packet.
        screen.mode = MODE_MOUSE_BUTTON;
        mouse.b = 35;
        mouse.lb = 3;
        assert!(input_key_get_mouse(&mut screen, &mouse, 0, 0, &mut second).is_none());
        screen.mode = MODE_MOUSE_ALL;
        assert!(input_key_get_mouse(&mut screen, &mouse, 0, 0, &mut second).is_some());
        screen.mode = 0;
        assert!(input_key_get_mouse(&mut screen, &mouse, 0, 0, &mut second).is_none());
        assert_eq!(first_bytes, b"\x1b[<0;10;20M");
    }
}

fn character(text: &str, width: u8) -> grid_cell {
    let mut cell = grid_default_cell;
    cell.data.data = [0; 32];
    cell.data.data[..text.len()].copy_from_slice(text.as_bytes());
    cell.data.size = text.len() as u8;
    cell.data.have = text.len() as u8;
    cell.data.width = width;
    cell.fg = 1;
    cell.bg = 4;
    cell.link = 7;
    cell
}

#[test]
fn codeset_conversion_preserves_cells_and_independent_results() {
    unsafe {
        let wide = character("漢", 2);
        let first = tty_check_codeset(false, &wide);
        assert_eq!(&first.data.data[..first.data.size as usize], b"__");
        assert_eq!((first.fg, first.bg, first.link), (1, 4, 7));

        let border = character("─", 1);
        let second = tty_check_codeset(false, &border);
        assert_eq!(&second.data.data[..second.data.size as usize], b"q");
        assert_ne!(second.attr as i32 & GRID_ATTR_CHARSET, 0);
        assert_eq!(&first.data.data[..first.data.size as usize], b"__");
        assert_eq!(&wide.data.data[..wide.data.size as usize], "漢".as_bytes());

        let utf8 = tty_check_codeset(true, &wide);
        assert_eq!(&utf8.data.data[..utf8.data.size as usize], "漢".as_bytes());
        let mut tab = wide;
        tab.flags = GRID_FLAG_TAB as u8;
        let tab = tty_check_codeset(false, &tab);
        assert_eq!(tab.data.size, wide.data.size);
        assert_eq!(tab.data.data, wide.data.data);
        let ascii = tty_check_codeset(false, &character("A", 1));
        assert_eq!(&ascii.data.data[..ascii.data.size as usize], b"A");
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
        // Interleaving a separate capture must not change the first one's state.
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
