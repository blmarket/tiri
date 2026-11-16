use crate::arguments::{args_command_state, args_make_commands, args_make_commands_prepare};
use crate::cmd::cmd_get_args;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::parse::cmd_parse_error_uppercase_first;
use crate::shared::command::CMD_FIND_CANFAIL;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_list};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::os::unix::process::ExitStatusExt;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::time::Duration;
use tiri_core::actions::Context;
use tiri_core::arguments::{
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING, ArgsParseError, args, args_count, args_get,
    args_has, args_parse, args_parse_type, args_string,
};
use tiri_core::compat::strtod::strtod;
use tiri_core::format::bytes::{format_message_with, write_cstr};
use tiri_core::format::{format_add, format_expand_cstring};
use tiri_core::job::job_run;
use tiri_core::server_client::Client as _;
use tiri_core::session::Session;
use tiri_core::shared::abi::{time_t, u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::event::Timer;
use tiri_core::shared::job::{JOB_NOWAIT, JOB_SHOWSTDERR, JobCompletion, job};
use tiri_core::tiri_buffer::{LineEnding, SegmentedBuf};
use tiri_core::shared::pane::window_pane;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::status::status_message_set;
use tiri_core::window::Window as _;
use tiri_core::window_copy::{window_copy_add, window_view_mode};
use tiri_core::window_pane::WindowPane as _;
use tiri_core::winlink::winlink_window;

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
            cb: Some(cmd_run_shell_args_parse),
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

fn cmd_run_shell_print(cdata: &cmd_run_shell_data, inv: Option<&Invocation<'_>>, msg: &CStr) {
    let mut pane = if cdata.wp_id != -1 {
        std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id(cdata.wp_id as u32)
    } else {
        None
    };
    if pane.is_none() {
        if let Some(inv) = inv {
            let mut evb = SegmentedBuf::from(msg.to_bytes().to_vec());
            inv.print_data(&mut evb);
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
        pane.set_mode(None, &window_view_mode, None, None);
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

fn cmd_run_shell_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let args = cmd_get_args(self_0);
        let c_owner = inv.client();
        let tc_owner = inv.target_client();
        let s: Option<SessionRef> = inv.target().session_handle();
        let pane_owner = inv.target().pane_handle();
        let mut tv = Duration::ZERO;
        let wait: ::core::ffi::c_int =
            (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
        let delay = match args_get(args, 'd' as i32 as u_char) {
            Some(delay) => {
                let (d, end) = strtod(delay);
                if !end.is_empty() {
                    if let Some(session) = s {
                        session.release(c"cmd_run_shell_exec");
                    }
                    inv.error(|out| {
                        out.write_all(b"invalid delay time: ")?;
                        write_cstr(out, delay)
                    });
                    return ExecOutcome::err();
                }
                Some(d)
            }
            None if args_count(args) == 0 as u_int => {
                if let Some(session) = s {
                    session.release(c"cmd_run_shell_exec");
                }
                return ExecOutcome::ok();
            }
            None => None,
        };
        let cwd = if args_has(args, 'c' as i32 as u_char) != 0 {
            args_get(args, 'c' as i32 as u_char).map(|cwd| inv.format_single(cwd))
        } else {
            ClientRef::working_directory(c_owner.as_ref(), s.as_ref())
        };
        let mut cdata = Box::new(cmd_run_shell_data {
            client: None,
            cmd: None,
            state: None,
            cwd: cwd.expect("shell working directory"),
            s,
            wp_id: 0,
            flags: 0,
        });
        if args_has(args, 'C' as i32 as u_char) == 0 {
            if let Some(cmd) = args_string(args, 0 as u_int) {
                let mut ft_owner = inv.format_from_target();
                let ft = &mut *ft_owner;
                let mut i: u_int = 1;
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
                inv,
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
        drop(pane_owner);
        drop(tc_owner);
        drop(c_owner);
        if let Some(d) = delay {
            if (0.0..time_t::MAX as f64).contains(&d) {
                tv = Duration::from_secs(d as u64)
                    + Duration::from_micros((d.fract() * 1_000_000.0) as u64);
            }
        }
        if wait == 0 {
            tiri_core::reactor::background_timer(tv, move || {
                if let RunShellTimer::Job(job) = cmd_run_shell_timer(&mut cdata, None) {
                    job.try_borrow_mut()
                        .expect("newly registered job")
                        .completecb = Some(Box::new(move |completion| {
                        cmd_run_shell_callback(completion, &cdata, None)
                    }));
                }
            })
            .expect("arm background run-shell delay");
            return ExecOutcome::ok();
        }
        cmd_run_shell_wait(self_0, inv, cdata, tv).await
    })
}

enum RunShellTimer {
    None,
    Job(tiri_core::refbox::Weak<job>),
    Commands(Rc<cmd_list>),
}

async fn cmd_run_shell_wait<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
    mut cdata: Box<cmd_run_shell_data>,
    delay: Duration,
) -> ExecOutcome {
    let (elapsed, delayed) = tiri_core::tiri_rt::oneshot::channel();
    let timer = Timer::new(delay, move || {
        let _ = elapsed.send(());
    })
    .expect("arm run-shell delay");
    inv.suspend(self_0);
    if delayed.await.is_none() {
        drop(cdata);
        return std::future::pending().await;
    }
    drop(timer);
    match cmd_run_shell_timer(&mut cdata, Some(inv)) {
        RunShellTimer::None => {
            drop(cdata);
            ExecOutcome::ok()
        }
        RunShellTimer::Commands(commands) => {
            drop(cdata);
            ExecOutcome::commands(commands)
        }
        RunShellTimer::Job(job) => {
            let (done, completed) = tiri_core::tiri_rt::oneshot::channel();
            job.try_borrow_mut()
                .expect("newly registered job")
                .completecb = Some(Box::new(move |completion| {
                let _ = done.send(completion);
            }));
            drop(job);
            cmd_run_shell_complete(inv, completed, cdata).await
        }
    }
}

async fn cmd_run_shell_complete<'a, 'ctx>(
    inv: &'a mut Invocation<'ctx>,
    completed: tiri_core::tiri_rt::oneshot::Receiver<JobCompletion>,
    cdata: Box<cmd_run_shell_data>,
) -> ExecOutcome {
    let Some(completion) = completed.await else {
        drop(cdata);
        return std::future::pending().await;
    };
    cmd_run_shell_callback(completion, &cdata, Some(inv));
    drop(cdata);
    ExecOutcome::ok()
}

fn cmd_run_shell_timer(
    cdata: &mut cmd_run_shell_data,
    inv: Option<&Invocation<'_>>,
) -> RunShellTimer {
    let c = cdata.client.as_ref();
    let cmd = cdata.cmd.as_deref();
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
        if let Some(inv) = inv {
            inv.error(|out| {
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
                return RunShellTimer::Commands(commands);
            }
            let client_weak = c.map_or_else(Weak::new, Rc::downgrade);
            crate::cmd::run::spawn(
                c,
                commands,
                Context::new(client_weak, None, None, 0),
            );
        }
    }
    RunShellTimer::None
}

fn cmd_run_shell_callback(
    completion: JobCompletion,
    cdata: &cmd_run_shell_data,
    inv: Option<&Invocation<'_>>,
) {
    let mut event = SegmentedBuf::from(completion.output);
    let cmd = cdata
        .cmd
        .as_ref()
        .expect("run-shell callback requires a command")
        .as_c_str();
    let mut msg: Option<CString> = None;
    let mut retcode: ::core::ffi::c_int = 0;
    while let Some(mut line) = event.read_line(LineEnding::Lf) {
        line.push(0);
        cmd_run_shell_print(
            cdata,
            inv,
            CStr::from_bytes_until_nul(&line).expect("read line is NUL terminated"),
        );
    }
    if !event.is_empty() {
        let mut partial_line = event.make_contiguous().to_vec();
        partial_line.push(0);
        cmd_run_shell_print(
            cdata,
            inv,
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
        cmd_run_shell_print(cdata, inv, msg);
    }
    if let Some(inv) = inv {
        if let Some(client) = inv.client() {
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

#[cfg(feature = "test-support")]
pub mod run_shell_tests {
    use super::{
        CStr, ClientRef, Duration, ExitStatusExt, Future, Invocation, JobCompletion, Pin, Rc, Weak,
        cmd, cmd_run_shell_complete, cmd_run_shell_data, cmd_run_shell_wait,
    };
    use crate::cmd::invocation::ExecOutcome;
    use crate::cmd::run::test_harness::spawn_invocation;
    use std::cell::Cell;
    use std::process::ExitStatus;
    use std::time::Instant;
    use tiri_core::hooks::running_hook_context;
    use tiri_core::reactor::Lane;
    use tiri_core::server_client::Client as _;
    use tiri_core::server_client::queue_test_clients as clients;
    use tiri_core::server_client::queue_test_clients::return_value;
    use tiri_core::shared::client::CLIENT_DEAD;
    use tiri_core::tiri_rt::Runtime as _;

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

    fn append_flag(owner: Option<&ClientRef>) -> Rc<Cell<bool>> {
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        spawn_invocation(owner, move |_, _| {
            Box::pin(async move {
                flag.set(true);
                ExecOutcome::ok()
            })
        });
        ran
    }

    fn append_job_wait(
        owner: &ClientRef,
        cdata: Box<cmd_run_shell_data>,
    ) -> (
        Weak<()>,
        tiri_core::tiri_rt::oneshot::Sender<JobCompletion>,
    ) {
        let (done, completed) = tiri_core::tiri_rt::oneshot::channel();
        let item = spawn_invocation(
            Some(owner),
            move |cmd_ref: &cmd, inv: &mut Invocation<'_>| -> Pin<Box<dyn Future<Output = ExecOutcome> + '_>> {
                Box::pin(async move {
                    inv.suspend(cmd_ref);
                    cmd_run_shell_complete(inv, completed, cdata).await
                })
            },
        );
        Lane::drive(Some(owner));
        (item, done)
    }

    fn exited(code: i32) -> JobCompletion {
        JobCompletion {
            status: ExitStatus::from_raw(code << 8),
            output: Vec::new(),
        }
    }

    pub fn a_zero_delay_runs_the_timer_body_one_runtime_turn_later() {
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let head = spawn_invocation(None, |cmd_ref, inv| {
                Box::pin(cmd_run_shell_wait(
                    cmd_ref,
                    inv,
                    data(None, None),
                    Duration::ZERO,
                ))
            });
            let ran = append_flag(None);
            assert_eq!(Lane::drive(None), 0, "the delay does not end in its dispatch");
            assert!(running_hook_context().is_none(), "the head is suspended");
            runtime.poll(Some(Duration::ZERO)).unwrap();
            assert!(running_hook_context().is_some());
            assert!(head.upgrade().is_some());
            assert!(!ran.get());
            assert_eq!(Lane::drive(None), 2);
            assert!(head.upgrade().is_none());
            assert!(ran.get());
        }
        tiri_core::reactor::shutdown_runtime(runtime);
    }

    pub fn a_delay_resumes_at_its_deadline() {
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let delay = Duration::from_millis(50);
            let start = Instant::now();
            let head = spawn_invocation(None, move |cmd_ref, inv| {
                Box::pin(cmd_run_shell_wait(cmd_ref, inv, data(None, None), delay))
            });
            assert_eq!(Lane::drive(None), 0);
            runtime.poll(Some(Duration::ZERO)).unwrap();
            assert!(running_hook_context().is_none());
            while head.upgrade().is_some() {
                assert!(start.elapsed() < Duration::from_secs(5));
                runtime.poll(Some(Duration::from_millis(100))).unwrap();
                Lane::drive(None);
            }
            assert!(start.elapsed() >= delay);
            assert!(running_hook_context().is_none());
            Lane::drive(None);
            assert!(head.upgrade().is_none());
        }
        tiri_core::reactor::shutdown_runtime(runtime);
    }

    pub fn dropping_the_item_during_the_delay_cancels_the_timer() {
        let mut runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let owner = clients::with_queue();
            let other = clients::with_queue();
            let cdata = data(Some(other.clone()), Some(c"true"));
            let head = spawn_invocation(Some(&owner), move |cmd_ref, inv| {
                Box::pin(cmd_run_shell_wait(
                    cmd_ref,
                    inv,
                    cdata,
                    Duration::from_secs(3600),
                ))
            });
            Lane::drive(Some(&owner));
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

    pub fn job_completion_runs_the_callback_body_inside_the_send() {
        let owner = clients::with_queue();
        owner.set_return_value(7);
        let (head, done) = append_job_wait(&owner, data(Some(owner.clone()), Some(c"true")));
        let ran = append_flag(Some(&owner));
        assert!(head.upgrade().is_some());
        assert!(done.send(exited(0)).is_ok());
        assert_eq!(return_value(&owner), 0);
        assert!(head.upgrade().is_some() && !ran.get());
        assert_eq!(Lane::drive(Some(&owner)), 2);
        assert!(head.upgrade().is_none());
        assert!(ran.get());
    }

    pub fn a_lost_client_is_freed_once_its_job_completes() {
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

    pub fn a_job_freed_unfinished_releases_the_data_and_parks() {
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
