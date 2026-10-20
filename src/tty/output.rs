//! Terminal output operates on the retained client holder. Each terminal
//! access ends before nested output, so no model borrow spans those operations.
use super::control::{tty_log, tty_start_write};
use super::model::*;
use crate::options::OptionsScope;
use tiri_rt::AsyncWrite as _;

use super::acs::{tty_acs_get, tty_acs_needed};
use crate::format::bytes::{format_cstring, format_message_with};
use crate::grid::{grid_cells_equal, grid_default_cell};
use crate::hyperlinks::hyperlinks_get;
use crate::log::{log_cstr, log_cstr_n, log_debug, log_get_level};
use crate::options::options_get_string;
use crate::reactor::evbuffer_add;
use crate::screen::screen_mode_display;
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::CLIENT_UTF8;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::colour::*;
use crate::shared::colour::{COLOUR_FLAG_256, COLOUR_FLAG_RGB, COLOUR_FLAG_THEME};
use crate::shared::display::*;
use crate::shared::grid::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::screen::{
    ScreenMode, ALL_MODES, ALL_MOUSE_MODES, CURSOR_MODES, MODE_CURSOR, MODE_CURSOR_BLINKING,
    MODE_CURSOR_BLINKING_SET, MODE_CURSOR_VERY_VISIBLE, MODE_MOUSE_ALL, MODE_MOUSE_BUTTON,
    MODE_MOUSE_STANDARD,
};
use crate::shared::tty::TTY_CTX_WRAPPED;
use crate::shared::tty::*;
use crate::style::colour::{
    colour_256to16, colour_dim, colour_find_rgb, colour_force_rgb, colour_palette_get,
    colour_split_rgb,
};
use crate::tty::term::tty_term_owner_ptr;
use crate::tty::term::{
    tty_term_flag, tty_term_has, tty_term_number, tty_term_string, tty_term_string_i,
    tty_term_string_ii, tty_term_string_iii, tty_term_string_s, tty_term_string_ss,
};
use libc::{abs, strcmp, strncmp};

use std::ffi::CStr;

/// Output retains its client holder and cannot run callbacks or replace
/// terminfo. The existing terminfo owner therefore stays alive after this
/// immediate component borrow has ended.
pub(super) unsafe fn terminal_term(owner: &ClientRef) -> &tty_term {
    let terminal = owner.borrow_terminal();
    let term = tty_term_owner_ptr(&terminal.term).expect("client has a terminal");
    std::ptr::NonNull::from(term).as_ref()
}

static mut tty_default_style_ctx: tty_style_ctx = {
    tty_style_ctx {
        defaults: grid_default_cell,
        palette: crate::shared::tty::PaletteSource::None,
        dim: 0 as u_int,
        hyperlinks: None,
    }
};

pub(super) unsafe fn tty_putcode(client: &ClientRef, mut code: tty_code_code) {
    tty_puts(client, tty_term_string(terminal_term(client), code));
}

pub(super) unsafe fn tty_putcode_i(
    client: &ClientRef,
    mut code: tty_code_code,
    mut a: ::core::ffi::c_int,
) {
    if a < 0 as ::core::ffi::c_int {
        return;
    }
    tty_puts(client, &tty_term_string_i(terminal_term(client), code, a));
}

pub(super) unsafe fn tty_putcode_ii(
    client: &ClientRef,
    mut code: tty_code_code,
    mut a: ::core::ffi::c_int,
    mut b: ::core::ffi::c_int,
) {
    if a < 0 as ::core::ffi::c_int || b < 0 as ::core::ffi::c_int {
        return;
    }
    tty_puts(
        client,
        &tty_term_string_ii(terminal_term(client), code, a, b),
    );
}

pub(super) unsafe fn tty_putcode_iii(
    client: &ClientRef,
    mut code: tty_code_code,
    mut a: ::core::ffi::c_int,
    mut b: ::core::ffi::c_int,
    mut c: ::core::ffi::c_int,
) {
    if a < 0 as ::core::ffi::c_int || b < 0 as ::core::ffi::c_int || c < 0 as ::core::ffi::c_int {
        return;
    }
    tty_puts(
        client,
        &tty_term_string_iii(terminal_term(client), code, a, b, c),
    );
}

pub(super) unsafe fn tty_putcode_s(client: &ClientRef, a: Option<&CStr>) {
    let mut code: tty_code_code = TTYC_CS;
    if let Some(a) = a {
        tty_puts(client, &tty_term_string_s(terminal_term(client), code, a));
    }
}

pub(super) unsafe fn tty_putcode_ss(
    client: &ClientRef,
    mut code: tty_code_code,
    a: Option<&CStr>,
    b: Option<&CStr>,
) {
    if let (Some(a), Some(b)) = (a, b) {
        tty_puts(
            client,
            &tty_term_string_ss(terminal_term(client), code, a, b),
        );
    }
}

pub(super) unsafe fn tty_add(client: &ClientRef, buf: &[u8]) {
    client.write_terminal(buf);
}

pub(crate) unsafe fn tty_enqueue_bytes(
    observer: ClientWeak,
    tty: &mut tty,
    name: Option<&CStr>,
    written: &mut usize,
    buf: &[u8],
) {
    let len = buf.len();
    if tty.flags & TTY_BLOCK != 0 {
        tty.discarded = tty.discarded.wrapping_add(len);
        return;
    }
    evbuffer_add(
        tty.out.as_deref_mut().expect("open TTY buffer"),
        &buf[..len],
    );
    log_debug(format_args!(
        "{}: {}",
        log_cstr(name.unwrap_or(c"(null)")),
        log_cstr_n(&buf[..len])
    ));
    *written = written.wrapping_add(len);
    if let Some((runtime, source)) = tty_log.as_mut() {
        let _ = runtime.block_on(source.write(&[std::io::IoSlice::new(buf)], None));
    }
    if tty.flags & TTY_STARTED != 0 {
        tty_start_write(tty, observer);
    }
}

pub unsafe fn tty_puts(client: &ClientRef, text: &CStr) {
    if !text.is_empty() {
        tty_add(client, text.to_bytes());
    }
}

