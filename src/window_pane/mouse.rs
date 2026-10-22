//! Pane mouse hit testing uses copied geometry across Window operations.
use super::*;
use crate::shared::pane::PANE_SCROLLBARS_RIGHT;

pub(super) fn in_scrollbar_area(
    pane: &Rc<RefCell<window_pane>>,
    mut px: ::core::ffi::c_int,
    mut py: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let (sx, sy, xoff, yoff) = pane.geometry();
    let scrollbar = &pane;
    let mut width: u_int = 0;
    let mut pad: u_int = 0;
    let mut total: u_int = 0;
    let mut start: ::core::ffi::c_int = 0;
    let mut end: ::core::ffi::c_int = 0;
    if !scrollbar.scrollbar_overlay() {
        return 0 as ::core::ffi::c_int;
    }
    if py < yoff || py >= yoff + sy as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    width = scrollbar.scrollbar_width() as u_int;
    pad = scrollbar.scrollbar_pad() as u_int;
    total = width.wrapping_add(pad);
    if total == 0 as u_int || total > sx {
        total = sx;
    }
    let window = pane
        .window_observer()
        .upgrade()
        .expect("scrollbar pane window");
    let position = window.scrollbar_position();
    drop(window);
    if position == PANE_SCROLLBARS_LEFT {
        start = xoff;
        end = xoff + total as ::core::ffi::c_int - 1 as ::core::ffi::c_int;
    } else {
        end = xoff + sx as ::core::ffi::c_int - 1 as ::core::ffi::c_int;
        start = end - total as ::core::ffi::c_int + 1 as ::core::ffi::c_int;
    }
    (px >= start && px <= end) as ::core::ffi::c_int
}

pub(super) fn mouse_location(
    pane: &Rc<RefCell<window_pane>>,
    x: i32,
    y: i32,
    slider: &mut u32,
) -> key_code_mouse_location {
    let window = pane.window_observer().upgrade().expect("mouse pane window");
    let location = mouse_location_in(pane, &window, x, y, slider);
    drop(window);
    location
}

fn mouse_location_in(
    pane_owner: &Rc<RefCell<window_pane>>,
    window_owner: &WindowRef,
    mut px: ::core::ffi::c_int,
    mut py: ::core::ffi::c_int,
    sl_mpos: &mut u_int,
) -> key_code_mouse_location {
    let (sx, sy, xoff, yoff) = pane_owner.geometry();
    let scrollbar = &pane_owner;
    let position = window_owner.scrollbar_position();
    let mut sb_w: ::core::ffi::c_int = 0;
    let mut sb_pad: ::core::ffi::c_int = 0;
    let mut sl_top: ::core::ffi::c_int = 0;
    let mut sl_bottom: ::core::ffi::c_int = 0;
    let mut sb_start: ::core::ffi::c_int = 0;
    let mut sb_end: ::core::ffi::c_int = 0;
    let mut sb_overlay: ::core::ffi::c_int = 0;
    sb_overlay = scrollbar.scrollbar_overlay() as i32;
    if scrollbar.scrollbar_visible() {
        sb_w = scrollbar.scrollbar_width();
        sb_pad = scrollbar.scrollbar_pad();
        if sb_overlay != 0 && sb_w > sx as ::core::ffi::c_int {
            sb_w = sx as ::core::ffi::c_int;
        }
    } else {
        sb_w = 0 as ::core::ffi::c_int;
        sb_pad = 0 as ::core::ffi::c_int;
    }
    if sb_overlay != 0
        && sb_w != 0 as ::core::ffi::c_int
        && py >= yoff
        && py < yoff + sy as ::core::ffi::c_int
        && px >= xoff
        && px < xoff + sx as ::core::ffi::c_int
    {
        if position == PANE_SCROLLBARS_LEFT {
            sb_start = xoff;
            sb_end = sb_start + sb_w - 1 as ::core::ffi::c_int;
        } else {
            sb_end = xoff + sx as ::core::ffi::c_int - 1 as ::core::ffi::c_int;
            sb_start = sb_end - sb_w + 1 as ::core::ffi::c_int;
        }
        if px >= sb_start && px <= sb_end {
            sl_top =
                (yoff as u_int).wrapping_add(scrollbar.scrollbar_slider_y()) as ::core::ffi::c_int;
            sl_bottom = (yoff as u_int)
                .wrapping_add(scrollbar.scrollbar_slider_y())
                .wrapping_add(scrollbar.scrollbar_slider_height())
                .wrapping_sub(1 as u_int) as ::core::ffi::c_int;
            if py < sl_top {
                return KEYC_MOUSE_LOCATION_SCROLLBAR_UP;
            } else if py >= sl_top && py <= sl_bottom {
                *sl_mpos = (py as u_int)
                    .wrapping_sub(scrollbar.scrollbar_slider_y())
                    .wrapping_sub(yoff as u_int);
                return KEYC_MOUSE_LOCATION_SCROLLBAR_SLIDER;
            } else {
                return KEYC_MOUSE_LOCATION_SCROLLBAR_DOWN;
            }
        }
        return KEYC_MOUSE_LOCATION_PANE;
    }
    if py >= yoff
        && py < yoff + sy as ::core::ffi::c_int
        && (position == PANE_SCROLLBARS_RIGHT
            && px < xoff + sx as ::core::ffi::c_int + sb_pad + sb_w
            || position == PANE_SCROLLBARS_LEFT
                && px < xoff + sx as ::core::ffi::c_int - sb_pad - sb_w)
    {
        if position == PANE_SCROLLBARS_RIGHT
            && (px >= xoff + sx as ::core::ffi::c_int + sb_pad
                && px < xoff + sx as ::core::ffi::c_int + sb_pad + sb_w)
            || position == PANE_SCROLLBARS_LEFT
                && (px >= xoff - sb_pad - sb_w && px < xoff - sb_pad)
        {
            sl_top =
                (yoff as u_int).wrapping_add(scrollbar.scrollbar_slider_y()) as ::core::ffi::c_int;
            sl_bottom = (yoff as u_int)
                .wrapping_add(scrollbar.scrollbar_slider_y())
                .wrapping_add(scrollbar.scrollbar_slider_height())
                .wrapping_sub(1 as u_int) as ::core::ffi::c_int;
            if py < sl_top {
                return KEYC_MOUSE_LOCATION_SCROLLBAR_UP;
            } else if py >= sl_top && py <= sl_bottom {
                *sl_mpos = (py as u_int)
                    .wrapping_sub(scrollbar.scrollbar_slider_y())
                    .wrapping_sub(yoff as u_int);
                return KEYC_MOUSE_LOCATION_SCROLLBAR_SLIDER;
            } else {
                return KEYC_MOUSE_LOCATION_SCROLLBAR_DOWN;
            }
        } else {
            return KEYC_MOUSE_LOCATION_PANE;
        }
    }
    KEYC_MOUSE_LOCATION_BORDER
}
