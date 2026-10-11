//! Pane process creation and editor completion keep storage inside its owner.
use super::{
    window_pane_create, window_pane_index, window_pane_reset_mode_all, window_pane_set_cwd,
    window_pane_set_event, window_pane_set_shell,
};
use crate::src::cmd::queue::cmdq_get_client;
use crate::src::cmd::{cmd_log_argv, cmd_stringify_argv_cstring};
use crate::src::compat::fdforkpty::fdforkpty;
use crate::src::compat::stdio::CFile;
use crate::src::compat::systemd::systemd_move_to_new_cgroup;
use crate::src::control::control_reset_pane;
use crate::src::environ::{
    environ_copy, environ_for_session, environ_log, environ_push, environ_set,
};
use crate::src::events::{events_fire, events_fire_window};
use crate::src::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_session,
    event_payload_set_string, event_payload_set_target, event_payload_set_window,
};
use crate::src::ffi::libc::{
    __errno_location, _exit, chdir, execl, execvp, fopen, fread, fseeko, ftello, getcwd, getpid,
    kill, memset, sigfillset, sigprocmask, strerror, strrchr,
};
use crate::src::ffi::utempter::utempter_add_record;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_cstring;
use crate::src::input::input_free;
use crate::src::log::{log_close, log_cstr, log_debug};
use crate::src::options::{options_get_number, options_get_string, options_set_number};
use crate::src::proc::proc_clear_signals;
use crate::src::reactor::BufferEvent;
use crate::src::screen::screen_reinit;
use crate::src::server::clients;
use crate::src::server::server_proc;
use crate::src::server_client::Client;
use crate::src::window::Window as _;

use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::spawn::spawn_context;
use crate::src::shared::window::WindowRef;
use crate::src::tmux::{checkshell, find_home_cstr, global_options, ptm_fd};

use crate::src::window_pane::WindowPane as _;
use std::ffi::{CStr, CString};

