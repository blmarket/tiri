//! Pane border cache and status rendering.
use super::*;
use crate::format::{
    format_create, format_create_defaults, format_defaults, format_expand_time_cstring,
};
use crate::format_draw::format_draw;
use crate::grid::{grid_compare, grid_default_cell};
use crate::options::options_get_string;
use crate::options::OptionsScope;
use crate::screen_redraw::redraw_get_status_border_cell_type;
use crate::screen_write::{
    screen_write_cell, screen_write_cursormove, screen_write_start, screen_write_stop,
};
use crate::shared::format::{FORMAT_PANE, FORMAT_STATUS};
use crate::shared::redraw::redraw_spans;
use crate::style::{style_apply_with_options, style_ranges_clear};
use crate::window_border::window_get_border_cell;

pub(super) unsafe fn cell(
    pane: &Rc<UnsafeCell<window_pane>>,
    cell_type: ::core::ffi::c_int,
    cell: &mut grid_cell,
) {
    let window = pane.window_observer().upgrade().expect("live pane parent");
    let index = window.pane_index(&Rc::downgrade(pane));
    drop(window);
    window_get_border_cell(index, pane.pane_lines(), cell_type, cell);
}

pub(super) unsafe fn border_style(
    pane: &Rc<UnsafeCell<window_pane>>,
    client: &ClientRef,
) -> grid_cell {
    let session = client
        .attached_session()
        .upgrade()
        .expect("live border session");
    let link = session.current_winlink();
    let window = link
        .get_unchecked()
        .window_handle()
        .expect("live border window")
        .clone();
    let active = window.active_pane().ptr_eq(&Rc::downgrade(pane));
    drop(window);
    let cached = {
        let state = &*pane.get();
        if active {
            (state.active_border_gc_set != 0, state.active_border_gc)
        } else {
            (state.border_gc_set != 0, state.border_gc)
        }
    };
    if cached.0 {
        return cached.1;
    }
    let mut context = format_create_defaults(None, Some(client), Some(&session), link, Some(pane));
    let mut cell = grid_default_cell;
    let key = if active {
        c"pane-active-border-style"
    } else {
        c"pane-border-style"
    };
    style_apply_with_options(
        &mut cell,
        key,
        Some(&mut context),
        &OptionsScope::Pane(Rc::downgrade(pane)),
    );
    drop(context);
    let state = &mut *pane.get();
    if active {
        state.active_border_gc = cell;
        state.active_border_gc_set = 1;
    } else {
        state.border_gc = cell;
        state.border_gc_set = 1;
    }
    cell
}

/// Build the pane's status line: the separator glyphs of its status row, each
/// the line its border cell draws, with `pane-border-format` drawn over them.
/// Returns whether the line changed.
pub(super) unsafe fn make_status(
    pane: &Rc<UnsafeCell<window_pane>>,
    client: &ClientRef,
    width: u32,
    spans: &redraw_spans,
    mut span_index: usize,
) -> bool {
    if pane.border_status() == PANE_STATUS_OFF || width == 0 {
        return false;
    }
    let mut context = format_create(
        Some(client),
        None,
        (FORMAT_PANE | pane.id()) as i32,
        FORMAT_STATUS,
    );
    let session = client
        .attached_session()
        .upgrade()
        .expect("live pane status session");
    format_defaults(
        &mut *context,
        Some(client),
        Some(&session),
        session.current_winlink(),
        Some(pane),
    );
    let value = options_get_string(&pane.borrow_options_mut(), c"pane-border-format");
    let expanded = format_expand_time_cstring(&mut *context, &value);
    let mut cell = pane.border_style(client);
    let lines = pane.pane_lines();
    let window = pane
        .window_observer()
        .upgrade()
        .expect("live pane status parent");
    let index = window.pane_index(&Rc::downgrade(pane));
    drop(window);
    let (mut old, mut ranges) = {
        let state = &mut *pane.get();
        (
            std::mem::take(&mut state.status_screen),
            std::mem::take(&mut state.border_status_line.ranges),
        )
    };
    let mut next = screen::empty();
    screen_init(&mut next, width, 1, 0);
    next.mode = 0;
    let mut write = screen_write_start(&mut next);
    for offset in 0..width {
        let cell_type = redraw_get_status_border_cell_type(spans, &mut span_index, offset);
        window_get_border_cell(index, lines, cell_type, &mut cell);
        screen_write_cell(&mut write, &cell);
    }
    cell.attr = (cell.attr as i32 & !GRID_ATTR_CHARSET) as u_short;
    screen_write_cursormove(&mut write, 0, 0, 0);
    style_ranges_clear(&mut ranges);
    format_draw(&mut write, &cell, width, &expanded, Some(&mut ranges), 0);
    screen_write_stop(write);
    drop(context);
    let changed = grid_compare(next.grid(), old.grid()) != 0;
    drop(old);
    {
        let state = &mut *pane.get();
        state.status_screen = next;
        state.border_status_line.ranges = ranges;
    }
    changed
}
