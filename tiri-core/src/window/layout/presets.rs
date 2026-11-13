//! Layouts that are a shape over the number of panes alone. The first pane in
//! the list is the main pane; the rest are side panes, in order. Swapping or
//! rotating the list is enough to change which pane is the main one.

use super::{Layout, LayoutNode};

/// The main pane, then the side panes in a line the other way, sharing the
/// split equally.
fn main_and_sides(count: usize, across: bool) -> LayoutNode {
    match count {
        1 => LayoutNode::Pane(0),
        count => LayoutNode::Split {
            across,
            children: vec![
                (1, LayoutNode::Pane(0)),
                (1, LayoutNode::line(!across, 1..count)),
            ],
        },
    }
}

/// The main pane on the left half; the side panes stacked on the right half.
pub(super) struct MainVertical;

impl Layout for MainVertical {
    fn shape(count: usize, _size: (u32, u32)) -> LayoutNode {
        main_and_sides(count, true)
    }
}

/// The main pane on the top half; the side panes side by side on the bottom
/// half.
pub(super) struct MainHorizontal;

impl Layout for MainHorizontal {
    fn shape(count: usize, _size: (u32, u32)) -> LayoutNode {
        main_and_sides(count, false)
    }
}

/// The main pane in the middle half; the side panes stacked in a column on
/// either side, the right one taking the first half of them. A lone side pane
/// sits on the right.
pub(super) struct MainCentered;

impl Layout for MainCentered {
    fn shape(count: usize, _size: (u32, u32)) -> LayoutNode {
        let sides = count - 1;
        if sides < 2 {
            return main_and_sides(count, true);
        }
        let right = 1..1 + sides.div_ceil(2);
        let left = right.end..count;
        LayoutNode::Split {
            across: true,
            children: vec![
                (1, LayoutNode::line(false, left)),
                (2, LayoutNode::Pane(0)),
                (1, LayoutNode::line(false, right)),
            ],
        }
    }
}

/// Every pane an equal share of an even grid.
pub(super) struct Grid;

impl Layout for Grid {
    fn shape(count: usize, _size: (u32, u32)) -> LayoutNode {
        LayoutNode::grid(count)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{adjacent, Direction, LayoutKind};
    use crate::shared::pane::window_pane;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn panes(count: usize) -> Vec<Rc<RefCell<window_pane>>> {
        (0..count)
            .map(|_| Rc::new(RefCell::new(window_pane::default())))
            .collect()
    }

    fn rects(kind: LayoutKind, count: usize, sx: u32, sy: u32) -> Vec<(u32, u32, i32, i32)> {
        kind.arrange(&panes(count), (sx, sy))
            .iter()
            .map(|g| (g.sx, g.sy, g.xoff, g.yoff))
            .collect()
    }

    #[test]
    fn a_lone_pane_fills_the_window_in_every_preset() {
        for kind in [
            LayoutKind::MainVertical,
            LayoutKind::MainCentered,
            LayoutKind::MainHorizontal,
            LayoutKind::Grid,
        ] {
            assert_eq!(rects(kind, 1, 80, 24), [(80, 24, 0, 0)]);
        }
    }

    #[test]
    fn main_vertical_stacks_the_side_panes_on_the_right() {
        assert_eq!(
            rects(LayoutKind::MainVertical, 3, 81, 25),
            [(40, 25, 0, 0), (40, 12, 41, 0), (40, 12, 41, 13)]
        );
    }

    #[test]
    fn main_horizontal_lines_the_side_panes_up_below() {
        assert_eq!(
            rects(LayoutKind::MainHorizontal, 3, 81, 25),
            [(81, 12, 0, 0), (40, 12, 0, 13), (40, 12, 41, 13)]
        );
    }

    #[test]
    fn main_centered_splits_the_side_panes_around_the_main_one() {
        assert_eq!(
            rects(LayoutKind::MainCentered, 2, 81, 24),
            [(40, 24, 0, 0), (40, 24, 41, 0)]
        );
        assert_eq!(
            rects(LayoutKind::MainCentered, 4, 81, 25),
            [
                (40, 25, 21, 0),
                (19, 12, 62, 0),
                (19, 12, 62, 13),
                (20, 25, 0, 0)
            ]
        );
    }

    #[test]
    fn grid_gives_every_pane_an_equal_share() {
        assert_eq!(
            rects(LayoutKind::Grid, 4, 81, 25),
            [
                (40, 12, 0, 0),
                (40, 12, 41, 0),
                (40, 12, 0, 13),
                (40, 12, 41, 13)
            ]
        );
        assert_eq!(
            rects(LayoutKind::Grid, 3, 81, 25),
            [(40, 12, 0, 0), (40, 12, 41, 0), (81, 12, 0, 13)]
        );
    }

    #[test]
    fn a_preset_takes_a_list_while_every_pane_stays_inside() {
        assert!(LayoutKind::MainVertical.admits(&panes(3), (12, 6)).is_ok());
        assert!(LayoutKind::MainVertical.admits(&panes(5), (12, 6)).is_err());
    }

    #[test]
    fn neighbours_are_across_the_separators() {
        let cells = LayoutKind::MainVertical.arrange(&panes(3), (81, 25));
        assert_eq!(adjacent(&cells, 0, Direction::Right), Some(1));
        assert_eq!(adjacent(&cells, 2, Direction::Left), Some(0));
        assert_eq!(adjacent(&cells, 1, Direction::Down), Some(2));
    }
}
