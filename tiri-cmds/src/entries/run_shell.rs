use crate::arguments::{args_command_state, args_make_commands, args_make_commands_prepare};
use crate::cmd::cmd_get_args;
use crate::cmd::parse::cmd_parse_error_uppercase_first;
use crate::cmd::queue::{
    CmdqFollowup, cmdq_append, cmdq_error, cmdq_followup, cmdq_get_command, cmdq_resumed,
};
use tiri_core::actions::TaskOutcome as CmdOutcome;
use tiri_core::actions::queue::{
    client as cmdq_get_client, target_client as cmdq_get_target_client,
};
use tiri_core::arguments::{
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING, ArgsParseError, args, args_count, args_get,
    args_has, args_parse, args_parse_type, args_string,
};
use tiri_core::compat::strtod::strtod;
use tiri_core::format::bytes::format_message_with;
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{
    format_add, format_create_from_target, format_expand_cstring, format_single_from_target_cstring,
};
use tiri_core::job::job_run;
use tiri_core::reactor::{
    evbuffer_add, evbuffer_add_formatted, evbuffer_get_length, evbuffer_new, evbuffer_pullup,
    evbuffer_readln,
};
use tiri_core::winlink::winlink_window;

use tiri_core::server_client::Client as _;

use crate::cmd::queue::cmdq_item;
use crate::shared::command::CMD_FIND_CANFAIL;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::{CStr, CString};
use std::os::unix::process::ExitStatusExt;
use std::rc::{Rc, Weak};
use std::time::Duration;
use tiri_core::session::Session;
use tiri_core::shared::abi::{size_t, time_t, u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::event::Timer;
use tiri_core::shared::job::{JOB_NOWAIT, JOB_SHOWSTDERR, JobCompletion, job};
use tiri_core::shared::pane::window_pane;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::status::status_message_set;
use tiri_core::window::Window as _;
use tiri_core::window_copy::{window_copy_add, window_view_mode};
use tiri_core::window_pane::WindowPane as _;

pub struct cmd_run_shell_data {
    pub client: Option<ClientRef>,
    pub cmd: Option<CString>,
    pub state: Option<Box<args_command_state>>,
    pub cwd: CString,
    pub s: Option<SessionRef>,
    pub wp_id: ::core::ffi::c_int,
    pub flags: ::core::ffi::c_int,
}
pub static cmd_run_shell_entry: cmd_entry = {
    cmd_entry {
        name: c"run-shell",
        alias: Some(c"run"),
        args: args_parse {
            template: c"bd:Ct:Es:c:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: Some(
                cmd_run_shell_args_parse
            ),
        },
        usage: c"[-bCE] [-c start-directory] [-d delay] [-t target-pane] [shell-command [argument ...]]",
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
        exec: Some(cmd_run_shell_exec),
    }
};
fn cmd_run_shell_args_parse(args: &args, _idx: u_int) -> Result<args_parse_type, ArgsParseError> {
    if args_has(args, 'C' as i32 as u_char) != 0 {
        return Ok(ARGS_PARSE_COMMANDS_OR_STRING);
    }
    Ok(ARGS_PARSE_STRING)
}
fn cmd_run_shell_print(cdata: &cmd_run_shell_data, item: Option<&Weak<cmdq_item>>, msg: &CStr) {
    let mut pane = if cdata.wp_id != -1 {
        std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id(cdata.wp_id as u32)
    } else {
        None
    };
    if pane.is_none() {
        if let Some(item) = item {
            let client = cmdq_get_client(Some(&cmdq_resumed(item).borrow()));
            let mut evb = evbuffer_new();
            evbuffer_add_formatted(&mut evb, |out| write_cstr(out, msg));
            ClientRef::print_to(client.as_ref(), true, &mut evb);
            return;
        }
        pane = cdata
            .client
            .as_ref()
            .and_then(|client| client.attached_session().upgrade())
            .and_then(|session| {
                winlink_window(&session.current_winlink())
                    .expect("live window")
                    .active_pane()
                    .upgrade()
            });
        if pane.is_none() {
            if let Some(find) = cmd_find_state::from_nothing(0) {
                pane = find.pane_handle();
            }
        }
    }
    let Some(pane) = pane else {
        return;
    };
    if !pane.is_mode(&window_view_mode) {
        pane.set_mode(None, &window_view_mode, None, None, None);
    }
    window_copy_add(&pane, 1, |out| write_cstr(out, Some(&*msg)));
}
fn cmd_run_shell_status_message(cmd: &CStr, suffix: &[u8], code: ::core::ffi::c_int) -> CString {
    let cmd = cmd.to_bytes();
    let code = code.to_string();
    let mut message = Vec::with_capacity(1 + cmd.len() + suffix.len() + code.len());
    message.push(b'\'');
    message.extend_from_slice(cmd);
    message.extend_from_slice(suffix);
    message.extend_from_slice(code.as_bytes());
    CString::new(message).expect("run-shell command and status text contain no NUL")
}

