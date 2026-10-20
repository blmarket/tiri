use crate::cmd::queue::{cmdq_append, cmdq_get_callback_owned};
use crate::format::bytes::format_message_with;
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create, format_create_defaults, format_defaults, format_expand_time_cstring,
};
use crate::format_draw::format_draw;
use crate::grid::{grid_cells_equal, grid_compare};
use crate::log::{fatalx, log_cstr, log_debug, log_pointer};
use crate::options::{options_get_number, options_get_string, OptionsScope};
use crate::prompt::{
    prompt_closed, prompt_create, prompt_incremental_start, prompt_key, prompt_mouse,
    prompt_set_options, prompt_update,
};
use crate::reactor::Timer;
use crate::screen::{screen_init, screen_resize};
use crate::screen_write::{
    screen_write_cursormove, screen_write_fast_copy, screen_write_putc, screen_write_start,
    screen_write_stop,
};
use crate::server::server_add_message;
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::session::Session as _;
use crate::shared::client::ClientRef;
use crate::shared::prompt::{prompt_create_data, prompt_draw_data};
use crate::shared::session::SessionRef;
use crate::style::{style_ranges_clear, style_ranges_init};
use std::ffi::{CStr, CString};
use std::rc::Rc;
use std::time::Duration;

use crate::shared::abi::*;
use crate::shared::client::{
    CLIENT_CONTROL, CLIENT_REDRAWSTATUS, CLIENT_STATUSFORCE, CLIENT_STATUSOFF,
};
use crate::shared::command::cmd_find_state;
use crate::shared::command::*;
use crate::shared::format::{FORMAT_FORCE, FORMAT_NONE, FORMAT_STATUS};
use crate::shared::grid::*;
use crate::shared::key::*;
use crate::shared::mouse::{mouse_event, MOUSE_BUTTON_1, MOUSE_MASK_BUTTONS, MOUSE_MASK_DRAG};
use crate::shared::prompt::*;
use crate::shared::prompt::{prompt_free_cb, PROMPT_ACCEPT, PROMPT_SINGLE};
use crate::shared::screen::screen;
use crate::shared::status::{status_line, status_prompt_input_cb};
use crate::shared::style::*;

unsafe fn status_timer_callback(client: &ClientRef) {
    let session = client.attached_session().upgrade();
    {
        let mut status = client.borrow_status_mut();
        drop(status.timer.take());
    }
    let Some(session) = session else { return };
    client.redraw_status_if_unobscured();
    let timeout = Duration::from_secs(
        (options_get_number(&session.borrow_options_mut(), c"status-interval")) as u64,
    );
    if timeout.as_secs() != 0 {
        let mut status = client.borrow_status_mut();
        let observer = std::rc::Rc::downgrade(client);
        status.timer = Some(
            Timer::new(timeout, move || unsafe {
                if let Some(owner) = observer.upgrade() {
                    status_timer_callback(&owner);
                }
            })
            .expect("arm timer"),
        );
    }
    log_debug(format_args!(
        "client {}, status interval {}",
        log_pointer(std::rc::Rc::as_ptr(client)),
        timeout.as_secs() as i32
    ));
}
pub unsafe fn status_timer_start(client: &ClientRef) {
    let session = client.attached_session().upgrade();
    {
        let mut status = client.borrow_status_mut();
        drop(status.timer.take());
    }
    if session
        .is_some_and(|session| options_get_number(&session.borrow_options_mut(), c"status") != 0)
    {
        status_timer_callback(client);
    }
}
pub unsafe fn status_timer_start_all() {
    let mut c: Option<ClientRef> = None;
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        status_timer_start(&c.clone().expect("live client"));
        registry_c_owner =
            ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
}
pub unsafe fn status_at_line(c: &ClientRef) -> ::core::ffi::c_int {
    use crate::session::Session;
    if c.flags() & (CLIENT_STATUSOFF | CLIENT_CONTROL) as uint64_t != 0 {
        return -1;
    }
    let session = c
        .attached_session()
        .upgrade()
        .expect("status client session");
    let (position, _) = session.status_layout();
    if position != 1 {
        return position;
    }
    c.terminal_size().1.wrapping_sub(status_line_size(c)) as ::core::ffi::c_int
}
pub unsafe fn status_line_size(c: &ClientRef) -> u_int {
    use crate::session::Session;
    if c.flags() & (CLIENT_STATUSOFF | CLIENT_CONTROL) as uint64_t != 0 {
        return 0;
    }
    match c.attached_session().upgrade() {
        Some(session) => session.status_layout().1,
        None => {
            OptionsScope::GlobalSession.with_local(|oo| options_get_number(oo, c"status")) as u_int
        }
    }
}
pub unsafe fn status_prompt_line_at(c: &ClientRef) -> u_int {
    let mut s: Option<SessionRef> = c.attached_session().upgrade();
    let mut line: u_int = 0;
    let mut lines: u_int = 0;
    lines = status_line_size(c);
    if lines == 0 as u_int {
        return 0 as u_int;
    }
    line = options_get_number(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"message-line",
    ) as u_int;
    if line >= lines {
        return lines.wrapping_sub(1 as u_int);
    }
    line
}
pub unsafe fn status_get_range(c: &ClientRef, x: u_int, y: u_int) -> Option<style_range> {
    c.borrow_status()
        .entries
        .get(y as usize)?
        .ranges
        .as_slice()
        .iter()
        .find(|range| x >= range.start && x < range.end)
        .map(|range| **range)
}
pub(crate) unsafe fn status_push_screen(client: &ClientRef) {
    let needs_screen = { client.borrow_status().active.is_none() };
    if needs_screen {
        let lines = status_line_size(client);
        let (width, _) = client.terminal_size();
        let mut status = client.borrow_status_mut();
        status.active = Some(Box::new(screen::empty()));
        screen_init(status.active_screen(), width, lines, 0);
    }
    client.borrow_status_mut().screen_users += 1;
}

