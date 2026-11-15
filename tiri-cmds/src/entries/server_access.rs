use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::CMD_CLIENT_CANFAIL;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::CStr;
use tiri_core::arguments::{args_count, args_has, args_parse, args_string};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::format_single_cstring;
use tiri_core::server_acl::{
    server_acl_allow, server_acl_allow_write, server_acl_deny, server_acl_deny_write,
    server_acl_display, server_acl_find,
};
use tiri_core::shared::abi::id_t;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::server_acl::SERVER_ACL_IS_GROUP;
use tiri_core::tiri_sys::{group_id_by_name, user_id, user_id_by_name};
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
fn cmd_server_access_deny(
    inv: &Invocation<'_>,
    mut id: id_t,
    mut flags: ::core::ffi::c_int,
    type_0: &CStr,
    name: &CStr,
) -> ExecOutcome {
    if server_acl_find(id, flags) == 0 {
        inv.error( |out| {
            write_cstr(out, Some(&*type_0))?;
            out.write_all(b" ")?;
            write_cstr(out, Some(&*name))?;
            out.write_all(b" not found")
        });
        return ExecOutcome::err();
    }
    server_acl_deny(id, flags);
    ExecOutcome::ok()
}
fn cmd_server_access_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let c_owner = inv.target_client();
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut name: Option<std::ffi::CString> = None;
    let mut type_0: &CStr = c"";
    let mut id: id_t = 0;
    let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if args_has(args, 'l' as i32 as u_char) != 0 {
        let queue_client = inv.client();
        server_acl_display(queue_client.as_ref());
        return ExecOutcome::ok();
    }
    if args_count(args) == 0 as u_int {
        inv.error( |out| {
            out.write_all(b"missing user or group argument")
        });
        return ExecOutcome::err();
    }
    let cf = inv.format();
    let arg = format_single_cstring(
        Some(&cf),
        args_string(args, 0 as u_int).unwrap_or(c""),
        c.as_ref(),
        None,
        (tiri_core::refbox::Weak::new()).clone(),
        None,
    );
    if args_has(args, 'g' as i32 as u_char) != 0 {
        type_0 = c"group";
        if let Some(gid) = group_id_by_name(&arg) {
            id = gid as id_t;
            name = Some(arg.clone());
            flags |= SERVER_ACL_IS_GROUP;
        }
    } else {
        type_0 = c"user";
        if let Some(uid) = user_id_by_name(&arg) {
            id = uid as id_t;
            name = Some(arg.clone());
        }
    }
    let Some(name) = name else {
        inv.error( |out| {
            out.write_all(b"unknown ")?;
            write_cstr(out, type_0)?;
            out.write_all(b": ")?;
            write_cstr(out, &*arg)
        });
        return ExecOutcome::err();
    };
    if !flags & SERVER_ACL_IS_GROUP != 0 && (id == 0 as id_t || id == user_id()) {
        inv.error( |out| {
            write_cstr(out, name.as_c_str())?;
            out.write_all(b" owns the server, can't change access")
        });
        return ExecOutcome::err();
    }
    if args_has(args, 'a' as i32 as u_char) != 0 && args_has(args, 'd' as i32 as u_char) != 0 {
        inv.error( |out| {
            out.write_all(b"-a and -d cannot be used together")
        });
        return ExecOutcome::err();
    }
    if args_has(args, 'w' as i32 as u_char) != 0 && args_has(args, 'r' as i32 as u_char) != 0 {
        inv.error( |out| {
            out.write_all(b"-r and -w cannot be used together")
        });
        return ExecOutcome::err();
    }
    if args_has(args, 'd' as i32 as u_char) != 0 {
        return cmd_server_access_deny(inv, id, flags, type_0, &name);
    }
    if args_has(args, 'a' as i32 as u_char) != 0 {
        if server_acl_find(id, flags) != 0 {
            inv.error( |out| {
                write_cstr(out, type_0)?;
                out.write_all(b" ")?;
                write_cstr(out, name.as_c_str())?;
                out.write_all(b" is already added")
            });
            return ExecOutcome::err();
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
            inv.error( |out| {
                write_cstr(out, type_0)?;
                out.write_all(b" ")?;
                write_cstr(out, name.as_c_str())?;
                out.write_all(b" not found")
            });
            return ExecOutcome::err();
        }
        server_acl_allow_write(id, flags);
        return ExecOutcome::ok();
    }
    if args_has(args, 'r' as i32 as u_char) != 0 {
        if server_acl_find(id, flags) == 0 {
            inv.error( |out| {
                write_cstr(out, type_0)?;
                out.write_all(b" ")?;
                write_cstr(out, name.as_c_str())?;
                out.write_all(b" not found")
            });
            return ExecOutcome::err();
        }
        server_acl_deny_write(id, flags);
        return ExecOutcome::ok();
    }
    ExecOutcome::ok()
})
}
