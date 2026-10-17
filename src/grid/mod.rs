//! Grid storage and cell operations, with viewport access and reader traversal.

mod core;
pub mod reader;
pub mod view;

pub use self::core::{
    grid_adjust_lines, grid_cell_attr_display, grid_cell_flags_display, grid_cells_equal,
    grid_cells_look_equal, grid_check_is_clear, grid_clear, grid_clear_history, grid_clear_lines,
    grid_collect_history, grid_compare, grid_create, grid_default_cell, grid_duplicate_lines,
    grid_empty_line, grid_free_lines, grid_get_cell, grid_get_line, grid_get_line_mut, grid_in_set,
    grid_line_flags_display, grid_line_length, grid_line_limit, grid_line_time, grid_move_cells,
    grid_move_lines, grid_peek_line, grid_reflow, grid_remove_history, grid_scroll_history,
    grid_scroll_history_region, grid_set_cell, grid_set_cells, grid_set_padding, grid_set_tab,
    grid_string_cells_bytes, grid_unwrap_position, grid_wrap_position,
};