pub(super) unsafe fn tty_putc(client: &ClientRef, mut ch: u_char) {
    if terminal_term(client).flags & TERM_NOAM != 0
        && ch as ::core::ffi::c_int >= 0x20 as ::core::ffi::c_int
        && ch as ::core::ffi::c_int != 0x7f as ::core::ffi::c_int
        && terminal_value!(client, cy) == terminal_value!(client, sy).wrapping_sub(1 as u_int)
        && terminal_value!(client, cx).wrapping_add(1 as u_int) >= terminal_value!(client, sx)
    {
        return;
    }
    if terminal_value!(client, cell.attr) as ::core::ffi::c_int & GRID_ATTR_CHARSET != 0 {
        let utf8 = client.flags() & CLIENT_UTF8 as u64 != 0;
        let acs = tty_acs_get(Some(client.borrow_terminal()), utf8, ch).map(ToOwned::to_owned);
        if let Some(acs) = acs {
            tty_add(client, acs.to_bytes());
        } else {
            tty_add(client, &[ch]);
        }
    } else {
        tty_add(client, &[ch]);
    }
    if ch as ::core::ffi::c_int >= 0x20 as ::core::ffi::c_int
        && ch as ::core::ffi::c_int != 0x7f as ::core::ffi::c_int
    {
        if terminal_value!(client, cx) >= terminal_value!(client, sx) {
            terminal_set!(client, cx, =, 1 as u_int);
            if terminal_value!(client, cy) != terminal_value!(client, rlower) {
                terminal_set!(client, cy, =, terminal_value!(client, cy).wrapping_add(1));
            }
            if terminal_term(client).flags & TERM_NOAM != 0 {
                tty_putcode_ii(
                    client,
                    TTYC_CUP,
                    terminal_value!(client, cy) as ::core::ffi::c_int,
                    terminal_value!(client, cx) as ::core::ffi::c_int,
                );
            }
        } else {
            terminal_set!(client, cx, =, terminal_value!(client, cx).wrapping_add(1));
        }
    }
}

pub(super) unsafe fn tty_putn(client: &ClientRef, buf: &[u8], mut width: u_int) {
    let mut len = buf.len();
    if terminal_term(client).flags & TERM_NOAM != 0
        && terminal_value!(client, cy) == terminal_value!(client, sy).wrapping_sub(1 as u_int)
        && (terminal_value!(client, cx) as size_t).wrapping_add(len)
            >= terminal_value!(client, sx) as size_t
    {
        len = terminal_value!(client, sx)
            .saturating_sub(terminal_value!(client, cx))
            .saturating_sub(1) as usize;
    }
    tty_add(client, &buf[..len]);
    if terminal_value!(client, cx).wrapping_add(width) > terminal_value!(client, sx) {
        terminal_set!(client, cx, =, terminal_value!(client, cx).wrapping_add(width).wrapping_sub(terminal_value!(client, sx)));
        if terminal_value!(client, cx) <= terminal_value!(client, sx) {
            terminal_set!(client, cy, =, terminal_value!(client, cy).wrapping_add(1));
        } else {
            terminal_set!(client, cy, =, UINT_MAX as u_int);
            terminal_set!(client, cx, =, terminal_value!(client, cy));
        }
    } else {
        terminal_set!(client, cx, =, terminal_value!(client, cx).wrapping_add(width));
    };
}

pub(super) unsafe fn tty_set_italics(client: &ClientRef) {
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if tty_term_has(terminal_term(client), TTYC_SITM) != 0 {
        let terminal =
            OptionsScope::GlobalServer.with_local(|oo| options_get_string(oo, c"default-terminal"));
        s = terminal.as_ptr();
        if strcmp(s, c"screen".as_ptr()) != 0 as ::core::ffi::c_int
            && strncmp(s, c"screen-".as_ptr(), 7 as size_t) != 0 as ::core::ffi::c_int
        {
            tty_putcode(client, TTYC_SITM);
            return;
        }
    }
    tty_putcode(client, TTYC_SMSO);
}

pub unsafe fn tty_set_title(client: &ClientRef, title: &CStr) {
    if tty_term_has(terminal_term(client), TTYC_TSL) == 0
        || tty_term_has(terminal_term(client), TTYC_FSL) == 0
    {
        return;
    }
    tty_putcode(client, TTYC_TSL);
    tty_puts(client, title);
    tty_putcode(client, TTYC_FSL);
}

pub unsafe fn tty_set_path(client: &ClientRef, title: &CStr) {
    if tty_term_has(terminal_term(client), TTYC_SWD) == 0
        || tty_term_has(terminal_term(client), TTYC_FSL) == 0
    {
        return;
    }
    tty_putcode(client, TTYC_SWD);
    tty_puts(client, title);
    tty_putcode(client, TTYC_FSL);
}

pub(super) unsafe fn tty_force_cursor_colour(client: &ClientRef, mut c: ::core::ffi::c_int) {
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    if c != -(1 as ::core::ffi::c_int) {
        c = tty_map_theme_colour(client, c);
        c = colour_force_rgb(c);
    }
    if c == terminal_value!(client, ccolour) {
        return;
    }
    if c == -(1 as ::core::ffi::c_int) {
        tty_putcode(client, TTYC_CR);
    } else {
        (r, g, b) = colour_split_rgb(c);
        let colour = format_cstring(format_args!("rgb:{r:02x}/{g:02x}/{b:02x}"))
            .expect("RGB colour contains no NUL");
        tty_putcode_s(client, Some(&*colour));
    }
    terminal_set!(client, ccolour, =, c);
}

pub(super) unsafe fn tty_update_cursor(
    client: &ClientRef,
    mut mode: ::core::ffi::c_int,
    s: Option<ScreenMode>,
) -> ::core::ffi::c_int {
    let mut cstyle: screen_cursor_style = SCREEN_CURSOR_DEFAULT;
    let mut ccolour: ::core::ffi::c_int = 0;
    let mut changed: ::core::ffi::c_int = 0;
    let mut cmode: ::core::ffi::c_int = mode;
    if let Some(s) = s {
        ccolour = s.ccolour;
        if s.ccolour == -(1 as ::core::ffi::c_int) {
            ccolour = s.default_ccolour;
        }
        tty_force_cursor_colour(client, ccolour);
    }
    if !cmode & MODE_CURSOR != 0 {
        if terminal_value!(client, mode) & MODE_CURSOR != 0 {
            tty_putcode(client, TTYC_CIVIS);
        }
        return cmode;
    }
    if let Some(s) = s {
        cstyle = s.cstyle;
        if cstyle as ::core::ffi::c_uint
            == SCREEN_CURSOR_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if !cmode & MODE_CURSOR_BLINKING_SET != 0 {
                if s.default_mode & MODE_CURSOR_BLINKING != 0 {
                    cmode |= MODE_CURSOR_BLINKING;
                } else {
                    cmode &= !MODE_CURSOR_BLINKING;
                }
            }
            cstyle = s.default_cstyle;
        }
    } else {
        cstyle = terminal_value!(client, cstyle);
    }
    changed = cmode ^ terminal_value!(client, mode);
    if changed & CURSOR_MODES == 0 as ::core::ffi::c_int
        && cstyle as ::core::ffi::c_uint == terminal_value!(client, cstyle) as ::core::ffi::c_uint
    {
        return cmode;
    }
    tty_putcode(client, TTYC_CNORM);
    match cstyle as ::core::ffi::c_uint {
        0 => {
            if terminal_value!(client, cstyle) as ::core::ffi::c_uint
                != SCREEN_CURSOR_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                if tty_term_has(terminal_term(client), TTYC_SE) != 0 {
                    tty_putcode(client, TTYC_SE);
                } else {
                    tty_putcode_i(client, TTYC_SS, 0 as ::core::ffi::c_int);
                }
            }
            if cmode & (MODE_CURSOR_BLINKING | MODE_CURSOR_VERY_VISIBLE) != 0 {
                tty_putcode(client, TTYC_CVVIS);
            }
        }
        1 => {
            if tty_term_has(terminal_term(client), TTYC_SS) != 0 {
                if cmode & MODE_CURSOR_BLINKING != 0 {
                    tty_putcode_i(client, TTYC_SS, 1 as ::core::ffi::c_int);
                } else {
                    tty_putcode_i(client, TTYC_SS, 2 as ::core::ffi::c_int);
                }
            } else if cmode & MODE_CURSOR_BLINKING != 0 {
                tty_putcode(client, TTYC_CVVIS);
            }
        }
        2 => {
            if tty_term_has(terminal_term(client), TTYC_SS) != 0 {
                if cmode & MODE_CURSOR_BLINKING != 0 {
                    tty_putcode_i(client, TTYC_SS, 3 as ::core::ffi::c_int);
                } else {
                    tty_putcode_i(client, TTYC_SS, 4 as ::core::ffi::c_int);
                }
            } else if cmode & MODE_CURSOR_BLINKING != 0 {
                tty_putcode(client, TTYC_CVVIS);
            }
        }
        3 => {
            if tty_term_has(terminal_term(client), TTYC_SS) != 0 {
                if cmode & MODE_CURSOR_BLINKING != 0 {
                    tty_putcode_i(client, TTYC_SS, 5 as ::core::ffi::c_int);
                } else {
                    tty_putcode_i(client, TTYC_SS, 6 as ::core::ffi::c_int);
                }
            } else if cmode & MODE_CURSOR_BLINKING != 0 {
                tty_putcode(client, TTYC_CVVIS);
            }
        }
        _ => {}
    }
    terminal_set!(client, cstyle, =, cstyle);
    cmode
}

