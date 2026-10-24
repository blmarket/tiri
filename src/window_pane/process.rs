//! Pane-owned process and pipe lifecycle operations.
use super::{window_pane_destroy_ready, window_pane_exited};
use crate::arguments::{args_count, args_has, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_target_client};
use crate::format::{format_create, format_defaults, format_expand_time_cstring};
use crate::log::{fatalx, log_debug};
use crate::proc::proc_clear_signals;
use crate::reactor::BufferEvent;
use crate::reactor::{
    bufferevent_enable, bufferevent_get_input, bufferevent_new, bufferevent_write, evbuffer_drain,
    evbuffer_get_length, evbuffer_pullup,
};
use crate::server::server_proc;
use crate::server_fn::server_destroy_pane;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmdq_item};
use crate::shared::errno::ErrorText;
use crate::shared::event::*;
use crate::shared::event::{EV_READ, EV_WRITE};
use crate::shared::format::FORMAT_NONE;
use crate::shared::pane::window_pane;
use crate::shared::posix_io::{_PATH_BSHELL, _PATH_DEVNULL, O_WRONLY, STDERR_FILENO};
use crate::shared::session::SessionRef;
use crate::shared::window::winlink;
use crate::tmux::setblocking;
use std::os::fd::AsFd;
use tiri_sys::{
    block_signals, close_on_exec_from, exec_with_stdio, exit_now, fork, lead_process_group,
    set_signal_mask, Program,
};
/// Start or stop piping the pane's output through a shell command. The
/// server is single-threaded, so the forked child may run ordinary code until
/// it execs.
pub(super) fn pipe_pane(
    pane_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    {
        let item = item_handle.borrow();
        let queue_client = cmdq_get_client(Some(&*item));
        let cmd_ref = self_0.try_borrow().expect("live command");
        let args = cmd_get_args(&cmd_ref);
        let target = crate::cmd::queue::cmdq_get_target(&item);
        let tc_owner = cmdq_get_target_client(Some(&*item));
        let mut tc: Option<ClientRef> = tc_owner.clone();
        let mut s: Option<SessionRef> = target.session_handle();
        let mut wl: refbox::Weak<winlink> = target.winlink_handle();
        let had_pipe = pane_owner.borrow().pipe_event.is_alive();
        let mut in_0: ::core::ffi::c_int = 0;
        let mut out: ::core::ffi::c_int = 0;
        if window_pane_exited(&pane_owner.borrow()) != 0 {
            cmdq_error(item_handle, |out| out.write_all(b"target pane has exited"));
            return CmdOutcome::Error;
        }
        if had_pipe {
            let pipe_event = std::mem::take(&mut pane_owner.borrow_mut().pipe_event);
            pipe_event.free();
            if window_pane_destroy_ready(pane_owner) != 0 {
                server_destroy_pane(pane_owner, 1);
                return CmdOutcome::Normal;
            }
        }
        if args_count(args) == 0 as u_int
            || args_string(args, 0 as u_int).is_none_or(std::ffi::CStr::is_empty)
        {
            return CmdOutcome::Normal;
        }
        if args_has(args, 'o' as i32 as u_char) != 0 && had_pipe {
            return CmdOutcome::Normal;
        }
        if args_has(args, 'I' as i32 as u_char) != 0 {
            in_0 = 1 as ::core::ffi::c_int;
            out = args_has(args, 'O' as i32 as u_char);
        } else {
            in_0 = 0 as ::core::ffi::c_int;
            out = 1 as ::core::ffi::c_int;
        }
        let (parent, child) = match tiri_sys::socket_pair() {
            Ok(pair) => pair,
            Err(error) => {
                cmdq_error(item_handle, |out| write!(out, "socketpair error: {error}"));
                return CmdOutcome::Error;
            }
        };
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            Some(item_handle),
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_defaults(ft, tc.as_ref(), s.as_ref(), wl.clone(), Some(pane_owner));
        let cmd = format_expand_time_cstring(ft, args_string(args, 0 as u_int).unwrap_or(c""));
        drop(ft_owner);
        let oldset = block_signals();
        let forked = fork();
        pane_owner.borrow_mut().pipe_pid = match forked {
            Ok(Some(pid)) => pid,
            Ok(None) => 0,
            Err(_) => -1,
        };
        match forked {
            Err(error) => {
                set_signal_mask(&oldset);
                cmdq_error(item_handle, |out| {
                    out.write_all(b"fork error: ")?;
                    write!(out, "{}", ErrorText(&error))
                });
                CmdOutcome::Error
            }
            Ok(None) => {
                proc_clear_signals(&server_proc(), 1 as ::core::ffi::c_int);
                set_signal_mask(&oldset);
                drop(parent);
                if lead_process_group().is_err() {
                    exit_now(1 as ::core::ffi::c_int);
                }
                let Ok(null) = tiri_sys::open(_PATH_DEVNULL, O_WRONLY, 0) else {
                    exit_now(1 as ::core::ffi::c_int);
                };
                let stdin = if out != 0 {
                    child.as_fd()
                } else {
                    null.as_fd()
                };
                let stdout = if in_0 != 0 {
                    child.as_fd()
                } else {
                    null.as_fd()
                };
                close_on_exec_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
                exec_with_stdio(
                    [stdin, stdout, null.as_fd()],
                    Program::Path(_PATH_BSHELL, &[c"sh", c"-c", &cmd]),
                );
            }
            Ok(Some(_)) => {
                set_signal_mask(&oldset);
                drop(child);
                setblocking(parent.as_fd(), 0 as ::core::ffi::c_int);
                let read_observer = std::rc::Rc::downgrade(pane_owner);
                let write_observer = read_observer.clone();
                let error_observer = read_observer.clone();
                let stream = bufferevent_new(
                    Some(parent),
                    bufferevent_data_callback(move || {
                        if let Some(owner) = read_observer.upgrade() {
                            cmd_pipe_pane_read_callback(&owner);
                        }
                    }),
                    bufferevent_data_callback(move || {
                        if let Some(owner) = write_observer.upgrade() {
                            cmd_pipe_pane_write_callback(&owner);
                        }
                    }),
                    bufferevent_event_callback(move |_, _| {
                        if let Some(owner) = error_observer.upgrade() {
                            cmd_pipe_pane_error_callback(&owner);
                        }
                    }),
                )
                .unwrap_or_else(|_| fatalx(|out| out.write_all(b"out of memory")));
                {
                    let mut pane = pane_owner.borrow_mut();
                    pane.pipe_offset = pane.offset;
                }
                let pipe_event = stream;
                pane_owner.borrow_mut().pipe_event = pipe_event.clone();
                if out != 0 {
                    let _ = pipe_event.with_stream(|event| {
                        bufferevent_enable(event, EV_WRITE as ::core::ffi::c_short);
                    });
                }
                if in_0 != 0 {
                    let _ = pipe_event.with_stream(|event| {
                        bufferevent_enable(event, EV_READ as ::core::ffi::c_short);
                    });
                }
                CmdOutcome::Normal
            }
        }
    }
}
fn cmd_pipe_pane_read_callback(pane_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>) {
    let (pipe, output, id) = {
        let pane = pane_owner.borrow();
        (pane.pipe_event.clone(), pane.event.clone(), pane.id)
    };
    if !pipe.is_alive() {
        return;
    }
    let data = pipe
        .with_stream(|event| {
            let evb = &mut *event.input;
            let available = evbuffer_get_length(evb);
            evbuffer_pullup(evb, -1).map_or_else(Vec::new, |bytes| bytes[..available].to_vec())
        })
        .unwrap_or_default();
    let available = data.len();
    log_debug(format_args!("%{} pipe read {}", id, { available }));
    let _ = output.with_stream(|event| {
        bufferevent_write(event, &data[..available]);
    });
    let _ = pipe.with_stream(|event| {
        evbuffer_drain(bufferevent_get_input(&mut *event), available);
    });
    if window_pane_destroy_ready(pane_owner) != 0 {
        server_destroy_pane(pane_owner, 1);
    }
}
fn cmd_pipe_pane_write_callback(pane_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>) {
    log_debug(format_args!("%{} pipe empty", pane_owner.borrow().id));
    if window_pane_destroy_ready(pane_owner) != 0 {
        server_destroy_pane(pane_owner, 1);
    }
}
fn cmd_pipe_pane_error_callback(pane_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>) {
    let (id, pipe_event) = {
        let mut pane = pane_owner.borrow_mut();
        (pane.id, std::mem::take(&mut pane.pipe_event))
    };
    log_debug(format_args!("%{} pipe error", id));
    pipe_event.free();
    if window_pane_destroy_ready(pane_owner) != 0 {
        server_destroy_pane(pane_owner, 1);
    }
}

