use crate::src::compat::strtonum::strtonum;
use crate::src::ffi::libc::{
    memcpy, snprintf, strcasecmp, strchr, strcmp, strcspn, strlcpy, strncasecmp, strspn,
};
use crate::src::format::bytes::xformat_with;
use crate::src::format::{format_create, format_single_cstring};
use crate::src::grid::grid_default_cell;
use crate::src::hyperlinks::{hyperlinks_get, hyperlinks_put, HyperlinksRef};
use crate::src::log::{fatalx, log_cstr, log_debug};
use crate::src::options::options_string_to_style;
use crate::src::shared::abi::*;
pub use crate::src::shared::command::cmdq_item;
pub use crate::src::shared::environment::environ;
pub use crate::src::shared::format::FORMAT_NOJOBS;
pub use crate::src::shared::format::{format_job_tree, format_tree};
use crate::src::shared::grid::*;
use crate::src::shared::hyperlinks::hyperlinks_uri;
pub use crate::src::shared::key::key_event;
pub use crate::src::shared::limits::UINT_MAX;
pub use crate::src::shared::mouse::mouse_event;
pub use crate::src::shared::options::options_table_entry;
pub use crate::src::shared::options::{options, options_entry};
pub use crate::src::shared::pane::{
    PANE_SCROLLBARS_CHARACTER, PANE_SCROLLBARS_DEFAULT_PADDING, PANE_SCROLLBARS_DEFAULT_WIDTH,
};
use crate::src::shared::style::*;
pub use crate::src::shared::tty::tty_term;
use crate::src::style::attributes::{attributes_format, attributes_parse_cstr};
use crate::src::style::colour::{colour_format, colour_parse_cstr};
use crate::src::text::utf8::utf8_set;
use std::ffi::{CStr, CString};

pub const STYLE_WIDTH_DEFAULT: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
pub const STYLE_PAD_DEFAULT: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);

