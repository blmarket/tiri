use crate::src::cfg::cfg_finished;
use crate::src::cmd::queue::cmdq_print;
use crate::src::cmd::{cmd_log_argv, cmd_stringify_argv_cstring};
use crate::src::compat::fdforkpty::fdforkpty;
use crate::src::environ::{environ_copy, environ_for_session, environ_push, environ_set};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::log::log_bytes;
use crate::src::log::{fatal, fatalx, log_cstr, log_debug};
use crate::src::options::options_get_string;
use crate::src::proc::proc_clear_signals;
use crate::src::reactor::BufferEvent;
use crate::src::reactor::{
    bufferevent_disable, bufferevent_enable, bufferevent_new, evbuffer_get_length, evbuffer_pullup,
};
use crate::src::server::server_proc;
use crate::src::session::Session as _;
use crate::src::shared::abi::*;
use crate::src::shared::command::cmdq_item;
use crate::src::shared::environment::environ;
use crate::src::shared::event::*;
use crate::src::shared::event::{EV_READ, EV_WRITE};
use crate::src::shared::job::{
    job, job_complete_cb, job_free_cb, job_state, job_update_cb, jobs, JobCompletion,
};
use crate::src::shared::job::{
    JOB_DEFAULTSHELL, JOB_KEEPWRITE, JOB_NOWAIT, JOB_PTY, JOB_SHOWSTDERR,
};
use crate::src::shared::options::options;
use crate::src::shared::posix_io::{
    _PATH_BSHELL, _PATH_DEVNULL, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO,
};
use crate::src::shared::posix_terminal::winsize;
use crate::src::shared::session::SessionRef;
use crate::src::shared::signal::{
    sigset_t, SIGCONT, SIGTERM, SIGTTIN, SIGTTOU, SIG_BLOCK, SIG_SETMASK,
};
use crate::src::shared::terminal::*;
use crate::src::tmux::{
    checkshell, find_home_cstr, global_s_options, ptm_fd, setblocking, shell_argv0_cstring,
};
use libc::{
    _exit, chdir, execl, execvp, fork, kill, killpg, memset, setenv, sigfillset, sigprocmask,
};
use refbox::{RefBox, Weak};
use std::ffi::{CStr, CString};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

pub const JOB_CLOSED: job_state = 2;
pub const JOB_DEAD: job_state = 1;
pub const JOB_RUNNING: job_state = 0;