pub unsafe fn tty_update_mode(
    client: &ClientRef,
    mut mode: ::core::ffi::c_int,
    s: Option<ScreenMode>,
) {
    let mut changed: ::core::ffi::c_int = 0;
    if terminal_value!(client, flags) & TTY_NOCURSOR != 0 {
        mode &= !MODE_CURSOR;
    }
    if tty_update_cursor(client, mode, s) & MODE_CURSOR_BLINKING != 0 {
        mode |= MODE_CURSOR_BLINKING;
    } else {
        mode &= !MODE_CURSOR_BLINKING;
    }
    changed = mode ^ terminal_value!(client, mode);
    if log_get_level() != 0 as ::core::ffi::c_int && changed != 0 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: current mode {}",
            log_cstr(client.name().as_deref().unwrap_or(c"(null)")),
            screen_mode_display(terminal_value!(client, mode))
        ));
        log_debug(format_args!(
            "{}: setting mode {}",
            log_cstr(client.name().as_deref().unwrap_or(c"(null)")),
            screen_mode_display(mode)
        ));
    }
    if changed & ALL_MOUSE_MODES != 0 && tty_term_has(terminal_term(client), TTYC_KMOUS) != 0 {
        tty_puts(client, c"\x1B[?1006l\x1B[?1000l\x1B[?1002l\x1B[?1003l");
        if mode & ALL_MOUSE_MODES != 0 {
            tty_puts(client, c"\x1B[?1006h");
        }
        if mode & MODE_MOUSE_ALL != 0 {
            tty_puts(client, c"\x1B[?1000h\x1B[?1002h\x1B[?1003h");
        } else if mode & MODE_MOUSE_BUTTON != 0 {
            tty_puts(client, c"\x1B[?1000h\x1B[?1002h");
        } else if mode & MODE_MOUSE_STANDARD != 0 {
            tty_puts(client, c"\x1B[?1000h");
        }
    }
    terminal_set!(client, mode, =, mode);
}

pub(super) unsafe fn tty_emulate_repeat(
    client: &ClientRef,
    mut code: tty_code_code,
    mut code1: tty_code_code,
    mut n: u_int,
) {
    if tty_term_has(terminal_term(client), code) != 0 {
        tty_putcode_i(client, code, n as ::core::ffi::c_int);
    } else {
        loop {
            let fresh0 = n;
            n = n.wrapping_sub(1);
            if !(fresh0 > 0 as u_int) {
                break;
            }
            tty_putcode(client, code1);
        }
    };
}

pub(super) unsafe fn tty_repeat_space(client: &ClientRef, mut n: u_int) {
    const SPACES: [u8; 500] = [b' '; 500];
    while n as usize > SPACES.len() {
        tty_putn(client, &SPACES, SPACES.len() as u_int);
        n -= SPACES.len() as u_int;
    }
    if n != 0 {
        tty_putn(client, &SPACES[..n as usize], n);
    }
}

pub unsafe fn tty_sync_start(client: &ClientRef) {
    if terminal_value!(client, flags) & TTY_BLOCK != 0 {
        return;
    }
    if terminal_value!(client, flags) & TTY_SYNCING != 0 {
        return;
    }
    terminal_set!(client, flags, |=, TTY_SYNCING);
    if tty_term_has(terminal_term(client), TTYC_SYNC) != 0 {
        log_debug(format_args!(
            "{} sync start",
            log_cstr(client.name().as_deref().unwrap_or(c"(null)"))
        ));
        tty_putcode_i(client, TTYC_SYNC, 1 as ::core::ffi::c_int);
    }
}

pub unsafe fn tty_sync_end(client: &ClientRef) {
    if terminal_value!(client, flags) & TTY_BLOCK != 0 {
        return;
    }
    if !terminal_value!(client, flags) & TTY_SYNCING != 0 {
        return;
    }
    terminal_set!(client, flags, &=, !TTY_SYNCING);
    if tty_term_has(terminal_term(client), TTYC_SYNC) != 0 {
        log_debug(format_args!(
            "{} sync end",
            log_cstr(client.name().as_deref().unwrap_or(c"(null)"))
        ));
        tty_putcode_i(client, TTYC_SYNC, 2 as ::core::ffi::c_int);
    }
}

pub unsafe fn tty_reset(client: &ClientRef) {
    let gc = terminal_value!(client, cell);
    if !grid_cells_equal(&gc, &grid_default_cell) {
        if gc.link != 0 {
            tty_putcode_ss(client, TTYC_HLS, Some(c""), Some(c""));
        }
        let utf8 = client.flags() & CLIENT_UTF8 as u64 != 0;
        if gc.attr as i32 & GRID_ATTR_CHARSET != 0
            && tty_acs_needed(Some(client.borrow_terminal()), utf8) != 0
        {
            tty_putcode(client, TTYC_RMACS);
        }
        tty_putcode(client, TTYC_SGR0);
        terminal_set!(client, cell, =, grid_default_cell);
    }
    terminal_set!(client, last_cell, =, grid_default_cell);
}

