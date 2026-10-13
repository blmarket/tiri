use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::src::format::{
    format_create_with_client, format_defaults, format_expand_cstring, format_true,
};
use crate::src::server_fn::{server_destroy_session, server_redraw_session};
use crate::src::session::sessions;
use crate::src::session::Session;
use std::ffi::CStr;

use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::session::SessionRef;
use crate::src::shared::tree::RB_NEGINF;
use crate::src::shared::window::winlink;
use crate::src::shared::window::WINLINK_ALERTFLAGS;
use crate::src::window::Window as _;
use crate::src::window::{winlinks_minmax, winlinks_next};
pub static cmd_kill_session_entry: cmd_entry = {
    cmd_entry {
        name: c"kill-session",
        alias: None,
        args: args_parse {
            template: c"aCgf:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-aCg] [-f filter] [-t target-session]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_kill_session_exec),
    }
};
unsafe fn cmd_kill_session_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let source = (*target).s.upgrade().expect("live target session");
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut filter: *const ::core::ffi::c_char =
        args_get(&*(args), 'f' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !filter.is_null()
        && (args_has(args, 'a' as i32 as u_char) == 0 || args_has(args, 'C' as i32 as u_char) != 0)
    {
        cmdq_error(item_handle, |out| out.write_all(b"-f only valid with -a"));
        return CMD_RETURN_ERROR;
    }
    if args_has(args, 'C' as i32 as u_char) != 0 {
        wl = source.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            wl.get_unchecked()
                .window_handle()
                .expect("live window")
                .clear_alert_flags();
            wl.get_mut_unchecked().flags &= !WINLINK_ALERTFLAGS;
            wl = winlinks_next(wl.get_unchecked());
        }
        server_redraw_session(&source);
    } else if args_has(args, 'a' as i32 as u_char) != 0 {
        let filter = (!filter.is_null()).then(|| CStr::from_ptr(filter));
        return cmd_kill_session_all(item_handle, filter);
    } else if args_has(args, 'g' as i32 as u_char) != 0 && {
        sg = crate::src::session::session_groups::for_session(&std::rc::Rc::downgrade(&source));
        sg.is_alive()
    } {
        for session_owner in crate::src::session::session_group_members(&sg) {
            server_destroy_session(&session_owner);
            session_owner.destroy(
                (1 as ::core::ffi::c_int) != 0,
                std::ffi::CStr::from_ptr(c"cmd_kill_session_exec".as_ptr()),
            );
        }
    } else {
        server_destroy_session(&source);
        source.destroy(
            (1 as ::core::ffi::c_int) != 0,
            std::ffi::CStr::from_ptr(c"cmd_kill_session_exec".as_ptr()),
        );
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_kill_session_all(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    filter: Option<&CStr>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut s: Option<SessionRef> =
        (*crate::src::cmd::queue::cmdq_get_target_mut(&mut *item)).session_handle();
    let mut sloop: Option<SessionRef> = None;
    let mut sloop_owner = sessions::first();
    sloop = sloop_owner.clone();
    while !sloop.is_none() {
        let name = sloop.as_ref().expect("live session").name();
        if !(crate::src::shared::rc::same(sloop.as_ref(), s.as_ref()))
            && !(cmd_kill_session_filter(
                item_handle,
                sloop.as_ref().expect("live session"),
                filter,
            ) == 0)
        {
            server_destroy_session(sloop_owner.as_ref().expect("registered session"));
            (sloop_owner.as_ref().expect("registered session")).destroy(
                (1 as ::core::ffi::c_int) != 0,
                std::ffi::CStr::from_ptr(c"cmd_kill_session_all".as_ptr()),
            );
        }
        sloop_owner = sessions::after(&name);
        sloop = sloop_owner.clone();
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_kill_session_filter(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    s_owner: &SessionRef,
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
    format_defaults(ft, None, Some(s_owner), (refbox::Weak::new()).clone(), None);
    let expanded = format_expand_cstring(ft, filter);
    flag = format_true(expanded.as_ptr());
    drop(ft_owner);
    flag
}
