use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::cmd::find::TargetLookup as _;
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::bindings::key_bindings_get_table;
use tiri_core::format::bytes::write_cstr;
use tiri_core::server_fn::server_redraw_window;
use tiri_core::tiri_sys::user_id;
use tiri_core::window::Window as _;
use tiri_core::window_pane::WindowPane as _;
use tiri_core::winlink::winlink_window;

use crate::shared::command::{
    CMD_CLIENT_CFLAG, CMD_FIND_PREFER_UNATTACHED, CMD_READONLY, CMDQ_STATE_REPEAT,
};
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION, cmd_find_type};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::shared::abi::uid_t;
use tiri_core::shared::abi::{u_char, uint64_t};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::client::{CLIENT_IGNORESIZE, CLIENT_READONLY};
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::sort::sort_criteria;
use tiri_core::shared::sort::{SORT_ACTIVITY, SORT_END};
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::shared::window::winlink;
use tiri_core::sort::sort_order_from_string;

use tiri_core::{server_client::Client, session::Session};
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
fn cmd_switch_client_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let mut target: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let tflag = args_get(args, 't' as i32 as u_char);
    let mut type_0: cmd_find_type = CMD_FIND_PANE;
    let mut flags: ::core::ffi::c_int = 0;
    let c_owner = inv.client();
    let mut c: Option<ClientRef> = c_owner.clone();
    let tc_owner = inv.target_client();
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut selected_session;
    let mut s: Option<SessionRef> = None;
    let mut wl: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    let mut wp = None;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let mut uid: uid_t = 0;
    if tflag.is_some_and(|target| {
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
    if target.find_target(Some(&*inv), tflag, type_0, flags) != 0 as ::core::ffi::c_int {
        return ExecOutcome::err();
    }
    selected_session = target.session_handle();
    s = selected_session.clone();
    wl = target.winlink_handle();
    wp = target.pane_handle();
    if args_has(args, 'r' as i32 as u_char) != 0 {
        if tc.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0 {
            uid = c.as_ref().expect("live client").peer_uid();
            if uid != user_id() {
                inv.error( |out| out.write_all(b"client is read-only"));
                return ExecOutcome::err();
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
            inv.error( |out| {
                out.write_all(b"table ")?;
                write_cstr(out, tablename)?;
                out.write_all(b" doesn't exist")
            });
            return ExecOutcome::err();
        };
        tc.as_ref().expect("target client").select_key_table(table);
        return ExecOutcome::ok();
    }
    sort_crit.order = sort_order_from_string(args_get(args, 'O' as i32 as u_char));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        inv.error( |out| out.write_all(b"invalid sort order"));
        return ExecOutcome::err();
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
            inv.error( |out| out.write_all(b"can't find next session"));
            return ExecOutcome::err();
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
            inv.error( |out| {
                out.write_all(b"can't find previous session")
            });
            return ExecOutcome::err();
        }
    } else if args_has(args, 'l' as i32 as u_char) != 0 {
        selected_session = tiri_core::session::sessions::resolve(
            &tc.as_ref().expect("live client").previous_session(),
        );
        s = selected_session.clone();
        if s.is_none() {
            inv.error( |out| out.write_all(b"can't find last session"));
            return ExecOutcome::err();
        }
    } else {
        if inv.client().is_none() {
            return ExecOutcome::ok();
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
            inv.context_mut().current = cmd_find_state::from_session(
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
    if !inv.flags() & CMDQ_STATE_REPEAT != 0 {
        tc.clone().expect("live client").set_key_table(None);
    }
    ExecOutcome::ok()
})
}