pub unsafe fn tty_invalidate(client: &ClientRef) {
    {
        let mut terminal = client.borrow_terminal_mut();
        terminal.cell = grid_default_cell;
        terminal.last_cell = grid_default_cell;
        terminal.cx = UINT_MAX;
        terminal.cy = UINT_MAX;
        terminal.rleft = UINT_MAX;
        terminal.rupper = UINT_MAX;
        terminal.rright = UINT_MAX;
        terminal.rlower = UINT_MAX;
    }
    if terminal_value!(client, flags) & TTY_STARTED != 0 {
        if terminal_term(client).flags & TERM_DECSLRM != 0 {
            tty_putcode(client, TTYC_ENMG);
        }
        tty_putcode(client, TTYC_SGR0);
        terminal_set!(client, mode, =, ALL_MODES);
        tty_update_mode(client, MODE_CURSOR, None);
        tty_cursor(client, 0, 0);
        tty_region_off(client);
        tty_margin_off(client);
    } else {
        terminal_set!(client, mode, =, MODE_CURSOR);
    }
}

pub unsafe fn tty_region_off(client: &ClientRef) {
    tty_region(
        client,
        0 as u_int,
        terminal_value!(client, sy).wrapping_sub(1 as u_int),
    );
}

pub(super) unsafe fn tty_region_pane(
    client: &ClientRef,
    ctx: &tty_ctx,
    mut rupper: u_int,
    mut rlower: u_int,
) {
    tty_region(
        client,
        (ctx.yoff as u_int)
            .wrapping_add(rupper)
            .wrapping_sub(ctx.woy),
        (ctx.yoff as u_int)
            .wrapping_add(rlower)
            .wrapping_sub(ctx.woy),
    );
}

pub(super) unsafe fn tty_region(client: &ClientRef, mut rupper: u_int, mut rlower: u_int) {
    if terminal_value!(client, rlower) == rlower && terminal_value!(client, rupper) == rupper {
        return;
    }
    if tty_term_has(terminal_term(client), TTYC_CSR) == 0 {
        return;
    }
    terminal_set!(client, rupper, =, rupper);
    terminal_set!(client, rlower, =, rlower);
    if terminal_value!(client, cx) >= terminal_value!(client, sx) {
        if terminal_value!(client, cy) == UINT_MAX {
            tty_cursor(client, 0 as u_int, 0 as u_int);
        } else {
            tty_cursor(client, 0 as u_int, terminal_value!(client, cy));
        }
    }
    tty_putcode_ii(
        client,
        TTYC_CSR,
        terminal_value!(client, rupper) as ::core::ffi::c_int,
        terminal_value!(client, rlower) as ::core::ffi::c_int,
    );
    terminal_set!(client, cy, =, UINT_MAX as u_int);
    terminal_set!(client, cx, =, terminal_value!(client, cy));
}

pub unsafe fn tty_margin_off(client: &ClientRef) {
    tty_margin(
        client,
        0 as u_int,
        terminal_value!(client, sx).wrapping_sub(1 as u_int),
    );
}

pub(super) unsafe fn tty_margin_pane(client: &ClientRef, ctx: &tty_ctx) {
    let mut l: ::core::ffi::c_int = 0;
    let mut r: ::core::ffi::c_int = 0;
    l = (ctx.xoff as u_int).wrapping_sub(ctx.wox) as ::core::ffi::c_int;
    r = (ctx.xoff as u_int)
        .wrapping_add(ctx.sx)
        .wrapping_sub(1 as u_int)
        .wrapping_sub(ctx.wox) as ::core::ffi::c_int;
    if l < 0 as ::core::ffi::c_int {
        l = 0 as ::core::ffi::c_int;
    }
    if l > ctx.wsx as ::core::ffi::c_int {
        l = ctx.wsx as ::core::ffi::c_int;
    }
    if r < 0 as ::core::ffi::c_int {
        r = 0 as ::core::ffi::c_int;
    }
    if r > ctx.wsx as ::core::ffi::c_int {
        r = ctx.wsx as ::core::ffi::c_int;
    }
    tty_margin(client, l as u_int, r as u_int);
}

pub(super) unsafe fn tty_margin(client: &ClientRef, mut rleft: u_int, mut rright: u_int) {
    if terminal_term(client).flags & TERM_DECSLRM == 0 {
        return;
    }
    if terminal_value!(client, rleft) == rleft && terminal_value!(client, rright) == rright {
        return;
    }
    tty_putcode_ii(
        client,
        TTYC_CSR,
        terminal_value!(client, rupper) as ::core::ffi::c_int,
        terminal_value!(client, rlower) as ::core::ffi::c_int,
    );
    terminal_set!(client, rleft, =, rleft);
    terminal_set!(client, rright, =, rright);
    if rleft == 0 as u_int && rright == terminal_value!(client, sx).wrapping_sub(1 as u_int) {
        tty_putcode(client, TTYC_CLMG);
    } else {
        tty_putcode_ii(
            client,
            TTYC_CMG,
            rleft as ::core::ffi::c_int,
            rright as ::core::ffi::c_int,
        );
    }
    terminal_set!(client, cy, =, UINT_MAX as u_int);
    terminal_set!(client, cx, =, terminal_value!(client, cy));
}

pub(super) unsafe fn tty_cursor_pane_unless_wrap(
    client: &ClientRef,
    ctx: &tty_ctx,
    mut cx: u_int,
    mut cy: u_int,
) {
    if !ctx.flags & TTY_CTX_WRAPPED != 0
        || !(ctx.xoff == 0 as ::core::ffi::c_int && ctx.sx >= terminal_value!(client, sx))
        || terminal_term(client).flags & TERM_NOAM != 0
        || (ctx.xoff as u_int).wrapping_add(cx) != 0 as u_int
        || (ctx.yoff as u_int).wrapping_add(cy)
            != terminal_value!(client, cy).wrapping_add(1 as u_int)
        || terminal_value!(client, cx) < terminal_value!(client, sx)
        || terminal_value!(client, cy) == terminal_value!(client, rlower)
    {
        tty_cursor_pane(client, ctx, cx, cy);
    } else {
        log_debug(format_args!(
            "{}: will wrap at {},{}",
            "tty_cursor_pane_unless_wrap",
            (terminal_value!(client, cx)),
            (terminal_value!(client, cy))
        ));
    };
}

pub(super) unsafe fn tty_cursor_pane(
    client: &ClientRef,
    ctx: &tty_ctx,
    mut cx: u_int,
    mut cy: u_int,
) {
    tty_cursor(
        client,
        (ctx.xoff as u_int).wrapping_add(cx).wrapping_sub(ctx.wox),
        (ctx.yoff as u_int).wrapping_add(cy).wrapping_sub(ctx.woy),
    );
}

