//! The scrolling layout: the panes sit side by side in one strip that may
//! extend beyond the window, each using half or all of the visible width, as
//! its metadata says, and the full height.

use super::{Layout, LayoutAction, LayoutNode};
use crate::shared::layout::layout_geometry;
use crate::shared::pane::window_pane;
use crate::shared::pane::PANE_MINIMUM;
use crate::shared::window::WINDOW_MAXIMUM;
use crate::window_pane::WindowPane as _;
use std::any::Any;
use std::cell::RefCell;
use std::ffi::CStr;
use std::rc::Rc;

/// A pane's share of the visible window width in the strip: this layout's
/// metadata.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
enum Width {
    #[default]
    Half,
    Full,
}

/// A pane's width: its metadata, or half when it has none of this layout's.
fn width_of(meta: Option<&dyn Any>) -> Width {
    meta.and_then(|meta| meta.downcast_ref::<Width>())
        .copied()
        .unwrap_or_default()
}

/// Every pane's width, in list order.
fn widths(panes: &[Rc<RefCell<window_pane>>]) -> Vec<Width> {
    panes
        .iter()
        .map(|pane| unsafe { width_of(pane.borrow_layout_meta().as_deref()) })
        .collect()
}

/// The scrolling layout.
pub(super) struct Scrolling;

impl Layout for Scrolling {
    /// A lone pane, or one row of `count` panes.
    fn shape(count: usize, _size: (u32, u32)) -> LayoutNode {
        LayoutNode::line(true, 0..count)
    }

    /// The strip of `panes` at their widths.
    fn arrange(panes: &[Rc<RefCell<window_pane>>], size: (u32, u32)) -> Vec<layout_geometry> {
        cells(&widths(panes), size)
    }

    /// The strip takes panes while its length stays within WINDOW_MAXIMUM
    /// columns.
    fn admits(panes: &[Rc<RefCell<window_pane>>], size: (u32, u32)) -> Result<(), &'static CStr> {
        if strip_length(widths(panes).into_iter(), size.0) <= WINDOW_MAXIMUM as u32 {
            Ok(())
        } else {
            Err(c"no space: the strip would pass its maximum width")
        }
    }

    /// Columns a view of the strip can scroll across, by the window height.
    fn logical_size(
        panes: impl Iterator<Item = Rc<RefCell<window_pane>>>,
        (sx, sy): (u32, u32),
    ) -> (u32, u32) {
        let widths = panes.map(|pane| unsafe { width_of(pane.borrow_layout_meta().as_deref()) });
        (strip_length(widths, sx), sy)
    }

    /// `resize-pane -W` switches the pane to the other width.
    fn apply(
        action: LayoutAction,
        meta: Option<&dyn Any>,
    ) -> Result<Option<Box<dyn Any>>, &'static CStr> {
        match action {
            LayoutAction::ToggleWidth => Ok(Some(Box::new(match width_of(meta) {
                Width::Half => Width::Full,
                Width::Full => Width::Half,
            }))),
        }
    }

    /// Every separator between two panes, whatever the pane count: a view
    /// shows two panes and the one separator between them however many the
    /// strip holds.
    fn splits_separator_colours(_panes: usize) -> bool {
        true
    }
}

/// A half pane is half the window beside its separator column, so two halves
/// and their separator fit in the window. A full pane is the window's width.
/// Neither is narrower than PANE_MINIMUM.
fn pane_width(width: Width, sx: u32) -> u32 {
    match width {
        Width::Half => sx.saturating_sub(1) / 2,
        Width::Full => sx,
    }
    .max(PANE_MINIMUM as u32)
}

