use crate::arguments::{args_get, args_has, args_parse, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_print};
use crate::environ::{environ_find, environ_iter};
use crate::format::bytes::write_cstr;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_CANFAIL};
use crate::shared::environment::ENVIRON_HIDDEN;
use crate::shared::environment::{environ, environ_entry};
use crate::tmux::global_environ;
use std::ffi::{CStr, CString};
pub static cmd_show_environment_entry: cmd_entry = {
    cmd_entry {
        name: c"show-environment",
        alias: Some(c"showenv"),
        args: args_parse {
            template: c"hgst:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-hgs] [-t target-session] [variable]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_SESSION,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_show_environment_exec),
    }
};
fn cmd_show_environment_escape(value: &CStr) -> CString {
    let value = value.to_bytes();
    let mut escaped = Vec::with_capacity(value.len().saturating_mul(2));
    for &byte in value {
        if matches!(byte, b'$' | b'`' | b'"' | b'\\') {
            escaped.push(b'\\');
        }
        escaped.push(byte);
    }
    CString::new(escaped).expect("environment value was truncated at its first NUL")
}
fn cmd_show_environment_print(
    self_0: &cmd,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    envent: &environ_entry,
) {
    let args = cmd_get_args(self_0);
    if args_has(args, 'h' as i32 as u_char) == 0 && envent.flags & ENVIRON_HIDDEN != 0 {
        return;
    }
    if args_has(args, 'h' as i32 as u_char) != 0 && !envent.flags & ENVIRON_HIDDEN != 0 {
        return;
    }
    if args_has(args, 's' as i32 as u_char) == 0 {
        if let Some(value) = envent.value.as_deref() {
            cmdq_print(item_handle, |out| {
                out.write_all(envent.name.as_bytes())?;
                out.write_all(b"=")?;
                out.write_all(value.to_bytes())
            });
        } else {
            cmdq_print(item_handle, |out| {
                out.write_all(b"-")?;
                out.write_all(envent.name.as_bytes())
            });
        }
        return;
    }
    if let Some(value) = envent.value.as_deref() {
        let escaped = cmd_show_environment_escape(value);
        cmdq_print(item_handle, |out| {
            out.write_all(envent.name.as_bytes())?;
            out.write_all(b"=\"")?;
            out.write_all(escaped.as_bytes())?;
            out.write_all(b"\"; export ")?;
            out.write_all(envent.name.as_bytes())?;
            out.write_all(b";")
        });
    } else {
        cmdq_print(item_handle, |out| {
            out.write_all(b"unset ")?;
            out.write_all(envent.name.as_bytes())?;
            out.write_all(b";")
        });
    };
}
fn cmd_show_environment_exec(
    self_0: &cmd,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0);
    let target = crate::cmd::queue::cmdq_get_target(&item);

    let mut envent: Option<&environ_entry> = None;
    let mut name: Option<&CStr> = args_string(args, 0 as u_int);
    let tflag = args_get(args, 't' as i32 as u_char);
    if tflag.is_some() && target.session_handle().is_none() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"no such session: ")?;
            write_cstr(out, tflag)
        });
        return CmdOutcome::Error;
    }
    let env: environ = if args_has(args, 'g' as i32 as u_char) != 0 {
        global_environ(Clone::clone)
    } else {
        if target.session_handle().is_none() {
            if tflag.is_some() {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"no such session: ")?;
                    write_cstr(out, tflag)
                });
            } else {
                cmdq_error(item_handle, |out| out.write_all(b"no current session"));
            }
            return CmdOutcome::Error;
        }
        let session = target.session_handle().expect("target session");
        session
            .with_environment_mut(|env| env.clone())
            .expect("environment")
    };
    if let Some(name) = name {
        envent = environ_find(&env, name);
        if envent.is_none() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"unknown variable: ")?;
                write_cstr(out, name)
            });
            return CmdOutcome::Error;
        }
        cmd_show_environment_print(self_0, item_handle, envent.unwrap());
        return CmdOutcome::Normal;
    }
    for entry in environ_iter(&env) {
        let envent = entry;
        cmd_show_environment_print(self_0, item_handle, envent);
    }
    CmdOutcome::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_preserves_non_utf8_bytes_and_stops_at_first_nul() {
        let mut value = b"\xff$`\"\\\0ignored".to_vec();
        let mut env = crate::environ::environ_create();
        env.set_cstr(
            c"",
            0,
            std::ffi::CStr::from_bytes_until_nul(&value).unwrap(),
        );
        let entry = env.find(c"").unwrap();
        let escaped = cmd_show_environment_escape(entry.value.as_deref().unwrap());
        assert_eq!(escaped.as_bytes(), b"\xff\\$\\`\\\"\\\\");
    }
}
