use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{
    cmdq_error, cmdq_get_client, cmdq_get_flags, cmdq_get_state_owned, cmdq_get_target_client,
};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::key_bindings::key_bindings_get_table;
use crate::server_fn::server_redraw_window;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use libc::getuid;

use crate::shared::abi::uid_t;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::{CLIENT_IGNORESIZE, CLIENT_READONLY};
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::command::{
    CMDQ_STATE_REPEAT, CMD_CLIENT_CFLAG, CMD_FIND_PREFER_UNATTACHED, CMD_READONLY,
};
use crate::shared::session::SessionRef;
use crate::shared::sort::sort_criteria;
use crate::shared::sort::*;
use crate::shared::window::winlink;
use crate::sort::sort_order_from_string;

use crate::{server_client::Client, session::Session};
pub static cmd_switch_client_entry: cmd_entry = {
    cmd_entry {
        name: c"switch-client",
        alias: Some(c"switchc"),
        args: args_parse {
            template: c"c:EFlnO:pt:rT:Z",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-ElnprZ] [-c target-client] [-t target-session] [-T key-table] [-O order]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_READONLY | CMD_CLIENT_CFLAG,
        exec: Some(cmd_switch_client_exec),
    }
};
unsafe fn cmd_switch_client_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let current = cmdq_get_state_owned(&(item));
    let mut target: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut tflag: *const ::core::ffi::c_char =
        args_get(args, 't' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut type_0: cmd_find_type = CMD_FIND_PANE;
    let mut flags: ::core::ffi::c_int = 0;
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut selected_session;
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wp = None;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let mut uid: uid_t = 0;
    if nullable_cstr(tflag).is_some_and(|target| {
        let target = target.to_bytes();
        target
            .iter()
            .any(|&byte| matches!(byte, b':' | b'.' | b'%'))
            || target == b"="
    }) {
        type_0 = CMD_FIND_PANE;
        flags = 0 as ::core::ffi::c_int;
    } else {
        type_0 = CMD_FIND_SESSION;
        flags = CMD_FIND_PREFER_UNATTACHED;
    }
    if target.find_target(Some(item_handle), nullable_cstr(tflag), type_0, flags)
        != 0 as ::core::ffi::c_int
    {
        return CmdOutcome::Error;
    }
    selected_session = target.session_handle();
    s = selected_session.clone();
    wl = target.winlink_handle();
    wp = target.pane_handle();
    if args_has(args, 'r' as i32 as u_char) != 0 {
        if tc.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0 {
            uid = c.as_ref().expect("live client").peer_uid();
            if uid != getuid() {
                cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
                return CmdOutcome::Error;
            }
        }
        if tc.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0 {
            tc.as_ref()
                .expect("live client")
                .update_flags(0, !(!(CLIENT_READONLY | CLIENT_IGNORESIZE) as uint64_t));
        } else {
            tc.as_ref()
                .expect("live client")
                .update_flags((CLIENT_READONLY | CLIENT_IGNORESIZE) as uint64_t, 0);
        }
    }
    if let Some(tablename) = args_get(args, 'T' as i32 as u_char) {
        let Some(table) = key_bindings_get_table(tablename, 0) else {
            cmdq_error(item_handle, |out| {
                out.write_all(b"table ")?;
                write_cstr(out, tablename)?;
                out.write_all(b" doesn't exist")
            });
            return CmdOutcome::Error;
        };
        tc.as_ref().expect("target client").select_key_table(table);
        return CmdOutcome::Normal;
    }
    sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(args, 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CmdOutcome::Error;
    }
    sort_crit.reversed = args_has(args, 'r' as i32 as u_char);
    if args_has(args, 'n' as i32 as u_char) != 0 {
        selected_session = (tc
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .as_ref())
        .and_then(|session| session.adjacent_session(&sort_crit, false));
        s = selected_session.clone();
        if s.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"can't find next session"));
            return CmdOutcome::Error;
        }
    } else if args_has(args, 'p' as i32 as u_char) != 0 {
        selected_session = (tc
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .as_ref())
        .and_then(|session| session.adjacent_session(&sort_crit, true));
        s = selected_session.clone();
        if s.is_none() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"can't find previous session")
            });
            return CmdOutcome::Error;
        }
    } else if args_has(args, 'l' as i32 as u_char) != 0 {
        selected_session = crate::session::sessions::resolve(
            &tc.as_ref().expect("live client").previous_session(),
        );
        s = selected_session.clone();
        if s.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"can't find last session"));
            return CmdOutcome::Error;
        }
    } else {
        if cmdq_get_client(Some(&*item)).is_none() {
            return CmdOutcome::Normal;
        }
        if let Some(pane) = wp.as_ref().filter(|pane| {
            wl.is_alive()
                && !winlink_window(&wl)
                    .expect("live window")
                    .active_pane()
                    .ptr_eq(&std::rc::Rc::downgrade(pane))
        }) {
            let window_owner = winlink_window(&wl).expect("switch window");
            let visible = pane.is_visible();
            if !visible && window_owner.push_zoom(false, args_has(args, b'Z') != 0) {
                server_redraw_window(&window_owner);
            }
            window_owner.redraw_active_switch(wp.as_ref());
            window_owner.select_pane(pane, true);
            if !visible && window_owner.pop_zoom() {
                server_redraw_window(&window_owner);
            }
            drop(window_owner);
        }
        if wl.is_alive() {
            (s.as_ref().expect("live session")).select_winlink(wl.clone());
            *current.current.borrow_mut() = cmd_find_state::find_from_session(
                s.as_ref().expect("live session"),
                0 as ::core::ffi::c_int,
            );
        }
    }
    if args_has(args, 'E' as i32 as u_char) == 0 {
        let source = tc_owner
            .as_ref()
            .expect("target client")
            .with_environment(|env| env.expect("client environment").clone());
        selected_session
            .as_ref()
            .expect("target session")
            .update_environment(&source);
    }
    tc.clone().expect("live client").set_session(s.as_ref());
    if !cmdq_get_flags(&(item)) & CMDQ_STATE_REPEAT != 0 {
        tc.clone().expect("live client").set_key_table(None);
    }
    CmdOutcome::Normal
}
