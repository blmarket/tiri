use crate::cfg::{cfg_finished, cfg_show_causes};
use crate::cmd::cmd_get_args;
use crate::cmd::find::TargetLookup as _;
use crate::cmd::queue::{cmdq_error, cmdq_get_flags};
use std::ffi::CStr;
use tiri_core::actions::TaskOutcome as CmdOutcome;
use tiri_core::actions::queue::{
    client as cmdq_get_client, execution_state as cmdq_get_state_owned,
};
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::compat::imsg::{MSG_DETACH, MSG_DETACHKILL, msgtype};
use tiri_core::events::events_fire_client;
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::format_single_cstring;
use tiri_core::server_client::ClientRegistry;
use tiri_core::tiri_sys::user_id;
use tiri_core::window::Window as _;

use crate::cmd::queue::cmdq_item;
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION, cmd_find_type};
use crate::shared::command::{
    CMD_FIND_PREFER_UNATTACHED, CMD_READONLY, CMD_STARTSERVER, CMDQ_STATE_REPEAT,
};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::session::sessions;
use tiri_core::shared::abi::uid_t;
use tiri_core::shared::abi::{u_char, uint64_t};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::client::{
    CLIENT_ATTACHED, CLIENT_CONTROL, CLIENT_IGNORESIZE, CLIENT_READONLY,
};
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::shared::window::winlink;

