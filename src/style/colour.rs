use crate::src::compat::strtonum::strtonum;
use crate::src::ffi::libm::round;
use crate::src::log::log_cstr;
use crate::src::options::options;
use crate::src::server_client::Client;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::colour::*;
use crate::src::shared::colour::{COLOUR_FLAG_256, COLOUR_FLAG_RGB, COLOUR_FLAG_THEME};
use crate::src::shared::tty::{TERM_256COLOURS, TERM_RGBCOLOURS};
use libc::{sscanf, strcasecmp, strcmp, strlen, strncasecmp};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_36 {
    pub name: &'static ::std::ffi::CStr,
    pub dark_option: &'static ::std::ffi::CStr,
    pub light_option: &'static ::std::ffi::CStr,
    pub terminal_colour: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_37 {
    pub name: &'static ::std::ffi::CStr,
    pub c: ::core::ffi::c_int,
}
const colour_theme_table: [C2RustUnnamed_36; 10] = [
    C2RustUnnamed_36 {
        name: c"themeblack",
        dark_option: c"dark-theme-black",
        light_option: c"light-theme-black",
        terminal_colour: 0 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themewhite",
        dark_option: c"dark-theme-white",
        light_option: c"light-theme-white",
        terminal_colour: 7 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themelightgrey",
        dark_option: c"dark-theme-light-grey",
        light_option: c"light-theme-light-grey",
        terminal_colour: 7 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themedarkgrey",
        dark_option: c"dark-theme-dark-grey",
        light_option: c"light-theme-dark-grey",
        terminal_colour: 0 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themegreen",
        dark_option: c"dark-theme-green",
        light_option: c"light-theme-green",
        terminal_colour: 2 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themeyellow",
        dark_option: c"dark-theme-yellow",
        light_option: c"light-theme-yellow",
        terminal_colour: 3 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themered",
        dark_option: c"dark-theme-red",
        light_option: c"light-theme-red",
        terminal_colour: 1 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themeblue",
        dark_option: c"dark-theme-blue",
        light_option: c"light-theme-blue",
        terminal_colour: 4 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"themecyan",
        dark_option: c"dark-theme-cyan",
        light_option: c"light-theme-cyan",
        terminal_colour: 6 as ::core::ffi::c_int,
    },
    C2RustUnnamed_36 {
        name: c"thememagenta",
        dark_option: c"dark-theme-magenta",
        light_option: c"light-theme-magenta",
        terminal_colour: 5 as ::core::ffi::c_int,
    },
];
pub fn colour_theme_option(n: u_int, theme: client_theme) -> Option<&'static std::ffi::CStr> {
    let entry = colour_theme_table.get(n as usize)?;
    Some(if theme == THEME_LIGHT {
        entry.light_option
    } else {
        entry.dark_option
    })
}
pub unsafe fn colour_theme_terminal_colour(mut n: u_int) -> ::core::ffi::c_int {
    if n as usize
        >= (::core::mem::size_of::<[C2RustUnnamed_36; 10]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_36>() as usize)
    {
        return 8 as ::core::ffi::c_int;
    }
    colour_theme_table[n as usize].terminal_colour
}
unsafe fn colour_dist_sq(
    mut R: ::core::ffi::c_int,
    mut G: ::core::ffi::c_int,
    mut B: ::core::ffi::c_int,
    mut r: ::core::ffi::c_int,
    mut g: ::core::ffi::c_int,
    mut b: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    (R - r) * (R - r) + (G - g) * (G - g) + (B - b) * (B - b)
}
unsafe fn colour_to_6cube(mut v: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if v < 48 as ::core::ffi::c_int {
        return 0 as ::core::ffi::c_int;
    }
    if v < 114 as ::core::ffi::c_int {
        return 1 as ::core::ffi::c_int;
    }
    (v - 35 as ::core::ffi::c_int) / 40 as ::core::ffi::c_int
}
pub unsafe fn colour_find_rgb(mut r: u_char, mut g: u_char, mut b: u_char) -> ::core::ffi::c_int {
    const q2c: [::core::ffi::c_int; 6] = [
        0 as ::core::ffi::c_int,
        0x5f as ::core::ffi::c_int,
        0x87 as ::core::ffi::c_int,
        0xaf as ::core::ffi::c_int,
        0xd7 as ::core::ffi::c_int,
        0xff as ::core::ffi::c_int,
    ];
    let mut qr: ::core::ffi::c_int = 0;
    let mut qg: ::core::ffi::c_int = 0;
    let mut qb: ::core::ffi::c_int = 0;
    let mut cr: ::core::ffi::c_int = 0;
    let mut cg: ::core::ffi::c_int = 0;
    let mut cb: ::core::ffi::c_int = 0;
    let mut d: ::core::ffi::c_int = 0;
    let mut idx: ::core::ffi::c_int = 0;
    let mut grey_avg: ::core::ffi::c_int = 0;
    let mut grey_idx: ::core::ffi::c_int = 0;
    let mut grey: ::core::ffi::c_int = 0;
    qr = colour_to_6cube(r as ::core::ffi::c_int);
    cr = q2c[qr as usize];
    qg = colour_to_6cube(g as ::core::ffi::c_int);
    cg = q2c[qg as usize];
    qb = colour_to_6cube(b as ::core::ffi::c_int);
    cb = q2c[qb as usize];
    if cr == r as ::core::ffi::c_int
        && cg == g as ::core::ffi::c_int
        && cb == b as ::core::ffi::c_int
    {
        return (16 as ::core::ffi::c_int
            + 36 as ::core::ffi::c_int * qr
            + 6 as ::core::ffi::c_int * qg
            + qb)
            | COLOUR_FLAG_256;
    }
    grey_avg = (r as ::core::ffi::c_int + g as ::core::ffi::c_int + b as ::core::ffi::c_int)
        / 3 as ::core::ffi::c_int;
    if grey_avg > 238 as ::core::ffi::c_int {
        grey_idx = 23 as ::core::ffi::c_int;
    } else {
        grey_idx = (grey_avg - 3 as ::core::ffi::c_int) / 10 as ::core::ffi::c_int;
    }
    grey = 8 as ::core::ffi::c_int + 10 as ::core::ffi::c_int * grey_idx;
    d = colour_dist_sq(
        cr,
        cg,
        cb,
        r as ::core::ffi::c_int,
        g as ::core::ffi::c_int,
        b as ::core::ffi::c_int,
    );
    if colour_dist_sq(
        grey,
        grey,
        grey,
        r as ::core::ffi::c_int,
        g as ::core::ffi::c_int,
        b as ::core::ffi::c_int,
    ) < d
    {
        idx = 232 as ::core::ffi::c_int + grey_idx;
    } else {
        idx = 16 as ::core::ffi::c_int
            + 36 as ::core::ffi::c_int * qr
            + 6 as ::core::ffi::c_int * qg
            + qb;
    }
    idx | COLOUR_FLAG_256
}
pub unsafe fn colour_join_rgb(mut r: u_char, mut g: u_char, mut b: u_char) -> ::core::ffi::c_int {
    (r as ::core::ffi::c_int & 0xff as ::core::ffi::c_int) << 16 as ::core::ffi::c_int
        | (g as ::core::ffi::c_int & 0xff as ::core::ffi::c_int) << 8 as ::core::ffi::c_int
        | b as ::core::ffi::c_int & 0xff as ::core::ffi::c_int
        | COLOUR_FLAG_RGB
}
pub fn colour_split_rgb(colour: i32) -> (u_char, u_char, u_char) {
    (
        (colour >> 16 & 0xff) as u_char,
        (colour >> 8 & 0xff) as u_char,
        (colour & 0xff) as u_char,
    )
}
pub unsafe fn colour_force_rgb(mut c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if c & COLOUR_FLAG_RGB != 0 {
        return c;
    }
    if c & COLOUR_FLAG_256 != 0 {
        return colour_256toRGB(c);
    }
    if c >= 0 as ::core::ffi::c_int && c <= 7 as ::core::ffi::c_int {
        return colour_256toRGB(c);
    }
    if c >= 90 as ::core::ffi::c_int && c <= 97 as ::core::ffi::c_int {
        return colour_256toRGB(8 as ::core::ffi::c_int + c - 90 as ::core::ffi::c_int);
    }
    -(1 as ::core::ffi::c_int)
}
pub unsafe fn colour_dim(mut c: ::core::ffi::c_int, mut dim: u_int) -> ::core::ffi::c_int {
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    if dim == 0 as u_int
        || (c == 8 as ::core::ffi::c_int || c == 9 as ::core::ffi::c_int)
        || c & COLOUR_FLAG_THEME != 0
    {
        return c;
    }
    if dim >= 100 as u_int {
        return colour_join_rgb(0 as u_char, 0 as u_char, 0 as u_char);
    }
    c = colour_force_rgb(c);
    if c == -(1 as ::core::ffi::c_int) {
        return -(1 as ::core::ffi::c_int);
    }
    (r, g, b) = colour_split_rgb(c);
    r = (r as u_int)
        .wrapping_mul((100 as u_int).wrapping_sub(dim))
        .wrapping_div(100 as u_int) as u_char;
    g = (g as u_int)
        .wrapping_mul((100 as u_int).wrapping_sub(dim))
        .wrapping_div(100 as u_int) as u_char;
    b = (b as u_int)
        .wrapping_mul((100 as u_int).wrapping_sub(dim))
        .wrapping_div(100 as u_int) as u_char;
    colour_join_rgb(r, g, b)
}
const COLOUR_THEME_NAMES: &[&[u8]] = &[
    b"themeblack",
    b"themewhite",
    b"themelightgrey",
    b"themedarkgrey",
    b"themegreen",
    b"themeyellow",
    b"themered",
    b"themeblue",
    b"themecyan",
    b"thememagenta",
];