unsafe fn job_completion(job: &job) -> JobCompletion {
    let output = job
        .event
        .with_ptr(|stream| unsafe {
            evbuffer_pullup(&mut (*stream).input, -1)
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

/// Start a registry-owned job and return its nonowning identity.
pub unsafe fn job_run(
    cmd: Option<&CStr>,
    argv: &[CString],
    e: Option<&environ>,
    s_owner: Option<&SessionRef>,
    cwd: Option<&CStr>,
    mut updatecb: job_update_cb,
    mut completecb: job_complete_cb,
    mut freecb: job_free_cb,
    mut flags: ::core::ffi::c_int,
    mut sx: ::core::ffi::c_int,
    mut sy: ::core::ffi::c_int,
) -> Weak<job> {
    let mut shell_value: Option<CString> = None;

    let mut current_block: u64;
    let mut pid: pid_t = 0;
    let mut nullfd: ::core::ffi::c_int = 0;
    let mut out: [::core::ffi::c_int; 2] = [0; 2];
    let mut out_owner = None;
    let mut master: ::core::ffi::c_int = 0;
    let mut do_close: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
    let mut home: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut shell: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut set: sigset_t = unsafe { ::core::mem::zeroed() };
    let mut oldset: sigset_t = unsafe { ::core::mem::zeroed() };
    let mut ws: winsize = winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut argvp: *mut *mut ::core::ffi::c_char =
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>();
    let mut tty: [::core::ffi::c_char; 32] = [0; 32];
    let _oo: *mut options = ::core::ptr::null_mut::<options>();
    let mut env_owner = Some(environ_for_session(
        s_owner,
        (cfg_finished == 0) as ::core::ffi::c_int,
    ));
    let env = env_owner
        .as_deref_mut()
        .expect("job environment owner must exist");
    if let Some(e) = e {
        environ_copy(e, env);
    }
    if !flags & JOB_DEFAULTSHELL != 0 {
        shell = _PATH_BSHELL.as_ptr();
    } else {
        shell_value = Some(if let Some(session) = s_owner {
            session.with_options_mut(|options| options_get_string(options, c"default-shell"))
        } else {
            options_get_string(&*global_s_options, c"default-shell")
        });
        shell = shell_value.as_ref().expect("shell snapshot").as_ptr();
        if checkshell(nullable_cstr(shell)) == 0 {
            shell = _PATH_BSHELL.as_ptr();
        }
    }
    let argv0 = shell_argv0_cstring(CStr::from_ptr(shell), false);
    sigfillset(&raw mut set);
    sigprocmask(SIG_BLOCK, &raw mut set, &raw mut oldset);
    if flags & JOB_PTY != 0 {
        memset(
            &raw mut ws as *mut ::core::ffi::c_void,
            0 as ::core::ffi::c_int,
            ::core::mem::size_of::<winsize>() as size_t,
        );
        ws.ws_col = sx as ::core::ffi::c_ushort;
        ws.ws_row = sy as ::core::ffi::c_ushort;
        pid = fdforkpty(
            ptm_fd,
            &raw mut master,
            &raw mut tty as *mut ::core::ffi::c_char,
            ::core::ptr::null_mut::<termios>(),
            &raw mut ws,
        );
        current_block = 224731115979188411;
    } else if let Ok((parent, child)) = tiri_rt::unix::socket_pair() {
        out_owner = Some([parent, child]);
        pid = fork() as pid_t;
        current_block = 224731115979188411;
    } else {
        current_block = 12393940290395533062;
    }
    if current_block == 224731115979188411 {
        match cmd {
            None => {
                cmd_log_argv(argv, c"job_run:");
                log_debug(format_args!(
                    "{}: cwd={}, shell={}",
                    "job_run",
                    log_cstr(cwd.unwrap_or(c"")),
                    log_cstr(CStr::from_ptr(shell))
                ));
            }
            Some(cmd) => {
                log_debug(format_args!(
                    "{}: cmd={}, cwd={}, shell={}",
                    "job_run",
                    log_cstr(cmd),
                    log_cstr(cwd.unwrap_or(c"")),
                    log_cstr(CStr::from_ptr(shell))
                ));
            }
        }
        match pid {
            -1 => {
                drop(out_owner.take());
            }
            0 => {
                if let Some(pair) = out_owner.take() {
                    out = pair.map(IntoRawFd::into_raw_fd);
                }
                proc_clear_signals(server_proc, 1 as ::core::ffi::c_int);
                sigprocmask(
                    SIG_SETMASK,
                    &raw mut oldset,
                    ::core::ptr::null_mut::<sigset_t>(),
                );
                if let Some(cwd) = cwd {
                    if chdir(cwd.as_ptr()) == 0 as ::core::ffi::c_int {
                        environ_set(env, c"PWD", 0 as ::core::ffi::c_int, |out| {
                            write_cstr(out, &*cwd)
                        });
                    } else {
                        home = find_home_cstr().map_or(::core::ptr::null(), CStr::as_ptr);
                        if !home.is_null() && chdir(home) == 0 as ::core::ffi::c_int {
                            environ_set(env, c"PWD", 0 as ::core::ffi::c_int, |out| {
                                write_cstr(out, nullable_cstr(home))
                            });
                        } else if chdir(c"/".as_ptr()) == 0 as ::core::ffi::c_int {
                            environ_set(env, c"PWD", 0 as ::core::ffi::c_int, |out| {
                                out.write_all(b"/")
                            });
                        } else {
                            _exit(1 as ::core::ffi::c_int);
                        }
                    }
                }
                environ_push(env);
                drop(env_owner.take());
                if !flags & JOB_PTY != 0 {
                    if tiri_rt::unix::redirect(
                        std::os::fd::BorrowedFd::borrow_raw(out[1 as ::core::ffi::c_int as usize]),
                        STDIN_FILENO,
                    )
                    .is_err()
                    {
                        _exit(1 as ::core::ffi::c_int);
                    }
                    do_close = (do_close != 0
                        && out[1 as ::core::ffi::c_int as usize] != STDIN_FILENO)
                        as ::core::ffi::c_int;
                    if tiri_rt::unix::redirect(
                        std::os::fd::BorrowedFd::borrow_raw(out[1 as ::core::ffi::c_int as usize]),
                        STDOUT_FILENO,
                    )
                    .is_err()
                    {
                        _exit(1 as ::core::ffi::c_int);
                    }
                    do_close = (do_close != 0
                        && out[1 as ::core::ffi::c_int as usize] != STDOUT_FILENO)
                        as ::core::ffi::c_int;
                    if flags & JOB_SHOWSTDERR != 0 {
                        if tiri_rt::unix::redirect(
                            std::os::fd::BorrowedFd::borrow_raw(
                                out[1 as ::core::ffi::c_int as usize],
                            ),
                            STDERR_FILENO,
                        )
                        .is_err()
                        {
                            _exit(1 as ::core::ffi::c_int);
                        }
                        do_close = (do_close != 0
                            && out[1 as ::core::ffi::c_int as usize] != STDERR_FILENO)
                            as ::core::ffi::c_int;
                    } else {
                        nullfd = match tiri_rt::unix::open(_PATH_DEVNULL, O_RDWR, 0) {
                            Ok(fd) => fd.into_raw_fd(),
                            Err(error) => crate::src::reactor::io_status(Err(error)),
                        };
                        if nullfd == -(1 as ::core::ffi::c_int) {
                            _exit(1 as ::core::ffi::c_int);
                        }
                        if tiri_rt::unix::redirect(
                            std::os::fd::BorrowedFd::borrow_raw(nullfd),
                            STDERR_FILENO,
                        )
                        .is_err()
                        {
                            _exit(1 as ::core::ffi::c_int);
                        }
                        if nullfd != STDERR_FILENO {
                            let _ = tiri_rt::unix::close(nullfd);
                        }
                    }
                    if do_close != 0 {
                        let _ = tiri_rt::unix::close(out[1 as ::core::ffi::c_int as usize]);
                    }
                    let _ = tiri_rt::unix::close(out[0 as ::core::ffi::c_int as usize]);
                }
                tiri_rt::unix::close_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
                if let Some(cmd) = cmd {
                    if flags & JOB_DEFAULTSHELL != 0 {
                        setenv(c"SHELL".as_ptr(), shell, 1 as ::core::ffi::c_int);
                    }
                    execl(
                        shell,
                        argv0.as_ptr(),
                        c"-c".as_ptr(),
                        cmd.as_ptr(),
                        NULL as *mut ::core::ffi::c_char,
                    );
                    _exit(1 as ::core::ffi::c_int);
                } else {
                    let mut pointer_view: Vec<_> =
                        argv.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
                    pointer_view.push(::core::ptr::null_mut());
                    argvp = pointer_view.as_mut_ptr();
                    execvp(
                        *argvp.offset(0 as ::core::ffi::c_int as isize),
                        argvp as *const *const ::core::ffi::c_char,
                    );
                    _exit(1 as ::core::ffi::c_int);
                }
            }
            _ => {
                sigprocmask(
                    SIG_SETMASK,
                    &raw mut oldset,
                    ::core::ptr::null_mut::<sigset_t>(),
                );
                drop(env_owner.take());
                drop(argv0);
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
                if flags & JOB_PTY == 0 {
                    let [parent, child] = out_owner.take().expect("job socket pair");
                    drop(child);
                    value.fd = Some(parent);
                } else {
                    value.fd = Some(OwnedFd::from_raw_fd(master));
                }
                let fd = value.fd.as_ref().expect("job descriptor").as_raw_fd();
                setblocking(fd, 0);
                let job = jobs::insert(RefBox::new(value));
                let read_job = job.clone();
                let write_job = job.clone();
                let error_job = job.clone();
                let stream = bufferevent_new(
                    fd,
                    bufferevent_data_callback(move |_| unsafe { job_read_callback(&read_job) }),
                    bufferevent_data_callback(move |_| unsafe { job_write_callback(&write_job) }),
                    bufferevent_event_callback(move |_, _| unsafe {
                        job_error_callback(&error_job)
                    }),
                );
                if stream.is_null() {
                    fatalx(|out| out.write_all(b"out of memory"));
                }
                job.try_borrow_mut().expect("registered job").event =
                    crate::src::reactor::StreamHandle::from_ptr(stream);
                bufferevent_enable(stream, (EV_READ | EV_WRITE) as ::core::ffi::c_short);
                {
                    let value = job.try_borrow_mut().expect("registered job");
                    log_debug(format_args!(
                        "run job: {}, pid {}",
                        log_bytes(value.cmd.as_deref().unwrap_or(c"(null)").to_bytes()),
                        value.pid
                    ));
                }
                return job;
            }
        }
    }
    sigprocmask(
        SIG_SETMASK,
        &raw mut oldset,
        ::core::ptr::null_mut::<sigset_t>(),
    );
    drop(argv0);
    Weak::new()
}
fn job_borrow_live(handle: &Weak<job>) -> Option<refbox::Borrow<'_, job>> {
    match handle.try_borrow_mut() {
        Ok(job) => Some(job),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("job already borrowed during dispatch"),
    }
}

unsafe fn job_log(action: &str, job: &job) {
    log_debug(format_args!(
        "{} job: {}, pid {}",
        action,
        log_bytes(job.cmd.as_deref().unwrap_or(c"(null)").to_bytes()),
        job.pid
    ));
}

/// Remove ownership before callbacks, but keep the allocation and resources alive
/// until the free callback returns. Reentrant cancellation is a no-op.
pub unsafe fn job_free(handle: &Weak<job>) {
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
            kill(job.pid as __pid_t, SIGTERM);
            job.pid = -1;
        }
        std::mem::take(&mut job.event).free();
        drop(job.fd.take());
    }
    drop(owner);
}

