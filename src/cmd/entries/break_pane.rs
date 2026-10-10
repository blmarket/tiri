use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::find::cmd_find_from_session;
use crate::src::cmd::queue::{
    cmdq_error, cmdq_get_state_owned, cmdq_get_target_client, cmdq_print,
};
use crate::src::events::events_fire_window;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_cstring;
use crate::src::names::default_window_name_cstring;
use crate::src::options::{options_get_number, options_set_number};
use crate::src::window::Window as _;

use crate::src::server_client::Client as _;
use crate::src::server_fn::{
    server_link_window, server_redraw_session, server_status_session_group, server_unlink_window,
};
use crate::src::session::Session;

use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::CMD_FIND_WINDOW_INDEX;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::session::SessionRef;

use crate::src::shared::window::winlink;
use crate::src::tmux::{check_name, clean_name_cstring};
use crate::src::window::{winlink_find_by_index, winlink_find_by_window, winlink_shuffle_up};
use crate::src::window_pane::WindowPane as _;

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
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let current = cmdq_get_state_owned(&*(item));
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut source: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_source_mut(&mut *item);
    let tc_owner = cmdq_get_target_client((item).as_ref());
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
        let mut name: *const ::core::ffi::c_char = args_get(&*(args), 'n' as i32 as u_char)
            .map_or(std::ptr::null(), |value| value.as_ptr());
        if !name.is_null() && !check_name(CStr::from_ptr(name)) {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid window name: ")?;
                write_cstr(out, nullable_cstr(name))
            });
            return CMD_RETURN_ERROR;
        }
        before = args_has(args, 'b' as i32 as u_char);
        if args_has(args, 'a' as i32 as u_char) != 0 || before != 0 {
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
                return CMD_RETURN_ERROR;
            }
        }
        if source_window.pane_snapshot().len() == 1 {
            if let Err(link_error) = server_link_window(
                src_s.as_ref().expect("live session"),
                wl.clone(),
                dst_s.as_ref().expect("live session"),
                idx,
                0 as ::core::ffi::c_int,
                (args_has(args, 'd' as i32 as u_char) == 0) as ::core::ffi::c_int,
            ) {
                cmdq_error(item_handle, |out| write_cstr(out, &*link_error));
                return CMD_RETURN_ERROR;
            }
            if !name.is_null() {
                source_window.rename(CStr::from_ptr(name), false);
                source_window.with_options_mut(|options| {
                    options_set_number(options, c"automatic-rename", 0)
                });
            }
            server_unlink_window(src_s.as_ref().expect("live session"), wl.clone());
            wl = dst_s
                .as_ref()
                .expect("live session")
                .with_winlinks(|links| winlink_find_by_window(links, &source_window));
            if !wl.is_alive() {
                return CMD_RETURN_ERROR;
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
                return CMD_RETURN_ERROR;
            }
            ClientRef::forget_pane(&pane_owner);
            let rest = source_window
                .pane_snapshot()
                .into_iter()
                .filter(|pane| !std::rc::Rc::ptr_eq(pane, &pane_owner))
                .collect::<Vec<_>>();
            source_window
                .rearrange_panes(&rest)
                .expect("a removal is never refused");
            let (sx, sy) = source_window.size();
            let (xpixel, ypixel) = source_window.cell_size();
            let window = crate::src::window::window_create(sx, sy, xpixel, ypixel);
            let destination = std::rc::Rc::downgrade(&window);
            pane_owner.reparent(&window);
            window.set_latest_client(tc.as_ref());
            window
                .rearrange_panes(std::slice::from_ref(&pane_owner))
                .expect("a first pane always fits");
            if name.is_null() {
                window.initialize_name(default_window_name_cstring(&window), false);
            } else {
                let cleaned = clean_name_cstring(std::ffi::CStr::from_ptr(name), 0)
                    .expect("check_name validated the explicit window name");
                window.initialize_name(cleaned, true);
            }
            window.refresh_fill_cells();
            if idx == -(1 as ::core::ffi::c_int) {
                idx = (-(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong
                    - dst_s
                        .as_ref()
                        .expect("live session")
                        .with_options_mut(|options| options_get_number(options, c"base-index")))
                    as ::core::ffi::c_int;
            }
            let destination_session = (*target)
                .session_handle()
                .expect("break-pane destination session");
            wl = match crate::src::session::Session::attach_window(
                &destination_session,
                &window,
                idx,
            ) {
                Ok(wl) => wl,
                Err(error) => {
                    cmdq_error(item_handle, |out| write_cstr(out, &*error));
                    window.release(c"cmd_break_pane_exec");
                    return CMD_RETURN_ERROR;
                }
            };
            pane_owner.mark_changed();
            pane_owner.refresh_palette();
            window.release(c"cmd_break_pane_exec");
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
            if args_has(args, 'd' as i32 as u_char) == 0 {
                (dst_s.as_ref().expect("live session")).select_index(wl.get_unchecked().idx);
                cmd_find_from_session(
                    &mut *current.current.borrow_mut(),
                    dst_s.as_ref().expect("live session"),
                    0 as ::core::ffi::c_int,
                );
            }
            server_redraw_session(src_s.as_ref().expect("live session"));
            if !crate::src::shared::rc::same(src_s.as_ref(), dst_s.as_ref()) {
                server_redraw_session(dst_s.as_ref().expect("live session"));
            }
            server_status_session_group(src_s.as_ref().expect("live session"));
            if !crate::src::shared::rc::same(src_s.as_ref(), dst_s.as_ref()) {
                server_status_session_group(dst_s.as_ref().expect("live session"));
            }
        }
        if args_has(args, 'P' as i32 as u_char) != 0 {
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
        CMD_RETURN_NORMAL
    })();
    source_window.release(c"break pane source");
    result
}
