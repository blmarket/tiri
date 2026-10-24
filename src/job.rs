use crate::cfg::cfg_finished;
use crate::cmd::queue::cmdq_print;
use crate::cmd::{cmd_log_argv, cmd_stringify_argv_cstring};
use crate::compat::fdforkpty::fdforkpty;
use crate::environ::{environ_copy, environ_for_session, environ_push, environ_set};
use crate::format::bytes::write_cstr;
use crate::log::log_bytes;
use crate::log::{fatal, fatalx, log_cstr, log_debug};
use crate::options::options_get_string;
use crate::options::OptionsScope;
use crate::proc::proc_clear_signals;
use crate::reactor::BufferEvent;
use crate::reactor::{
    bufferevent_disable, bufferevent_enable, bufferevent_new, evbuffer_get_length, evbuffer_pullup,
};
use crate::server::server_proc;
use crate::session::Session as _;
use crate::shared::abi::*;
use crate::shared::command::cmdq_item;
use crate::shared::environment::environ;
use crate::shared::event::*;
use crate::shared::event::{EV_READ, EV_WRITE};
use crate::shared::job::{
    job, job_complete_cb, job_free_cb, job_state, job_update_cb, jobs, JobCompletion,
};
use crate::shared::job::{JOB_DEFAULTSHELL, JOB_KEEPWRITE, JOB_NOWAIT, JOB_PTY, JOB_SHOWSTDERR};
use crate::shared::posix_io::{_PATH_BSHELL, _PATH_DEVNULL, STDERR_FILENO};
use crate::shared::posix_terminal::winsize;
use crate::shared::session::SessionRef;
use crate::shared::signal::{SIGCONT, SIGTERM, SIGTTIN, SIGTTOU};
use crate::tmux::{checkshell, find_home_cstr, setblocking, shell_argv0_cstring};
use refbox::{RefBox, Weak};
use std::ffi::OsStr;
use std::ffi::{CStr, CString};
use std::os::fd::OwnedFd;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use tiri_sys::{
    block_signals, close_on_exec_from, exec, exec_with_stdio, exit_now, fork, send_group_signal,
    send_signal, set_signal_mask, Program, SignalMask,
};

pub const JOB_CLOSED: job_state = 2;
pub const JOB_DEAD: job_state = 1;
pub const JOB_RUNNING: job_state = 0;

fn job_completion(job: &job) -> JobCompletion {
    let output = job
        .event
        .with_stream(|stream| {
            evbuffer_pullup(&mut stream.input, -1)
                .unwrap_or_default()
                .to_vec()
        })
        .unwrap_or_default();
    JobCompletion {
        status: job.status,
        output,
    }
}
pub use ::libc::O_RDWR;

thread_local! {
    static JOBS: std::cell::RefCell<jobs> = const {
        std::cell::RefCell::new(jobs {
            storage: Vec::new(),
        })
    };
}

/// Each call borrows the job list only for its own duration and returns
/// nonowning handles; a removed owner is dropped by the caller.
impl jobs {
    fn insert(owner: RefBox<job>) -> Weak<job> {
        let observer = owner.downgrade();
        JOBS.with_borrow_mut(|jobs| jobs.storage.insert(0, owner));
        observer
    }

    fn remove(handle: &Weak<job>) -> Option<RefBox<job>> {
        JOBS.with_borrow_mut(|jobs| {
            let index = jobs.storage.iter().position(|owner| handle.is(owner))?;
            Some(jobs.storage.remove(index))
        })
    }

    fn snapshot() -> Vec<Weak<job>> {
        JOBS.with_borrow(|jobs| jobs.storage.iter().map(RefBox::downgrade).collect())
    }
}

