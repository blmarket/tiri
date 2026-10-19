use crate::arguments::{args, args_get, args_has, args_parse};
use crate::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::format::{format_create, format_defaults, format_expand_cstring, format_true};
use crate::resize::recalculate_sizes;
use crate::server_fn::{server_kill_window, server_renumber_all, server_unlink_window};
use std::ffi::CStr;

use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::format::FORMAT_NONE;
use crate::shared::session::SessionRef;
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::winlink;
use crate::window::{winlinks_minmax, winlinks_next, winlinks_prev};
pub static cmd_kill_window_entry: cmd_entry = {
    cmd_entry {
        name: c"kill-window",
        alias: Some(c"killw"),
        args: args_parse {
            template: c"af:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-a] [-f filter] [-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_kill_window_exec),
    }
};
pub static cmd_unlink_window_entry: cmd_entry = {
    cmd_entry {
        name: c"unlink-window",
        alias: Some(c"unlinkw"),
        args: args_parse {
            template: c"kt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-k] [-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_kill_window_exec),
    }
};
unsafe fn cmd_kill_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut filter: Option<&CStr> = args_get(&*(args), 'f' as i32 as u_char);
    if filter.is_some() && args_has(&*args, 'a' as i32 as u_char) == 0 {
        cmdq_error(item_handle, |out| out.write_all(b"-f only valid with -a"));
        return CmdOutcome::Error;
    }
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_unlink_window_entry,
    ) {
        if args_has(&*args, 'k' as i32 as u_char) == 0
            && (crate::shared::session::SessionRef::window_linked_outside_group(
                s.as_ref(),
                wl.get_unchecked().window_handle().expect("live window"),
            ) as i32)
                == 0
        {
            cmdq_error(item_handle, |out| {
                out.write_all(b"window only linked to one session")
            });
            return CmdOutcome::Error;
        }
        server_unlink_window(s.as_ref().expect("live session"), wl.clone());
        recalculate_sizes();
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'a' as i32 as u_char) != 0 {
        return cmd_kill_window_all(item_handle, filter);
    }
    server_kill_window(
        wl.get_unchecked()
            .window_owner
            .as_ref()
            .expect("winlink window")
            .clone(),
        1 as ::core::ffi::c_int,
    );
    CmdOutcome::Normal
}
unsafe fn cmd_kill_window_all(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    filter: Option<&CStr>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut loop_0: refbox::Weak<winlink> = refbox::Weak::new();
    let mut found: u_int = 0;
    let mut kill_current: u_int = 0;
    if !winlinks_prev(wl.get_unchecked()).is_alive()
        && !winlinks_next(wl.get_unchecked()).is_alive()
    {
        return CmdOutcome::Normal;
    }
    loop {
        found = 0 as u_int;
        loop_0 = s
            .as_ref()
            .expect("live session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while loop_0.is_alive() {
            if !crate::shared::rc::same(
                loop_0.get_unchecked().window_handle(),
                wl.get_unchecked().window_handle(),
            ) && cmd_kill_window_filter(
                item_handle,
                s.as_ref().expect("live session"),
                (loop_0).clone(),
                filter,
            ) != 0
            {
                server_kill_window(
                    loop_0
                        .get_unchecked()
                        .window_owner
                        .as_ref()
                        .expect("winlink window")
                        .clone(),
                    0 as ::core::ffi::c_int,
                );
                found = found.wrapping_add(1);
                break;
            } else {
                loop_0 = winlinks_next(loop_0.get_unchecked());
            }
        }
        if !(found != 0 as u_int) {
            break;
        }
    }
    kill_current = 0 as u_int;
    found = kill_current;
    loop_0 = s
        .as_ref()
        .expect("live session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while loop_0.is_alive() {
        if crate::shared::rc::same(
            loop_0.get_unchecked().window_handle(),
            wl.get_unchecked().window_handle(),
        ) {
            found = found.wrapping_add(1);
            if cmd_kill_window_filter(
                item_handle,
                s.as_ref().expect("live session"),
                (loop_0).clone(),
                filter,
            ) != 0
            {
                kill_current = 1 as u_int;
            }
        }
        loop_0 = winlinks_next(loop_0.get_unchecked());
    }
    if kill_current != 0 && found > 1 as u_int {
        server_kill_window(
            wl.get_unchecked()
                .window_owner
                .as_ref()
                .expect("winlink window")
                .clone(),
            0 as ::core::ffi::c_int,
        );
    }
    server_renumber_all();
    CmdOutcome::Normal
}
unsafe fn cmd_kill_window_filter(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let item = item_handle.borrow();

    let queue_client = cmdq_get_client(Some(&*item));
    let mut flag: ::core::ffi::c_int = 0;
    if filter.is_null() {
        return 1 as ::core::ffi::c_int;
    }
    let mut ft_owner = format_create(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, None, Some(s_owner), wl.clone(), None);
    let expanded = format_expand_cstring(ft, filter);
    flag = format_true(expanded.as_ptr());
    drop(ft_owner);
    flag
}