pub(crate) unsafe fn status_pop_screen(client: &ClientRef) {
    let active = {
        let mut status = client.borrow_status_mut();
        status.screen_users -= 1;
        (status.screen_users == 0).then(|| {
            status
                .active
                .take()
                .expect("status screen push/pop balanced")
        })
    };
    if let Some(mut active) = active {
        drop(active);
    }
}
pub unsafe fn status_init(sl: &mut status_line, width: u_int) {
    for entry in &mut sl.entries {
        style_ranges_init(&mut entry.ranges);
    }
    screen_init(&mut sl.screen, width, 1, 0);
    sl.active = None;
}
pub fn status_free(sl: &mut status_line) {
    for entry in &mut sl.entries {
        drop(std::mem::take(&mut entry.ranges));
        entry.expanded = None;
    }
    drop(sl.timer.take());
    drop(sl.active.take());
    drop(std::mem::take(&mut sl.screen));
}
pub unsafe fn status_redraw(c_owner: &ClientRef) -> ::core::ffi::c_int {
    let mut c: Option<ClientRef> = Some(c_owner.clone());

    let mut s: Option<SessionRef> = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
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
    let mut lines: u_int = 0;
    let mut i: u_int = 0;
    let mut n: u_int = 0;
    let (width, terminal_height) = c_owner.terminal_size();
    let mut flags: ::core::ffi::c_int = 0;
    let mut force: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut changed: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut fg: ::core::ffi::c_int = 0;
    let mut bg: ::core::ffi::c_int = 0;
    log_debug(format_args!("{} enter", "status_redraw"));
    if c_owner.borrow_status().active.is_some() {
        fatalx(|out| out.write_all(b"not the active screen"));
    }
    lines = status_line_size(c.as_ref().expect("live client"));
    if terminal_height == 0 as u_int || lines == 0 as u_int {
        return 1 as ::core::ffi::c_int;
    }
    flags = FORMAT_STATUS;
    if c.as_ref().expect("live client").flags() & CLIENT_STATUSFORCE as uint64_t != 0 {
        flags |= FORMAT_FORCE;
    }
    let mut ft_owner = format_create(Some(c_owner), None, FORMAT_NONE, flags);
    let ft = &mut *ft_owner;
    format_defaults(ft, Some(c_owner), None, (refbox::Weak::new()).clone(), None);
    let scope = OptionsScope::Session(Rc::downgrade(s.as_ref().expect("live session")));
    crate::style::style_apply_with_options(&mut gc, c"status-style", Some(&mut *ft), &scope);
    fg = scope.with_local(|oo| options_get_number(oo, c"status-fg")) as ::core::ffi::c_int;
    if !(fg == 8 as ::core::ffi::c_int || fg == 9 as ::core::ffi::c_int) {
        gc.fg = fg;
    }
    bg = scope.with_local(|oo| options_get_number(oo, c"status-bg")) as ::core::ffi::c_int;
    if !(bg == 8 as ::core::ffi::c_int || bg == 9 as ::core::ffi::c_int) {
        gc.bg = bg;
    }
    {
        let mut sl = c_owner.borrow_status_mut();
        if !grid_cells_equal(&gc, &sl.style) {
            force = 1 as ::core::ffi::c_int;
            sl.style = gc;
        }
        if sl.screen.grid().sx != width || sl.screen.grid().sy != lines {
            screen_resize(&mut sl.screen, width, lines, 0 as ::core::ffi::c_int);
            force = 1 as ::core::ffi::c_int;
            changed = force;
        }
    }
    // Draw into the status screen with it taken out of the client, so the
    // single write context can live across the format expansions below while
    // they reach the client.
    let mut screen = std::mem::take(&mut c_owner.borrow_status_mut().screen);
    let mut ctx = screen_write_start(&mut screen);
    let format_owner = scope.resolve(c"status-format", false);
    if let Some(format_owner) = &format_owner {
        i = 0 as u_int;
        while i < lines {
            screen_write_cursormove(
                &mut ctx,
                0 as ::core::ffi::c_int,
                i as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
            );
            let line_format = format_owner
                .with_entry(c"status-format", |entry| {
                    entry
                        .value
                        .array_mut()
                        .and_then(|array| array.get_index_mut(i))
                        .map(|value| value.string_ptr().map(|text| text.to_owned()))
                })
                .expect("status format stays live during expansion");
            if line_format.is_none() {
                n = 0 as u_int;
                while n < width {
                    screen_write_putc(&mut ctx, &gc, ' ' as i32 as u_char);
                    n = n.wrapping_add(1);
                }
            } else {
                let expanded = format_expand_time_cstring(
                    ft,
                    line_format
                        .as_ref()
                        .and_then(|value| value.as_deref())
                        .unwrap_or(c""),
                );
                let mut status = c_owner.borrow_status_mut();
                let sle = &mut status.entries[i as usize];
                if force != 0 || sle.expanded.as_ref() != Some(&expanded) {
                    changed = 1 as ::core::ffi::c_int;
                    n = 0 as u_int;
                    while n < width {
                        screen_write_putc(&mut ctx, &gc, ' ' as i32 as u_char);
                        n = n.wrapping_add(1);
                    }
                    screen_write_cursormove(
                        &mut ctx,
                        0 as ::core::ffi::c_int,
                        i as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                    );
                    style_ranges_clear(&mut sle.ranges);
                    format_draw(
                        &mut ctx,
                        &gc,
                        width,
                        &expanded,
                        Some(&mut sle.ranges),
                        0 as ::core::ffi::c_int,
                    );
                    sle.expanded = Some(expanded);
                }
            }
            i = i.wrapping_add(1);
        }
    } else {
        n = 0 as u_int;
        while n < width.wrapping_mul(lines) {
            screen_write_putc(&mut ctx, &gc, ' ' as i32 as u_char);
            n = n.wrapping_add(1);
        }
    }
    screen_write_stop(ctx);
    c_owner.borrow_status_mut().screen = screen;
    drop(ft_owner);
    log_debug(format_args!(
        "{} exit: force={}, changed={}",
        "status_redraw",
        (force) as i32,
        (changed) as i32
    ));
    (force != 0 || changed != 0) as ::core::ffi::c_int
}
fn status_message_escape(s: &CStr) -> CString {
    let source = s.to_bytes();
    let extra = source.iter().filter(|&&byte| byte == b'#').count();
    let mut escaped = Vec::with_capacity(source.len() + extra);
    for &byte in source {
        if byte == b'#' {
            escaped.push(b'#');
        }
        escaped.push(byte);
    }
    CString::new(escaped).expect("C string has no interior NUL")
}

