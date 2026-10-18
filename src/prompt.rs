use crate::src::cmd::cmd_table;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{
    format_add, format_create_defaults, format_create_from_state, format_expand_time_cstring,
};
use crate::src::format_draw::{format_draw, format_width};
use crate::src::grid::grid_default_cell;
use crate::src::key_string::key_string_format;
use crate::src::log::{log_cstr, log_debug};
use crate::src::options::options;
use crate::src::options::{options_get_number, options_get_string};
use crate::src::paste::{paste_buffer_data, paste_get_top};
use crate::src::prompt_history::{prompt_add_history, prompt_down_history, prompt_up_history};
use crate::src::screen::screen_set_cursor_style;
use crate::src::screen_write::{
    screen_write_cell, screen_write_clearcharacter, screen_write_cursormove,
};
use crate::src::shared::abi::ssize_t;
use crate::src::shared::abi::*;
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::format::format_tree;
use crate::src::shared::grid::*;
use crate::src::shared::key::MODEKEY_VI;
use crate::src::shared::key::*;
use crate::src::shared::prompt::prompt;
use crate::src::shared::prompt::*;
use crate::src::shared::prompt::{prompt_create_data, prompt_draw_data};
use crate::src::shared::prompt::{
    PROMPT_ACCEPT, PROMPT_BSPACE_EXIT, PROMPT_CLOSE, PROMPT_COMMANDMODE, PROMPT_EDITARROWS,
    PROMPT_INCREMENTAL, PROMPT_ISMODE, PROMPT_ISPANE, PROMPT_KEY, PROMPT_NOFORMAT, PROMPT_NOFREEZE,
    PROMPT_NUMERIC, PROMPT_QUOTENEXT, PROMPT_SINGLE,
};
use crate::src::shared::screen::screen;
use crate::src::shared::screen_write::screen_write_ctx;
use crate::src::shared::session::SessionRef;
use crate::src::shared::style::*;
use crate::src::shared::utf8::*;
use crate::src::style::{style_parse, style_set};
use crate::src::text::utf8::{
    utf8_append, utf8_copy, utf8_cstrwidth, utf8_fromcstr_vec, utf8_open, utf8_set, utf8_strlen,
    utf8_strwidth, utf8_to_data, utf8_tocstr_cstring,
};
use crate::src::tmux::{with_global_options, with_global_s_options};
use std::ffi::{CStr, CString};
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};

const PROMPT_CALLBACK_ACTIVE: ::core::ffi::c_int = 0x2000;
const PROMPT_FREE_PENDING: ::core::ffi::c_int = 0x4000;
const PROMPT_FREEING: ::core::ffi::c_int = 0x8000;