use crate::src::shared::abi::__off_t;
use crate::src::shared::abi::*;
use crate::src::shared::client::CLIENT_CONTROL;
use crate::src::shared::command::{cmd_find_state, cmdq_item};
use crate::src::shared::key::*;
use crate::src::shared::limits::SIZE_MAX;
use crate::src::shared::pane::window_pane;
use crate::src::shared::pane::{PANE_EMPTY, PANE_EXITED, PANE_STATUSDRAWN, PANE_STATUSREADY};
use crate::src::shared::posix_io::{_PATH_BSHELL, STDERR_FILENO, STDIN_FILENO};
use crate::src::shared::posix_terminal::{winsize, TCSANOW, VERASE};
use crate::src::shared::screen::{MODE_CRLF, MODE_CURSOR};
use crate::src::shared::signal::{__sigset_t, sigset_t, SIGCHLD, SIGHUP, SIG_BLOCK, SIG_SETMASK};
use crate::src::shared::spawn::spawn_editor_state;
use crate::src::shared::spawn::{
    SPAWN_BEFORE, SPAWN_DETACHED, SPAWN_EMPTY, SPAWN_KILL, SPAWN_NONOTIFY, SPAWN_RESPAWN,
};
use crate::src::shared::stdio::FILE;
use crate::src::shared::terminal::*;

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use crate::src::shared::spawn::EditorHandle;
use crate::src::spawn::{
    off_t, set_spawn_cause, spawn_log, uintmax_t, _PATH_DEFPATH, IUTF8, SEEK_END, SEEK_SET,
};
unsafe fn spawn_fire_pane_created(
    mut sc: *mut spawn_context,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let mut wp = wp_owner.get();
    let session_owner = (*sc).s.upgrade();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut cwd: *const ::core::ffi::c_char = (*wp)
        .cwd
        .as_ref()
        .map_or(::core::ptr::null(), |value| value.as_ptr());
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_winlink_pane(
        ((*sc).winlink_handle()).clone(),
        wp_owner,
        0 as ::core::ffi::c_int,
    );
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_session(
        &mut ep,
        c"session",
        session_owner.expect("spawn context session"),
    );
    event_payload_set_window(
        &mut ep,
        c"window",
        WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_int(
        &mut ep,
        c"window_index",
        ((*sc).winlink_handle()).get_unchecked().idx,
    );
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    let cmd = if !(*wp).argv.is_empty() {
        cmd_stringify_argv_cstring(&(*wp).argv)
    } else {
        None
    };
    if let Some(cmd) = cmd.as_ref().filter(|text| !text.as_bytes().is_empty()) {
        event_payload_set_string(&mut ep, c"pane_command", |out| {
            write_cstr(out, (&*cmd).as_c_str())
        });
    } else if (*wp).shell.is_some() {
        event_payload_set_string(&mut ep, c"pane_command", |out| {
            write_cstr(out, (*wp).shell.as_deref())
        });
    }
    if !cwd.is_null() {
        event_payload_set_string(&mut ep, c"pane_current_path", |out| {
            write_cstr(out, nullable_cstr(cwd))
        });
    }
    if (*sc).flags & SPAWN_EMPTY != 0 {
        event_payload_set_int(&mut ep, c"created_empty", 1 as ::core::ffi::c_int);
    } else {
        event_payload_set_int(&mut ep, c"created_empty", 0 as ::core::ffi::c_int);
    }
    if (*sc).flags & SPAWN_RESPAWN != 0 {
        event_payload_set_int(&mut ep, c"created_respawn", 1 as ::core::ffi::c_int);
    } else {
        event_payload_set_int(&mut ep, c"created_respawn", 0 as ::core::ffi::c_int);
    }
    events_fire(c"pane-created", ep);
}
/// Create a pane beside the context's source pane (before it with
/// SPAWN_BEFORE), or as the first pane of an empty window, and place it. When
/// the layout has no room the pane is destroyed again and the layout's reason
/// returned.
unsafe fn place_new_pane(
    sc: &spawn_context,
    window: &WindowRef,
) -> Result<std::rc::Rc<std::cell::UnsafeCell<window_pane>>, &'static CStr> {
    let mut panes = window.pane_snapshot();
    // A new window's first pane has no neighbour; otherwise the pane goes
    // beside its source.
    let position = if panes.is_empty() {
        0
    } else {
        let source = sc.wp0.upgrade().expect("source pane retained by caller");
        let position = panes
            .iter()
            .position(|candidate| std::rc::Rc::ptr_eq(candidate, &source))
            .expect("source pane belongs to window");
        position + usize::from(sc.flags & SPAWN_BEFORE == 0)
    };
    let hlimit =
        sc.s.upgrade()
            .expect("spawn context session")
            .with_options_mut(|options| options_get_number(options, c"history-limit") as u_int);
    let (sx, sy) = window.size();
    let pane = window_pane_create(window, sx, sy, hlimit);
    panes.insert(position, pane.clone());
    if let Err(reason) = window.rearrange_panes(&panes) {
        pane.destroy();
        return Err(reason);
    }
    Ok(pane)
}
/// Create and place a new pane and start its process, or with SPAWN_RESPAWN
/// restart the context's source pane. A full layout refuses before any side
/// effect.
pub(super) unsafe fn spawn_pane(
    mut sc: *mut spawn_context,
    cause: *mut Option<CString>,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    let source_pane_owner = (*sc).wp0.upgrade();
    let source_pane = source_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let item_owner = (*sc).item.upgrade();
    let mut item: *mut cmdq_item = item_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut c: Option<ClientRef> = None;
    let mut loop_0: Option<ClientRef> = None;
    let session_owner = (*sc).s.upgrade().expect("spawn context session");
    let original_window = (*sc)
        .winlink_handle()
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("spawn window");
    let result = (|| {
        let created_pane = if (*sc).flags & SPAWN_RESPAWN != 0 {
            None
        } else {
            match place_new_pane(&*sc, &original_window) {
                Ok(pane) => Some(pane),
                Err(reason) => {
                    set_spawn_cause(cause.as_mut(), &[reason.to_bytes()]);
                    return None;
                }
            }
        };
        let target_session_owner;
        let new_pane_owner;
        let mut new_wp: *mut window_pane = ::core::ptr::null_mut::<window_pane>();
        let mut cp: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
        let mut argvp: *mut *mut ::core::ffi::c_char =
            ::core::ptr::null_mut::<*mut ::core::ffi::c_char>();
        let mut cwd: Option<CString> = None;
        let mut path: [::core::ffi::c_char; 4096] = [0; 4096];
        let _cmd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut tmp: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut home: *const ::core::ffi::c_char =
            find_home_cstr().map_or(::core::ptr::null(), CStr::as_ptr);
        let mut actual_cwd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut idx: u_int = 0;
        let mut now: termios = termios {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_line: 0,
            c_cc: [0; 32],
            c2rust_unnamed: termios_input_speed { __ispeed: 0 },
            c2rust_unnamed_0: termios_output_speed { __ospeed: 0 },
        };
        let mut ws: winsize = winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let mut set: sigset_t = __sigset_t { __val: [0; 16] };
        let mut oldset: sigset_t = __sigset_t { __val: [0; 16] };
        let mut key: key_code = 0;
        let c_owner;
        if !item.is_null() {
            target_session_owner =
                (*crate::src::cmd::queue::cmdq_get_target_mut(&mut *item)).session_handle();
            c_owner = cmdq_get_client((item).as_ref());
            c = c_owner.clone();
        } else {
            target_session_owner = Some(session_owner.clone());
            c_owner = (*sc).tc.upgrade();
            c = c_owner.clone();
        }
        spawn_log(c"spawn_pane", sc);
        if let Some(requested_cwd) = (*sc).cwd.as_ref() {
            if !item.is_null() {
                cwd = Some(format_single_cstring(
                    item_owner.as_ref(),
                    requested_cwd.as_ptr(),
                    c.as_ref(),
                    target_session_owner.as_ref(),
                    (refbox::Weak::new()).clone(),
                    None,
                ));
            } else {
                cwd = Some(requested_cwd.clone());
            }
            let value = cwd.as_ref().expect("spawn cwd was just set");
            if !value.as_bytes().starts_with(b"/") {
                let base_owner = match c_owner.as_ref() {
                    Some(client) => client.cwd(target_session_owner.as_ref()),
                    None => ClientRef::working_directory(None, target_session_owner.as_ref()),
                };
                // Preserve the old formatter's rendering for an absent startup cwd.
                let base = base_owner
                    .as_deref()
                    .map_or(b"(null)".as_slice(), CStr::to_bytes);
                let mut combined = Vec::with_capacity(base.len() + value.as_bytes().len() + 1);
                combined.extend_from_slice(base);
                if !value.as_bytes().is_empty() {
                    combined.push(b'/');
                }
                combined.extend_from_slice(value.as_bytes());
                cwd = Some(CString::new(combined).expect("combined cwd contains no NUL"));
            }
        } else if !(*sc).flags & SPAWN_RESPAWN != 0 {
            cwd = match c_owner.as_ref() {
                Some(client) => client.cwd(target_session_owner.as_ref()),
                None => ClientRef::working_directory(None, target_session_owner.as_ref()),
            };
        }
        if (*sc).flags & SPAWN_RESPAWN != 0 {
            if (*source_pane).fd.is_some() && !(*sc).flags & SPAWN_KILL != 0 {
                idx = window_pane_index(source_pane_owner.as_ref().expect("respawn pane"))
                    .expect("pane belongs to window ordering");
                set_spawn_cause(
                    cause.as_mut(),
                    &[
                        b"pane ",
                        session_owner.name().as_bytes(),
                        b":",
                        ((*sc).winlink_handle())
                            .get_unchecked()
                            .idx
                            .to_string()
                            .as_bytes(),
                        b".",
                        idx.to_string().as_bytes(),
                        b" still active",
                    ],
                );
                return None;
            }
            std::mem::take(&mut (*source_pane).event).free();
            drop((*source_pane).fd.take());
            window_pane_reset_mode_all(source_pane_owner.as_ref().expect("respawn source pane"));
            screen_reinit(&mut (*source_pane).base, 0 as ::core::ffi::c_int);
            if let Some(ictx) = (*source_pane).ictx.take() {
                input_free(ictx);
            }
            (*source_pane).offset.used = 0 as size_t;
            (*source_pane).base_offset = 0 as size_t;
            (*source_pane).pipe_offset.used = 0 as size_t;
            let mut registry_loop_0_owner = clients.first();
            loop_0 = registry_loop_0_owner.clone();
            while !loop_0.is_none() {
                if loop_0.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
                    control_reset_pane(
                        &loop_0.clone().expect("live client"),
                        source_pane_owner.as_ref().expect("respawn pane"),
                    );
                }
                registry_loop_0_owner = clients.next(
                    registry_loop_0_owner
                        .as_ref()
                        .expect("current registry client"),
                );
                loop_0 = registry_loop_0_owner.clone();
            }
            new_pane_owner = source_pane_owner.as_ref().expect("respawn pane").clone();
            new_wp = new_pane_owner.get();
            (*new_wp).flags &= !(PANE_STATUSREADY | PANE_STATUSDRAWN);
        } else {
            new_pane_owner = created_pane.expect("new pane placed");
            new_wp = new_pane_owner.get();
        }
        if (*sc).argv.is_empty() {
            if (*sc).flags & SPAWN_RESPAWN == 0 {
                let command = session_owner.with_options_mut(|options| {
                    crate::src::options::options_get_string_optional(options, c"default-command")
                });
                if let Some(command) = command.filter(|command| !command.as_bytes().is_empty()) {
                    (*new_wp).argv = vec![command];
                } else {
                    (*new_wp).argv.clear();
                }
            }
        } else {
            (*new_wp).argv = (*sc).argv.clone();
        }
        if let Some(cwd) = cwd.take() {
            window_pane_set_cwd(&mut *new_wp, Some(cwd));
        }
        // Each fork path owns its environment until it has been installed.
        let mut child_owner = Some(environ_for_session(Some(&session_owner), 0));
        let child = child_owner.as_deref_mut().expect("spawn environment");
        if let Some(overrides) = (*sc).environ.as_deref() {
            environ_copy(overrides, child);
        }
        environ_set(child, c"TMUX_PANE", 0 as ::core::ffi::c_int, |out| {
            write!(out, "%{}", ((*new_wp).id) as u32)
        });
        if !c.is_none()
            && c.as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .is_none()
        {
            let path = c.as_ref().expect("live client").with_environment(|env| {
                env.and_then(|env| env.find(c"PATH"))
                    .map(|entry| entry.value.clone())
            });
            if let Some(value) = path {
                environ_set(child, c"PATH", 0, |out| write_cstr(out, value.as_deref()));
            }
        }
        if child.find(c"PATH").is_none() {
            environ_set(child, c"PATH", 0 as ::core::ffi::c_int, |out| {
                write_cstr(out, &*_PATH_DEFPATH)
            });
        }
        if !(*sc).flags & SPAWN_RESPAWN != 0 {
            let shell = session_owner
                .with_options_mut(|options| options_get_string(options, c"default-shell"));
            tmp = shell.as_ptr();
            if checkshell(nullable_cstr(tmp)) == 0 {
                tmp = _PATH_BSHELL.as_ptr();
            }
            window_pane_set_shell(&mut *new_wp, Some(CStr::from_ptr(tmp).to_owned()));
        }
        environ_set(child, c"SHELL", 0 as ::core::ffi::c_int, |out| {
            write_cstr(out, (*new_wp).shell.as_deref())
        });
        log_debug(format_args!(
            "{}: shell={}",
            "spawn_pane",
            log_cstr((*new_wp).shell.as_deref().unwrap_or(c"(null)"))
        ));
        if !(*new_wp).argv.is_empty() {
            let command = cmd_stringify_argv_cstring(&(*new_wp).argv);
            log_debug(format_args!(
                "{}: cmd={}",
                "spawn_pane",
                log_cstr(command.as_deref().unwrap_or(c"(null)"))
            ));
        }
        log_debug(format_args!(
            "{}: cwd={}",
            "spawn_pane",
            log_cstr((*new_wp).cwd.as_deref().unwrap_or(c"(null)"))
        ));
        cmd_log_argv(&(*new_wp).argv, c"spawn_pane");
        environ_log(child, |out| {
            write_cstr(out, c"spawn_pane")?;
            out.write_all(b": environment ")
        });
        memset(
            &raw mut ws as *mut ::core::ffi::c_void,
            0 as ::core::ffi::c_int,
            ::core::mem::size_of::<winsize>() as size_t,
        );
        ws.ws_col = (*new_wp).base.grid().sx as ::core::ffi::c_ushort;
        ws.ws_row = (*new_wp).base.grid().sy as ::core::ffi::c_ushort;
        ws.ws_xpixel = ((((*sc).winlink_handle())
            .get_unchecked()
            .window_handle()
            .as_ref())
        .expect("live window"))
        .cell_size()
        .0
        .wrapping_mul(ws.ws_col as u_int) as ::core::ffi::c_ushort;
        ws.ws_ypixel = ((((*sc).winlink_handle())
            .get_unchecked()
            .window_handle()
            .as_ref())
        .expect("live window"))
        .cell_size()
        .1
        .wrapping_mul(ws.ws_row as u_int) as ::core::ffi::c_ushort;
        sigfillset(&raw mut set);
        sigprocmask(SIG_BLOCK, &raw mut set, &raw mut oldset);
        if (*sc).flags & SPAWN_EMPTY != 0 {
            (*new_wp).flags |= PANE_EMPTY;
            (*new_wp).base.mode &= !MODE_CURSOR;
            (*new_wp).base.mode |= MODE_CRLF;
        } else {
            (*new_wp).flags &= !PANE_EMPTY;
            if !getcwd(
                &raw mut path as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 4096]>() as size_t,
            )
            .is_null()
            {
                if chdir(
                    (*new_wp)
                        .cwd
                        .as_ref()
                        .map_or(::core::ptr::null(), |value| value.as_ptr()),
                ) == 0 as ::core::ffi::c_int
                {
                    actual_cwd = (*new_wp)
                        .cwd
                        .as_ref()
                        .map_or(::core::ptr::null(), |value| value.as_ptr());
                } else if !home.is_null() && chdir(home) == 0 as ::core::ffi::c_int {
                    actual_cwd = home;
                } else if chdir(c"/".as_ptr()) == 0 as ::core::ffi::c_int {
                    actual_cwd = c"/".as_ptr();
                }
            }
            let mut master = -1;
            (*new_wp).pid = fdforkpty(
                ptm_fd,
                &raw mut master,
                &raw mut (*new_wp).tty as *mut ::core::ffi::c_char,
                ::core::ptr::null_mut::<termios>(),
                &raw mut ws,
            );
            if (*new_wp).pid == -(1 as ::core::ffi::c_int) {
                set_spawn_cause(
                    cause.as_mut(),
                    &[
                        b"fork failed: ",
                        CStr::from_ptr(strerror(*__errno_location())).to_bytes(),
                    ],
                );
                (*new_wp).fd = None;
                if !(*sc).flags & SPAWN_RESPAWN != 0 {
                    ClientRef::forget_pane(&new_pane_owner);
                    WindowRef::clone(
                        (((*sc).winlink_handle())
                            .get_unchecked()
                            .window_handle()
                            .as_ref())
                        .expect("live window"),
                    )
                    .remove_pane(&new_pane_owner);
                }
                sigprocmask(
                    SIG_SETMASK,
                    &raw mut oldset,
                    ::core::ptr::null_mut::<sigset_t>(),
                );
                return None;
            }
            if (*new_wp).pid != 0 as ::core::ffi::c_int {
                (*new_wp).fd = Some(OwnedFd::from_raw_fd(master));
                if !actual_cwd.is_null()
                    && chdir(&raw mut path as *mut ::core::ffi::c_char) != 0 as ::core::ffi::c_int
                    && (home.is_null() || chdir(home) != 0 as ::core::ffi::c_int)
                {
                    chdir(c"/".as_ptr());
                }
            } else {
                let (systemd_status, systemd_error) = systemd_move_to_new_cgroup();
                if systemd_status < 0 as ::core::ffi::c_int {
                    log_debug(format_args!(
                        "{}: moving pane to new cgroup failed: {}",
                        "spawn_pane",
                        log_cstr(systemd_error.as_deref().unwrap_or(c"(null)"))
                    ));
                }
                drop(systemd_error);
                if !actual_cwd.is_null() {
                    environ_set(child, c"PWD", 0 as ::core::ffi::c_int, |out| {
                        write_cstr(out, nullable_cstr(actual_cwd))
                    });
                }
                if crate::src::shared::terminal::read_attributes(STDIN_FILENO, &mut now)
                    != 0 as ::core::ffi::c_int
                {
                    _exit(1 as ::core::ffi::c_int);
                }
                if let Some(terminal) = session_owner.termios() {
                    now.c_cc = terminal.c_cc;
                }
                key = options_get_number(global_options, c"backspace") as key_code;
                if key >= 0x7f as key_code {
                    now.c_cc[VERASE as usize] = '\u{7f}' as i32 as cc_t;
                } else {
                    now.c_cc[VERASE as usize] = key as cc_t;
                }
                now.c_iflag |= IUTF8 as tcflag_t;
                if crate::src::shared::terminal::set_attributes(STDIN_FILENO, TCSANOW, &mut now)
                    != 0 as ::core::ffi::c_int
                {
                    _exit(1 as ::core::ffi::c_int);
                }
                proc_clear_signals(server_proc, 1 as ::core::ffi::c_int);
                hmux_rt::unix::close_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
                sigprocmask(
                    SIG_SETMASK,
                    &raw mut oldset,
                    ::core::ptr::null_mut::<sigset_t>(),
                );
                log_close();
                environ_push(child);
                // After fork this is the child's private copy. Release it after
                // publishing the process environment; the parent retains its own
                // owner and drops it when this function returns.
                drop(child_owner.take());
                if (*new_wp).argv.len() > 1 {
                    let mut exec_argv: Vec<_> =
                        (*new_wp).argv.iter().map(|arg| arg.as_ptr()).collect();
                    exec_argv.push(::core::ptr::null());
                    argvp = exec_argv.as_mut_ptr().cast();
                    execvp(
                        *argvp.offset(0 as ::core::ffi::c_int as isize),
                        argvp as *const *mut ::core::ffi::c_char,
                    );
                    _exit(1 as ::core::ffi::c_int);
                }
                cp = strrchr(
                    (*new_wp)
                        .shell
                        .as_ref()
                        .map_or(::core::ptr::null(), |value| value.as_ptr()),
                    '/' as i32,
                );
                let shell_name = if !cp.is_null() && *cp.add(1) != 0 {
                    cp.add(1)
                } else {
                    (*new_wp)
                        .shell
                        .as_ref()
                        .map_or(::core::ptr::null(), |value| value.as_ptr())
                };
                if (*new_wp).argv.len() == 1 {
                    tmp = (&(*new_wp).argv)[0].as_ptr();
                    let argv0 = CStr::from_ptr(shell_name).to_owned();
                    execl(
                        (*new_wp)
                            .shell
                            .as_ref()
                            .map_or(::core::ptr::null(), |value| value.as_ptr()),
                        argv0.as_ptr(),
                        c"-c".as_ptr(),
                        tmp,
                        NULL as *mut ::core::ffi::c_char,
                    );
                    _exit(1 as ::core::ffi::c_int);
                }
                let mut login_name = vec![b'-'];
                login_name.extend_from_slice(CStr::from_ptr(shell_name).to_bytes());
                let argv0 = CString::new(login_name).expect("shell name contains no NUL");
                execl(
                    (*new_wp)
                        .shell
                        .as_ref()
                        .map_or(::core::ptr::null(), |value| value.as_ptr()),
                    argv0.as_ptr(),
                    NULL as *mut ::core::ffi::c_char,
                );
                _exit(1 as ::core::ffi::c_int);
            }
        }
        if !(*new_wp).flags & PANE_EMPTY != 0 {
            let record = CString::new(format!("tmux({}).%{}", getpid(), (*new_wp).id))
                .expect("pane login record contains no NUL");
            utempter_add_record(
                (*new_wp).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                record.as_ptr(),
            );
            kill(getpid(), SIGCHLD);
        }
        (*new_wp).flags &= !PANE_EXITED;
        sigprocmask(
            SIG_SETMASK,
            &raw mut oldset,
            ::core::ptr::null_mut::<sigset_t>(),
        );
        window_pane_set_event(&new_pane_owner);
        drop(child_owner.take());
        spawn_fire_pane_created(sc, &new_pane_owner);
        if (*sc).flags & SPAWN_RESPAWN != 0 {
            return Some(new_pane_owner);
        }
        if !(*sc).flags & SPAWN_DETACHED != 0
            || ((((*sc).winlink_handle())
                .get_unchecked()
                .window_handle()
                .as_ref())
            .expect("live window"))
            .active_pane()
            .is_none()
        {
            if (*sc).flags & SPAWN_NONOTIFY != 0 {
                WindowRef::clone(
                    (((*sc).winlink_handle())
                        .get_unchecked()
                        .window_handle()
                        .as_ref())
                    .expect("live window"),
                )
                .select_pane(&new_pane_owner, false);
            } else {
                WindowRef::clone(
                    (((*sc).winlink_handle())
                        .get_unchecked()
                        .window_handle()
                        .as_ref())
                    .expect("live window"),
                )
                .select_pane(&new_pane_owner, true);
            }
        }
        if !(*sc).flags & SPAWN_NONOTIFY != 0 {
            events_fire_window(
                c"window-layout-changed",
                WindowRef::clone(
                    (((*sc).winlink_handle())
                        .get_unchecked()
                        .window_handle()
                        .as_ref())
                    .expect("live window"),
                ),
            );
        }
        Some(new_pane_owner)
    })();
    drop(original_window);
    result
}
pub(super) unsafe fn finish_editing(wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>) {
    let mut wp = wp_owner.get();
    let Some(mut owner) = (*wp).editor.take() else {
        return;
    };
    let es = &raw mut *owner;
    let mut f: *mut FILE = ::core::ptr::null_mut::<FILE>();
    let mut result: Option<Vec<u8>> = None;
    let mut len: off_t = 0 as off_t;
    let mut status: ::core::ffi::c_int = 128 as ::core::ffi::c_int + SIGHUP;
    if (*wp).flags & PANE_STATUSREADY != 0 {
        if (*wp).status & 0x7f as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            status = ((*wp).status & 0xff00 as ::core::ffi::c_int) >> 8 as ::core::ffi::c_int;
        } else if (((*wp).status & 0x7f as ::core::ffi::c_int) + 1 as ::core::ffi::c_int)
            as ::core::ffi::c_schar as ::core::ffi::c_int
            >> 1 as ::core::ffi::c_int
            > 0 as ::core::ffi::c_int
        {
            status = ((*wp).status & 0x7f as ::core::ffi::c_int) + 128 as ::core::ffi::c_int;
        }
    }
    if (*es).cb.is_none() {
        return;
    }
    if status != 0 as ::core::ffi::c_int {
        (*es).cb.take().expect("non-null editor callback")(
            std::ptr::NonNull::new(es).unwrap(),
            None,
        );
        return;
    }
    f = fopen(((*es).path).as_ptr().cast_mut(), c"r".as_ptr()) as *mut FILE;
    if !f.is_null() {
        let stream = CFile::from_raw(f).expect("fopen returned a non-null stream");
        if fseeko(stream.as_ptr(), 0 as __off_t, SEEK_END) == 0 as ::core::ffi::c_int {
            len = ftello(stream.as_ptr()) as off_t;
            if len >= 0 as off_t && len as uintmax_t <= SIZE_MAX as uintmax_t {
                if fseeko(stream.as_ptr(), 0 as __off_t, SEEK_SET) == 0 as ::core::ffi::c_int {
                    if len == 0 as off_t {
                        result = Some(Vec::new());
                    } else {
                        let mut bytes = Vec::new();
                        if bytes.try_reserve_exact(len as usize).is_ok() {
                            bytes.resize(len as usize, 0);
                            if fread(
                                bytes.as_mut_ptr().cast::<::core::ffi::c_void>(),
                                len as size_t,
                                1 as size_t,
                                stream.as_ptr(),
                            ) == 1 as ::core::ffi::c_ulong
                            {
                                result = Some(bytes);
                            }
                        }
                    }
                }
            } else {
                len = 0 as off_t;
            }
        }
        drop(stream);
    }
    (*es).cb.take().expect("non-null editor callback")(std::ptr::NonNull::new(es).unwrap(), result);
}
pub(super) unsafe fn install_editor(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut editor: Box<spawn_editor_state>,
) -> EditorHandle {
    pane.with_options_mut(|options| {
        options_set_number(options, c"remain-on-exit", 0);
    });
    let pointer = std::ptr::NonNull::from(&mut *editor);
    {
        let state = &mut *pane.get();
        editor.pid = state.pid;
        state.editor = Some(editor);
    }
    EditorHandle::new(pane, pointer)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::ffi::libc::fclose;
    use crate::src::ffi::libc::{fwrite, mkstemp};
    use crate::src::shared::spawn::spawn_finish_edit_cb;
    use crate::src::shared::spawn::EditorHandle;
    use crate::src::spawn::{spawn_editor_fdopen, spawn_editor_finish};
    use std::ffi::CStr;
    use std::fs;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn stale_editor_handle_does_not_cancel_replacement() {
        let pane = window_pane::new();
        unsafe {
            let first = spawn_editor_state::new(c"".to_owned(), Some(Box::new(|_, _| {})));
            (*pane.get()).editor = Some(first);
            let first_ptr =
                std::ptr::NonNull::from((*pane.get()).editor.as_mut().unwrap().as_mut());
            let stale = EditorHandle::new(&pane, first_ptr);
            let previous = (*pane.get()).editor.take();
            let second = spawn_editor_state::new(c"".to_owned(), Some(Box::new(|_, _| {})));
            (*pane.get()).editor = Some(second);
            let second_ptr =
                std::ptr::NonNull::from((*pane.get()).editor.as_mut().unwrap().as_mut());
            let current = EditorHandle::new(&pane, second_ptr);

            assert_eq!(stale.pid(), -1);
            assert!(!stale.matches(second_ptr.as_ref()));
            stale.cancel();
            assert!((*pane.get()).editor.as_ref().unwrap().cb.is_some());
            assert!(current.matches(second_ptr.as_ref()));
            current.cancel();
            assert!((*pane.get()).editor.as_ref().unwrap().cb.is_none());
            drop(previous);
            drop(pane);
            assert_eq!(current.pid(), -1);
            current.cancel();
        }
    }

    const CHILD_CASE: &str = "HMUX_EDITOR_FD_OWNER_CASE";
    const SUCCESS_CASE: &str = "success";
    const SUCCESS_TEST: &str =
        "src::window_pane::spawning::tests::editor_completion_reads_and_unlinks";

    fn run_isolated(test_name: &str, case: &str) {
        let test_exe = std::env::current_exe().expect("test executable");
        let mut child = Command::new(test_exe)
            .args(["--exact", test_name, "--nocapture"])
            .env(CHILD_CASE, case)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn isolated editor ownership test");
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            match child.try_wait().expect("poll isolated editor test") {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("isolated editor ownership test timed out");
                }
                None => thread::sleep(Duration::from_millis(10)),
            }
        };
        assert!(status.success(), "isolated test exited with {status}");
    }

    unsafe fn create_temp_file() -> (OwnedFd, std::ffi::CString) {
        let mut template = b"/tmp/hmux-editor-owner-XXXXXX\0".to_vec();
        let fd = mkstemp(template.as_mut_ptr() as *mut ::core::ffi::c_char);
        assert!(
            fd >= 0,
            "mkstemp failed: {}",
            *strerror(*__errno_location())
        );
        (
            OwnedFd::from_raw_fd(fd),
            CStr::from_ptr(template.as_ptr() as *const ::core::ffi::c_char).to_owned(),
        )
    }

    fn capture_editor_result(result: refbox::Weak<Option<Vec<u8>>>) -> spawn_finish_edit_cb {
        Some(Box::new(move |_, value| {
            *result.try_borrow_mut().expect("live editor result") = value;
        }))
    }

    #[test]
    fn editor_completion_reads_and_unlinks() {
        if std::env::var(CHILD_CASE).as_deref() == Ok(SUCCESS_CASE) {
            unsafe {
                let (fd_owner, path) = create_temp_file();
                let fd = fd_owner.as_raw_fd();
                let flags = ::libc::fcntl(fd, ::libc::F_GETFL);
                assert!(flags >= 0);
                assert_eq!(flags & ::libc::O_ACCMODE, ::libc::O_RDWR);
                assert!(std::path::Path::new(path.to_str().unwrap()).exists());
                let file = spawn_editor_fdopen(fd_owner);
                assert!(!file.is_null());
                assert_eq!(::libc::fcntl(fd, ::libc::F_GETFL), flags);
                assert!(::libc::fcntl(fd, ::libc::F_GETFD) >= 0);
                let original = b"created through fdopen";
                assert_eq!(
                    fwrite(
                        original.as_ptr() as *const ::core::ffi::c_void,
                        original.len(),
                        1,
                        file,
                    ),
                    1
                );
                assert_eq!(fclose(file), 0);
                assert_eq!(::libc::fcntl(fd, ::libc::F_GETFD), -1);
                assert_eq!(fs::read(path.to_str().unwrap()).unwrap(), original);

                let edited = b"edited by child\0\xff";
                fs::write(path.to_str().unwrap(), edited).unwrap();
                let result = refbox::RefBox::new(None::<Vec<u8>>);
                let state = spawn_editor_state::new(
                    path.to_owned(),
                    capture_editor_result(result.downgrade()),
                );
                let pane_owner = window_pane::new();
                let wp = pane_owner.get();
                (*wp).editor = Some(state);
                (*wp).flags = PANE_STATUSREADY;
                (*wp).status = 0;
                spawn_editor_finish(&pane_owner);
                assert_eq!(
                    result.try_borrow_mut().expect("live editor result").take(),
                    Some(edited.to_vec())
                );
                assert!(!std::path::Path::new(path.to_str().unwrap()).exists());
                drop(pane_owner);

                let (fd_owner, path) = create_temp_file();
                drop(fd_owner);
                fs::write(path.to_str().unwrap(), []).unwrap();
                let result = refbox::RefBox::new(None::<Vec<u8>>);
                let state = spawn_editor_state::new(
                    path.to_owned(),
                    capture_editor_result(result.downgrade()),
                );
                let pane_owner = window_pane::new();
                let wp = pane_owner.get();
                (*wp).editor = Some(state);
                (*wp).flags = PANE_STATUSREADY;
                (*wp).status = 0;
                spawn_editor_finish(&pane_owner);
                assert_eq!(
                    result.try_borrow_mut().expect("live editor result").take(),
                    Some(Vec::new())
                );
                assert!(!std::path::Path::new(path.to_str().unwrap()).exists());
                drop(pane_owner);

                let (fd_owner, path) = create_temp_file();
                drop(fd_owner);
                fs::write(path.to_str().unwrap(), b"ignored after failure").unwrap();
                let result = refbox::RefBox::new(None::<Vec<u8>>);
                let state = spawn_editor_state::new(
                    path.to_owned(),
                    capture_editor_result(result.downgrade()),
                );
                let pane_owner = window_pane::new();
                let wp = pane_owner.get();
                (*wp).editor = Some(state);
                (*wp).flags = PANE_STATUSREADY;
                (*wp).status = 1 << 8;
                spawn_editor_finish(&pane_owner);
                assert_eq!(
                    result.try_borrow_mut().expect("live editor result").take(),
                    None
                );
                assert!(!std::path::Path::new(path.to_str().unwrap()).exists());
                drop(pane_owner);
            }
            return;
        }
        run_isolated(SUCCESS_TEST, SUCCESS_CASE);
    }
}
