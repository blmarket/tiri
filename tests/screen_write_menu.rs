use std::ffi::CStr;
use tiri::src::grid::view::grid_view_get_cell;
use tiri::src::grid::{grid_cells_equal, grid_create, grid_default_cell};
use tiri::src::options::options_default;
use tiri::src::options::options_table;
use tiri::src::screen_write::{screen_write_menu, screen_write_start, screen_write_stop};
use tiri::src::shared::grid::{grid_cell, GRID_ATTR_DIM};
use tiri::src::shared::menu::{menu, MenuRow};
use tiri::src::shared::screen::{screen, MODE_WRAP};
use tiri::src::shared::screen_write::screen_write_ctx;
use tiri::src::tmux::global_options;

unsafe fn cell_at(s: &screen, x: u32, y: u32) -> grid_cell {
    let mut cell = grid_default_cell;
    grid_view_get_cell(s.grid(), x, y, &mut cell);
    cell
}

#[test]
fn menu_rendering_preserves_selection_disabled_rows_and_borrowed_inputs() {
    unsafe {
        let table = &options_table;
        let definition = table
            .iter()
            .find(|entry| entry.name == Some(c"extended-keys"))
            .unwrap();
        options_default(global_options(), definition);
        let menu = menu {
            title: c"Title".to_owned(),
            width: 6,
            items: [Some(c"First"), None, Some(c"-Off"), Some(c"After")]
                .into_iter()
                .map(|name| MenuRow {
                    name: name.map(CStr::to_owned),
                    ..Default::default()
                })
                .collect(),
        };
        for (choice, base_dim) in [(0, false), (2, false), (0, true), (2, true)] {
            let mut s = screen::empty();
            s.grid = Some(grid_create(20, 10, 0));
            s.mode = MODE_WRAP;
            s.rlower = 9;
            s.cx = 1;
            s.cy = 1;
            let mut normal = grid_default_cell;
            normal.fg = 1;
            normal.bg = 2;
            if base_dim {
                normal.attr |= GRID_ATTR_DIM as u16;
            }
            let mut selected = grid_default_cell;
            selected.fg = 3;
            selected.bg = 4;
            let mut border = grid_default_cell;
            border.fg = 5;
            border.bg = 6;
            let originals = [normal, selected, border];
            let mut ctx = screen_write_ctx::default();
            screen_write_start(&mut ctx, &mut s);
            screen_write_menu(
                &mut ctx,
                &menu,
                choice,
                0,
                &normal,
                Some(&border),
                &selected,
            );
            screen_write_stop(&mut ctx);
            assert_eq!((s.cx, s.cy), (1, 1));
            for (style, original) in [&normal, &selected, &border].into_iter().zip(&originals) {
                assert!(grid_cells_equal(style, original));
            }
            for (y, text) in [
                (1, b"Title".as_slice()),
                (2, b"First"),
                (4, b"Off"),
                (5, b"After"),
            ] {
                for (index, byte) in text.iter().enumerate() {
                    assert_eq!(cell_at(&s, 3 + index as u32, y).data.data[0], *byte);
                }
            }
            assert_eq!(cell_at(&s, 1, 3).data.data[0], b't');
            assert_eq!(cell_at(&s, 10, 3).data.data[0], b'u');
            let first = cell_at(&s, 3, 2);
            assert_eq!(
                (first.fg, first.bg),
                if choice == 0 { (3, 4) } else { (1, 2) }
            );
            let disabled = cell_at(&s, 3, 4);
            assert_eq!((disabled.fg, disabled.bg), (1, 2));
            assert_ne!(disabled.attr as i32 & GRID_ATTR_DIM, 0);
            assert_eq!(cell_at(&s, 3, 5).attr as i32 & GRID_ATTR_DIM, 0);
            assert_eq!(menu.items[2].name.as_deref(), Some(c"-Off"));
            assert_eq!(menu.title.as_c_str(), c"Title");
        }
    }
}
