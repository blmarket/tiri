use crate::src::ffi::libc::strlcpy;
use crate::src::format::format_skip;
use crate::src::grid::grid_default_cell;
use crate::src::hyperlinks::hyperlinks_put;
use crate::src::log::{log_cstr, log_debug};
use crate::src::screen::screen_init;
use crate::src::screen_write::{
    screen_write_cell, screen_write_clearendofline, screen_write_cursormove,
    screen_write_fast_copy, screen_write_putc, screen_write_start, screen_write_stop,
};
use crate::src::shared::abi::*;
use crate::src::shared::grid::*;
use crate::src::shared::screen::screen;
use crate::src::shared::screen_write::screen_write_ctx;
use crate::src::shared::style::*;
use crate::src::shared::utf8::*;
use crate::src::style::{style_copy, style_link, style_parse, style_set, style_tostring};
use crate::src::text::utf8::{utf8_append, utf8_open, utf8_set};
use libc::{strcmp, strlen};
use std::ffi::{CStr, CString};

pub const TOTAL: C2RustUnnamed_39 = 8;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct format_range {
    pub index: u_int,
    pub start: u_int,
    pub end: u_int,
    pub type_0: style_range_type,
    pub argument: u_int,
    pub string: [::core::ffi::c_char; 16],
}
type format_ranges = Vec<format_range>;

