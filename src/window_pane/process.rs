//! Pane-owned process and pipe lifecycle operations.
use super::{window_pane_destroy_ready, window_pane_exited};
use crate::src::arguments::{args, args_count, args_has, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_target_client};
use crate::src::ffi::libc::__errno_location;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{format_create_with_client, format_defaults, format_expand_time_cstring};
use crate::src::log::{fatalx, log_debug};
use crate::src::proc::proc_clear_signals;
use crate::src::reactor::BufferEvent;
use crate::src::reactor::{
    bufferevent_enable, bufferevent_get_input, bufferevent_new, bufferevent_write, evbuffer_drain,
    evbuffer_get_length, evbuffer_pullup,
};
use crate::src::server::server_proc;
use crate::src::server_fn::server_destroy_pane;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_find_state, cmdq_item};
use crate::src::shared::event::*;
use crate::src::shared::event::{EV_READ, EV_WRITE};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::FORMAT_NONE;
use crate::src::shared::pane::window_pane;
use crate::src::shared::pane::window_pane_offset;
use crate::src::shared::posix_io::{
    _PATH_BSHELL, _PATH_DEVNULL, O_WRONLY, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO,
};
use crate::src::shared::session::SessionRef;
use crate::src::shared::signal::{sigset_t, SIG_BLOCK, SIG_SETMASK};
use crate::src::shared::window::winlink;
use crate::src::tmux::setblocking;
use libc::{_exit, execl, fork, memcpy, setpgid, sigfillset, sigprocmask, strerror};
use std::os::fd::{AsRawFd, IntoRawFd};
pub(super) unsafe fn pipe_pane(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let wp = pane_owner.get();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut wpo: *mut window_pane_offset = &raw mut (*wp).pipe_offset;
    let had_pipe = (*wp).pipe_fd.is_some();
    let mut null_fd: ::core::ffi::c_int = 0;
    let mut in_0: ::core::ffi::c_int = 0;
    let mut out: ::core::ffi::c_int = 0;
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut set: sigset_t = unsafe { ::core::mem::zeroed() };
    let mut oldset: sigset_t = unsafe { ::core::mem::zeroed() };
    if window_pane_exited(&*wp) != 0 {
        cmdq_error(item_handle, |out| out.write_all(b"target pane has exited"));
        return CmdOutcome::Error;
    }
    if (*wp).pipe_fd.is_some() {
        std::mem::take(&mut (*wp).pipe_event).free();
        drop((*wp).pipe_fd.take());
        if window_pane_destroy_ready(pane_owner) != 0 {
            server_destroy_pane(pane_owner, 1);
            return CmdOutcome::Normal;
        }
    }
    if args_count(&*args) == 0 as u_int
        || *args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr())
            as ::core::ffi::c_int
            == '\0' as i32
    {
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'o' as i32 as u_char) != 0 && had_pipe {
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'I' as i32 as u_char) != 0 {
        in_0 = 1 as ::core::ffi::c_int;
        out = args_has(&*args, 'O' as i32 as u_char);
    } else {
        in_0 = 0 as ::core::ffi::c_int;
        out = 1 as ::core::ffi::c_int;
    }
    let (parent, child) = match tiri_rt::unix::socket_pair() {
        Ok(pair) => pair,
        Err(error) => {
            cmdq_error(item_handle, |out| write!(out, "socketpair error: {error}"));
            return CmdOutcome::Error;
        }
    };
    let mut ft_owner = format_create_with_client(
        queue_client.as_ref(),
        Some(item_handle),
        FORMAT_NONE,
        0 as ::core::ffi::c_int,
    );
    ft = &raw mut *ft_owner;
    format_defaults(ft, tc.as_ref(), s.as_ref(), wl.clone(), Some(pane_owner));
    let cmd = format_expand_time_cstring(
        ft,
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
    );
    drop(ft_owner);
    sigfillset(&raw mut set);
    sigprocmask(SIG_BLOCK, &raw mut set, &raw mut oldset);
    (*wp).pipe_pid = fork() as pid_t;
    match (*wp).pipe_pid {
        -1 => {
            sigprocmask(
                SIG_SETMASK,
                &raw mut oldset,
                ::core::ptr::null_mut::<sigset_t>(),
            );
            cmdq_error(item_handle, |out| {
                out.write_all(b"fork error: ")?;
                write_cstr(out, nullable_cstr(strerror(*__errno_location())))
            });
            CmdOutcome::Error
        }
        0 => {
            proc_clear_signals(server_proc, 1 as ::core::ffi::c_int);
            sigprocmask(
                SIG_SETMASK,
                &raw mut oldset,
                ::core::ptr::null_mut::<sigset_t>(),
            );
            drop(parent);
            let child_fd = child.into_raw_fd();
            if setpgid(0 as __pid_t, 0 as __pid_t) == -(1 as ::core::ffi::c_int) {
                _exit(1 as ::core::ffi::c_int);
            }
            null_fd = match tiri_rt::unix::open(_PATH_DEVNULL, O_WRONLY, 0) {
                Ok(fd) => fd.into_raw_fd(),
                Err(error) => crate::src::reactor::io_status(Err(error)),
            };
            if out != 0 {
                if tiri_rt::unix::redirect(
                    std::os::fd::BorrowedFd::borrow_raw(child_fd),
                    STDIN_FILENO,
                )
                .is_err()
                {
                    _exit(1 as ::core::ffi::c_int);
                }
            } else if tiri_rt::unix::redirect(
                std::os::fd::BorrowedFd::borrow_raw(null_fd),
                STDIN_FILENO,
            )
            .is_err()
            {
                _exit(1 as ::core::ffi::c_int);
            }
            if in_0 != 0 {
                if tiri_rt::unix::redirect(
                    std::os::fd::BorrowedFd::borrow_raw(child_fd),
                    STDOUT_FILENO,
                )
                .is_err()
                {
                    _exit(1 as ::core::ffi::c_int);
                }
                if child_fd != STDOUT_FILENO {
                    let _ = tiri_rt::unix::close(child_fd);
                }
            } else if tiri_rt::unix::redirect(
                std::os::fd::BorrowedFd::borrow_raw(null_fd),
                STDOUT_FILENO,
            )
            .is_err()
            {
                _exit(1 as ::core::ffi::c_int);
            }
            if tiri_rt::unix::redirect(std::os::fd::BorrowedFd::borrow_raw(null_fd), STDERR_FILENO)
                .is_err()
            {
                _exit(1 as ::core::ffi::c_int);
            }
            tiri_rt::unix::close_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
            execl(
                _PATH_BSHELL.as_ptr(),
                c"sh".as_ptr(),
                c"-c".as_ptr(),
                cmd.as_ptr(),
                NULL as *mut ::core::ffi::c_char,
            );
            _exit(1 as ::core::ffi::c_int);
        }
        _ => {
            sigprocmask(
                SIG_SETMASK,
                &raw mut oldset,
                ::core::ptr::null_mut::<sigset_t>(),
            );
            drop(child);
            (*wp).pipe_fd = Some(parent);
            memcpy(
                wpo as *mut ::core::ffi::c_void,
                &raw mut (*wp).offset as *const ::core::ffi::c_void,
                ::core::mem::size_of::<window_pane_offset>() as size_t,
            );
            setblocking(
                (*wp).pipe_fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                0 as ::core::ffi::c_int,
            );
            let read_observer = std::rc::Rc::downgrade(pane_owner);
            let write_observer = read_observer.clone();
            let error_observer = read_observer.clone();
            let stream = bufferevent_new(
                (*wp).pipe_fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                bufferevent_data_callback(move |_| unsafe {
                    if let Some(owner) = read_observer.upgrade() {
                        cmd_pipe_pane_read_callback(&owner);
                    }
                }),
                bufferevent_data_callback(move |_| unsafe {
                    if let Some(owner) = write_observer.upgrade() {
                        cmd_pipe_pane_write_callback(&owner);
                    }
                }),
                bufferevent_event_callback(move |_, _| unsafe {
                    if let Some(owner) = error_observer.upgrade() {
                        cmd_pipe_pane_error_callback(&owner);
                    }
                }),
            );
            if stream.is_null() {
                fatalx(|out| out.write_all(b"out of memory"));
            }
            (*wp).pipe_event = crate::src::reactor::StreamHandle::from_ptr(stream);
            if out != 0 {
                let _ = (*wp).pipe_event.with_ptr(|event| unsafe {
                    bufferevent_enable(event, EV_WRITE as ::core::ffi::c_short);
                });
            }
            if in_0 != 0 {
                let _ = (*wp).pipe_event.with_ptr(|event| unsafe {
                    bufferevent_enable(event, EV_READ as ::core::ffi::c_short);
                });
            }
            CmdOutcome::Normal
        }
    }
}
unsafe fn cmd_pipe_pane_read_callback(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let (pipe, output, id) = {
        let pane = &*pane_owner.get();
        (pane.pipe_event.clone(), pane.event.clone(), pane.id)
    };
    if !pipe.is_alive() {
        return;
    }
    let data = pipe
        .with_ptr(|event| unsafe {
            let evb = &mut *(*event).input;
            let available = evbuffer_get_length(evb);
            evbuffer_pullup(evb, -1).map_or_else(Vec::new, |bytes| bytes[..available].to_vec())
        })
        .unwrap_or_default();
    let available = data.len();
    log_debug(format_args!("%{} pipe read {}", id, { available }));
    let _ = output.with_ptr(|event| unsafe {
        bufferevent_write(event, &data[..available]);
    });
    let _ = pipe.with_ptr(|event| unsafe {
        evbuffer_drain(bufferevent_get_input(&mut *event), available);
    });
    if window_pane_destroy_ready(pane_owner) != 0 {
        server_destroy_pane(pane_owner, 1);
    }
}
unsafe fn cmd_pipe_pane_write_callback(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let wp = pane_owner.get();
    log_debug(format_args!("%{} pipe empty", { (*wp).id }));
    if window_pane_destroy_ready(pane_owner) != 0 {
        server_destroy_pane(pane_owner, 1);
    }
}
unsafe fn cmd_pipe_pane_error_callback(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let wp = pane_owner.get();
    log_debug(format_args!("%{} pipe error", { (*wp).id }));
    std::mem::take(&mut (*wp).pipe_event).free();
    drop((*wp).pipe_fd.take());
    if window_pane_destroy_ready(pane_owner) != 0 {
        server_destroy_pane(pane_owner, 1);
    }
}