pub unsafe fn status_message_set(
    c_owner: Option<&ClientRef>,
    mut delay: ::core::ffi::c_int,
    mut ignore_styles: ::core::ffi::c_int,
    mut ignore_keys: ::core::ffi::c_int,
    mut no_freeze: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let message = format_message_with(write);
    log_debug(format_args!("status_message_set: {}", log_cstr(&message)));
    if let Some(client) = c_owner {
        client.show_status_message(message, delay, ignore_styles, ignore_keys, no_freeze);
    } else {
        server_add_message(|out| {
            out.write_all(b"message: ")?;
            write_cstr(out, &*message)
        });
    }
}
pub unsafe fn status_message_clear(client: &ClientRef) {
    client.clear_status_message();
}

unsafe fn status_message_area(c: &ClientRef) -> (u_int, u_int) {
    let (terminal_width, _) = c.terminal_size();
    let sy = crate::style::style_resolve_with_options(
        c"message-style",
        None,
        &OptionsScope::Session(c.attached_session()),
    );
    let mut width = match sy.filter(|style| style.width >= 0) {
        Some(style) if style.width_percentage != 0 => {
            terminal_width.wrapping_mul(style.width as u_int) / 100
        }
        Some(style) => style.width as u_int,
        None => terminal_width,
    };
    if width == 0 || width > terminal_width {
        width = terminal_width;
    }
    let x = match sy.map(|style| style.align) {
        Some(STYLE_ALIGN_CENTRE | STYLE_ALIGN_ABSOLUTE_CENTRE) => {
            terminal_width.wrapping_sub(width) / 2
        }
        Some(STYLE_ALIGN_RIGHT) => terminal_width.wrapping_sub(width),
        _ => 0,
    };
    (x, width)
}
pub unsafe fn status_message_redraw(c_owner: &ClientRef) -> ::core::ffi::c_int {
    let mut msgfmt_session_value: Option<std::ffi::CString> = None;

    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let (terminal_width, terminal_height) = c_owner.terminal_size();
    let mut s: Option<SessionRef> = c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let mut old_screen: screen = screen::empty();
    let mut lines: u_int = 0;
    let mut messageline: u_int = 0;
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
    if terminal_width == 0 as u_int || terminal_height == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    old_screen = std::mem::replace(c_owner.borrow_status_mut().active_screen(), screen::empty());
    lines = status_line_size(c.as_ref().expect("live client"));
    if lines <= 1 as u_int {
        lines = 1 as u_int;
    }
    screen_init(
        c_owner.borrow_status_mut().active_screen(),
        terminal_width,
        lines,
        0,
    );
    messageline = status_prompt_line_at(c.as_ref().expect("live client"));
    if messageline > lines.wrapping_sub(1 as u_int) {
        messageline = lines.wrapping_sub(1 as u_int);
    }
    let (ax, aw) = status_message_area(c.as_ref().expect("live client"));
    let mut ft_owner = format_create_defaults(
        None,
        Some(c_owner),
        None,
        (refbox::Weak::new()).clone(),
        None,
    );
    let ft = &mut *ft_owner;
    crate::style::style_apply_with_options(
        &mut gc,
        c"message-style",
        Some(&mut *ft),
        &OptionsScope::Session(Rc::downgrade(s.as_ref().expect("live session"))),
    );
    let (message, ignore_styles) = c_owner.status_message_text();
    if ignore_styles {
        let msg = status_message_escape(message.as_deref().unwrap_or(c""));
        format_add(ft, c"message", |out| write_cstr(out, &*msg));
    } else {
        format_add(ft, c"message", |out| write_cstr(out, (message).as_deref()));
    }
    format_add(ft, c"command_prompt", |out| {
        write!(out, "{}", { 0 as ::core::ffi::c_int })
    });
    msgfmt_session_value = Some(options_get_string(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"message-format",
    ));
    let msgfmt = msgfmt_session_value.as_deref().expect("option snapshot");
    let expanded = format_expand_time_cstring(ft, msgfmt);
    drop(ft_owner);
    let sl = c_owner.borrow_status_mut();
    // A shown message always has its own pushed screen over the base one.
    let active = sl
        .active
        .as_deref_mut()
        .expect("status message has a pushed screen");
    let mut ctx = screen_write_start(active);
    screen_write_fast_copy(
        &mut ctx,
        &sl.screen,
        0 as u_int,
        0 as u_int,
        terminal_width,
        lines,
    );
    screen_write_cursormove(
        &mut ctx,
        ax as ::core::ffi::c_int,
        messageline as ::core::ffi::c_int,
        0 as ::core::ffi::c_int,
    );
    format_draw(&mut ctx, &gc, aw, &expanded, None, 0 as ::core::ffi::c_int);
    screen_write_stop(ctx);
    let changed = grid_compare(sl.active_screen().grid(), old_screen.grid()) != 0;
    drop(old_screen);
    i32::from(changed)
}
unsafe fn status_prompt_accept(c_owner: &ClientRef) -> CmdOutcome {
    let _c: Option<ClientRef> = Some(c_owner.clone());
    if c_owner.prompt_observer().is_alive() {
        status_prompt_key(c_owner, 'y' as i32 as key_code, None);
    }
    CmdOutcome::Normal
}
pub unsafe fn status_prompt_set(
    c_owner: &ClientRef,
    fs: Option<&cmd_find_state>,
    msg: &CStr,
    input: Option<&CStr>,
    mut inputcb: status_prompt_input_cb,
    mut freecb: prompt_free_cb,
    mut flags: ::core::ffi::c_int,
    mut prompt_type: prompt_type,
) {
    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let mut pd = prompt_create_data::default();
    c_owner.clear_overlay();
    status_message_clear(c_owner);
    status_prompt_clear(c_owner);
    status_push_screen(c.as_ref().expect("live client"));
    prompt_set_options(
        &mut pd,
        c.as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .as_ref(),
    );
    pd.fs = fs;
    pd.prompt = msg;
    pd.input = input;
    pd.type_0 = prompt_type;
    pd.flags = flags;
    if let Some(mut inputcb) = inputcb.take() {
        let client = std::rc::Rc::downgrade(c_owner);
        pd.inputcb = Some(Box::new(move |s, key| {
            let owner = client.upgrade();
            inputcb(owner.as_ref(), s, key)
        }));
    }
    pd.freecb = freecb.take();
    let prompt = prompt_create(pd);
    let prompt_observer = prompt.downgrade();
    c_owner.install_prompt(prompt, flags);
    let prompt = prompt_observer;
    prompt_incremental_start(&prompt);
    if flags & PROMPT_SINGLE != 0 && flags & PROMPT_ACCEPT != 0 {
        cmdq_append(
            c.as_ref(),
            cmdq_get_callback_owned(
                c"status_prompt_accept",
                Some(Box::new({
                    let observer = std::rc::Rc::downgrade(c_owner);
                    move |_| unsafe {
                        observer
                            .upgrade()
                            .map_or(CmdOutcome::Normal, |owner| status_prompt_accept(&owner))
                    }
                })),
            ),
        );
    }
}
pub unsafe fn status_prompt_clear(c_owner: &ClientRef) {
    c_owner.clear_prompt();
}

