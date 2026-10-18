use crate::src::arguments::{args, args_count, args_has, args_parse, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_target_client};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_cstring;
use crate::src::server_acl::{
    server_acl_allow, server_acl_allow_write, server_acl_deny, server_acl_deny_write,
    server_acl_display, server_acl_find,
};
use crate::src::shared::abi::id_t;
use crate::src::shared::abi::*;
use crate::src::shared::account::{group, passwd};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::CMD_CLIENT_CANFAIL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::server_acl::SERVER_ACL_IS_GROUP;
use libc::{getgrnam, getpwnam, getuid};
use std::ffi::CStr;
pub static cmd_server_access_entry: cmd_entry = {
    cmd_entry {
        name: c"server-access",
        alias: None,
        args: args_parse {
            template: c"adglrw",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-adglrw] [-t target-pane] [user|group]",
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
        flags: CMD_CLIENT_CANFAIL,
        exec: Some(cmd_server_access_exec),
    }
};
unsafe fn cmd_server_access_deny(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut id: id_t,
    mut flags: ::core::ffi::c_int,
    type_0: &CStr,
    name: &CStr,
) -> cmd_retval {
    if server_acl_find(id, flags) == 0 {
        cmdq_error(item_handle, |out| {
            write_cstr(out, nullable_cstr(type_0.as_ptr()))?;
            out.write_all(b" ")?;
            write_cstr(out, nullable_cstr(name.as_ptr()))?;
            out.write_all(b" not found")
        });
        return CMD_RETURN_ERROR;
    }
    server_acl_deny(id, flags);
    CMD_RETURN_NORMAL
}
unsafe fn cmd_server_access_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let c_owner = cmdq_get_target_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut name: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut type_0: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut pw: *mut passwd = ::core::ptr::null_mut::<passwd>();
    let mut gr: *mut group = ::core::ptr::null_mut::<group>();
    let mut id: id_t = 0;
    let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if args_has(args, 'l' as i32 as u_char) != 0 {
        server_acl_display(item_handle);
        return CMD_RETURN_NORMAL;
    }
    if args_count(args) == 0 as u_int {
        cmdq_error(item_handle, |out| {
            out.write_all(b"missing user or group argument")
        });
        return CMD_RETURN_ERROR;
    }
    let arg = format_single_cstring(
        Some(item_handle),
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
        c.as_ref(),
        None,
        (refbox::Weak::new()).clone(),
        None,
    );
    if args_has(args, 'g' as i32 as u_char) != 0 {
        type_0 = c"group".as_ptr();
        gr = getgrnam(arg.as_ptr());
        if !gr.is_null() {
            id = (*gr).gr_gid as id_t;
            name = (*gr).gr_name;
            flags |= SERVER_ACL_IS_GROUP;
        }
    } else {
        type_0 = c"user".as_ptr();
        pw = getpwnam(arg.as_ptr());
        if !pw.is_null() {
            id = (*pw).pw_uid as id_t;
            name = (*pw).pw_name;
        }
    }
    if name.is_null() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"unknown ")?;
            write_cstr(out, nullable_cstr(type_0))?;
            out.write_all(b": ")?;
            write_cstr(out, &*arg)
        });
        return CMD_RETURN_ERROR;
    }
    if !flags & SERVER_ACL_IS_GROUP != 0 && (id == 0 as id_t || id == getuid()) {
        cmdq_error(item_handle, |out| {
            write_cstr(out, nullable_cstr(name))?;
            out.write_all(b" owns the server, can't change access")
        });
        return CMD_RETURN_ERROR;
    }
    if args_has(args, 'a' as i32 as u_char) != 0 && args_has(args, 'd' as i32 as u_char) != 0 {
        cmdq_error(item_handle, |out| {
            out.write_all(b"-a and -d cannot be used together")
        });
        return CMD_RETURN_ERROR;
    }
    if args_has(args, 'w' as i32 as u_char) != 0 && args_has(args, 'r' as i32 as u_char) != 0 {
        cmdq_error(item_handle, |out| {
            out.write_all(b"-r and -w cannot be used together")
        });
        return CMD_RETURN_ERROR;
    }
    if args_has(args, 'd' as i32 as u_char) != 0 {
        return cmd_server_access_deny(
            item_handle,
            id,
            flags,
            CStr::from_ptr(type_0),
            CStr::from_ptr(name),
        );
    }
    if args_has(args, 'a' as i32 as u_char) != 0 {
        if server_acl_find(id, flags) != 0 {
            cmdq_error(item_handle, |out| {
                write_cstr(out, nullable_cstr(type_0))?;
                out.write_all(b" ")?;
                write_cstr(out, nullable_cstr(name))?;
                out.write_all(b" is already added")
            });
            return CMD_RETURN_ERROR;
        }
        server_acl_allow(id, flags);
    } else if (args_has(args, 'r' as i32 as u_char) != 0
        || args_has(args, 'w' as i32 as u_char) != 0)
        && server_acl_find(id, flags) == 0
    {
        server_acl_allow(id, flags);
    }
    if args_has(args, 'w' as i32 as u_char) != 0 {
        if server_acl_find(id, flags) == 0 {
            cmdq_error(item_handle, |out| {
                write_cstr(out, nullable_cstr(type_0))?;
                out.write_all(b" ")?;
                write_cstr(out, nullable_cstr(name))?;
                out.write_all(b" not found")
            });
            return CMD_RETURN_ERROR;
        }
        server_acl_allow_write(id, flags);
        return CMD_RETURN_NORMAL;
    }
    if args_has(args, 'r' as i32 as u_char) != 0 {
        if server_acl_find(id, flags) == 0 {
            cmdq_error(item_handle, |out| {
                write_cstr(out, nullable_cstr(type_0))?;
                out.write_all(b" ")?;
                write_cstr(out, nullable_cstr(name))?;
                out.write_all(b" not found")
            });
            return CMD_RETURN_ERROR;
        }
        server_acl_deny_write(id, flags);
        return CMD_RETURN_NORMAL;
    }
    CMD_RETURN_NORMAL
}
