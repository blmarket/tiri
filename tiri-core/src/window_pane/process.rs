//! Pane-owned process and pipe lifecycle operations.
use super::{window_pane_destroy_ready, window_pane_exited};
use crate::actions::Target;
use crate::arguments::{args_count, args_has, args_string};
use crate::format::bytes::format_message_with;
use crate::format::{format_create, format_defaults, format_expand_time_cstring};
use crate::log::{fatalx, log_debug};
use crate::proc::proc_clear_signals;
use crate::reactor::BufferEvent;
use crate::reactor::{bufferevent_enable, bufferevent_get_input, bufferevent_new, bufferevent_write};
use crate::server::server_proc;
use crate::server_fn::server_destroy_pane;
use crate::shared::abi::{u_char, u_int};
use crate::shared::client::ClientRef;
use crate::shared::errno::ErrorText;
use crate::shared::event::{bufferevent_data_callback, bufferevent_event_callback};
use crate::shared::event::{EV_READ, EV_WRITE};
use crate::shared::format::{CommandFormat, FORMAT_NONE};
use crate::shared::pane::window_pane;
use crate::shared::posix_io::{_PATH_BSHELL, _PATH_DEVNULL, O_WRONLY, STDERR_FILENO};
use crate::shared::session::SessionRef;
use crate::shared::window::winlink;
use crate::tmux::setblocking;
use std::ffi::CString;
use std::os::fd::AsFd;
use tiri_sys::{
    block_signals, close_on_exec_from, exec_with_stdio, exit_now, fork, lead_process_group,
    set_signal_mask, Program,
};

pub(super) fn pipe_pane(
    pane_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    args: &crate::arguments::args,
    cmd_format: Option<&CommandFormat>,
    tc: Option<&ClientRef>,
    target: &Target,
) -> Result<(), CString> {
    {
        let queue_client = cmd_format.and_then(|cf| cf.client.upgrade());
        let s: Option<SessionRef> = target.session_handle();
        let wl: refbox::Weak<winlink> = target.winlink_handle();
        let had_pipe = pane_owner.borrow().pipe_event.is_alive();
        let in_0: ::core::ffi::c_int;
        let out: ::core::ffi::c_int;
        if window_pane_exited(&pane_owner.borrow()) != 0 {
            return Err(c"target pane has exited".to_owned());
        }
        if had_pipe {
            let pipe_event = std::mem::take(&mut pane_owner.borrow_mut().pipe_event);
            pipe_event.free();
            if window_pane_destroy_ready(pane_owner) != 0 {
                server_destroy_pane(pane_owner, 1);
                return Ok(());
            }
        }
        if args_count(args) == 0 as u_int
            || args_string(args, 0 as u_int).is_none_or(std::ffi::CStr::is_empty)
        {
            return Ok(());
        }
        if args_has(args, 'o' as i32 as u_char) != 0 && had_pipe {
            return Ok(());
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
                return Err(format_message_with(|out| {
                    write!(out, "socketpair error: {error}")
                }));
            }
        };
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            cmd_format,
            FORMAT_NONE,
            0 as ::core::ffi::c_int,
        );
        let ft = &mut *ft_owner;
        format_defaults(ft, tc, s.as_ref(), wl, Some(pane_owner));
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
                Err(format_message_with(|out| {
                    out.write_all(b"fork error: ")?;
                    write!(out, "{}", ErrorText(&error))
                }))
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
                Ok(())
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
        .with_stream(|event| event.input.copy_prefix(event.input.len()))
        .unwrap_or_default();
    let available = data.len();
    log_debug(format_args!("%{} pipe read {}", id, { available }));
    let _ = output.with_stream(|event| {
        bufferevent_write(event, &data[..available]);
    });
    let _ = pipe.with_stream(|event| {
        let input = bufferevent_get_input(&mut *event);
        input.advance(available.min(input.len()));
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
    use super::{
        BufferEvent, bufferevent_new, cmd_pipe_pane_error_callback, cmd_pipe_pane_read_callback,
        window_pane,
    };
    use crate::reactor::shutdown_runtime;
    use std::os::fd::AsRawFd;

    #[test]
    fn pipe_error_closes_descriptor_while_pane_remains_alive() {
        use std::os::unix::net::UnixStream;

        let runtime = tiri_rt::Runtime::new().unwrap();
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
        stale.with_stream(|stream| stream.input.extend_from_slice(bytes));

        cmd_pipe_pane_read_callback(&pane_owner);

        assert_eq!(
            pane_owner
                .borrow()
                .event
                .with_stream(|event| event.output.len()),
            Some(bytes.len())
        );
        assert_eq!(
            stale.with_stream(|stream| stream.input.len()),
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
