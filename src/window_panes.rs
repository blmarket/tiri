use crate::arguments::{
    args, args_command_state, args_has, args_make_commands, args_make_commands_prepare,
    args_strtonum_result,
};
use crate::cmd::cmd_mouse_at;
use crate::cmd::queue::{cmdq_append, cmdq_error, cmdq_get_cmd, cmdq_get_command, cmdq_get_error};
use crate::format::bytes::format_bytes;
use crate::format::bytes::write_cstr;
use crate::format::{format_create_defaults, format_single_cstring};
use crate::format_draw::format_draw;
use crate::grid::grid_default_cell;
use crate::options::OptionsScope;
use crate::options::{options_get_number, options_get_string};
use crate::screen::{screen_init, screen_resize};
use crate::screen_write::{
    screen_write_cell, screen_write_clearscreen, screen_write_cursormove, screen_write_fast_copy,
    screen_write_preview, screen_write_putc, screen_write_puts, screen_write_start,
    screen_write_stop,
};
use crate::server_fn::{server_redraw_window, server_redraw_window_borders, server_status_window};
use crate::session::Session;
use crate::window::Window as _;

use crate::shared::abi::*;
use crate::shared::borders::{CELL_BORDERS, CELL_LR, CELL_UD};
use crate::shared::client::ClientRef;
use crate::shared::command::{cmd, cmd_find_state, cmdq_item};
use crate::shared::event::*;
use crate::shared::grid::*;
use crate::shared::key::*;
use crate::shared::layout::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::screen::{screen, MODE_CURSOR};
use crate::shared::screen_write::screen_write_ctx;
use crate::shared::session::{SessionRef, SessionWeak};
use crate::shared::style::*;
use crate::shared::window::WINDOW_MODE_NO_STACK;
use crate::shared::window::{window_mode, window_mode_entry, winlink};
use crate::shared::window::{WindowRef, WindowWeak};
use crate::style::style_apply_with_options;
use crate::text::utf8::utf8_set;
use crate::window::winlink_find_by_window;
use crate::window::WindowPane;
use crate::window_clock::window_clock_table;
use std::cell::UnsafeCell;
use std::ffi::CString;
use std::rc::{Rc, Weak};
use std::time::Duration;

#[repr(C)]
pub struct window_panes_modedata {
    pub wp: Weak<UnsafeCell<window_pane>>,
    pub session: SessionWeak,
    pub source_session: u_int,
    pub source_window: u_int,
    pub screen: screen,
    preview: Option<Box<screen>>,
    pub timer: Option<Timer>,
    pub state: Option<Box<args_command_state>>,
    pub delay: u_int,
    pub ignore_keys: ::core::ffi::c_int,
    areas: Vec<window_panes_area>,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct window_panes_area {
    pub id: u_int,
    pub x: u_int,
    pub y: u_int,
    pub sx: u_int,
    pub sy: u_int,
}

pub const WINDOW_MODE_FILL_WINDOW: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub static window_panes_mode: window_mode = {
    window_mode {
        name: c"panes-mode",
        default_format: None,
        flags: WINDOW_MODE_NO_STACK | WINDOW_MODE_FILL_WINDOW,
        init: Some(
            window_panes_init
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
                    Option<&cmd_find_state>,
                    Option<&mut args>,
                ) -> bool,
        ),
        free: Some(window_panes_free as unsafe fn(refbox::Weak<window_mode_entry>) -> ()),
        resize: Some(
            window_panes_resize as unsafe fn(refbox::Weak<window_mode_entry>, u_int, u_int) -> (),
        ),
        update: None,
        style_changed: None,
        key: Some(
            window_panes_key
                as unsafe fn(
                    refbox::Weak<window_mode_entry>,
                    &ClientRef,
                    refbox::Weak<winlink>,
                    key_code,
                    Option<&mut mouse_event>,
                ) -> (),
        ),
        key_table: None,
        command: None,
        formats: None,
        get_screen: None,
        display_screen: Some(window_panes_get_screen),
    }
};
unsafe fn window_panes_session(data: *mut window_panes_modedata) -> Option<SessionRef> {
    let owner = (*data).session.upgrade()?;
    if !owner.is_registered() {
        drop(owner);
        return None;
    }
    Some(owner)
}