fn colour_basic_name(c: i32) -> &'static [u8] {
    match c {
        0 => b"black",
        1 => b"red",
        2 => b"green",
        3 => b"yellow",
        4 => b"blue",
        5 => b"magenta",
        6 => b"cyan",
        7 => b"white",
        8 => b"default",
        9 => b"terminal",
        90 => b"brightblack",
        91 => b"brightred",
        92 => b"brightgreen",
        93 => b"brightyellow",
        94 => b"brightblue",
        95 => b"brightmagenta",
        96 => b"brightcyan",
        97 => b"brightwhite",
        _ => b"invalid",
    }
}

fn copy_colour_text(output: &mut [u8], text: &[u8]) -> Option<usize> {
    if output.len() <= text.len() {
        return None;
    }
    output[..text.len()].copy_from_slice(text);
    output[text.len()] = 0;
    Some(text.len())
}

/// Format canonical colour text into a caller-owned NUL-terminated buffer.
///
/// The returned length excludes the trailing NUL. If `output` is too small,
/// this function returns `None` without modifying it. Formatting precedence is
/// sentinel, theme, RGB, indexed, then basic/bright colour.
pub fn colour_format_into(c: i32, output: &mut [u8]) -> Option<usize> {
    if c == -1 {
        return copy_colour_text(output, b"none");
    }
    if c & COLOUR_FLAG_THEME != 0 {
        return copy_colour_text(
            output,
            COLOUR_THEME_NAMES
                .get((c & 0xff) as usize)
                .copied()
                .unwrap_or(b"invalid"),
        );
    }
    if c & COLOUR_FLAG_RGB != 0 {
        if output.len() < 8 {
            return None;
        }
        let rgb = c as u32 & 0x00ff_ffff;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        output[0] = b'#';
        for digit in 0..6 {
            let shift = (5 - digit) * 4;
            output[digit + 1] = HEX[((rgb >> shift) & 0xf) as usize];
        }
        output[7] = 0;
        return Some(7);
    }
    if c & COLOUR_FLAG_256 != 0 {
        let index = (c & 0xff) as u32;
        let digits: usize = if index >= 100 {
            3
        } else if index >= 10 {
            2
        } else {
            1
        };
        let required = b"colour".len() + digits;
        if output.len() <= required {
            return None;
        }
        output[..b"colour".len()].copy_from_slice(b"colour");
        let mut divisor = 10_u32.pow((digits - 1) as u32);
        let mut position = b"colour".len();
        while divisor != 0 {
            output[position] = b'0' + ((index / divisor) % 10) as u8;
            position += 1;
            divisor /= 10;
        }
        output[position] = 0;
        return Some(position);
    }
    copy_colour_text(output, colour_basic_name(c))
}

/// Owned canonical colour text. Results remain valid across subsequent calls.
pub fn colour_format(c: i32) -> std::ffi::CString {
    let mut bytes = [0_u8; 32];
    let written = colour_format_into(c, &mut bytes).expect("sized colour buffer");
    std::ffi::CString::from_vec_with_nul(bytes[..=written].to_vec())
        .expect("colour names contain no NUL")
}

pub(crate) fn colour_format_escape_resolved(
    mut colour: i32,
    background: bool,
    flags: i32,
) -> Option<std::ffi::CString> {
    let offset = if background { 40 } else { 30 };
    if colour == 8 || colour == 9 {
        return Some(std::ffi::CString::new(format!("\x1b[{}m", offset + 9)).unwrap());
    }
    if !flags & TERM_RGBCOLOURS & (colour & COLOUR_FLAG_RGB) != 0 {
        colour =
            unsafe { colour_find_rgb((colour >> 16) as u8, (colour >> 8) as u8, colour as u8) };
    }
    if !flags & TERM_256COLOURS & (colour & COLOUR_FLAG_256) != 0 {
        colour = unsafe { colour_256to16(colour) };
    }
    let text = if colour & COLOUR_FLAG_RGB != 0 {
        format!(
            "\x1b[{};2;{};{};{}m",
            offset + 8,
            (colour >> 16) & 255,
            (colour >> 8) & 255,
            colour & 255
        )
    } else if colour & COLOUR_FLAG_256 != 0 {
        format!("\x1b[{};5;{}m", offset + 8, colour & 255)
    } else if (0..=7).contains(&colour) {
        format!("\x1b[{}m", colour + offset)
    } else if (90..=97).contains(&colour) {
        format!("\x1b[{}m", colour + offset - 30)
    } else {
        return None;
    };
    Some(std::ffi::CString::new(text).expect("SGR contains no NUL"))
}

/// Return owned SGR text through the retained client, or default capabilities.
pub unsafe fn colour_format_escape_for_client(
    client: Option<&ClientRef>,
    mut colour: i32,
    background: bool,
) -> Option<std::ffi::CString> {
    if let Some(client) = client {
        return client.colour_escape(colour, background);
    }
    if colour & COLOUR_FLAG_THEME != 0 {
        colour = colour_theme_terminal_colour((colour & 0xff) as u32);
    }
    colour_format_escape_resolved(colour, background, TERM_256COLOURS | TERM_RGBCOLOURS)
}