pub const AFTER: C2RustUnnamed_39 = 7;
pub const LIST_RIGHT: C2RustUnnamed_39 = 6;
pub const LIST_LEFT: C2RustUnnamed_39 = 5;
pub const LIST: C2RustUnnamed_39 = 4;
pub const ABSOLUTE_CENTRE: C2RustUnnamed_39 = 3;
pub const RIGHT: C2RustUnnamed_39 = 2;
pub const CENTRE: C2RustUnnamed_39 = 1;
pub const LEFT: C2RustUnnamed_39 = 0;
pub type C2RustUnnamed_39 = ::core::ffi::c_uint;
unsafe fn format_is_type(mut fr: *mut format_range, mut sy: *mut style) -> ::core::ffi::c_int {
    if (*fr).type_0 as ::core::ffi::c_uint != (*sy).range_type as ::core::ffi::c_uint {
        return 0 as ::core::ffi::c_int;
    }
    match (*fr).type_0 as ::core::ffi::c_uint {
        0 | 1 | 2 | 7 => return 1 as ::core::ffi::c_int,
        3..=5 => {
            return ((*fr).argument == (*sy).range_argument) as ::core::ffi::c_int;
        }
        6 => {
            return (strcmp(
                &raw mut (*fr).string as *mut ::core::ffi::c_char,
                &raw mut (*sy).range_string as *mut ::core::ffi::c_char,
            ) == 0 as ::core::ffi::c_int) as ::core::ffi::c_int;
        }
        _ => {}
    }
    1 as ::core::ffi::c_int
}
unsafe fn format_update_ranges(
    mut frs: *mut format_ranges,
    index: u_int,
    mut offset: u_int,
    mut start: u_int,
    mut width: u_int,
) {
    if frs.is_null() {
        return;
    }
    (*frs).retain_mut(|fr| {
        if fr.index != index {
            return true;
        }
        if fr.end <= start || fr.start >= start.wrapping_add(width) {
            return false;
        }
        if fr.start < start {
            fr.start = start;
        }
        if fr.end > start.wrapping_add(width) {
            fr.end = start.wrapping_add(width);
        }
        if fr.start == fr.end {
            return false;
        }
        fr.start = fr.start.wrapping_sub(start).wrapping_add(offset);
        fr.end = fr.end.wrapping_sub(start).wrapping_add(offset);
        true
    });
}
unsafe fn format_draw_put(
    mut octx: *mut screen_write_ctx,
    mut ocx: u_int,
    mut ocy: u_int,
    mut s: *mut screen,
    index: u_int,
    mut frs: *mut format_ranges,
    mut offset: u_int,
    mut start: u_int,
    mut width: u_int,
) {
    screen_write_cursormove(
        &mut *octx,
        ocx.wrapping_add(offset) as ::core::ffi::c_int,
        ocy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    screen_write_fast_copy(&mut *octx, &*s, start, 0 as u_int, width, 1 as u_int);
    format_update_ranges(frs, index, offset, start, width);
}
unsafe fn format_draw_put_list(
    mut octx: *mut screen_write_ctx,
    mut ocx: u_int,
    mut ocy: u_int,
    mut offset: u_int,
    mut width: u_int,
    mut list: *mut screen,
    mut list_left: *mut screen,
    mut list_right: *mut screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    mut frs: *mut format_ranges,
) {
    let mut start: u_int = 0;
    let mut focus_centre: u_int = 0;
    if width >= (*list).cx {
        format_draw_put(octx, ocx, ocy, list, LIST, frs, offset, 0 as u_int, width);
        return;
    }
    focus_centre = (focus_start + (focus_end - focus_start) / 2 as ::core::ffi::c_int) as u_int;
    if focus_centre < width.wrapping_div(2 as u_int) {
        start = 0 as u_int;
    } else {
        start = focus_centre.wrapping_sub(width.wrapping_div(2 as u_int));
    }
    if start.wrapping_add(width) > (*list).cx {
        start = (*list).cx.wrapping_sub(width);
    }
    if start != 0 as u_int && width > (*list_left).cx {
        screen_write_cursormove(
            &mut *octx,
            ocx.wrapping_add(offset) as ::core::ffi::c_int,
            ocy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_fast_copy(
            &mut *octx,
            &*list_left,
            0 as u_int,
            0 as u_int,
            (*list_left).cx,
            1 as u_int,
        );
        offset = offset.wrapping_add((*list_left).cx);
        start = start.wrapping_add((*list_left).cx);
        width = width.wrapping_sub((*list_left).cx);
    }
    if start.wrapping_add(width) < (*list).cx && width > (*list_right).cx {
        screen_write_cursormove(
            &mut *octx,
            ocx.wrapping_add(offset)
                .wrapping_add(width)
                .wrapping_sub((*list_right).cx) as ::core::ffi::c_int,
            ocy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_fast_copy(
            &mut *octx,
            &*list_right,
            0 as u_int,
            0 as u_int,
            (*list_right).cx,
            1 as u_int,
        );
        width = width.wrapping_sub((*list_right).cx);
    }
    format_draw_put(octx, ocx, ocy, list, LIST, frs, offset, start, width);
}
unsafe fn format_draw_none(
    mut octx: *mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    mut left: *mut screen,
    mut centre: *mut screen,
    mut right: *mut screen,
    mut abs_centre: *mut screen,
    mut frs: *mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut width_abs_centre: u_int = 0;
    width_left = (*left).cx;
    width_centre = (*centre).cx;
    width_right = (*right).cx;
    width_abs_centre = (*abs_centre).cx;
    while width_left
        .wrapping_add(width_centre)
        .wrapping_add(width_right)
        > available
    {
        if width_centre > 0 as u_int {
            width_centre = width_centre.wrapping_sub(1);
        } else if width_right > 0 as u_int {
            width_right = width_right.wrapping_sub(1);
        } else {
            width_left = width_left.wrapping_sub(1);
        }
    }
    format_draw_put(
        octx, ocx, ocy, left, LEFT, frs, 0 as u_int, 0 as u_int, width_left,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        right,
        RIGHT,
        frs,
        available.wrapping_sub(width_right),
        (*right).cx.wrapping_sub(width_right),
        width_right,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        centre,
        CENTRE,
        frs,
        width_left
            .wrapping_add(
                available
                    .wrapping_sub(width_right)
                    .wrapping_sub(width_left)
                    .wrapping_div(2 as u_int),
            )
            .wrapping_sub(width_centre.wrapping_div(2 as u_int)),
        (*centre)
            .cx
            .wrapping_div(2 as u_int)
            .wrapping_sub(width_centre.wrapping_div(2 as u_int)),
        width_centre,
    );
    if width_abs_centre > available {
        width_abs_centre = available;
    }
    format_draw_put(
        octx,
        ocx,
        ocy,
        abs_centre,
        ABSOLUTE_CENTRE,
        frs,
        available
            .wrapping_sub(width_abs_centre)
            .wrapping_div(2 as u_int),
        0 as u_int,
        width_abs_centre,
    );
}
unsafe fn format_draw_left(
    mut octx: *mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    mut left: *mut screen,
    mut centre: *mut screen,
    mut right: *mut screen,
    mut abs_centre: *mut screen,
    mut list: *mut screen,
    mut list_left: *mut screen,
    mut list_right: *mut screen,
    mut after: *mut screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    mut frs: *mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut width_abs_centre: u_int = 0;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    width_left = (*left).cx;
    width_centre = (*centre).cx;
    width_right = (*right).cx;
    width_abs_centre = (*abs_centre).cx;
    width_list = (*list).cx;
    width_after = (*after).cx;
    while width_left
        .wrapping_add(width_centre)
        .wrapping_add(width_right)
        .wrapping_add(width_list)
        .wrapping_add(width_after)
        > available
    {
        if width_centre > 0 as u_int {
            width_centre = width_centre.wrapping_sub(1);
        } else if width_list > 0 as u_int {
            width_list = width_list.wrapping_sub(1);
        } else if width_right > 0 as u_int {
            width_right = width_right.wrapping_sub(1);
        } else if width_after > 0 as u_int {
            width_after = width_after.wrapping_sub(1);
        } else {
            width_left = width_left.wrapping_sub(1);
        }
    }
    if width_list == 0 as u_int {
        screen_write_start(&mut ctx, left);
        screen_write_fast_copy(
            &mut ctx,
            &*after,
            0 as u_int,
            0 as u_int,
            width_after,
            1 as u_int,
        );
        screen_write_stop(&mut ctx);
        format_draw_none(
            octx, available, ocx, ocy, left, centre, right, abs_centre, frs,
        );
        return;
    }
    format_draw_put(
        octx, ocx, ocy, left, LEFT, frs, 0 as u_int, 0 as u_int, width_left,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        right,
        RIGHT,
        frs,
        available.wrapping_sub(width_right),
        (*right).cx.wrapping_sub(width_right),
        width_right,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        after,
        AFTER,
        frs,
        width_left.wrapping_add(width_list),
        0 as u_int,
        width_after,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        centre,
        CENTRE,
        frs,
        width_left
            .wrapping_add(width_list)
            .wrapping_add(width_after)
            .wrapping_add(
                available
                    .wrapping_sub(width_right)
                    .wrapping_sub(
                        width_left
                            .wrapping_add(width_list)
                            .wrapping_add(width_after),
                    )
                    .wrapping_div(2 as u_int),
            )
            .wrapping_sub(width_centre.wrapping_div(2 as u_int)),
        (*centre)
            .cx
            .wrapping_div(2 as u_int)
            .wrapping_sub(width_centre.wrapping_div(2 as u_int)),
        width_centre,
    );
    if focus_start == -(1 as ::core::ffi::c_int) || focus_end == -(1 as ::core::ffi::c_int) {
        focus_end = 0 as ::core::ffi::c_int;
        focus_start = focus_end;
    }
    format_draw_put_list(
        octx,
        ocx,
        ocy,
        width_left,
        width_list,
        list,
        list_left,
        list_right,
        focus_start,
        focus_end,
        frs,
    );
    if width_abs_centre > available {
        width_abs_centre = available;
    }
    format_draw_put(
        octx,
        ocx,
        ocy,
        abs_centre,
        ABSOLUTE_CENTRE,
        frs,
        available
            .wrapping_sub(width_abs_centre)
            .wrapping_div(2 as u_int),
        0 as u_int,
        width_abs_centre,
    );
}
unsafe fn format_draw_centre(
    mut octx: *mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    mut left: *mut screen,
    mut centre: *mut screen,
    mut right: *mut screen,
    mut abs_centre: *mut screen,
    mut list: *mut screen,
    mut list_left: *mut screen,
    mut list_right: *mut screen,
    mut after: *mut screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    mut frs: *mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut middle: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut width_abs_centre: u_int = 0;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    width_left = (*left).cx;
    width_centre = (*centre).cx;
    width_right = (*right).cx;
    width_abs_centre = (*abs_centre).cx;
    width_list = (*list).cx;
    width_after = (*after).cx;
    while width_left
        .wrapping_add(width_centre)
        .wrapping_add(width_right)
        .wrapping_add(width_list)
        .wrapping_add(width_after)
        > available
    {
        if width_list > 0 as u_int {
            width_list = width_list.wrapping_sub(1);
        } else if width_after > 0 as u_int {
            width_after = width_after.wrapping_sub(1);
        } else if width_centre > 0 as u_int {
            width_centre = width_centre.wrapping_sub(1);
        } else if width_right > 0 as u_int {
            width_right = width_right.wrapping_sub(1);
        } else {
            width_left = width_left.wrapping_sub(1);
        }
    }
    if width_list == 0 as u_int {
        screen_write_start(&mut ctx, centre);
        screen_write_fast_copy(
            &mut ctx,
            &*after,
            0 as u_int,
            0 as u_int,
            width_after,
            1 as u_int,
        );
        screen_write_stop(&mut ctx);
        format_draw_none(
            octx, available, ocx, ocy, left, centre, right, abs_centre, frs,
        );
        return;
    }
    format_draw_put(
        octx, ocx, ocy, left, LEFT, frs, 0 as u_int, 0 as u_int, width_left,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        right,
        RIGHT,
        frs,
        available.wrapping_sub(width_right),
        (*right).cx.wrapping_sub(width_right),
        width_right,
    );
    middle = width_left.wrapping_add(
        available
            .wrapping_sub(width_right)
            .wrapping_sub(width_left)
            .wrapping_div(2 as u_int),
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        centre,
        CENTRE,
        frs,
        middle
            .wrapping_sub(width_list.wrapping_div(2 as u_int))
            .wrapping_sub(width_centre),
        0 as u_int,
        width_centre,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        after,
        AFTER,
        frs,
        middle
            .wrapping_sub(width_list.wrapping_div(2 as u_int))
            .wrapping_add(width_list),
        0 as u_int,
        width_after,
    );
    if focus_start == -(1 as ::core::ffi::c_int) || focus_end == -(1 as ::core::ffi::c_int) {
        focus_end = (*list).cx.wrapping_div(2 as u_int) as ::core::ffi::c_int;
        focus_start = focus_end;
    }
    format_draw_put_list(
        octx,
        ocx,
        ocy,
        middle.wrapping_sub(width_list.wrapping_div(2 as u_int)),
        width_list,
        list,
        list_left,
        list_right,
        focus_start,
        focus_end,
        frs,
    );
    if width_abs_centre > available {
        width_abs_centre = available;
    }
    format_draw_put(
        octx,
        ocx,
        ocy,
        abs_centre,
        ABSOLUTE_CENTRE,
        frs,
        available
            .wrapping_sub(width_abs_centre)
            .wrapping_div(2 as u_int),
        0 as u_int,
        width_abs_centre,
    );
}
unsafe fn format_draw_right(
    mut octx: *mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    mut left: *mut screen,
    mut centre: *mut screen,
    mut right: *mut screen,
    mut abs_centre: *mut screen,
    mut list: *mut screen,
    mut list_left: *mut screen,
    mut list_right: *mut screen,
    mut after: *mut screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    mut frs: *mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut width_abs_centre: u_int = 0;
    let mut ctx: screen_write_ctx = screen_write_ctx {
        wp: std::rc::Weak::new(),
        target: Default::default(),
        flags: 0,
        init_ctx_cb: None,
        item: None,
        scrolled: 0,
        bg: 0,
    };
    width_left = (*left).cx;
    width_centre = (*centre).cx;
    width_right = (*right).cx;
    width_abs_centre = (*abs_centre).cx;
    width_list = (*list).cx;
    width_after = (*after).cx;
    while width_left
        .wrapping_add(width_centre)
        .wrapping_add(width_right)
        .wrapping_add(width_list)
        .wrapping_add(width_after)
        > available
    {
        if width_centre > 0 as u_int {
            width_centre = width_centre.wrapping_sub(1);
        } else if width_list > 0 as u_int {
            width_list = width_list.wrapping_sub(1);
        } else if width_right > 0 as u_int {
            width_right = width_right.wrapping_sub(1);
        } else if width_after > 0 as u_int {
            width_after = width_after.wrapping_sub(1);
        } else {
            width_left = width_left.wrapping_sub(1);
        }
    }
    if width_list == 0 as u_int {
        screen_write_start(&mut ctx, right);
        screen_write_fast_copy(
            &mut ctx,
            &*after,
            0 as u_int,
            0 as u_int,
            width_after,
            1 as u_int,
        );
        screen_write_stop(&mut ctx);
        format_draw_none(
            octx, available, ocx, ocy, left, centre, right, abs_centre, frs,
        );
        return;
    }
    format_draw_put(
        octx, ocx, ocy, left, LEFT, frs, 0 as u_int, 0 as u_int, width_left,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        after,
        AFTER,
        frs,
        available.wrapping_sub(width_after),
        (*after).cx.wrapping_sub(width_after),
        width_after,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        right,
        RIGHT,
        frs,
        available
            .wrapping_sub(width_right)
            .wrapping_sub(width_list)
            .wrapping_sub(width_after),
        0 as u_int,
        width_right,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        centre,
        CENTRE,
        frs,
        width_left
            .wrapping_add(
                available
                    .wrapping_sub(width_right)
                    .wrapping_sub(width_list)
                    .wrapping_sub(width_after)
                    .wrapping_sub(width_left)
                    .wrapping_div(2 as u_int),
            )
            .wrapping_sub(width_centre.wrapping_div(2 as u_int)),
        (*centre)
            .cx
            .wrapping_div(2 as u_int)
            .wrapping_sub(width_centre.wrapping_div(2 as u_int)),
        width_centre,
    );
    if focus_start == -(1 as ::core::ffi::c_int) || focus_end == -(1 as ::core::ffi::c_int) {
        focus_end = 0 as ::core::ffi::c_int;
        focus_start = focus_end;
    }
    format_draw_put_list(
        octx,
        ocx,
        ocy,
        available.wrapping_sub(width_list).wrapping_sub(width_after),
        width_list,
        list,
        list_left,
        list_right,
        focus_start,
        focus_end,
        frs,
    );
    if width_abs_centre > available {
        width_abs_centre = available;
    }
    format_draw_put(
        octx,
        ocx,
        ocy,
        abs_centre,
        ABSOLUTE_CENTRE,
        frs,
        available
            .wrapping_sub(width_abs_centre)
            .wrapping_div(2 as u_int),
        0 as u_int,
        width_abs_centre,
    );
}
unsafe fn format_draw_absolute_centre(
    mut octx: *mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    mut left: *mut screen,
    mut centre: *mut screen,
    mut right: *mut screen,
    mut abs_centre: *mut screen,
    mut list: *mut screen,
    mut list_left: *mut screen,
    mut list_right: *mut screen,
    mut after: *mut screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    mut frs: *mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut width_abs_centre: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut middle: u_int = 0;
    let mut abs_centre_offset: u_int = 0;
    width_left = (*left).cx;
    width_centre = (*centre).cx;
    width_right = (*right).cx;
    width_abs_centre = (*abs_centre).cx;
    width_list = (*list).cx;
    width_after = (*after).cx;
    while width_left
        .wrapping_add(width_centre)
        .wrapping_add(width_right)
        > available
    {
        if width_centre > 0 as u_int {
            width_centre = width_centre.wrapping_sub(1);
        } else if width_right > 0 as u_int {
            width_right = width_right.wrapping_sub(1);
        } else {
            width_left = width_left.wrapping_sub(1);
        }
    }
    while width_list
        .wrapping_add(width_after)
        .wrapping_add(width_abs_centre)
        > available
    {
        if width_list > 0 as u_int {
            width_list = width_list.wrapping_sub(1);
        } else if width_after > 0 as u_int {
            width_after = width_after.wrapping_sub(1);
        } else {
            width_abs_centre = width_abs_centre.wrapping_sub(1);
        }
    }
    format_draw_put(
        octx, ocx, ocy, left, LEFT, frs, 0 as u_int, 0 as u_int, width_left,
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        right,
        RIGHT,
        frs,
        available.wrapping_sub(width_right),
        (*right).cx.wrapping_sub(width_right),
        width_right,
    );
    middle = width_left.wrapping_add(
        available
            .wrapping_sub(width_right)
            .wrapping_sub(width_left)
            .wrapping_div(2 as u_int),
    );
    format_draw_put(
        octx,
        ocx,
        ocy,
        centre,
        CENTRE,
        frs,
        middle.wrapping_sub(width_centre),
        0 as u_int,
        width_centre,
    );
    if focus_start == -(1 as ::core::ffi::c_int) || focus_end == -(1 as ::core::ffi::c_int) {
        focus_end = (*list).cx.wrapping_div(2 as u_int) as ::core::ffi::c_int;
        focus_start = focus_end;
    }
    abs_centre_offset = available
        .wrapping_sub(width_list)
        .wrapping_sub(width_abs_centre)
        .wrapping_div(2 as u_int);
    format_draw_put(
        octx,
        ocx,
        ocy,
        abs_centre,
        ABSOLUTE_CENTRE,
        frs,
        abs_centre_offset,
        0 as u_int,
        width_abs_centre,
    );
    abs_centre_offset = abs_centre_offset.wrapping_add(width_abs_centre);
    format_draw_put_list(
        octx,
        ocx,
        ocy,
        abs_centre_offset,
        width_list,
        list,
        list_left,
        list_right,
        focus_start,
        focus_end,
        frs,
    );
    abs_centre_offset = abs_centre_offset.wrapping_add(width_list);
    format_draw_put(
        octx,
        ocx,
        ocy,
        after,
        AFTER,
        frs,
        abs_centre_offset,
        0 as u_int,
        width_after,
    );
}
unsafe fn format_leading_hashes(
    mut cp: *const ::core::ffi::c_char,
    mut n: *mut u_int,
    mut width: *mut u_int,
) -> *const ::core::ffi::c_char {
    *n = 0 as u_int;
    while *cp.offset(*n as isize) as ::core::ffi::c_int == '#' as i32 {
        *n = (*n).wrapping_add(1);
    }
    if *n == 0 as u_int {
        *width = 0 as u_int;
        return cp;
    }
    if *cp.offset(*n as isize) as ::core::ffi::c_int != '[' as i32 {
        if (*n).wrapping_rem(2 as u_int) == 0 as u_int {
            *width = (*n).wrapping_div(2 as u_int);
        } else {
            *width = (*n).wrapping_div(2 as u_int).wrapping_add(1 as u_int);
        }
        return cp.offset(*n as isize);
    }
    *width = (*n).wrapping_div(2 as u_int);
    if (*n).wrapping_rem(2 as u_int) == 0 as u_int {
        return cp.offset(*n as isize);
    }
    cp.offset(*n as isize)
        .offset(-(1 as ::core::ffi::c_int as isize))
}
unsafe fn format_draw_many(mut ctx: *mut screen_write_ctx, mut sy: *mut style, mut n: u_int) {
    let mut ch: ::core::ffi::c_char = '#' as i32 as ::core::ffi::c_char;
    let mut i: u_int = 0;
    utf8_set(&mut (*sy).gc.data, ch as u_char);
    i = 0 as u_int;
    while i < n {
        screen_write_cell(&mut *ctx, &(*sy).gc);
        i = i.wrapping_add(1);
    }
}
pub unsafe fn format_draw(
    mut octx: *mut screen_write_ctx,
    base: &grid_cell,
    mut available: u_int,
    mut expanded: *const ::core::ffi::c_char,
    mut srs: *mut style_ranges,
    mut default_colours: ::core::ffi::c_int,
) {
    let mut current_block: u64;
    let mut current: C2RustUnnamed_39 = LEFT;
    let mut last: C2RustUnnamed_39 = LEFT;
    let names: [&'static CStr; 8] = [
        c"LEFT",
        c"CENTRE",
        c"RIGHT",
        c"ABSOLUTE_CENTRE",
        c"LIST",
        c"LIST_LEFT",
        c"LIST_RIGHT",
        c"AFTER",
    ];
    let mut size: size_t = strlen(expanded);
    let mut os: *mut screen = (*octx).screen_ptr();
    let mut s: [screen; 8] = std::array::from_fn(|_| screen::empty());
    let hl = (*os).hyperlinks.clone();
    let mut ctx: [screen_write_ctx; 8] = [const {
        screen_write_ctx {
            wp: std::rc::Weak::new(),
            target: crate::src::shared::screen_write::ScreenWriteTarget::none(),
            flags: 0,
            init_ctx_cb: None,
            item: None,
            scrolled: 0,
            bg: 0,
        }
    }; 8];
    let mut ocx: u_int = (*os).cx;
    let mut ocy: u_int = (*os).cy;
    let mut n: u_int = 0;
    let mut i: u_int = 0;
    let mut width: [u_int; 8] = [0; 8];
    let mut map: [u_int; 5] = [
        LEFT as ::core::ffi::c_int as u_int,
        LEFT as ::core::ffi::c_int as u_int,
        CENTRE as ::core::ffi::c_int as u_int,
        RIGHT as ::core::ffi::c_int as u_int,
        ABSOLUTE_CENTRE as ::core::ffi::c_int as u_int,
    ];
    let mut focus_start: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut focus_end: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut list_state: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut fill: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut even: ::core::ffi::c_int = 0;
    let mut list_align: style_align = STYLE_ALIGN_DEFAULT;
    let mut gc: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut current_default: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut base_default: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut sy: style = style {
        gc: grid_cell {
            data: utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            },
            attr: 0,
            flags: 0,
            fg: 0,
            bg: 0,
            us: 0,
            link: 0,
        },
        ignore: 0,
        dim: 0,
        fill: 0,
        align: STYLE_ALIGN_DEFAULT,
        list: STYLE_LIST_OFF,
        range_type: STYLE_RANGE_NONE,
        range_argument: 0,
        range_string: [0; 16],
        width: 0,
        width_percentage: 0,
        pad: 0,
        default_type: STYLE_DEFAULT_BASE,
        link: 0,
    };
    let mut saved_sy: style = style {
        gc: grid_cell {
            data: utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            },
            attr: 0,
            flags: 0,
            fg: 0,
            bg: 0,
            us: 0,
            link: 0,
        },
        ignore: 0,
        dim: 0,
        fill: 0,
        align: STYLE_ALIGN_DEFAULT,
        list: STYLE_LIST_OFF,
        range_type: STYLE_RANGE_NONE,
        range_argument: 0,
        range_string: [0; 16],
        width: 0,
        width_percentage: 0,
        pad: 0,
        default_type: STYLE_DEFAULT_BASE,
        link: 0,
    };
    let mut ud: *mut utf8_data = &raw mut sy.gc.data;
    let mut cp: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut more: utf8_state = UTF8_MORE;
    let mut fr: Option<format_range> = None;
    let mut frs: format_ranges = Vec::new();
    base_default = *base;
    current_default = *base;
    style_set(&raw mut sy, &current_default);
    log_debug(format_args!(
        "{}: {}",
        "format_draw",
        log_cstr(CStr::from_ptr(expanded))
    ));
    i = 0 as u_int;
    while i < TOTAL as ::core::ffi::c_int as u_int {
        screen_init(&mut s[i as usize], size as u_int, 1 as u_int, 0 as u_int);
        screen_write_start(
            &mut ctx[i as usize],
            (&raw mut s as *mut screen).offset(i as isize),
        );
        screen_write_clearendofline(&mut ctx[i as usize], current_default.bg as u_int);
        width[i as usize] = 0 as u_int;
        i = i.wrapping_add(1);
    }
    cp = expanded;
    loop {
        if !(*cp as ::core::ffi::c_int != '\0' as i32) {
            current_block = 1830138855519935310;
            break;
        }
        if *cp.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '#' as i32
            && *cp.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32
            && *cp.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\0' as i32
        {
            n = 1 as u_int;
            while *cp.offset(n as isize) as ::core::ffi::c_int == '#' as i32 {
                n = n.wrapping_add(1);
            }
            even = (n.wrapping_rem(2 as u_int) == 0 as u_int) as ::core::ffi::c_int;
            if *cp.offset(n as isize) as ::core::ffi::c_int != '[' as i32 {
                cp = cp.offset(n as isize);
                if even != 0 {
                    n = n.wrapping_div(2 as u_int);
                } else {
                    n = n.wrapping_div(2 as u_int).wrapping_add(1 as u_int);
                }
                width[current as usize] = width[current as usize].wrapping_add(n);
                format_draw_many(
                    (&raw mut ctx as *mut screen_write_ctx).offset(current as isize),
                    &raw mut sy,
                    n,
                );
            } else {
                if even != 0 {
                    cp = cp.offset(n.wrapping_add(1 as u_int) as isize);
                } else {
                    cp = cp.offset(n.wrapping_sub(1 as u_int) as isize);
                }
                if sy.ignore != 0 {
                    continue;
                }
                format_draw_many(
                    (&raw mut ctx as *mut screen_write_ctx).offset(current as isize),
                    &raw mut sy,
                    n.wrapping_div(2 as u_int),
                );
                width[current as usize] =
                    width[current as usize].wrapping_add(n.wrapping_div(2 as u_int));
                if even != 0 {
                    utf8_set(&mut *ud, '[' as i32 as u_char);
                    screen_write_cell(&mut ctx[current as usize], &sy.gc);
                    width[current as usize] = width[current as usize].wrapping_add(1);
                }
            }
        } else if *cp.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '#' as i32
            || *cp.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '[' as i32
            || sy.ignore != 0
        {
            more = utf8_open(&mut *ud, *cp as u_char);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    cp = cp.offset(1);
                    if !(*cp as ::core::ffi::c_int != '\0' as i32
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut *ud, *cp as u_char);
                }
                if more as ::core::ffi::c_uint
                    != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    cp = cp.offset(-((*ud).have as ::core::ffi::c_int as isize));
                }
            }
            if more as ::core::ffi::c_uint != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                if (*cp as ::core::ffi::c_int) < 0x20 as ::core::ffi::c_int
                    || *cp as ::core::ffi::c_int > 0x7e as ::core::ffi::c_int
                {
                    cp = cp.offset(1);
                    continue;
                } else {
                    utf8_set(&mut *ud, *cp as u_char);
                    cp = cp.offset(1);
                }
            }
            screen_write_cell(&mut ctx[current as usize], &sy.gc);
            width[current as usize] = width[current as usize].wrapping_add((*ud).width as u_int);
        } else {
            end = format_skip(cp.offset(2 as ::core::ffi::c_int as isize));
            if end.is_null() {
                log_debug(format_args!(
                    "{}: no terminating ] at '{}'",
                    "format_draw",
                    log_cstr(CStr::from_ptr(cp.offset(2 as ::core::ffi::c_int as isize)))
                ));
                frs.clear();
                i = 0 as u_int;
                while i < TOTAL as ::core::ffi::c_int as u_int {
                    screen_write_stop(&mut ctx[i as usize]);
                    i = i.wrapping_add(1);
                }
                current_block = 4329038292887906754;
                break;
            } else {
                let style_start = cp.offset(2);
                let style_len = end.offset_from(style_start) as usize;
                let style_text = CString::new(::core::slice::from_raw_parts(
                    style_start as *const u8,
                    style_len,
                ))
                .expect("format style contains an interior NUL");
                style_copy(&raw mut saved_sy, &raw mut sy);
                if style_parse(&raw mut sy, &current_default, &style_text)
                    != 0 as ::core::ffi::c_int
                {
                    log_debug(format_args!(
                        "{}: invalid style '{}'",
                        "format_draw",
                        crate::src::log::log_bytes(style_text.as_bytes())
                    ));
                    drop(style_text);
                    cp = end.offset(1 as ::core::ffi::c_int as isize);
                } else {
                    log_debug(format_args!(
                        "{}: style '{}' -> '{}'",
                        "format_draw",
                        crate::src::log::log_bytes(style_text.as_bytes()),
                        log_cstr(&style_tostring(&raw mut sy))
                    ));
                    drop(style_text);
                    if default_colours != 0 {
                        sy.gc.bg = base_default.bg;
                        sy.gc.fg = base_default.fg;
                    }
                    sy.gc.link = match (style_link(&sy), hl.as_ref()) {
                        (Some(link), Some(table)) => {
                            hyperlinks_put(table, &link.uri, Some(&link.uri))
                        }
                        _ => 0,
                    };
                    if sy.fill != 8 as ::core::ffi::c_int {
                        fill = sy.fill;
                    }
                    if sy.default_type as ::core::ffi::c_uint
                        == STYLE_DEFAULT_PUSH as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        current_default = saved_sy.gc;
                        sy.default_type = STYLE_DEFAULT_BASE;
                    } else if sy.default_type as ::core::ffi::c_uint
                        == STYLE_DEFAULT_POP as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        current_default = base_default;
                        sy.default_type = STYLE_DEFAULT_BASE;
                    } else if sy.default_type as ::core::ffi::c_uint
                        == STYLE_DEFAULT_SET as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        base_default = saved_sy.gc;
                        current_default = saved_sy.gc;
                        sy.default_type = STYLE_DEFAULT_BASE;
                    }
                    match sy.list as ::core::ffi::c_uint {
                        1 => {
                            if list_state != 0 as ::core::ffi::c_int {
                                fr = None;
                                list_state = 0 as ::core::ffi::c_int;
                                list_align = sy.align;
                            }
                            if focus_start != -(1 as ::core::ffi::c_int)
                                && focus_end == -(1 as ::core::ffi::c_int)
                            {
                                focus_end =
                                    s[LIST as ::core::ffi::c_int as usize].cx as ::core::ffi::c_int;
                            }
                            current = LIST;
                        }
                        2 => {
                            if !(list_state != 0 as ::core::ffi::c_int)
                                && focus_start == -(1 as ::core::ffi::c_int)
                            {
                                focus_start =
                                    s[LIST as ::core::ffi::c_int as usize].cx as ::core::ffi::c_int;
                            }
                        }
                        0 => {
                            if list_state == 0 as ::core::ffi::c_int {
                                fr = None;
                                if focus_start != -(1 as ::core::ffi::c_int)
                                    && focus_end == -(1 as ::core::ffi::c_int)
                                {
                                    focus_end = s[LIST as ::core::ffi::c_int as usize].cx
                                        as ::core::ffi::c_int;
                                }
                                map[list_align as usize] = AFTER as ::core::ffi::c_int as u_int;
                                if list_align as ::core::ffi::c_uint
                                    == STYLE_ALIGN_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
                                {
                                    map[STYLE_ALIGN_DEFAULT as ::core::ffi::c_int as usize] =
                                        AFTER as ::core::ffi::c_int as u_int;
                                }
                                list_state = 1 as ::core::ffi::c_int;
                            }
                            current = map[sy.align as usize] as C2RustUnnamed_39;
                        }
                        3 => {
                            if !(list_state != 0 as ::core::ffi::c_int)
                                && !(s[LIST_LEFT as ::core::ffi::c_int as usize].cx != 0 as u_int)
                            {
                                fr = None;
                                if focus_start != -(1 as ::core::ffi::c_int)
                                    && focus_end == -(1 as ::core::ffi::c_int)
                                {
                                    focus_end = -(1 as ::core::ffi::c_int);
                                    focus_start = focus_end;
                                }
                                current = LIST_LEFT;
                            }
                        }
                        4 if !(list_state != 0 as ::core::ffi::c_int)
                            && !(s[LIST_RIGHT as ::core::ffi::c_int as usize].cx != 0 as u_int) =>
                        {
                            fr = None;
                            if focus_start != -(1 as ::core::ffi::c_int)
                                && focus_end == -(1 as ::core::ffi::c_int)
                            {
                                focus_end = -(1 as ::core::ffi::c_int);
                                focus_start = focus_end;
                            }
                            current = LIST_RIGHT;
                        }
                        _ => {}
                    }
                    if current as ::core::ffi::c_uint != last as ::core::ffi::c_uint {
                        log_debug(format_args!(
                            "{}: change {} -> {}",
                            "format_draw",
                            log_cstr(names[last as usize]),
                            log_cstr(names[current as usize])
                        ));
                        last = current;
                    }
                    if !srs.is_null() {
                        if fr
                            .as_mut()
                            .is_some_and(|pending| format_is_type(pending, &raw mut sy) == 0)
                        {
                            let mut finished = fr.take().unwrap();
                            if s[current as usize].cx != finished.start {
                                finished.end = s[current as usize].cx;
                                frs.push(finished);
                            }
                        }
                        if fr.is_none()
                            && sy.range_type as ::core::ffi::c_uint
                                != STYLE_RANGE_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
                        {
                            let mut pending = format_range {
                                index: current as u_int,
                                start: s[current as usize].cx,
                                end: 0,
                                type_0: sy.range_type,
                                argument: sy.range_argument,
                                string: [0; 16],
                            };
                            strlcpy(
                                &raw mut pending.string as *mut ::core::ffi::c_char,
                                &raw mut sy.range_string as *mut ::core::ffi::c_char,
                                ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t,
                            );
                            fr = Some(pending);
                        }
                    }
                    cp = end.offset(1 as ::core::ffi::c_int as isize);
                }
            }
        }
    }
    if current_block == 1830138855519935310 {
        fr = None;
        i = 0 as u_int;
        while i < TOTAL as ::core::ffi::c_int as u_int {
            screen_write_stop(&mut ctx[i as usize]);
            log_debug(format_args!(
                "{}: width {} is {}",
                "format_draw",
                log_cstr(names[i as usize]),
                (width[i as usize]) as u32
            ));
            i = i.wrapping_add(1);
        }
        if focus_start != -(1 as ::core::ffi::c_int) && focus_end != -(1 as ::core::ffi::c_int) {
            log_debug(format_args!(
                "{}: focus {}-{}",
                "format_draw",
                (focus_start) as i32,
                (focus_end) as i32
            ));
        }
        for fr in &frs {
            log_debug(format_args!(
                "{}: range {}|{} is {} {}-{}",
                "format_draw",
                (fr.type_0 as ::core::ffi::c_uint) as i32,
                { fr.argument },
                log_cstr(names[fr.index as usize]),
                { fr.start },
                { fr.end }
            ));
        }
        if fill != -(1 as ::core::ffi::c_int) {
            gc = grid_default_cell;
            gc.bg = fill;
            i = 0 as u_int;
            while i < available {
                screen_write_putc(&mut *octx, &gc, ' ' as i32 as u_char);
                i = i.wrapping_add(1);
            }
        }
        match list_align as ::core::ffi::c_uint {
            0 => {
                format_draw_none(
                    octx,
                    available,
                    ocx,
                    ocy,
                    (&raw mut s as *mut screen).offset(LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen)
                        .offset(ABSOLUTE_CENTRE as ::core::ffi::c_int as isize),
                    &raw mut frs,
                );
            }
            1 => {
                format_draw_left(
                    octx,
                    available,
                    ocx,
                    ocy,
                    (&raw mut s as *mut screen).offset(LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen)
                        .offset(ABSOLUTE_CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(AFTER as ::core::ffi::c_int as isize),
                    focus_start,
                    focus_end,
                    &raw mut frs,
                );
            }
            2 => {
                format_draw_centre(
                    octx,
                    available,
                    ocx,
                    ocy,
                    (&raw mut s as *mut screen).offset(LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen)
                        .offset(ABSOLUTE_CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(AFTER as ::core::ffi::c_int as isize),
                    focus_start,
                    focus_end,
                    &raw mut frs,
                );
            }
            3 => {
                format_draw_right(
                    octx,
                    available,
                    ocx,
                    ocy,
                    (&raw mut s as *mut screen).offset(LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen)
                        .offset(ABSOLUTE_CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(AFTER as ::core::ffi::c_int as isize),
                    focus_start,
                    focus_end,
                    &raw mut frs,
                );
            }
            4 => {
                format_draw_absolute_centre(
                    octx,
                    available,
                    ocx,
                    ocy,
                    (&raw mut s as *mut screen).offset(LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen)
                        .offset(ABSOLUTE_CENTRE as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_LEFT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(LIST_RIGHT as ::core::ffi::c_int as isize),
                    (&raw mut s as *mut screen).offset(AFTER as ::core::ffi::c_int as isize),
                    focus_start,
                    focus_end,
                    &raw mut frs,
                );
            }
            _ => {}
        }
        for range in &frs {
            let mut owned = Box::new(style_range {
                type_0: range.type_0,
                argument: range.argument,
                string: [0; 16],
                start: range.start,
                end: range.end,
            });
            strlcpy(
                &raw mut owned.string as *mut ::core::ffi::c_char,
                range.string.as_ptr(),
                ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t,
            );
            match owned.type_0 as ::core::ffi::c_uint {
                1 => {
                    log_debug(format_args!(
                        "{}: range left at {}-{}",
                        "format_draw",
                        { owned.start },
                        { owned.end }
                    ));
                }
                2 => {
                    log_debug(format_args!(
                        "{}: range right at {}-{}",
                        "format_draw",
                        { owned.start },
                        { owned.end }
                    ));
                }
                3 => {
                    log_debug(format_args!(
                        "{}: range pane|%{} at {}-{}",
                        "format_draw",
                        { owned.argument },
                        { owned.start },
                        { owned.end }
                    ));
                }
                4 => {
                    log_debug(format_args!(
                        "{}: range window|{} at {}-{}",
                        "format_draw",
                        { owned.argument },
                        { owned.start },
                        { owned.end }
                    ));
                }
                5 => {
                    log_debug(format_args!(
                        "{}: range session|${} at {}-{}",
                        "format_draw",
                        { owned.argument },
                        { owned.start },
                        { owned.end }
                    ));
                }
                6 => {
                    log_debug(format_args!(
                        "{}: range user|{} at {}-{}",
                        "format_draw",
                        { owned.argument },
                        { owned.start },
                        { owned.end }
                    ));
                }
                7 => {
                    log_debug(format_args!(
                        "{}: range control|{} at {}-{}",
                        "format_draw",
                        { owned.argument },
                        { owned.start },
                        { owned.end }
                    ));
                }
                _ => {}
            }
            if !srs.is_null() {
                (*srs).push(owned);
            }
        }
    }
    i = 0 as u_int;
    while i < TOTAL as ::core::ffi::c_int as u_int {
        drop(std::mem::take(&mut s[i as usize]));
        i = i.wrapping_add(1);
    }
    screen_write_cursormove(
        &mut *octx,
        ocx as ::core::ffi::c_int,
        ocy as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
}
pub unsafe fn format_width(mut expanded: *const ::core::ffi::c_char) -> u_int {
    let mut cp: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut n: u_int = 0;
    let mut leading_width: u_int = 0;
    let mut width: u_int = 0 as u_int;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    cp = expanded;
    while *cp as ::core::ffi::c_int != '\0' as i32 {
        if *cp as ::core::ffi::c_int == '#' as i32 {
            end = format_leading_hashes(cp, &raw mut n, &raw mut leading_width);
            width = width.wrapping_add(leading_width);
            cp = end;
            if *cp as ::core::ffi::c_int == '#' as i32 {
                end = format_skip(cp.offset(2 as ::core::ffi::c_int as isize));
                if end.is_null() {
                    return 0 as u_int;
                }
                cp = end.offset(1 as ::core::ffi::c_int as isize);
            }
        } else {
            more = utf8_open(&mut ud, *cp as u_char);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    cp = cp.offset(1);
                    if !(*cp as ::core::ffi::c_int != '\0' as i32
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut ud, *cp as u_char);
                }
                if more as ::core::ffi::c_uint
                    == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    width = width.wrapping_add(ud.width as u_int);
                }
            } else if *cp as ::core::ffi::c_int > 0x1f as ::core::ffi::c_int
                && (*cp as ::core::ffi::c_int) < 0x7f as ::core::ffi::c_int
            {
                width = width.wrapping_add(1);
                cp = cp.offset(1);
            } else {
                cp = cp.offset(1);
            }
        }
    }
    width
}
pub(crate) unsafe fn format_trim_left_bytes(expanded: &CStr, mut limit: u_int) -> Vec<u8> {
    let mut out = Vec::<u8>::new();
    let mut cp: *const ::core::ffi::c_char = expanded.as_ptr();
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut n: u_int = 0;
    let mut width: u_int = 0 as u_int;
    let mut leading_width: u_int = 0;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    while *cp as ::core::ffi::c_int != '\0' as i32 {
        if width >= limit {
            break;
        }
        if *cp as ::core::ffi::c_int == '#' as i32 {
            end = format_leading_hashes(cp, &raw mut n, &raw mut leading_width);
            if leading_width > limit.wrapping_sub(width) {
                leading_width = limit.wrapping_sub(width);
            }
            if leading_width != 0 as u_int {
                if n == 1 as u_int {
                    out.push(b'#');
                } else {
                    out.resize(out.len() + 2 * leading_width as usize, b'#');
                }
                width = width.wrapping_add(leading_width);
            }
            cp = end;
            if !(*cp as ::core::ffi::c_int == '#' as i32) {
                continue;
            }
            end = format_skip(cp.offset(2 as ::core::ffi::c_int as isize));
            if end.is_null() {
                break;
            }
            let span = end.offset(1).offset_from(cp) as usize;
            out.extend_from_slice(::core::slice::from_raw_parts(cp.cast::<u8>(), span));
            cp = end.offset(1 as ::core::ffi::c_int as isize);
        } else {
            more = utf8_open(&mut ud, *cp as u_char);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    cp = cp.offset(1);
                    if !(*cp as ::core::ffi::c_int != '\0' as i32
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut ud, *cp as u_char);
                }
                if more as ::core::ffi::c_uint
                    == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    if width.wrapping_add(ud.width as u_int) <= limit {
                        out.extend_from_slice(&ud.data[..ud.size as usize]);
                    }
                    width = width.wrapping_add(ud.width as u_int);
                } else {
                    cp = cp.offset(-(ud.have as ::core::ffi::c_int as isize));
                    cp = cp.offset(1);
                }
            } else if *cp as ::core::ffi::c_int > 0x1f as ::core::ffi::c_int
                && (*cp as ::core::ffi::c_int) < 0x7f as ::core::ffi::c_int
            {
                if width.wrapping_add(1 as u_int) <= limit {
                    out.push(*cp as u8);
                }
                width = width.wrapping_add(1);
                cp = cp.offset(1);
            } else {
                cp = cp.offset(1);
            }
        }
    }
    out
}
pub(crate) unsafe fn format_trim_right_bytes(expanded: &CStr, mut limit: u_int) -> Vec<u8> {
    let mut out = Vec::<u8>::new();
    let mut cp: *const ::core::ffi::c_char = expanded.as_ptr();
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut width: u_int = 0 as u_int;
    let mut total_width: u_int = 0;
    let mut skip: u_int = 0;
    let mut n: u_int = 0;
    let mut leading_width: u_int = 0;
    let mut copy_width: u_int = 0;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    total_width = format_width(expanded.as_ptr());
    if total_width <= limit {
        return expanded.to_bytes().to_vec();
    }
    skip = total_width.wrapping_sub(limit);
    while *cp as ::core::ffi::c_int != '\0' as i32 {
        if *cp as ::core::ffi::c_int == '#' as i32 {
            end = format_leading_hashes(cp, &raw mut n, &raw mut leading_width);
            copy_width = leading_width;
            if width <= skip {
                if skip.wrapping_sub(width) >= copy_width {
                    copy_width = 0 as u_int;
                } else {
                    copy_width = copy_width.wrapping_sub(skip.wrapping_sub(width));
                }
            }
            if copy_width != 0 as u_int {
                if n == 1 as u_int {
                    out.push(b'#');
                } else {
                    out.resize(out.len() + 2 * copy_width as usize, b'#');
                }
            }
            width = width.wrapping_add(leading_width);
            cp = end;
            if !(*cp as ::core::ffi::c_int == '#' as i32) {
                continue;
            }
            end = format_skip(cp.offset(2 as ::core::ffi::c_int as isize));
            if end.is_null() {
                break;
            }
            let span = end.offset(1).offset_from(cp) as usize;
            out.extend_from_slice(::core::slice::from_raw_parts(cp.cast::<u8>(), span));
            cp = end.offset(1 as ::core::ffi::c_int as isize);
        } else {
            more = utf8_open(&mut ud, *cp as u_char);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    cp = cp.offset(1);
                    if !(*cp as ::core::ffi::c_int != '\0' as i32
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut ud, *cp as u_char);
                }
                if more as ::core::ffi::c_uint
                    == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    if width >= skip {
                        out.extend_from_slice(&ud.data[..ud.size as usize]);
                    }
                    width = width.wrapping_add(ud.width as u_int);
                } else {
                    cp = cp.offset(-(ud.have as ::core::ffi::c_int as isize));
                    cp = cp.offset(1);
                }
            } else if *cp as ::core::ffi::c_int > 0x1f as ::core::ffi::c_int
                && (*cp as ::core::ffi::c_int) < 0x7f as ::core::ffi::c_int
            {
                if width >= skip {
                    out.push(*cp as u8);
                }
                width = width.wrapping_add(1);
                cp = cp.offset(1);
            } else {
                cp = cp.offset(1);
            }
        }
    }
    out
}

