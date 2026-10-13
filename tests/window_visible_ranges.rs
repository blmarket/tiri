use tiri::src::shared::display::visible_range;
use tiri::src::shared::pane::window_pane;
use tiri::src::window_visible::{window_position_is_visible, window_visible_ranges};
use std::cell::UnsafeCell;
use std::rc::Rc;

fn segments(ranges: &[visible_range]) -> Vec<(u32, u32)> {
    ranges.iter().map(|range| (range.px, range.nx)).collect()
}

unsafe fn calculate_ranges(
    pane: Option<&Rc<UnsafeCell<window_pane>>>,
    x: i32,
    y: i32,
    width: u32,
) -> Vec<visible_range> {
    let mut ranges = Vec::new();
    window_visible_ranges(pane, x, y, width, &mut ranges);
    ranges
}

#[test]
fn pane_less_results_survive_later_nonempty_and_empty_queries() {
    unsafe {
        let first = calculate_ranges(None, 10, 5, 2);
        let second = calculate_ranges(None, 0, 6, 80);
        let empty = calculate_ranges(None, 0, -1, 80);
        assert_eq!(segments(&first), [(10, 2)]);
        assert_eq!(segments(&second), [(0, 80)]);
        assert!(empty.is_empty());
        assert!(window_position_is_visible(&first, 10));
        assert!(!window_position_is_visible(&first, 12));
        assert!(!window_position_is_visible(&empty, 10));

        let third = calculate_ranges(None, -3, 0, 8);
        assert_eq!(segments(&third), [(0, 5)]);
        assert!(empty.is_empty());
    }
}
