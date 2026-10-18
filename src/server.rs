use crate::src::cmd::entries::wait_for::cmd_wait_for_flush;
use crate::src::cmd::queue::cmdq_next;
#[cfg(target_os = "linux")]
use crate::src::compat::systemd::{systemd_activated, systemd_create_socket};
use crate::src::control_notify::control_build_events;
use crate::src::ffi::libc::{__errno_location, stderr};
use crate::src::format::bytes::format_message_with;
use crate::src::format::format_tidy_jobs;
use crate::src::hooks::hooks_build_events;
use crate::src::input_keys::input_key_build;
use crate::src::job::{job_check_died, job_kill_all, job_still_running};
use crate::src::key_bindings::key_bindings_init;
use crate::src::log::{fatal, log_cstr, log_debug, log_get_level};
use crate::src::options::OptionsScope;
use crate::src::options::{options_get_number, options_set_number};
use crate::src::proc::{
    proc_fork_and_daemon, proc_loop, proc_set_signals, proc_start, proc_toggle_log,
};
use crate::src::prompt_history::prompt_save_history;
use crate::src::reactor;
use crate::src::server_acl::{server_acl_init, server_acl_join};
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use crate::src::shared::window::WindowWeak;
use crate::src::window::Window as _;
use libc::{
    chmod, exit, fprintf, kill, killpg, sigfillset, sigprocmask, stat, strerror, strsignal, time,
    umask, unlink, waitpid,
};
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
use std::os::fd::{IntoRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixListener;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use crate::src::session::sessions;
use crate::src::session::Session;

use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::shared::status::message_list;
use crate::src::text::utf8::utf8_update_width_cache;
use crate::src::tmux::{get_timer, socket_path_cstr, start_time};
use crate::src::tty::tty_create_log;

use crate::src::window::windows;
use crate::src::window_pane::WindowPane as _;
use std::time::{Duration, SystemTime};
use tiri_rt::{AsyncAccept as _, Runtime as _};

use std::ffi::{CStr, CString, OsStr};
use std::io;

unsafe fn server_clear_messages() {
    message_log.clear();
}

use crate::src::shared::abi::__mode_t;
use crate::src::shared::abi::*;
use crate::src::shared::client::{CLIENT_DEFAULTSOCKET, CLIENT_IDENTIFIED, CLIENT_NOFORK};
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::errno::{EAGAIN, ECHILD, EINTR, ENAMETOOLONG};
use crate::src::shared::event::*;
use crate::src::shared::pane::window_pane;
use crate::src::shared::posix_io::{S_IRWXU, WAIT_ANY, WNOHANG};
use crate::src::shared::process::tmuxproc;
use crate::src::shared::signal::{
    sigset_t, ProcessSignal, SIGCONT, SIGTTIN, SIGTTOU, SIG_BLOCK, SIG_SETMASK,
};
use crate::src::shared::window::winlink;

pub type mode_t = __mode_t;

pub const ACCESSPERMS: mode_t = S_IRWXU | S_IRWXG | S_IRWXO;

pub use ::libc::{
    ECONNABORTED, EMFILE, ENFILE, S_IRGRP, S_IROTH, S_IRUSR, S_IRWXG, S_IRWXO, S_IXGRP, S_IXOTH,
    S_IXUSR, WUNTRACED,
};

pub static mut server_proc: *mut tmuxproc = ::core::ptr::null::<tmuxproc>() as *mut tmuxproc;
static mut server_fd: Option<UnixListener> = None;
static mut server_client_flags: uint64_t = 0;
static mut server_exit: ::core::ffi::c_int = 0;
static mut server_accept_task: Option<tiri_rt::mio::Task> = None::<tiri_rt::mio::Task>;
static mut server_ev_tidy: Option<Timer> = None;
pub static mut marked_pane: cmd_find_state = cmd_find_state {
    flags: 0,
    s: std::rc::Weak::new(),
    wl: refbox::Weak::new(),
    w: WindowWeak::new(),
    wp: std::rc::Weak::new(),
    idx: 0,
};
static mut message_next: u_int = 0;
pub static mut message_log: message_list = message_list::new();
pub static mut current_time: time_t = 0;
pub unsafe fn server_set_marked(
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
) {
    marked_pane = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
    marked_pane.set_s(s_owner);
    marked_pane.set_wl(wl.clone());
    if wl.is_alive() {
        marked_pane.set_w(wl.get_unchecked().window_handle());
    }
    marked_pane.wp = wp_owner.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
}
pub unsafe fn server_clear_marked() {
    marked_pane = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
}
pub unsafe fn server_is_marked(
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
) -> ::core::ffi::c_int {
    if s_owner.is_none() || !wl.is_alive() || wp_owner.is_none() {
        return 0 as ::core::ffi::c_int;
    }
    if !marked_pane
        .s
        .ptr_eq(&std::rc::Rc::downgrade(s_owner.unwrap()))
        || marked_pane.winlink_handle() != wl
    {
        return 0 as ::core::ffi::c_int;
    }
    if !marked_pane
        .wp
        .ptr_eq(&std::rc::Rc::downgrade(wp_owner.unwrap()))
    {
        return 0;
    }
    server_check_marked()
}
pub unsafe fn server_check_marked() -> ::core::ffi::c_int {
    marked_pane.is_valid() as ::core::ffi::c_int
}
pub unsafe fn server_create_socket(flags: uint64_t) -> Result<::core::ffi::c_int, CString> {
    server_create_listener(flags).map(IntoRawFd::into_raw_fd)
}

unsafe fn server_create_listener(flags: uint64_t) -> Result<UnixListener, CString> {
    let path = socket_path_cstr().to_bytes();
    let result = (|| {
        if path.len() >= 108 {
            return Err(io::Error::from_raw_os_error(ENAMETOOLONG));
        }
        unlink(socket_path_cstr().as_ptr());
        let mask = if flags & CLIENT_DEFAULTSOCKET as uint64_t != 0 {
            umask((S_IXUSR | S_IXGRP | S_IRWXO) as __mode_t)
        } else {
            umask((S_IXUSR | S_IRWXG | S_IRWXO) as __mode_t)
        };
        let listener = tiri_rt::unix::listen(std::path::Path::new(OsStr::from_bytes(path)), 128);
        umask(mask);
        let listener = listener?;
        Ok(listener)
    })();
    result.map_err(|error: io::Error| {
        let reason =
            CStr::from_ptr(strerror(error.raw_os_error().unwrap_or(::libc::EINVAL))).to_bytes();
        let mut message = Vec::with_capacity(17 + path.len() + reason.len());
        message.extend_from_slice(b"error creating ");
        message.extend_from_slice(path);
        message.extend_from_slice(b" (");
        message.extend_from_slice(reason);
        message.push(b')');
        CString::new(message).expect("C strings contain no interior NUL")
    })
}
unsafe fn server_tidy_event() {
    let tv = Duration::from_secs(3600);
    let mut t: uint64_t = get_timer();
    format_tidy_jobs();
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    libc::malloc_trim(0 as size_t);
    log_debug(format_args!(
        "{}: took {} milliseconds",
        "server_tidy_event",
        get_timer().wrapping_sub(t) as ::core::ffi::c_ulonglong
    ));
    server_ev_tidy = Some(Timer::new(tv, || unsafe { server_tidy_event() }).expect("arm timer"));
}
pub(crate) unsafe fn server_start(
    mut flags: uint64_t,
    lockfd: &mut Option<OwnedFd>,
    lockfile: &mut Option<CString>,
) -> OwnedFd {
    let mut fd = None;
    let mut set: sigset_t = unsafe { ::core::mem::zeroed() };
    let mut oldset: sigset_t = unsafe { ::core::mem::zeroed() };
    let mut cause: Option<CString> = None;
    let tv = Duration::from_secs(3600);
    sigfillset(&raw mut set);
    sigprocmask(SIG_BLOCK, &raw mut set, &raw mut oldset);
    if !flags & CLIENT_NOFORK as uint64_t != 0 {
        let (pid, socket) = proc_fork_and_daemon();
        if pid != 0 {
            sigprocmask(
                SIG_SETMASK,
                &raw mut oldset,
                ::core::ptr::null_mut::<sigset_t>(),
            );
            return socket;
        }
        fd = Some(socket);
    }
    server_client_flags = flags;
    let runtime = tiri_rt::mio::Runtime::new().expect("tiri-rt initialization");
    let mut process_owner = proc_start(c"server");
    server_proc = &raw mut *process_owner;
    proc_set_signals(
        server_proc,
        Some(Box::new(|sig| unsafe { server_signal(sig) })),
    );
    sigprocmask(
        SIG_SETMASK,
        &raw mut oldset,
        ::core::ptr::null_mut::<sigset_t>(),
    );
    if log_get_level() > 1 as ::core::ffi::c_int {
        tty_create_log();
    }
    if 0 as ::core::ffi::c_int != 0 as ::core::ffi::c_int {
        fatal(|out| out.write_all(b"pledge failed"));
    }
    input_key_build();
    utf8_update_width_cache();
    key_bindings_init();
    control_build_events();
    hooks_build_events();
    server_clear_messages();
    start_time = SystemTime::now();
    crate::src::plugin::init();
    #[cfg(target_os = "linux")]
    let listener = if systemd_activated() != 0 {
        systemd_create_socket(flags as ::core::ffi::c_int).map(|fd| UnixListener::from_raw_fd(fd))
    } else {
        server_create_listener(flags)
    };
    #[cfg(not(target_os = "linux"))]
    let listener = server_create_listener(flags);
    match listener {
        Ok(socket) => {
            server_fd = Some(socket);
            server_update_socket();
        }
        Err(error) => {
            server_fd = None;
            cause = Some(error);
        }
    }
    // The registry owns the client; this reference only reports a startup
    // cause and is released before the loop, so it never outlives a loss.
    let c: Option<ClientRef> = match fd {
        Some(fd) => Some(ClientRef::create(fd)),
        None => {
            OptionsScope::GlobalServer.with_local(|oo| {
                options_set_number(oo, c"exit-empty", 0 as ::core::ffi::c_longlong);
            });
            None
        }
    };
    if lockfd.is_some() {
        if let Some(path) = lockfile.take() {
            unlink(path.as_ptr());
        }
        drop(lockfd.take());
    }
    if let Some(cause) = cause {
        if let Some(c_value) = c.as_ref() {
            c_value.exit_with_message(cause, Some(1));
        } else {
            fprintf(stderr, c"%s\n".as_ptr(), cause.as_ptr());
            drop(process_owner);
            server_proc = std::ptr::null_mut();
            crate::src::plugin::shutdown();
            reactor::shutdown_runtime(runtime);
            exit(1 as ::core::ffi::c_int);
        }
    }
    drop(c);
    server_ev_tidy = Some(Timer::new(tv, || unsafe { server_tidy_event() }).expect("arm timer"));
    server_acl_init();
    server_add_accept(0 as ::core::ffi::c_int);
    let mut loop_callback = || unsafe { server_loop() == 0 };
    proc_loop(server_proc, runtime, Some(&mut loop_callback));
    crate::src::plugin::shutdown();
    crate::src::cmd::queue::cmdq_cancel_background();
    job_kill_all();
    prompt_save_history();
    drop(process_owner);
    server_proc = std::ptr::null_mut();
    exit(0 as ::core::ffi::c_int);
}
unsafe fn server_loop() -> ::core::ffi::c_int {
    let mut c: Option<ClientRef> = None;
    let mut items: u_int = 0;
    current_time = time(::core::ptr::null_mut::<time_t>());
    loop {
        items = cmdq_next(None);
        let mut registry_c_owner = ClientRegistry::first();
        c = registry_c_owner.clone();
        while !c.is_none() {
            if c.as_ref().expect("live client").flags() & CLIENT_IDENTIFIED as uint64_t != 0 {
                items = items.wrapping_add(cmdq_next(registry_c_owner.as_ref()));
            }
            registry_c_owner =
                ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
            c = registry_c_owner.clone();
        }
        if !(items != 0 as u_int) {
            break;
        }
    }
    ClientRef::run_cycle();
    if OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"exit-empty")) == 0
        && server_exit == 0
    {
        return 0 as ::core::ffi::c_int;
    }
    if OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"exit-unattached")) == 0
        && !sessions::is_empty()
    {
        return 0 as ::core::ffi::c_int;
    }
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        if !c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
        {
            return 0 as ::core::ffi::c_int;
        }
        registry_c_owner =
            ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
    cmd_wait_for_flush();
    if ClientRegistry::first().is_some() {
        return 0 as ::core::ffi::c_int;
    }
    if job_still_running() != 0 {
        return 0 as ::core::ffi::c_int;
    }
    1 as ::core::ffi::c_int
}
unsafe fn server_send_exit() {
    let mut s: Option<SessionRef> = None;
    cmd_wait_for_flush();
    let mut registry_c_owner = ClientRegistry::first();
    while let Some(client_owner) = registry_c_owner {
        let _c: Option<ClientRef> = Some(client_owner.clone());
        registry_c_owner = ClientRegistry::next(&client_owner);
        client_owner.shutdown();
    }
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        let name = s.as_ref().expect("live session").name();
        (s_owner.as_ref().expect("registered session"))
            .destroy((1 as ::core::ffi::c_int) != 0, c"server_send_exit");
        s_owner = sessions::after(&name);
        s = s_owner.clone();
    }
}
pub unsafe fn server_update_socket() {
    let mut s: Option<SessionRef> = None;
    static mut last: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut n: ::core::ffi::c_int = 0;
    let mut mode: __mode_t = 0;
    let mut sb: stat = unsafe { ::core::mem::zeroed() };
    n = 0 as ::core::ffi::c_int;
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        if s.as_ref().expect("live session").is_attached() {
            n += 1;
            break;
        } else {
            s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
            s = s_owner.clone();
        }
    }
    if n != last {
        last = n;
        if stat(socket_path_cstr().as_ptr(), &raw mut sb) != 0 as ::core::ffi::c_int {
            return;
        }
        mode = sb.st_mode & ACCESSPERMS;
        if n != 0 as ::core::ffi::c_int {
            if mode & S_IRUSR != 0 {
                mode |= S_IXUSR;
            }
            if mode & S_IRGRP != 0 {
                mode |= S_IXGRP;
            }
            if mode & S_IROTH != 0 {
                mode |= S_IXOTH;
            }
        } else {
            mode &= !(S_IXUSR | S_IXGRP | S_IXOTH);
        }
        chmod(socket_path_cstr().as_ptr(), mode);
    }
}
unsafe fn server_accept(result: io::Result<OwnedFd>) {
    server_add_accept(0);
    let socket = match result {
        Ok(socket) => socket,
        Err(error) => match error.raw_os_error() {
            Some(EAGAIN | EINTR | ECONNABORTED) => return,
            Some(ENFILE | EMFILE) => {
                server_add_accept(1);
                return;
            }
            _ => fatal(|out| write!(out, "accept failed: {error}")),
        },
    };
    if server_exit != 0 {
        return;
    }
    let c = Some(ClientRef::create(socket));
    if server_acl_join(c.as_ref().expect("live client")) == 0 {
        c.as_ref()
            .expect("live client")
            .exit_with_message(CString::new("access not allowed").unwrap(), Some(1));
    }
}
pub unsafe fn server_add_accept(mut timeout: ::core::ffi::c_int) {
    let tv = Duration::from_secs(timeout as u64);
    drop(server_accept_task.take());
    let Some(socket) = server_fd.as_ref() else {
        return;
    };
    if timeout == 0 as ::core::ffi::c_int {
        crate::src::reactor::task_start(&mut server_accept_task, move || {
            let source = tiri_rt::mio::Listener::new(socket.try_clone()?)?;
            Ok(async move {
                let accepted = source.accept().await;
                unsafe { server_accept(accepted) };
            })
        })
        .expect("start accept wait");
    } else {
        let now = std::time::Instant::now();
        let deadline = now.checked_add(tv).unwrap_or(now);
        crate::src::reactor::task_start(&mut server_accept_task, move || {
            let wait = tiri_rt::mio::Sleep::new(deadline);
            Ok(async move {
                wait.await.expect("accept backoff wait");
                unsafe { server_add_accept(0) };
            })
        })
        .expect("start accept backoff");
    };
}
unsafe fn server_signal(sig: ProcessSignal) {
    let _fd: ::core::ffi::c_int = 0;
    log_debug(format_args!(
        "{}: {}",
        "server_signal",
        log_cstr(CStr::from_ptr(strsignal(sig.as_raw())))
    ));
    match sig {
        ProcessSignal::Interrupt | ProcessSignal::Terminate => {
            server_exit = 1 as ::core::ffi::c_int;
            server_send_exit();
        }
        ProcessSignal::Child => {
            server_child_signal();
        }
        ProcessSignal::User1 => {
            drop(server_accept_task.take());
            if let Ok(listener) = server_create_listener(server_client_flags) {
                server_fd = Some(listener);
                server_update_socket();
            }
            server_add_accept(0 as ::core::ffi::c_int);
        }
        ProcessSignal::User2 => {
            proc_toggle_log(server_proc);
        }
        _ => {}
    };
}
unsafe fn server_child_signal() {
    let mut status: ::core::ffi::c_int = 0;
    let mut pid: pid_t = 0;
    loop {
        pid = waitpid(WAIT_ANY, &raw mut status, WNOHANG | WUNTRACED) as pid_t;
        match pid {
            -1 => {
                if *__errno_location() == ECHILD {
                    return;
                }
                fatal(|out| out.write_all(b"waitpid failed"));
            }
            0 => return,
            _ => {}
        }
        let status = ExitStatus::from_raw(status);
        if status.stopped_signal().is_some() {
            server_child_stopped(pid, status);
        } else if status.code().is_some() || status.signal().is_some() {
            server_child_exited(pid, status);
        }
    }
}
unsafe fn server_child_exited(pid: pid_t, status: ExitStatus) {
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        window_cursor = window_owner.next_window().upgrade();
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            if pane_owner.process_exited(pid, status) {
                break;
            }
        }
        drop(window_owner);
    }
    job_check_died(pid, status);
}
unsafe fn server_child_stopped(pid: pid_t, status: ExitStatus) {
    if matches!(status.stopped_signal(), Some(SIGTTIN | SIGTTOU)) {
        return;
    }
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        for pane_owner in window_owner
            .pane_snapshot()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            if pane_owner.process_id() == pid && killpg(pid as __pid_t, SIGCONT) != 0 {
                kill(pid as __pid_t, SIGCONT);
            }
        }
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
    job_check_died(pid, status);
}
pub unsafe fn server_add_message(
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let mut limit: u_int = 0;
    let s = format_message_with(write);
    log_debug(format_args!("message: {}", log_cstr(&s)));
    let fresh0 = message_next;
    message_next = message_next.wrapping_add(1);
    let msg_time = SystemTime::now();
    message_log.push_back(s, fresh0, msg_time);
    limit = OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"message-limit"))
        as u_int;
    message_log.trim(message_next, limit);
}
