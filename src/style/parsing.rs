use crate::compat::strtonum::strtonum;
use crate::format::bytes::{cstr_copy_into, format_bytes, format_message_with};
use crate::format::format_single_cstring;
use crate::grid::grid_default_cell;
use crate::hyperlinks::{hyperlinks_get, hyperlinks_put, HyperlinksRef};
use crate::log::{fatalx, log_bytes, log_debug};
use crate::shared::abi::*;
use crate::shared::grid::*;
use crate::shared::hyperlinks::hyperlinks_uri;
pub use crate::shared::limits::UINT_MAX;
pub use crate::shared::pane::{
    PANE_SCROLLBARS_CHARACTER, PANE_SCROLLBARS_DEFAULT_PADDING, PANE_SCROLLBARS_DEFAULT_WIDTH,
};
use crate::shared::style::*;
use crate::style::attributes::{attributes_format, attributes_parse};
use crate::style::colour::{colour_format, colour_parse_cstr};
use crate::text::utf8::utf8_set;
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
unsafe fn style_set_range_string(sy: &mut style, s: &CStr) {
    cstr_copy_into(&mut sy.range_string, s);
}
const STYLE_TOKEN_MAX: usize = 255;
fn style_strip_prefix<'a>(token: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    if token.len() >= prefix.len() && token[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&token[prefix.len()..])
    } else {
        None
    }
}
fn style_cstring(bytes: &[u8]) -> CString {
    CString::new(bytes).expect("style tokens come from a C string")
}
fn style_parse_number(
    bytes: &[u8],
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
) -> Option<u_int> {
    strtonum(&style_cstring(bytes), minval, maxval)
        .ok()
        .map(|n| n as u_int)
}
unsafe fn style_parse_token(sy: &mut style, base: &grid_cell, tmp: &[u8]) -> bool {
    if tmp.eq_ignore_ascii_case(b"default") {
        sy.gc.fg = base.fg;
        sy.gc.bg = base.bg;
        sy.gc.us = base.us;
        sy.gc.attr = base.attr;
        sy.gc.flags = base.flags;
        sy.link = 0 as u_int;
    } else if tmp.eq_ignore_ascii_case(b"ignore") {
        sy.ignore = 1 as ::core::ffi::c_int;
    } else if tmp.eq_ignore_ascii_case(b"noignore") {
        sy.ignore = 0 as ::core::ffi::c_int;
    } else if tmp.eq_ignore_ascii_case(b"push-default") {
        sy.default_type = STYLE_DEFAULT_PUSH;
    } else if tmp.eq_ignore_ascii_case(b"pop-default") {
        sy.default_type = STYLE_DEFAULT_POP;
    } else if tmp.eq_ignore_ascii_case(b"set-default") {
        sy.default_type = STYLE_DEFAULT_SET;
    } else if tmp.eq_ignore_ascii_case(b"nolist") {
        sy.list = STYLE_LIST_OFF;
    } else if let Some(value) = style_strip_prefix(tmp, b"list=") {
        if value.eq_ignore_ascii_case(b"on") {
            sy.list = STYLE_LIST_ON;
        } else if value.eq_ignore_ascii_case(b"focus") {
            sy.list = STYLE_LIST_FOCUS;
        } else if value.eq_ignore_ascii_case(b"left-marker") {
            sy.list = STYLE_LIST_LEFT_MARKER;
        } else if value.eq_ignore_ascii_case(b"right-marker") {
            sy.list = STYLE_LIST_RIGHT_MARKER;
        } else {
            return false;
        }
    } else if tmp.eq_ignore_ascii_case(b"norange") {
        sy.range_type = style_default.range_type;
        sy.range_argument = style_default.range_type as u_int;
        cstr_copy_into(
            &mut sy.range_string,
            CStr::from_ptr((&raw const style_default.range_string).cast()),
        );
    } else if let Some(value) = style_strip_prefix(tmp, b"range=").filter(|v| !v.is_empty()) {
        let (name, found) = match value.iter().position(|&b| b == b'|') {
            Some(i) => (&value[..i], Some(&value[i + 1..])),
            None => (value, None),
        };
        if found.is_some_and(|found| found.is_empty()) {
            return false;
        }
        if name.eq_ignore_ascii_case(b"left") {
            if found.is_some() {
                return false;
            }
            sy.range_type = STYLE_RANGE_LEFT;
            sy.range_argument = 0 as u_int;
            style_set_range_string(sy, c"");
        } else if name.eq_ignore_ascii_case(b"right") {
            if found.is_some() {
                return false;
            }
            sy.range_type = STYLE_RANGE_RIGHT;
            sy.range_argument = 0 as u_int;
            style_set_range_string(sy, c"");
        } else if name.eq_ignore_ascii_case(b"control") {
            let Some(n) = found.and_then(|found| style_parse_number(found, 0, 9)) else {
                return false;
            };
            sy.range_type = STYLE_RANGE_CONTROL;
            sy.range_argument = n;
            style_set_range_string(sy, c"");
        } else if name.eq_ignore_ascii_case(b"pane") {
            let Some(n) = found
                .and_then(|found| found.strip_prefix(b"%"))
                .filter(|found| !found.is_empty())
                .and_then(|found| {
                    style_parse_number(found, 0, UINT_MAX as ::core::ffi::c_longlong)
                })
            else {
                return false;
            };
            sy.range_type = STYLE_RANGE_PANE;
            sy.range_argument = n;
            style_set_range_string(sy, c"");
        } else if name.eq_ignore_ascii_case(b"window") {
            let Some(n) = found.and_then(|found| {
                style_parse_number(found, 0, UINT_MAX as ::core::ffi::c_longlong)
            }) else {
                return false;
            };
            sy.range_type = STYLE_RANGE_WINDOW;
            sy.range_argument = n;
            style_set_range_string(sy, c"");
        } else if name.eq_ignore_ascii_case(b"session") {
            let Some(n) = found
                .and_then(|found| found.strip_prefix(b"$"))
                .filter(|found| !found.is_empty())
                .and_then(|found| {
                    style_parse_number(found, 0, UINT_MAX as ::core::ffi::c_longlong)
                })
            else {
                return false;
            };
            sy.range_type = STYLE_RANGE_SESSION;
            sy.range_argument = n;
            style_set_range_string(sy, c"");
        } else if name.eq_ignore_ascii_case(b"user") {
            let Some(found) = found else {
                return false;
            };
            sy.range_type = STYLE_RANGE_USER;
            sy.range_argument = 0 as u_int;
            style_set_range_string(sy, &style_cstring(found));
        }
    } else if tmp.eq_ignore_ascii_case(b"noalign") {
        sy.align = style_default.align;
    } else if let Some(value) = style_strip_prefix(tmp, b"align=").filter(|v| !v.is_empty()) {
        if value.eq_ignore_ascii_case(b"left") {
            sy.align = STYLE_ALIGN_LEFT;
        } else if value.eq_ignore_ascii_case(b"centre") {
            sy.align = STYLE_ALIGN_CENTRE;
        } else if value.eq_ignore_ascii_case(b"right") {
            sy.align = STYLE_ALIGN_RIGHT;
        } else if value.eq_ignore_ascii_case(b"absolute-centre") {
            sy.align = STYLE_ALIGN_ABSOLUTE_CENTRE;
        } else {
            return false;
        }
    } else if let Some(value) = style_strip_prefix(tmp, b"fill=").filter(|v| !v.is_empty()) {
        let Some(value) = colour_parse_cstr(&style_cstring(value)) else {
            return false;
        };
        sy.fill = value;
    } else if let Some(value) = style_strip_prefix(tmp, b"dim=").filter(|v| !v.is_empty()) {
        let value = value.strip_suffix(b"%").unwrap_or(value);
        let Some(n) = style_parse_number(value, 0, 100) else {
            return false;
        };
        sy.dim = n as ::core::ffi::c_int;
    } else if let Some(value) = tmp
        .get(1..)
        .and_then(|rest| style_strip_prefix(rest, b"g="))
        .filter(|v| !v.is_empty())
    {
        let Some(value) = colour_parse_cstr(&style_cstring(value)) else {
            return false;
        };
        if tmp[0] == b'f' || tmp[0] == b'F' {
            if value != 8 as ::core::ffi::c_int {
                sy.gc.fg = value;
            } else {
                sy.gc.fg = base.fg;
            }
        } else {
            if !(tmp[0] == b'b' || tmp[0] == b'B') {
                return false;
            }
            if value != 8 as ::core::ffi::c_int {
                sy.gc.bg = value;
            } else {
                sy.gc.bg = base.bg;
            }
        }
    } else if let Some(value) = style_strip_prefix(tmp, b"us=").filter(|v| !v.is_empty()) {
        let Some(value) = colour_parse_cstr(&style_cstring(value)) else {
            return false;
        };
        if value != 8 as ::core::ffi::c_int {
            sy.gc.us = value;
        } else {
            sy.gc.us = base.us;
        }
    } else if tmp.eq_ignore_ascii_case(b"none") {
        sy.gc.attr = 0 as u_short;
    } else if let Some(value) = style_strip_prefix(tmp, b"no").filter(|v| !v.is_empty()) {
        if value == b"link" {
            sy.link = 0 as u_int;
        } else if value == b"attr" {
            sy.gc.attr = (sy.gc.attr as ::core::ffi::c_int | GRID_ATTR_NOATTR) as u_short;
        } else {
            let Some(value) = attributes_parse(value) else {
                return false;
            };
            sy.gc.attr = (sy.gc.attr as ::core::ffi::c_int & !value) as u_short;
        }
    } else if let Some(value) = style_strip_prefix(tmp, b"width=").filter(|v| !v.is_empty()) {
        if let Some(value) = value.strip_suffix(b"%").filter(|v| !v.is_empty()) {
            let Some(n) = style_parse_number(value, 0, 100) else {
                return false;
            };
            sy.width = n as ::core::ffi::c_int;
            sy.width_percentage = 1 as ::core::ffi::c_int;
        } else {
            let Some(n) = style_parse_number(value, 0, UINT_MAX as ::core::ffi::c_longlong) else {
                return false;
            };
            sy.width = n as ::core::ffi::c_int;
            sy.width_percentage = 0 as ::core::ffi::c_int;
        }
    } else if let Some(value) = style_strip_prefix(tmp, b"pad=").filter(|v| !v.is_empty()) {
        let Some(n) = style_parse_number(value, 0, UINT_MAX as ::core::ffi::c_longlong) else {
            return false;
        };
        sy.pad = n as ::core::ffi::c_int;
    } else if let Some(value) = style_strip_prefix(tmp, b"link=") {
        if value.is_empty() {
            sy.link = 0 as u_int;
        } else {
            let table = style_hyperlinks(true).expect("style hyperlink table");
            let uri = style_cstring(value);
            sy.link = hyperlinks_put(&table, &uri, Some(&uri));
        }
    } else {
        let Some(value) = attributes_parse(tmp) else {
            return false;
        };
        sy.gc.attr = (sy.gc.attr as ::core::ffi::c_int | value) as u_short;
    }
    true
}
pub unsafe fn style_parse(sy: &mut style, base: &grid_cell, in_0: &CStr) -> ::core::ffi::c_int {
    const DELIMITERS: &[u8] = b" ,\n";
    let mut in_0 = in_0.to_bytes();
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
    if in_0.is_empty() {
        return 0 as ::core::ffi::c_int;
    }
    style_copy(&mut saved, sy);
    log_debug(format_args!("{}: {}", "style_parse", log_bytes(in_0)));
    loop {
        let start = in_0
            .iter()
            .position(|b| !DELIMITERS.contains(b))
            .unwrap_or(in_0.len());
        in_0 = &in_0[start..];
        if in_0.is_empty() {
            return 0 as ::core::ffi::c_int;
        }
        let end = in_0
            .iter()
            .position(|b| DELIMITERS.contains(b))
            .unwrap_or(in_0.len());
        if end > STYLE_TOKEN_MAX {
            break;
        }
        let tmp = &in_0[..end];
        log_debug(format_args!("{}: {}", "style_parse", log_bytes(tmp)));
        if !style_parse_token(sy, base, tmp) {
            break;
        }
        in_0 = &in_0[end..];
    }
    style_copy(sy, &saved);
    -(1 as ::core::ffi::c_int)
}
pub unsafe fn style_tostring(sy: &style) -> CString {
    let gc: &grid_cell = &sy.gc;
    let s = format_message_with(|out| {
        let b: Vec<u8>;
        let mut comma: &[u8] = b"";
        let mut tmp: &[u8] = b"";
        if sy.list as ::core::ffi::c_uint
            != STYLE_LIST_OFF as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if sy.list as ::core::ffi::c_uint
                == STYLE_LIST_ON as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"on";
            } else if sy.list as ::core::ffi::c_uint
                == STYLE_LIST_FOCUS as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"focus";
            } else if sy.list as ::core::ffi::c_uint
                == STYLE_LIST_LEFT_MARKER as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"left-marker";
            } else if sy.list as ::core::ffi::c_uint
                == STYLE_LIST_RIGHT_MARKER as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"right-marker";
            }
            out.write_all(comma)?;
            out.write_all(b"list=")?;
            out.write_all(tmp)?;
            comma = b",";
        }
        if sy.range_type as ::core::ffi::c_uint
            != STYLE_RANGE_NONE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if sy.range_type as ::core::ffi::c_uint
                == STYLE_RANGE_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"left";
            } else if sy.range_type as ::core::ffi::c_uint
                == STYLE_RANGE_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"right";
            } else if sy.range_type as ::core::ffi::c_uint
                == STYLE_RANGE_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                b = format_bytes(format_args!("pane|%{}", sy.range_argument));
                tmp = &b;
            } else if sy.range_type as ::core::ffi::c_uint
                == STYLE_RANGE_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                b = format_bytes(format_args!("window|{}", sy.range_argument));
                tmp = &b;
            } else if sy.range_type as ::core::ffi::c_uint
                == STYLE_RANGE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                b = format_bytes(format_args!("session|${}", sy.range_argument));
                tmp = &b;
            } else if sy.range_type as ::core::ffi::c_uint
                == STYLE_RANGE_USER as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                let range_string = sy.range_string.map(|c| c as u8);
                let range_string = CStr::from_bytes_until_nul(&range_string)
                    .expect("range string is NUL-terminated");
                b = [b"user|", range_string.to_bytes()].concat();
                tmp = &b;
            }
            out.write_all(comma)?;
            out.write_all(b"range=")?;
            out.write_all(tmp)?;
            comma = b",";
        }
        if sy.align as ::core::ffi::c_uint
            != STYLE_ALIGN_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if sy.align as ::core::ffi::c_uint
                == STYLE_ALIGN_LEFT as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"left";
            } else if sy.align as ::core::ffi::c_uint
                == STYLE_ALIGN_CENTRE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"centre";
            } else if sy.align as ::core::ffi::c_uint
                == STYLE_ALIGN_RIGHT as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"right";
            } else if sy.align as ::core::ffi::c_uint
                == STYLE_ALIGN_ABSOLUTE_CENTRE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"absolute-centre";
            }
            out.write_all(comma)?;
            out.write_all(b"align=")?;
            out.write_all(tmp)?;
            comma = b",";
        }
        if sy.default_type as ::core::ffi::c_uint
            != STYLE_DEFAULT_BASE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if sy.default_type as ::core::ffi::c_uint
                == STYLE_DEFAULT_PUSH as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"push-default";
            } else if sy.default_type as ::core::ffi::c_uint
                == STYLE_DEFAULT_POP as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"pop-default";
            } else if sy.default_type as ::core::ffi::c_uint
                == STYLE_DEFAULT_SET as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                tmp = b"set-default";
            }
            out.write_all(comma)?;
            out.write_all(tmp)?;
            comma = b",";
        }
        if sy.fill != 8 as ::core::ffi::c_int {
            out.write_all(comma)?;
            out.write_all(b"fill=")?;
            out.write_all(colour_format(sy.fill).to_bytes())?;
            comma = b",";
        }
        if sy.dim != 0 as ::core::ffi::c_int {
            out.write_all(comma)?;
            write!(out, "dim={}%", { sy.dim })?;
            comma = b",";
        }
        if (*gc).fg != 8 as ::core::ffi::c_int {
            out.write_all(comma)?;
            out.write_all(b"fg=")?;
            out.write_all(colour_format((*gc).fg).to_bytes())?;
            comma = b",";
        }
        if (*gc).bg != 8 as ::core::ffi::c_int {
            out.write_all(comma)?;
            out.write_all(b"bg=")?;
            out.write_all(colour_format((*gc).bg).to_bytes())?;
            comma = b",";
        }
        if (*gc).us != 8 as ::core::ffi::c_int {
            out.write_all(comma)?;
            out.write_all(b"us=")?;
            out.write_all(colour_format((*gc).us).to_bytes())?;
            comma = b",";
        }
        if (*gc).attr as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
            out.write_all(comma)?;
            out.write_all(attributes_format((*gc).attr as ::core::ffi::c_int).to_bytes())?;
            comma = b",";
        }
        if sy.width >= 0 as ::core::ffi::c_int {
            if sy.width_percentage != 0 {
                out.write_all(comma)?;
                write!(out, "width={}%", (sy.width) as u32)?;
            } else {
                out.write_all(comma)?;
                write!(out, "width={}", (sy.width) as u32)?;
            }
            comma = b",";
        }
        if sy.pad >= 0 as ::core::ffi::c_int {
            out.write_all(comma)?;
            write!(out, "pad={}", (sy.pad) as u32)?;
            comma = b",";
        }
        if let Some(uri) = style_link(&*sy) {
            out.write_all(comma)?;
            out.write_all(b"link=")?;
            out.write_all(uri.uri.as_bytes())?;
        }
        Ok(())
    });
    if s.is_empty() {
        return c"default".to_owned();
    }
    s
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
pub unsafe fn style_parse_colour(sy: &mut style, base: &grid_cell, s: &CStr) -> ::core::ffi::c_int {
    let mut c: ::core::ffi::c_int = 0;
    style_set(sy, base);
    if s.is_empty() {
        sy.gc.fg = -(1 as ::core::ffi::c_int);
        return 0 as ::core::ffi::c_int;
    }
    c = colour_parse_cstr(s).unwrap_or(-1);
    if c == -(1 as ::core::ffi::c_int) {
        return -(1 as ::core::ffi::c_int);
    }
    if c == 8 as ::core::ffi::c_int {
        sy.gc.fg = base.fg;
    } else {
        sy.gc.fg = c;
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn style_set(sy: &mut style, gc: &grid_cell) {
    *sy = style_default;
    sy.gc = *gc;
}
pub unsafe fn style_copy(dst: &mut style, src: &style) {
    *dst = *src;
}
pub unsafe fn style_set_scrollbar_style_from_option(
    sb_style: &mut style,
    scope: &crate::options::OptionsScope,
) {
    style_set(sb_style, &grid_default_cell);
    let (oe, value) = scope.with_local(|oo| {
        let oe = crate::options::options_read_entry(oo, c"pane-scrollbars-style", |entry| {
            entry.tableentry
        })
        .flatten();
        let value = crate::options::options_get_string_optional(oo, c"pane-scrollbars-style");
        (oe, value)
    });
    let oe = oe.unwrap_or_else(|| fatalx(|out| out.write_all(b"missing pane-scrollbars-style")));
    let style = format_single_cstring(
        None,
        oe.default_str.map_or(std::ptr::null(), CStr::as_ptr),
        None,
        None,
        (refbox::Weak::new()).clone(),
        None,
    );
    if style_parse(sb_style, &grid_default_cell, &style) != 0 as ::core::ffi::c_int {
        fatalx(|out| out.write_all(b"bad pane-scrollbars-style default"));
    }
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
    if sb_style.width < 1 as ::core::ffi::c_int {
        sb_style.width = PANE_SCROLLBARS_DEFAULT_WIDTH;
    }
    if sb_style.pad < 0 as ::core::ffi::c_int {
        sb_style.pad = PANE_SCROLLBARS_DEFAULT_PADDING;
    }
    utf8_set(&mut sb_style.gc.data, PANE_SCROLLBARS_CHARACTER as u_char);
}
pub unsafe fn style_ranges_init(srs: &mut style_ranges) {
    *srs = Vec::new();
}
pub unsafe fn style_ranges_clear(srs: &mut style_ranges) {
    srs.clear();
}

#[cfg(test)]
mod style_ranges_tests {
    use super::{style_range, style_ranges, STYLE_RANGE_LEFT};
    use crate::shared::abi::u_int;

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
}
