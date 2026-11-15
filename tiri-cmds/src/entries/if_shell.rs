use crate::arguments::{
    args_command_state, args_make_commands, args_make_commands_now, args_make_commands_prepare,
};
use crate::cmd::cmd_get_args;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::parse::cmd_parse_error_uppercase_first;
use crate::shared::command::CMD_FIND_CANFAIL;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_list};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use tiri_core::actions::Context;
use tiri_core::arguments::{
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING, ArgsParseError, args, args_count, args_has,
    args_parse, args_parse_type, args_string,
};
use tiri_core::format::bytes::write_cstr;
use tiri_core::job::job_run;
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::job::JobCompletion;
use tiri_core::shared::session::SessionRef;
use tiri_core::status::status_message_set;

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
            flag: b't',
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

fn cmd_if_shell_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let queue_client = inv.client();
        let args = cmd_get_args(self_0);
        let tc_owner = inv.target_client();
        let s: Option<SessionRef> = inv.target().session_handle();
        let count: u_int = args_count(args);
        let wait: ::core::ffi::c_int =
            (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
        let shellcmd = inv.format_single(args_string(args, 0 as u_int).unwrap_or(c""));
        if args_has(args, 'F' as i32 as u_char) != 0 {
            let cmdlist = if !matches!(shellcmd.as_bytes().first(), None | Some(b'0')) {
                args_make_commands_now(self_0, inv, 1 as u_int, 0 as ::core::ffi::c_int)
            } else if count == 3 as u_int {
                args_make_commands_now(self_0, inv, 2 as u_int, 0 as ::core::ffi::c_int)
            } else {
                return ExecOutcome::ok();
            };
            let Some(cmdlist) = cmdlist else {
                return ExecOutcome::err();
            };
            return ExecOutcome::commands(cmdlist);
        }
        let mut cdata = Box::new(cmd_if_shell_data {
            cmd_if: None,
            cmd_else: None,
            client: None,
        });
        cdata.cmd_if = Some(args_make_commands_prepare(
            self_0,
            inv,
            1 as u_int,
            None,
            wait,
            0 as ::core::ffi::c_int,
        ));
        if count == 3 as u_int {
            cdata.cmd_else = Some(args_make_commands_prepare(
                self_0,
                inv,
                2 as u_int,
                None,
                wait,
                0 as ::core::ffi::c_int,
            ));
        }
        if wait != 0 {
            cdata.client = queue_client.clone();
        } else {
            cdata.client = tc_owner.clone();
        }
        let cwd = ClientRef::working_directory(queue_client.as_ref(), s.as_ref());
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
            inv.error(|out| {
                out.write_all(b"failed to run command: ")?;
                write_cstr(out, &*shellcmd)
            });
            return ExecOutcome::err();
        }
        if wait == 0 {
            job.try_borrow_mut()
                .expect("newly registered job")
                .completecb = Some(Box::new(move |completion| {
                cmd_if_shell_callback(completion, &mut cdata, None);
            }));
            return ExecOutcome::ok();
        }
        let (done, completed) = tiri_core::tiri_rt::oneshot::channel();
        job.try_borrow_mut()
            .expect("newly registered job")
            .completecb = Some(Box::new(move |completion| {
            let _ = done.send(completion);
        }));
        drop(s);
        drop(tc_owner);
        drop(queue_client);
        inv.suspend(self_0);
        let Some(completion) = completed.await else {
            drop(cdata);
            return std::future::pending().await;
        };
        let followup = cmd_if_shell_callback(completion, &mut cdata, Some(inv));
        drop(cdata);
        match followup {
            Some(commands) => ExecOutcome::commands(commands),
            None => ExecOutcome::ok(),
        }
    })
}

fn cmd_if_shell_callback(
    completion: JobCompletion,
    cdata: &mut cmd_if_shell_data,
    inv: Option<&Invocation<'_>>,
) -> Option<Rc<cmd_list>> {
    let c = cdata.client.as_ref();
    let state = if completion.status.success() {
        cdata.cmd_if.as_deref_mut()
    } else {
        cdata.cmd_else.as_deref_mut()
    };
    if let Some(state) = state {
        match args_make_commands(state, &Vec::new()) {
            Err(mut error) => {
                if inv.is_none() {
                    cmd_parse_error_uppercase_first(&mut error);
                }
                let error = error.as_deref();
                if let Some(inv) = inv {
                    inv.error(|out| write_cstr(out, error));
                } else {
                    status_message_set(
                        c,
                        -(1 as ::core::ffi::c_int),
                        1 as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                        0 as ::core::ffi::c_int,
                        |out| write_cstr(out, error),
                    );
                }
            }
            Ok(commands) => {
                if inv.is_some() {
                    return Some(commands);
                }
                let client = c.map_or_else(std::rc::Weak::new, Rc::downgrade);
                let ctx = Context::new(client, None, None, 0);
                crate::cmd::run::spawn(c, commands, ctx);
            }
        }
    }
    None
}

impl Drop for cmd_if_shell_data {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            drop(client);
        }
        drop(self.cmd_else.take());
        drop(self.cmd_if.take());
    }
}
