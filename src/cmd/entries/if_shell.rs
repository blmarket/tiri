use crate::src::arguments::{
    args_count, args_has, args_make_commands, args_make_commands_now, args_make_commands_prepare,
    args_string,
};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::parse::cmd_parse_error_uppercase_first;
use crate::src::cmd::queue::{
    cmdq_append, cmdq_error, cmdq_get_client, cmdq_get_command, cmdq_get_target_client,
    cmdq_insert_after, cmdq_resumed, cmdq_suspend,
};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::job::job_run;

use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::args_command_state;
use crate::src::shared::arguments::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::CMD_FIND_CANFAIL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::job::JobCompletion;
use crate::src::shared::session::SessionRef;
use crate::src::status::status_message_set;
use std::cell::UnsafeCell;
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
fn cmd_if_shell_args_parse(
    _args: &mut args,
    idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    if idx == 1 as u_int || idx == 2 as u_int {
        return Ok(ARGS_PARSE_COMMANDS_OR_STRING);
    }
    Ok(ARGS_PARSE_STRING)
}
unsafe fn cmd_if_shell_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let mut queue_client_ptr: Option<ClientRef> = queue_client.clone();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let new_item_allocation;
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let _tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut count: u_int = args_count(args);
    let mut wait: ::core::ffi::c_int =
        (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let shellcmd = format_single_from_target_cstring(
        item_handle,
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
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
            return CMD_RETURN_NORMAL;
        };
        let Some(cmdlist) = cmdlist else {
            return CMD_RETURN_ERROR;
        };
        new_item_allocation = cmdq_get_command(&cmdlist, (*item).state.as_ref());
        cmdq_insert_after(item_handle, new_item_allocation);
        drop(cmdlist);
        return CMD_RETURN_NORMAL;
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
        cdata.client = cmdq_get_client((item).as_ref());
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
        return CMD_RETURN_ERROR;
    }
    // job_run does not dispatch callbacks before returning. The job takes the
    // record only after startup succeeds; dropping the callback also releases
    // it if the job is freed before completion.
    if wait == 0 {
        job.try_borrow_mut()
            .expect("newly registered job")
            .completecb = Some(Box::new(move |completion| unsafe {
            cmd_if_shell_callback(completion, &mut cdata, None);
        }));
        return CMD_RETURN_NORMAL;
    }
    let (done, completed) = tiri_rt::oneshot::channel();
    // A job freed before completion drops the sender unsent.
    job.try_borrow_mut()
        .expect("newly registered job")
        .completecb = Some(Box::new(move |completion| {
        let _ = done.send(completion);
    }));
    cmdq_suspend(
        item_handle,
        cmd_if_shell_complete(completed, cdata, Rc::downgrade(item_handle)),
    )
}
/// Run the callback body inside the job's completion, where tmux runs it. The
/// continuation owns the data, and with it the client (tmux's cdata). A job
/// freed without completing never resumes the item (tmux's cmd_if_shell_free):
/// release the data, and the client with it, then park.
async fn cmd_if_shell_complete(
    completed: tiri_rt::oneshot::Receiver<JobCompletion>,
    mut cdata: Box<cmd_if_shell_data>,
    item: Weak<UnsafeCell<cmdq_item>>,
) {
    let Some(completion) = completed.await else {
        drop(cdata);
        return std::future::pending().await;
    };
    unsafe { cmd_if_shell_callback(completion, &mut cdata, Some(&item)) };
}
// `item` is the polled item of a foreground if-shell, and None for -b.
unsafe fn cmd_if_shell_callback(
    completion: JobCompletion,
    cdata: &mut cmd_if_shell_data,
    item: Option<&Weak<UnsafeCell<cmdq_item>>>,
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
                    new_item_allocation = cmdq_get_command(&commands, (*item.get()).state.as_ref());
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
                (client).release();
            }
            drop(self.cmd_else.take());
            drop(self.cmd_if.take());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::cmd::core::cmd_list_new;
    use crate::src::cmd::queue::{cmdq_get_callback_owned, cmdq_next};
    use crate::src::server_client::queue_test_clients as clients;
    use crate::src::shared::client::CLIENT_DEAD;
    use std::cell::Cell;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    /// Foreground data whose if branch is an empty command list, without an
    /// else branch. With no if branch, the body panics if the job succeeded.
    fn data(client: &ClientRef, branch: bool) -> Box<cmd_if_shell_data> {
        let mut cmd_if = args_command_state::empty();
        cmd_if.cmdlist = branch.then(|| unsafe { cmd_list_new() });
        Box::new(cmd_if_shell_data {
            cmd_if: Some(Box::new(cmd_if)),
            cmd_else: None,
            client: Some(client.clone()),
        })
    }

    /// Queue an item whose continuation waits for a job completion on the
    /// returned sender, as the completecb that cmd_if_shell_exec installs.
    unsafe fn append_job_wait(
        owner: &ClientRef,
        cdata: Box<cmd_if_shell_data>,
    ) -> (
        Weak<UnsafeCell<cmdq_item>>,
        tiri_rt::oneshot::Sender<JobCompletion>,
    ) {
        let (done, completed) = tiri_rt::oneshot::channel();
        let item = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"if-shell",
                Some(Box::new(move |item| {
                    cmdq_suspend(
                        item,
                        cmd_if_shell_complete(completed, cdata, Rc::downgrade(item)),
                    )
                })),
            ),
        );
        cmdq_next(Some(owner));
        (item, done)
    }

    /// Queue a callback that records whether it ran.
    unsafe fn append_flag(owner: &ClientRef) -> Rc<Cell<bool>> {
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"flag",
                Some(Box::new(move |_| {
                    flag.set(true);
                    CMD_RETURN_NORMAL
                })),
            ),
        );
        ran
    }

    fn exited(code: i32) -> JobCompletion {
        JobCompletion {
            status: ExitStatus::from_raw(code << 8),
            output: Vec::new(),
        }
    }

    #[test]
    fn job_completion_runs_the_body_inside_the_send() {
        unsafe {
            for (code, branches) in [(0, 1), (1, 0)] {
                let owner = clients::with_queue();
                let (head, done) = append_job_wait(&owner, data(&owner, true));
                let ran = append_flag(&owner);
                assert_eq!(Rc::strong_count(&owner), 2, "the data keeps the client");
                assert!(done.send(exited(code)).is_ok());
                // The body ran and its data was released at the job's event;
                // removal and the chosen branch wait for cmdq_next.
                assert_eq!(Rc::strong_count(&owner), 1);
                assert!(head.upgrade().is_some() && !ran.get());
                assert_eq!(cmdq_next(Some(&owner)), branches + 1);
                assert!(head.upgrade().is_none());
                assert!(ran.get());
            }
        }
    }

    #[test]
    fn a_lost_client_is_freed_once_its_job_completes() {
        unsafe {
            let owner = clients::with_queue();
            let (head, done) = append_job_wait(&owner, data(&owner, true));
            owner.update_flags(CLIENT_DEAD as u64, 0);
            let client = Rc::downgrade(&owner);
            drop(owner);
            // The data keeps the client while the job runs (L7).
            assert!(client.upgrade().is_some());
            assert!(done.send(exited(0)).is_ok());
            assert!(client.upgrade().is_none());
            assert!(head.upgrade().is_none());
        }
    }

    #[test]
    fn a_job_freed_unfinished_releases_the_data_and_parks() {
        unsafe {
            let owner = clients::with_queue();
            let (head, done) = append_job_wait(&owner, data(&owner, false));
            let ran = append_flag(&owner);
            drop(done);
            assert_eq!(Rc::strong_count(&owner), 1, "the data is released");
            // The item never resumes and blocks its queue, as in tmux.
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(head.upgrade().is_some() && !ran.get());
            owner.update_flags(CLIENT_DEAD as u64, 0);
            drop(owner);
            assert!(head.upgrade().is_none());
        }
    }
}