pub unsafe fn job_resize(handle: &Weak<job>, sx: u_int, sy: u_int) {
    let job = handle.try_borrow_mut().expect("live popup job");
    if job.fd.is_none() || job.flags & JOB_PTY == 0 {
        return;
    }
    log_debug(format_args!("resize job: {}x{}", sx, sy));
    let ws = winsize {
        ws_row: sy as _,
        ws_col: sx as _,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    if crate::src::shared::terminal::set_size(job.fd.as_ref().expect("job PTY").as_raw_fd(), &ws)
        == -1
    {
        fatal(|out| out.write_all(b"ioctl failed"));
    }
}

unsafe fn job_read_callback(handle: &Weak<job>) {
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

unsafe fn job_write_callback(handle: &Weak<job>) {
    let Some(job) = job_borrow_live(handle) else {
        return;
    };
    let Some(len) = job
        .event
        .with_ptr(|stream| evbuffer_get_length(&(*stream).output))
    else {
        return;
    };
    job_log("write", &job);
    log_debug(format_args!("job output left {}", len));
    if len == 0 && job.flags & JOB_KEEPWRITE == 0 {
        let _ = tiri_rt::unix::shutdown_write(std::os::fd::AsFd::as_fd(
            job.fd.as_ref().expect("job descriptor"),
        ));
        job.event.with_ptr(|stream| {
            bufferevent_disable(stream, EV_WRITE as _);
        });
    }
}

unsafe fn job_finish(handle: &Weak<job>) {
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

unsafe fn job_error_callback(handle: &Weak<job>) {
    let complete = {
        let Some(mut job) = job_borrow_live(handle) else {
            return;
        };
        job_log("error", &job);
        if job.state == JOB_DEAD {
            true
        } else {
            job.event.with_ptr(|stream| {
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

pub unsafe fn job_check_died(pid: pid_t, status: ExitStatus) {
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
                killpg(job.pid as __pid_t, SIGCONT);
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

pub unsafe fn job_get_event(handle: &Weak<job>) -> *mut bufferevent {
    handle
        .try_borrow_mut()
        .expect("live job stream")
        .event
        .ptr()
}

pub unsafe fn job_kill_all() {
    for handle in jobs::snapshot() {
        let job = handle.try_borrow_mut().expect("registered job");
        if job.pid != -1 {
            kill(job.pid as __pid_t, SIGTERM);
        }
    }
}

pub unsafe fn job_still_running() -> ::core::ffi::c_int {
    jobs::snapshot().iter().any(|handle| {
        let job = handle.try_borrow_mut().expect("registered job");
        job.flags & JOB_NOWAIT == 0 && job.state == JOB_RUNNING
    }) as _
}

pub unsafe fn job_print_summary(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut blank: ::core::ffi::c_int,
) {
    for (n, handle) in jobs::snapshot().into_iter().enumerate() {
        let (cmd, fd, pid, status) = {
            let job = handle.try_borrow_mut().expect("registered job");
            (
                job.cmd.clone(),
                job.fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
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
    use crate::src::shared::job::job_update_callback;
    use std::cell::Cell;
    use std::os::unix::net::UnixStream;
    use std::rc::Rc;

    fn idle_job() -> job {
        job {
            pid: -1,
            fd: None,
            ..job::empty()
        }
    }

    #[test]
    fn registry_cleanup_reentrancy_and_both_completion_orders() {
        use tiri_rt::Runtime as _;
        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            assert!(jobs::snapshot().is_empty());
            let calls = Rc::new(Cell::new(0));
            let second = jobs::insert(RefBox::new(idle_job()));
            let second_observer = second.clone();
            let (socket, remote) = UnixStream::pair().unwrap();
            let fd = socket.as_raw_fd();
            let stream = bufferevent_new(fd, None, None, None);
            let first = jobs::insert(RefBox::new(job {
                fd: Some(socket.into()),
                event: crate::src::reactor::StreamHandle::from_ptr(stream),
                ..idle_job()
            }));
            let event = first.try_borrow_mut().unwrap().event.clone();
            let callback_event = event.clone();
            let observed = calls.clone();
            let callback_first = first.clone();
            first.try_borrow_mut().unwrap().freecb = Some(Box::new(move || {
                assert_eq!(jobs::snapshot(), vec![second.clone()]);
                assert_eq!(
                    callback_first
                        .try_borrow_mut()
                        .unwrap()
                        .fd
                        .as_ref()
                        .unwrap()
                        .as_raw_fd(),
                    fd
                );
                assert_eq!(callback_event.ptr(), stream);
                assert!(::libc::fcntl(fd, ::libc::F_GETFD) >= 0);
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
            assert!(event.ptr().is_null());
            assert_eq!(::libc::fcntl(fd, ::libc::F_GETFD), -1);
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
                let stream = bufferevent_new(-1, None, None, None);
                crate::src::reactor::evbuffer_add(&mut (*stream).input, b"remaining");
                let handle = jobs::insert(RefBox::new(job {
                    pid: 1234567,
                    event: crate::src::reactor::StreamHandle::from_ptr(stream),
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
            crate::src::reactor::shutdown_runtime(runtime);
        }
    }

    #[test]
    fn completion_reads_a_live_stream_and_skips_one_after_free() {
        unsafe {
            let stream = bufferevent_new(-1, None, None, None);
            let owner = RefBox::new(job {
                event: crate::src::reactor::StreamHandle::from_ptr(stream),
                ..idle_job()
            });
            let handle = owner.downgrade();
            assert_eq!(job_get_event(&handle), stream);
            crate::src::reactor::evbuffer_add(&mut (*stream).input, b"output");
            assert_eq!(
                job_completion(&owner.try_borrow_mut().unwrap()).output,
                b"output"
            );
            std::mem::take(&mut owner.try_borrow_mut().unwrap().event).free();
            assert!(job_get_event(&handle).is_null());
            assert!(job_completion(&owner.try_borrow_mut().unwrap())
                .output
                .is_empty());
        }
    }
}