/// Rectangles of a strip whose panes have `widths`, in an `sx` by `sy`
/// window, in pane order. Each pane starts one separator column after the
/// previous one ends and uses the full height.
fn cells(widths: &[Width], (sx, sy): (u32, u32)) -> Vec<layout_geometry> {
    let mut xoff = 0u32;
    widths
        .iter()
        .map(|&width| {
            let width = pane_width(width, sx);
            let cell = layout_geometry {
                sx: width,
                sy,
                xoff: xoff as i32,
                yoff: 0,
            };
            xoff = xoff.saturating_add(width + 1);
            cell
        })
        .collect()
}

/// Columns a view of a strip whose panes have `widths` in an `sx`-column
/// window can scroll across: the last pane's first column plus the window
/// width. Every pane's first column is then a reachable view offset, and the
/// last pane's right border has room. A pane raised to PANE_MINIMUM past a
/// tiny window can push the last pane's right edge further; the length
/// reaches it too. Without panes the length is the window width.
fn strip_length(widths: impl Iterator<Item = Width>, sx: u32) -> u32 {
    let mut start = 0u32;
    let mut last = None;
    for width in widths {
        if let Some(previous) = last.replace(width) {
            start = start.saturating_add(pane_width(previous, sx) + 1);
        }
    }
    last.map_or(sx, |width| {
        start.saturating_add(pane_width(width, sx).max(sx))
    })
}

#[cfg(test)]
mod tests {
    use super::super::content_width as width;
    use super::*;

    fn starts(cells: &[layout_geometry]) -> Vec<i32> {
        cells.iter().map(|cell| cell.xoff).collect()
    }

    const HALVES: [Width; 3] = [Width::Half; 3];

    #[test]
    fn panes_are_half_width_and_extend_the_strip() {
        let odd = cells(&HALVES, (81, 24));
        assert!(odd
            .iter()
            .all(|cell| (cell.sx, cell.sy, cell.yoff) == (40, 24, 0)));
        assert_eq!(starts(&odd), [0, 41, 82]);
        assert_eq!(width(&odd[..2]), 81);
        let even = cells(&HALVES[..2], (80, 24));
        assert_eq!((even[0].sx, starts(&even)), (39, vec![0, 40]));
        assert_eq!(width(&even), 79);
        assert_eq!(width(&even[..1]), 39);
        assert_eq!(width(&[]), 0);
    }

    #[test]
    fn the_length_reaches_the_window_width_past_the_last_pane_start() {
        let length = |widths: &[Width], sx| strip_length(widths.iter().copied(), sx);
        assert_eq!(length(&HALVES, 81), 82 + 81);
        assert_eq!(length(&HALVES[..2], 80), 40 + 80);
        assert_eq!(length(&HALVES[..1], 80), 80);
        assert_eq!(length(&[Width::Full], 80), 80);
        assert_eq!(length(&[], 80), 80);
        assert_eq!(length(&HALVES[..2], 2), 2 + 2);
    }

    #[test]
    fn full_panes_take_the_window_width_beside_halves() {
        use Width::{Full, Half};
        let mixed = cells(&[Half, Full, Half], (80, 24));
        assert_eq!(
            mixed.iter().map(|cell| cell.sx).collect::<Vec<_>>(),
            [39, 80, 39]
        );
        assert_eq!(starts(&mixed), [0, 40, 121]);
        assert_eq!(width(&mixed), 160);
        assert_eq!(strip_length([Half, Full, Half].into_iter(), 80), 201);
        assert_eq!(cells(&[Full], (81, 24))[0].sx, 81);
        assert_eq!(width(&cells(&[Full], (81, 24))), 81);
        assert_eq!(width(&cells(&[Full, Full], (81, 24))), 163);
    }

    #[test]
    fn tiny_windows_raise_panes_to_the_minimum() {
        let tiny = cells(&HALVES[..2], (2, 3));
        assert!(tiny.iter().all(|cell| cell.sx == 1));
        assert_eq!(starts(&tiny), [0, 2]);
        assert_eq!(width(&tiny), 3);
        assert_eq!(cells(&HALVES[..1], (1, 1))[0].sx, 1);
    }
}