fn cmd_run_shell_exec(self_0: &cmd, item_handle: &std::rc::Rc<cmdq_item>) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0);
    let target = tiri_core::actions::queue::target(&item);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let _tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = target.session_handle();
    let pane_owner = target.pane_handle();
    let mut tv = Duration::ZERO;
    let mut i: u_int = 0;
    let mut wait: ::core::ffi::c_int =
        (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let delay = match args_get(args, 'd' as i32 as u_char) {
        Some(delay) => {
            let (d, end) = strtod(delay);
            if !end.is_empty() {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"invalid delay time: ")?;
                    write_cstr(out, delay)
                });
                return CmdOutcome::Error;
            }
            Some(d)
        }
        None if args_count(args) == 0 as u_int => return CmdOutcome::Normal,
        None => None,
    };
    let cwd = if args_has(args, 'c' as i32 as u_char) != 0 {
        args_get(args, 'c' as i32 as u_char)
            .map(|cwd| format_single_from_target_cstring(item_handle, cwd))
    } else {
        ClientRef::working_directory(c.as_ref(), s.as_ref())
    };
    let mut cdata = Box::new(cmd_run_shell_data {
        client: None,
        cmd: None,
        state: None,
        cwd: cwd.expect("shell working directory"),
        s: None,
        wp_id: 0,
        flags: 0,
    });
    if args_has(args, 'C' as i32 as u_char) == 0 {
        if let Some(cmd) = args_string(args, 0 as u_int) {
            let mut ft_owner = format_create_from_target(item_handle);
            let ft = &mut *ft_owner;
            i = 1 as u_int;
            while i < args_count(args) {
                let key = format_message_with(|out| write!(out, "{}", i));
                format_add(ft, &key, |out| write_cstr(out, args_string(args, i)));
                i = i.wrapping_add(1);
            }
            cdata.cmd = Some(format_expand_cstring(ft, cmd));
            drop(ft_owner);
        }
    } else {
        cdata.state = Some(args_make_commands_prepare(
            self_0,
            item_handle,
            0 as u_int,
            None,
            wait,
            1 as ::core::ffi::c_int,
        ));
    }
    if let Some(pane) = pane_owner
        .as_ref()
        .filter(|_| args_has(args, 't' as i32 as u_char) != 0)
    {
        cdata.wp_id = pane.id() as ::core::ffi::c_int;
    } else {
        cdata.wp_id = -(1 as ::core::ffi::c_int);
    }
    if wait != 0 {
        cdata.client = c_owner.clone();
    } else {
        cdata.client = tc_owner.clone();
        cdata.flags |= JOB_NOWAIT;
    }
    if args_has(args, 'E' as i32 as u_char) != 0 {
        cdata.flags |= JOB_SHOWSTDERR;
    }
    cdata.s = if s.is_none() { None } else { s.clone() };
    if let Some(d) = delay {
        if (0.0..time_t::MAX as f64).contains(&d) {
            tv = Duration::from_secs(d as u64)
                + Duration::from_micros((d.fract() * 1_000_000.0) as u64);
        }
    }
    if wait == 0 {
        crate::cmd::queue::cmdq_background(tv, move || {
            if let RunShellTimer::Job(job) = cmd_run_shell_timer(&mut cdata, None) {
                job.try_borrow_mut()
                    .expect("newly registered job")
                    .completecb = Some(Box::new(move |completion| {
                    cmd_run_shell_callback(completion, &cdata, None)
                }));
            }
        })
        .expect("arm background run-shell delay");
        return CmdOutcome::Normal;
    }
    cmd_run_shell_wait(item_handle, cdata, tv)
}
enum RunShellTimer {
    None,
    Job(tiri_core::refbox::Weak<job>),
    Commands(CmdqFollowup),
}
/// Suspend a foreground run-shell on a continuation that owns its data, and
/// with it the client (tmux's cdata). The timer body runs inside the delay's
/// wake and the callback body inside the job's, where tmux runs them.
fn cmd_run_shell_wait(
    item_handle: &Rc<cmdq_item>,
    mut cdata: Box<cmd_run_shell_data>,
    delay: Duration,
) -> CmdOutcome {
    let item = Rc::downgrade(item_handle);
    let (elapsed, delayed) = tiri_core::tiri_rt::oneshot::channel();
    let timer = Timer::new(delay, move || {
        let _ = elapsed.send(());
    })
    .expect("arm run-shell delay");
    CmdOutcome::Suspend(Box::pin(async move {
        if delayed.await.is_none() {
            drop(cdata);
            return std::future::pending().await;
        }
        drop(timer);
        match cmd_run_shell_timer(&mut cdata, Some(&item)) {
            RunShellTimer::None => {}
            RunShellTimer::Commands(followup) => {
                drop(cdata);
                followup.run().await;
            }
            RunShellTimer::Job(job) => {
                let (done, completed) = tiri_core::tiri_rt::oneshot::channel();
                job.try_borrow_mut()
                    .expect("newly registered job")
                    .completecb = Some(Box::new(move |completion| {
                    let _ = done.send(completion);
                }));
                drop(job);
                cmd_run_shell_complete(completed, cdata, item).await
            }
        }
    }))
}
/// Run the callback body once the job completes. A job freed without
/// completing never resumes the item (tmux's cmd_run_shell_free): release the
/// data, and the client with it, then park.
async fn cmd_run_shell_complete(
    completed: tiri_core::tiri_rt::oneshot::Receiver<JobCompletion>,
    cdata: Box<cmd_run_shell_data>,
    item: Weak<cmdq_item>,
) {
    let Some(completion) = completed.await else {
        drop(cdata);
        return std::future::pending().await;
    };
    cmd_run_shell_callback(completion, &cdata, Some(&item));
}
fn cmd_run_shell_timer(
    cdata: &mut cmd_run_shell_data,
    item: Option<&Weak<cmdq_item>>,
) -> RunShellTimer {
    let c = cdata.client.as_ref();
    let cmd = cdata.cmd.as_deref();
    let new_item_allocation;
    if cdata.state.is_none() {
        let Some(cmd) = cmd else {
            return RunShellTimer::None;
        };
        let job = job_run(
            Some(cmd),
            &Vec::new(),
            None,
            cdata.s.as_ref(),
            Some(cdata.cwd.as_c_str()),
            None,
            None,
            None,
            cdata.flags,
            -(1 as ::core::ffi::c_int),
            -(1 as ::core::ffi::c_int),
        );
        if !job.is_empty() {
            return RunShellTimer::Job(job);
        }
        if let Some(item) = item {
            cmdq_error(&cmdq_resumed(item), |out| {
                out.write_all(b"failed to run command: ")?;
                write_cstr(out, cmd)
            });
        } else {
            status_message_set(
                c,
                -(1 as ::core::ffi::c_int),
                1 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                0 as ::core::ffi::c_int,
                |out| {
                    out.write_all(b"failed to run command: ")?;
                    write_cstr(out, cmd)
                },
            );
        }
        return RunShellTimer::None;
    }
    match args_make_commands(
        cdata.state.as_deref_mut().expect("prepared command state"),
        &Vec::new(),
    ) {
        Err(mut error) => {
            if item.is_none() {
                cmd_parse_error_uppercase_first(&mut error);
            }
            let error = error.as_deref();
            if let Some(item) = item {
                cmdq_error(&cmdq_resumed(item), |out| write_cstr(out, error));
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
            if let Some(item) = item {
                let item = cmdq_resumed(item);
                let mut followup = cmdq_followup(&item);
                followup.push(cmdq_get_command(&commands, item.borrow().state.as_ref()));
                drop(commands);
                return RunShellTimer::Commands(followup);
            }
            new_item_allocation = cmdq_get_command(&commands, None);
            cmdq_append(c, new_item_allocation);
            drop(commands);
        }
    }
    RunShellTimer::None
}
fn cmd_run_shell_callback(
    completion: JobCompletion,
    cdata: &cmd_run_shell_data,
    item: Option<&Weak<cmdq_item>>,
) {
    let mut event = evbuffer_new();
    if !completion.output.is_empty() {
        evbuffer_add(&mut event, &completion.output);
    }
    let cmd = cdata
        .cmd
        .as_ref()
        .expect("run-shell callback requires a command")
        .as_c_str();
    let mut msg: Option<CString> = None;
    let mut size: size_t = 0;
    let mut retcode: ::core::ffi::c_int = 0;
    while let Some(line) = evbuffer_readln(&mut event) {
        cmd_run_shell_print(
            cdata,
            item,
            CStr::from_bytes_until_nul(&line).expect("read line is NUL terminated"),
        );
    }
    size = evbuffer_get_length(&event);
    if size != 0 as size_t {
        let mut partial_line = evbuffer_pullup(&mut event, -1).unwrap_or_default().to_vec();
        partial_line.push(0);
        cmd_run_shell_print(
            cdata,
            item,
            CStr::from_bytes_until_nul(&partial_line).expect("partial line is NUL terminated"),
        );
    }
    if let Some(code) = completion.status.code() {
        retcode = code;
        if retcode != 0 as ::core::ffi::c_int {
            msg = Some(cmd_run_shell_status_message(cmd, b"' returned ", retcode));
        }
    } else if let Some(signal) = completion.status.signal() {
        retcode = signal;
        msg = Some(cmd_run_shell_status_message(
            cmd,
            b"' terminated by signal ",
            retcode,
        ));
        retcode += 128 as ::core::ffi::c_int;
    }
    if let Some(msg) = msg.as_ref() {
        cmd_run_shell_print(cdata, item, msg);
    }
    if let Some(item) = item {
        let client = cmdq_get_client(Some(&cmdq_resumed(item).borrow()));
        if let Some(client) = client {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(retcode);
            }
        }
    }
}
impl Drop for cmd_run_shell_data {
    fn drop(&mut self) {
        if let Some(session) = self.s.take() {
            (session).release(c"cmd_run_shell_data::drop");
        }
        if let Some(client) = self.client.take() {
            drop(client);
        }
        drop(self.state.take());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CStr, ClientRef, Duration, ExitStatusExt, JobCompletion, Rc, Weak, cmd_run_shell_complete,
        cmd_run_shell_data, cmd_run_shell_wait, cmdq_item,
    };
    use crate::cmd::queue::{cmdq_append, cmdq_get_callback_owned, cmdq_next, cmdq_running};
    use crate::test_support::clients;
    use std::cell::Cell;
    use std::process::ExitStatus;
    use std::time::Instant;
    use tiri_core::actions::TaskOutcome as CmdOutcome;
    use tiri_core::server_client::Client as _;
    use tiri_core::server_client::queue_test_clients::return_value;
    use tiri_core::shared::client::CLIENT_DEAD;
    use tiri_core::tiri_rt::Runtime as _;