unsafe fn window_panes_get_source(
    data: *mut window_panes_modedata,
    wlp: *mut refbox::Weak<winlink>,
    session_owner: &mut Option<SessionRef>,
) -> Option<WindowWeak> {
    let window_owner = crate::window::windows::resolve((*data).source_window)?;
    let mut link = refbox::Weak::new();
    *session_owner = crate::session::sessions::find_by_id((*data).source_session);
    if let Some(session) = session_owner.as_ref() {
        link = session.with_winlinks(|links| winlink_find_by_window(links, &window_owner));
    }
    if !link.is_alive() {
        *session_owner = window_panes_session(data);
    }
    if let Some(session) = session_owner.as_ref() {
        link = session.with_winlinks(|links| winlink_find_by_window(links, &window_owner));
    }
    if !wlp.is_null() {
        *wlp = link;
    }
    let window = WindowRef::downgrade(&window_owner);
    drop(window_owner);
    Some(window)
}
unsafe fn window_panes_set_preview(data: *mut window_panes_modedata) {
    let Some(pane) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let (sx, sy) = pane.screen_size(false);
    let mut preview = Box::new(screen::empty());
    pane.copy_screen(&mut preview, 0, 0, sx, sy, false);
    let mode = pane.screen_mode(false);
    preview.mode = mode.mode;
    preview.cx = mode.cx;
    preview.cy = mode.cy;
    (*data).preview = Some(preview);
}

