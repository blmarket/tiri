use crate::src::arguments::{args_count, args_get, args_has, args_string, args_strtonum_result};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::find::cmd_find_best_client;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_target_client, cmdq_print};
use crate::src::format::bytes::write_cstr;
use crate::src::format::{
    format_create_with_client, format_defaults, format_each, format_expand_time_cstring,
};
use crate::src::json::{json_parse, json_to_string};
use crate::src::reactor::{evbuffer_add_formatted, evbuffer_new};

use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_CONTROL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{
    CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_FIND_CANFAIL,
};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::{FORMAT_NONE, FORMAT_VERBOSE};
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::window::winlink;
use crate::src::status::status_message_set;
use crate::src::window_pane::WindowPane as _;
use std::ffi::{CStr, CString};

pub const DISPLAY_MESSAGE_TEMPLATE: &std::ffi::CStr = c"[#{session_name}] #{window_index}:#{window_name}, current pane #{pane_index} - (%H:%M %d-%b-%y)";
pub static cmd_display_message_entry: cmd_entry = {
    cmd_entry {
        name: c"display-message",
        alias: Some(c"display"),
        args: args_parse {
            template: c"aCc:d:jlINpt:F:v",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-aCIjlNpv] [-c target-client] [-d delay] [-F format] [-t target-pane] [message]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_CFLAG | CMD_CLIENT_CANFAIL,
        exec: Some(cmd_display_message_exec),
    }
};
unsafe fn cmd_display_message_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut c: Option<ClientRef> = None;
    let s = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let wp = (*target).pane_handle();
    let mut template: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cause: Option<CString> = None;
    let mut delay: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut flags: ::core::ffi::c_int = 0;
    let mut Nflag: ::core::ffi::c_int = args_has(args, 'N' as i32 as u_char);
    let mut Cflag: ::core::ffi::c_int = args_has(args, 'C' as i32 as u_char);
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut count: u_int = args_count(args);
    if args_has(args, 'I' as i32 as u_char) != 0 && args_has(args, 'j' as i32 as u_char) == 0 {
        if wp.is_none() {
            return CMD_RETURN_NORMAL;
        }
        match wp.as_ref().expect("target pane").start_input(item_handle) {
            Err(error) => {
                cmdq_error(item_handle, |out| write_cstr(out, &*error));
                return CMD_RETURN_ERROR;
            }
            Ok(1) => return CMD_RETURN_NORMAL,
            Ok(0) => return CMD_RETURN_WAIT,
            _ => {}
        }
    }
    if args_has(args, 'F' as i32 as u_char) != 0 && count != 0 as u_int {
        cmdq_error(item_handle, |out| {
            out.write_all(b"only one of -F or argument must be given")
        });
        return CMD_RETURN_ERROR;
    }
    if args_has(args, 'd' as i32 as u_char) != 0 {
        delay = match args_strtonum_result(
            args,
            'd' as i32 as u_char,
            0 as ::core::ffi::c_longlong,
            UINT_MAX as ::core::ffi::c_longlong,
        ) {
            Ok(value) => value as ::core::ffi::c_int,
            Err(error) => {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"delay ")?;
                    write_cstr(out, &*(error.message()))
                });
                return CMD_RETURN_ERROR;
            }
        };
    }
    if count != 0 as u_int {
        template =
            args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    } else {
        template = args_get(&*(args), 'F' as i32 as u_char)
            .map_or(std::ptr::null(), |value| value.as_ptr());
    }
    if args_has(args, 'j' as i32 as u_char) != 0 && template.is_null() {
        template = c"".as_ptr();
    } else if template.is_null() {
        template = DISPLAY_MESSAGE_TEMPLATE.as_ptr();
    }
    let best_client_owner;
    if !tc.is_none()
        && tc
            .as_ref()
            .expect("target client")
            .attached_session()
            .ptr_eq(
                &s.as_ref()
                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
            )
    {
        c = tc.clone();
    } else if s.is_some() {
        best_client_owner = cmd_find_best_client(s.as_ref().unwrap());
        c = best_client_owner.clone();
    } else {
        c = None;
    }
    if args_has(args, 'v' as i32 as u_char) != 0 {
        flags = FORMAT_VERBOSE;
    } else {
        flags = 0 as ::core::ffi::c_int;
    }
    let mut ft_owner =
        format_create_with_client(queue_client.as_ref(), Some(item_handle), FORMAT_NONE, flags);
    ft = &raw mut *ft_owner;
    format_defaults(ft, c.as_ref(), s.as_ref(), wl.clone(), wp.as_ref());
    if args_has(args, 'a' as i32 as u_char) != 0 && args_has(args, 'j' as i32 as u_char) == 0 {
        format_each(ft, |key, value| unsafe {
            cmdq_print(item_handle, |out| {
                write_cstr(out, &*key)?;
                out.write_all(b"=")?;
                write_cstr(out, &*value)
            });
        });
        drop(ft_owner);
        return CMD_RETURN_NORMAL;
    }
    let mut msg = if args_has(args, 'l' as i32 as u_char) != 0 {
        CStr::from_ptr(template).to_owned()
    } else {
        format_expand_time_cstring(ft, template)
    };
    if args_has(args, 'j' as i32 as u_char) != 0 {
        let Some(jn) = json_parse(&msg, Some(&mut cause)) else {
            cmdq_error(item_handle, |out| write_cstr(out, cause.as_deref()));
            drop(msg);
            drop(ft_owner);
            return CMD_RETURN_ERROR;
        };
        msg = json_to_string(&jn);
    }
    if cmdq_get_client((item).as_ref()).is_none() {
        cmdq_error(item_handle, |out| write_cstr(out, &*msg));
    } else if args_has(args, 'p' as i32 as u_char) != 0 {
        cmdq_print(item_handle, |out| write_cstr(out, &*msg));
    } else if !tc.is_none()
        && tc.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
    {
        let mut evb = evbuffer_new();
        evbuffer_add_formatted(&mut evb, |out| {
            out.write_all(b"%message ")?;
            write_cstr(out, &*msg)
        });
        ClientRef::print_to(tc_owner.as_ref(), (0 as ::core::ffi::c_int) != 0, &mut evb);
    } else if !tc.is_none() {
        status_message_set(
            tc.as_ref(),
            delay,
            0 as ::core::ffi::c_int,
            Nflag,
            Cflag,
            |out| write_cstr(out, &*msg),
        );
    }
    drop(msg);
    drop(ft_owner);
    CMD_RETURN_NORMAL
}