    /// Foreground data; the callback body panics on a missing command.
    fn data(client: Option<ClientRef>, cmd: Option<&CStr>) -> Box<cmd_run_shell_data> {
        Box::new(cmd_run_shell_data {
            client,
            cmd: cmd.map(CStr::to_owned),
            state: None,
            cwd: c"/".to_owned(),
            s: None,
            wp_id: -1,
            flags: 0,
        })
    }

    /// Queue an item that suspends on `suspend`, after `owner`'s other items.
    fn append(
        owner: Option<&ClientRef>,
        suspend: impl FnOnce(&Rc<cmdq_item>) -> CmdOutcome + 'static,
    ) -> Weak<cmdq_item> {
        cmdq_append(
            owner,
            cmdq_get_callback_owned(c"run-shell", Some(Box::new(suspend))),
        )
    }

    /// Queue a callback that records whether it ran.
    fn append_flag(owner: Option<&ClientRef>) -> Rc<Cell<bool>> {
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        append(owner, move |_| {
            flag.set(true);
            CmdOutcome::Normal
        });
        ran
    }

    /// Queue an item whose continuation waits for a job completion on the
    /// returned sender, as the completecb that cmd_run_shell_wait installs.
    fn append_job_wait(
        owner: &ClientRef,
        cdata: Box<cmd_run_shell_data>,
    ) -> (
        Weak<cmdq_item>,
        tiri_core::tiri_rt::oneshot::Sender<JobCompletion>,
    ) {
        let (done, completed) = tiri_core::tiri_rt::oneshot::channel();
        let item = append(Some(owner), move |item| {
            CmdOutcome::Suspend(Box::pin(cmd_run_shell_complete(
                completed,
                cdata,
                Rc::downgrade(item),
            )))
        });
        cmdq_next(Some(owner));
        (item, done)
    }