#[cfg(test)]
mod pipe_stream_tests {
    use super::*;
    use crate::reactor::{evbuffer_add, shutdown_runtime};
    use std::os::fd::AsRawFd;

    #[test]
    fn pipe_error_closes_descriptor_while_pane_remains_alive() {
        use std::os::unix::net::UnixStream;
        use tiri_rt::Runtime as _;

        let runtime = tiri_rt::mio::Runtime::new().unwrap();
        {
            let pane = window_pane::new();
            let (socket, _receiver) = UnixStream::pair().unwrap();
            let fd = socket.as_raw_fd();
            let stream = bufferevent_new(Some(socket.into()), None, None, None).unwrap();
            pane.borrow_mut().pipe_event = stream;
            let observer = pane.borrow().pipe_event.clone();

            cmd_pipe_pane_error_callback(&pane);

            assert!(!pane.borrow().pipe_event.is_alive());
            assert!(!observer.is_alive());
            assert!(!tiri_sys::is_open(fd));
            let replacement = std::fs::File::open("/dev/null").unwrap();
            cmd_pipe_pane_error_callback(&pane);
            assert!(tiri_sys::is_open(replacement.as_raw_fd()));
            shutdown_runtime(runtime);
        }
    }

    #[test]
    fn pipe_read_forwards_bytes_and_stream_handle_expires() {
        let pane_owner = window_pane::new();
        let main = { bufferevent_new(None, None, None, None) }.unwrap();
        let pipe = { bufferevent_new(None, None, None, None) }.unwrap();
        let stale = {
            let mut wp = pane_owner.borrow_mut();
            wp.fd = None;
            wp.event = main;
            wp.pipe_event = pipe;
            wp.pipe_event.clone()
        };
        let bytes = b"pipe output";
        stale.with_stream(|stream| evbuffer_add(&mut stream.input, bytes));

        cmd_pipe_pane_read_callback(&pane_owner);

        assert_eq!(
            pane_owner
                .borrow()
                .event
                .with_stream(|event| evbuffer_get_length(&(*event).output)),
            Some(bytes.len())
        );
        assert_eq!(
            stale.with_stream(|stream| evbuffer_get_length(&stream.input)),
            Some(0)
        );
        let pipe_event = std::mem::take(&mut pane_owner.borrow_mut().pipe_event);
        pipe_event.free();
        assert!(!stale.is_alive());
        assert!(!pane_owner.borrow().pipe_event.is_alive());
        let event = std::mem::take(&mut pane_owner.borrow_mut().event);
        event.free();
        drop(pane_owner);
    }
}
