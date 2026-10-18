use crate::src::arguments::{args_get, args_has, args_parse};
use crate::src::cmd::queue::{
    cmdq_error, cmdq_get_client, cmdq_get_source, cmdq_get_target_client,
};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::compat::imsg::*;
use crate::src::server_client::Client;
use crate::src::server_client::ClientRegistry;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::command::{
    CMD_CLIENT_TFLAG, CMD_FIND_CANFAIL, CMD_READONLY, CMD_TARGET_CLIENT_USAGE,
};
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
            flag: 's' as i32 as ::core::ffi::c_char,
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
unsafe fn cmd_detach_client_exec(
    mut command: refbox::Weak<cmd>,
    item: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let suspend = std::ptr::eq(
        cmd_get_entry(command.get_unchecked()),
        &cmd_suspend_client_entry,
    );
    let args = cmd_get_args_mut(command.get_mut_unchecked()).expect("command arguments");
    let source = cmdq_get_source(&*item.get());
    let client = cmdq_get_client(Some(&*item.get()));
    let target = cmdq_get_target_client(Some(&*item.get())).expect("target client");
    let exec = args_get(args, b'E').map(std::ffi::CStr::to_owned);
    if suspend {
        target.suspend();
        return CmdOutcome::Normal;
    }
    if client.as_ref().expect("command client").is_read_only()
        && (args_has(args, b's') != 0
            || args_has(args, b'a') != 0
            || !std::rc::Rc::ptr_eq(client.as_ref().unwrap(), &target))
    {
        cmdq_error(item, |out| out.write_all(b"client is read-only"));
        return CmdOutcome::Error;
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
            return CmdOutcome::Normal;
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
        return CmdOutcome::Stop;
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
        return CmdOutcome::Normal;
    }
    detach(&target);
    CmdOutcome::Normal
}