unsafe fn window_panes_free_areas(mut data: *mut window_panes_modedata) {
    (*data).areas = Vec::new();
}
unsafe fn window_panes_add_area(
    mut data: *mut window_panes_modedata,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut x: u_int,
    mut y: u_int,
    mut sx: u_int,
    mut sy: u_int,
) {
    (*data).areas.push(window_panes_area {
        id: wp_owner.id(),
        x,
        y,
        sx,
        sy,
    });
}
unsafe fn window_panes_pane_geometry(
    pane: &Rc<UnsafeCell<window_pane>>,
) -> Option<layout_geometry> {
    let window = pane.window_observer().upgrade()?;
    window
        .pane_cells()
        .into_iter()
        .find_map(|(candidate, cell)| candidate.ptr_eq(&Rc::downgrade(pane)).then_some(cell))
}
fn window_panes_scaled_geometry(
    geometry: &layout_geometry,
    osx: u_int,
    osy: u_int,
    dsx: u_int,
    dsy: u_int,
) -> Option<(u_int, u_int, u_int, u_int)> {
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut x2: u_int = 0;
    let mut y2: u_int = 0;
    if osx == 0 || osy == 0 || dsx == 0 || dsy == 0 {
        return None;
    }
    if osx <= dsx && osy <= dsy {
        x = geometry.xoff as u_int;
        y = geometry.yoff as u_int;
        x2 = x.wrapping_add(geometry.sx);
        y2 = y.wrapping_add(geometry.sy);
    } else {
        x = (geometry.xoff as u_int).wrapping_mul(dsx).wrapping_div(osx);
        y = (geometry.yoff as u_int).wrapping_mul(dsy).wrapping_div(osy);
        x2 = (geometry.xoff as u_int)
            .wrapping_add(geometry.sx)
            .wrapping_mul(dsx)
            .wrapping_div(osx);
        y2 = (geometry.yoff as u_int)
            .wrapping_add(geometry.sy)
            .wrapping_mul(dsy)
            .wrapping_div(osy);
    }
    if x >= dsx || y >= dsy {
        return None;
    }
    if x2 <= x {
        x2 = x.wrapping_add(1 as u_int);
    }
    if y2 <= y {
        y2 = y.wrapping_add(1 as u_int);
    }
    if x2 > dsx {
        x2 = dsx;
    }
    if y2 > dsy {
        y2 = dsy;
    }
    sx = x2.wrapping_sub(x);
    sy = y2.wrapping_sub(y);
    if sx == 0 as u_int || sy == 0 as u_int {
        return None;
    }
    Some((x, y, sx, sy))
}
unsafe fn window_panes_get_geometry(
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut osx: u_int,
    mut osy: u_int,
    mut dsx: u_int,
    mut dsy: u_int,
    xp: &mut u_int,
    yp: &mut u_int,
    sxp: &mut u_int,
    syp: &mut u_int,
) -> ::core::ffi::c_int {
    let Some(geometry) = window_panes_pane_geometry(wp_owner) else {
        return 0;
    };
    let Some((x, y, sx, sy)) = window_panes_scaled_geometry(&geometry, osx, osy, dsx, dsy) else {
        return 0;
    };
    *xp = x;
    *yp = y;
    *sxp = sx;
    *syp = sy;
    1 as ::core::ffi::c_int
}
unsafe fn window_panes_get_border_cell(mut data: *mut window_panes_modedata, gc: &mut grid_cell) {
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let session_owner = window_panes_session(data);
    let mut s = session_owner.clone();
    *gc = grid_default_cell;
    let mut ft_owner = format_create_defaults(
        None,
        None,
        s.as_ref(),
        s.as_ref()
            .map_or_else(refbox::Weak::new, |session| session.current_winlink()),
        Some(&mode_pane_owner),
    );
    let ft = &mut *ft_owner;
    let options_window = mode_pane_owner.window_observer();
    style_apply_with_options(
        gc,
        c"display-panes-border-style",
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    drop(ft_owner);
    if let Some(owner) = session_owner {
        drop(owner);
    }
}
fn window_panes_map_x(mut x: u_int, mut osx: u_int, mut dsx: u_int) -> ::core::ffi::c_int {
    if osx <= dsx {
        return x as ::core::ffi::c_int;
    }
    x.wrapping_mul(dsx).wrapping_div(osx) as ::core::ffi::c_int
}
fn window_panes_map_y(mut y: u_int, mut osy: u_int, mut dsy: u_int) -> ::core::ffi::c_int {
    if osy <= dsy {
        return y as ::core::ffi::c_int;
    }
    y.wrapping_mul(dsy).wrapping_div(osy) as ::core::ffi::c_int
}
/// Draw the separator to the right of every pane but the rightmost, and the
/// one below every pane but the lowest. Where separators cross the preview
/// draws plain lines.
unsafe fn window_panes_draw_borders(
    mut ctx: &mut screen_write_ctx,
    w_owner: &WindowRef,
    gc: &grid_cell,
    mut osx: u_int,
    mut osy: u_int,
    mut dsx: u_int,
    mut dsy: u_int,
) {
    if dsx == 0 as u_int || dsy == 0 as u_int {
        return;
    }
    let mut vertical = *gc;
    vertical.attr = (vertical.attr as ::core::ffi::c_int | GRID_ATTR_CHARSET) as u_short;
    let mut horizontal = vertical;
    utf8_set(&mut vertical.data, CELL_BORDERS[CELL_UD as usize] as u_char);
    utf8_set(
        &mut horizontal.data,
        CELL_BORDERS[CELL_LR as usize] as u_char,
    );
    let cells = w_owner.pane_cells();
    let right_edge = cells
        .iter()
        .map(|(_, g)| (g.xoff as u_int).wrapping_add(g.sx))
        .max()
        .unwrap_or(0);
    let bottom_edge = cells
        .iter()
        .map(|(_, g)| (g.yoff as u_int).wrapping_add(g.sy))
        .max()
        .unwrap_or(0);
    for (_, geometry) in &cells {
        let right = (geometry.xoff as u_int).wrapping_add(geometry.sx);
        let bottom = (geometry.yoff as u_int).wrapping_add(geometry.sy);
        let x = window_panes_map_x(geometry.xoff as u_int, osx, dsx).max(0);
        let x2 = window_panes_map_x(right, osx, dsx);
        let y = window_panes_map_y(geometry.yoff as u_int, osy, dsy).max(0);
        let y2 = window_panes_map_y(bottom, osy, dsy);
        if right < right_edge && x2 >= 0 && (x2 as u_int) < dsx {
            for yy in y..y2.min(dsy as ::core::ffi::c_int) {
                screen_write_cursormove(ctx, x2, yy, 0 as ::core::ffi::c_int);
                screen_write_cell(ctx, &vertical);
            }
        }
        if bottom < bottom_edge && y2 >= 0 && (y2 as u_int) < dsy {
            for xx in x..x2.min(dsx as ::core::ffi::c_int) {
                screen_write_cursormove(ctx, xx, y2, 0 as ::core::ffi::c_int);
                screen_write_cell(ctx, &horizontal);
            }
        }
    }
}
unsafe fn window_panes_draw_format(
    mut data: *mut window_panes_modedata,
    mut ctx: &mut screen_write_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut x: u_int,
    mut y: u_int,
    mut sx: u_int,
    gc: &grid_cell,
) {
    let mut source_session_owner = None;
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let session_owner = window_panes_session(data);
    let mut s = session_owner.clone();
    let mut wl: refbox::Weak<winlink> = s
        .as_ref()
        .map_or_else(refbox::Weak::new, |session| session.current_winlink());
    if sx == 0 as u_int {
        if let Some(owner) = session_owner {
            drop(owner);
        }
        return;
    }
    let format = options_get_string(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live display-panes window")
            .borrow_options_mut(),
        c"display-panes-format",
    );
    if format.as_bytes().is_empty() {
        if let Some(owner) = session_owner {
            drop(owner);
        }
        return;
    }
    if window_panes_get_source(data, &raw mut wl, &mut source_session_owner).is_some() {
        s = source_session_owner.clone();
    }
    if s.is_none() {
        if let Some(owner) = session_owner {
            drop(owner);
        }
        if let Some(owner) = source_session_owner {
            drop(owner);
        }
        return;
    }
    let expanded = format_single_cstring(
        None,
        format.as_ptr(),
        None,
        s.as_ref(),
        wl.clone(),
        Some(wp_owner),
    );
    if !expanded.is_empty() {
        screen_write_cursormove(
            ctx,
            x as ::core::ffi::c_int,
            y as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        format_draw(
            ctx,
            gc,
            sx,
            expanded.as_ptr(),
            ::core::ptr::null_mut::<style_ranges>(),
            0 as ::core::ffi::c_int,
        );
    }
    if let Some(owner) = session_owner {
        drop(owner);
    }
    if let Some(owner) = source_session_owner {
        drop(owner);
    }
}
unsafe fn window_panes_draw_number(
    mut data: *mut window_panes_modedata,
    mut ctx: &mut screen_write_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut pane: u_int,
    mut x: u_int,
    mut y: u_int,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mut source_session_owner = None;
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let session_owner = window_panes_session(data);
    let mut s = session_owner.clone();
    let mut wl: refbox::Weak<winlink> = s
        .as_ref()
        .map_or_else(refbox::Weak::new, |session| session.current_winlink());
    let options_window = mode_pane_owner.window_observer();
    let mut fgc: grid_cell = grid_cell {
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
    let mut bgc: grid_cell = grid_cell {
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
    let mut len: size_t = 0;
    let mut llen: size_t = 0 as size_t;
    let mut width: size_t = 0;
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut idx: u_int = 0;
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    let mut format: u_int = 0;
    let buf = format_bytes(format_args!("{}", pane));
    len = buf.len();
    let mut lbuf = Vec::new();
    if pane > 9 as u_int && pane < 35 as u_int {
        lbuf.push(b'a' + (pane - 10) as u8);
    }
    llen = lbuf.len();
    if (sx as size_t) < len {
        if let Some(owner) = session_owner {
            drop(owner);
        }
        return;
    }
    if window_panes_get_source(data, &raw mut wl, &mut source_session_owner).is_some() {
        s = source_session_owner.clone();
    }
    if let Some(session) = s.as_ref().filter(|_| !wl.is_alive()) {
        wl = session.current_winlink();
    }
    let window = wp_owner
        .window_observer()
        .upgrade()
        .expect("live pane parent");
    let active = window.active_pane().ptr_eq(&Rc::downgrade(wp_owner));
    drop(window);
    let name = if active {
        c"display-panes-active-colour"
    } else {
        c"display-panes-colour"
    };
    let mut ft_owner = format_create_defaults(None, None, s.as_ref(), wl.clone(), Some(wp_owner));
    let ft = &mut *ft_owner;
    style_apply_with_options(
        &mut fgc,
        name,
        Some(&mut *ft),
        &OptionsScope::Window(options_window.clone()),
    );
    drop(ft_owner);
    bgc = grid_default_cell;
    bgc.bg = fgc.fg;
    format = 0 as u_int;
    let window = options_window.upgrade().expect("live display-panes window");
    if !options_get_string(&window.borrow_options_mut(), c"display-panes-format")
        .as_bytes()
        .is_empty()
    {
        format = 1;
    }
    width = len.wrapping_mul(6 as size_t).wrapping_sub(1 as size_t);
    if (sx as size_t) < width
        || sy
            < (if format != 0 {
                7 as ::core::ffi::c_int
            } else {
                5 as ::core::ffi::c_int
            }) as u_int
    {
        width = len;
        if llen != 0 as size_t && sx as size_t >= len.wrapping_add(llen).wrapping_add(1 as size_t) {
            width = width.wrapping_add(llen.wrapping_add(1 as size_t));
        }
        cx = (x as size_t)
            .wrapping_add((sx as size_t).wrapping_sub(width).wrapping_div(2 as size_t))
            as u_int;
        cy = y.wrapping_add(sy.wrapping_div(2 as u_int));
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_puts(ctx, &fgc, |out| out.write_all(&buf));
        if width > len {
            screen_write_puts(ctx, &fgc, |out| {
                out.write_all(b" ")?;
                out.write_all(&lbuf)
            });
        }
        if format != 0 && sy > 1 as u_int {
            window_panes_draw_format(data, ctx, wp_owner, x, y, sx, &fgc);
        }
        if let Some(owner) = session_owner {
            drop(owner);
        }
        if let Some(owner) = source_session_owner {
            drop(owner);
        }
        return;
    }
    px = (sx as size_t).wrapping_sub(width).wrapping_div(2 as size_t) as u_int;
    py = sy.wrapping_sub(5 as u_int).wrapping_div(2 as u_int);
    for &digit in &buf {
        if digit.is_ascii_digit() {
            idx = (digit - b'0') as u_int;
            j = 0 as u_int;
            while j < 5 as u_int {
                i = 0 as u_int;
                while i < 5 as u_int {
                    if !(window_clock_table[idx as usize][j as usize][i as usize] == 0) {
                        screen_write_cursormove(
                            ctx,
                            x.wrapping_add(px).wrapping_add(i) as ::core::ffi::c_int,
                            y.wrapping_add(py).wrapping_add(j) as ::core::ffi::c_int,
                            0 as ::core::ffi::c_int,
                        );
                        screen_write_putc(ctx, &bgc, ' ' as i32 as u_char);
                    }
                    i = i.wrapping_add(1);
                }
                j = j.wrapping_add(1);
            }
            px = px.wrapping_add(6 as u_int);
        }
    }
    if sy <= 6 as u_int {
        if let Some(owner) = session_owner {
            drop(owner);
        }
        if let Some(owner) = source_session_owner {
            drop(owner);
        }
        return;
    }
    window_panes_draw_format(data, ctx, wp_owner, x, y, sx, &fgc);
    if llen != 0 as size_t {
        cx = (x.wrapping_add(px) as size_t)
            .wrapping_sub(llen)
            .wrapping_sub(1 as size_t) as u_int;
        cy = y.wrapping_add(py).wrapping_add(5 as u_int);
        screen_write_cursormove(
            ctx,
            cx as ::core::ffi::c_int,
            cy as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        screen_write_puts(ctx, &fgc, |out| out.write_all(&lbuf));
    }
    if let Some(owner) = session_owner {
        drop(owner);
    }
    if let Some(owner) = source_session_owner {
        drop(owner);
    }
}
unsafe fn window_panes_draw_pane(
    mut data: *mut window_panes_modedata,
    mut ctx: &mut screen_write_ctx,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut osx: u_int,
    mut osy: u_int,
    mut dsx: u_int,
    mut dsy: u_int,
) {
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let _pane: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    if window_panes_get_geometry(
        wp_owner, osx, osy, dsx, dsy, &mut x, &mut y, &mut sx, &mut sy,
    ) == 0
    {
        return;
    }
    let window = wp_owner
        .window_observer()
        .upgrade()
        .expect("live pane parent");
    let index = window.pane_index(&Rc::downgrade(wp_owner));
    drop(window);
    let Some(pane) = index else {
        return;
    };
    window_panes_add_area(data, wp_owner, x, y, sx, sy);
    screen_write_cursormove(
        ctx,
        x as ::core::ffi::c_int,
        y as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    let preview = (*data).preview.as_deref().filter(|preview| {
        Rc::ptr_eq(wp_owner, &mode_pane_owner) && sx <= preview.grid().sx && sy <= preview.grid().sy
    });
    if let Some(preview) = preview {
        if osx <= dsx && osy <= dsy {
            screen_write_fast_copy(ctx, preview, 0, preview.grid().hsize, sx, sy);
        } else {
            screen_write_preview(ctx, preview, sx, sy);
        }
    } else {
        let (source_sx, source_sy) = wp_owner.screen_size(false);
        let mut snapshot = screen::empty();
        wp_owner.copy_screen(&mut snapshot, 0, 0, source_sx, source_sy, false);
        let s = &snapshot;
        if osx <= dsx && osy <= dsy {
            screen_write_fast_copy(ctx, s, 0, s.grid().hsize, sx, sy);
        } else {
            screen_write_preview(ctx, s, sx, sy);
        }
        drop(snapshot);
    }
    window_panes_draw_number(data, ctx, wp_owner, pane, x, y, sx, sy);
}
unsafe fn window_panes_data(wme: refbox::Weak<window_mode_entry>) -> *mut window_panes_modedata {
    wme.get_unchecked()
        .boxed_data_ptr::<window_panes_modedata>()
        .expect("panes mode payload")
}

unsafe fn window_panes_draw_screen(mut wme: refbox::Weak<window_mode_entry>) {
    let mut source_session_owner = None;
    let mut data: *mut window_panes_modedata = window_panes_data(wme.clone());
    let Some(mode_pane_owner) = Rc::<UnsafeCell<window_pane>>::from_observer(&(*data).wp) else {
        return;
    };
    let mut border_gc: grid_cell = grid_cell {
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
    let mut osx: u_int = 0;
    let mut osy: u_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let source_window =
        window_panes_get_source(data, std::ptr::null_mut(), &mut source_session_owner);
    if source_window.is_none() {
        if let Some(owner) = source_session_owner {
            drop(owner);
        }
        return;
    }
    let window = source_window
        .unwrap()
        .upgrade()
        .expect("source window remains owned");
    (|| {
        if window.pane_snapshot().is_empty() {
            return;
        }
        let strip = window
            .pane_cells()
            .iter()
            .map(|(_, cell)| (cell.xoff as u_int).wrapping_add(cell.sx))
            .max()
            .unwrap_or(0);
        (osx, osy) = window.size();
        osx = osx.max(strip);
        sx = (*data).screen.grid().sx;
        sy = (*data).screen.grid().sy;
        window_panes_free_areas(data);
        let mut ctx = screen_write_start(&mut (*data).screen);
        screen_write_clearscreen(&mut ctx, 8 as u_int);
        for pane in window
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            window_panes_draw_pane(data, &mut ctx, &pane, osx, osy, sx, sy);
        }
        window_panes_get_border_cell(data, &mut border_gc);
        window_panes_draw_borders(&mut ctx, &window, &border_gc, osx, osy, sx, sy);
        screen_write_stop(ctx);
        mode_pane_owner.request_redraw(false);
    })();
    drop(window);
    if let Some(owner) = source_session_owner {
        drop(owner);
    }
}
unsafe fn window_panes_timer_callback(wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    mode_pane_owner.reset_mode();
}
unsafe fn window_panes_init(
    mut wme: refbox::Weak<window_mode_entry>,
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    _fs: Option<&cmd_find_state>,
    args: Option<&mut args>,
) -> bool {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let original_window = mode_pane_owner.window_observer();
    let mut data: *mut window_panes_modedata = ::core::ptr::null_mut::<window_panes_modedata>();
    let mut self_0: refbox::Weak<cmd> = refbox::Weak::new();
    let mut source: *mut cmd_find_state = ::core::ptr::null_mut::<cmd_find_state>();
    let mut target: *mut cmd_find_state = ::core::ptr::null_mut::<cmd_find_state>();
    let mut s: Option<SessionRef> = None;
    let (sx, sy) = mode_pane_owner.screen_size(false);
    let mut delay: u_int = 0;
    let Some(item_owner) = item_handle else {
        return false;
    };
    let (mut source_state, mut target_state) = {
        let item = item_owner.borrow();
        self_0 = cmdq_get_cmd(&item);
        (
            crate::cmd::queue::cmdq_get_source(&item).clone(),
            crate::cmd::queue::cmdq_get_target(&item).clone(),
        )
    };
    if !self_0.is_alive() {
        return false;
    }
    source = &raw mut source_state;
    target = &raw mut target_state;
    s = (*target).session_handle();
    let args = args.expect("mode arguments");
    if args_has(&*args, 'd' as i32 as u_char) == 0 {
        delay = options_get_number(
            &original_window
                .upgrade()
                .expect("live display-panes window")
                .borrow_options_mut(),
            c"display-panes-time",
        ) as u_int;
    } else {
        delay = match args_strtonum_result(
            &*args,
            'd' as i32 as u_char,
            0 as ::core::ffi::c_longlong,
            UINT_MAX as ::core::ffi::c_longlong,
        ) {
            Ok(value) => value as u_int,
            Err(error) => {
                cmdq_error(item_handle.expect("command queue item"), |out| {
                    out.write_all(b"delay ")?;
                    write_cstr(out, error.message())
                });
                return false;
            }
        };
    }
    let owner = Box::new(std::cell::UnsafeCell::new(window_panes_modedata {
        wp: Weak::new(),
        session: Weak::new(),
        source_session: 0,
        source_window: 0,
        screen: screen::empty(),
        preview: None,
        timer: Default::default(),
        state: None,
        delay: 0,
        ignore_keys: 0,
        areas: Vec::new(),
    }));
    data = owner.get();
    wme.get_mut_unchecked().boxed_data = Some(owner);
    (*data).wp = Rc::downgrade(&mode_pane_owner);
    (*data).session = std::rc::Rc::downgrade(s.as_ref().expect("live session"));
    screen_init(&mut (*data).screen, sx, sy, 0 as u_int);
    (*data).screen.mode &= !MODE_CURSOR;
    (*data).state = Some(args_make_commands_prepare(
        self_0.clone(),
        (item_handle).expect("command queue item"),
        0 as u_int,
        Some(c"select-pane -t \"%%%\""),
        0 as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    ));
    if args_has(&*args, 's' as i32 as u_char) != 0 {
        (*data).source_session = (*source).session_handle().expect("live session").id();
        (*data).source_window = (((*source).window_handle().as_ref()).expect("live window")).id();
    } else {
        (*data).source_session = (*target).session_handle().expect("live session").id();
        (*data).source_window = (((*target).window_handle().as_ref()).expect("live window")).id();
    }
    (*data).delay = delay;
    (*data).ignore_keys = args_has(&*args, 'N' as i32 as u_char);
    window_panes_set_preview(data);
    if (*data).delay != 0 as u_int {
        let timeout = Duration::from_millis((*data).delay as u64);
        let mode_observer = wme.clone();
        (*data).timer = Some(
            Timer::new(timeout, move || unsafe {
                let live = match mode_observer.try_borrow_mut() {
                    Ok(_) => true,
                    Err(refbox::BorrowError::Dropped) => false,
                    Err(refbox::BorrowError::Borrowed) => {
                        panic!("display-panes mode already borrowed")
                    }
                };
                if live {
                    window_panes_timer_callback(mode_observer.clone());
                }
            })
            .expect("arm timer"),
        );
    }
    window_panes_draw_screen(wme.clone());
    true
}
unsafe fn window_panes_get_screen(
    wme: refbox::Weak<window_mode_entry>,
    visit: &mut dyn FnMut(&mut screen),
) {
    if let Some(data) = wme
        .get_unchecked()
        .boxed_data_ptr::<window_panes_modedata>()
    {
        visit(&mut (*data).screen);
    }
}

unsafe fn window_panes_free(wme: refbox::Weak<window_mode_entry>) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    server_redraw_window(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live pane parent"),
    );
    server_redraw_window_borders(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live pane parent"),
    );
    server_status_window(
        &mode_pane_owner
            .window_observer()
            .upgrade()
            .expect("live pane parent"),
    );
}
unsafe fn window_panes_resize(
    mut wme: refbox::Weak<window_mode_entry>,
    mut sx: u_int,
    mut sy: u_int,
) {
    let mut data: *mut window_panes_modedata = window_panes_data(wme.clone());
    screen_resize(&mut (*data).screen, sx, sy, 0 as ::core::ffi::c_int);
    window_panes_draw_screen(wme.clone());
}
unsafe fn window_panes_run_command(
    mut data: *mut window_panes_modedata,
    client_owner: &ClientRef,
    pane: u32,
) {
    let new_item_allocation;
    let expanded = CString::new(format!("%{}", pane)).expect("pane ID contains NUL");
    match args_make_commands(
        (*data)
            .state
            .as_deref_mut()
            .expect("prepared command state"),
        &vec![expanded],
    ) {
        Err(error) => {
            cmdq_append(
                Some(client_owner),
                cmdq_get_error(error.as_deref().expect("command error")),
            );
        }
        Ok(commands) => {
            let cmdlist = commands;
            new_item_allocation = cmdq_get_command(&cmdlist, None);
            cmdq_append(Some(client_owner), new_item_allocation);
            drop(cmdlist);
        }
    }
}
unsafe fn window_panes_find_pane(
    mut data: *mut window_panes_modedata,
    mut x: u_int,
    mut y: u_int,
) -> Option<Rc<UnsafeCell<window_pane>>> {
    for area in (*data).areas.iter().rev() {
        if !(x < area.x || x >= area.x.wrapping_add(area.sx))
            && !(y < area.y || y >= area.y.wrapping_add(area.sy))
        {
            return Rc::<UnsafeCell<window_pane>>::find_by_id(area.id);
        }
    }
    None
}
unsafe fn window_panes_key_pane(
    mut data: *mut window_panes_modedata,
    mut key: key_code,
) -> Option<Rc<UnsafeCell<window_pane>>> {
    let mut source_session_owner = None;
    let mut index: u_int = 0;
    if key >= '0' as i32 as key_code && key <= '9' as i32 as key_code {
        index = key.wrapping_sub('0' as i32 as key_code) as u_int;
    } else if key as ::core::ffi::c_ulonglong & KEYC_MASK_MODIFIERS == 0 as ::core::ffi::c_ulonglong
    {
        key &= KEYC_MASK_KEY;
        if key < 'a' as i32 as key_code || key > 'z' as i32 as key_code {
            return None;
        }
        index = (10 as key_code).wrapping_add(key.wrapping_sub('a' as i32 as key_code)) as u_int;
    } else {
        return None;
    }
    let source_window =
        window_panes_get_source(data, std::ptr::null_mut(), &mut source_session_owner);
    if source_window.is_none() {
        if let Some(owner) = source_session_owner {
            drop(owner);
        }
        return None;
    }
    let window_owner = source_window
        .unwrap()
        .upgrade()
        .expect("source window remains owned");
    let result = window_owner.pane_at_index(index).upgrade();
    drop(window_owner);
    if let Some(owner) = source_session_owner {
        drop(owner);
    }
    result
}
unsafe fn window_panes_get_target(
    mut wme: refbox::Weak<window_mode_entry>,
    mut key: key_code,
    m: Option<&mut mouse_event>,
) -> Option<Rc<UnsafeCell<window_pane>>> {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_panes_modedata = window_panes_data(wme.clone());
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if (*data).ignore_keys != 0 {
        return None;
    }
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
    {
        if key != KEYC_MOUSEDOWN1_PANE as ::core::ffi::c_ulong as key_code
            || m.is_none_or(|m| {
                cmd_mouse_at(&mode_pane_owner, m, &mut x, &mut y, 0 as ::core::ffi::c_int)
                    != 0 as ::core::ffi::c_int
            })
        {
            return None;
        }
        return window_panes_find_pane(data, x, y);
    }
    window_panes_key_pane(data, key)
}
unsafe fn window_panes_key(
    mut wme: refbox::Weak<window_mode_entry>,
    client_owner: &ClientRef,
    _wl: refbox::Weak<winlink>,
    mut key: key_code,
    m: Option<&mut mouse_event>,
) {
    let mode_pane_owner = wme
        .get_unchecked()
        .wp
        .upgrade()
        .expect("mode belongs to a live pane");
    let mut data: *mut window_panes_modedata = window_panes_data(wme.clone());
    if key == '\u{1b}' as i32 as key_code || key == 'q' as i32 as key_code {
        mode_pane_owner.reset_mode();
        return;
    }
    let target_owner = window_panes_get_target(wme.clone(), key, m);
    if target_owner.is_none() {
        if (*data).ignore_keys == 0
            && !(key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
                == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
                || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                    >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                        << 32 as ::core::ffi::c_int
                    && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                        <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int
                            as ::core::ffi::c_ulonglong)
                            << 32 as ::core::ffi::c_int)
        {
            mode_pane_owner.reset_mode();
        }
        return;
    }
    window_panes_run_command(
        data,
        client_owner,
        target_owner.as_ref().expect("display-panes target").id(),
    );
    mode_pane_owner.reset_mode();
}

#[cfg(test)]
mod preview_geometry_tests {
    use super::*;

    fn geometry(offset: (i32, i32), size: (u32, u32)) -> layout_geometry {
        layout_geometry {
            xoff: offset.0,
            yoff: offset.1,
            sx: size.0,
            sy: size.1,
        }
    }

    #[test]
    fn scaling_keeps_legacy_mixed_axis_and_minimum_cell_rules() {
        let g = geometry((4, 2), (10, 4));
        assert_eq!(
            window_panes_scaled_geometry(&g, 80, 24, 160, 48),
            Some((4, 2, 10, 4))
        );
        assert_eq!(
            window_panes_scaled_geometry(&g, 80, 24, 40, 12),
            Some((2, 1, 5, 2))
        );
        assert_eq!(
            window_panes_scaled_geometry(&g, 80, 24, 40, 48),
            Some((2, 4, 5, 8))
        );
        assert_eq!(
            window_panes_scaled_geometry(&geometry((1, 1), (1, 1)), 80, 24, 4, 2),
            Some((0, 0, 1, 1))
        );
        assert_eq!(
            window_panes_scaled_geometry(&geometry((79, 23), (20, 10)), 80, 24, 40, 12),
            Some((39, 11, 1, 1))
        );
    }

    #[test]
    fn scaling_rejects_empty_viewports_and_offscreen_origins() {
        let g = geometry((0, 0), (80, 24));
        for dimensions in [
            (0, 24, 40, 12),
            (80, 0, 40, 12),
            (80, 24, 0, 12),
            (80, 24, 40, 0),
        ] {
            assert_eq!(
                window_panes_scaled_geometry(
                    &g,
                    dimensions.0,
                    dimensions.1,
                    dimensions.2,
                    dimensions.3
                ),
                None
            );
        }
        assert_eq!(
            window_panes_scaled_geometry(&geometry((80, 0), (1, 1)), 80, 24, 40, 12),
            None
        );
        assert_eq!(
            window_panes_scaled_geometry(&geometry((0, 24), (1, 1)), 80, 24, 40, 12),
            None
        );
    }
}
