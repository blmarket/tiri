use crate::src::arguments::{args, args_get, args_has, args_parse};
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::format::{
    format_create_with_client, format_defaults, format_expand_cstring, format_true,
};
use crate::src::resize::recalculate_sizes;
use crate::src::server_fn::{server_kill_window, server_renumber_all, server_unlink_window};
use std::ffi::CStr;

use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::session::SessionRef;
use crate::src::shared::tree::RB_NEGINF;
use crate::src::shared::window::winlink;
use crate::src::window::{winlinks_minmax, winlinks_next, winlinks_prev};
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
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut filter: *const ::core::ffi::c_char =
        args_get(&*(args), 'f' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !filter.is_null() && args_has(&*args, 'a' as i32 as u_char) == 0 {
        cmdq_error(item_handle, |out| out.write_all(b"-f only valid with -a"));
        return CMD_RETURN_ERROR;
    }
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_unlink_window_entry,
    ) {
        if args_has(&*args, 'k' as i32 as u_char) == 0
            && (crate::src::shared::session::SessionRef::window_linked_outside_group(
                s.as_ref(),
                wl.get_unchecked().window_handle().expect("live window"),
            ) as i32)
                == 0
        {
            cmdq_error(item_handle, |out| {
                out.write_all(b"window only linked to one session")
            });
            return CMD_RETURN_ERROR;
        }
        server_unlink_window(s.as_ref().expect("live session"), wl.clone());
        recalculate_sizes();
        return CMD_RETURN_NORMAL;
    }
    if args_has(&*args, 'a' as i32 as u_char) != 0 {
        let filter = (!filter.is_null()).then(|| CStr::from_ptr(filter));
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
    CMD_RETURN_NORMAL
}
unsafe fn cmd_kill_window_all(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    filter: Option<&CStr>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut loop_0: refbox::Weak<winlink> = refbox::Weak::new();
    let mut found: u_int = 0;
    let mut kill_current: u_int = 0;
    if !winlinks_prev(wl.get_unchecked()).is_alive()
        && !winlinks_next(wl.get_unchecked()).is_alive()
    {
        return CMD_RETURN_NORMAL;
    }
    loop {
        found = 0 as u_int;
        loop_0 = s
            .as_ref()
            .expect("live session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while loop_0.is_alive() {
            if !crate::src::shared::rc::same(
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
        if crate::src::shared::rc::same(
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
    CMD_RETURN_NORMAL
}
unsafe fn cmd_kill_window_filter(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let filter: *const ::core::ffi::c_char = filter.map_or(std::ptr::null(), CStr::as_ptr);
    let item = item_handle.get();

    let queue_client = cmdq_get_client((item).as_ref());
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut flag: ::core::ffi::c_int = 0;
    if filter.is_null() {
        return 1 as ::core::ffi::c_int;
    }
    let mut ft_owner = format_create_with_client(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    ft = &raw mut *ft_owner;
    format_defaults(ft, None, Some(s_owner), wl.clone(), None);
    let expanded = format_expand_cstring(ft, filter);
    flag = format_true(expanded.as_ptr());
    drop(ft_owner);
    flag
}
