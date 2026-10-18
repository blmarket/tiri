use crate::shared::abi::*;
use crate::shared::display::visible_range;
use crate::shared::pane::window_pane;

pub fn window_position_is_visible(ranges: &[visible_range], px: u_int) -> bool {
    ranges
        .iter()
        .any(|range| range.nx != 0 && px >= range.px && px < range.px.wrapping_add(range.nx))
}

/// Replace `ranges` with the unobstructed parts of a horizontal span in window
/// coordinates, retaining its allocation. Callers may reuse the buffer once its
/// previous result is consumed; nested queries need a separate buffer.
pub unsafe fn window_visible_ranges(
    pane: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
    x: i32,
    y: i32,
    width: u32,
    ranges: &mut Vec<visible_range>,
) {
    use crate::window::WindowPane;
    <std::rc::Rc<std::cell::UnsafeCell<window_pane>> as WindowPane>::visible_ranges(
        pane, x, y, width, ranges,
    );
}
