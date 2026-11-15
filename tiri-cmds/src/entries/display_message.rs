use crate::cmd::cmd_get_args;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{
    CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_FIND_CANFAIL,
};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::pin::Pin;
use tiri_core::arguments::{
    args_count, args_get, args_has, args_parse, args_string, args_strtonum_result,
};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{format_create, format_defaults, format_each, format_expand_time_cstring};
use tiri_core::json::{json_parse, json_to_string};
use tiri_core::reactor::{evbuffer_add_formatted, evbuffer_new};
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int, uint64_t};
use tiri_core::shared::client::CLIENT_CONTROL;
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::format::{FORMAT_NONE, FORMAT_VERBOSE};
use tiri_core::shared::limits::UINT_MAX;
use tiri_core::shared::target::best_client as cmd_find_best_client;
use tiri_core::shared::window::winlink;
use tiri_core::status::status_message_set;
use tiri_core::window_pane::WindowPane as _;

pub const DISPLAY_MESSAGE_TEMPLATE: &std::ffi::CStr =
    c"[#{session_name}] #{window_index}:#{window_name}, current pane #{pane_index} - (%H:%M %d-%b-%y)";

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
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_CFLAG | CMD_CLIENT_CANFAIL,
        exec: Some(cmd_display_message_exec),
    }
};

fn cmd_display_message_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let queue_client = inv.client();
        let args = cmd_get_args(self_0);
        let target = inv.target();
        let tc_owner = inv.target_client();
        let tc: Option<ClientRef> = tc_owner.clone();
        let c: Option<ClientRef>;
        let s = target.session_handle();
        let wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
        let wp = target.pane_handle();
        let mut cause: Option<CString> = None;
        let mut delay: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
        let flags: ::core::ffi::c_int;
        let Nflag: ::core::ffi::c_int = args_has(args, 'N' as i32 as u_char);
        let Cflag: ::core::ffi::c_int = args_has(args, 'C' as i32 as u_char);
        let count: u_int = args_count(args);
        if args_has(args, 'I' as i32 as u_char) != 0 && args_has(args, 'j' as i32 as u_char) == 0 {
            let Some(wp) = wp else {
                return ExecOutcome::ok();
            };
            let input = match wp.start_input(queue_client.as_ref()) {
                Err(error) => {
                    inv.error(|out| write_cstr(out, &*error));
                    return ExecOutcome::err();
                }
                Ok(None) => return ExecOutcome::ok(),
                Ok(Some(input)) => input,
            };
            drop(wp);
            drop(s);
            drop(tc);
            drop(tc_owner);
            drop(queue_client);
            inv.suspend(self_0);
            input.await;
            return ExecOutcome::ok();
        }
        if args_has(args, 'F' as i32 as u_char) != 0 && count != 0 as u_int {
            inv.error(|out| out.write_all(b"only one of -F or argument must be given"));
            return ExecOutcome::err();
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
                    inv.error(|out| {
                        out.write_all(b"delay ")?;
                        write_cstr(out, error.message())
                    });
                    return ExecOutcome::err();
                }
            };
        }
        let template = if count != 0 as u_int {
            args_string(args, 0 as u_int)
        } else {
            args_get(args, 'F' as i32 as u_char)
        };
        let template: &CStr = match template {
            Some(template) => template,
            None if args_has(args, 'j' as i32 as u_char) != 0 => c"",
            None => DISPLAY_MESSAGE_TEMPLATE,
        };
        if let Some(tc_ref) = tc.as_ref().filter(|tc_ref| {
            tc_ref.attached_session().ptr_eq(
                &s.as_ref()
                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
            )
        }) {
            c = Some(tc_ref.clone());
        } else if let Some(s_ref) = s.as_ref() {
            c = cmd_find_best_client(s_ref);
        } else {
            c = None;
        }
        if args_has(args, 'v' as i32 as u_char) != 0 {
            flags = FORMAT_VERBOSE;
        } else {
            flags = 0 as ::core::ffi::c_int;
        }
        let cf = inv.format();
        let mut ft_owner = format_create(queue_client.as_ref(), Some(&cf), FORMAT_NONE, flags);
        let ft = &mut *ft_owner;
        format_defaults(ft, c.as_ref(), s.as_ref(), wl, wp.as_ref());
        if args_has(args, 'a' as i32 as u_char) != 0 && args_has(args, 'j' as i32 as u_char) == 0 {
            format_each(ft, |key, value| {
                inv.print(|out| {
                    write_cstr(out, &*key)?;
                    out.write_all(b"=")?;
                    write_cstr(out, &*value)
                });
            });
            drop(ft_owner);
            return ExecOutcome::ok();
        }
        let mut msg = if args_has(args, 'l' as i32 as u_char) != 0 {
            template.to_owned()
        } else {
            format_expand_time_cstring(ft, template)
        };
        if args_has(args, 'j' as i32 as u_char) != 0 {
            let Some(jn) = json_parse(&msg, Some(&mut cause)) else {
                inv.error(|out| write_cstr(out, cause.as_deref()));
                drop(msg);
                drop(ft_owner);
                return ExecOutcome::err();
            };
            msg = json_to_string(&jn);
        }
        if inv.client().is_none() {
            inv.error(|out| write_cstr(out, &*msg));
        } else if args_has(args, 'p' as i32 as u_char) != 0 {
            inv.print(|out| write_cstr(out, &*msg));
        } else if tc
            .as_ref()
            .is_some_and(|tc_ref| tc_ref.flags() & CLIENT_CONTROL as uint64_t != 0)
        {
            let mut evb = evbuffer_new();
            evbuffer_add_formatted(&mut evb, |out| {
                out.write_all(b"%message ")?;
                write_cstr(out, &*msg)
            });
            ClientRef::print_to(tc_owner.as_ref(), false, &mut evb);
        } else if tc.is_some() {
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
        ExecOutcome::ok()
    })
}