pub unsafe fn colour_totheme(mut c: ::core::ffi::c_int) -> client_theme {
    let mut r: ::core::ffi::c_int = 0;
    let mut g: ::core::ffi::c_int = 0;
    let mut b: ::core::ffi::c_int = 0;
    let mut brightness: ::core::ffi::c_int = 0;
    if c == -(1 as ::core::ffi::c_int) {
        return THEME_UNKNOWN;
    }
    if c & COLOUR_FLAG_RGB != 0 {
        r = c >> 16 as ::core::ffi::c_int & 0xff as ::core::ffi::c_int;
        g = c >> 8 as ::core::ffi::c_int & 0xff as ::core::ffi::c_int;
        b = c >> 0 as ::core::ffi::c_int & 0xff as ::core::ffi::c_int;
        brightness = r + g + b;
        if brightness > 382 as ::core::ffi::c_int {
            return THEME_LIGHT;
        }
        return THEME_DARK;
    }
    if c & COLOUR_FLAG_256 != 0 {
        return colour_totheme(colour_256toRGB(c));
    }
    match c {
        0 | 90 => return THEME_DARK,
        7 | 97 => return THEME_LIGHT,
        _ => {
            if c >= 0 as ::core::ffi::c_int && c <= 7 as ::core::ffi::c_int {
                return colour_totheme(colour_256toRGB(c));
            }
            if c >= 90 as ::core::ffi::c_int && c <= 97 as ::core::ffi::c_int {
                return colour_totheme(colour_256toRGB(
                    8 as ::core::ffi::c_int + c - 90 as ::core::ffi::c_int,
                ));
            }
        }
    }
    THEME_UNKNOWN
}
unsafe fn colour_fromstring_impl(input: &std::ffi::CStr) -> ::core::ffi::c_int {
    let s = input.as_ptr();
    let mut cp: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut n: ::core::ffi::c_int = 0;
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    let mut i: u_int = 0;
    if *s as ::core::ffi::c_int == '#' as i32 && strlen(s) == 7 as size_t {
        cp = s.offset(1 as ::core::ffi::c_int as isize);
        while ::libc::isxdigit(*cp as u_char as ::core::ffi::c_int) != 0 {
            cp = cp.offset(1);
        }
        if *cp as ::core::ffi::c_int != '\0' as i32 {
            return -(1 as ::core::ffi::c_int);
        }
        n = sscanf(
            s.offset(1 as ::core::ffi::c_int as isize),
            c"%2hhx%2hhx%2hhx".as_ptr(),
            &raw mut r,
            &raw mut g,
            &raw mut b,
        );
        if n != 3 as ::core::ffi::c_int {
            return -(1 as ::core::ffi::c_int);
        }
        return colour_join_rgb(r, g, b);
    }
    if strncasecmp(
        s,
        c"colour".as_ptr(),
        (::core::mem::size_of::<[::core::ffi::c_char; 7]>() as size_t).wrapping_sub(1 as size_t),
    ) == 0 as ::core::ffi::c_int
    {
        let parsed = strtonum(
            std::ffi::CStr::from_ptr(
                s.add(::core::mem::size_of::<[::core::ffi::c_char; 7]>() as usize)
                    .offset(-(1 as ::core::ffi::c_int as isize)),
            ),
            0 as ::core::ffi::c_longlong,
            255 as ::core::ffi::c_longlong,
        );
        n = parsed.unwrap_or(0) as ::core::ffi::c_int;
        if parsed.is_err() {
            return -(1 as ::core::ffi::c_int);
        }
        return n | COLOUR_FLAG_256;
    }
    if strncasecmp(
        s,
        c"color".as_ptr(),
        (::core::mem::size_of::<[::core::ffi::c_char; 6]>() as size_t).wrapping_sub(1 as size_t),
    ) == 0 as ::core::ffi::c_int
    {
        let parsed = strtonum(
            std::ffi::CStr::from_ptr(
                s.add(::core::mem::size_of::<[::core::ffi::c_char; 6]>() as usize)
                    .offset(-(1 as ::core::ffi::c_int as isize)),
            ),
            0 as ::core::ffi::c_longlong,
            255 as ::core::ffi::c_longlong,
        );
        n = parsed.unwrap_or(0) as ::core::ffi::c_int;
        if parsed.is_err() {
            return -(1 as ::core::ffi::c_int);
        }
        return n | COLOUR_FLAG_256;
    }
    if strcasecmp(s, c"default".as_ptr()) == 0 as ::core::ffi::c_int {
        return 8 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"terminal".as_ptr()) == 0 as ::core::ffi::c_int {
        return 9 as ::core::ffi::c_int;
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[C2RustUnnamed_36; 10]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_36>() as usize)
    {
        if strcasecmp(s, colour_theme_table[i as usize].name.as_ptr()) == 0 as ::core::ffi::c_int {
            return (i | COLOUR_FLAG_THEME as u_int) as ::core::ffi::c_int;
        }
        i = i.wrapping_add(1);
    }
    if strcasecmp(s, c"black".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"0".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 0 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"red".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"1".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 1 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"green".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"2".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 2 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"yellow".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"3".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 3 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"blue".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"4".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 4 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"magenta".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"5".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 5 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"cyan".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"6".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 6 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"white".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"7".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 7 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightblack".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"90".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 90 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightred".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"91".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 91 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightgreen".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"92".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 92 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightyellow".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"93".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 93 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightblue".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"94".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 94 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightmagenta".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"95".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 95 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightcyan".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"96".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 96 as ::core::ffi::c_int;
    }
    if strcasecmp(s, c"brightwhite".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(s, c"97".as_ptr()) == 0 as ::core::ffi::c_int
    {
        return 97 as ::core::ffi::c_int;
    }
    colour_byname_impl(input)
}
pub unsafe fn colour_256toRGB(mut c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    const RGB_TABLE: [::core::ffi::c_int; 256] = [
        0 as ::core::ffi::c_int,
        0x800000 as ::core::ffi::c_int,
        0x8000 as ::core::ffi::c_int,
        0x808000 as ::core::ffi::c_int,
        0x80 as ::core::ffi::c_int,
        0x800080 as ::core::ffi::c_int,
        0x8080 as ::core::ffi::c_int,
        0xc0c0c0 as ::core::ffi::c_int,
        0x808080 as ::core::ffi::c_int,
        0xff0000 as ::core::ffi::c_int,
        0xff00 as ::core::ffi::c_int,
        0xffff00 as ::core::ffi::c_int,
        0xff as ::core::ffi::c_int,
        0xff00ff as ::core::ffi::c_int,
        0xffff as ::core::ffi::c_int,
        0xffffff as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
        0x5f as ::core::ffi::c_int,
        0x87 as ::core::ffi::c_int,
        0xaf as ::core::ffi::c_int,
        0xd7 as ::core::ffi::c_int,
        0xff as ::core::ffi::c_int,
        0x5f00 as ::core::ffi::c_int,
        0x5f5f as ::core::ffi::c_int,
        0x5f87 as ::core::ffi::c_int,
        0x5faf as ::core::ffi::c_int,
        0x5fd7 as ::core::ffi::c_int,
        0x5fff as ::core::ffi::c_int,
        0x8700 as ::core::ffi::c_int,
        0x875f as ::core::ffi::c_int,
        0x8787 as ::core::ffi::c_int,
        0x87af as ::core::ffi::c_int,
        0x87d7 as ::core::ffi::c_int,
        0x87ff as ::core::ffi::c_int,
        0xaf00 as ::core::ffi::c_int,
        0xaf5f as ::core::ffi::c_int,
        0xaf87 as ::core::ffi::c_int,
        0xafaf as ::core::ffi::c_int,
        0xafd7 as ::core::ffi::c_int,
        0xafff as ::core::ffi::c_int,
        0xd700 as ::core::ffi::c_int,
        0xd75f as ::core::ffi::c_int,
        0xd787 as ::core::ffi::c_int,
        0xd7af as ::core::ffi::c_int,
        0xd7d7 as ::core::ffi::c_int,
        0xd7ff as ::core::ffi::c_int,
        0xff00 as ::core::ffi::c_int,
        0xff5f as ::core::ffi::c_int,
        0xff87 as ::core::ffi::c_int,
        0xffaf as ::core::ffi::c_int,
        0xffd7 as ::core::ffi::c_int,
        0xffff as ::core::ffi::c_int,
        0x5f0000 as ::core::ffi::c_int,
        0x5f005f as ::core::ffi::c_int,
        0x5f0087 as ::core::ffi::c_int,
        0x5f00af as ::core::ffi::c_int,
        0x5f00d7 as ::core::ffi::c_int,
        0x5f00ff as ::core::ffi::c_int,
        0x5f5f00 as ::core::ffi::c_int,
        0x5f5f5f as ::core::ffi::c_int,
        0x5f5f87 as ::core::ffi::c_int,
        0x5f5faf as ::core::ffi::c_int,
        0x5f5fd7 as ::core::ffi::c_int,
        0x5f5fff as ::core::ffi::c_int,
        0x5f8700 as ::core::ffi::c_int,
        0x5f875f as ::core::ffi::c_int,
        0x5f8787 as ::core::ffi::c_int,
        0x5f87af as ::core::ffi::c_int,
        0x5f87d7 as ::core::ffi::c_int,
        0x5f87ff as ::core::ffi::c_int,
        0x5faf00 as ::core::ffi::c_int,
        0x5faf5f as ::core::ffi::c_int,
        0x5faf87 as ::core::ffi::c_int,
        0x5fafaf as ::core::ffi::c_int,
        0x5fafd7 as ::core::ffi::c_int,
        0x5fafff as ::core::ffi::c_int,
        0x5fd700 as ::core::ffi::c_int,
        0x5fd75f as ::core::ffi::c_int,
        0x5fd787 as ::core::ffi::c_int,
        0x5fd7af as ::core::ffi::c_int,
        0x5fd7d7 as ::core::ffi::c_int,
        0x5fd7ff as ::core::ffi::c_int,
        0x5fff00 as ::core::ffi::c_int,
        0x5fff5f as ::core::ffi::c_int,
        0x5fff87 as ::core::ffi::c_int,
        0x5fffaf as ::core::ffi::c_int,
        0x5fffd7 as ::core::ffi::c_int,
        0x5fffff as ::core::ffi::c_int,
        0x870000 as ::core::ffi::c_int,
        0x87005f as ::core::ffi::c_int,
        0x870087 as ::core::ffi::c_int,
        0x8700af as ::core::ffi::c_int,
        0x8700d7 as ::core::ffi::c_int,
        0x8700ff as ::core::ffi::c_int,
        0x875f00 as ::core::ffi::c_int,
        0x875f5f as ::core::ffi::c_int,
        0x875f87 as ::core::ffi::c_int,
        0x875faf as ::core::ffi::c_int,
        0x875fd7 as ::core::ffi::c_int,
        0x875fff as ::core::ffi::c_int,
        0x878700 as ::core::ffi::c_int,
        0x87875f as ::core::ffi::c_int,
        0x878787 as ::core::ffi::c_int,
        0x8787af as ::core::ffi::c_int,
        0x8787d7 as ::core::ffi::c_int,
        0x8787ff as ::core::ffi::c_int,
        0x87af00 as ::core::ffi::c_int,
        0x87af5f as ::core::ffi::c_int,
        0x87af87 as ::core::ffi::c_int,
        0x87afaf as ::core::ffi::c_int,
        0x87afd7 as ::core::ffi::c_int,
        0x87afff as ::core::ffi::c_int,
        0x87d700 as ::core::ffi::c_int,
        0x87d75f as ::core::ffi::c_int,
        0x87d787 as ::core::ffi::c_int,
        0x87d7af as ::core::ffi::c_int,
        0x87d7d7 as ::core::ffi::c_int,
        0x87d7ff as ::core::ffi::c_int,
        0x87ff00 as ::core::ffi::c_int,
        0x87ff5f as ::core::ffi::c_int,
        0x87ff87 as ::core::ffi::c_int,
        0x87ffaf as ::core::ffi::c_int,
        0x87ffd7 as ::core::ffi::c_int,
        0x87ffff as ::core::ffi::c_int,
        0xaf0000 as ::core::ffi::c_int,
        0xaf005f as ::core::ffi::c_int,
        0xaf0087 as ::core::ffi::c_int,
        0xaf00af as ::core::ffi::c_int,
        0xaf00d7 as ::core::ffi::c_int,
        0xaf00ff as ::core::ffi::c_int,
        0xaf5f00 as ::core::ffi::c_int,
        0xaf5f5f as ::core::ffi::c_int,
        0xaf5f87 as ::core::ffi::c_int,
        0xaf5faf as ::core::ffi::c_int,
        0xaf5fd7 as ::core::ffi::c_int,
        0xaf5fff as ::core::ffi::c_int,
        0xaf8700 as ::core::ffi::c_int,
        0xaf875f as ::core::ffi::c_int,
        0xaf8787 as ::core::ffi::c_int,
        0xaf87af as ::core::ffi::c_int,
        0xaf87d7 as ::core::ffi::c_int,
        0xaf87ff as ::core::ffi::c_int,
        0xafaf00 as ::core::ffi::c_int,
        0xafaf5f as ::core::ffi::c_int,
        0xafaf87 as ::core::ffi::c_int,
        0xafafaf as ::core::ffi::c_int,
        0xafafd7 as ::core::ffi::c_int,
        0xafafff as ::core::ffi::c_int,
        0xafd700 as ::core::ffi::c_int,
        0xafd75f as ::core::ffi::c_int,
        0xafd787 as ::core::ffi::c_int,
        0xafd7af as ::core::ffi::c_int,
        0xafd7d7 as ::core::ffi::c_int,
        0xafd7ff as ::core::ffi::c_int,
        0xafff00 as ::core::ffi::c_int,
        0xafff5f as ::core::ffi::c_int,
        0xafff87 as ::core::ffi::c_int,
        0xafffaf as ::core::ffi::c_int,
        0xafffd7 as ::core::ffi::c_int,
        0xafffff as ::core::ffi::c_int,
        0xd70000 as ::core::ffi::c_int,
        0xd7005f as ::core::ffi::c_int,
        0xd70087 as ::core::ffi::c_int,
        0xd700af as ::core::ffi::c_int,
        0xd700d7 as ::core::ffi::c_int,
        0xd700ff as ::core::ffi::c_int,
        0xd75f00 as ::core::ffi::c_int,
        0xd75f5f as ::core::ffi::c_int,
        0xd75f87 as ::core::ffi::c_int,
        0xd75faf as ::core::ffi::c_int,
        0xd75fd7 as ::core::ffi::c_int,
        0xd75fff as ::core::ffi::c_int,
        0xd78700 as ::core::ffi::c_int,
        0xd7875f as ::core::ffi::c_int,
        0xd78787 as ::core::ffi::c_int,
        0xd787af as ::core::ffi::c_int,
        0xd787d7 as ::core::ffi::c_int,
        0xd787ff as ::core::ffi::c_int,
        0xd7af00 as ::core::ffi::c_int,
        0xd7af5f as ::core::ffi::c_int,
        0xd7af87 as ::core::ffi::c_int,
        0xd7afaf as ::core::ffi::c_int,
        0xd7afd7 as ::core::ffi::c_int,
        0xd7afff as ::core::ffi::c_int,
        0xd7d700 as ::core::ffi::c_int,
        0xd7d75f as ::core::ffi::c_int,
        0xd7d787 as ::core::ffi::c_int,
        0xd7d7af as ::core::ffi::c_int,
        0xd7d7d7 as ::core::ffi::c_int,
        0xd7d7ff as ::core::ffi::c_int,
        0xd7ff00 as ::core::ffi::c_int,
        0xd7ff5f as ::core::ffi::c_int,
        0xd7ff87 as ::core::ffi::c_int,
        0xd7ffaf as ::core::ffi::c_int,
        0xd7ffd7 as ::core::ffi::c_int,
        0xd7ffff as ::core::ffi::c_int,
        0xff0000 as ::core::ffi::c_int,
        0xff005f as ::core::ffi::c_int,
        0xff0087 as ::core::ffi::c_int,
        0xff00af as ::core::ffi::c_int,
        0xff00d7 as ::core::ffi::c_int,
        0xff00ff as ::core::ffi::c_int,
        0xff5f00 as ::core::ffi::c_int,
        0xff5f5f as ::core::ffi::c_int,
        0xff5f87 as ::core::ffi::c_int,
        0xff5faf as ::core::ffi::c_int,
        0xff5fd7 as ::core::ffi::c_int,
        0xff5fff as ::core::ffi::c_int,
        0xff8700 as ::core::ffi::c_int,
        0xff875f as ::core::ffi::c_int,
        0xff8787 as ::core::ffi::c_int,
        0xff87af as ::core::ffi::c_int,
        0xff87d7 as ::core::ffi::c_int,
        0xff87ff as ::core::ffi::c_int,
        0xffaf00 as ::core::ffi::c_int,
        0xffaf5f as ::core::ffi::c_int,
        0xffaf87 as ::core::ffi::c_int,
        0xffafaf as ::core::ffi::c_int,
        0xffafd7 as ::core::ffi::c_int,
        0xffafff as ::core::ffi::c_int,
        0xffd700 as ::core::ffi::c_int,
        0xffd75f as ::core::ffi::c_int,
        0xffd787 as ::core::ffi::c_int,
        0xffd7af as ::core::ffi::c_int,
        0xffd7d7 as ::core::ffi::c_int,
        0xffd7ff as ::core::ffi::c_int,
        0xffff00 as ::core::ffi::c_int,
        0xffff5f as ::core::ffi::c_int,
        0xffff87 as ::core::ffi::c_int,
        0xffffaf as ::core::ffi::c_int,
        0xffffd7 as ::core::ffi::c_int,
        0xffffff as ::core::ffi::c_int,
        0x80808 as ::core::ffi::c_int,
        0x121212 as ::core::ffi::c_int,
        0x1c1c1c as ::core::ffi::c_int,
        0x262626 as ::core::ffi::c_int,
        0x303030 as ::core::ffi::c_int,
        0x3a3a3a as ::core::ffi::c_int,
        0x444444 as ::core::ffi::c_int,
        0x4e4e4e as ::core::ffi::c_int,
        0x585858 as ::core::ffi::c_int,
        0x626262 as ::core::ffi::c_int,
        0x6c6c6c as ::core::ffi::c_int,
        0x767676 as ::core::ffi::c_int,
        0x808080 as ::core::ffi::c_int,
        0x8a8a8a as ::core::ffi::c_int,
        0x949494 as ::core::ffi::c_int,
        0x9e9e9e as ::core::ffi::c_int,
        0xa8a8a8 as ::core::ffi::c_int,
        0xb2b2b2 as ::core::ffi::c_int,
        0xbcbcbc as ::core::ffi::c_int,
        0xc6c6c6 as ::core::ffi::c_int,
        0xd0d0d0 as ::core::ffi::c_int,
        0xdadada as ::core::ffi::c_int,
        0xe4e4e4 as ::core::ffi::c_int,
        0xeeeeee as ::core::ffi::c_int,
    ];
    RGB_TABLE[(c & 0xff as ::core::ffi::c_int) as usize] | COLOUR_FLAG_RGB
}
pub unsafe fn colour_256to16(mut c: ::core::ffi::c_int) -> ::core::ffi::c_int {
    const ANSI_TABLE: [::core::ffi::c_char; 256] = [
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        4 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        2 as ::core::ffi::c_int as ::core::ffi::c_char,
        6 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        1 as ::core::ffi::c_int as ::core::ffi::c_char,
        5 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        3 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        12 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        10 as ::core::ffi::c_int as ::core::ffi::c_char,
        14 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        9 as ::core::ffi::c_int as ::core::ffi::c_char,
        13 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        11 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        0 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        8 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        7 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
        15 as ::core::ffi::c_int as ::core::ffi::c_char,
    ];
    ANSI_TABLE[(c & 0xff as ::core::ffi::c_int) as usize] as ::core::ffi::c_int
}
unsafe fn colour_byname_impl(name: &std::ffi::CStr) -> ::core::ffi::c_int {
    let name = name.as_ptr();
    const colours: [C2RustUnnamed_37; 578] = [
        C2RustUnnamed_37 {
            name: c"AliceBlue",
            c: 0xf0f8ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"AntiqueWhite",
            c: 0xfaebd7 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"AntiqueWhite1",
            c: 0xffefdb as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"AntiqueWhite2",
            c: 0xeedfcc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"AntiqueWhite3",
            c: 0xcdc0b0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"AntiqueWhite4",
            c: 0x8b8378 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"BlanchedAlmond",
            c: 0xffebcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"BlueViolet",
            c: 0x8a2be2 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"CadetBlue",
            c: 0x5f9ea0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"CadetBlue1",
            c: 0x98f5ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"CadetBlue2",
            c: 0x8ee5ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"CadetBlue3",
            c: 0x7ac5cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"CadetBlue4",
            c: 0x53868b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"CornflowerBlue",
            c: 0x6495ed as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkBlue",
            c: 0x8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkCyan",
            c: 0x8b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGoldenrod",
            c: 0xb8860b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGoldenrod1",
            c: 0xffb90f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGoldenrod2",
            c: 0xeead0e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGoldenrod3",
            c: 0xcd950c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGoldenrod4",
            c: 0x8b6508 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGray",
            c: 0xa9a9a9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGreen",
            c: 0x6400 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkGrey",
            c: 0xa9a9a9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkKhaki",
            c: 0xbdb76b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkMagenta",
            c: 0x8b008b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOliveGreen",
            c: 0x556b2f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOliveGreen1",
            c: 0xcaff70 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOliveGreen2",
            c: 0xbcee68 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOliveGreen3",
            c: 0xa2cd5a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOliveGreen4",
            c: 0x6e8b3d as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrange",
            c: 0xff8c00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrange1",
            c: 0xff7f00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrange2",
            c: 0xee7600 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrange3",
            c: 0xcd6600 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrange4",
            c: 0x8b4500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrchid",
            c: 0x9932cc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrchid1",
            c: 0xbf3eff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrchid2",
            c: 0xb23aee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrchid3",
            c: 0x9a32cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkOrchid4",
            c: 0x68228b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkRed",
            c: 0x8b0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSalmon",
            c: 0xe9967a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSeaGreen",
            c: 0x8fbc8f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSeaGreen1",
            c: 0xc1ffc1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSeaGreen2",
            c: 0xb4eeb4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSeaGreen3",
            c: 0x9bcd9b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSeaGreen4",
            c: 0x698b69 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateBlue",
            c: 0x483d8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateGray",
            c: 0x2f4f4f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateGray1",
            c: 0x97ffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateGray2",
            c: 0x8deeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateGray3",
            c: 0x79cdcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateGray4",
            c: 0x528b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkSlateGrey",
            c: 0x2f4f4f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkTurquoise",
            c: 0xced1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DarkViolet",
            c: 0x9400d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepPink",
            c: 0xff1493 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepPink1",
            c: 0xff1493 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepPink2",
            c: 0xee1289 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepPink3",
            c: 0xcd1076 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepPink4",
            c: 0x8b0a50 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepSkyBlue",
            c: 0xbfff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepSkyBlue1",
            c: 0xbfff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepSkyBlue2",
            c: 0xb2ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepSkyBlue3",
            c: 0x9acd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DeepSkyBlue4",
            c: 0x688b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DimGray",
            c: 0x696969 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DimGrey",
            c: 0x696969 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DodgerBlue",
            c: 0x1e90ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DodgerBlue1",
            c: 0x1e90ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DodgerBlue2",
            c: 0x1c86ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DodgerBlue3",
            c: 0x1874cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"DodgerBlue4",
            c: 0x104e8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"FloralWhite",
            c: 0xfffaf0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ForestGreen",
            c: 0x228b22 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"GhostWhite",
            c: 0xf8f8ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"GreenYellow",
            c: 0xadff2f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"HotPink",
            c: 0xff69b4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"HotPink1",
            c: 0xff6eb4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"HotPink2",
            c: 0xee6aa7 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"HotPink3",
            c: 0xcd6090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"HotPink4",
            c: 0x8b3a62 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"IndianRed",
            c: 0xcd5c5c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"IndianRed1",
            c: 0xff6a6a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"IndianRed2",
            c: 0xee6363 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"IndianRed3",
            c: 0xcd5555 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"IndianRed4",
            c: 0x8b3a3a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LavenderBlush",
            c: 0xfff0f5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LavenderBlush1",
            c: 0xfff0f5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LavenderBlush2",
            c: 0xeee0e5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LavenderBlush3",
            c: 0xcdc1c5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LavenderBlush4",
            c: 0x8b8386 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LawnGreen",
            c: 0x7cfc00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LemonChiffon",
            c: 0xfffacd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LemonChiffon1",
            c: 0xfffacd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LemonChiffon2",
            c: 0xeee9bf as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LemonChiffon3",
            c: 0xcdc9a5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LemonChiffon4",
            c: 0x8b8970 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightBlue",
            c: 0xadd8e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightBlue1",
            c: 0xbfefff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightBlue2",
            c: 0xb2dfee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightBlue3",
            c: 0x9ac0cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightBlue4",
            c: 0x68838b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightCoral",
            c: 0xf08080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightCyan",
            c: 0xe0ffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightCyan1",
            c: 0xe0ffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightCyan2",
            c: 0xd1eeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightCyan3",
            c: 0xb4cdcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightCyan4",
            c: 0x7a8b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGoldenrod",
            c: 0xeedd82 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGoldenrod1",
            c: 0xffec8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGoldenrod2",
            c: 0xeedc82 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGoldenrod3",
            c: 0xcdbe70 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGoldenrod4",
            c: 0x8b814c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGoldenrodYellow",
            c: 0xfafad2 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGray",
            c: 0xd3d3d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGreen",
            c: 0x90ee90 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightGrey",
            c: 0xd3d3d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightPink",
            c: 0xffb6c1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightPink1",
            c: 0xffaeb9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightPink2",
            c: 0xeea2ad as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightPink3",
            c: 0xcd8c95 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightPink4",
            c: 0x8b5f65 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSalmon",
            c: 0xffa07a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSalmon1",
            c: 0xffa07a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSalmon2",
            c: 0xee9572 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSalmon3",
            c: 0xcd8162 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSalmon4",
            c: 0x8b5742 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSeaGreen",
            c: 0x20b2aa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSkyBlue",
            c: 0x87cefa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSkyBlue1",
            c: 0xb0e2ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSkyBlue2",
            c: 0xa4d3ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSkyBlue3",
            c: 0x8db6cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSkyBlue4",
            c: 0x607b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSlateBlue",
            c: 0x8470ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSlateGray",
            c: 0x778899 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSlateGrey",
            c: 0x778899 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSteelBlue",
            c: 0xb0c4de as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSteelBlue1",
            c: 0xcae1ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSteelBlue2",
            c: 0xbcd2ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSteelBlue3",
            c: 0xa2b5cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightSteelBlue4",
            c: 0x6e7b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightYellow",
            c: 0xffffe0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightYellow1",
            c: 0xffffe0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightYellow2",
            c: 0xeeeed1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightYellow3",
            c: 0xcdcdb4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LightYellow4",
            c: 0x8b8b7a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"LimeGreen",
            c: 0x32cd32 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumAquamarine",
            c: 0x66cdaa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumBlue",
            c: 0xcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumOrchid",
            c: 0xba55d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumOrchid1",
            c: 0xe066ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumOrchid2",
            c: 0xd15fee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumOrchid3",
            c: 0xb452cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumOrchid4",
            c: 0x7a378b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumPurple",
            c: 0x9370db as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumPurple1",
            c: 0xab82ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumPurple2",
            c: 0x9f79ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumPurple3",
            c: 0x8968cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumPurple4",
            c: 0x5d478b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumSeaGreen",
            c: 0x3cb371 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumSlateBlue",
            c: 0x7b68ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumSpringGreen",
            c: 0xfa9a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumTurquoise",
            c: 0x48d1cc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MediumVioletRed",
            c: 0xc71585 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MidnightBlue",
            c: 0x191970 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MintCream",
            c: 0xf5fffa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MistyRose",
            c: 0xffe4e1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MistyRose1",
            c: 0xffe4e1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MistyRose2",
            c: 0xeed5d2 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MistyRose3",
            c: 0xcdb7b5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"MistyRose4",
            c: 0x8b7d7b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"NavajoWhite",
            c: 0xffdead as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"NavajoWhite1",
            c: 0xffdead as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"NavajoWhite2",
            c: 0xeecfa1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"NavajoWhite3",
            c: 0xcdb38b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"NavajoWhite4",
            c: 0x8b795e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"NavyBlue",
            c: 0x80 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OldLace",
            c: 0xfdf5e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OliveDrab",
            c: 0x6b8e23 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OliveDrab1",
            c: 0xc0ff3e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OliveDrab2",
            c: 0xb3ee3a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OliveDrab3",
            c: 0x9acd32 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OliveDrab4",
            c: 0x698b22 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OrangeRed",
            c: 0xff4500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OrangeRed1",
            c: 0xff4500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OrangeRed2",
            c: 0xee4000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OrangeRed3",
            c: 0xcd3700 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"OrangeRed4",
            c: 0x8b2500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleGoldenrod",
            c: 0xeee8aa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleGreen",
            c: 0x98fb98 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleGreen1",
            c: 0x9aff9a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleGreen2",
            c: 0x90ee90 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleGreen3",
            c: 0x7ccd7c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleGreen4",
            c: 0x548b54 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleTurquoise",
            c: 0xafeeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleTurquoise1",
            c: 0xbbffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleTurquoise2",
            c: 0xaeeeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleTurquoise3",
            c: 0x96cdcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleTurquoise4",
            c: 0x668b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleVioletRed",
            c: 0xdb7093 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleVioletRed1",
            c: 0xff82ab as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleVioletRed2",
            c: 0xee799f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleVioletRed3",
            c: 0xcd6889 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PaleVioletRed4",
            c: 0x8b475d as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PapayaWhip",
            c: 0xffefd5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PeachPuff",
            c: 0xffdab9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PeachPuff1",
            c: 0xffdab9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PeachPuff2",
            c: 0xeecbad as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PeachPuff3",
            c: 0xcdaf95 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PeachPuff4",
            c: 0x8b7765 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"PowderBlue",
            c: 0xb0e0e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RebeccaPurple",
            c: 0x663399 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RosyBrown",
            c: 0xbc8f8f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RosyBrown1",
            c: 0xffc1c1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RosyBrown2",
            c: 0xeeb4b4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RosyBrown3",
            c: 0xcd9b9b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RosyBrown4",
            c: 0x8b6969 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RoyalBlue",
            c: 0x4169e1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RoyalBlue1",
            c: 0x4876ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RoyalBlue2",
            c: 0x436eee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RoyalBlue3",
            c: 0x3a5fcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"RoyalBlue4",
            c: 0x27408b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SaddleBrown",
            c: 0x8b4513 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SandyBrown",
            c: 0xf4a460 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SeaGreen",
            c: 0x2e8b57 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SeaGreen1",
            c: 0x54ff9f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SeaGreen2",
            c: 0x4eee94 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SeaGreen3",
            c: 0x43cd80 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SeaGreen4",
            c: 0x2e8b57 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SkyBlue",
            c: 0x87ceeb as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SkyBlue1",
            c: 0x87ceff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SkyBlue2",
            c: 0x7ec0ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SkyBlue3",
            c: 0x6ca6cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SkyBlue4",
            c: 0x4a708b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateBlue",
            c: 0x6a5acd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateBlue1",
            c: 0x836fff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateBlue2",
            c: 0x7a67ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateBlue3",
            c: 0x6959cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateBlue4",
            c: 0x473c8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateGray",
            c: 0x708090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateGray1",
            c: 0xc6e2ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateGray2",
            c: 0xb9d3ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateGray3",
            c: 0x9fb6cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateGray4",
            c: 0x6c7b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SlateGrey",
            c: 0x708090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SpringGreen",
            c: 0xff7f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SpringGreen1",
            c: 0xff7f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SpringGreen2",
            c: 0xee76 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SpringGreen3",
            c: 0xcd66 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SpringGreen4",
            c: 0x8b45 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SteelBlue",
            c: 0x4682b4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SteelBlue1",
            c: 0x63b8ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SteelBlue2",
            c: 0x5cacee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SteelBlue3",
            c: 0x4f94cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"SteelBlue4",
            c: 0x36648b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"VioletRed",
            c: 0xd02090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"VioletRed1",
            c: 0xff3e96 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"VioletRed2",
            c: 0xee3a8c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"VioletRed3",
            c: 0xcd3278 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"VioletRed4",
            c: 0x8b2252 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"WebGray",
            c: 0x808080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"WebGreen",
            c: 0x8000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"WebGrey",
            c: 0x808080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"WebMaroon",
            c: 0x800000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"WebPurple",
            c: 0x800080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"WhiteSmoke",
            c: 0xf5f5f5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"X11Gray",
            c: 0xbebebe as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"X11Green",
            c: 0xff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"X11Grey",
            c: 0xbebebe as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"X11Maroon",
            c: 0xb03060 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"X11Purple",
            c: 0xa020f0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"YellowGreen",
            c: 0x9acd32 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"alice blue",
            c: 0xf0f8ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"antique white",
            c: 0xfaebd7 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"aqua",
            c: 0xffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"aquamarine",
            c: 0x7fffd4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"aquamarine1",
            c: 0x7fffd4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"aquamarine2",
            c: 0x76eec6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"aquamarine3",
            c: 0x66cdaa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"aquamarine4",
            c: 0x458b74 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"azure",
            c: 0xf0ffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"azure1",
            c: 0xf0ffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"azure2",
            c: 0xe0eeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"azure3",
            c: 0xc1cdcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"azure4",
            c: 0x838b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"beige",
            c: 0xf5f5dc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"bisque",
            c: 0xffe4c4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"bisque1",
            c: 0xffe4c4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"bisque2",
            c: 0xeed5b7 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"bisque3",
            c: 0xcdb79e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"bisque4",
            c: 0x8b7d6b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"black",
            c: 0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blanched almond",
            c: 0xffebcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blue violet",
            c: 0x8a2be2 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blue",
            c: 0xff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blue1",
            c: 0xff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blue2",
            c: 0xee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blue3",
            c: 0xcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"blue4",
            c: 0x8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"brown",
            c: 0xa52a2a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"brown1",
            c: 0xff4040 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"brown2",
            c: 0xee3b3b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"brown3",
            c: 0xcd3333 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"brown4",
            c: 0x8b2323 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"burlywood",
            c: 0xdeb887 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"burlywood1",
            c: 0xffd39b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"burlywood2",
            c: 0xeec591 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"burlywood3",
            c: 0xcdaa7d as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"burlywood4",
            c: 0x8b7355 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cadet blue",
            c: 0x5f9ea0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chartreuse",
            c: 0x7fff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chartreuse1",
            c: 0x7fff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chartreuse2",
            c: 0x76ee00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chartreuse3",
            c: 0x66cd00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chartreuse4",
            c: 0x458b00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chocolate",
            c: 0xd2691e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chocolate1",
            c: 0xff7f24 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chocolate2",
            c: 0xee7621 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chocolate3",
            c: 0xcd661d as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"chocolate4",
            c: 0x8b4513 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"coral",
            c: 0xff7f50 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"coral1",
            c: 0xff7256 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"coral2",
            c: 0xee6a50 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"coral3",
            c: 0xcd5b45 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"coral4",
            c: 0x8b3e2f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cornflower blue",
            c: 0x6495ed as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cornsilk",
            c: 0xfff8dc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cornsilk1",
            c: 0xfff8dc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cornsilk2",
            c: 0xeee8cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cornsilk3",
            c: 0xcdc8b1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cornsilk4",
            c: 0x8b8878 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"crimson",
            c: 0xdc143c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cyan",
            c: 0xffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cyan1",
            c: 0xffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cyan2",
            c: 0xeeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cyan3",
            c: 0xcdcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"cyan4",
            c: 0x8b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark blue",
            c: 0x8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark cyan",
            c: 0x8b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark goldenrod",
            c: 0xb8860b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark gray",
            c: 0xa9a9a9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark green",
            c: 0x6400 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark grey",
            c: 0xa9a9a9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark khaki",
            c: 0xbdb76b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark magenta",
            c: 0x8b008b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark olive green",
            c: 0x556b2f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark orange",
            c: 0xff8c00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark orchid",
            c: 0x9932cc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark red",
            c: 0x8b0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark salmon",
            c: 0xe9967a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark sea green",
            c: 0x8fbc8f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark slate blue",
            c: 0x483d8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark slate gray",
            c: 0x2f4f4f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark slate grey",
            c: 0x2f4f4f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark turquoise",
            c: 0xced1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dark violet",
            c: 0x9400d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"deep pink",
            c: 0xff1493 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"deep sky blue",
            c: 0xbfff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dim gray",
            c: 0x696969 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dim grey",
            c: 0x696969 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"dodger blue",
            c: 0x1e90ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"firebrick",
            c: 0xb22222 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"firebrick1",
            c: 0xff3030 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"firebrick2",
            c: 0xee2c2c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"firebrick3",
            c: 0xcd2626 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"firebrick4",
            c: 0x8b1a1a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"floral white",
            c: 0xfffaf0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"forest green",
            c: 0x228b22 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"fuchsia",
            c: 0xff00ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"gainsboro",
            c: 0xdcdcdc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ghost white",
            c: 0xf8f8ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"gold",
            c: 0xffd700 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"gold1",
            c: 0xffd700 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"gold2",
            c: 0xeec900 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"gold3",
            c: 0xcdad00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"gold4",
            c: 0x8b7500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"goldenrod",
            c: 0xdaa520 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"goldenrod1",
            c: 0xffc125 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"goldenrod2",
            c: 0xeeb422 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"goldenrod3",
            c: 0xcd9b1d as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"goldenrod4",
            c: 0x8b6914 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"green yellow",
            c: 0xadff2f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"green",
            c: 0xff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"green1",
            c: 0xff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"green2",
            c: 0xee00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"green3",
            c: 0xcd00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"green4",
            c: 0x8b00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"honeydew",
            c: 0xf0fff0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"honeydew1",
            c: 0xf0fff0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"honeydew2",
            c: 0xe0eee0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"honeydew3",
            c: 0xc1cdc1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"honeydew4",
            c: 0x838b83 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"hot pink",
            c: 0xff69b4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"indian red",
            c: 0xcd5c5c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"indigo",
            c: 0x4b0082 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ivory",
            c: 0xfffff0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ivory1",
            c: 0xfffff0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ivory2",
            c: 0xeeeee0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ivory3",
            c: 0xcdcdc1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"ivory4",
            c: 0x8b8b83 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"khaki",
            c: 0xf0e68c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"khaki1",
            c: 0xfff68f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"khaki2",
            c: 0xeee685 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"khaki3",
            c: 0xcdc673 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"khaki4",
            c: 0x8b864e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"lavender blush",
            c: 0xfff0f5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"lavender",
            c: 0xe6e6fa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"lawn green",
            c: 0x7cfc00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"lemon chiffon",
            c: 0xfffacd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light blue",
            c: 0xadd8e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light coral",
            c: 0xf08080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light cyan",
            c: 0xe0ffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light goldenrod yellow",
            c: 0xfafad2 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light goldenrod",
            c: 0xeedd82 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light gray",
            c: 0xd3d3d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light green",
            c: 0x90ee90 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light grey",
            c: 0xd3d3d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light pink",
            c: 0xffb6c1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light salmon",
            c: 0xffa07a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light sea green",
            c: 0x20b2aa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light sky blue",
            c: 0x87cefa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light slate blue",
            c: 0x8470ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light slate gray",
            c: 0x778899 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light slate grey",
            c: 0x778899 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light steel blue",
            c: 0xb0c4de as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"light yellow",
            c: 0xffffe0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"lime green",
            c: 0x32cd32 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"lime",
            c: 0xff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"linen",
            c: 0xfaf0e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"magenta",
            c: 0xff00ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"magenta1",
            c: 0xff00ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"magenta2",
            c: 0xee00ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"magenta3",
            c: 0xcd00cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"magenta4",
            c: 0x8b008b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"maroon",
            c: 0xb03060 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"maroon1",
            c: 0xff34b3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"maroon2",
            c: 0xee30a7 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"maroon3",
            c: 0xcd2990 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"maroon4",
            c: 0x8b1c62 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium aquamarine",
            c: 0x66cdaa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium blue",
            c: 0xcd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium orchid",
            c: 0xba55d3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium purple",
            c: 0x9370db as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium sea green",
            c: 0x3cb371 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium slate blue",
            c: 0x7b68ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium spring green",
            c: 0xfa9a as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium turquoise",
            c: 0x48d1cc as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"medium violet red",
            c: 0xc71585 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"midnight blue",
            c: 0x191970 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"mint cream",
            c: 0xf5fffa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"misty rose",
            c: 0xffe4e1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"moccasin",
            c: 0xffe4b5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"navajo white",
            c: 0xffdead as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"navy blue",
            c: 0x80 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"navy",
            c: 0x80 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"old lace",
            c: 0xfdf5e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"olive drab",
            c: 0x6b8e23 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"olive",
            c: 0x808000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orange red",
            c: 0xff4500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orange",
            c: 0xffa500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orange1",
            c: 0xffa500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orange2",
            c: 0xee9a00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orange3",
            c: 0xcd8500 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orange4",
            c: 0x8b5a00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orchid",
            c: 0xda70d6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orchid1",
            c: 0xff83fa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orchid2",
            c: 0xee7ae9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orchid3",
            c: 0xcd69c9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"orchid4",
            c: 0x8b4789 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pale goldenrod",
            c: 0xeee8aa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pale green",
            c: 0x98fb98 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pale turquoise",
            c: 0xafeeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pale violet red",
            c: 0xdb7093 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"papaya whip",
            c: 0xffefd5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"peach puff",
            c: 0xffdab9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"peru",
            c: 0xcd853f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pink",
            c: 0xffc0cb as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pink1",
            c: 0xffb5c5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pink2",
            c: 0xeea9b8 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pink3",
            c: 0xcd919e as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"pink4",
            c: 0x8b636c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"plum",
            c: 0xdda0dd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"plum1",
            c: 0xffbbff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"plum2",
            c: 0xeeaeee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"plum3",
            c: 0xcd96cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"plum4",
            c: 0x8b668b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"powder blue",
            c: 0xb0e0e6 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"purple",
            c: 0xa020f0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"purple1",
            c: 0x9b30ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"purple2",
            c: 0x912cee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"purple3",
            c: 0x7d26cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"purple4",
            c: 0x551a8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"rebecca purple",
            c: 0x663399 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"red",
            c: 0xff0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"red1",
            c: 0xff0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"red2",
            c: 0xee0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"red3",
            c: 0xcd0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"red4",
            c: 0x8b0000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"rosy brown",
            c: 0xbc8f8f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"royal blue",
            c: 0x4169e1 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"saddle brown",
            c: 0x8b4513 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"salmon",
            c: 0xfa8072 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"salmon1",
            c: 0xff8c69 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"salmon2",
            c: 0xee8262 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"salmon3",
            c: 0xcd7054 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"salmon4",
            c: 0x8b4c39 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sandy brown",
            c: 0xf4a460 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sea green",
            c: 0x2e8b57 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"seashell",
            c: 0xfff5ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"seashell1",
            c: 0xfff5ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"seashell2",
            c: 0xeee5de as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"seashell3",
            c: 0xcdc5bf as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"seashell4",
            c: 0x8b8682 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sienna",
            c: 0xa0522d as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sienna1",
            c: 0xff8247 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sienna2",
            c: 0xee7942 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sienna3",
            c: 0xcd6839 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sienna4",
            c: 0x8b4726 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"silver",
            c: 0xc0c0c0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"sky blue",
            c: 0x87ceeb as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"slate blue",
            c: 0x6a5acd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"slate gray",
            c: 0x708090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"slate grey",
            c: 0x708090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"snow",
            c: 0xfffafa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"snow1",
            c: 0xfffafa as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"snow2",
            c: 0xeee9e9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"snow3",
            c: 0xcdc9c9 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"snow4",
            c: 0x8b8989 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"spring green",
            c: 0xff7f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"steel blue",
            c: 0x4682b4 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tan",
            c: 0xd2b48c as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tan1",
            c: 0xffa54f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tan2",
            c: 0xee9a49 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tan3",
            c: 0xcd853f as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tan4",
            c: 0x8b5a2b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"teal",
            c: 0x8080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"thistle",
            c: 0xd8bfd8 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"thistle1",
            c: 0xffe1ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"thistle2",
            c: 0xeed2ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"thistle3",
            c: 0xcdb5cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"thistle4",
            c: 0x8b7b8b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tomato",
            c: 0xff6347 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tomato1",
            c: 0xff6347 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tomato2",
            c: 0xee5c42 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tomato3",
            c: 0xcd4f39 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"tomato4",
            c: 0x8b3626 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"turquoise",
            c: 0x40e0d0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"turquoise1",
            c: 0xf5ff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"turquoise2",
            c: 0xe5ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"turquoise3",
            c: 0xc5cd as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"turquoise4",
            c: 0x868b as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"violet red",
            c: 0xd02090 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"violet",
            c: 0xee82ee as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"web gray",
            c: 0x808080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"web green",
            c: 0x8000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"web grey",
            c: 0x808080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"web maroon",
            c: 0x800000 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"web purple",
            c: 0x800080 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"wheat",
            c: 0xf5deb3 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"wheat1",
            c: 0xffe7ba as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"wheat2",
            c: 0xeed8ae as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"wheat3",
            c: 0xcdba96 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"wheat4",
            c: 0x8b7e66 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"white smoke",
            c: 0xf5f5f5 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"white",
            c: 0xffffff as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"x11 gray",
            c: 0xbebebe as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"x11 green",
            c: 0xff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"x11 grey",
            c: 0xbebebe as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"x11 maroon",
            c: 0xb03060 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"x11 purple",
            c: 0xa020f0 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"yellow green",
            c: 0x9acd32 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"yellow",
            c: 0xffff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"yellow1",
            c: 0xffff00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"yellow2",
            c: 0xeeee00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"yellow3",
            c: 0xcdcd00 as ::core::ffi::c_int,
        },
        C2RustUnnamed_37 {
            name: c"yellow4",
            c: 0x8b8b00 as ::core::ffi::c_int,
        },
    ];
    let mut i: u_int = 0;
    let mut c: ::core::ffi::c_int = 0;
    if strncasecmp(name, c"grey".as_ptr(), 4 as size_t) == 0 as ::core::ffi::c_int
        || strncasecmp(name, c"gray".as_ptr(), 4 as size_t) == 0 as ::core::ffi::c_int
    {
        if *name.offset(4 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '\0' as i32 {
            return 0xbebebe as ::core::ffi::c_int | COLOUR_FLAG_RGB;
        }
        let parsed = strtonum(
            std::ffi::CStr::from_ptr(name.offset(4 as ::core::ffi::c_int as isize)),
            0 as ::core::ffi::c_longlong,
            100 as ::core::ffi::c_longlong,
        );
        c = parsed.unwrap_or(0) as ::core::ffi::c_int;
        if parsed.is_err() {
            return -(1 as ::core::ffi::c_int);
        }
        c = round(2.55f64 * c as ::core::ffi::c_double) as ::core::ffi::c_int;
        if c < 0 as ::core::ffi::c_int || c > 255 as ::core::ffi::c_int {
            return -(1 as ::core::ffi::c_int);
        }
        return colour_join_rgb(c as u_char, c as u_char, c as u_char);
    }
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[C2RustUnnamed_37; 578]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_37>() as usize)
    {
        if strcasecmp(colours[i as usize].name.as_ptr(), name) == 0 as ::core::ffi::c_int {
            return colours[i as usize].c | COLOUR_FLAG_RGB;
        }
        i = i.wrapping_add(1);
    }
    -(1 as ::core::ffi::c_int)
}
unsafe fn colour_parseX11_impl(input: &std::ffi::CStr) -> ::core::ffi::c_int {
    let p = input.as_ptr();
    let mut c: ::core::ffi::c_double = 0.;
    let mut m: ::core::ffi::c_double = 0.;
    let mut y: ::core::ffi::c_double = 0.;
    let mut k: ::core::ffi::c_double = 0 as ::core::ffi::c_int as ::core::ffi::c_double;
    let mut r: u_int = 0;
    let mut g: u_int = 0;
    let mut b: u_int = 0;
    let mut len: size_t = input.to_bytes().len();
    let mut colour: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    if len == 12 as size_t
        && sscanf(
            p,
            c"rgb:%02x/%02x/%02x".as_ptr(),
            &raw mut r,
            &raw mut g,
            &raw mut b,
        ) == 3 as ::core::ffi::c_int
        || len == 7 as size_t
            && sscanf(
                p,
                c"#%02x%02x%02x".as_ptr(),
                &raw mut r,
                &raw mut g,
                &raw mut b,
            ) == 3 as ::core::ffi::c_int
        || sscanf(p, c"%d,%d,%d".as_ptr(), &raw mut r, &raw mut g, &raw mut b)
            == 3 as ::core::ffi::c_int
    {
        colour = colour_join_rgb(r as u_char, g as u_char, b as u_char);
    } else if len == 18 as size_t
        && sscanf(
            p,
            c"rgb:%04x/%04x/%04x".as_ptr(),
            &raw mut r,
            &raw mut g,
            &raw mut b,
        ) == 3 as ::core::ffi::c_int
        || len == 13 as size_t
            && sscanf(
                p,
                c"#%04x%04x%04x".as_ptr(),
                &raw mut r,
                &raw mut g,
                &raw mut b,
            ) == 3 as ::core::ffi::c_int
    {
        colour = colour_join_rgb(
            (r >> 8 as ::core::ffi::c_int) as u_char,
            (g >> 8 as ::core::ffi::c_int) as u_char,
            (b >> 8 as ::core::ffi::c_int) as u_char,
        );
    } else if (sscanf(
        p,
        c"cmyk:%lf/%lf/%lf/%lf".as_ptr(),
        &raw mut c,
        &raw mut m,
        &raw mut y,
        &raw mut k,
    ) == 4 as ::core::ffi::c_int
        || sscanf(
            p,
            c"cmy:%lf/%lf/%lf".as_ptr(),
            &raw mut c,
            &raw mut m,
            &raw mut y,
        ) == 3 as ::core::ffi::c_int)
        && c >= 0 as ::core::ffi::c_int as ::core::ffi::c_double
        && c <= 1 as ::core::ffi::c_int as ::core::ffi::c_double
        && m >= 0 as ::core::ffi::c_int as ::core::ffi::c_double
        && m <= 1 as ::core::ffi::c_int as ::core::ffi::c_double
        && y >= 0 as ::core::ffi::c_int as ::core::ffi::c_double
        && y <= 1 as ::core::ffi::c_int as ::core::ffi::c_double
        && k >= 0 as ::core::ffi::c_int as ::core::ffi::c_double
        && k <= 1 as ::core::ffi::c_int as ::core::ffi::c_double
    {
        colour = colour_join_rgb(
            ((1 as ::core::ffi::c_int as ::core::ffi::c_double - c)
                * (1 as ::core::ffi::c_int as ::core::ffi::c_double - k)
                * 255 as ::core::ffi::c_int as ::core::ffi::c_double) as u_char,
            ((1 as ::core::ffi::c_int as ::core::ffi::c_double - m)
                * (1 as ::core::ffi::c_int as ::core::ffi::c_double - k)
                * 255 as ::core::ffi::c_int as ::core::ffi::c_double) as u_char,
            ((1 as ::core::ffi::c_int as ::core::ffi::c_double - y)
                * (1 as ::core::ffi::c_int as ::core::ffi::c_double - k)
                * 255 as ::core::ffi::c_int as ::core::ffi::c_double) as u_char,
        );
    } else {
        let bytes = input.to_bytes();
        let mut start = 0;
        while start < bytes.len() && bytes[start] == b' ' {
            start += 1;
        }
        len = bytes.len() - start;
        while len != 0 as size_t && bytes[start + len - 1] == b' ' {
            len = len.wrapping_sub(1);
        }
        let trimmed = &bytes[start..start + len];
        let copy = std::ffi::CString::new(trimmed).expect("trimmed C string contains NUL");
        colour = colour_byname_impl(&copy);
    }
    colour
}
pub fn colour_palette_init(p: &mut colour_palette) {
    *p = colour_palette {
        fg: 8,
        bg: 8,
        palette: None,
        default_palette: None,
    };
}
pub fn colour_palette_clear(p: Option<&mut colour_palette>) {
    if let Some(p) = p {
        p.fg = 8;
        p.bg = 8;
        p.palette = None;
    }
}
pub fn colour_palette_get(
    p: Option<&colour_palette>,
    mut n: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let Some(p) = p else {
        return -1;
    };
    if (90..=97).contains(&n) {
        n = 8 + n - 90;
    } else if n & COLOUR_FLAG_256 != 0 {
        n &= !COLOUR_FLAG_256;
    } else if n >= 8 {
        return -1;
    }
    let lookup = |palette: &Option<Box<[::core::ffi::c_int; 256]>>| {
        palette
            .as_ref()
            .and_then(|colours| colours.get(n as usize))
            .copied()
            .filter(|&colour| colour != -1)
    };
    lookup(&p.palette)
        .or_else(|| lookup(&p.default_palette))
        .unwrap_or(-1)
}
pub fn colour_palette_set(
    p: Option<&mut colour_palette>,
    n: ::core::ffi::c_int,
    c: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let Some(p) = p else {
        return 0;
    };
    if !(0..=255).contains(&n) || (c == -1 && p.palette.is_none()) {
        return 0;
    }
    let palette = p.palette.get_or_insert_with(|| Box::new([-1; 256]));
    palette[n as usize] = c;
    1
}
pub unsafe fn colour_palette_from_option(p: Option<&mut colour_palette>, oo: &options) {
    let Some(p) = p else {
        return;
    };
    let values = crate::src::options::options_read_entry(oo, c"pane-colours", |entry| {
        entry
            .value
            .array()
            .into_iter()
            .flat_map(|array| array.iter())
            .next()?;
        let mut palette = [-1; 256];
        for (index, colour) in palette.iter_mut().enumerate() {
            let key = std::ffi::CString::new(index.to_string()).unwrap();
            if let Some(value) = entry.value.array().and_then(|array| array.get(&key)) {
                *colour = value.number() as i32;
            }
        }
        Some(palette)
    })
    .expect("pane-colours option");
    if let Some(values) = values {
        **p.default_palette.get_or_insert_with(|| Box::new([-1; 256])) = values;
    } else {
        p.default_palette = None;
    }
}

/// Bounded C string entry point. Locale-sensitive operations remain in libc.
pub fn colour_parse_cstr(input: &std::ffi::CStr) -> Option<i32> {
    let value = unsafe { colour_fromstring_impl(input) };
    (value != -1).then_some(value)
}

/// Bounded C string entry point. Locale-sensitive operations remain in libc.
pub fn colour_parse_x11_cstr(input: &std::ffi::CStr) -> Option<i32> {
    let value = unsafe { colour_parseX11_impl(input) };
    (value != -1).then_some(value)
}

/// Application logging adapter; the bounded parser itself has no logging side effects.
/// # Safety
/// Call on the application thread, where the global debug log is managed.
pub unsafe fn colour_parse_x11_logged(input: &std::ffi::CStr) -> Option<i32> {
    let value = colour_parse_x11_cstr(input);
    let formatted = colour_format(value.unwrap_or(-1));
    crate::src::log::log_debug(format_args!(
        "{}: {} = {}",
        "colour_parseX11",
        log_cstr(input),
        log_cstr(&formatted)
    ));
    value
}