#[cfg(test)]
mod pipe_stream_tests {
    use super::*;
    use crate::src::reactor::{evbuffer_add, shutdown_runtime};

    #[test]
    fn pipe_error_closes_descriptor_while_pane_remains_alive() {
        use std::os::unix::net::UnixStream;
        use tiri_rt::Runtime as _;

        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let pane = window_pane::new();
            let (socket, _receiver) = UnixStream::pair().unwrap();
            let fd = socket.as_raw_fd();
            (*pane.get()).pipe_fd = Some(socket.into());
            let stream = bufferevent_new(fd, None, None, None);
            (*pane.get()).pipe_event = crate::src::reactor::StreamHandle::from_ptr(stream);
            let observer = (*pane.get()).pipe_event.clone();

            cmd_pipe_pane_error_callback(&pane);

            assert!((*pane.get()).pipe_fd.is_none());
            assert!(!observer.is_alive());
            assert_eq!(libc::fcntl(fd, libc::F_GETFD), -1);
            let replacement = std::fs::File::open("/dev/null").unwrap();
            cmd_pipe_pane_error_callback(&pane);
            assert!(libc::fcntl(replacement.as_raw_fd(), libc::F_GETFD) >= 0);
            shutdown_runtime(runtime);
        }
    }

    #[test]
    fn pipe_read_forwards_bytes_and_stream_handle_expires() {
        unsafe {
            let pane_owner = window_pane::new();
            let wp = pane_owner.get();
            (*wp).fd = None;
            (*wp).pipe_fd = None;
            let main = bufferevent_new(-1, None, None, None);
            (*wp).event = crate::src::reactor::StreamHandle::from_ptr(main);
            let pipe = bufferevent_new(-1, None, None, None);
            (*wp).pipe_event = crate::src::reactor::StreamHandle::from_ptr(pipe);
            let stale = (*wp).pipe_event.clone();
            let bytes = b"pipe output";
            evbuffer_add(&mut (*pipe).input, bytes);

            cmd_pipe_pane_read_callback(&pane_owner);

            assert_eq!(
                (*wp)
                    .event
                    .with_ptr(|event| evbuffer_get_length(&(*event).output)),
                Some(bytes.len())
            );
            assert_eq!(evbuffer_get_length(&(*pipe).input), 0);
            std::mem::take(&mut (*wp).pipe_event).free();
            assert!(!stale.is_alive());
            assert!(!(*wp).pipe_event.is_alive());
            std::mem::take(&mut (*wp).event).free();
            drop(pane_owner);
        }
    }
}