pub unsafe fn tty_cursor(client: &ClientRef, mut cx: u_int, mut cy: u_int) {
    let mut current_block: u64;
    let mut thisx: u_int = 0;
    let mut thisy: u_int = 0;
    let mut change: ::core::ffi::c_int = 0;
    if terminal_value!(client, flags) & TTY_BLOCK != 0 {
        return;
    }
    thisx = terminal_value!(client, cx);
    thisy = terminal_value!(client, cy);
    if cx == thisx && cy == thisy && cx == terminal_value!(client, sx) {
        return;
    }
    if cx > terminal_value!(client, sx).wrapping_sub(1 as u_int) {
        log_debug(format_args!(
            "{}: x too big {} > {}",
            "tty_cursor",
            { cx },
            { terminal_value!(client, sx).wrapping_sub(1 as u_int) }
        ));
        cx = terminal_value!(client, sx).wrapping_sub(1 as u_int);
    }
    if cx == thisx && cy == thisy {
        return;
    }
    if thisx > terminal_value!(client, sx).wrapping_sub(1 as u_int) {
        current_block = 11555347550778173209;
    } else if cx == 0 as u_int
        && cy == 0 as u_int
        && tty_term_has(terminal_term(client), TTYC_HOME) != 0
    {
        tty_putcode(client, TTYC_HOME);
        current_block = 5411263895410993842;
    } else if cx == 0 as u_int
        && cy == thisy.wrapping_add(1 as u_int)
        && thisy != terminal_value!(client, rlower)
        && (terminal_term(client).flags & TERM_DECSLRM == 0
            || terminal_value!(client, rleft) == 0 as u_int)
    {
        tty_putc(client, '\r' as i32 as u_char);
        tty_putc(client, '\n' as i32 as u_char);
        current_block = 5411263895410993842;
    } else if cy == thisy {
        if cx == 0 as u_int
            && (terminal_term(client).flags & TERM_DECSLRM == 0
                || terminal_value!(client, rleft) == 0 as u_int)
        {
            tty_putc(client, '\r' as i32 as u_char);
            current_block = 5411263895410993842;
        } else if cx == thisx.wrapping_sub(1 as u_int)
            && tty_term_has(terminal_term(client), TTYC_CUB1) != 0
        {
            tty_putcode(client, TTYC_CUB1);
            current_block = 5411263895410993842;
        } else if cx == thisx.wrapping_add(1 as u_int)
            && tty_term_has(terminal_term(client), TTYC_CUF1) != 0
        {
            tty_putcode(client, TTYC_CUF1);
            current_block = 5411263895410993842;
        } else {
            change = thisx.wrapping_sub(cx) as ::core::ffi::c_int;
            if abs(change) as u_int > cx && tty_term_has(terminal_term(client), TTYC_HPA) != 0 {
                tty_putcode_i(client, TTYC_HPA, cx as ::core::ffi::c_int);
                current_block = 5411263895410993842;
            } else if change > 0 as ::core::ffi::c_int
                && tty_term_has(terminal_term(client), TTYC_CUB) != 0
                && terminal_term(client).flags & TERM_DECSLRM == 0
            {
                if change == 2 as ::core::ffi::c_int
                    && tty_term_has(terminal_term(client), TTYC_CUB1) != 0
                {
                    tty_putcode(client, TTYC_CUB1);
                    tty_putcode(client, TTYC_CUB1);
                } else {
                    tty_putcode_i(client, TTYC_CUB, change);
                }
                current_block = 5411263895410993842;
            } else if change < 0 as ::core::ffi::c_int
                && tty_term_has(terminal_term(client), TTYC_CUF) != 0
                && terminal_term(client).flags & TERM_DECSLRM == 0
            {
                tty_putcode_i(client, TTYC_CUF, -change);
                current_block = 5411263895410993842;
            } else {
                current_block = 11555347550778173209;
            }
        }
    } else if cx == thisx {
        if thisy != terminal_value!(client, rupper)
            && cy == thisy.wrapping_sub(1 as u_int)
            && tty_term_has(terminal_term(client), TTYC_CUU1) != 0
        {
            tty_putcode(client, TTYC_CUU1);
            current_block = 5411263895410993842;
        } else if thisy != terminal_value!(client, rlower)
            && cy == thisy.wrapping_add(1 as u_int)
            && tty_term_has(terminal_term(client), TTYC_CUD1) != 0
        {
            tty_putcode(client, TTYC_CUD1);
            current_block = 5411263895410993842;
        } else {
            change = thisy.wrapping_sub(cy) as ::core::ffi::c_int;
            if abs(change) as u_int > cy
                || change < 0 as ::core::ffi::c_int
                    && cy.wrapping_sub(change as u_int) > terminal_value!(client, rlower)
                || change > 0 as ::core::ffi::c_int
                    && cy.wrapping_sub(change as u_int) < terminal_value!(client, rupper)
            {
                if tty_term_has(terminal_term(client), TTYC_VPA) != 0 {
                    tty_putcode_i(client, TTYC_VPA, cy as ::core::ffi::c_int);
                    current_block = 5411263895410993842;
                } else {
                    current_block = 11555347550778173209;
                }
            } else if change > 0 as ::core::ffi::c_int
                && tty_term_has(terminal_term(client), TTYC_CUU) != 0
            {
                tty_putcode_i(client, TTYC_CUU, change);
                current_block = 5411263895410993842;
            } else if change < 0 as ::core::ffi::c_int
                && tty_term_has(terminal_term(client), TTYC_CUD) != 0
            {
                tty_putcode_i(client, TTYC_CUD, -change);
                current_block = 5411263895410993842;
            } else {
                current_block = 11555347550778173209;
            }
        }
    } else {
        current_block = 11555347550778173209;
    }
    if current_block == 11555347550778173209 {
        tty_putcode_ii(
            client,
            TTYC_CUP,
            cy as ::core::ffi::c_int,
            cx as ::core::ffi::c_int,
        );
    }
    terminal_set!(client, cx, =, cx);
    terminal_set!(client, cy, =, cy);
}

pub(super) unsafe fn tty_hyperlink(
    client: &ClientRef,
    gc: &grid_cell,
    hl: Option<&crate::hyperlinks::HyperlinksRef>,
) {
    if gc.link == terminal_value!(client, cell.link) {
        return;
    }
    terminal_set!(client, cell.link, =, gc.link);
    let Some(hl) = hl else {
        return;
    };
    let link = if gc.link == 0 {
        None
    } else {
        hyperlinks_get(hl, gc.link)
    };
    if let Some(link) = link {
        tty_putcode_ss(client, TTYC_HLS, Some(&*link.external_id), Some(&*link.uri));
    } else {
        tty_putcode_ss(client, TTYC_HLS, Some(c""), Some(c""));
    }
}

pub(super) unsafe fn tty_dim_default_colour(
    client: &ClientRef,
    mut c: ::core::ffi::c_int,
    mut foreground: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let mut theme: client_theme = THEME_UNKNOWN;
    if !(c == 8 as ::core::ffi::c_int || c == 9 as ::core::ffi::c_int) {
        return c;
    }
    if foreground != 0 && terminal_value!(client, fg) != -(1 as ::core::ffi::c_int) {
        return terminal_value!(client, fg);
    }
    if foreground == 0 && terminal_value!(client, bg) != -(1 as ::core::ffi::c_int) {
        return terminal_value!(client, bg);
    }
    theme = client.terminal_theme();
    if theme as ::core::ffi::c_uint == THEME_DARK as ::core::ffi::c_int as ::core::ffi::c_uint {
        return if foreground != 0 {
            7 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        };
    }
    if theme as ::core::ffi::c_uint == THEME_LIGHT as ::core::ffi::c_int as ::core::ffi::c_uint {
        return if foreground != 0 {
            0 as ::core::ffi::c_int
        } else {
            7 as ::core::ffi::c_int
        };
    }
    c
}

