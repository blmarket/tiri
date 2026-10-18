use crate::src::arguments::{args, args_get, args_has, args_parse, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_print};
use crate::src::environ::{environ_find, environ_iter};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_FIND_CANFAIL};
use crate::src::shared::environment::ENVIRON_HIDDEN;
use crate::src::shared::environment::{environ, environ_entry};
use crate::src::tmux::global_environ;
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
            flag: 't' as i32 as ::core::ffi::c_char,
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
unsafe fn cmd_show_environment_print(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    envent: &environ_entry,
) {
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
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
unsafe fn cmd_show_environment_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);

    let mut envent: Option<&environ_entry> = None;
    let mut tflag: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut name: *const ::core::ffi::c_char =
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    tflag =
        args_get(&*(args), 't' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !tflag.is_null() && (*target).session_handle().is_none() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"no such session: ")?;
            write_cstr(out, nullable_cstr(tflag))
        });
        return CMD_RETURN_ERROR;
    }
    let env: environ = if args_has(args, 'g' as i32 as u_char) != 0 {
        global_environ.as_deref().expect("environment").clone()
    } else {
        if (*target).session_handle().is_none() {
            tflag = args_get(&*(args), 't' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr());
            if !tflag.is_null() {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"no such session: ")?;
                    write_cstr(out, nullable_cstr(tflag))
                });
            } else {
                cmdq_error(item_handle, |out| out.write_all(b"no current session"));
            }
            return CMD_RETURN_ERROR;
        }
        let session = (*target).session_handle().expect("target session");
        session.borrow_environment().expect("environment").clone()
    };
    if !name.is_null() {
        envent = environ_find(&env, CStr::from_ptr(name));
        if envent.is_none() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"unknown variable: ")?;
                write_cstr(out, nullable_cstr(name))
            });
            return CMD_RETURN_ERROR;
        }
        cmd_show_environment_print(self_0.clone(), item_handle, envent.unwrap());
        return CMD_RETURN_NORMAL;
    }
    for entry in environ_iter(&env) {
        let envent = entry;
        cmd_show_environment_print(self_0.clone(), item_handle, envent);
    }
    CMD_RETURN_NORMAL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_preserves_non_utf8_bytes_and_stops_at_first_nul() {
        let mut value = b"\xff$`\"\\\0ignored".to_vec();
        let mut env = crate::src::environ::environ_create();
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