use tiri_core::window_pane::WindowPane as _;
use tiri_core::{server_client::Client, session::Session};
pub static cmd_attach_session_entry: cmd_entry = {
    cmd_entry {
        name: c"attach-session",
        alias: Some(c"attach"),
        args: args_parse {
            template: c"c:dEf:rt:x",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-dErx] [-c working-directory] [-f flags] [-t target-session]",
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
        flags: CMD_STARTSERVER | CMD_READONLY,
        exec: Some(cmd_attach_session_exec),
    }
};
pub fn cmd_attach_session(
    item_handle: &std::rc::Rc<cmdq_item>,
    tflag: Option<&CStr>,
    mut dflag: ::core::ffi::c_int,
    mut xflag: ::core::ffi::c_int,
    mut rflag: ::core::ffi::c_int,
    cflag: Option<&CStr>,
    mut Eflag: ::core::ffi::c_int,
    fflag: Option<&CStr>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let current = cmdq_get_state_owned(&(item));
    let mut target: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut type_0: cmd_find_type = CMD_FIND_PANE;
    let mut flags: ::core::ffi::c_int = 0;
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut c_loop: Option<ClientRef> = None;
    let mut s: Option<SessionRef> = None;
    let mut wl: tiri_core::refbox::Weak<winlink> = tiri_core::refbox::Weak::new();
    let mut wp = None;
    let mut msgtype: msgtype = 0 as msgtype;
    let mut uid: uid_t = 0;
    if !!sessions::is_empty() {
        cmdq_error(item_handle, |out| out.write_all(b"no sessions"));
        return CmdOutcome::Error;
    }
    if c.is_none() {
        return CmdOutcome::Normal;
    }
    if c.as_ref().expect("live client").is_nested() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"sessions should be nested with care, unset $TMUX to force")
        });
        return CmdOutcome::Error;
    }
    if tflag.is_some_and(|target| {
        target
            .to_bytes()
            .iter()
            .any(|&byte| matches!(byte, b':' | b'.'))
    }) {
        type_0 = CMD_FIND_PANE;
        flags = 0 as ::core::ffi::c_int;
    } else {
        type_0 = CMD_FIND_SESSION;
        flags = CMD_FIND_PREFER_UNATTACHED;
    }
    if target.find_target(Some(item_handle), tflag, type_0, flags) != 0 as ::core::ffi::c_int {
        return CmdOutcome::Error;
    }
    s = target.session_handle();
    wl = target.winlink_handle();
    wp = target.pane_handle();
    if wl.is_alive() {
        if let Some(pane) = wp.as_ref() {
            let window = pane
                .window_observer()
                .upgrade()
                .expect("attach pane window");
            window.select_pane(pane, true);
            drop(window);
        }
        (s.as_ref().expect("live session")).select_winlink(wl.clone());
        if let Some(wp_value) = wp.as_ref() {
            *current.current.borrow_mut() =
                cmd_find_state::from_winlink_pane(wl.clone(), wp_value, 0 as ::core::ffi::c_int);
        } else {
            *current.current.borrow_mut() =
                cmd_find_state::from_winlink(wl.clone(), 0 as ::core::ffi::c_int);
        }
    }
    if let Some(cflag) = cflag {
        s.as_ref()
            .expect("target session")
            .set_cwd(Some(format_single_cstring(
                Some(item_handle),
                cflag,
                c.as_ref(),
                s.as_ref(),
                wl.clone(),
                wp.as_ref(),
            )));
    }
    if let Some(fflag) = fflag {
        c.clone().expect("live client").parse_flags(fflag);
    }
    if rflag != 0 {
        if c.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0 {
            uid = c.as_ref().expect("live client").peer_uid();
            if uid != user_id() {
                cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
                return CmdOutcome::Error;
            }
        }
        c.as_ref()
            .expect("live client")
            .update_flags((CLIENT_READONLY | CLIENT_IGNORESIZE) as uint64_t, 0);
    }
    c.as_ref().expect("live client").remember_session();
    if !c
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade()
        .is_none()
    {
        if dflag != 0 || xflag != 0 {
            if xflag != 0 {
                msgtype = MSG_DETACHKILL;
            } else {
                msgtype = MSG_DETACH;
            }
            let mut registry_c_loop_owner = ClientRegistry::first();
            c_loop = registry_c_loop_owner.clone();
            while !c_loop.is_none() {
                if !(!tiri_core::shared::rc::same(
                    c_loop
                        .as_ref()
                        .expect("live client")
                        .attached_session()
                        .upgrade()
                        .as_ref(),
                    s.as_ref(),
                ) || tiri_core::shared::rc::same(c.as_ref(), c_loop.as_ref()))
                {
                    c_loop.clone().expect("live client").detach(msgtype);
                }
                registry_c_loop_owner = ClientRegistry::next(
                    registry_c_loop_owner
                        .as_ref()
                        .expect("current registry client"),
                );
                c_loop = registry_c_loop_owner.clone();
            }
        }
        if Eflag == 0 {
            let source = c_owner
                .as_ref()
                .expect("terminal client")
                .with_environment(|env| env.expect("client environment").clone());
            target
                .session_handle()
                .expect("target session")
                .update_environment(&source);
        }
        c.clone().expect("live client").set_session(s.as_ref());
        if !cmdq_get_flags(&(item)) & CMDQ_STATE_REPEAT != 0 {
            c.clone().expect("live client").set_key_table(None);
        }
    } else {
        if let Err(cause) = (c_owner.as_ref().expect("terminal client")).open_terminal() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"open terminal failed: ")?;
                write_cstr(out, &*cause)
            });
            return CmdOutcome::Error;
        }
        if dflag != 0 || xflag != 0 {
            if xflag != 0 {
                msgtype = MSG_DETACHKILL;
            } else {
                msgtype = MSG_DETACH;
            }
            let mut registry_c_loop_owner = ClientRegistry::first();
            c_loop = registry_c_loop_owner.clone();
            while !c_loop.is_none() {
                if !(!tiri_core::shared::rc::same(
                    c_loop
                        .as_ref()
                        .expect("live client")
                        .attached_session()
                        .upgrade()
                        .as_ref(),
                    s.as_ref(),
                ) || tiri_core::shared::rc::same(c.as_ref(), c_loop.as_ref()))
                {
                    c_loop.clone().expect("live client").detach(msgtype);
                }
                registry_c_loop_owner = ClientRegistry::next(
                    registry_c_loop_owner
                        .as_ref()
                        .expect("current registry client"),
                );
                c_loop = registry_c_loop_owner.clone();
            }
        }
        if Eflag == 0 {
            let source = c_owner
                .as_ref()
                .expect("terminal client")
                .with_environment(|env| env.expect("client environment").clone());
            target
                .session_handle()
                .expect("target session")
                .update_environment(&source);
        }
        c.clone().expect("live client").set_session(s.as_ref());
        c.clone().expect("live client").set_key_table(None);
        if !c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
            c.as_ref().expect("live client").send_ready();
        }
        events_fire_client(c"client-attached", c.clone().expect("live client"));
        c.as_ref()
            .expect("live client")
            .update_flags(CLIENT_ATTACHED as uint64_t, 0);
    }
    if cfg_finished() {
        cfg_show_causes(s.as_ref());
    }
    CmdOutcome::Normal
}
fn cmd_attach_session_exec(self_0: &cmd, item_handle: &std::rc::Rc<cmdq_item>) -> CmdOutcome {
    let args = cmd_get_args(self_0);
    cmd_attach_session(
        item_handle,
        args_get(args, 't' as i32 as u_char),
        args_has(args, 'd' as i32 as u_char),
        args_has(args, 'x' as i32 as u_char),
        args_has(args, 'r' as i32 as u_char),
        args_get(args, 'c' as i32 as u_char),
        args_has(args, 'E' as i32 as u_char),
        args_get(args, 'f' as i32 as u_char),
    )
}
