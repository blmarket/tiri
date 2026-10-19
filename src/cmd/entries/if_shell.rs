use crate::arguments::{
    args, args_command_state, args_count, args_has, args_make_commands, args_make_commands_now,
    args_make_commands_prepare, args_parse, args_parse_type, args_string, ArgsParseError,
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING,
};
use crate::cmd::cmd_get_args;
use crate::cmd::parse::cmd_parse_error_uppercase_first;
use crate::cmd::queue::{
    cmdq_append, cmdq_error, cmdq_get_client, cmdq_get_command, cmdq_get_target_client,
    cmdq_insert_after, cmdq_resumed,
};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::format_single_from_target_cstring;
use crate::job::job_run;

use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_FIND_CANFAIL;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::job::JobCompletion;
use crate::shared::session::SessionRef;
use crate::status::status_message_set;
use std::rc::{Rc, Weak};

pub struct cmd_if_shell_data {
    pub cmd_if: Option<Box<args_command_state>>,
    pub cmd_else: Option<Box<args_command_state>>,
    pub client: Option<ClientRef>,
}
pub static cmd_if_shell_entry: cmd_entry = {
    cmd_entry {
        name: c"if-shell",
        alias: Some(c"if"),
        args: args_parse {
            template: c"bFt:",
            lower: 2 as ::core::ffi::c_int,
            upper: 3 as ::core::ffi::c_int,
            cb: Some(cmd_if_shell_args_parse),
        },
        usage: c"[-bF] [-t target-pane] shell-command command [command]",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_if_shell_exec),
    }
};
fn cmd_if_shell_args_parse(_args: &args, idx: u_int) -> Result<args_parse_type, ArgsParseError> {
    if idx == 1 as u_int || idx == 2 as u_int {
        return Ok(ARGS_PARSE_COMMANDS_OR_STRING);
    }
    Ok(ARGS_PARSE_STRING)
}
unsafe fn cmd_if_shell_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let mut queue_client_ptr: Option<ClientRef> = queue_client.clone();
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let new_item_allocation;
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let _tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = target.session_handle();
    let mut count: u_int = args_count(args);
    let mut wait: ::core::ffi::c_int =
        (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let shellcmd = format_single_from_target_cstring(
        item_handle,
        args_string(args, 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
    );
    if args_has(args, 'F' as i32 as u_char) != 0 {
        let cmdlist = if *shellcmd.as_ptr() as ::core::ffi::c_int != '0' as i32
            && *shellcmd.as_ptr() as ::core::ffi::c_int != '\0' as i32
        {
            args_make_commands_now(
                self_0.clone(),
                item_handle,
                1 as u_int,
                0 as ::core::ffi::c_int,
            )
        } else if count == 3 as u_int {
            args_make_commands_now(
                self_0.clone(),
                item_handle,
                2 as u_int,
                0 as ::core::ffi::c_int,
            )
        } else {
            return CmdOutcome::Normal;
        };
        let Some(cmdlist) = cmdlist else {
            return CmdOutcome::Error;
        };
        new_item_allocation = cmdq_get_command(&cmdlist, (*item).state.as_ref());
        cmdq_insert_after(item_handle, new_item_allocation);
        drop(cmdlist);
        return CmdOutcome::Normal;
    }
    let mut cdata = Box::new(cmd_if_shell_data {
        cmd_if: None,
        cmd_else: None,
        client: None,
    });
    cdata.cmd_if = Some(args_make_commands_prepare(
        self_0.clone(),
        item_handle,
        1 as u_int,
        None,
        wait,
        0 as ::core::ffi::c_int,
    ));
    if count == 3 as u_int {
        cdata.cmd_else = Some(args_make_commands_prepare(
            self_0.clone(),
            item_handle,
            2 as u_int,
            None,
            wait,
            0 as ::core::ffi::c_int,
        ));
    }
    if wait != 0 {
        cdata.client = cmdq_get_client(Some(&*item));
    } else {
        cdata.client = tc_owner.clone();
    }
    let cwd = ClientRef::working_directory(queue_client_ptr.as_ref(), s.as_ref());
    let job = job_run(
        Some(shellcmd.as_c_str()),
        &Vec::new(),
        None,
        s.as_ref(),
        cwd.as_deref(),
        None,
        None,
        None,
        0 as ::core::ffi::c_int,
        -(1 as ::core::ffi::c_int),
        -(1 as ::core::ffi::c_int),
    );
    if job.is_empty() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"failed to run command: ")?;
            write_cstr(out, &*shellcmd)
        });
        return CmdOutcome::Error;
    }
    if wait == 0 {
        job.try_borrow_mut()
            .expect("newly registered job")
            .completecb = Some(Box::new(move |completion| unsafe {
            cmd_if_shell_callback(completion, &mut cdata, None);
        }));
        return CmdOutcome::Normal;
    }
    let (done, completed) = tiri_rt::oneshot::channel();
    job.try_borrow_mut()
        .expect("newly registered job")
        .completecb = Some(Box::new(move |completion| {
        let _ = done.send(completion);
    }));
    CmdOutcome::Suspend(Box::pin(cmd_if_shell_complete(
        completed,
        cdata,
        Rc::downgrade(item_handle),
    )))
}
/// Run the callback body inside the job's completion, where tmux runs it. The
/// continuation owns the data, and with it the client (tmux's cdata). A job
/// freed without completing never resumes the item (tmux's cmd_if_shell_free):
/// release the data, and the client with it, then park.
async fn cmd_if_shell_complete(
    completed: tiri_rt::oneshot::Receiver<JobCompletion>,
    mut cdata: Box<cmd_if_shell_data>,
    item: Weak<std::cell::RefCell<cmdq_item>>,
) {
    let Some(completion) = completed.await else {
        drop(cdata);
        return std::future::pending().await;
    };
    unsafe { cmd_if_shell_callback(completion, &mut cdata, Some(&item)) };
}
unsafe fn cmd_if_shell_callback(
    completion: JobCompletion,
    cdata: &mut cmd_if_shell_data,
    item: Option<&Weak<std::cell::RefCell<cmdq_item>>>,
) {
    let c = cdata.client.as_ref();
    let new_item_allocation;
    let state = if completion.status.success() {
        cdata.cmd_if.as_deref_mut()
    } else {
        cdata.cmd_else.as_deref_mut()
    };
    if let Some(state) = state {
        match args_make_commands(state, &Vec::new()) {
            Err(mut error) => {
                if item.is_none() {
                    cmd_parse_error_uppercase_first(&mut error);
                }
                let error_ptr = error
                    .as_ref()
                    .map_or(::core::ptr::null(), |cause| cause.as_ptr());
                if let Some(item) = item {
                    cmdq_error(&cmdq_resumed(item), |out| {
                        write_cstr(out, nullable_cstr(error_ptr))
                    });
                } else {
                    status_message_set(
                        c,
                        -(1 as ::core::ffi::c_int),
                        1 as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                        |out| write_cstr(out, nullable_cstr(error_ptr)),
                    );
                }
            }
            Ok(commands) => {
                if let Some(item) = item {
                    let item = cmdq_resumed(item);
                    new_item_allocation = cmdq_get_command(&commands, item.borrow().state.as_ref());
                    cmdq_insert_after(&item, new_item_allocation);
                } else {
                    new_item_allocation = cmdq_get_command(&commands, None);
                    cmdq_append(c, new_item_allocation);
                }
                drop(commands);
            }
        }
    }
}
impl Drop for cmd_if_shell_data {
    fn drop(&mut self) {
        {
            if let Some(client) = self.client.take() {
                drop(client);
            }
            drop(self.cmd_else.take());
            drop(self.cmd_if.take());
        }
    }
}