fn prompt_trim_buffer(pr: &mut prompt) {
    let len = utf8_strlen(&pr.buffer) + 1;
    pr.buffer.truncate(len);
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct prompt_layout {
    pub area_x: u_int,
    pub area_width: u_int,
    pub content_x: u_int,
    pub content_width: u_int,
    pub label_width: u_int,
    pub input_x: u_int,
    pub cursor_x: u_int,
    pub input_offset: u_int,
    pub input_width: u_int,
}

fn prompt_write_flags(
    out: &mut dyn std::io::Write,
    flags: ::core::ffi::c_int,
) -> std::io::Result<()> {
    let mut separator = "";
    for (flag, name) in [
        (PROMPT_SINGLE, "SINGLE"),
        (PROMPT_NUMERIC, "NUMERIC"),
        (PROMPT_INCREMENTAL, "INCREMENTAL"),
        (PROMPT_NOFORMAT, "NOFORMAT"),
        (PROMPT_KEY, "KEY"),
        (PROMPT_ACCEPT, "ACCEPT"),
        (PROMPT_QUOTENEXT, "QUOTENEXT"),
        (PROMPT_BSPACE_EXIT, "BSPACE_EXIT"),
        (PROMPT_NOFREEZE, "NOFREEZE"),
        (PROMPT_COMMANDMODE, "COMMANDMODE"),
        (PROMPT_ISPANE, "ISPANE"),
        (PROMPT_ISMODE, "ISMODE"),
        (PROMPT_EDITARROWS, "EDITARROWS"),
    ] {
        if flags & flag != 0 {
            out.write_all(separator.as_bytes())?;
            out.write_all(name.as_bytes())?;
            separator = ",";
        }
    }
    Ok(())
}
pub unsafe fn prompt_set_options(pd: &mut prompt_create_data<'_>, session: Option<&SessionRef>) {
    use crate::src::session::Session;
    let mut access = |visit: &mut dyn FnMut(&mut options)| {
        if let Some(session) = session {
            visit(&mut session.borrow_options_mut());
        } else {
            with_global_s_options(|oo| visit(oo));
        }
    };
    let read_string = |key: &CStr| {
        if let Some(session) = session {
            options_get_string(&session.borrow_options_mut(), key)
        } else {
            with_global_s_options(|oo| options_get_string(oo, key))
        }
    };
    let read_number = |key: &'static CStr| {
        if let Some(session) = session {
            options_get_number(&session.borrow_options_mut(), key)
        } else {
            with_global_s_options(|oo| options_get_number(oo, key))
        }
    };
    use crate::src::style::style_apply_with_options;
    style_apply_with_options(&mut pd.style, c"message-style", None, &mut access);
    style_apply_with_options(
        &mut pd.command_style,
        c"message-command-style",
        None,
        &mut access,
    );
    pd.style_str = read_string(c"message-style");
    pd.command_style_str = read_string(c"message-command-style");
    screen_set_cursor_style(
        read_number(c"prompt-cursor-style") as u_int,
        &mut pd.cstyle,
        &mut pd.cmode,
    );
    screen_set_cursor_style(
        read_number(c"prompt-command-cursor-style") as u_int,
        &mut pd.command_cstyle,
        &mut pd.command_cmode,
    );
    let mut gc = grid_cell::default();
    style_apply_with_options(&mut gc, c"prompt-cursor-colour", None, &mut access);
    pd.ccolour = gc.fg;
    style_apply_with_options(&mut gc, c"prompt-command-cursor-colour", None, &mut access);
    pd.command_ccolour = gc.fg;
    pd.message_format = read_string(c"message-format");
    pd.keys = read_number(c"status-keys") as i32;
    pd.word_separators = read_string(c"word-separators");
}
pub unsafe fn prompt_create(
    pd: prompt_create_data<'_>,
) -> refbox::RefBox<crate::src::shared::prompt::prompt> {
    let owner = refbox::RefBox::new(prompt::default());
    let mut pr = owner.try_borrow_mut().expect("live unborrowed prompt");
    let mut ft_owner = if let Some(fs) = pd.fs {
        pr.state = cmd_find_state {
            s: fs.s.clone(),
            wl: fs.wl.clone(),
            w: fs.w.clone(),
            wp: fs.wp.clone(),
            idx: fs.idx,
            ..Default::default()
        };
        format_create_defaults(
            None,
            None,
            fs.session_handle().as_ref(),
            (fs.winlink_handle()).clone(),
            fs.pane_handle().as_ref(),
        )
    } else {
        pr.state = cmd_find_state::clear_state(0);
        format_create_defaults(None, None, None, refbox::Weak::new(), None)
    };
    let ft = &raw mut *ft_owner;
    let input = pd.input.unwrap_or(c"");
    pr.string = pd.prompt.to_owned();
    let expanded = if pd.flags & PROMPT_NOFORMAT != 0 {
        None
    } else {
        Some(format_expand_time_cstring(ft, input.as_ptr()))
    };
    if pd.flags & PROMPT_INCREMENTAL != 0 {
        pr.last = Some(expanded.unwrap_or_else(|| input.to_owned()));
        pr.buffer = utf8_fromcstr_vec(c"");
    } else {
        pr.buffer = utf8_fromcstr_vec(expanded.as_deref().unwrap_or(input));
    }
    pr.index = utf8_strlen(&pr.buffer);
    pr.inputcb = pd.inputcb;
    pr.freecb = pd.freecb;
    pr.flags = pd.flags;
    pr.type_0 = pd.type_0;
    pr.style = pd.style;
    pr.command_style = pd.command_style;
    pr.style_str = pd.style_str;
    pr.command_style_str = pd.command_style_str;
    pr.cstyle = pd.cstyle;
    pr.command_cstyle = pd.command_cstyle;
    pr.ccolour = pd.ccolour;
    pr.command_ccolour = pd.command_ccolour;
    pr.cmode = pd.cmode;
    pr.command_cmode = pd.command_cmode;
    pr.message_format = pd.message_format;
    pr.keys = pd.keys;
    pr.word_separators = pd.word_separators;
    drop(ft_owner);
    drop(pr);
    owner
}
fn prompt_borrow_live(
    owner: &refbox::Weak<crate::src::shared::prompt::prompt>,
) -> Option<refbox::Borrow<'_, prompt>> {
    match owner.try_borrow_mut() {
        Ok(prompt) => Some(prompt),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("prompt already borrowed"),
    }
}
pub fn prompt_free(owner: &refbox::Weak<crate::src::shared::prompt::prompt>) {
    let callback = {
        let Some(mut pr) = prompt_borrow_live(owner) else {
            return;
        };
        if pr.flags & PROMPT_FREEING != 0 {
            return;
        }
        if pr.flags & PROMPT_CALLBACK_ACTIVE != 0 {
            pr.flags |= PROMPT_FREE_PENDING;
            return;
        }
        pr.flags |= PROMPT_FREEING;
        pr.closed = 1;
        pr.freecb.take()
    };
    if let Some(callback) = callback {
        callback();
    }
    let inputcb = {
        let Some(mut pr) = prompt_borrow_live(owner) else {
            return;
        };
        prompt_clear_complete(&mut pr);
        pr.inputcb.take()
    };
    drop(inputcb);
}
fn prompt_last(pr: &prompt) -> &CStr {
    pr.last
        .as_ref()
        .expect("incremental prompt has saved input")
        .as_c_str()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PromptCallbackResult {
    Continue,
    Close,
    Freed,
}

impl PromptCallbackResult {
    fn key_result(self) -> prompt_key_result {
        match self {
            Self::Continue => PROMPT_KEY_HANDLED,
            Self::Close | Self::Freed => PROMPT_KEY_CLOSE,
        }
    }
}

fn prompt_fire_callback(
    owner: &refbox::Weak<crate::src::shared::prompt::prompt>,
    text: Option<&CStr>,
    kind: prompt_key_result,
    redraw: Option<&mut ::core::ffi::c_int>,
) -> PromptCallbackResult {
    let (mut callback, freecb) = {
        let Some(mut pr) = prompt_borrow_live(owner) else {
            return PromptCallbackResult::Freed;
        };
        if pr.flags & PROMPT_FREEING != 0 {
            return PromptCallbackResult::Freed;
        }
        let callback = pr.inputcb.take().expect("non-null prompt callback");
        pr.flags |= PROMPT_CALLBACK_ACTIVE;
        (callback, pr.freecb.take())
    };
    let result = callback(text, kind);
    let free_pending = {
        let Some(mut pr) = prompt_borrow_live(owner) else {
            if let Some(freecb) = freecb {
                freecb();
            }
            return PromptCallbackResult::Freed;
        };
        pr.freecb = freecb;
        pr.flags &= !PROMPT_CALLBACK_ACTIVE;
        pr.flags & PROMPT_FREE_PENDING != 0
    };
    if free_pending {
        prompt_free(owner);
        return PromptCallbackResult::Freed;
    }
    let mut pr = owner.try_borrow_mut().expect("live unborrowed prompt");
    if pr.inputcb.is_none() {
        pr.inputcb = Some(callback);
    }
    if result == PROMPT_CLOSE {
        pr.closed = 1;
        return PromptCallbackResult::Close;
    }
    if let Some(redraw) = redraw {
        *redraw = 1;
    }
    PromptCallbackResult::Continue
}

pub fn prompt_incremental_start(owner: &refbox::Weak<crate::src::shared::prompt::prompt>) {
    let callback_input = {
        let Some(pr) = prompt_borrow_live(owner) else {
            return;
        };
        if pr.flags & PROMPT_INCREMENTAL == 0 || pr.flags & PROMPT_FREEING != 0 {
            return;
        }
        prompt_incremental_input(&pr, b'=')
    };
    prompt_fire_callback(owner, Some(&callback_input), PROMPT_KEY_HANDLED, None);
}

fn prompt_incremental_input(pr: &prompt, prefix: u8) -> CString {
    let input = utf8_tocstr_cstring(&pr.buffer);
    let mut bytes = Vec::with_capacity(input.as_bytes().len() + 1);
    bytes.push(prefix);
    bytes.extend_from_slice(input.as_bytes());
    CString::new(bytes).expect("the first NUL ends the prompt input")
}
pub unsafe fn prompt_update(pr: &mut prompt, msg: &CStr, input: Option<&CStr>) {
    let mut ft_owner = if pr.state.is_valid() {
        format_create_from_state(None, None, &pr.state)
    } else {
        format_create_defaults(None, None, None, refbox::Weak::new(), None)
    };
    let ft = &raw mut *ft_owner;
    pr.string = msg.to_owned();
    let input = input.unwrap_or(c"");
    let expanded = if pr.flags & PROMPT_NOFORMAT != 0 {
        None
    } else {
        Some(format_expand_time_cstring(ft, input.as_ptr()))
    };
    pr.buffer = utf8_fromcstr_vec(expanded.as_deref().unwrap_or(input));
    pr.index = utf8_strlen(&pr.buffer);
    pr.hindex.fill(0);
    pr.closed = 0;
    prompt_clear_complete(pr);
    drop(ft_owner);
}
pub fn prompt_closed(pr: &prompt) -> ::core::ffi::c_int {
    pr.closed
}
unsafe fn prompt_redraw_character(
    ctx: &mut screen_write_ctx,
    mut offset: u_int,
    mut pwidth: u_int,
    width: &mut u_int,
    gc: &mut grid_cell,
    ud: &utf8_data,
) -> ::core::ffi::c_int {
    let mut ch: u_char = 0;
    if *width < offset {
        *width = (*width).wrapping_add(ud.width as u_int);
        return 1 as ::core::ffi::c_int;
    }
    if *width >= offset.wrapping_add(pwidth) {
        return 0 as ::core::ffi::c_int;
    }
    *width = (*width).wrapping_add(ud.width as u_int);
    if *width > offset.wrapping_add(pwidth) {
        return 0 as ::core::ffi::c_int;
    }
    ch = ud.data[0];
    if ud.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
        && (ch as ::core::ffi::c_int <= 0x1f as ::core::ffi::c_int
            || ch as ::core::ffi::c_int == 0x7f as ::core::ffi::c_int)
    {
        gc.data.data[0 as ::core::ffi::c_int as usize] = '^' as i32 as u_char;
        gc.data.data[1 as ::core::ffi::c_int as usize] =
            (if ch as ::core::ffi::c_int == 0x7f as ::core::ffi::c_int {
                '?' as i32
            } else {
                ch as ::core::ffi::c_int | 0x40 as ::core::ffi::c_int
            }) as u_char;
        gc.data.have = 2 as u_char;
        gc.data.size = gc.data.have;
        gc.data.width = 2 as u_char;
    } else {
        gc.data = utf8_copy(ud);
    }
    screen_write_cell(ctx, gc);
    1 as ::core::ffi::c_int
}
unsafe fn prompt_redraw_quote(
    pr: &prompt,
    mut pcursor: u_int,
    mut input_x: u_int,
    ctx: &mut screen_write_ctx,
    mut offset: u_int,
    mut pw: u_int,
    w: &mut u_int,
    gc: &mut grid_cell,
) -> ::core::ffi::c_int {
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    if pr.flags & PROMPT_QUOTENEXT != 0
        && pcursor >= offset
        && (*ctx.screen_ptr()).cx == input_x.wrapping_add(pcursor).wrapping_sub(offset)
    {
        utf8_set(&mut ud, '^' as i32 as u_char);
        return prompt_redraw_character(ctx, offset, pw, w, gc, &ud);
    }
    1 as ::core::ffi::c_int
}
unsafe fn prompt_draw_complete(
    pr: &prompt,
    ctx: &mut screen_write_ctx,
    mut ax: u_int,
    mut aw: u_int,
    mut cx: u_int,
    mut py: u_int,
    base: &grid_cell,
) {
    let mut gc = *base;
    let mut avail: u_int = 0;
    let mut width: u_int = 0;
    let Some(display) = pr.completion.display.as_deref() else {
        return;
    };
    if pr.index != utf8_strlen(&pr.buffer) {
        return;
    }
    if cx < ax || cx.wrapping_sub(ax) >= aw {
        return;
    }
    avail = aw.wrapping_sub(cx.wrapping_sub(ax));
    gc.attr = (gc.attr as ::core::ffi::c_int | GRID_ATTR_UNDERSCORE) as u_short;
    screen_write_cursormove(
        ctx,
        cx as ::core::ffi::c_int,
        py as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    width = 0 as u_int;
    let mut cells = utf8_fromcstr_vec(display);
    for cell in &mut cells {
        if cell.size == 0 || width.wrapping_add(cell.width as u_int) > avail {
            break;
        }
        gc.data = utf8_copy(cell);
        screen_write_cell(ctx, &gc);
        width = width.wrapping_add(cell.width as u_int);
    }
}
unsafe fn prompt_format_tree(pr: &prompt) -> Box<format_tree> {
    let mut owner;
    if pr.state.is_valid() {
        owner = format_create_from_state(None, None, &pr.state);
    } else {
        owner = format_create_defaults(None, None, None, (refbox::Weak::new()).clone(), None);
    }
    let ft = &raw mut *owner;
    let tmp = utf8_tocstr_cstring(&pr.buffer);
    format_add(ft, c"prompt_input", |out| write_cstr(out, &*tmp));
    format_add(ft, c"prompt_flags", |out| prompt_write_flags(out, pr.flags));
    format_add(ft, c"prompt_type", |out| {
        out.write_all(prompt_type_string(pr.type_0).to_bytes())
    });
    if pr.flags & PROMPT_COMMANDMODE != 0 {
        format_add(ft, c"command_prompt", |out| out.write_all(b"1"));
    } else {
        format_add(ft, c"command_prompt", |out| out.write_all(b"0"));
    }
    owner
}
unsafe fn prompt_expand1(pr: &prompt, mut ft: *mut format_tree) -> CString {
    let prompt = format_expand_time_cstring(ft, pr.string.as_ptr());
    format_add(ft, c"message", |out| write_cstr(out, &*prompt));
    format_expand_time_cstring(ft, pr.message_format.as_ptr())
}
unsafe fn prompt_effective_style(pr: &prompt, sy: &mut style, ft: *mut format_tree) {
    let (text, cell) = if pr.flags & PROMPT_COMMANDMODE != 0 {
        (&pr.command_style_str, &pr.command_style)
    } else {
        (&pr.style_str, &pr.style)
    };
    style_set(sy, cell);
    let expanded = format_expand_time_cstring(ft, text.as_ptr());
    if style_parse(sy, &grid_default_cell, &expanded) != 0 {
        style_set(sy, cell);
    }
}
unsafe fn prompt_layout(
    pr: &prompt,
    mut ax: u_int,
    mut aw: u_int,
    mut sy: Option<&mut style>,
) -> (prompt_layout, CString) {
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut pcursor: u_int = 0;
    let mut pwidth: u_int = 0;
    let mut end: u_int = 0;
    let mut width: u_int = 0;
    let mut offset: u_int = 0;
    let mut avail: u_int = 0;
    let mut pl = prompt_layout {
        area_x: ax,
        area_width: aw,
        ..Default::default()
    };
    let mut ft_owner = prompt_format_tree(pr);
    ft = &raw mut *ft_owner;
    if let Some(sy) = sy.as_deref_mut() {
        prompt_effective_style(pr, sy, ft);
    }
    let expanded = prompt_expand1(pr, ft);
    drop(ft_owner);
    if aw == 0 as u_int {
        return (pl, expanded);
    }
    pl.label_width = format_width(expanded.as_ptr());
    if pl.label_width > aw {
        pl.label_width = aw;
    }
    pcursor = utf8_strwidth(&pr.buffer, pr.index as ssize_t);
    pwidth = utf8_strwidth(&pr.buffer, -(1 as ::core::ffi::c_int) as ssize_t);
    if pr.flags & PROMPT_QUOTENEXT != 0 {
        pwidth = pwidth.wrapping_add(1);
    }
    avail = aw.wrapping_sub(pl.label_width);
    if avail == 0 as u_int {
        pl.input_offset = 0 as u_int;
        pl.input_width = 0 as u_int;
        pl.cursor_x = pl.label_width;
    } else {
        if pcursor >= avail {
            offset = pcursor.wrapping_sub(avail).wrapping_add(1 as u_int);
            width = avail;
        } else {
            offset = 0 as u_int;
            width = pwidth;
        }
        if width > avail {
            width = avail;
        }
        pl.input_offset = offset;
        pl.input_width = width;
        pl.cursor_x = pl.label_width.wrapping_add(pcursor).wrapping_sub(offset);
    }
    pl.content_width = pl.label_width.wrapping_add(pl.input_width);
    if let Some(display) = pr
        .completion
        .display
        .as_deref()
        .filter(|_| pr.index == utf8_strlen(&pr.buffer) && pl.cursor_x < aw)
    {
        avail = aw.wrapping_sub(pl.cursor_x);
        width = utf8_cstrwidth(display);
        if width > avail {
            width = avail;
        }
        end = pl.cursor_x.wrapping_add(width);
        if end > pl.content_width {
            pl.content_width = end;
        }
    }
    if pl.content_width > aw {
        pl.content_width = aw;
    }
    if let Some(sy) = sy.as_ref() {
        match sy.align as ::core::ffi::c_uint {
            2 | 4 => {
                pl.content_x =
                    ax.wrapping_add(aw.wrapping_sub(pl.content_width).wrapping_div(2 as u_int));
            }
            3 => {
                pl.content_x = ax.wrapping_add(aw).wrapping_sub(pl.content_width);
            }
            _ => {
                pl.content_x = ax;
            }
        }
    } else {
        pl.content_x = ax;
    }
    pl.input_x = pl.content_x.wrapping_add(pl.label_width);
    pl.cursor_x = pl.cursor_x.wrapping_add(pl.content_x);
    (pl, expanded)
}
unsafe fn prompt_mouse_complete(
    pr: &mut prompt,
    mut x: u_int,
    mut cx: u_int,
    mut ax: u_int,
    mut aw: u_int,
    mut redraw: Option<&mut ::core::ffi::c_int>,
) -> prompt_key_result {
    let mut avail: u_int = 0;
    let mut clicked: u_int = 0;
    let mut end: u_int = 0;
    let mut i: u_int = 0;
    let mut start: u_int = 0;
    let mut width: u_int = 0;
    let Some(display) = pr.completion.display.as_deref() else {
        return PROMPT_KEY_NOT_HANDLED;
    };
    if pr.completion.names.is_empty() {
        return PROMPT_KEY_NOT_HANDLED;
    }
    if pr.index != utf8_strlen(&pr.buffer) {
        return PROMPT_KEY_NOT_HANDLED;
    }
    if cx < ax || cx.wrapping_sub(ax) >= aw || x < cx {
        return PROMPT_KEY_NOT_HANDLED;
    }
    avail = aw.wrapping_sub(cx.wrapping_sub(ax));
    clicked = x.wrapping_sub(cx);
    width = utf8_cstrwidth(display);
    if width > avail {
        width = avail;
    }
    if clicked >= width {
        return PROMPT_KEY_NOT_HANDLED;
    }
    end = 0 as u_int;
    i = 0 as u_int;
    while (i as usize) < pr.completion.names.len() {
        start = end.wrapping_add(1 as u_int);
        end = start.wrapping_add(utf8_cstrwidth(&pr.completion.names[i as usize]));
        if clicked < start || clicked >= end {
            i = i.wrapping_add(1);
        } else {
            let mut replacement = pr.completion.names[i as usize].as_bytes().to_vec();
            replacement.push(b' ');
            let replacement = CString::new(replacement).expect("completion name contains no NUL");
            if prompt_replace_complete(pr, Some(&replacement)) != 0 {
                prompt_clear_complete(pr);
                if let Some(redraw) = redraw {
                    *redraw = 1 as ::core::ffi::c_int;
                }
            }
            return PROMPT_KEY_HANDLED;
        }
    }
    PROMPT_KEY_HANDLED
}
pub(crate) struct PreparedPromptDraw {
    layout: prompt_layout,
    expanded: CString,
    style: style,
}

pub(crate) fn prompt_draw_cursor_style(pr: &prompt, s: &mut screen) {
    if pr.flags & PROMPT_COMMANDMODE != 0 {
        s.default_cstyle = pr.command_cstyle;
        s.default_mode = pr.command_cmode;
        s.default_ccolour = pr.command_ccolour;
    } else {
        s.default_cstyle = pr.cstyle;
        s.default_mode = pr.cmode;
        s.default_ccolour = pr.ccolour;
    }
}

/// Expand formats before borrowing the destination model's screen.
pub(crate) unsafe fn prompt_prepare_draw(pr: &prompt, ax: u_int, aw: u_int) -> PreparedPromptDraw {
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
    let (layout, expanded) = prompt_layout(pr, ax, aw, Some(&mut sy));
    PreparedPromptDraw {
        layout,
        expanded,
        style: sy,
    }
}

/// Draw already-expanded text; this stage does not invoke format callbacks.
pub(crate) unsafe fn prompt_draw_prepared(
    pr: &prompt,
    ctx: &mut screen_write_ctx,
    pd: prompt_draw_data,
    prepared: PreparedPromptDraw,
) -> u_int {
    let ax = pd.area_x;
    let py = pd.prompt_line;
    let aw = pd.area_width;
    let PreparedPromptDraw {
        layout: pl,
        expanded,
        style: sy,
    } = prepared;
    let mut width = 0;
    let mut pcursor = 0;
    let mut gc = sy.gc;
    screen_write_cursormove(
        ctx,
        ax as ::core::ffi::c_int,
        py as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    if sy.fill != 8 as ::core::ffi::c_int {
        screen_write_clearcharacter(ctx, aw, sy.fill as u_int);
    }
    pcursor = utf8_strwidth(&pr.buffer, pr.index as ssize_t);
    if pl.content_width != 0 as u_int {
        screen_write_cursormove(
            ctx,
            pl.content_x as ::core::ffi::c_int,
            py as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        if pl.label_width != 0 as u_int {
            format_draw(
                ctx,
                &gc,
                pl.label_width,
                expanded.as_ptr(),
                ::core::ptr::null_mut::<style_ranges>(),
                0 as ::core::ffi::c_int,
            );
        }
        screen_write_cursormove(
            ctx,
            pl.input_x as ::core::ffi::c_int,
            py as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
        );
        width = 0 as u_int;
        for cell in pr.buffer.iter().take_while(|cell| cell.size != 0) {
            if prompt_redraw_quote(
                pr,
                pcursor,
                pl.input_x,
                ctx,
                pl.input_offset,
                pl.input_width,
                &mut width,
                &mut gc,
            ) == 0
            {
                break;
            }
            if prompt_redraw_character(
                ctx,
                pl.input_offset,
                pl.input_width,
                &mut width,
                &mut gc,
                cell,
            ) == 0
            {
                break;
            }
        }
        prompt_redraw_quote(
            pr,
            pcursor,
            pl.input_x,
            ctx,
            pl.input_offset,
            pl.input_width,
            &mut width,
            &mut gc,
        );
        prompt_draw_complete(
            pr,
            ctx,
            pl.content_x,
            pl.content_width,
            pl.cursor_x,
            py,
            &gc,
        );
    }
    pl.cursor_x
}

pub unsafe fn prompt_draw(pr: &prompt, ctx: &mut screen_write_ctx, pd: prompt_draw_data) -> u_int {
    prompt_draw_cursor_style(pr, &mut *ctx.screen_ptr());
    let prepared = prompt_prepare_draw(pr, pd.area_x, pd.area_width);
    prompt_draw_prepared(pr, ctx, pd, prepared)
}
pub unsafe fn prompt_mouse(
    pr: &mut prompt,
    mut x: u_int,
    mut ax: u_int,
    mut aw: u_int,
    mut redraw: Option<&mut ::core::ffi::c_int>,
) -> prompt_key_result {
    let mut result: prompt_key_result = PROMPT_KEY_NOT_HANDLED;
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
    let mut pwidth: u_int = 0;
    let mut width: u_int = 0;
    let mut target: u_int = 0;
    let mut idx: size_t = 0;
    if x < ax || x >= ax.wrapping_add(aw) {
        return PROMPT_KEY_NOT_HANDLED;
    }
    let (pl, _) = prompt_layout(pr, ax, aw, Some(&mut sy));
    if pl.input_width == 0 as u_int {
        return PROMPT_KEY_HANDLED;
    }
    pwidth = utf8_strwidth(&pr.buffer, -(1 as ::core::ffi::c_int) as ssize_t);
    if pr.flags & PROMPT_QUOTENEXT != 0 {
        pwidth = pwidth.wrapping_add(1);
    }
    result = prompt_mouse_complete(
        pr,
        x,
        pl.cursor_x,
        pl.content_x,
        pl.content_width,
        redraw.as_deref_mut(),
    );
    if result as ::core::ffi::c_uint
        != PROMPT_KEY_NOT_HANDLED as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return result;
    }
    if x <= pl.input_x {
        target = pl.input_offset;
    } else {
        target = pl.input_offset.wrapping_add(x).wrapping_sub(pl.input_x);
    }
    if target > pwidth {
        target = pwidth;
    }
    width = 0 as u_int;
    idx = 0 as size_t;
    for cell in pr.buffer.iter().take_while(|cell| cell.size != 0) {
        if width >= target {
            break;
        }
        width = width.wrapping_add(cell.width as u_int);
        idx += 1;
    }
    if idx == pr.index {
        return PROMPT_KEY_HANDLED;
    }
    pr.index = idx;
    prompt_clear_complete(pr);
    if let Some(redraw) = redraw {
        *redraw = 1 as ::core::ffi::c_int;
    }
    PROMPT_KEY_HANDLED
}
fn prompt_in_list(separators: &CStr, cell: &utf8_data) -> ::core::ffi::c_int {
    (cell.size == 1 && cell.width == 1 && separators.to_bytes_with_nul().contains(&cell.data[0]))
        as ::core::ffi::c_int
}
fn prompt_space(cell: &utf8_data) -> ::core::ffi::c_int {
    (cell.size == 1 && cell.width == 1 && cell.data[0] == b' ') as ::core::ffi::c_int
}
fn prompt_keypad_key(key: key_code) -> key_code {
    if key & KEYC_MASK_MODIFIERS != 0 {
        return key;
    }
    match key {
        KEYC_KP_SLASH => b'/' as key_code,
        KEYC_KP_STAR => b'*' as key_code,
        KEYC_KP_MINUS => b'-' as key_code,
        KEYC_KP_SEVEN => b'7' as key_code,
        KEYC_KP_EIGHT => b'8' as key_code,
        KEYC_KP_NINE => b'9' as key_code,
        KEYC_KP_PLUS => b'+' as key_code,
        KEYC_KP_FOUR => b'4' as key_code,
        KEYC_KP_FIVE => b'5' as key_code,
        KEYC_KP_SIX => b'6' as key_code,
        KEYC_KP_ONE => b'1' as key_code,
        KEYC_KP_TWO => b'2' as key_code,
        KEYC_KP_THREE => b'3' as key_code,
        KEYC_KP_ENTER => b'\r' as key_code,
        KEYC_KP_ZERO => b'0' as key_code,
        KEYC_KP_PERIOD => b'.' as key_code,
        _ => key,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PromptTranslatedKey {
    Handled,
    Process(key_code),
    Append(key_code),
}

fn prompt_translate_key(
    pr: &mut prompt,
    key: key_code,
    redraw: &mut ::core::ffi::c_int,
) -> PromptTranslatedKey {
    use PromptTranslatedKey::{Append, Handled, Process};

    if pr.flags & PROMPT_COMMANDMODE == 0 {
        if key == 27 || key == KEYC_CTRL | b'[' as key_code {
            pr.flags |= PROMPT_COMMANDMODE;
            pr.index = pr.index.saturating_sub(1);
            *redraw = 1;
            return Handled;
        }
        if matches!(
            key,
            9 | 10
                | 13
                | KEYC_BSPACE
                | KEYC_DC
                | KEYC_DOWN
                | KEYC_END
                | KEYC_HOME
                | KEYC_LEFT
                | KEYC_RIGHT
                | KEYC_UP
        ) || key == KEYC_LEFT | KEYC_CTRL
            || key == KEYC_RIGHT | KEYC_CTRL
            || b"aceghknptuvwy"
                .iter()
                .any(|&byte| key == KEYC_CTRL | byte as key_code)
        {
            return Process(key);
        }
        return Append(key);
    }

    match key {
        KEYC_BSPACE => return Process(KEYC_LEFT),
        KEYC_DC | KEYC_DOWN | KEYC_LEFT | KEYC_RIGHT | KEYC_UP | 10 | 13 => {
            return Process(key);
        }
        _ => {}
    }
    if key == KEYC_CTRL | b'h' as key_code || key == KEYC_CTRL | b'c' as key_code {
        return Process(key);
    }
    let Ok(byte) = u8::try_from(key) else {
        return Handled;
    };
    match byte {
        b'A' | b'I' | b'C' | b's' | b'a' | b'S' | b'i' => {
            pr.flags &= !PROMPT_COMMANDMODE;
            *redraw = 1;
        }
        _ => {}
    }
    match byte {
        b'A' | b'$' => Process(KEYC_END),
        b'I' | b'0' | b'^' => Process(KEYC_HOME),
        b'C' | b'D' => Process(KEYC_CTRL | b'k' as key_code),
        b'X' => Process(KEYC_BSPACE),
        b'b' => Process(KEYC_META | b'b' as key_code),
        b'B' => Process(KEYC_VI | b'B' as key_code),
        b'd' | b'S' => Process(KEYC_CTRL | b'u' as key_code),
        b'e' | b'E' | b'w' | b'W' => Process(KEYC_VI | key),
        b'p' => Process(KEYC_CTRL | b'y' as key_code),
        b'q' => Process(KEYC_CTRL | b'c' as key_code),
        b's' | b'x' => Process(KEYC_DC),
        b'j' => Process(KEYC_DOWN),
        b'h' => Process(KEYC_LEFT),
        b'a' | b'l' => Process(KEYC_RIGHT),
        b'k' => Process(KEYC_UP),
        _ => Handled,
    }
}
fn prompt_save_copied(pr: &mut prompt, idx: size_t) {
    let count = pr.index.wrapping_sub(idx);
    let empty = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut copied = vec![empty; count.wrapping_add(1)];
    let buffer = &pr.buffer;
    copied[..count].copy_from_slice(&buffer[idx..idx + count]);
    pr.copied = Some(copied.into_boxed_slice());
}

fn prompt_decode_paste(data: &[u8]) -> Vec<utf8_data> {
    let mut cells = vec![utf8_data::default(); data.len().wrapping_add(1)];
    let mut count = 0;
    let mut i: u_int = 0;
    while i as usize != data.len() {
        let cell = &mut cells[count];
        let mut more = utf8_open(cell, data[i as usize]);
        if more == UTF8_MORE {
            loop {
                i = i.wrapping_add(1);
                if i as usize == data.len() || more != UTF8_MORE {
                    break;
                }
                more = utf8_append(cell, data[i as usize]);
            }
            if more == UTF8_DONE {
                count += 1;
                continue;
            }
            i = i.wrapping_sub(cell.have as u_int);
        }
        if !(b' '..=b'~').contains(&data[i as usize]) {
            break;
        }
        utf8_set(cell, data[i as usize]);
        count += 1;
        i = i.wrapping_add(1);
    }
    cells[count].size = 0;
    cells.truncate(count + 1);
    cells
}

unsafe fn prompt_paste(pr: &mut prompt) -> ::core::ffi::c_int {
    let decoded;
    let pasted = if let Some(copied) = pr.copied.as_deref() {
        &copied[..utf8_strlen(copied)]
    } else {
        let Some(buffer) = paste_get_top(None) else {
            return 0;
        };
        let buffer = buffer.borrow();
        let bytes = paste_buffer_data(&buffer).unwrap_or_default();
        decoded = prompt_decode_paste(bytes);
        &decoded[..decoded.len() - 1]
    };
    if !pasted.is_empty() {
        pr.buffer.splice(pr.index..pr.index, pasted.iter().copied());
        pr.index = pr.index.wrapping_add(pasted.len());
    }
    1
}
unsafe fn prompt_replace_complete(
    pr: &mut prompt,
    replacement: Option<&CStr>,
) -> ::core::ffi::c_int {
    let mut word = [0_u8; 64];
    let size = utf8_strlen(&pr.buffer);
    let index = pr.index.saturating_sub(1);
    let mut first = index;
    while first > 0 && prompt_space(&pr.buffer[first]) == 0 {
        first -= 1;
    }
    while pr.buffer[first].size != 0 && prompt_space(&pr.buffer[first]) != 0 {
        first += 1;
    }
    let mut last = index;
    while pr.buffer[last].size != 0 && prompt_space(&pr.buffer[last]) == 0 {
        last += 1;
    }
    while last > 0 && prompt_space(&pr.buffer[last]) != 0 {
        last -= 1;
    }
    if pr.buffer[last].size != 0 {
        last += 1;
    }
    if last < first {
        return 0;
    }
    let completed = if replacement.is_none() {
        let mut used = 0;
        for cell in &pr.buffer[first..last] {
            let bytes = &cell.data[..cell.size as usize];
            if used + bytes.len() >= word.len() {
                return 0;
            }
            word[used..used + bytes.len()].copy_from_slice(bytes);
            used += bytes.len();
        }
        let word =
            CStr::from_bytes_until_nul(&word[..used + 1]).expect("terminated completion word");
        prompt_complete(pr, word, first as u_int)
    } else {
        None
    };
    let Some(replacement) = replacement.or(completed.as_deref()) else {
        return 0;
    };
    let replacement_bytes = replacement.to_bytes();
    let mut cells = Vec::with_capacity(first + replacement_bytes.len() + size + 1 - last);
    cells.extend_from_slice(&pr.buffer[..first]);
    for &byte in replacement_bytes {
        let mut cell = utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        };
        utf8_set(&mut cell, byte);
        cells.push(cell);
    }
    cells.extend_from_slice(&pr.buffer[last..=size]);
    pr.buffer = cells;
    pr.index = first + replacement_bytes.len();
    1
}
fn prompt_forward_word(pr: &prompt, size: usize, vi: bool, separators: &CStr) -> usize {
    let buffer = &pr.buffer;
    let mut index = pr.index;
    if !vi {
        while index != size && prompt_space(&buffer[index]) != 0 {
            index += 1;
        }
    }
    if index == size {
        return index;
    }
    let word_is_separators =
        prompt_in_list(separators, &buffer[index]) != 0 && prompt_space(&buffer[index]) == 0;
    loop {
        index += 1;
        if prompt_space(&buffer[index]) != 0 {
            if vi {
                while index != size && prompt_space(&buffer[index]) != 0 {
                    index += 1;
                }
            }
            break;
        }
        if index == size || word_is_separators != (prompt_in_list(separators, &buffer[index]) != 0)
        {
            break;
        }
    }
    index
}
fn prompt_end_word(pr: &prompt, size: usize, separators: &CStr) -> usize {
    let buffer = &pr.buffer;
    let mut index = pr.index;
    if index == size {
        return index;
    }
    loop {
        index += 1;
        if index == size {
            return index;
        }
        if prompt_space(&buffer[index]) == 0 {
            break;
        }
    }
    let word_is_separators = prompt_in_list(separators, &buffer[index]);
    loop {
        index += 1;
        if index == size
            || prompt_space(&buffer[index]) != 0
            || word_is_separators != prompt_in_list(separators, &buffer[index])
        {
            break;
        }
    }
    index - 1
}
fn prompt_backward_word(pr: &prompt, separators: &CStr) -> usize {
    let buffer = &pr.buffer;
    let mut index = pr.index;
    while index != 0 {
        index -= 1;
        if prompt_space(&buffer[index]) == 0 {
            break;
        }
    }
    let word_is_separators = prompt_in_list(separators, &buffer[index]);
    while index != 0 {
        index -= 1;
        if prompt_space(&buffer[index]) != 0
            || word_is_separators != prompt_in_list(separators, &buffer[index])
        {
            index += 1;
            break;
        }
    }
    index
}
fn prompt_done(
    pr: &refbox::Weak<crate::src::shared::prompt::prompt>,
    text: Option<&CStr>,
    redraw: &mut ::core::ffi::c_int,
) -> PromptCallbackResult {
    prompt_fire_callback(pr, text, PROMPT_KEY_CLOSE, Some(redraw))
}
unsafe fn prompt_done_with_history(
    pr: &refbox::Weak<crate::src::shared::prompt::prompt>,
    redraw: &mut ::core::ffi::c_int,
) -> prompt_key_result {
    let (input, kind) = {
        let pr = pr.try_borrow_mut().expect("live unborrowed prompt");
        (utf8_tocstr_cstring(&pr.buffer), pr.type_0)
    };
    if !input.is_empty() {
        prompt_add_history(&input, kind);
    }
    prompt_done(pr, Some(&input), redraw).key_result()
}
fn prompt_check_move(
    owner: &refbox::Weak<crate::src::shared::prompt::prompt>,
    key: key_code,
) -> prompt_key_result {
    let input = {
        let pr = owner.try_borrow_mut().expect("live unborrowed prompt");
        if pr.flags & PROMPT_INCREMENTAL == 0 {
            return PROMPT_KEY_NOT_HANDLED;
        }
        match key {
            KEYC_UP | KEYC_DOWN | KEYC_PPAGE | KEYC_NPAGE => {}
            KEYC_LEFT | KEYC_RIGHT if pr.flags & PROMPT_EDITARROWS == 0 => {}
            _ => return PROMPT_KEY_NOT_HANDLED,
        }
        utf8_tocstr_cstring(&pr.buffer)
    };
    match prompt_fire_callback(owner, Some(&input), PROMPT_KEY_MOVE, None) {
        PromptCallbackResult::Continue => PROMPT_KEY_MOVE,
        PromptCallbackResult::Close | PromptCallbackResult::Freed => PROMPT_KEY_CLOSE,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PromptEditAction {
    Unchanged,
    Changed(u8),
    Append(key_code),
    Submit,
    Cancel,
}

/// Edit cells without invoking an input callback. The caller releases this
/// borrow before dispatching actions which may close or replace the prompt.
unsafe fn prompt_edit_key(pr: &mut prompt, key: key_code) -> PromptEditAction {
    let size = utf8_strlen(&pr.buffer);
    match key {
        k if k == KEYC_LEFT || k == KEYC_CTRL | b'b' as key_code => {
            pr.index = pr.index.saturating_sub(1);
            return PromptEditAction::Unchanged;
        }
        k if k == KEYC_RIGHT || k == KEYC_CTRL | b'f' as key_code => {
            if pr.index < size {
                pr.index += 1;
            }
            return PromptEditAction::Unchanged;
        }
        k if k == KEYC_HOME || k == KEYC_CTRL | b'a' as key_code => {
            pr.index = 0;
            return PromptEditAction::Unchanged;
        }
        k if k == KEYC_END || k == KEYC_CTRL | b'e' as key_code => {
            pr.index = size;
            return PromptEditAction::Unchanged;
        }
        9 => {
            if prompt_replace_complete(pr, None) == 0 {
                return PromptEditAction::Unchanged;
            }
        }
        k if k == KEYC_BSPACE || k == KEYC_CTRL | b'h' as key_code => {
            if pr.flags & PROMPT_BSPACE_EXIT != 0 && size == 0 {
                return PromptEditAction::Cancel;
            }
            if pr.index == 0 {
                return PromptEditAction::Unchanged;
            }
            if pr.index == size {
                pr.buffer[pr.index - 1].size = 0;
            } else {
                pr.buffer.copy_within(pr.index..size + 1, pr.index - 1);
            }
            pr.index -= 1;
        }
        k if k == KEYC_DC || k == KEYC_CTRL | b'd' as key_code => {
            if pr.index == size {
                return PromptEditAction::Unchanged;
            }
            pr.buffer.copy_within(pr.index + 1..size + 1, pr.index);
        }
        k if k == KEYC_CTRL | b'u' as key_code => {
            pr.buffer[0].size = 0;
            pr.index = 0;
        }
        k if k == KEYC_CTRL | b'k' as key_code => {
            if pr.index == size {
                return PromptEditAction::Unchanged;
            }
            pr.buffer[pr.index].size = 0;
        }
        k if k == KEYC_CTRL | b'w' as key_code => {
            let index = prompt_backward_word(pr, &pr.word_separators);
            prompt_save_copied(pr, index);
            pr.buffer.copy_within(pr.index..size + 1, index);
            pr.buffer[size - (pr.index - index)..size].fill(utf8_data::default());
            pr.index = index;
        }
        k if k == KEYC_RIGHT | KEYC_CTRL || k == KEYC_META | b'f' as key_code => {
            pr.index = prompt_forward_word(pr, size, false, &pr.word_separators);
        }
        k if k == KEYC_VI | b'E' as key_code => {
            pr.index = prompt_end_word(pr, size, c"");
        }
        k if k == KEYC_VI | b'e' as key_code => {
            pr.index = prompt_end_word(pr, size, &pr.word_separators);
        }
        k if k == KEYC_VI | b'W' as key_code => {
            pr.index = prompt_forward_word(pr, size, true, c"");
        }
        k if k == KEYC_VI | b'w' as key_code => {
            pr.index = prompt_forward_word(pr, size, true, &pr.word_separators);
        }
        k if k == KEYC_VI | b'B' as key_code => {
            pr.index = prompt_backward_word(pr, c"");
        }
        k if k == KEYC_LEFT | KEYC_CTRL || k == KEYC_META | b'b' as key_code => {
            pr.index = prompt_backward_word(pr, &pr.word_separators);
        }
        k if k == KEYC_UP || k == KEYC_CTRL | b'p' as key_code => {
            let Some(history) = prompt_up_history(&mut pr.hindex, pr.type_0) else {
                return PromptEditAction::Unchanged;
            };
            pr.buffer = utf8_fromcstr_vec(&history);
            pr.index = utf8_strlen(&pr.buffer);
        }
        k if k == KEYC_DOWN || k == KEYC_CTRL | b'n' as key_code => {
            let history = prompt_down_history(&mut pr.hindex, pr.type_0);
            pr.buffer = utf8_fromcstr_vec(history.as_deref().unwrap_or(c""));
            pr.index = utf8_strlen(&pr.buffer);
        }
        k if k == KEYC_CTRL | b'y' as key_code => {
            if prompt_paste(pr) == 0 {
                return PromptEditAction::Unchanged;
            }
        }
        k if k == KEYC_CTRL | b't' as key_code => {
            let index = pr.index + usize::from(pr.index < size);
            if index < 2 {
                return PromptEditAction::Unchanged;
            }
            let tmp = utf8_copy(&pr.buffer[index - 2]);
            pr.buffer[index - 2] = utf8_copy(&pr.buffer[index - 1]);
            pr.buffer[index - 1] = utf8_copy(&tmp);
            pr.index = index;
        }
        10 | 13 => return PromptEditAction::Submit,
        k if k == 27
            || k == KEYC_CTRL | b'[' as key_code
            || k == KEYC_CTRL | b'c' as key_code
            || k == KEYC_CTRL | b'g' as key_code =>
        {
            return PromptEditAction::Cancel;
        }
        k if k == KEYC_CTRL | b'r' as key_code || k == KEYC_CTRL | b's' as key_code => {
            if pr.flags & PROMPT_INCREMENTAL == 0 {
                return PromptEditAction::Unchanged;
            }
            if pr.buffer[0].size == 0 {
                pr.buffer = utf8_fromcstr_vec(prompt_last(pr));
                pr.index = utf8_strlen(&pr.buffer);
            } else {
                return PromptEditAction::Changed(if k == KEYC_CTRL | b'r' as key_code {
                    b'-'
                } else {
                    b'+'
                });
            }
        }
        k if k == KEYC_CTRL | b'v' as key_code => {
            pr.flags |= PROMPT_QUOTENEXT;
            return PromptEditAction::Unchanged;
        }
        _ => return PromptEditAction::Append(key),
    }
    PromptEditAction::Changed(b'=')
}

fn prompt_unicode_key(key: key_code) -> bool {
    key & KEYC_MASK_TYPE == (KEYC_TYPE_UNICODE as key_code) << 32 && key & KEYC_MASK_KEY > 0x7f
}

fn prompt_append_key(pr: &mut prompt, key: key_code) -> bool {
    let mut cell = utf8_data::default();
    if key <= 0x7f {
        utf8_set(&mut cell, key as u8);
        if key <= 0x1f || key == 0x7f {
            cell.width = 2;
        }
    } else if prompt_unicode_key(key) {
        utf8_to_data(key as utf8_char, &mut cell);
        if cell.size == 0 {
            return false;
        }
    } else {
        return false;
    }
    pr.buffer.insert(pr.index, cell);
    pr.index += 1;
    true
}

pub unsafe fn prompt_key(
    pr: &refbox::Weak<crate::src::shared::prompt::prompt>,
    mut key: key_code,
    redraw: &mut ::core::ffi::c_int,
) -> prompt_key_result {
    let (flags, keys) = {
        let Some(mut state) = prompt_borrow_live(pr) else {
            return PROMPT_KEY_CLOSE;
        };
        if state.flags & PROMPT_FREEING != 0 {
            return PROMPT_KEY_CLOSE;
        }
        state.closed = 0;
        prompt_clear_complete(&mut state);
        (state.flags, state.keys)
    };
    if flags & PROMPT_KEY != 0 {
        let key_string = key_string_format(key, false);
        if prompt_fire_callback(pr, Some(&key_string), PROMPT_KEY_CLOSE, None)
            == PromptCallbackResult::Continue
        {
            pr.try_borrow_mut().expect("live unborrowed prompt").closed = 1;
        }
        return PROMPT_KEY_CLOSE;
    }
    key = prompt_keypad_key(key & !KEYC_MASK_FLAGS);
    let translated = if flags & PROMPT_NUMERIC != 0 {
        if !(b'0' as key_code..=b'9' as key_code).contains(&key) {
            let input =
                utf8_tocstr_cstring(&pr.try_borrow_mut().expect("live unborrowed prompt").buffer);
            if prompt_fire_callback(pr, Some(&input), PROMPT_KEY_CLOSE, None)
                == PromptCallbackResult::Continue
            {
                pr.try_borrow_mut().expect("live unborrowed prompt").closed = 1;
            }
            return PROMPT_KEY_NOT_HANDLED;
        }
        PromptTranslatedKey::Append(key)
    } else if flags & (PROMPT_SINGLE | PROMPT_QUOTENEXT) != 0 {
        if key & KEYC_MASK_KEY == KEYC_BSPACE {
            key = 0x7f;
        } else if key & KEYC_MASK_KEY > 0x7f {
            if !prompt_unicode_key(key) {
                return PROMPT_KEY_HANDLED;
            }
            key &= KEYC_MASK_KEY;
        } else {
            key &= if key & KEYC_CTRL != 0 {
                0x1f
            } else {
                KEYC_MASK_KEY
            };
        }
        pr.try_borrow_mut().expect("live unborrowed prompt").flags &= !PROMPT_QUOTENEXT;
        PromptTranslatedKey::Append(key)
    } else if keys == MODEKEY_VI {
        prompt_translate_key(
            &mut pr.try_borrow_mut().expect("live unborrowed prompt"),
            key,
            redraw,
        )
    } else {
        PromptTranslatedKey::Process(key)
    };
    let action = match translated {
        PromptTranslatedKey::Handled => return PROMPT_KEY_HANDLED,
        PromptTranslatedKey::Append(key) => PromptEditAction::Append(key),
        PromptTranslatedKey::Process(key) => {
            let result = prompt_check_move(pr, key);
            if result != PROMPT_KEY_NOT_HANDLED {
                return result;
            }
            prompt_edit_key(
                &mut pr.try_borrow_mut().expect("live unborrowed prompt"),
                key,
            )
        }
    };
    let mut result = PROMPT_KEY_HANDLED;
    let prefix = match action {
        PromptEditAction::Unchanged => {
            *redraw = 1;
            return PROMPT_KEY_HANDLED;
        }
        PromptEditAction::Submit => return prompt_done_with_history(pr, redraw),
        PromptEditAction::Cancel => return prompt_done(pr, None, redraw).key_result(),
        PromptEditAction::Changed(prefix) => prefix,
        PromptEditAction::Append(key) => {
            if !prompt_append_key(
                &mut pr.try_borrow_mut().expect("live unborrowed prompt"),
                key,
            ) {
                return PROMPT_KEY_HANDLED;
            }
            if flags & PROMPT_SINGLE != 0 {
                if utf8_strlen(&pr.try_borrow_mut().expect("live unborrowed prompt").buffer) != 1 {
                    pr.try_borrow_mut().expect("live unborrowed prompt").closed = 1;
                    result = PROMPT_KEY_CLOSE;
                } else {
                    let input = utf8_tocstr_cstring(
                        &pr.try_borrow_mut().expect("live unborrowed prompt").buffer,
                    );
                    let outcome = prompt_done(pr, Some(&input), redraw);
                    if outcome == PromptCallbackResult::Freed {
                        *redraw = 1;
                        return PROMPT_KEY_CLOSE;
                    }
                    result = outcome.key_result();
                }
            }
            b'='
        }
    };
    prompt_trim_buffer(&mut pr.try_borrow_mut().expect("live unborrowed prompt"));
    *redraw = 1;
    let callback_input = {
        let state = pr.try_borrow_mut().expect("live unborrowed prompt");
        (state.flags & PROMPT_INCREMENTAL != 0).then(|| prompt_incremental_input(&state, prefix))
    };
    if let Some(input) = callback_input {
        prompt_fire_callback(pr, Some(&input), PROMPT_KEY_HANDLED, None);
    }
    result
}
fn prompt_complete_add(list: &mut Vec<CString>, s: &CStr) {
    if !list.iter().any(|name| name.as_bytes() == s.to_bytes()) {
        list.push(s.to_owned());
    }
}
unsafe fn prompt_complete_commands(s: &CStr) -> Vec<CString> {
    let mut list = Vec::new();
    let prefix = s.to_bytes();
    for &entry in &cmd_table {
        let name = entry.name;
        if name.to_bytes().starts_with(prefix) {
            prompt_complete_add(&mut list, name);
        }
    }
    let aliases = with_global_options(|oo| {
        crate::src::options::options_read_entry(oo, c"command-alias", |entry| {
            entry
                .value
                .array()
                .into_iter()
                .flat_map(|array| array.iter())
                .map(|item| item.value.string_ptr().expect("string option").to_owned())
                .collect::<Vec<_>>()
        })
    })
    .unwrap_or_default();
    for value in aliases {
        if let Some(separator) = value.to_bytes().iter().position(|&byte| byte == b'=') {
            if prefix.len() <= separator && &value.to_bytes()[..prefix.len()] == prefix {
                let alias = CString::new(&value.to_bytes()[..separator])
                    .expect("alias prefix contains no NUL");
                prompt_complete_add(&mut list, alias.as_c_str());
            }
        }
    }
    list
}
fn prompt_complete_prefix(list: &[CString]) -> CString {
    let first = list
        .first()
        .expect("completion list is nonempty")
        .as_bytes();
    let mut prefix_len = first.len();
    for name in &list[1..] {
        prefix_len = first[..prefix_len]
            .iter()
            .zip(name.as_bytes())
            .take_while(|(a, b)| a == b)
            .count();
    }
    CString::new(&first[..prefix_len]).expect("completion names contain no NUL")
}
fn prompt_clear_complete(pr: &mut prompt) {
    pr.completion.names = Vec::new();
    pr.completion.display = None;
}
fn prompt_store_complete(pr: &mut prompt, list: Vec<CString>) {
    prompt_clear_complete(pr);
    pr.completion.names = list;
    let mut display = Vec::new();
    for name in &pr.completion.names {
        display.push(b' ');
        display.extend_from_slice(name.as_bytes());
    }
    pr.completion.display = Some(CString::new(display).expect("names contain no NUL"));
}
unsafe fn prompt_complete(pr: &mut prompt, word: &CStr, mut offset: u_int) -> Option<CString> {
    let mut list: Vec<CString>;
    let mut i: u_int = 0;
    if pr.type_0 as ::core::ffi::c_uint
        != PROMPT_TYPE_COMMAND as ::core::ffi::c_int as ::core::ffi::c_uint
        || offset != 0 as u_int
        || word.to_bytes().is_empty()
    {
        return None;
    }
    list = prompt_complete_commands(word);
    if list.is_empty() {
        return None;
    }
    list.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    i = 0 as u_int;
    while (i as usize) < list.len() {
        log_debug(format_args!(
            "complete {}: {}",
            (i) as u32,
            log_cstr(&list[i as usize])
        ));
        i = i.wrapping_add(1);
    }
    let out = if list.len() == 1 {
        let mut bytes = list[0].as_bytes().to_vec();
        bytes.push(b' ');
        CString::new(bytes).expect("completion name contains no NUL")
    } else {
        prompt_complete_prefix(&list)
    };
    if word != out.as_c_str() {
        return Some(out);
    }
    if list.len() <= 1 {
        return None;
    }
    prompt_store_complete(pr, list);
    None
}
pub fn prompt_type(name: &CStr) -> prompt_type {
    match name.to_bytes() {
        b"command" => PROMPT_TYPE_COMMAND,
        b"search" => PROMPT_TYPE_SEARCH,
        _ => PROMPT_TYPE_INVALID,
    }
}
pub fn prompt_type_string(kind: prompt_type) -> &'static CStr {
    match kind {
        PROMPT_TYPE_COMMAND => c"command",
        PROMPT_TYPE_SEARCH => c"search",
        PROMPT_TYPE_INVALID => c"invalid",
        _ => c"unknown",
    }
}

#[cfg(test)]
mod prompt_buffer_tests {
    use super::*;

    fn make_prompt(input: &CStr, index: usize, copied: Option<Box<[utf8_data]>>) -> Box<prompt> {
        Box::new(prompt {
            buffer: utf8_fromcstr_vec(input),
            copied,
            index,
            ..Default::default()
        })
    }

    fn make_prompt_owner(
        input: &CStr,
        index: usize,
    ) -> refbox::RefBox<crate::src::shared::prompt::prompt> {
        refbox::RefBox::new(prompt {
            buffer: utf8_fromcstr_vec(input),
            index,
            ..Default::default()
        })
    }

    fn cell(byte: u8) -> utf8_data {
        let mut result = utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        };
        utf8_set(&mut result, byte);
        result
    }

    #[test]
    fn word_motion_preserves_tmux_separator_classes_and_end_positions() {
        type Motion = (usize, usize, usize, usize);
        let cases: &[(&CStr, &CStr, &[Motion])] = &[
            (c"", c",/", &[(0, 0, 0, 0)]),
            (c"   ", c",/", &[(3, 3, 3, 0); 4]),
            (
                c"ab,cd",
                c",/",
                &[
                    (2, 2, 1, 0),
                    (2, 2, 2, 0),
                    (3, 3, 4, 0),
                    (5, 5, 4, 2),
                    (5, 5, 5, 3),
                    (5, 5, 5, 3),
                ],
            ),
            (
                c"ab,cd",
                c"",
                &[
                    (5, 5, 4, 0),
                    (5, 5, 4, 0),
                    (5, 5, 4, 0),
                    (5, 5, 4, 0),
                    (5, 5, 5, 0),
                    (5, 5, 5, 0),
                ],
            ),
            (
                c"  ab,,é漢 /z  ",
                c",/",
                &[
                    (4, 2, 3, 0),
                    (4, 4, 3, 0),
                    (4, 4, 3, 0),
                    (4, 4, 5, 2),
                    (6, 6, 5, 2),
                    (6, 6, 7, 4),
                    (8, 9, 7, 4),
                    (8, 9, 9, 6),
                    (10, 9, 9, 6),
                    (10, 10, 10, 6),
                    (11, 13, 13, 9),
                    (13, 13, 13, 10),
                    (13, 13, 13, 10),
                    (13, 13, 13, 10),
                ],
            ),
            (
                c"  ab,,é漢 /z  ",
                c"",
                &[
                    (8, 2, 7, 0),
                    (8, 9, 7, 0),
                    (8, 9, 7, 0),
                    (8, 9, 7, 2),
                    (8, 9, 7, 2),
                    (8, 9, 7, 2),
                    (8, 9, 7, 2),
                    (8, 9, 10, 2),
                    (11, 13, 10, 2),
                    (11, 13, 10, 2),
                    (11, 13, 13, 9),
                    (13, 13, 13, 9),
                    (13, 13, 13, 9),
                    (13, 13, 13, 9),
                ],
            ),
            (
                c"a\tb c",
                c",/",
                &[
                    (3, 4, 2, 0),
                    (3, 4, 2, 0),
                    (3, 4, 4, 0),
                    (5, 5, 4, 0),
                    (5, 5, 5, 0),
                    (5, 5, 5, 4),
                ],
            ),
        ];
        for &(text, separators, expected) in cases {
            let mut pr = make_prompt(text, 0, None);
            let size = utf8_strlen(&pr.buffer);
            assert_eq!(expected.len(), size + 1);
            for (index, &positions) in expected.iter().enumerate() {
                pr.index = index;
                assert_eq!(
                    (
                        prompt_forward_word(&pr, size, false, separators),
                        prompt_forward_word(&pr, size, true, separators),
                        prompt_end_word(&pr, size, separators),
                        prompt_backward_word(&pr, separators)
                    ),
                    positions,
                    "{text:?}, separators {separators:?}, cursor {index}",
                );
            }
        }
        assert_eq!(prompt_in_list(c",/", &cell(0)), 1);
        let mut wide_space = cell(b' ');
        wide_space.width = 2;
        assert_eq!(prompt_space(&wide_space), 0);
        assert_eq!(prompt_in_list(c" ", &wide_space), 0);
    }

    #[test]
    fn completion_keeps_prefix_suffix_bytes_and_only_the_logical_sentinel() {
        let input = CString::new("cmd old tail").unwrap();
        let replacement = CString::new(vec![0xff]).unwrap();
        let mut pr = make_prompt(&input, 7, None);
        pr.buffer.push(cell(b'!'));

        unsafe {
            assert_eq!(prompt_replace_complete(&mut pr, Some(&replacement)), 1);
            assert_eq!(utf8_tocstr_cstring(&pr.buffer).as_bytes(), b"cmd \xff tail");
            assert_eq!(pr.index, 5);
            assert_eq!(pr.buffer.len(), utf8_strlen(&pr.buffer) + 1);

            for (input, index, replaced, expected, next) in [
                (c"", 0, 1, c"new ", 4),
                (c"cmd old tail", 0, 1, c"new  old tail", 4),
                (c"cmd old tail", 4, 0, c"cmd old tail", 4),
                (c"cmd old tail", 5, 1, c"cmd new  tail", 8),
                (c"cmd old tail", 12, 1, c"cmd old new ", 12),
                (c"   ", 0, 0, c"   ", 0),
                (c"   ", 3, 0, c"   ", 3),
                (c"é漢 old", 1, 1, c"new  old", 4),
            ] {
                let mut pr = make_prompt(input, index, None);
                assert_eq!(prompt_replace_complete(&mut pr, Some(c"new ")), replaced);
                assert_eq!(utf8_tocstr_cstring(&pr.buffer).as_c_str(), expected);
                assert_eq!(pr.index, next);
            }
            let long = CString::new(vec![b'x'; 64]).unwrap();
            let mut pr = make_prompt(&long, 64, None);
            assert_eq!(prompt_replace_complete(&mut pr, Some(c"new ")), 1);
            assert_eq!(utf8_tocstr_cstring(&pr.buffer).as_c_str(), c"new ");
        }
    }

    #[test]
    fn vi_translation_preserves_mode_switches_and_multibyte_cursor_edits() {
        let pr = make_prompt_owner(c"é漢Z", 3);
        pr.try_borrow_mut().unwrap().keys = MODEKEY_VI;
        for (key, expected, index, command) in [
            (27, c"é漢Z", 2, true),
            (KEYC_BSPACE, c"é漢Z", 1, true),
            (b'a' as key_code, c"é漢Z", 2, false),
            (KEYC_KP_PLUS, c"é漢+Z", 3, false),
            (27, c"é漢+Z", 2, true),
            (b's' as key_code, c"é漢Z", 2, false),
            (b'!' as key_code, c"é漢!Z", 3, false),
            (27, c"é漢!Z", 2, true),
            (b'S' as key_code, c"", 0, false),
        ] {
            let mut redraw = 0;
            unsafe {
                assert_eq!(
                    prompt_key(&pr.downgrade(), key, &mut redraw),
                    PROMPT_KEY_HANDLED
                );
                assert_eq!(
                    utf8_tocstr_cstring(&pr.try_borrow_mut().unwrap().buffer).as_c_str(),
                    expected
                );
            }
            assert_eq!(redraw, 1);
            assert_eq!(pr.try_borrow_mut().unwrap().index, index);
            assert_eq!(
                pr.try_borrow_mut().unwrap().flags & PROMPT_COMMANDMODE != 0,
                command
            );
            {
                let state = pr.try_borrow_mut().unwrap();
                assert_eq!(state.buffer.len(), utf8_strlen(&state.buffer) + 1);
            }
        }

        let mut redraw = 7;
        let modified_keypad = KEYC_KP_PLUS | KEYC_CTRL;
        assert_eq!(prompt_keypad_key(modified_keypad), modified_keypad);
        assert_eq!(
            prompt_translate_key(
                &mut pr.try_borrow_mut().unwrap(),
                modified_keypad,
                &mut redraw
            ),
            PromptTranslatedKey::Append(modified_keypad)
        );
        assert_eq!(redraw, 7);
        assert_eq!(
            prompt_translate_key(&mut pr.try_borrow_mut().unwrap(), 27, &mut redraw),
            PromptTranslatedKey::Handled
        );
        assert_eq!((pr.try_borrow_mut().unwrap().index, redraw), (0, 1));
        redraw = 7;
        assert_eq!(
            prompt_translate_key(&mut pr.try_borrow_mut().unwrap(), 27, &mut redraw),
            PromptTranslatedKey::Handled
        );
        assert_eq!((pr.try_borrow_mut().unwrap().index, redraw), (0, 7));
    }

    #[test]
    fn transpose_keeps_multibyte_cells_and_the_logical_sentinel() {
        for (index, expected, next) in [(0, c"é漢Z", 0), (1, c"漢éZ", 2), (3, c"éZ漢", 3)] {
            let pr = make_prompt_owner(c"é漢Z", index);
            let mut redraw = 0;
            unsafe {
                assert_eq!(
                    prompt_key(&pr.downgrade(), KEYC_CTRL | b't' as key_code, &mut redraw),
                    PROMPT_KEY_HANDLED
                );
                assert_eq!(
                    utf8_tocstr_cstring(&pr.try_borrow_mut().unwrap().buffer).as_c_str(),
                    expected
                );
            }
            assert_eq!(pr.try_borrow_mut().unwrap().index, next);
            assert_eq!(pr.try_borrow_mut().unwrap().buffer.len(), 4);
            assert_eq!(pr.try_borrow_mut().unwrap().buffer[3].size, 0);
        }
    }

    #[test]
    fn deletions_preserve_overlapping_multibyte_cells_and_copied_words() {
        for (key, index, expected, next, copied) in [
            (KEYC_BSPACE, 0, c"ab,é漢 Z", 0, None),
            (KEYC_BSPACE, 4, c"ab,漢 Z", 3, None),
            (KEYC_BSPACE, 7, c"ab,é漢 ", 6, None),
            (KEYC_DC, 3, c"ab,漢 Z", 3, None),
            (KEYC_DC, 7, c"ab,é漢 Z", 7, None),
            (KEYC_CTRL | b'k' as key_code, 3, c"ab,", 3, None),
            (KEYC_CTRL | b'u' as key_code, 3, c"", 0, None),
            (KEYC_CTRL | b'w' as key_code, 0, c"ab,é漢 Z", 0, Some(c"")),
            (KEYC_CTRL | b'w' as key_code, 3, c"abé漢 Z", 2, Some(c",")),
            (KEYC_CTRL | b'w' as key_code, 6, c"ab,Z", 3, Some(c"é漢 ")),
        ] {
            let pr = make_prompt_owner(c"ab,é漢 Z", index);
            pr.try_borrow_mut().unwrap().word_separators = c",".to_owned();
            let mut redraw = 0;
            unsafe {
                assert_eq!(
                    prompt_key(&pr.downgrade(), key, &mut redraw),
                    PROMPT_KEY_HANDLED
                );
            }
            assert_eq!(
                utf8_tocstr_cstring(&pr.try_borrow_mut().unwrap().buffer).as_c_str(),
                expected
            );
            assert_eq!(pr.try_borrow_mut().unwrap().index, next);
            {
                let state = pr.try_borrow_mut().unwrap();
                assert_eq!(state.buffer.len(), utf8_strlen(&state.buffer) + 1);
            }
            assert_eq!(pr.try_borrow_mut().unwrap().buffer.last().unwrap().size, 0);
            assert_eq!(
                pr.try_borrow_mut()
                    .unwrap()
                    .copied
                    .as_deref()
                    .map(utf8_tocstr_cstring)
                    .as_deref(),
                copied
            );
            if let Some(copied) = copied {
                assert_eq!(
                    pr.try_borrow_mut().unwrap().copied.as_ref().unwrap().len(),
                    copied.to_str().unwrap().chars().count() + 1
                );
                unsafe {
                    prompt_key(&pr.downgrade(), KEYC_CTRL | b'y' as key_code, &mut redraw);
                }
                assert_eq!(
                    utf8_tocstr_cstring(&pr.try_borrow_mut().unwrap().buffer).as_c_str(),
                    c"ab,é漢 Z"
                );
                assert_eq!(pr.try_borrow_mut().unwrap().buffer.len(), 8);
            }
        }
    }

    #[test]
    fn incremental_edits_distinguish_cursor_moves_searches_and_quoted_cells() {
        use std::{cell::RefCell, rc::Rc};
        let events = Rc::new(RefCell::new(Vec::new()));
        let inputs = events.clone();
        let pr = make_prompt_owner(c"ab", 2);
        pr.try_borrow_mut().unwrap().flags = PROMPT_INCREMENTAL | PROMPT_EDITARROWS;
        pr.try_borrow_mut().unwrap().last = Some(c"é漢".to_owned());
        pr.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |input, kind| {
            inputs.borrow_mut().push((input.unwrap().to_owned(), kind));
            PROMPT_CONTINUE
        }));
        for (key, expected, index, event, redraw_expected) in [
            (KEYC_LEFT, c"ab", 1, None, 1),
            (KEYC_CTRL | b'd' as key_code, c"a", 1, Some(c"=a"), 1),
            (KEYC_CTRL | b'd' as key_code, c"a", 1, None, 1),
            (KEYC_CTRL | b'u' as key_code, c"", 0, Some(c"="), 1),
            (KEYC_CTRL | b'u' as key_code, c"", 0, Some(c"="), 1),
            (KEYC_CTRL | b'r' as key_code, c"é漢", 2, Some(c"=é漢"), 1),
            (KEYC_CTRL | b'r' as key_code, c"é漢", 2, Some(c"-é漢"), 1),
            (KEYC_CTRL | b's' as key_code, c"é漢", 2, Some(c"+é漢"), 1),
            (KEYC_CTRL | b'b' as key_code, c"é漢", 1, None, 1),
            (b'!' as key_code, c"é!漢", 2, Some(c"=é!漢"), 1),
            (KEYC_CTRL | b'v' as key_code, c"é!漢", 2, None, 1),
            (KEYC_UP, c"é!漢", 2, None, 0),
            (
                KEYC_CTRL | b'h' as key_code,
                c"é!\x08漢",
                3,
                Some(c"=é!\x08漢"),
                1,
            ),
        ] {
            let mut redraw = 0;
            unsafe {
                assert_eq!(
                    prompt_key(&pr.downgrade(), key, &mut redraw),
                    PROMPT_KEY_HANDLED
                );
            }
            assert_eq!(
                utf8_tocstr_cstring(&pr.try_borrow_mut().unwrap().buffer).as_c_str(),
                expected
            );
            assert_eq!(pr.try_borrow_mut().unwrap().index, index);
            assert_eq!(redraw, redraw_expected);
            {
                let state = pr.try_borrow_mut().unwrap();
                assert_eq!(state.buffer.len(), utf8_strlen(&state.buffer) + 1);
            }
            assert_eq!(
                &*events.borrow(),
                &event
                    .map(|s| (s.to_owned(), PROMPT_KEY_HANDLED))
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            events.borrow_mut().clear();
        }
        assert_eq!(pr.try_borrow_mut().unwrap().flags & PROMPT_QUOTENEXT, 0);
        assert_eq!(pr.try_borrow_mut().unwrap().buffer[2].width, 2);
    }

    #[test]
    fn callbacks_can_free_prompts_after_single_keys_moves_and_cancellation() {
        use std::{cell::RefCell, rc::Rc};
        for callback_result in [PROMPT_CONTINUE, PROMPT_CLOSE] {
            for (flags, key, expected, kind, text) in [
                (
                    PROMPT_SINGLE,
                    b'a' as key_code,
                    PROMPT_KEY_CLOSE,
                    PROMPT_KEY_CLOSE,
                    Some(c"a"),
                ),
                (
                    PROMPT_SINGLE | PROMPT_INCREMENTAL,
                    b'a' as key_code,
                    PROMPT_KEY_CLOSE,
                    PROMPT_KEY_CLOSE,
                    Some(c"a"),
                ),
                (
                    PROMPT_NUMERIC,
                    13,
                    PROMPT_KEY_NOT_HANDLED,
                    PROMPT_KEY_CLOSE,
                    Some(c""),
                ),
                (
                    PROMPT_KEY,
                    b'a' as key_code,
                    PROMPT_KEY_CLOSE,
                    PROMPT_KEY_CLOSE,
                    Some(c"a"),
                ),
                (
                    PROMPT_INCREMENTAL,
                    b'a' as key_code,
                    PROMPT_KEY_HANDLED,
                    PROMPT_KEY_HANDLED,
                    Some(c"=a"),
                ),
                (
                    PROMPT_INCREMENTAL,
                    KEYC_UP,
                    PROMPT_KEY_CLOSE,
                    PROMPT_KEY_MOVE,
                    Some(c""),
                ),
                (0, 27, PROMPT_KEY_CLOSE, PROMPT_KEY_CLOSE, None),
                (
                    PROMPT_BSPACE_EXIT,
                    KEYC_BSPACE,
                    PROMPT_KEY_CLOSE,
                    PROMPT_KEY_CLOSE,
                    None,
                ),
            ] {
                let events = Rc::new(RefCell::new(Vec::new()));
                let callback_events = events.clone();
                let free_events = events.clone();
                let pr = make_prompt_owner(c"", 0);
                let weak = pr.downgrade();
                unsafe {
                    pr.try_borrow_mut().unwrap().flags = flags;
                    let free_prompt = weak.clone();
                    pr.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
                        let pr = free_prompt.clone();
                        free_events.borrow_mut().push("freed");
                        prompt_free(&pr);
                    }));
                    let input_prompt = weak.clone();
                    pr.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |input, result| {
                        let pr = input_prompt.clone();
                        assert_eq!(input, text);
                        assert_eq!(result, kind);
                        callback_events.borrow_mut().push("start");
                        prompt_free(&pr);
                        assert_eq!(&*callback_events.borrow(), &["start"]);
                        assert_ne!(pr.try_borrow_mut().unwrap().flags & PROMPT_FREE_PENDING, 0);
                        callback_events.borrow_mut().push("end");
                        callback_result
                    }));
                    assert_eq!(prompt_key(&pr.downgrade(), key, &mut 0), expected);
                }
                assert_ne!(pr.try_borrow_mut().unwrap().flags & PROMPT_FREEING, 0);
                assert!(pr.try_borrow_mut().unwrap().inputcb.is_none());
                drop(pr);
                assert!(!weak.is_alive());
                assert_eq!(&*events.borrow(), &["start", "end", "freed"]);
                assert_eq!(Rc::strong_count(&events), 1);
            }
        }
    }

    #[test]
    fn retained_single_prompts_use_callback_updates_for_incremental_input() {
        use std::{cell::RefCell, rc::Rc};
        for callback_result in [PROMPT_CONTINUE, PROMPT_CLOSE] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let callback_events = events.clone();
            let pr = make_prompt_owner(c"", 0);
            let weak = pr.downgrade();
            unsafe {
                pr.try_borrow_mut().unwrap().flags = PROMPT_SINGLE | PROMPT_INCREMENTAL;
                let input_prompt = weak.clone();
                pr.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |input, kind| {
                    let pr = input_prompt.clone();
                    callback_events
                        .borrow_mut()
                        .push((input.unwrap().to_owned(), kind));
                    if kind == PROMPT_KEY_CLOSE {
                        pr.try_borrow_mut().unwrap().buffer = utf8_fromcstr_vec(c"é漢");
                        pr.try_borrow_mut().unwrap().index = 2;
                    }
                    callback_result
                }));
                let mut redraw = 0;
                let expected = if callback_result == PROMPT_CLOSE {
                    PROMPT_KEY_CLOSE
                } else {
                    PROMPT_KEY_HANDLED
                };
                assert_eq!(
                    prompt_key(&pr.downgrade(), b'a' as key_code, &mut redraw),
                    expected
                );
                assert_eq!(redraw, 1);
                assert_eq!(
                    utf8_tocstr_cstring(&pr.try_borrow_mut().unwrap().buffer).as_c_str(),
                    c"é漢"
                );
                assert_eq!(
                    &*events.borrow(),
                    &[
                        (c"a".to_owned(), PROMPT_KEY_CLOSE),
                        (c"=é漢".to_owned(), PROMPT_KEY_HANDLED)
                    ]
                );
                prompt_free(&pr.downgrade());
            }
            drop(pr);
            assert!(!weak.is_alive());
            assert_eq!(Rc::strong_count(&events), 1);
        }
    }

    #[test]
    fn dispatch_defers_cleanup_when_input_drops_the_only_owner() {
        for (flags, key, expected) in [
            (
                PROMPT_SINGLE | PROMPT_INCREMENTAL,
                b'x' as key_code,
                PROMPT_KEY_CLOSE,
            ),
            (PROMPT_KEY, b'x' as key_code, PROMPT_KEY_CLOSE),
            (PROMPT_NUMERIC, 13, PROMPT_KEY_NOT_HANDLED),
            (PROMPT_INCREMENTAL, b'x' as key_code, PROMPT_KEY_HANDLED),
            (PROMPT_INCREMENTAL, KEYC_UP, PROMPT_KEY_CLOSE),
            (0, 27, PROMPT_KEY_CLOSE),
        ] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let slot = Rc::new(RefCell::new(
                None::<refbox::RefBox<crate::src::shared::prompt::prompt>>,
            ));
            let prompt = make_prompt_owner(c"", 0);
            let observer = prompt.downgrade();
            prompt.try_borrow_mut().unwrap().flags = flags;
            let callback_slot = slot.clone();
            let callback_observer = observer.clone();
            let input_events = events.clone();
            prompt.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |text, _| {
                let text_copy = text.map(CStr::to_owned);
                input_events.borrow_mut().push("input");
                let old = callback_slot.borrow_mut().take().unwrap();
                prompt_free(&old.downgrade());
                drop(old);
                assert!(!callback_observer.is_alive());
                assert_eq!(text, text_copy.as_deref());
                assert_eq!(&*input_events.borrow(), &["input"]);
                input_events.borrow_mut().push("returned");
                PROMPT_CONTINUE
            }));
            let free_events = events.clone();
            prompt.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
                free_events.borrow_mut().push("cleanup");
            }));
            *slot.borrow_mut() = Some(prompt);
            assert_eq!(unsafe { prompt_key(&observer, key, &mut 0) }, expected);
            assert!(!observer.is_alive());
            assert_eq!(&*events.borrow(), &["input", "returned", "cleanup"]);
            assert_eq!(
                unsafe { prompt_key(&observer, key, &mut 0) },
                PROMPT_KEY_CLOSE
            );
            assert_eq!(Rc::strong_count(&events), 1);
        }
    }

    #[test]
    fn cleanup_callback_can_drop_the_only_owner() {
        let slot = Rc::new(RefCell::new(
            None::<refbox::RefBox<crate::src::shared::prompt::prompt>>,
        ));
        let prompt = make_prompt_owner(c"", 0);
        let observer = prompt.downgrade();
        let callback_slot = slot.clone();
        prompt.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
            drop(callback_slot.borrow_mut().take());
        }));
        *slot.borrow_mut() = Some(prompt);
        prompt_free(&observer);
        assert!(!observer.is_alive());
        prompt_free(&observer);
    }

    #[test]
    fn callbacks_replace_owners_without_retaining_closed_prompts() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let owner = Rc::new(RefCell::new(
            None::<refbox::RefBox<crate::src::shared::prompt::prompt>>,
        ));
        let pr = make_prompt_owner(c"", 0);
        pr.try_borrow_mut().unwrap().flags = PROMPT_SINGLE;
        let weak = pr.downgrade();
        let weak_owner = Rc::downgrade(&owner);
        let callback_events = events.clone();
        let observed = weak.clone();
        pr.try_borrow_mut().unwrap().inputcb = Some(Box::new(move |input, _| {
            assert_eq!(input, Some(c"x"));
            let owner = weak_owner.upgrade().unwrap();
            let old = owner.borrow_mut().take().unwrap();
            prompt_free(&old.downgrade());
            drop(old);
            assert!(!observed.is_alive());
            callback_events.borrow_mut().push("replacing");
            *owner.borrow_mut() = Some(make_prompt_owner(c"new", 3));
            PROMPT_CLOSE
        }));
        let weak_owner = Rc::downgrade(&owner);
        let free_events = events.clone();
        pr.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
            let owner = weak_owner.upgrade().unwrap();
            let current = owner.borrow().as_ref().unwrap().downgrade();
            assert_eq!(
                utf8_tocstr_cstring(&current.try_borrow_mut().unwrap().buffer).as_c_str(),
                c"new"
            );
            free_events.borrow_mut().push("freed");
        }));
        *owner.borrow_mut() = Some(pr);
        unsafe {
            assert_eq!(
                prompt_key(&weak, b'x' as key_code, &mut 0),
                PROMPT_KEY_CLOSE
            );
            assert_eq!(
                prompt_key(&weak, b'y' as key_code, &mut 0),
                PROMPT_KEY_CLOSE
            );
        }
        assert_eq!(&*events.borrow(), &["replacing", "freed"]);
        let replacement = owner.borrow_mut().take().unwrap();
        assert!(!weak.is(&replacement));
        assert_eq!(prompt_closed(&replacement.try_borrow_mut().unwrap()), 0);
        prompt_free(&weak);
        assert!(!weak.is_alive());
        assert_eq!(Rc::strong_count(&events), 1);
        prompt_free(&replacement.downgrade());
    }

    #[test]
    fn copied_paste_inserts_cells_before_the_sentinel() {
        let input = CString::new(&b"a\xc3\xa9Z"[..]).unwrap();
        let copied = CString::new(&b"\xce\xbb"[..]).unwrap();
        let copied_cells = utf8_fromcstr_vec(copied.as_c_str()).into_boxed_slice();
        let mut pr = make_prompt(&input, 1, Some(copied_cells));
        pr.buffer.push(cell(b'!'));

        unsafe {
            assert_eq!(prompt_paste(&mut pr), 1);
            prompt_trim_buffer(&mut pr);
            assert_eq!(
                utf8_tocstr_cstring(&pr.buffer).as_bytes(),
                b"a\xce\xbb\xc3\xa9Z"
            );
            assert_eq!(pr.index, 2);
            assert_eq!(pr.buffer.len(), utf8_strlen(&pr.buffer) + 1);

            pr.copied = Some(vec![utf8_data::default()].into_boxed_slice());
            assert_eq!(prompt_paste(&mut pr), 1);
            assert_eq!(pr.index, 2);
            assert_eq!(
                utf8_tocstr_cstring(&pr.buffer).as_bytes(),
                b"a\xce\xbb\xc3\xa9Z"
            );
        }
    }

    #[test]
    fn paste_decoding_preserves_utf8_and_stops_at_control_or_invalid_bytes() {
        unsafe {
            assert!(!libc::setlocale(libc::LC_CTYPE, c"C.UTF-8".as_ptr()).is_null());
            for (input, expected, count) in [
                (&b""[..], &b""[..], 0),
                (&b"ab"[..], &b"ab"[..], 2),
                ("Aé漢Z".as_bytes(), "Aé漢Z".as_bytes(), 4),
                ("éZ".as_bytes(), "éZ".as_bytes(), 3),
                (&b"abc\nignored"[..], &b"abc"[..], 3),
                (&b"abc\tignored"[..], &b"abc"[..], 3),
                (&b"ab\0ignored"[..], &b"ab"[..], 2),
                (&b"x\x7fz"[..], &b"x"[..], 1),
                (&b"x\x1bz"[..], &b"x"[..], 1),
                (&b"x\xffz"[..], &b"x"[..], 1),
                (&b"x\xc0\xafz"[..], &b"x"[..], 1),
                (&b"x\xe2(\xa1Z"[..], &b"x"[..], 1),
                (&b"x\xe2\x82"[..], &b"x"[..], 1),
                (&b"x\xf0\x9f"[..], &b"x"[..], 1),
                (&b"x\xed\xa0\x80Z"[..], &b"x"[..], 1),
                (&b"x\xf4\x90\x80\x80z"[..], &b"x"[..], 1),
            ] {
                let cells = prompt_decode_paste(input);
                assert_eq!(
                    utf8_tocstr_cstring(&cells).as_bytes(),
                    expected,
                    "{input:?}"
                );
                assert_eq!(utf8_strlen(&cells), count, "{input:?}");
                assert_eq!(cells.len(), count + 1);
                assert_eq!(cells.last().unwrap().size, 0);
            }
        }
    }
}