pub(super) unsafe fn tty_attributes(
    client: &ClientRef,
    gc: &grid_cell,
    style_ctx: Option<&tty_style_ctx>,
) {
    let mut gc2 = *gc;
    let mut changed: ::core::ffi::c_int = 0;
    let style_ctx = style_ctx.unwrap_or(&tty_default_style_ctx);
    let palette = &style_ctx.palette;
    if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_NOPALETTE != 0 {
        if gc2.fg == 8 as ::core::ffi::c_int {
            gc2.fg = style_ctx.defaults.fg;
        }
        if gc2.bg == 8 as ::core::ffi::c_int {
            gc2.bg = style_ctx.defaults.bg;
        }
        palette.with_palette(|palette| {
            let changed = colour_palette_get(palette, gc2.fg);
            if changed != -(1 as ::core::ffi::c_int) {
                gc2.fg = changed;
            }
            let changed = colour_palette_get(palette, gc2.bg);
            if changed != -(1 as ::core::ffi::c_int) {
                gc2.bg = changed;
            }
        });
    }
    gc2.fg = tty_map_theme_colour(client, gc2.fg);
    gc2.bg = tty_map_theme_colour(client, gc2.bg);
    gc2.us = tty_map_theme_colour(client, gc2.us);
    if style_ctx.dim != 0 as u_int {
        gc2.fg = tty_dim_default_colour(client, gc2.fg, 1 as ::core::ffi::c_int);
        gc2.bg = tty_dim_default_colour(client, gc2.bg, 0 as ::core::ffi::c_int);
        changed = colour_dim(gc2.fg, style_ctx.dim);
        if changed != -(1 as ::core::ffi::c_int) {
            gc2.fg = changed;
        }
        changed = colour_dim(gc2.bg, style_ctx.dim);
        if changed != -(1 as ::core::ffi::c_int) {
            gc2.bg = changed;
        }
    }
    if gc2.attr as ::core::ffi::c_int
        == terminal_value!(client, last_cell.attr) as ::core::ffi::c_int
        && gc2.fg == terminal_value!(client, last_cell.fg)
        && gc2.bg == terminal_value!(client, last_cell.bg)
        && gc2.us == terminal_value!(client, last_cell.us)
        && gc2.link == terminal_value!(client, last_cell.link)
    {
        return;
    }
    if tty_term_has(terminal_term(client), TTYC_SETAB) == 0 {
        if gc2.attr as ::core::ffi::c_int & GRID_ATTR_REVERSE != 0 {
            if gc2.fg != 7 as ::core::ffi::c_int
                && !(gc2.fg == 8 as ::core::ffi::c_int || gc2.fg == 9 as ::core::ffi::c_int)
            {
                gc2.attr = (gc2.attr as ::core::ffi::c_int & !GRID_ATTR_REVERSE) as u_short;
            }
        } else if gc2.bg != 0 as ::core::ffi::c_int
            && !(gc2.bg == 8 as ::core::ffi::c_int || gc2.bg == 9 as ::core::ffi::c_int)
        {
            gc2.attr = (gc2.attr as ::core::ffi::c_int | GRID_ATTR_REVERSE) as u_short;
        }
    }
    tty_check_fg(client, palette, &mut gc2);
    tty_check_bg(client, palette, &mut gc2);
    tty_check_us(client, palette, &mut gc2);
    if terminal_value!(client, cell.attr) as ::core::ffi::c_int & !(gc2.attr as ::core::ffi::c_int)
        != 0
        || terminal_value!(client, cell.us) != gc2.us && gc2.us == 0 as ::core::ffi::c_int
    {
        tty_reset(client);
    }
    tty_colours(client, &gc2);
    changed = gc2.attr as ::core::ffi::c_int
        & !(terminal_value!(client, cell.attr) as ::core::ffi::c_int);
    terminal_set!(client, cell.attr, =, gc2.attr);
    if changed & GRID_ATTR_BRIGHT != 0 {
        tty_putcode(client, TTYC_BOLD);
    }
    if changed & GRID_ATTR_DIM != 0 {
        tty_putcode(client, TTYC_DIM);
    }
    if changed & GRID_ATTR_ITALICS != 0 {
        tty_set_italics(client);
    }
    if changed & GRID_ATTR_ALL_UNDERSCORE != 0 {
        if changed & GRID_ATTR_UNDERSCORE != 0 {
            tty_putcode(client, TTYC_SMUL);
        } else if changed & GRID_ATTR_UNDERSCORE_2 != 0 {
            tty_putcode_i(client, TTYC_SMULX, 2 as ::core::ffi::c_int);
        } else if changed & GRID_ATTR_UNDERSCORE_3 != 0 {
            tty_putcode_i(client, TTYC_SMULX, 3 as ::core::ffi::c_int);
        } else if changed & GRID_ATTR_UNDERSCORE_4 != 0 {
            tty_putcode_i(client, TTYC_SMULX, 4 as ::core::ffi::c_int);
        } else if changed & GRID_ATTR_UNDERSCORE_5 != 0 {
            tty_putcode_i(client, TTYC_SMULX, 5 as ::core::ffi::c_int);
        }
    }
    if changed & GRID_ATTR_BLINK != 0 {
        tty_putcode(client, TTYC_BLINK);
    }
    if changed & GRID_ATTR_REVERSE != 0 {
        if tty_term_has(terminal_term(client), TTYC_REV) != 0 {
            tty_putcode(client, TTYC_REV);
        } else if tty_term_has(terminal_term(client), TTYC_SMSO) != 0 {
            tty_putcode(client, TTYC_SMSO);
        }
    }
    if changed & GRID_ATTR_HIDDEN != 0 {
        tty_putcode(client, TTYC_INVIS);
    }
    if changed & GRID_ATTR_STRIKETHROUGH != 0 {
        tty_putcode(client, TTYC_SMXX);
    }
    if changed & GRID_ATTR_OVERLINE != 0 {
        tty_putcode(client, TTYC_SMOL);
    }
    if changed & GRID_ATTR_CHARSET != 0 && {
        let utf8 = client.flags() & CLIENT_UTF8 as u64 != 0;
        tty_acs_needed(Some(client.borrow_terminal()), utf8)
    } != 0
    {
        tty_putcode(client, TTYC_SMACS);
    }
    tty_hyperlink(client, gc, style_ctx.hyperlinks.as_ref());
    terminal_set!(client, last_cell, =, gc2);
}

