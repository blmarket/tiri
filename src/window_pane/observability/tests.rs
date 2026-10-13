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

#[test]
fn weak_observation_expires_on_destruction_and_cannot_follow_reused_id() {
    use crate::src::shared::pane::window_pane_tree;
    use crate::src::shared::pane::PANE_DESTROYED;
    unsafe {
        let owner = window_pane::new();
        (*owner.get()).id = 71;
        (*owner.get()).output_generation = 42;
        window_pane_tree::insert(owner.clone());
        let observed = resolve(PaneId(71)).unwrap();
        assert_eq!(observed.output_revision().unwrap(), 42);
        let registry_owner = window_pane_tree::remove(&owner).unwrap();
        (*owner.get()).flags |= PANE_DESTROYED;
        drop(registry_owner);
        assert!(observed.output_revision().is_err());
        let replacement = window_pane::new();
        (*replacement.get()).id = 71;
        window_pane_tree::insert(replacement.clone());
        assert!(observed.process().is_err());
        assert_eq!(resolve(PaneId(71)).unwrap().output_revision().unwrap(), 0);
        drop(window_pane_tree::remove(&replacement));
        // Drop cannot retain pane storage: these observations only hold Weak.
        assert_eq!(Rc::strong_count(&owner), 1);
        drop(observed);
    }
}
