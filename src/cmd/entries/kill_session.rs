use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::format::{format_create, format_defaults, format_expand_cstring, format_true};
use crate::server_fn::{server_destroy_session, server_redraw_session};
use crate::session::sessions;
use crate::session::Session;
use crate::winlink::winlink_window;
use std::ffi::CStr;

use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::format::FORMAT_NONE;
use crate::shared::session::SessionRef;
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::winlink;
use crate::shared::window::WINLINK_ALERTFLAGS;
use crate::window::Window as _;
use crate::window::{winlink_next, winlinks_minmax};
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
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let source = target.s.upgrade().expect("live target session");
    let mut sg = crate::shared::session::SessionGroupWeak::new();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut filter: Option<&CStr> = args_get(args, 'f' as i32 as u_char);
    if filter.is_some()
        && (args_has(args, 'a' as i32 as u_char) == 0 || args_has(args, 'C' as i32 as u_char) != 0)
    {
        cmdq_error(item_handle, |out| out.write_all(b"-f only valid with -a"));
        return CmdOutcome::Error;
    }
    if args_has(args, 'C' as i32 as u_char) != 0 {
        wl = source.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            winlink_window(&wl)
                .expect("live window")
                .clear_alert_flags();
            wl.try_borrow_mut().expect("live winlink").flags &= !WINLINK_ALERTFLAGS;
            wl = winlink_next(&wl);
        }
        server_redraw_session(&source);
    } else if args_has(args, 'a' as i32 as u_char) != 0 {
        return cmd_kill_session_all(item_handle, filter);
    } else if args_has(args, 'g' as i32 as u_char) != 0 && {
        sg = crate::session::session_groups::for_session(&std::rc::Rc::downgrade(&source));
        sg.is_alive()
    } {
        for session_owner in crate::session::session_group_members(&sg) {
            server_destroy_session(&session_owner);
            session_owner.destroy((1 as ::core::ffi::c_int) != 0, c"cmd_kill_session_exec");
        }
    } else {
        server_destroy_session(&source);
        source.destroy((1 as ::core::ffi::c_int) != 0, c"cmd_kill_session_exec");
    }
    CmdOutcome::Normal
}
unsafe fn cmd_kill_session_all(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    filter: Option<&CStr>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut s: Option<SessionRef> = crate::cmd::queue::cmdq_get_target(&item).session_handle();
    let mut sloop: Option<SessionRef> = None;
    let mut sloop_owner = sessions::first();
    sloop = sloop_owner.clone();
    while !sloop.is_none() {
        let name = sloop.as_ref().expect("live session").name();
        if !(crate::shared::rc::same(sloop.as_ref(), s.as_ref()))
            && !(cmd_kill_session_filter(
                item_handle,
                sloop.as_ref().expect("live session"),
                filter,
            ) == 0)
        {
            server_destroy_session(sloop_owner.as_ref().expect("registered session"));
            (sloop_owner.as_ref().expect("registered session"))
                .destroy((1 as ::core::ffi::c_int) != 0, c"cmd_kill_session_all");
        }
        sloop_owner = sessions::after(&name);
        sloop = sloop_owner.clone();
    }
    CmdOutcome::Normal
}
unsafe fn cmd_kill_session_filter(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    s_owner: &SessionRef,
    filter: Option<&CStr>,
) -> ::core::ffi::c_int {
    let item = item_handle.borrow();

    let queue_client = cmdq_get_client(Some(&*item));
    let Some(filter) = filter else {
        return 1 as ::core::ffi::c_int;
    };
    let mut ft_owner = format_create(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    let ft = &mut *ft_owner;
    format_defaults(ft, None, Some(s_owner), (refbox::Weak::new()).clone(), None);
    let expanded = format_expand_cstring(ft, filter);
    let flag = format_true(&expanded);
    drop(ft_owner);
    ::core::ffi::c_int::from(flag)
}