pub(super) unsafe fn tty_colours(client: &ClientRef, gc: &grid_cell) {
    if gc.fg == terminal_value!(client, cell.fg)
        && gc.bg == terminal_value!(client, cell.bg)
        && gc.us == terminal_value!(client, cell.us)
    {
        return;
    }
    if gc.fg == 8 as ::core::ffi::c_int
        || gc.fg == 9 as ::core::ffi::c_int
        || (gc.bg == 8 as ::core::ffi::c_int || gc.bg == 9 as ::core::ffi::c_int)
    {
        if tty_term_flag(terminal_term(client), TTYC_AX) == 0 {
            tty_reset(client);
        } else {
            if (gc.fg == 8 as ::core::ffi::c_int || gc.fg == 9 as ::core::ffi::c_int)
                && !(terminal_value!(client, cell.fg) == 8 as ::core::ffi::c_int
                    || terminal_value!(client, cell.fg) == 9 as ::core::ffi::c_int)
            {
                tty_puts(client, c"\x1B[39m");
                terminal_set!(client, cell.fg, =, gc.fg);
            }
            if (gc.bg == 8 as ::core::ffi::c_int || gc.bg == 9 as ::core::ffi::c_int)
                && !(terminal_value!(client, cell.bg) == 8 as ::core::ffi::c_int
                    || terminal_value!(client, cell.bg) == 9 as ::core::ffi::c_int)
            {
                tty_puts(client, c"\x1B[49m");
                terminal_set!(client, cell.bg, =, gc.bg);
            }
        }
    }
    if !(gc.fg == 8 as ::core::ffi::c_int || gc.fg == 9 as ::core::ffi::c_int)
        && gc.fg != terminal_value!(client, cell.fg)
    {
        tty_colours_fg(client, gc);
    }
    if !(gc.bg == 8 as ::core::ffi::c_int || gc.bg == 9 as ::core::ffi::c_int)
        && gc.bg != terminal_value!(client, cell.bg)
    {
        tty_colours_bg(client, gc);
    }
    if gc.us != terminal_value!(client, cell.us) {
        tty_colours_us(client, gc);
    }
}

pub(super) unsafe fn tty_map_theme_colour(client: &ClientRef, colour: i32) -> i32 {
    if colour & COLOUR_FLAG_THEME == 0 {
        return colour;
    }
    let index = (colour & 0xff) as usize;
    let mapped = client.terminal_theme_colour(index);
    if mapped == -1 || mapped & COLOUR_FLAG_THEME != 0 {
        8
    } else {
        mapped
    }
}

pub(super) unsafe fn tty_check_fg(
    client: &ClientRef,
    palette: &crate::shared::tty::PaletteSource,
    gc: &mut grid_cell,
) {
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    let mut colours: u_int = 0;
    let mut c: ::core::ffi::c_int = 0;
    if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_NOPALETTE != 0 {
        c = gc.fg;
        if c < 8 as ::core::ffi::c_int
            && gc.attr as ::core::ffi::c_int & GRID_ATTR_BRIGHT != 0
            && tty_term_has(terminal_term(client), TTYC_NOBR) == 0
        {
            c += 90 as ::core::ffi::c_int;
        }
        c = palette.with_palette(|palette| colour_palette_get(palette, c));
        if c != -(1 as ::core::ffi::c_int) {
            gc.fg = c;
        }
    }
    gc.fg = tty_map_theme_colour(client, gc.fg);
    if gc.fg & COLOUR_FLAG_RGB != 0 {
        if terminal_term(client).flags & TERM_RGBCOLOURS != 0 {
            return;
        }
        (r, g, b) = colour_split_rgb(gc.fg);
        gc.fg = colour_find_rgb(r, g, b);
    }
    if terminal_term(client).flags & TERM_256COLOURS != 0 {
        colours = 256 as u_int;
    } else {
        colours = tty_term_number(terminal_term(client), TTYC_COLORS) as u_int;
    }
    if gc.fg & COLOUR_FLAG_256 != 0 {
        if colours >= 256 as u_int {
            return;
        }
        gc.fg = colour_256to16(gc.fg);
        if !gc.fg & 8 as ::core::ffi::c_int != 0 {
            return;
        }
        gc.fg &= 7 as ::core::ffi::c_int;
        if colours >= 16 as u_int {
            gc.fg += 90 as ::core::ffi::c_int;
        } else if gc.fg == 0 as ::core::ffi::c_int && gc.bg == 0 as ::core::ffi::c_int {
            gc.fg = 7 as ::core::ffi::c_int;
        } else if gc.fg == 7 as ::core::ffi::c_int && gc.bg == 7 as ::core::ffi::c_int {
            gc.fg = 0 as ::core::ffi::c_int;
        }
        return;
    }
    if gc.fg >= 90 as ::core::ffi::c_int
        && gc.fg <= 97 as ::core::ffi::c_int
        && colours < 16 as u_int
    {
        gc.fg -= 90 as ::core::ffi::c_int;
        gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_BRIGHT) as u_short;
    }
}

pub(super) unsafe fn tty_check_bg(
    client: &ClientRef,
    palette: &crate::shared::tty::PaletteSource,
    gc: &mut grid_cell,
) {
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    let mut colours: u_int = 0;
    let mut c: ::core::ffi::c_int = 0;
    if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_NOPALETTE != 0 {
        c = palette.with_palette(|palette| colour_palette_get(palette, gc.bg));
        if c != -(1 as ::core::ffi::c_int) {
            gc.bg = c;
        }
    }
    gc.bg = tty_map_theme_colour(client, gc.bg);
    if gc.bg & COLOUR_FLAG_RGB != 0 {
        if terminal_term(client).flags & TERM_RGBCOLOURS != 0 {
            return;
        }
        (r, g, b) = colour_split_rgb(gc.bg);
        gc.bg = colour_find_rgb(r, g, b);
    }
    if terminal_term(client).flags & TERM_256COLOURS != 0 {
        colours = 256 as u_int;
    } else {
        colours = tty_term_number(terminal_term(client), TTYC_COLORS) as u_int;
    }
    if gc.bg & COLOUR_FLAG_256 != 0 {
        if colours >= 256 as u_int {
            return;
        }
        gc.bg = colour_256to16(gc.bg);
        if !gc.bg & 8 as ::core::ffi::c_int != 0 {
            return;
        }
        gc.bg &= 7 as ::core::ffi::c_int;
        if colours >= 16 as u_int {
            gc.bg += 90 as ::core::ffi::c_int;
        }
        return;
    }
    if gc.bg >= 90 as ::core::ffi::c_int
        && gc.bg <= 97 as ::core::ffi::c_int
        && colours < 16 as u_int
    {
        gc.bg -= 90 as ::core::ffi::c_int;
    }
}

