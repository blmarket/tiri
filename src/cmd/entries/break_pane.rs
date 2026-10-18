use crate::arguments::{args, args_get, args_has, args_parse};
use crate::cmd::cmd_get_args_mut;
use crate::cmd::queue::{cmdq_error, cmdq_get_state_owned, cmdq_get_target_client, cmdq_print};
use crate::events::events_fire_window;
use crate::format::bytes::write_cstr;
use crate::format::format_single_cstring;
use crate::names::default_window_name_cstring;
use crate::options::{options_get_number, options_set_number};
use crate::shared::window::WindowRef;
use crate::window::Window as _;

use crate::server_client::Client as _;
use crate::server_fn::{
    server_link_window, server_redraw_session, server_status_session_group, server_unlink_window,
};
use crate::session::Session;

use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_FIND_WINDOW_INDEX;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::session::SessionRef;

use crate::shared::window::winlink;
use crate::tmux::{check_name, clean_name_cstring};
use crate::window::{winlink_find_by_index, winlink_find_by_window, winlink_shuffle_up};
use crate::window_pane::WindowPane as _;

pub const BREAK_PANE_TEMPLATE: &std::ffi::CStr = c"#{session_name}:#{window_index}.#{pane_index}";
use std::ffi::CStr;

pub static cmd_break_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"break-pane",
        alias: Some(c"breakp"),
        args: args_parse {
            template: c"abdPF:n:s:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-abdP] [-F format] [-n window-name] [-s src-pane] [-t dst-window]",
        source: cmd_entry_flag {
            flag: 's' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: CMD_FIND_WINDOW_INDEX,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_break_pane_exec),
    }
};
unsafe fn cmd_break_pane_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let current = cmdq_get_state_owned(&*(item));
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut source_state = crate::cmd::queue::cmdq_get_source(&item).clone();
    let mut source: *mut cmd_find_state = &mut source_state;
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut wl: refbox::Weak<winlink> = (*source).winlink_handle();
    let mut src_s: Option<SessionRef> = (*source).session_handle();
    let mut dst_s: Option<SessionRef> = (*target).session_handle();
    let pane_owner = (*source).pane_handle().expect("live break pane");
    let source_window = wl
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("live source window");
    let result = (|| {
        let _cause: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
        let mut idx: ::core::ffi::c_int = (*target).idx;
        let mut before: ::core::ffi::c_int = 0;
        let mut old_idx: ::core::ffi::c_int = wl.get_unchecked().idx;
        let mut template: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut name: Option<&CStr> = args_get(&*(args), 'n' as i32 as u_char);
        if name.is_some_and(|name| !check_name(name)) {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid window name: ")?;
                write_cstr(out, name)
            });
            return CmdOutcome::Error;
        }
        before = args_has(&*args, 'b' as i32 as u_char);
        if args_has(&*args, 'a' as i32 as u_char) != 0 || before != 0 {
            if (*target).winlink_handle().is_alive() {
                idx = winlink_shuffle_up(
                    dst_s.as_ref().expect("live session"),
                    ((*target).winlink_handle()).clone(),
                    before,
                );
            } else {
                idx = winlink_shuffle_up(
                    dst_s.as_ref().expect("live session"),
                    (dst_s.as_ref().expect("live session").current_winlink()).clone(),
                    before,
                );
            }
            if idx == -(1 as ::core::ffi::c_int) {
                return CmdOutcome::Error;
            }
        }
        if source_window.pane_snapshot().len() == 1 {
            if let Err(link_error) = server_link_window(
                src_s.as_ref().expect("live session"),
                wl.clone(),
                dst_s.as_ref().expect("live session"),
                idx,
                0 as ::core::ffi::c_int,
                (args_has(&*args, 'd' as i32 as u_char) == 0) as ::core::ffi::c_int,
            ) {
                cmdq_error(item_handle, |out| write_cstr(out, &*link_error));
                return CmdOutcome::Error;
            }
            if let Some(name) = name {
                source_window.rename(name, false);
                options_set_number(
                    &mut source_window.borrow_options_mut(),
                    c"automatic-rename",
                    0,
                );
            }
            server_unlink_window(src_s.as_ref().expect("live session"), wl.clone());
            wl = dst_s
                .as_ref()
                .expect("live session")
                .with_winlinks(|links| winlink_find_by_window(links, &source_window));
            if !wl.is_alive() {
                return CmdOutcome::Error;
            }
            pane_owner.notify_moved(
                &source_window,
                old_idx,
                &source_window,
                wl.get_unchecked().idx,
            );
        } else {
            if idx != -(1 as ::core::ffi::c_int)
                && dst_s
                    .as_ref()
                    .expect("live session")
                    .with_winlinks(|links| winlink_find_by_index(links, idx))
                    .is_alive()
            {
                cmdq_error(item_handle, |out| {
                    write!(out, "index in use: {}", (idx) as i32)
                });
                return CmdOutcome::Error;
            }
            ClientRef::forget_pane(&pane_owner);
            let rest = source_window
                .pane_snapshot()
                .iter()
                .filter_map(std::rc::Weak::upgrade)
                .filter(|pane| !std::rc::Rc::ptr_eq(pane, &pane_owner))
                .collect::<Vec<_>>();
            source_window
                .rearrange_panes(&rest)
                .expect("a removal is never refused");
            let (sx, sy) = source_window.size();
            let (xpixel, ypixel) = source_window.cell_size();
            let window = crate::window::window_create(sx, sy, xpixel, ypixel);
            let destination = WindowRef::downgrade(&window);
            pane_owner.reparent(&window);
            window.set_latest_client(tc.as_ref());
            window
                .rearrange_panes(std::slice::from_ref(&pane_owner))
                .expect("a first pane always fits");
            if let Some(name) = name {
                let cleaned = clean_name_cstring(name, 0)
                    .expect("check_name validated the explicit window name");
                window.initialize_name(cleaned, true);
            } else {
                window.initialize_name(default_window_name_cstring(&window), false);
            }
            window.refresh_fill_cells();
            if idx == -(1 as ::core::ffi::c_int) {
                idx = (-(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong
                    - options_get_number(
                        &dst_s.as_ref().expect("live session").borrow_options_mut(),
                        c"base-index",
                    )) as ::core::ffi::c_int;
            }
            let destination_session = (*target)
                .session_handle()
                .expect("break-pane destination session");
            wl = match crate::session::Session::attach_window(&destination_session, &window, idx) {
                Ok(wl) => wl,
                Err(error) => {
                    cmdq_error(item_handle, |out| write_cstr(out, &*error));
                    drop(window);
                    return CmdOutcome::Error;
                }
            };
            pane_owner.mark_changed();
            pane_owner.refresh_palette();
            drop(window);
            events_fire_window(
                c"window-created",
                destination.upgrade().expect("live destination window"),
            );
            pane_owner.notify_moved(
                &source_window,
                old_idx,
                &destination.upgrade().expect("live destination window"),
                wl.get_unchecked().idx,
            );
            if args_has(&*args, 'd' as i32 as u_char) == 0 {
                (dst_s.as_ref().expect("live session")).select_index(wl.get_unchecked().idx);
                *current.current.borrow_mut() = cmd_find_state::find_from_session(
                    dst_s.as_ref().expect("live session"),
                    0 as ::core::ffi::c_int,
                );
            }
            server_redraw_session(src_s.as_ref().expect("live session"));
            if !crate::shared::rc::same(src_s.as_ref(), dst_s.as_ref()) {
                server_redraw_session(dst_s.as_ref().expect("live session"));
            }
            server_status_session_group(src_s.as_ref().expect("live session"));
            if !crate::shared::rc::same(src_s.as_ref(), dst_s.as_ref()) {
                server_status_session_group(dst_s.as_ref().expect("live session"));
            }
        }
        if args_has(&*args, 'P' as i32 as u_char) != 0 {
            template = args_get(&*(args), 'F' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr());
            if template.is_null() {
                template = BREAK_PANE_TEMPLATE.as_ptr();
            }
            let cp = format_single_cstring(
                Some(item_handle),
                template,
                tc.as_ref(),
                dst_s.as_ref(),
                wl.clone(),
                Some(&pane_owner),
            );
            cmdq_print(item_handle, |out| write_cstr(out, &*cp));
        }
        CmdOutcome::Normal
    })();
    drop(source_window);
    result
}
