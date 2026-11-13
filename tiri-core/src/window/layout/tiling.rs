//! The tiling layout: the panes divide the window between them, and nothing
//! extends past it. A fixed rule over the pane list places them: the first
//! pane takes the window, and each next pane splits the previous pane's
//! rectangle in two, the earlier pane keeping the first half. The first split
//! is along the window's longer side; the splits after it alternate, so a wide
//! window does not end up as one row of narrow columns.

use super::{fits, window_geometry, Layout, LayoutNode};
use crate::shared::layout::layout_geometry;
use crate::shared::pane::window_pane;
use std::cell::RefCell;
use std::ffi::CStr;
use std::rc::Rc;

/// The tiling layout.
pub(super) struct Tiling;

impl Layout for Tiling {
    /// `count` panes, each splitting the previous one's rectangle.
    fn shape(count: usize, size: (u32, u32)) -> LayoutNode {
        let g = window_geometry(size);
        split_from(0, count, g, splits_across(g))
    }

    /// Only a split adds a pane, so a list that does not fit is a pane too
    /// small to split.
    fn admits(panes: &[Rc<RefCell<window_pane>>], size: (u32, u32)) -> Result<(), &'static CStr> {
        fits(
            &Self::arrange(panes, size),
            size,
            c"no space for a new pane: the pane is too small to split",
        )
    }
}

/// Panes `first` onwards of `count`, in rectangle `g`, split `across` or
/// stacked; the next split goes the other way.
fn split_from(first: usize, count: usize, g: layout_geometry, across: bool) -> LayoutNode {
    if first + 1 == count {
        return LayoutNode::Pane(first);
    }
    LayoutNode::Split {
        across,
        children: vec![
            (1, LayoutNode::Pane(first)),
            (
                1,
                split_from(first + 1, count, second_half(g, across), !across),
            ),
        ],
    }
}

/// Whether the first split of `g` puts its halves side by side: its longer
/// side, as the eye sees it, is across. A terminal cell is about twice as tall as it is
/// wide.
fn splits_across(g: layout_geometry) -> bool {
    g.sx >= 2 * g.sy
}

/// The second half of `g` split `across` or stacked, as `LayoutNode::place`
/// shares it: one separator between the halves, the odd cell to the first.
fn second_half(g: layout_geometry, across: bool) -> layout_geometry {
    let extent = if across { g.sx } else { g.sy };
    let shared = extent.saturating_sub(1);
    let first = shared - shared / 2;
    let offset = (first + 1) as i32;
    if across {
        layout_geometry {
            sx: shared / 2,
            xoff: g.xoff + offset,
            ..g
        }
    } else {
        layout_geometry {
            sy: shared / 2,
            yoff: g.yoff + offset,
            ..g
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::LayoutKind;
    use crate::shared::pane::window_pane;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn panes(count: usize) -> Vec<Rc<RefCell<window_pane>>> {
        (0..count)
            .map(|_| Rc::new(RefCell::new(window_pane::default())))
            .collect()
    }

    fn rects(count: usize, size: (u32, u32)) -> Vec<(u32, u32, i32, i32)> {
        LayoutKind::Tiling
            .arrange(&panes(count), size)
            .iter()
            .map(|g| (g.sx, g.sy, g.xoff, g.yoff))
            .collect()
    }

    #[test]
    fn a_lone_pane_fills_the_window() {
        assert_eq!(rects(1, (80, 24)), [(80, 24, 0, 0)]);
    }

    #[test]
    fn the_first_split_is_along_the_longer_side_and_the_rest_alternate() {
        assert_eq!(rects(2, (80, 24)), [(40, 24, 0, 0), (39, 24, 41, 0)]);
        assert_eq!(
            rects(3, (80, 24)),
            [(40, 24, 0, 0), (39, 12, 41, 0), (39, 11, 41, 13)]
        );
        assert_eq!(
            rects(4, (80, 24)),
            [
                (40, 24, 0, 0),
                (39, 12, 41, 0),
                (19, 11, 41, 13),
                (19, 11, 61, 13)
            ]
        );
        assert_eq!(
            rects(3, (40, 60)),
            [(40, 30, 0, 0), (20, 29, 0, 31), (19, 29, 21, 31)]
        );
    }

    #[test]
    fn a_wide_window_does_not_become_a_row_of_columns() {
        assert_eq!(
            rects(4, (200, 24)),
            [
                (100, 24, 0, 0),
                (99, 12, 101, 0),
                (49, 11, 101, 13),
                (49, 11, 151, 13)
            ]
        );
    }

    #[test]
    fn the_same_list_always_tiles_the_same_way() {
        assert_eq!(rects(5, (81, 25)), rects(5, (81, 25)));
    }

    #[test]
    fn panes_below_the_minimum_are_refused() {
        let list = panes(3);
        assert!(LayoutKind::Tiling.admits(&list, (81, 25)).is_ok());
        assert!(LayoutKind::Tiling.admits(&list, (2, 2)).is_err());
    }
}