pub(super) static mut style_default: style = unsafe {
    style {
        gc: grid_cell {
            data: utf8_data {
                data: [
                    ' ' as i32 as u_char,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
                have: 0 as u_char,
                size: 1 as u_char,
                width: 1 as u_char,
            },
            attr: 0 as u_short,
            flags: 0 as u_char,
            fg: 8 as ::core::ffi::c_int,
            bg: 8 as ::core::ffi::c_int,
            us: 0 as ::core::ffi::c_int,
            link: 0 as u_int,
        },
        ignore: 0 as ::core::ffi::c_int,
        dim: 0 as ::core::ffi::c_int,
        fill: 8 as ::core::ffi::c_int,
        align: STYLE_ALIGN_DEFAULT,
        list: STYLE_LIST_OFF,
        range_type: STYLE_RANGE_NONE,
        range_argument: 0 as u_int,
        range_string: ::core::mem::transmute::<[u8; 16], [::core::ffi::c_char; 16]>(
            *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        ),
        width: STYLE_WIDTH_DEFAULT,
        width_percentage: 0 as ::core::ffi::c_int,
        pad: STYLE_PAD_DEFAULT,
        default_type: STYLE_DEFAULT_BASE,
        link: 0 as u_int,
    }
};
thread_local! {
    // Style parsing and the global hyperlink eviction list run on the event loop.
    static STYLE_HYPERLINKS: std::cell::RefCell<Option<HyperlinksRef>> = const { std::cell::RefCell::new(None) };
}
fn style_hyperlinks(create: bool) -> Option<HyperlinksRef> {
    STYLE_HYPERLINKS.with(|slot| {
        let mut owner = slot.borrow_mut();
        if create && owner.is_none() {
            *owner = Some(HyperlinksRef::new());
        }
        owner.clone()
    })
}
unsafe fn style_set_range_string(mut sy: *mut style, s: &CStr) {
    strlcpy(
        &raw mut (*sy).range_string as *mut ::core::ffi::c_char,
        s.as_ptr(),
        ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t,
    );
}
pub unsafe fn style_parse(mut sy: *mut style, base: &grid_cell, in_0: &CStr) -> ::core::ffi::c_int {
    let mut in_0: *const ::core::ffi::c_char = in_0.as_ptr();
    let mut current_block: u64;
    let mut saved: style = style {
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
    let delimiters: [::core::ffi::c_char; 4] =
        ::core::mem::transmute::<[u8; 4], [::core::ffi::c_char; 4]>(*b" ,\n\0");
    let mut tmp: [::core::ffi::c_char; 256] = [0; 256];
    let mut found: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut value: ::core::ffi::c_int = 0;
    let mut end: size_t = 0;
    let mut n: u_int = 0;
    if *in_0 as ::core::ffi::c_int == '\0' as i32 {
        return 0 as ::core::ffi::c_int;
    }
    style_copy(&raw mut saved, sy);
    log_debug(format_args!(
        "{}: {}",
        "style_parse",
        log_cstr(CStr::from_ptr(in_0))
    ));
    loop {
        while *in_0 as ::core::ffi::c_int != '\0' as i32
            && !strchr(
                &raw const delimiters as *const ::core::ffi::c_char,
                *in_0 as ::core::ffi::c_int,
            )
            .is_null()
        {
            in_0 = in_0.offset(1);
        }
        if *in_0 as ::core::ffi::c_int == '\0' as i32 {
            current_block = 5832582820025303349;
            break;
        }
        end = strcspn(in_0, &raw const delimiters as *const ::core::ffi::c_char) as size_t;
        if end
            > (::core::mem::size_of::<[::core::ffi::c_char; 256]>() as usize).wrapping_sub(1_usize)
        {
            current_block = 6605876559004397942;
            break;
        }
        memcpy(
            &raw mut tmp as *mut ::core::ffi::c_char as *mut ::core::ffi::c_void,
            in_0 as *const ::core::ffi::c_void,
            end,
        );
        tmp[end as usize] = '\0' as i32 as ::core::ffi::c_char;
        log_debug(format_args!(
            "{}: {}",
            "style_parse",
            log_cstr(CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char))
        ));
        if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"default".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).gc.fg = base.fg;
            (*sy).gc.bg = base.bg;
            (*sy).gc.us = base.us;
            (*sy).gc.attr = base.attr;
            (*sy).gc.flags = base.flags;
            (*sy).link = 0 as u_int;
        } else if strcasecmp(&raw mut tmp as *mut ::core::ffi::c_char, c"ignore".as_ptr())
            == 0 as ::core::ffi::c_int
        {
            (*sy).ignore = 1 as ::core::ffi::c_int;
        } else if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"noignore".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).ignore = 0 as ::core::ffi::c_int;
        } else if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"push-default".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).default_type = STYLE_DEFAULT_PUSH;
        } else if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"pop-default".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).default_type = STYLE_DEFAULT_POP;
        } else if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"set-default".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).default_type = STYLE_DEFAULT_SET;
        } else if strcasecmp(&raw mut tmp as *mut ::core::ffi::c_char, c"nolist".as_ptr())
            == 0 as ::core::ffi::c_int
        {
            (*sy).list = STYLE_LIST_OFF;
        } else if strncasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"list=".as_ptr(),
            5 as size_t,
        ) == 0 as ::core::ffi::c_int
        {
            if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(5 as ::core::ffi::c_int as isize),
                c"on".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).list = STYLE_LIST_ON;
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(5 as ::core::ffi::c_int as isize),
                c"focus".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).list = STYLE_LIST_FOCUS;
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(5 as ::core::ffi::c_int as isize),
                c"left-marker".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).list = STYLE_LIST_LEFT_MARKER;
            } else {
                if !(strcasecmp(
                    (&raw mut tmp as *mut ::core::ffi::c_char)
                        .offset(5 as ::core::ffi::c_int as isize),
                    c"right-marker".as_ptr(),
                ) == 0 as ::core::ffi::c_int)
                {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).list = STYLE_LIST_RIGHT_MARKER;
            }
        } else if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"norange".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).range_type = style_default.range_type;
            (*sy).range_argument = style_default.range_type as u_int;
            strlcpy(
                &raw mut (*sy).range_string as *mut ::core::ffi::c_char,
                &raw mut style_default.range_string as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t,
            );
        } else if end > 6 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"range=".as_ptr(),
                6 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            found = strchr(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                '|' as i32,
            );
            if !found.is_null() {
                let fresh0 = found;
                found = found.offset(1);
                *fresh0 = '\0' as i32 as ::core::ffi::c_char;
                if *found as ::core::ffi::c_int == '\0' as i32 {
                    current_block = 6605876559004397942;
                    break;
                }
            }
            if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"left".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if !found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_LEFT;
                (*sy).range_argument = 0 as u_int;
                style_set_range_string(sy, c"");
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"right".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if !found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_RIGHT;
                (*sy).range_argument = 0 as u_int;
                style_set_range_string(sy, c"");
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"control".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                let parsed = strtonum(
                    CStr::from_ptr(found),
                    0 as ::core::ffi::c_longlong,
                    9 as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_CONTROL;
                (*sy).range_argument = n;
                style_set_range_string(sy, c"");
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"pane".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                if *found as ::core::ffi::c_int != '%' as i32
                    || *found.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
                        == '\0' as i32
                {
                    current_block = 6605876559004397942;
                    break;
                }
                let parsed = strtonum(
                    CStr::from_ptr(found.offset(1 as ::core::ffi::c_int as isize)),
                    0 as ::core::ffi::c_longlong,
                    UINT_MAX as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_PANE;
                (*sy).range_argument = n;
                style_set_range_string(sy, c"");
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"window".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                let parsed = strtonum(
                    CStr::from_ptr(found),
                    0 as ::core::ffi::c_longlong,
                    UINT_MAX as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_WINDOW;
                (*sy).range_argument = n;
                style_set_range_string(sy, c"");
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"session".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                if *found as ::core::ffi::c_int != '$' as i32
                    || *found.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
                        == '\0' as i32
                {
                    current_block = 6605876559004397942;
                    break;
                }
                let parsed = strtonum(
                    CStr::from_ptr(found.offset(1 as ::core::ffi::c_int as isize)),
                    0 as ::core::ffi::c_longlong,
                    UINT_MAX as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_SESSION;
                (*sy).range_argument = n;
                style_set_range_string(sy, c"");
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"user".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                if found.is_null() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).range_type = STYLE_RANGE_USER;
                (*sy).range_argument = 0 as u_int;
                style_set_range_string(sy, CStr::from_ptr(found));
            }
        } else if strcasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"noalign".as_ptr(),
        ) == 0 as ::core::ffi::c_int
        {
            (*sy).align = style_default.align;
        } else if end > 6 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"align=".as_ptr(),
                6 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"left".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).align = STYLE_ALIGN_LEFT;
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"centre".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).align = STYLE_ALIGN_CENTRE;
            } else if strcasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(6 as ::core::ffi::c_int as isize),
                c"right".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).align = STYLE_ALIGN_RIGHT;
            } else {
                if !(strcasecmp(
                    (&raw mut tmp as *mut ::core::ffi::c_char)
                        .offset(6 as ::core::ffi::c_int as isize),
                    c"absolute-centre".as_ptr(),
                ) == 0 as ::core::ffi::c_int)
                {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).align = STYLE_ALIGN_ABSOLUTE_CENTRE;
            }
        } else if end > 5 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"fill=".as_ptr(),
                5 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            value = colour_parse_cstr(std::ffi::CStr::from_ptr(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(5 as ::core::ffi::c_int as isize),
            ))
            .unwrap_or(-1);
            if value == -(1 as ::core::ffi::c_int) {
                current_block = 6605876559004397942;
                break;
            }
            (*sy).fill = value;
        } else if end > 4 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"dim=".as_ptr(),
                4 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            if tmp[end.wrapping_sub(1 as size_t) as usize] as ::core::ffi::c_int == '%' as i32 {
                tmp[end.wrapping_sub(1 as size_t) as usize] = '\0' as i32 as ::core::ffi::c_char;
            }
            let parsed = strtonum(
                CStr::from_ptr(
                    (&raw mut tmp as *mut ::core::ffi::c_char)
                        .offset(4 as ::core::ffi::c_int as isize),
                ),
                0 as ::core::ffi::c_longlong,
                100 as ::core::ffi::c_longlong,
            );
            n = parsed.unwrap_or(0) as u_int;
            if parsed.is_err() {
                current_block = 6605876559004397942;
                break;
            }
            (*sy).dim = n as ::core::ffi::c_int;
        } else if end > 3 as size_t
            && strncasecmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(1 as ::core::ffi::c_int as isize),
                c"g=".as_ptr(),
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            value = colour_parse_cstr(std::ffi::CStr::from_ptr(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(3 as ::core::ffi::c_int as isize),
            ))
            .unwrap_or(-1);
            if value == -(1 as ::core::ffi::c_int) {
                current_block = 6605876559004397942;
                break;
            }
            if *in_0 as ::core::ffi::c_int == 'f' as i32
                || *in_0 as ::core::ffi::c_int == 'F' as i32
            {
                if value != 8 as ::core::ffi::c_int {
                    (*sy).gc.fg = value;
                } else {
                    (*sy).gc.fg = base.fg;
                }
            } else {
                if !(*in_0 as ::core::ffi::c_int == 'b' as i32
                    || *in_0 as ::core::ffi::c_int == 'B' as i32)
                {
                    current_block = 6605876559004397942;
                    break;
                }
                if value != 8 as ::core::ffi::c_int {
                    (*sy).gc.bg = value;
                } else {
                    (*sy).gc.bg = base.bg;
                }
            }
        } else if end > 3 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"us=".as_ptr(),
                3 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            value = colour_parse_cstr(std::ffi::CStr::from_ptr(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(3 as ::core::ffi::c_int as isize),
            ))
            .unwrap_or(-1);
            if value == -(1 as ::core::ffi::c_int) {
                current_block = 6605876559004397942;
                break;
            }
            if value != 8 as ::core::ffi::c_int {
                (*sy).gc.us = value;
            } else {
                (*sy).gc.us = base.us;
            }
        } else if strcasecmp(&raw mut tmp as *mut ::core::ffi::c_char, c"none".as_ptr())
            == 0 as ::core::ffi::c_int
        {
            (*sy).gc.attr = 0 as u_short;
        } else if end > 2 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"no".as_ptr(),
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            if strcmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(2 as ::core::ffi::c_int as isize),
                c"link".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).link = 0 as u_int;
            } else if strcmp(
                (&raw mut tmp as *mut ::core::ffi::c_char).offset(2 as ::core::ffi::c_int as isize),
                c"attr".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                (*sy).gc.attr = ((*sy).gc.attr as ::core::ffi::c_int | GRID_ATTR_NOATTR) as u_short;
            } else {
                value = attributes_parse_cstr(std::ffi::CStr::from_ptr(
                    (&raw mut tmp as *mut ::core::ffi::c_char)
                        .offset(2 as ::core::ffi::c_int as isize),
                ))
                .unwrap_or(-1);
                if value == -(1 as ::core::ffi::c_int) {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).gc.attr = ((*sy).gc.attr as ::core::ffi::c_int & !value) as u_short;
            }
        } else if end > 6 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"width=".as_ptr(),
                6 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            if end > 7 as size_t
                && tmp[end.wrapping_sub(1 as size_t) as usize] as ::core::ffi::c_int == '%' as i32
            {
                tmp[end.wrapping_sub(1 as size_t) as usize] = '\0' as i32 as ::core::ffi::c_char;
                let parsed = strtonum(
                    CStr::from_ptr(
                        (&raw mut tmp as *mut ::core::ffi::c_char)
                            .offset(6 as ::core::ffi::c_int as isize),
                    ),
                    0 as ::core::ffi::c_longlong,
                    100 as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).width = n as ::core::ffi::c_int;
                (*sy).width_percentage = 1 as ::core::ffi::c_int;
            } else {
                let parsed = strtonum(
                    CStr::from_ptr(
                        (&raw mut tmp as *mut ::core::ffi::c_char)
                            .offset(6 as ::core::ffi::c_int as isize),
                    ),
                    0 as ::core::ffi::c_longlong,
                    UINT_MAX as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    current_block = 6605876559004397942;
                    break;
                }
                (*sy).width = n as ::core::ffi::c_int;
                (*sy).width_percentage = 0 as ::core::ffi::c_int;
            }
        } else if end > 4 as size_t
            && strncasecmp(
                &raw mut tmp as *mut ::core::ffi::c_char,
                c"pad=".as_ptr(),
                4 as size_t,
            ) == 0 as ::core::ffi::c_int
        {
            let parsed = strtonum(
                CStr::from_ptr(
                    (&raw mut tmp as *mut ::core::ffi::c_char)
                        .offset(4 as ::core::ffi::c_int as isize),
                ),
                0 as ::core::ffi::c_longlong,
                UINT_MAX as ::core::ffi::c_longlong,
            );
            n = parsed.unwrap_or(0) as u_int;
            if parsed.is_err() {
                current_block = 6605876559004397942;
                break;
            }
            (*sy).pad = n as ::core::ffi::c_int;
        } else if strncasecmp(
            &raw mut tmp as *mut ::core::ffi::c_char,
            c"link=".as_ptr(),
            5 as size_t,
        ) == 0 as ::core::ffi::c_int
        {
            if tmp[5 as ::core::ffi::c_int as usize] as ::core::ffi::c_int == '\0' as i32 {
                (*sy).link = 0 as u_int;
            } else {
                let table = style_hyperlinks(true).expect("style hyperlink table");
                let uri = std::ffi::CStr::from_ptr(tmp.as_ptr().add(5));
                (*sy).link = hyperlinks_put(&table, uri, Some(uri));
            }
        } else {
            value = attributes_parse_cstr(std::ffi::CStr::from_ptr(
                &raw mut tmp as *mut ::core::ffi::c_char,
            ))
            .unwrap_or(-1);
            if value == -(1 as ::core::ffi::c_int) {
                current_block = 6605876559004397942;
                break;
            }
            (*sy).gc.attr = ((*sy).gc.attr as ::core::ffi::c_int | value) as u_short;
        }
        in_0 = in_0.add(end.wrapping_add(strspn(
            in_0.add(end),
            &raw const delimiters as *const ::core::ffi::c_char,
        ) as size_t));
        if !(*in_0 as ::core::ffi::c_int != '\0' as i32) {
            current_block = 5832582820025303349;
            break;
        }
    }
    match current_block {
        5832582820025303349 => 0 as ::core::ffi::c_int,
        _ => {
            style_copy(sy, &raw mut saved);
            -(1 as ::core::ffi::c_int)
        }
    }
}
pub unsafe fn style_tostring(mut sy: *mut style) -> CString {
    let mut gc: *mut grid_cell = &raw mut (*sy).gc;
    let mut off: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut comma: *const ::core::ffi::c_char = c"".as_ptr();
    let mut tmp: *const ::core::ffi::c_char = c"".as_ptr();
    static mut s: [u8; 2048] = [0; 2048];
    let mut b: [::core::ffi::c_char; 21] = [0; 21];
    *(&raw mut s as *mut ::core::ffi::c_char) = '\0' as i32 as ::core::ffi::c_char;
    if (*sy).list as ::core::ffi::c_uint
        != STYLE_LIST_OFF as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if (*sy).list as ::core::ffi::c_uint
            == STYLE_LIST_ON as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"on".as_ptr();
        } else if (*sy).list as ::core::ffi::c_uint
            == STYLE_LIST_FOCUS as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"focus".as_ptr();
        } else if (*sy).list as ::core::ffi::c_uint
            == STYLE_LIST_LEFT_MARKER as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"left-marker".as_ptr();
        } else if (*sy).list as ::core::ffi::c_uint
            == STYLE_LIST_RIGHT_MARKER as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"right-marker".as_ptr();
        }
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"list=")?;
            out.write_all(std::ffi::CStr::from_ptr(tmp).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*sy).range_type as ::core::ffi::c_uint
        != STYLE_RANGE_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if (*sy).range_type as ::core::ffi::c_uint
            == STYLE_RANGE_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"left".as_ptr();
        } else if (*sy).range_type as ::core::ffi::c_uint
            == STYLE_RANGE_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"right".as_ptr();
        } else if (*sy).range_type as ::core::ffi::c_uint
            == STYLE_RANGE_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            snprintf(
                &raw mut b as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 21]>() as size_t,
                c"pane|%%%u".as_ptr(),
                (*sy).range_argument,
            );
            tmp = &raw mut b as *mut ::core::ffi::c_char;
        } else if (*sy).range_type as ::core::ffi::c_uint
            == STYLE_RANGE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            snprintf(
                &raw mut b as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 21]>() as size_t,
                c"window|%u".as_ptr(),
                (*sy).range_argument,
            );
            tmp = &raw mut b as *mut ::core::ffi::c_char;
        } else if (*sy).range_type as ::core::ffi::c_uint
            == STYLE_RANGE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            snprintf(
                &raw mut b as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 21]>() as size_t,
                c"session|$%u".as_ptr(),
                (*sy).range_argument,
            );
            tmp = &raw mut b as *mut ::core::ffi::c_char;
        } else if (*sy).range_type as ::core::ffi::c_uint
            == STYLE_RANGE_USER as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            snprintf(
                &raw mut b as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 21]>() as size_t,
                c"user|%s".as_ptr(),
                &raw mut (*sy).range_string as *mut ::core::ffi::c_char,
            );
            tmp = &raw mut b as *mut ::core::ffi::c_char;
        }
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"range=")?;
            out.write_all(std::ffi::CStr::from_ptr(tmp).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*sy).align as ::core::ffi::c_uint
        != STYLE_ALIGN_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if (*sy).align as ::core::ffi::c_uint
            == STYLE_ALIGN_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"left".as_ptr();
        } else if (*sy).align as ::core::ffi::c_uint
            == STYLE_ALIGN_CENTRE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"centre".as_ptr();
        } else if (*sy).align as ::core::ffi::c_uint
            == STYLE_ALIGN_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"right".as_ptr();
        } else if (*sy).align as ::core::ffi::c_uint
            == STYLE_ALIGN_ABSOLUTE_CENTRE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"absolute-centre".as_ptr();
        }
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"align=")?;
            out.write_all(std::ffi::CStr::from_ptr(tmp).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*sy).default_type as ::core::ffi::c_uint
        != STYLE_DEFAULT_BASE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        if (*sy).default_type as ::core::ffi::c_uint
            == STYLE_DEFAULT_PUSH as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"push-default".as_ptr();
        } else if (*sy).default_type as ::core::ffi::c_uint
            == STYLE_DEFAULT_POP as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"pop-default".as_ptr();
        } else if (*sy).default_type as ::core::ffi::c_uint
            == STYLE_DEFAULT_SET as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            tmp = c"set-default".as_ptr();
        }
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(std::ffi::CStr::from_ptr(tmp).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*sy).fill != 8 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"fill=")?;
            out.write_all(colour_format((*sy).fill).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*sy).dim != 0 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            write!(out, "dim={}%", { (*sy).dim })
        });
        comma = c",".as_ptr();
    }
    if (*gc).fg != 8 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"fg=")?;
            out.write_all(colour_format((*gc).fg).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*gc).bg != 8 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"bg=")?;
            out.write_all(colour_format((*gc).bg).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*gc).us != 8 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"us=")?;
            out.write_all(colour_format((*gc).us).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*gc).attr as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(attributes_format((*gc).attr as ::core::ffi::c_int).to_bytes())
        });
        comma = c",".as_ptr();
    }
    if (*sy).width >= 0 as ::core::ffi::c_int {
        if (*sy).width_percentage != 0 {
            off += xformat_with(&mut (&mut s)[off as usize..], |out| {
                out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
                write!(out, "width={}%", ((*sy).width) as u32)
            });
        } else {
            off += xformat_with(&mut (&mut s)[off as usize..], |out| {
                out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
                write!(out, "width={}", ((*sy).width) as u32)
            });
        }
        comma = c",".as_ptr();
    }
    if (*sy).pad >= 0 as ::core::ffi::c_int {
        off += xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            write!(out, "pad={}", ((*sy).pad) as u32)
        });
        comma = c",".as_ptr();
    }
    if let Some(uri) = style_link(&*sy) {
        xformat_with(&mut (&mut s)[off as usize..], |out| {
            out.write_all(std::ffi::CStr::from_ptr(comma).to_bytes())?;
            out.write_all(b"link=")?;
            out.write_all(uri.uri.as_bytes())
        });
        comma = c",".as_ptr();
    }
    if *(&raw mut s as *mut ::core::ffi::c_char) as ::core::ffi::c_int == '\0' as i32 {
        return c"default".to_owned();
    }
    CStr::from_ptr(&raw const s as *const ::core::ffi::c_char).to_owned()
}
/// Copy link data before callers insert it into a table, which may evict the source.
pub fn style_link(sy: &style) -> Option<Box<hyperlinks_uri>> {
    if sy.link == 0 {
        return None;
    }
    let table = style_hyperlinks(false)?;
    let entry = hyperlinks_get(&table, sy.link)?;
    Some(Box::new(entry.clone()))
}
pub unsafe fn style_add(
    gc: *mut grid_cell,
    oo: *mut options,
    name: &CStr,
    ft: *mut format_tree,
) -> style {
    let mut owned_context = None;
    let context = if let Some(context) = ft.as_mut() {
        context
    } else {
        owned_context.insert(format_create(None, None, 0, FORMAT_NOJOBS))
    };
    let parsed = options_string_to_style(oo, name, context).unwrap_or(style_default);
    if parsed.gc.fg != 8 {
        (*gc).fg = parsed.gc.fg;
    }
    if parsed.gc.bg != 8 {
        (*gc).bg = parsed.gc.bg;
    }
    if parsed.gc.us != 8 {
        (*gc).us = parsed.gc.us;
    }
    (*gc).attr |= parsed.gc.attr;
    if let Some(context) = owned_context {
        drop(context);
    }
    parsed
}
pub unsafe fn style_apply(
    mut gc: *mut grid_cell,
    mut oo: *mut options,
    name: &CStr,
    mut ft: *mut format_tree,
) {
    *gc = grid_default_cell;
    style_add(gc, oo, name, ft);
}
pub unsafe fn style_parse_colour(
    mut sy: *mut style,
    base: &grid_cell,
    s: &CStr,
) -> ::core::ffi::c_int {
    let s: *const ::core::ffi::c_char = s.as_ptr();
    let mut c: ::core::ffi::c_int = 0;
    style_set(sy, base);
    if *s as ::core::ffi::c_int == '\0' as i32 {
        (*sy).gc.fg = -(1 as ::core::ffi::c_int);
        return 0 as ::core::ffi::c_int;
    }
    c = colour_parse_cstr(std::ffi::CStr::from_ptr(s)).unwrap_or(-1);
    if c == -(1 as ::core::ffi::c_int) {
        return -(1 as ::core::ffi::c_int);
    }
    if c == 8 as ::core::ffi::c_int {
        (*sy).gc.fg = base.fg;
    } else {
        (*sy).gc.fg = c;
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn style_set(mut sy: *mut style, gc: &grid_cell) {
    memcpy(
        sy as *mut ::core::ffi::c_void,
        &raw mut style_default as *const ::core::ffi::c_void,
        ::core::mem::size_of::<style>() as size_t,
    );
    (*sy).gc = *gc;
}
pub unsafe fn style_copy(mut dst: *mut style, mut src: *mut style) {
    memcpy(
        dst as *mut ::core::ffi::c_void,
        src as *const ::core::ffi::c_void,
        ::core::mem::size_of::<style>() as size_t,
    );
}
pub unsafe fn style_set_scrollbar_style_from_option(
    mut sb_style: *mut style,
    mut oo: *mut options,
) {
    let mut oe: *const options_table_entry = ::core::ptr::null::<options_table_entry>();
    let _o: *mut options_entry = ::core::ptr::null_mut::<options_entry>();
    let _s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    style_set(sb_style, &grid_default_cell);
    oe = crate::src::options::options_read_entry(&*oo, c"pane-scrollbars-style", |entry| {
        entry.tableentry
    })
    .flatten()
    .unwrap_or_else(|| fatalx(|out| out.write_all(b"missing pane-scrollbars-style")));
    let style = format_single_cstring(
        None,
        (*oe).default_str.map_or(std::ptr::null(), CStr::as_ptr),
        None,
        None,
        (refbox::Weak::new()).clone(),
        None,
    );
    if style_parse(sb_style, &grid_default_cell, &style) != 0 as ::core::ffi::c_int {
        fatalx(|out| out.write_all(b"bad pane-scrollbars-style default"));
    }
    let value = crate::src::options::options_get_string_optional(oo, c"pane-scrollbars-style");
    if let Some(value) = value {
        let expanded = format_single_cstring(
            None,
            value.as_ptr(),
            None,
            None,
            (refbox::Weak::new()).clone(),
            None,
        );
        if style_parse(sb_style, &grid_default_cell, &expanded) != 0 as ::core::ffi::c_int {
            style_parse(sb_style, &grid_default_cell, &style);
        }
    }
    if (*sb_style).width < 1 as ::core::ffi::c_int {
        (*sb_style).width = PANE_SCROLLBARS_DEFAULT_WIDTH;
    }
    if (*sb_style).pad < 0 as ::core::ffi::c_int {
        (*sb_style).pad = PANE_SCROLLBARS_DEFAULT_PADDING;
    }
    utf8_set(
        &mut (*sb_style).gc.data,
        PANE_SCROLLBARS_CHARACTER as u_char,
    );
}
pub unsafe fn style_ranges_init(mut srs: *mut style_ranges) {
    if srs.is_null() {
        return;
    }
    *srs = Vec::new();
}
pub unsafe fn style_ranges_clear(mut srs: *mut style_ranges) {
    if !srs.is_null() {
        (*srs).clear();
    }
}
pub unsafe fn style_ranges_get_range(mut srs: *mut style_ranges, mut x: u_int) -> *mut style_range {
    if srs.is_null() {
        return ::core::ptr::null_mut::<style_range>();
    }
    for range in (*srs).as_slice() {
        let sr = range.as_ref();
        if x >= sr.start && x < sr.end {
            return sr as *const style_range as *mut style_range;
        }
    }
    ::core::ptr::null_mut::<style_range>()
}

#[cfg(test)]
mod style_ranges_tests {
    use super::{
        style_range, style_ranges, style_ranges_clear, style_ranges_get_range, style_ranges_init,
        STYLE_RANGE_LEFT,
    };
    use crate::src::shared::abi::u_int;

    fn range(start: u_int, end: u_int) -> Box<style_range> {
        Box::new(style_range {
            type_0: STYLE_RANGE_LEFT,
            argument: 0,
            string: [0; 16],
            start,
            end,
        })
    }

    #[test]
    fn cloned_ranges_own_independent_boxes() {
        let original: style_ranges = vec![range(2, 4)];
        let cloned = original.clone();
        assert!(!std::ptr::eq(
            original.as_slice()[0].as_ref(),
            cloned.as_slice()[0].as_ref(),
        ));
        drop(original);
        assert_eq!(cloned.as_slice()[0].start, 2);
        assert_eq!(cloned.as_slice()[0].end, 4);
    }

    #[test]
    fn style_ranges_own_stable_boxes_and_support_clear_and_release() {
        let mut ranges = style_ranges::default();
        unsafe {
            style_ranges_init(&mut ranges);
            let first = range(2, 4);
            let first_ptr = (&*first) as *const style_range as *mut style_range;
            ranges.push(first);
            for i in 0..1024 {
                ranges.push(range(10 + i, 11 + i));
            }
            assert_eq!(style_ranges_get_range(&mut ranges, 3), first_ptr);

            style_ranges_clear(&mut ranges);
            assert!(style_ranges_get_range(&mut ranges, 3).is_null());
            let after_clear = range(30, 35);
            let after_clear_ptr = (&*after_clear) as *const style_range as *mut style_range;
            ranges.push(after_clear);
            assert_eq!(style_ranges_get_range(&mut ranges, 32), after_clear_ptr);

            drop(std::mem::take(&mut ranges));
            assert!(ranges.is_empty());
            assert_eq!(ranges.capacity(), 0);
            assert!(style_ranges_get_range(&mut ranges, 32).is_null());

            style_ranges_init(&mut ranges);
            ranges.push(range(40, 45));
            assert!(!style_ranges_get_range(&mut ranges, 42).is_null());
        }
    }
}