pub(super) unsafe fn tty_check_us(
    client: &ClientRef,
    palette: &crate::shared::tty::PaletteSource,
    gc: &mut grid_cell,
) {
    let mut c: ::core::ffi::c_int = 0;
    if !(gc.flags as ::core::ffi::c_int) & GRID_FLAG_NOPALETTE != 0 {
        c = palette.with_palette(|palette| colour_palette_get(palette, gc.us));
        if c != -(1 as ::core::ffi::c_int) {
            gc.us = c;
        }
    }
    gc.us = tty_map_theme_colour(client, gc.us);
    if tty_term_has(terminal_term(client), TTYC_SETULC1) == 0 {
        c = colour_force_rgb(gc.us);
        if c == -(1 as ::core::ffi::c_int) {
            gc.us = 8 as ::core::ffi::c_int;
        } else {
            gc.us = c;
        }
    }
}

pub(super) unsafe fn tty_colours_fg(client: &ClientRef, gc: &grid_cell) {
    if terminal_value!(client, cell.fg) >= 90 as ::core::ffi::c_int
        && terminal_value!(client, cell.fg) <= 97 as ::core::ffi::c_int
        && (gc.fg < 90 as ::core::ffi::c_int || gc.fg > 97 as ::core::ffi::c_int)
    {
        tty_reset(client);
    }
    if gc.fg & COLOUR_FLAG_RGB != 0 || gc.fg & COLOUR_FLAG_256 != 0 {
        if !(tty_try_colour(client, gc.fg, true) == 0 as ::core::ffi::c_int) {
            return;
        }
    } else if gc.fg >= 90 as ::core::ffi::c_int && gc.fg <= 97 as ::core::ffi::c_int {
        if terminal_term(client).flags & TERM_256COLOURS != 0 {
            let s = format_message_with(|out| write!(out, "\x1B[{}m", { gc.fg }));
            tty_puts(client, &s);
        } else {
            tty_putcode_i(
                client,
                TTYC_SETAF,
                gc.fg - 90 as ::core::ffi::c_int + 8 as ::core::ffi::c_int,
            );
        }
    } else {
        tty_putcode_i(client, TTYC_SETAF, gc.fg);
    }
    terminal_set!(client, cell.fg, =, gc.fg);
}

pub(super) unsafe fn tty_colours_bg(client: &ClientRef, gc: &grid_cell) {
    if gc.bg & COLOUR_FLAG_RGB != 0 || gc.bg & COLOUR_FLAG_256 != 0 {
        if !(tty_try_colour(client, gc.bg, false) == 0 as ::core::ffi::c_int) {
            return;
        }
    } else if gc.bg >= 90 as ::core::ffi::c_int && gc.bg <= 97 as ::core::ffi::c_int {
        if terminal_term(client).flags & TERM_256COLOURS != 0 {
            let s = format_message_with(|out| {
                write!(out, "\x1B[{}m", { gc.bg + 10 as ::core::ffi::c_int })
            });
            tty_puts(client, &s);
        } else {
            tty_putcode_i(
                client,
                TTYC_SETAB,
                gc.bg - 90 as ::core::ffi::c_int + 8 as ::core::ffi::c_int,
            );
        }
    } else {
        tty_putcode_i(client, TTYC_SETAB, gc.bg);
    }
    terminal_set!(client, cell.bg, =, gc.bg);
}

pub(super) unsafe fn tty_colours_us(client: &ClientRef, gc: &grid_cell) {
    let mut c: u_int = 0;
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    if gc.us == 8 as ::core::ffi::c_int || gc.us == 9 as ::core::ffi::c_int {
        tty_putcode(client, TTYC_OL);
    } else {
        if !gc.us & COLOUR_FLAG_RGB != 0 {
            c = gc.us as u_int;
            if !c & COLOUR_FLAG_256 as u_int != 0 && (c >= 90 as u_int && c <= 97 as u_int) {
                c = c.wrapping_sub(82 as u_int);
            }
            tty_putcode_i(
                client,
                TTYC_SETULC1,
                (c & !COLOUR_FLAG_256 as u_int) as ::core::ffi::c_int,
            );
            return;
        }
        (r, g, b) = colour_split_rgb(gc.us);
        c = (65536 as ::core::ffi::c_int * r as ::core::ffi::c_int
            + 256 as ::core::ffi::c_int * g as ::core::ffi::c_int
            + b as ::core::ffi::c_int) as u_int;
        if tty_term_has(terminal_term(client), TTYC_SETULC) != 0 {
            tty_putcode_i(client, TTYC_SETULC, c as ::core::ffi::c_int);
        } else if tty_term_has(terminal_term(client), TTYC_SETAL) != 0
            && tty_term_has(terminal_term(client), TTYC_RGB) != 0
        {
            tty_putcode_i(client, TTYC_SETAL, c as ::core::ffi::c_int);
        }
    }
    terminal_set!(client, cell.us, =, gc.us);
}

pub(super) unsafe fn tty_try_colour(
    client: &ClientRef,
    mut colour: ::core::ffi::c_int,
    foreground: bool,
) -> ::core::ffi::c_int {
    let mut r: u_char = 0;
    let mut g: u_char = 0;
    let mut b: u_char = 0;
    if colour & COLOUR_FLAG_256 != 0 {
        if foreground && tty_term_has(terminal_term(client), TTYC_SETAF) != 0 {
            tty_putcode_i(client, TTYC_SETAF, colour & 0xff as ::core::ffi::c_int);
        } else if tty_term_has(terminal_term(client), TTYC_SETAB) != 0 {
            tty_putcode_i(client, TTYC_SETAB, colour & 0xff as ::core::ffi::c_int);
        }
        return 0 as ::core::ffi::c_int;
    }
    if colour & COLOUR_FLAG_RGB != 0 {
        (r, g, b) = colour_split_rgb(colour & 0xffffff as ::core::ffi::c_int);
        if foreground && tty_term_has(terminal_term(client), TTYC_SETRGBF) != 0 {
            tty_putcode_iii(
                client,
                TTYC_SETRGBF,
                r as ::core::ffi::c_int,
                g as ::core::ffi::c_int,
                b as ::core::ffi::c_int,
            );
        } else if tty_term_has(terminal_term(client), TTYC_SETRGBB) != 0 {
            tty_putcode_iii(
                client,
                TTYC_SETRGBB,
                r as ::core::ffi::c_int,
                g as ::core::ffi::c_int,
                b as ::core::ffi::c_int,
            );
        }
        return 0 as ::core::ffi::c_int;
    }
    -(1 as ::core::ffi::c_int)
}

pub(super) unsafe fn tty_default_attributes(
    client: &ClientRef,
    bg: u_int,
    style_ctx: Option<&tty_style_ctx>,
) {
    let mut gc = grid_default_cell;
    gc.bg = bg as i32;
    tty_attributes(client, &gc, style_ctx);
}

pub unsafe fn tty_set_progress_bar(client: &ClientRef, pb: &progress_bar) {
    if tty_term_has(terminal_term(client), TTYC_SPB) != 0 {
        tty_putcode_ii(
            client,
            TTYC_SPB,
            pb.state as ::core::ffi::c_int,
            pb.progress,
        );
    }
}