pub unsafe fn status_prompt_update(c_owner: &ClientRef, msg: &CStr, input: Option<&CStr>) {
    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let prompt = c_owner.prompt_observer();
    if !prompt.is_alive() {
        return;
    }
    prompt_update(
        &mut prompt.try_borrow_mut().expect("unborrowed prompt"),
        msg,
        input,
    );
    c.as_ref()
        .expect("live client")
        .update_flags(CLIENT_REDRAWSTATUS as uint64_t, 0);
}
unsafe fn status_prompt_screen_line(c: &ClientRef) -> u_int {
    let (_, height) = c.terminal_size();
    let mut n: u_int = 0;
    if options_get_number(
        &c.attached_session()
            .upgrade()
            .expect("live session")
            .borrow_options_mut(),
        c"status-position",
    ) == 0 as ::core::ffi::c_longlong
    {
        return status_prompt_line_at(c);
    }
    n = status_line_size(c).wrapping_sub(status_prompt_line_at(c));
    if n <= height {
        return height.wrapping_sub(n);
    }
    height.wrapping_sub(1 as u_int)
}
pub unsafe fn status_prompt_redraw(c_owner: &ClientRef) -> ::core::ffi::c_int {
    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let (terminal_width, terminal_height) = c_owner.terminal_size();
    let mut old_screen: screen = screen::empty();
    let mut lines: u_int = 0;
    let mut promptline: u_int = 0;
    if terminal_width == 0 as u_int || terminal_height == 0 as u_int {
        return 0 as ::core::ffi::c_int;
    }
    old_screen = std::mem::replace(c_owner.borrow_status_mut().active_screen(), screen::empty());
    lines = status_line_size(c.as_ref().expect("live client"));
    if lines <= 1 as u_int {
        lines = 1 as u_int;
    }
    screen_init(
        c_owner.borrow_status_mut().active_screen(),
        terminal_width,
        lines,
        0,
    );
    promptline = status_prompt_line_at(c.as_ref().expect("live client"));
    if promptline > lines.wrapping_sub(1 as u_int) {
        promptline = lines.wrapping_sub(1 as u_int);
    }
    let (ax, aw) = status_message_area(c.as_ref().expect("live client"));
    // Draw the active screen with it taken out of the client, so the write
    // context can live across the prompt preparation below.
    let mut active = std::mem::take(c_owner.borrow_status_mut().active_screen());
    let mut ctx = screen_write_start(&mut active);
    {
        let status = c_owner.borrow_status_mut();
        // Without a pushed screen the status screen is the active one, now
        // in hand and freshly initialized, so there is nothing to copy.
        if status.active.is_some() {
            screen_write_fast_copy(
                &mut ctx,
                &status.screen,
                0 as u_int,
                0 as u_int,
                terminal_width,
                lines,
            );
        }
    }
    let pdd = prompt_draw_data {
        area_x: ax,
        area_width: aw,
        prompt_line: promptline,
    };
    let prompt = c_owner.prompt_observer();
    let prompt = prompt.try_borrow_mut().expect("unborrowed active prompt");
    crate::prompt::prompt_draw_cursor_style(&prompt, ctx.screen);
    let prepared = crate::prompt::prompt_prepare_draw(&prompt, ax, aw);
    let prompt_cx = crate::prompt::prompt_draw_prepared(&prompt, &mut ctx, pdd, prepared);
    drop(prompt);
    screen_write_stop(ctx);
    let changed = grid_compare(active.grid(), old_screen.grid()) != 0;
    let mut status = c_owner.borrow_status_mut();
    status.prompt_cx = prompt_cx;
    *status.active_screen() = active;
    drop(old_screen);
    i32::from(changed)
}
pub unsafe fn status_prompt_cursor(c: &ClientRef) -> (u_int, u_int) {
    let y = status_prompt_screen_line(c);
    (c.borrow_status().prompt_cx, y)
}
pub unsafe fn status_prompt_key(
    c_owner: &ClientRef,
    mut key: key_code,
    m: Option<&mouse_event>,
) -> prompt_key_result {
    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let prompt = c_owner.prompt_observer();
    if !prompt.is_alive() {
        return PROMPT_KEY_NOT_HANDLED;
    }
    let mut result: prompt_key_result = PROMPT_KEY_NOT_HANDLED;
    let mut redraw: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
    {
        let Some(m) = m else {
            return PROMPT_KEY_NOT_HANDLED;
        };
        if m.b & MOUSE_MASK_BUTTONS as u_int != MOUSE_BUTTON_1 as u_int
            || m.b & MOUSE_MASK_DRAG as u_int != 0
            || m.b & MOUSE_MASK_BUTTONS as u_int == 3 as u_int
            || m.y != status_prompt_screen_line(c.as_ref().expect("live client"))
        {
            return PROMPT_KEY_NOT_HANDLED;
        }
        let (ax, aw) = status_message_area(c.as_ref().expect("live client"));
        result = prompt_mouse(
            &mut c_owner
                .prompt_observer()
                .try_borrow_mut()
                .expect("unborrowed prompt"),
            (*m).x,
            ax,
            aw,
            Some(&mut redraw),
        );
    } else {
        result = prompt_key(&prompt, key, &mut redraw);
    }
    if redraw != 0 && c_owner.prompt_observer().is_alive() {
        c.as_ref()
            .expect("live client")
            .update_flags(CLIENT_REDRAWSTATUS as uint64_t, 0);
    }
    let current = c_owner.prompt_observer();
    let closed = current.is_alive()
        && prompt_closed(&current.try_borrow_mut().expect("unborrowed prompt")) != 0;
    if closed {
        status_prompt_clear(c_owner);
    }
    result
}

#[cfg(test)]
mod status_message_escape_tests {
    use super::{status_message_escape, CString};

    #[test]
    fn doubles_hash_without_changing_other_bytes() {
        let input = CString::new(b"#\xff##tail".as_slice()).unwrap();
        let escaped = status_message_escape(input.as_c_str());
        assert_eq!(escaped.as_bytes(), b"##\xff####tail");
        assert!(status_message_escape(c"").as_bytes().is_empty());
    }
}