/// Start a registry-owned job and return its nonowning identity. The server is
/// single-threaded, so the forked child may run ordinary code until it execs.
pub fn job_run(
    cmd: Option<&CStr>,
    argv: &[CString],
    e: Option<&environ>,
    s_owner: Option<&SessionRef>,
    cwd: Option<&CStr>,
    updatecb: job_update_cb,
    completecb: job_complete_cb,
    freecb: job_free_cb,
    flags: ::core::ffi::c_int,
    sx: ::core::ffi::c_int,
    sy: ::core::ffi::c_int,
) -> Weak<job> {
    let mut env = environ_for_session(s_owner, (!cfg_finished()) as ::core::ffi::c_int);
    if let Some(e) = e {
        environ_copy(e, &mut env);
    }
    let shell_value;
    let shell: &CStr = if !flags & JOB_DEFAULTSHELL != 0 {
        _PATH_BSHELL
    } else {
        shell_value = if let Some(session) = s_owner {
            options_get_string(&session.borrow_options_mut(), c"default-shell")
        } else {
            OptionsScope::GlobalSession.with_local(|oo| options_get_string(oo, c"default-shell"))
        };
        if checkshell(Some(&shell_value)) == 0 {
            _PATH_BSHELL
        } else {
            &shell_value
        }
    };
    let argv0 = shell_argv0_cstring(shell, false);
    let oldset = block_signals();
    let mut tty = [0u8; 32];
    let (forked, child_socket) = if flags & JOB_PTY != 0 {
        let ws = winsize {
            ws_row: sy as ::core::ffi::c_ushort,
            ws_col: sx as ::core::ffi::c_ushort,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        (fdforkpty(&mut tty, &ws), None)
    } else {
        let Ok((parent, child)) = tiri_sys::socket_pair() else {
            set_signal_mask(&oldset);
            return Weak::new();
        };
        (fork().map(|pid| pid.map(|pid| (pid, parent))), Some(child))
    };
    match cmd {
        None => {
            cmd_log_argv(argv, c"job_run:");
            log_debug(format_args!(
                "{}: cwd={}, shell={}",
                "job_run",
                log_cstr(cwd.unwrap_or(c"")),
                log_cstr(shell)
            ));
        }
        Some(cmd) => {
            log_debug(format_args!(
                "{}: cmd={}, cwd={}, shell={}",
                "job_run",
                log_cstr(cmd),
                log_cstr(cwd.unwrap_or(c"")),
                log_cstr(shell)
            ));
        }
    }
    let (pid, fd) = match forked {
        Err(_) => {
            set_signal_mask(&oldset);
            return Weak::new();
        }
        Ok(None) => job_exec_child(
            env,
            child_socket,
            cmd,
            argv,
            cwd,
            shell,
            &argv0,
            flags,
            &oldset,
        ),
        Ok(Some(forked)) => {
            drop(child_socket);
            forked
        }
    };
    set_signal_mask(&oldset);
    drop(env);
    let cmd_owner = if let Some(cmd) = cmd {
        Some(cmd.to_owned())
    } else {
        cmd_stringify_argv_cstring(argv)
    };
    let mut value = job {
        cmd: cmd_owner,
        state: JOB_RUNNING,
        flags,
        pid,
        updatecb,
        completecb,
        freecb,
        ..job::empty()
    };
    if flags & JOB_PTY != 0 {
        value.tty = tty;
    }
    setblocking(fd.as_fd(), 0);
    let job = jobs::insert(RefBox::new(value));
    let read_job = job.clone();
    let write_job = job.clone();
    let error_job = job.clone();
    let stream = bufferevent_new(
        Some(fd),
        bufferevent_data_callback(move || job_read_callback(&read_job)),
        bufferevent_data_callback(move || job_write_callback(&write_job)),
        bufferevent_event_callback(move |_, _| job_error_callback(&error_job)),
    )
    .unwrap_or_else(|_| fatalx(|out| out.write_all(b"out of memory")));
    job.try_borrow_mut().expect("registered job").event = stream.clone();
    let _ = stream.with_stream(|stream| {
        bufferevent_enable(stream, (EV_READ | EV_WRITE) as ::core::ffi::c_short)
    });
    {
        let value = job.try_borrow_mut().expect("registered job");
        log_debug(format_args!(
            "run job: {}, pid {}",
            log_bytes(value.cmd.as_deref().unwrap_or(c"(null)").to_bytes()),
            value.pid
        ));
    }
    job
}
/// Set up the forked job child's directory, environment and stdio, then exec.
fn job_exec_child(
    mut env: Box<environ>,
    socket: Option<OwnedFd>,
    cmd: Option<&CStr>,
    argv: &[CString],
    cwd: Option<&CStr>,
    shell: &CStr,
    argv0: &CStr,
    flags: ::core::ffi::c_int,
    oldset: &SignalMask,
) -> ! {
    proc_clear_signals(&server_proc(), 1 as ::core::ffi::c_int);
    set_signal_mask(oldset);
    if let Some(cwd) = cwd {
        let change =
            |path: &CStr| std::env::set_current_dir(OsStr::from_bytes(path.to_bytes())).is_ok();
        let pwd = if change(cwd) {
            cwd
        } else if let Some(home) = find_home_cstr().filter(|home| change(home)) {
            home
        } else if change(c"/") {
            c"/"
        } else {
            exit_now(1);
        };
        environ_set(&mut env, c"PWD", 0 as ::core::ffi::c_int, |out| {
            write_cstr(out, pwd)
        });
    }
    environ_push(&env);
    drop(env);
    if cmd.is_some() && flags & JOB_DEFAULTSHELL != 0 {
        std::env::set_var("SHELL", OsStr::from_bytes(shell.to_bytes()));
    }
    let argv: Vec<&CStr> = argv.iter().map(CString::as_c_str).collect();
    let shell_argv;
    let program = match cmd {
        Some(cmd) => {
            shell_argv = [argv0, c"-c", cmd];
            Program::Path(shell, &shell_argv)
        }
        None => Program::Search(&argv),
    };
    close_on_exec_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
    let Some(child) = socket else {
        exec(program);
        exit_now(1);
    };
    if flags & JOB_SHOWSTDERR != 0 {
        exec_with_stdio([child.as_fd(); 3], program);
    }
    let Ok(null) = tiri_sys::open(_PATH_DEVNULL, O_RDWR, 0) else {
        exit_now(1);
    };
    exec_with_stdio([child.as_fd(), child.as_fd(), null.as_fd()], program)
}
fn job_borrow_live(handle: &Weak<job>) -> Option<refbox::Borrow<'_, job>> {
    match handle.try_borrow_mut() {
        Ok(job) => Some(job),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("job already borrowed during dispatch"),
    }
}

