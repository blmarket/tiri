use crate::format::format_skip;
use crate::grid::grid_default_cell;
use crate::hyperlinks::hyperlinks_put;
use crate::log::{log_cstr, log_debug};
use crate::screen::screen_init;
use crate::screen_write::{
    screen_write_cell, screen_write_clearendofline, screen_write_cursormove,
    screen_write_fast_copy, screen_write_putc, screen_write_start, screen_write_stop,
};
use crate::shared::abi::*;
use crate::shared::grid::*;
use crate::shared::screen::screen;
use crate::shared::screen_write::screen_write_ctx;
use crate::shared::style::*;
use crate::shared::utf8::*;
use crate::style::{style_copy, style_link, style_parse, style_set, style_tostring};
use crate::text::utf8::{utf8_append, utf8_open, utf8_set};
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
    pub string: [u8; 16],
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
fn format_is_type(fr: &format_range, sy: &style) -> ::core::ffi::c_int {
    if fr.type_0 as ::core::ffi::c_uint != sy.range_type as ::core::ffi::c_uint {
        return 0 as ::core::ffi::c_int;
    }
    match fr.type_0 as ::core::ffi::c_uint {
        0 | 1 | 2 | 7 => return 1 as ::core::ffi::c_int,
        3..=5 => {
            return (fr.argument == sy.range_argument) as ::core::ffi::c_int;
        }
        6 => {
            let name = |s| CStr::from_bytes_until_nul(s).expect("range string is terminated");
            return (name(&fr.string) == name(&sy.range_string)) as ::core::ffi::c_int;
        }
        _ => {}
    }
    1 as ::core::ffi::c_int
}
fn format_update_ranges(
    frs: &mut format_ranges,
    index: u_int,
    mut offset: u_int,
    mut start: u_int,
    mut width: u_int,
) {
    frs.retain_mut(|fr| {
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
    mut octx: &mut screen_write_ctx,
    mut ocx: u_int,
    mut ocy: u_int,
    s: &screen,
    index: u_int,
    frs: &mut format_ranges,
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
    mut octx: &mut screen_write_ctx,
    mut ocx: u_int,
    mut ocy: u_int,
    mut offset: u_int,
    mut width: u_int,
    list: &screen,
    list_left: &screen,
    list_right: &screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    frs: &mut format_ranges,
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
    mut octx: &mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    left: &screen,
    centre: &screen,
    right: &screen,
    abs_centre: &screen,
    frs: &mut format_ranges,
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
    mut octx: &mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    left: &mut screen,
    centre: &screen,
    right: &screen,
    abs_centre: &screen,
    list: &screen,
    list_left: &screen,
    list_right: &screen,
    after: &screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    frs: &mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut width_abs_centre: u_int = 0;
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
        let mut ctx = screen_write_start(&mut *left);
        screen_write_fast_copy(
            &mut ctx,
            &*after,
            0 as u_int,
            0 as u_int,
            width_after,
            1 as u_int,
        );
        screen_write_stop(ctx);
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
    mut octx: &mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    left: &screen,
    centre: &mut screen,
    right: &screen,
    abs_centre: &screen,
    list: &screen,
    list_left: &screen,
    list_right: &screen,
    after: &screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    frs: &mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut middle: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut width_abs_centre: u_int = 0;
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
        let mut ctx = screen_write_start(&mut *centre);
        screen_write_fast_copy(
            &mut ctx,
            &*after,
            0 as u_int,
            0 as u_int,
            width_after,
            1 as u_int,
        );
        screen_write_stop(ctx);
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
    mut octx: &mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    left: &screen,
    centre: &screen,
    right: &mut screen,
    abs_centre: &screen,
    list: &screen,
    list_left: &screen,
    list_right: &screen,
    after: &screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    frs: &mut format_ranges,
) {
    let mut width_left: u_int = 0;
    let mut width_centre: u_int = 0;
    let mut width_right: u_int = 0;
    let mut width_list: u_int = 0;
    let mut width_after: u_int = 0;
    let mut width_abs_centre: u_int = 0;
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
        let mut ctx = screen_write_start(&mut *right);
        screen_write_fast_copy(
            &mut ctx,
            &*after,
            0 as u_int,
            0 as u_int,
            width_after,
            1 as u_int,
        );
        screen_write_stop(ctx);
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
    mut octx: &mut screen_write_ctx,
    mut available: u_int,
    mut ocx: u_int,
    mut ocy: u_int,
    left: &screen,
    centre: &screen,
    right: &screen,
    abs_centre: &screen,
    list: &screen,
    list_left: &screen,
    list_right: &screen,
    after: &screen,
    mut focus_start: ::core::ffi::c_int,
    mut focus_end: ::core::ffi::c_int,
    frs: &mut format_ranges,
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
/// Measure the run of `#` at the start of `s`: the bytes it consumes (an odd
/// run before `[` leaves the `#[` that opens a style), its length and its
/// drawn width.
fn format_leading_hashes(s: &[u8]) -> (usize, u_int, u_int) {
    let n = s.iter().take_while(|&&byte| byte == b'#').count();
    if n == 0 {
        return (0, 0, 0);
    }
    let count = n as u_int;
    if s.get(n) != Some(&b'[') {
        return (n, count, count.div_ceil(2));
    }
    (if n % 2 == 0 { n } else { n - 1 }, count, count / 2)
}
unsafe fn format_draw_many(mut ctx: &mut screen_write_ctx, sy: &mut style, mut n: u_int) {
    let mut i: u_int = 0;
    utf8_set(&mut sy.gc.data, b'#');
    i = 0 as u_int;
    while i < n {
        screen_write_cell(ctx, &sy.gc);
        i = i.wrapping_add(1);
    }
}
pub unsafe fn format_draw(
    mut octx: &mut screen_write_ctx,
    base: &grid_cell,
    mut available: u_int,
    expanded: &CStr,
    mut srs: Option<&mut style_ranges>,
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
    let bytes = expanded.to_bytes();
    let mut size: size_t = bytes.len();
    let mut s: [screen; 8] = std::array::from_fn(|_| screen::empty());
    let hl = octx.screen.hyperlinks.clone();
    let mut ocx: u_int = octx.screen.cx;
    let mut ocy: u_int = octx.screen.cy;
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
    let mut more: utf8_state = UTF8_MORE;
    let mut fr: Option<format_range> = None;
    let mut frs: format_ranges = Vec::new();
    base_default = *base;
    current_default = *base;
    style_set(&mut sy, &current_default);
    log_debug(format_args!(
        "{}: {}",
        "format_draw",
        crate::log::log_bytes(bytes)
    ));
    for part in &mut s {
        screen_init(part, size as u_int, 1 as u_int, 0 as u_int);
    }
    {
        // One write per part; all stop before the parts are laid out below.
        let mut ctx: [screen_write_ctx; 8] = s.each_mut().map(|part| screen_write_start(part));
        for part_ctx in &mut ctx {
            screen_write_clearendofline(part_ctx, current_default.bg as u_int);
        }
        let at = |i: usize| bytes.get(i).copied().unwrap_or(0);
        let mut cp = 0;
        loop {
            if cp >= bytes.len() {
                current_block = 1830138855519935310;
                break;
            }
            if at(cp) == b'#' && at(cp + 1) != b'[' && at(cp + 1) != 0 {
                n = 1 as u_int;
                while at(cp + n as usize) == b'#' {
                    n = n.wrapping_add(1);
                }
                even = (n.wrapping_rem(2 as u_int) == 0 as u_int) as ::core::ffi::c_int;
                if at(cp + n as usize) != b'[' {
                    cp += n as usize;
                    if even != 0 {
                        n = n.wrapping_div(2 as u_int);
                    } else {
                        n = n.wrapping_div(2 as u_int).wrapping_add(1 as u_int);
                    }
                    width[current as usize] = width[current as usize].wrapping_add(n);
                    format_draw_many(&mut ctx[current as usize], &mut sy, n);
                } else {
                    if even != 0 {
                        cp += n as usize + 1;
                    } else {
                        cp += n as usize - 1;
                    }
                    if sy.ignore != 0 {
                        continue;
                    }
                    format_draw_many(
                        &mut ctx[current as usize],
                        &mut sy,
                        n.wrapping_div(2 as u_int),
                    );
                    width[current as usize] =
                        width[current as usize].wrapping_add(n.wrapping_div(2 as u_int));
                    if even != 0 {
                        utf8_set(&mut sy.gc.data, '[' as i32 as u_char);
                        screen_write_cell(&mut ctx[current as usize], &sy.gc);
                        width[current as usize] = width[current as usize].wrapping_add(1);
                    }
                }
            } else if at(cp) != b'#' || at(cp + 1) != b'[' || sy.ignore != 0 {
                more = utf8_open(&mut sy.gc.data, bytes[cp]);
                if more as ::core::ffi::c_uint
                    == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    loop {
                        cp += 1;
                        if !(cp < bytes.len()
                            && more as ::core::ffi::c_uint
                                == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                        {
                            break;
                        }
                        more = utf8_append(&mut sy.gc.data, bytes[cp]);
                    }
                    if more as ::core::ffi::c_uint
                        != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                    {
                        cp -= sy.gc.data.have as usize;
                    }
                }
                if more as ::core::ffi::c_uint
                    != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    if bytes[cp] < 0x20 || bytes[cp] > 0x7e {
                        cp += 1;
                        continue;
                    } else {
                        utf8_set(&mut sy.gc.data, bytes[cp]);
                        cp += 1;
                    }
                }
                screen_write_cell(&mut ctx[current as usize], &sy.gc);
                width[current as usize] =
                    width[current as usize].wrapping_add(sy.gc.data.width as u_int);
            } else {
                let style_start = cp + 2;
                let Some(style_len) = format_skip(&bytes[style_start..]) else {
                    log_debug(format_args!(
                        "{}: no terminating ] at '{}'",
                        "format_draw",
                        crate::log::log_bytes(&bytes[style_start..])
                    ));
                    frs.clear();
                    current_block = 4329038292887906754;
                    break;
                };
                let end = style_start + style_len;
                {
                    let style_text = CString::new(&bytes[style_start..end])
                        .expect("format style contains an interior NUL");
                    style_copy(&mut saved_sy, &sy);
                    if style_parse(&mut sy, &current_default, &style_text)
                        != 0 as ::core::ffi::c_int
                    {
                        log_debug(format_args!(
                            "{}: invalid style '{}'",
                            "format_draw",
                            crate::log::log_bytes(style_text.as_bytes())
                        ));
                        drop(style_text);
                        cp = end + 1;
                    } else {
                        log_debug(format_args!(
                            "{}: style '{}' -> '{}'",
                            "format_draw",
                            crate::log::log_bytes(style_text.as_bytes()),
                            log_cstr(&style_tostring(&sy))
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
                                    focus_end = ctx[LIST as ::core::ffi::c_int as usize].screen.cx
                                        as ::core::ffi::c_int;
                                }
                                current = LIST;
                            }
                            2 => {
                                if !(list_state != 0 as ::core::ffi::c_int)
                                    && focus_start == -(1 as ::core::ffi::c_int)
                                {
                                    focus_start = ctx[LIST as ::core::ffi::c_int as usize].screen.cx
                                        as ::core::ffi::c_int;
                                }
                            }
                            0 => {
                                if list_state == 0 as ::core::ffi::c_int {
                                    fr = None;
                                    if focus_start != -(1 as ::core::ffi::c_int)
                                        && focus_end == -(1 as ::core::ffi::c_int)
                                    {
                                        focus_end =
                                            ctx[LIST as ::core::ffi::c_int as usize].screen.cx
                                                as ::core::ffi::c_int;
                                    }
                                    map[list_align as usize] = AFTER as ::core::ffi::c_int as u_int;
                                    if list_align as ::core::ffi::c_uint
                                        == STYLE_ALIGN_LEFT as ::core::ffi::c_int
                                            as ::core::ffi::c_uint
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
                                    && !(ctx[LIST_LEFT as ::core::ffi::c_int as usize].screen.cx
                                        != 0 as u_int)
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
                                && !(ctx[LIST_RIGHT as ::core::ffi::c_int as usize].screen.cx
                                    != 0 as u_int) =>
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
                        if srs.is_some() {
                            if fr
                                .as_mut()
                                .is_some_and(|pending| format_is_type(pending, &sy) == 0)
                            {
                                let mut finished = fr.take().unwrap();
                                if ctx[current as usize].screen.cx != finished.start {
                                    finished.end = ctx[current as usize].screen.cx;
                                    frs.push(finished);
                                }
                            }
                            if fr.is_none()
                                && sy.range_type as ::core::ffi::c_uint
                                    != STYLE_RANGE_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
                            {
                                fr = Some(format_range {
                                    index: current as u_int,
                                    start: ctx[current as usize].screen.cx,
                                    end: 0,
                                    type_0: sy.range_type,
                                    argument: sy.range_argument,
                                    string: sy.range_string,
                                });
                            }
                        }
                        cp = end + 1;
                    }
                }
            }
        }
        for (i, part_ctx) in ctx.into_iter().enumerate() {
            screen_write_stop(part_ctx);
            if current_block == 1830138855519935310 {
                log_debug(format_args!(
                    "{}: width {} is {}",
                    "format_draw",
                    log_cstr(names[i]),
                    (width[i]) as u32
                ));
            }
        }
    }
    if current_block == 1830138855519935310 {
        fr = None;
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
                let [left, centre, right, abs_centre, ..] = &mut s;
                format_draw_none(
                    octx, available, ocx, ocy, left, centre, right, abs_centre, &mut frs,
                );
            }
            1 => {
                let [left, centre, right, abs_centre, list, list_left, list_right, after] = &mut s;
                format_draw_left(
                    octx,
                    available,
                    ocx,
                    ocy,
                    left,
                    centre,
                    right,
                    abs_centre,
                    list,
                    list_left,
                    list_right,
                    after,
                    focus_start,
                    focus_end,
                    &mut frs,
                );
            }
            2 => {
                let [left, centre, right, abs_centre, list, list_left, list_right, after] = &mut s;
                format_draw_centre(
                    octx,
                    available,
                    ocx,
                    ocy,
                    left,
                    centre,
                    right,
                    abs_centre,
                    list,
                    list_left,
                    list_right,
                    after,
                    focus_start,
                    focus_end,
                    &mut frs,
                );
            }
            3 => {
                let [left, centre, right, abs_centre, list, list_left, list_right, after] = &mut s;
                format_draw_right(
                    octx,
                    available,
                    ocx,
                    ocy,
                    left,
                    centre,
                    right,
                    abs_centre,
                    list,
                    list_left,
                    list_right,
                    after,
                    focus_start,
                    focus_end,
                    &mut frs,
                );
            }
            4 => {
                let [left, centre, right, abs_centre, list, list_left, list_right, after] = &mut s;
                format_draw_absolute_centre(
                    octx,
                    available,
                    ocx,
                    ocy,
                    left,
                    centre,
                    right,
                    abs_centre,
                    list,
                    list_left,
                    list_right,
                    after,
                    focus_start,
                    focus_end,
                    &mut frs,
                );
            }
            _ => {}
        }
        for range in &frs {
            let owned = Box::new(style_range {
                type_0: range.type_0,
                argument: range.argument,
                string: range.string,
                start: range.start,
                end: range.end,
            });
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
            if let Some(srs) = srs.as_deref_mut() {
                srs.push(owned);
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
pub fn format_width(expanded: &CStr) -> u_int {
    let s = expanded.to_bytes();
    let mut width: u_int = 0 as u_int;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'#' {
            let (consumed, _, leading_width) = format_leading_hashes(&s[i..]);
            width = width.wrapping_add(leading_width);
            i += consumed;
            if s.get(i) == Some(&b'#') {
                let Some(end) = format_skip(&s[i + 2..]) else {
                    return 0 as u_int;
                };
                i += 2 + end + 1;
            }
        } else {
            more = utf8_open(&mut ud, s[i]);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    i += 1;
                    if !(i < s.len()
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut ud, s[i]);
                }
                if more as ::core::ffi::c_uint
                    == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    width = width.wrapping_add(ud.width as u_int);
                }
            } else if s[i] > 0x1f && s[i] < 0x7f {
                width = width.wrapping_add(1);
                i += 1;
            } else {
                i += 1;
            }
        }
    }
    width
}
pub(crate) fn format_trim_left_bytes(expanded: &CStr, mut limit: u_int) -> Vec<u8> {
    let s = expanded.to_bytes();
    let mut out = Vec::<u8>::new();
    let mut width: u_int = 0 as u_int;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    let mut i = 0;
    while i < s.len() {
        if width >= limit {
            break;
        }
        if s[i] == b'#' {
            let (consumed, n, mut leading_width) = format_leading_hashes(&s[i..]);
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
            i += consumed;
            if s.get(i) != Some(&b'#') {
                continue;
            }
            let Some(end) = format_skip(&s[i + 2..]) else {
                break;
            };
            let span = 2 + end + 1;
            out.extend_from_slice(&s[i..i + span]);
            i += span;
        } else {
            more = utf8_open(&mut ud, s[i]);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    i += 1;
                    if !(i < s.len()
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut ud, s[i]);
                }
                if more as ::core::ffi::c_uint
                    == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    if width.wrapping_add(ud.width as u_int) <= limit {
                        out.extend_from_slice(&ud.data[..ud.size as usize]);
                    }
                    width = width.wrapping_add(ud.width as u_int);
                } else {
                    i -= ud.have as usize;
                    i += 1;
                }
            } else if s[i] > 0x1f && s[i] < 0x7f {
                if width.wrapping_add(1 as u_int) <= limit {
                    out.push(s[i]);
                }
                width = width.wrapping_add(1);
                i += 1;
            } else {
                i += 1;
            }
        }
    }
    out
}
pub(crate) fn format_trim_right_bytes(expanded: &CStr, mut limit: u_int) -> Vec<u8> {
    let s = expanded.to_bytes();
    let mut out = Vec::<u8>::new();
    let mut width: u_int = 0 as u_int;
    let mut total_width: u_int = 0;
    let mut skip: u_int = 0;
    let mut copy_width: u_int = 0;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut more: utf8_state = UTF8_MORE;
    total_width = format_width(expanded);
    if total_width <= limit {
        return s.to_vec();
    }
    skip = total_width.wrapping_sub(limit);
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'#' {
            let (consumed, n, leading_width) = format_leading_hashes(&s[i..]);
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
            i += consumed;
            if s.get(i) != Some(&b'#') {
                continue;
            }
            let Some(end) = format_skip(&s[i + 2..]) else {
                break;
            };
            let span = 2 + end + 1;
            out.extend_from_slice(&s[i..i + span]);
            i += span;
        } else {
            more = utf8_open(&mut ud, s[i]);
            if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                loop {
                    i += 1;
                    if !(i < s.len()
                        && more as ::core::ffi::c_uint
                            == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint)
                    {
                        break;
                    }
                    more = utf8_append(&mut ud, s[i]);
                }
                if more as ::core::ffi::c_uint
                    == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    if width >= skip {
                        out.extend_from_slice(&ud.data[..ud.size as usize]);
                    }
                    width = width.wrapping_add(ud.width as u_int);
                } else {
                    i -= ud.have as usize;
                    i += 1;
                }
            } else if s[i] > 0x1f && s[i] < 0x7f {
                if width >= skip {
                    out.push(s[i]);
                }
                width = width.wrapping_add(1);
                i += 1;
            } else {
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod range_tests {
    use super::*;
    use crate::options::options_table;
    use crate::options::OptionsScope;

    #[test]
    fn rendered_ranges_follow_their_alignment_and_clipping() {
        unsafe {
            OptionsScope::GlobalServer.reset_default(
                options_table
                    .iter()
                    .find(|entry| entry.name == Some(c"extended-keys"))
                    .unwrap(),
            );
            for (width, expected) in [
                (
                    20,
                    vec![("left", 0, 2), ("centre", 9, 11), ("right", 18, 20)],
                ),
                (3, vec![("left", 0, 2), ("right", 2, 3)]),
            ] {
                let mut output = screen::default();
                screen_init(&mut output, width, 1, 0);
                let mut ranges = style_ranges::default();
                let mut ctx = screen_write_start(&mut output);
                format_draw(
                    &mut ctx,
                    &grid_default_cell,
                    width,
                    c"#[align=left,range=user|left]LL#[norange]#[align=centre,range=user|centre]CC#[norange]#[align=right,range=user|right]RR#[norange]",
                    Some(&mut ranges),
                    0,
                );
                screen_write_stop(ctx);
                let actual: Vec<_> = ranges
                    .iter()
                    .map(|range| {
                        (
                            CStr::from_bytes_until_nul(&range.string)
                                .unwrap()
                                .to_str()
                                .unwrap(),
                            range.start,
                            range.end,
                        )
                    })
                    .collect();
                drop(output);
                assert_eq!(actual, expected, "available width {width}");
            }
        }
    }
}