#[cfg(test)]
mod range_tests {
    use super::*;
    use crate::src::options::{options_create_owned, options_default};
    use crate::src::options_table::options_table;
    use crate::src::tmux::global_options;

    #[test]
    fn rendered_ranges_follow_their_alignment_and_clipping() {
        unsafe {
            for (width, expected) in [
                (
                    20,
                    vec![("left", 0, 2), ("centre", 9, 11), ("right", 18, 20)],
                ),
                (3, vec![("left", 0, 2), ("right", 2, 3)]),
            ] {
                let saved_options = global_options;
                let mut options = options_create_owned(None);
                options_default(
                    &mut *options,
                    options_table
                        .iter()
                        .find(|entry| entry.name == Some(c"extended-keys"))
                        .unwrap(),
                );
                global_options = &mut *options;
                let mut output = screen::default();
                screen_init(&mut output, width, 1, 0);
                let mut ctx = screen_write_ctx::default();
                let mut ranges = style_ranges::default();
                screen_write_start(&mut ctx, &mut output);
                format_draw(
                    &mut ctx,
                    &grid_default_cell,
                    width,
                    c"#[align=left,range=user|left]LL#[norange]#[align=centre,range=user|centre]CC#[norange]#[align=right,range=user|right]RR#[norange]".as_ptr(),
                    &mut ranges,
                    0,
                );
                screen_write_stop(&mut ctx);
                let actual: Vec<_> = ranges
                    .iter()
                    .map(|range| {
                        (
                            CStr::from_ptr(range.string.as_ptr()).to_str().unwrap(),
                            range.start,
                            range.end,
                        )
                    })
                    .collect();
                drop(output);
                global_options = saved_options;
                assert_eq!(actual, expected, "available width {width}");
            }
        }
    }
}
