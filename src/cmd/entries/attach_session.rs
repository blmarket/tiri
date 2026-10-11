use crate::src::arguments::{args_get, args_has};
use crate::src::cfg::{cfg_finished, cfg_show_causes};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_flags, cmdq_get_state_owned};
use crate::src::compat::imsg::*;
use crate::src::events::events_fire_client;
use crate::src::ffi::libc::{getuid, strcspn};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_cstring;
use crate::src::server::clients;
use crate::src::session::SessionIndex as _;
use crate::src::window::Window as _;
use std::ffi::CStr;

use crate::src::session::sessions;
use crate::src::shared::abi::uid_t;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::{
    CLIENT_ATTACHED, CLIENT_CONTROL, CLIENT_IGNORESIZE, CLIENT_READONLY,
};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{
    CMDQ_STATE_REPEAT, CMD_FIND_PREFER_UNATTACHED, CMD_READONLY, CMD_STARTSERVER,
};
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::winlink;

use crate::src::window_pane::WindowPane as _;
use crate::src::{server_client::Client, session::Session};
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
pub unsafe fn cmd_attach_session(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    tflag: Option<&CStr>,
    mut dflag: ::core::ffi::c_int,
    mut xflag: ::core::ffi::c_int,
    mut rflag: ::core::ffi::c_int,
    cflag: Option<&CStr>,
    mut Eflag: ::core::ffi::c_int,
    fflag: Option<&CStr>,
) -> cmd_retval {
    let fflag: *const ::core::ffi::c_char = fflag.map_or(std::ptr::null(), CStr::as_ptr);
    let cflag: *const ::core::ffi::c_char = cflag.map_or(std::ptr::null(), CStr::as_ptr);
    let tflag: *const ::core::ffi::c_char = tflag.map_or(std::ptr::null(), CStr::as_ptr);
    let item = item_handle.get();
    let current = cmdq_get_state_owned(&*(item));
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
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut c_loop: Option<ClientRef> = None;
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wp = None;
    let mut msgtype: msgtype = 0 as msgtype;
    let mut uid: uid_t = 0;
    if !sessions.has_entries() {
        cmdq_error(item_handle, |out| out.write_all(b"no sessions"));
        return CMD_RETURN_ERROR;
    }
    if c.is_none() {
        return CMD_RETURN_NORMAL;
    }
    if c.as_ref().expect("live client").is_nested() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"sessions should be nested with care, unset $TMUX to force")
        });
        return CMD_RETURN_ERROR;
    }
    if !tflag.is_null()
        && *tflag.offset(strcspn(tflag, c":.".as_ptr()) as isize) as ::core::ffi::c_int
            != '\0' as i32
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
            *current.current.borrow_mut() = cmd_find_state::find_from_winlink_pane(
                wl.clone(),
                wp_value,
                0 as ::core::ffi::c_int,
            );
        } else {
            *current.current.borrow_mut() =
                cmd_find_state::find_from_winlink(wl.clone(), 0 as ::core::ffi::c_int);
        }
    }
    if !cflag.is_null() {
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
    if !fflag.is_null() {
        c.clone()
            .expect("live client")
            .parse_flags(std::ffi::CStr::from_ptr(fflag));
    }
    if rflag != 0 {
        if c.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0 {
            uid = c.as_ref().expect("live client").peer_uid();
            if uid != getuid() {
                cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
                return CMD_RETURN_ERROR;
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
            let mut registry_c_loop_owner = clients.first();
            c_loop = registry_c_loop_owner.clone();
            while !c_loop.is_none() {
                if !(!crate::src::shared::rc::same(
                    c_loop
                        .as_ref()
                        .expect("live client")
                        .attached_session()
                        .upgrade()
                        .as_ref(),
                    s.as_ref(),
                ) || crate::src::shared::rc::same(c.as_ref(), c_loop.as_ref()))
                {
                    c_loop.clone().expect("live client").detach(msgtype);
                }
                registry_c_loop_owner = clients.next(
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
        if !cmdq_get_flags(&*(item)) & CMDQ_STATE_REPEAT != 0 {
            c.clone().expect("live client").set_key_table(None);
        }
    } else {
        if let Err(cause) = (c_owner.as_ref().expect("terminal client")).open_terminal() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"open terminal failed: ")?;
                write_cstr(out, &*cause)
            });
            return CMD_RETURN_ERROR;
        }
        if dflag != 0 || xflag != 0 {
            if xflag != 0 {
                msgtype = MSG_DETACHKILL;
            } else {
                msgtype = MSG_DETACH;
            }
            let mut registry_c_loop_owner = clients.first();
            c_loop = registry_c_loop_owner.clone();
            while !c_loop.is_none() {
                if !(!crate::src::shared::rc::same(
                    c_loop
                        .as_ref()
                        .expect("live client")
                        .attached_session()
                        .upgrade()
                        .as_ref(),
                    s.as_ref(),
                ) || crate::src::shared::rc::same(c.as_ref(), c_loop.as_ref()))
                {
                    c_loop.clone().expect("live client").detach(msgtype);
                }
                registry_c_loop_owner = clients.next(
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
    if cfg_finished != 0 {
        cfg_show_causes(s.as_ref());
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_attach_session_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    cmd_attach_session(
        item_handle,
        nullable_cstr(
            args_get(&*(args), 't' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr()),
        ),
        args_has(args, 'd' as i32 as u_char),
        args_has(args, 'x' as i32 as u_char),
        args_has(args, 'r' as i32 as u_char),
        nullable_cstr(
            args_get(&*(args), 'c' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr()),
        ),
        args_has(args, 'E' as i32 as u_char),
        nullable_cstr(
            args_get(&*(args), 'f' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr()),
        ),
    )
}