    fn exited(code: i32) -> JobCompletion {
        JobCompletion {
            status: ExitStatus::from_raw(code << 8),
            output: Vec::new(),
        }
    }

    #[test]
    fn a_zero_delay_runs_the_timer_body_one_runtime_turn_later() {
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let head = append(None, |item| {
                cmd_run_shell_wait(item, data(None, None), Duration::ZERO)
            });
            let ran = append_flag(None);
            assert_eq!(cmdq_next(None), 0, "the delay does not end in its dispatch");
            assert!(cmdq_running().upgrade().is_none(), "the head is suspended");
            runtime.poll(Some(Duration::ZERO)).unwrap();
            assert!(cmdq_running().ptr_eq(&head));
            assert!(!ran.get());
            assert_eq!(cmdq_next(None), 2);
            assert!(head.upgrade().is_none());
            assert!(ran.get());
        }
        tiri_core::reactor::shutdown_runtime(runtime);
    }

    #[test]
    fn a_delay_resumes_at_its_deadline() {
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let delay = Duration::from_millis(50);
            let start = Instant::now();
            let head = append(None, move |item| {
                cmd_run_shell_wait(item, data(None, None), delay)
            });
            assert_eq!(cmdq_next(None), 0);
            runtime.poll(Some(Duration::ZERO)).unwrap();
            assert!(cmdq_running().upgrade().is_none());
            while head.upgrade().is_some() {
                assert!(start.elapsed() < Duration::from_secs(5));
                runtime.poll(Some(Duration::from_millis(100))).unwrap();
                cmdq_next(None);
            }
            assert!(start.elapsed() >= delay);
            assert!(cmdq_running().upgrade().is_none());
            cmdq_next(None);
            assert!(head.upgrade().is_none());
        }
        tiri_core::reactor::shutdown_runtime(runtime);
    }

    #[test]
    fn dropping_the_item_during_the_delay_cancels_the_timer() {
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let owner = clients::with_queue();
            let other = clients::with_queue();
            let cdata = data(Some(other.clone()), Some(c"true"));
            let head = append(Some(&owner), move |item| {
                cmd_run_shell_wait(item, cdata, Duration::from_secs(3600))
            });
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_some());
            drop(owner);
            assert!(head.upgrade().is_none());
            assert_eq!(Rc::strong_count(&other), 1, "the data is released");
            let start = Instant::now();
            runtime.poll(Some(Duration::from_secs(2))).unwrap();
            assert!(start.elapsed() < Duration::from_secs(1));
        }
        tiri_core::reactor::shutdown_runtime(runtime);
    }

    #[test]
    fn job_completion_runs_the_callback_body_inside_the_send() {
        let owner = clients::with_queue();
        owner.set_return_value(7);
        let (head, done) = append_job_wait(&owner, data(Some(owner.clone()), Some(c"true")));
        let ran = append_flag(Some(&owner));
        assert!(head.upgrade().is_some());
        assert!(done.send(exited(0)).is_ok());
        assert_eq!(return_value(&owner), 0);
        assert!(head.upgrade().is_some() && !ran.get());
        assert_eq!(cmdq_next(Some(&owner)), 2);
        assert!(head.upgrade().is_none());
        assert!(ran.get());
    }

    #[test]
    fn a_lost_client_is_freed_once_its_job_completes() {
        let owner = clients::with_queue();
        let (head, done) = append_job_wait(&owner, data(Some(owner.clone()), Some(c"true")));
        owner.update_flags(CLIENT_DEAD as u64, 0);
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_some());
        assert!(done.send(exited(0)).is_ok());
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
    }

    #[test]
    fn a_job_freed_unfinished_releases_the_data_and_parks() {
        let owner = clients::with_queue();
        let (head, done) = append_job_wait(&owner, data(Some(owner.clone()), None));
        owner.update_flags(CLIENT_DEAD as u64, 0);
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_some());
        drop(done);
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
    }
}
