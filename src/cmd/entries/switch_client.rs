use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{
    cmdq_error, cmdq_get_client, cmdq_get_flags, cmdq_get_state_owned, cmdq_get_target_client,
};
use crate::src::ffi::libc::{getuid, strcmp, strcspn};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::key_bindings::key_bindings_get_table;
use crate::src::session::SessionIndex as _;
use crate::src::window::Window as _;

use crate::src::shared::abi::uid_t;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::{CLIENT_IGNORESIZE, CLIENT_READONLY};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{
    CMDQ_STATE_REPEAT, CMD_CLIENT_CFLAG, CMD_FIND_PREFER_UNATTACHED, CMD_READONLY,
};
use crate::src::shared::session::SessionRef;
use crate::src::shared::sort::sort_criteria;
use crate::src::shared::sort::*;
use crate::src::shared::window::winlink;
use crate::src::sort::sort_order_from_string;

use crate::src::{server_client::Client, session::Session};
pub static cmd_switch_client_entry: cmd_entry = {
    cmd_entry {
        name: c"switch-client",
        alias: Some(c"switchc"),
        args: args_parse {
            template: c"c:EFlnO:pt:rT:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-Elnpr] [-c target-client] [-t target-session] [-T key-table] [-O order]",
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
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let current = cmdq_get_state_owned(&*(item));
    let mut target: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut tflag: *const ::core::ffi::c_char =
        args_get(&*(args), 't' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut type_0: cmd_find_type = CMD_FIND_PANE;
    let mut flags: ::core::ffi::c_int = 0;
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut selected_session;
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wp = None;
    let mut tablename: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_ACTIVITY,
        reversed: 0,
        order_seq: &[],
    };
    let mut uid: uid_t = 0;
    if !tflag.is_null()
        && (*tflag.offset(strcspn(tflag, c":.%".as_ptr()) as isize) as ::core::ffi::c_int
            != '\0' as i32
            || strcmp(tflag, c"=".as_ptr()) == 0 as ::core::ffi::c_int)
    {
        type_0 = CMD_FIND_PANE;
        flags = 0 as ::core::ffi::c_int;
    } else {
        type_0 = CMD_FIND_SESSION;
        flags = CMD_FIND_PREFER_UNATTACHED;
    }
    if target.find_target(Some(item_handle), nullable_cstr(tflag), type_0, flags)
        != 0 as ::core::ffi::c_int
    {
        return CMD_RETURN_ERROR;
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
                return CMD_RETURN_ERROR;
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
    tablename =
        args_get(&*(args), 'T' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !tablename.is_null() {
        let Some(table) = key_bindings_get_table(std::ffi::CStr::from_ptr(tablename), 0) else {
            cmdq_error(item_handle, |out| {
                out.write_all(b"table ")?;
                write_cstr(out, nullable_cstr(tablename))?;
                out.write_all(b" doesn't exist")
            });
            return CMD_RETURN_ERROR;
        };
        tc.as_ref().expect("target client").select_key_table(table);
        return CMD_RETURN_NORMAL;
    }
    sort_crit.order = sort_order_from_string(nullable_cstr(
        args_get(&*(args), 'O' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr()),
    ));
    if sort_crit.order as ::core::ffi::c_uint
        == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CMD_RETURN_ERROR;
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
            return CMD_RETURN_ERROR;
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
            return CMD_RETURN_ERROR;
        }
    } else if args_has(args, 'l' as i32 as u_char) != 0 {
        selected_session = crate::src::session::sessions
            .resolve(&tc.as_ref().expect("live client").previous_session());
        s = selected_session.clone();
        if s.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"can't find last session"));
            return CMD_RETURN_ERROR;
        }
    } else {
        if cmdq_get_client((item).as_ref()).is_none() {
            return CMD_RETURN_NORMAL;
        }
        if let Some(pane) = wp.as_ref().filter(|pane| {
            wl.is_alive()
                && !wl
                    .get_unchecked()
                    .window_handle()
                    .expect("live window")
                    .active_pane()
                    .is_some_and(|active| std::rc::Rc::ptr_eq(&active, pane))
        }) {
            let window_owner = wl
                .get_unchecked()
                .window_handle()
                .cloned()
                .expect("switch window");
            window_owner.redraw_active_switch(wp.as_ref());
            window_owner.select_pane(pane, true);
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
    if !cmdq_get_flags(&*(item)) & CMDQ_STATE_REPEAT != 0 {
        tc.clone().expect("live client").set_key_table(None);
    }
    CMD_RETURN_NORMAL
}