fn job_log(action: &str, job: &job) {
    log_debug(format_args!(
        "{} job: {}, pid {}",
        action,
        log_bytes(job.cmd.as_deref().unwrap_or(c"(null)").to_bytes()),
        job.pid
    ));
}

/// Remove ownership before callbacks, but keep the allocation and resources alive
/// until the free callback returns. Reentrant cancellation is a no-op.
pub fn job_free(handle: &Weak<job>) {
    let Some(owner) = jobs::remove(handle) else {
        return;
    };
    let callback = {
        let mut job = owner
            .try_borrow_mut()
            .expect("unborrowed job during cleanup");
        job_log("free", &job);
        job.freecb.take()
    };
    if let Some(callback) = callback {
        callback();
    }
    {
        let mut job = owner
            .try_borrow_mut()
            .expect("unborrowed job after cleanup callback");
        if job.pid != -1 {
            let _ = send_signal(job.pid, SIGTERM);
        }
        std::mem::take(&mut job.event).free();
    }
}

pub fn job_resize(handle: &Weak<job>, sx: u_int, sy: u_int) {
    let job = handle.try_borrow_mut().expect("live popup job");
    if job.flags & JOB_PTY == 0 {
        return;
    }
    let ws = winsize {
        ws_row: sy as _,
        ws_col: sx as _,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let resized = job.event.with_stream(|stream| {
        let fd = stream.fd.as_ref()?;
        log_debug(format_args!("resize job: {}x{}", sx, sy));
        Some(crate::shared::terminal::set_size(fd.as_fd(), &ws))
    });
    if let Some(Some(Err(error))) = resized {
        fatal(&error, |out| out.write_all(b"ioctl failed"));
    }
}

fn job_read_callback(handle: &Weak<job>) {
    let callback_slot = {
        let Some(job) = job_borrow_live(handle) else {
            return;
        };
        let Some(slot) = job.updatecb.clone() else {
            return;
        };
        slot
    };
    let Some(mut callback) = callback_slot.borrow_mut().take() else {
        return;
    };
    callback(handle);
    let mut slot = callback_slot.borrow_mut();
    if slot.is_none() {
        *slot = Some(callback);
    }
}

fn job_write_callback(handle: &Weak<job>) {
    let Some(job) = job_borrow_live(handle) else {
        return;
    };
    let Some(len) = job
        .event
        .with_stream(|stream| evbuffer_get_length(&stream.output))
    else {
        return;
    };
    job_log("write", &job);
    log_debug(format_args!("job output left {}", len));
    if len == 0 && job.flags & JOB_KEEPWRITE == 0 {
        job.event.with_stream(|stream| {
            let _ =
                tiri_sys::shutdown_write(stream.fd.as_ref().expect("job descriptor").as_fd());
            bufferevent_disable(stream, EV_WRITE as _);
        });
    }
}

fn job_finish(handle: &Weak<job>) {
    let completion = {
        let Some(mut job) = job_borrow_live(handle) else {
            return;
        };
        job.completecb
            .take()
            .map(|callback| (callback, job_completion(&job)))
    };
    if let Some((callback, result)) = completion {
        callback(result);
    }
    job_free(handle);
}

fn job_error_callback(handle: &Weak<job>) {
    let complete = {
        let Some(mut job) = job_borrow_live(handle) else {
            return;
        };
        job_log("error", &job);
        if job.state == JOB_DEAD {
            true
        } else {
            job.event.with_stream(|stream| {
                bufferevent_disable(stream, EV_READ as _);
            });
            job.state = JOB_CLOSED;
            false
        }
    };
    if complete {
        job_finish(handle);
    }
}

pub fn job_check_died(pid: pid_t, status: ExitStatus) {
    let Some(handle) = jobs::snapshot()
        .into_iter()
        .find(|handle| handle.try_borrow_mut().expect("registered job").pid == pid)
    else {
        return;
    };
    let complete = {
        let mut job = handle.try_borrow_mut().expect("registered job");
        if let Some(signal) = status.stopped_signal() {
            if signal != SIGTTIN && signal != SIGTTOU {
                let _ = send_group_signal(job.pid, SIGCONT);
            }
            return;
        }
        job_log("died", &job);
        job.status = status;
        job.pid = -1;
        if job.state == JOB_CLOSED {
            true
        } else {
            job.state = JOB_DEAD;
            false
        }
    };
    if complete {
        job_finish(&handle);
    }
}

pub fn job_get_event(handle: &Weak<job>) -> crate::reactor::StreamHandle {
    handle
        .try_borrow_mut()
        .expect("live job stream")
        .event
        .clone()
}

pub fn job_kill_all() {
    for handle in jobs::snapshot() {
        let job = handle.try_borrow_mut().expect("registered job");
        if job.pid != -1 {
            let _ = send_signal(job.pid, SIGTERM);
        }
    }
}

pub fn job_still_running() -> ::core::ffi::c_int {
    jobs::snapshot().iter().any(|handle| {
        let job = handle.try_borrow_mut().expect("registered job");
        job.flags & JOB_NOWAIT == 0 && job.state == JOB_RUNNING
    }) as _
}

pub fn job_print_summary(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut blank: ::core::ffi::c_int,
) {
    for (n, handle) in jobs::snapshot().into_iter().enumerate() {
        let (cmd, fd, pid, status) = {
            let job = handle.try_borrow_mut().expect("registered job");
            (
                job.cmd.clone(),
                job.event
                    .with_stream(|stream| stream.fd.as_ref().map(AsRawFd::as_raw_fd))
                    .flatten()
                    .unwrap_or(-1),
                job.pid,
                job.status,
            )
        };
        if blank != 0 {
            cmdq_print(item_handle, |_| Ok(()));
            blank = 0;
        }
        cmdq_print(item_handle, |out| {
            write!(out, "Job {}: ", n)?;
            out.write_all(cmd.as_deref().unwrap_or(c"(null)").to_bytes())?;
            write!(
                out,
                " [fd={}, pid={}, status={}]",
                fd,
                pid,
                status.into_raw()
            )
        });
    }
}

#[cfg(test)]
mod job_stream_tests {
    use super::*;
    use crate::shared::job::job_update_callback;
    use std::cell::Cell;
    use std::os::unix::net::UnixStream;
    use std::rc::Rc;

    fn idle_job() -> job {
        job {
            pid: -1,
            ..job::empty()
        }
    }

    #[test]
    fn registry_cleanup_reentrancy_and_both_completion_orders() {
        use tiri_rt::Runtime as _;
        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        {
            assert!(jobs::snapshot().is_empty());
            let calls = Rc::new(Cell::new(0));
            let second = jobs::insert(RefBox::new(idle_job()));
            let second_observer = second.clone();
            let (socket, remote) = UnixStream::pair().unwrap();
            let fd = socket.as_raw_fd();
            let stream = bufferevent_new(Some(socket.into()), None, None, None).unwrap();
            let first = jobs::insert(RefBox::new(job {
                event: stream.clone(),
                ..idle_job()
            }));
            let event = first.try_borrow_mut().unwrap().event.clone();
            let callback_event = event.clone();
            let observed = calls.clone();
            let callback_first = first.clone();
            first.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
                assert_eq!(jobs::snapshot(), vec![second.clone()]);
                assert_eq!(
                    callback_event.with_stream(|stream| stream.fd.as_ref().unwrap().as_raw_fd()),
                    Some(fd)
                );
                assert!(callback_event == stream);
                assert!(tiri_sys::is_open(fd));
                job_free(&callback_first);
                job_free(&second);
                let replacement = jobs::insert(RefBox::new(idle_job()));
                assert_ne!(replacement, callback_first);
                job_free(&replacement);
                observed.set(observed.get() + 1);
            }));
            job_free(&first);
            assert_eq!(calls.get(), 1);
            assert!(first.try_borrow_mut().is_err());
            assert!(second_observer.try_borrow_mut().is_err());
            assert!(!event.is_alive());
            assert!(!tiri_sys::is_open(fd));
            drop(remote);
            assert!(jobs::snapshot().is_empty());

            let update = jobs::insert(RefBox::new(idle_job()));
            let observed = calls.clone();
            update.try_borrow_mut().unwrap().updatecb = job_update_callback(move |job| {
                observed.set(observed.get() + 1);
                job_free(job);
            });
            job_read_callback(&update);
            job_read_callback(&update);
            assert_eq!(calls.get(), 2);
            assert!(update.try_borrow_mut().is_err());

            for eof_first in [false, true] {
                let stream = bufferevent_new(None, None, None, None).unwrap();
                stream.with_stream(|stream| {
                    crate::reactor::evbuffer_add(&mut stream.input, b"remaining")
                });
                let handle = jobs::insert(RefBox::new(job {
                    pid: 1234567,
                    event: stream,
                    ..idle_job()
                }));
                let callback_handle = handle.clone();
                let observed = calls.clone();
                handle.try_borrow_mut().unwrap().completecb = Some(Box::new(move |completion| {
                    assert_eq!(completion.output, b"remaining");
                    assert_eq!(completion.status.code(), Some(7));
                    assert_eq!(callback_handle.try_borrow_mut().unwrap().pid, -1);
                    observed.set(observed.get() + 1);
                    job_free(&callback_handle);
                }));
                if eof_first {
                    job_error_callback(&handle);
                    assert_eq!(handle.try_borrow_mut().unwrap().state, JOB_CLOSED);
                    job_check_died(1234567, ExitStatus::from_raw(7 << 8));
                } else {
                    job_check_died(1234567, ExitStatus::from_raw(7 << 8));
                    assert_eq!(handle.try_borrow_mut().unwrap().state, JOB_DEAD);
                    job_error_callback(&handle);
                }
                assert!(handle.try_borrow_mut().is_err());
                job_error_callback(&handle);
                job_write_callback(&handle);
            }
            assert_eq!(calls.get(), 4);
            assert!(jobs::snapshot().is_empty());
            crate::reactor::shutdown_runtime(runtime);
        }
    }

    #[test]
    fn completion_reads_a_live_stream_and_skips_one_after_free() {
        let stream = { bufferevent_new(None, None, None, None) }.unwrap();
        let owner = RefBox::new(job {
            event: stream.clone(),
            ..idle_job()
        });
        let handle = owner.downgrade();
        assert!(job_get_event(&handle) == stream);
        stream.with_stream(|stream| crate::reactor::evbuffer_add(&mut stream.input, b"output"));
        assert_eq!(
            job_completion(&owner.try_borrow_mut().unwrap()).output,
            b"output"
        );
        std::mem::take(&mut owner.try_borrow_mut().unwrap().event).free();
        assert!(!job_get_event(&handle).is_alive());
        assert!(job_completion(&owner.try_borrow_mut().unwrap())
            .output
            .is_empty());
    }
}
