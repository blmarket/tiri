use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::shared::command::{
    CMD_CLIENT_TFLAG, CMD_FIND_CANFAIL, CMD_READONLY, CMD_TARGET_CLIENT_USAGE,
};
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::compat::imsg::{MSG_DETACH, MSG_DETACHKILL};
use tiri_core::server_client::Client;
use tiri_core::server_client::ClientRegistry;
use tiri_core::shared::client::ClientRef;
pub static cmd_detach_client_entry: cmd_entry = {
    cmd_entry {
        name: c"detach-client",
        alias: Some(c"detach"),
        args: args_parse {
            template: c"aE:s:t:P",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-aP] [-E shell-command] [-s target-session] [-t target-client]",
        source: cmd_entry_flag {
            flag: b's',
            type_0: CMD_FIND_SESSION,
            flags: CMD_FIND_CANFAIL,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_READONLY | CMD_CLIENT_TFLAG,
        exec: Some(cmd_detach_client_exec),
    }
};
pub static cmd_suspend_client_entry: cmd_entry = {
    cmd_entry {
        name: c"suspend-client",
        alias: Some(c"suspendc"),
        args: args_parse {
            template: c"t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: CMD_TARGET_CLIENT_USAGE,
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
        flags: CMD_CLIENT_TFLAG,
        exec: Some(cmd_detach_client_exec),
    }
};
fn cmd_detach_client_exec<'a, 'ctx>(
    command: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let suspend = std::ptr::eq(cmd_get_entry(command), &cmd_suspend_client_entry);
    let args = cmd_get_args(command);
    let source = inv.source().clone();
    let client = inv.client();
    let target = inv.target_client().expect("target client");
    let exec = args_get(args, b'E').map(std::ffi::CStr::to_owned);
    if suspend {
        target.suspend();
        return ExecOutcome::ok();
    }
    if client.as_ref().expect("command client").is_read_only()
        && (args_has(args, b's') != 0
            || args_has(args, b'a') != 0
            || !std::rc::Rc::ptr_eq(client.as_ref().unwrap(), &target))
    {
        inv.error( |out| out.write_all(b"client is read-only"));
        return ExecOutcome::err();
    }
    let message = if args_has(args, b'P') != 0 {
        MSG_DETACHKILL
    } else {
        MSG_DETACH
    };
    let detach = |client: &ClientRef| {
        if let Some(command) = exec.as_deref() {
            client.exec(command);
        } else {
            client.detach(message);
        }
    };
    if args_has(args, b's') != 0 {
        let Some(session) = source.session_handle() else {
            return ExecOutcome::ok();
        };
        let observer = std::rc::Rc::downgrade(&session);
        drop(session);
        let mut cursor = ClientRegistry::first();
        while let Some(client) = cursor {
            if client.attached_session().ptr_eq(&observer) {
                detach(&client);
            }
            cursor = ClientRegistry::next(&client);
        }
        return ExecOutcome::ok();
    }
    if args_has(args, b'a') != 0 {
        let mut cursor = ClientRegistry::first();
        while let Some(client) = cursor {
            if client.attached_session().upgrade().is_some()
                && !std::rc::Rc::ptr_eq(&client, &target)
            {
                detach(&client);
            }
            cursor = ClientRegistry::next(&client);
        }
        return ExecOutcome::ok();
    }
    detach(&target);
    ExecOutcome::ok()
})
}
