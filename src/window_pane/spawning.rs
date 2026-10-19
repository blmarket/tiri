//! Pane process creation and editor completion keep storage inside its owner.
use super::pty::PanePty;
use super::{
    window_pane_create, window_pane_index, window_pane_reset_mode_all, window_pane_set_event,
};
use crate::cmd::queue::cmdq_get_client;
use crate::cmd::{cmd_log_argv, cmd_stringify_argv_cstring};
use crate::compat::fdforkpty::fdforkpty;
#[cfg(target_os = "linux")]
use crate::compat::systemd::systemd_move_to_new_cgroup;
use crate::control::control_reset_pane;
use crate::environ::{environ_copy, environ_for_session, environ_log, environ_push, environ_set};
use crate::events::{events_fire, events_fire_window};
use crate::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_session,
    event_payload_set_string, event_payload_set_target, event_payload_set_window,
};
use crate::format::bytes::cstr_suffix;
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::format_single_cstring;
use crate::log::{log_close, log_cstr, log_debug};
use crate::options::OptionsScope;
use crate::options::{options_get_number, options_get_string, options_set_number};
use crate::proc::proc_clear_signals;
use crate::reactor::BufferEvent;
use crate::screen::screen_reinit;
use crate::server::server_proc;
use crate::server_client::Client;
use crate::server_client::ClientRegistry;
use crate::shared::errno::ErrorText;
use crate::window::Window as _;
use libc::{_exit, chdir, execl, execvp, getcwd, memset, sigfillset, sigprocmask};

use crate::session::Session;
use crate::shared::client::ClientRef;
use crate::shared::spawn::spawn_context;
use crate::shared::window::WindowRef;
use crate::tmux::{checkshell, find_home_cstr, ptm_fd};

use crate::window_pane::WindowPane as _;
use std::ffi::{CStr, CString, OsStr};

use crate::shared::abi::*;
use crate::shared::client::CLIENT_CONTROL;
use crate::shared::command::cmd_find_state;
use crate::shared::key::*;
use crate::shared::pane::window_pane;
use crate::shared::pane::{PANE_EMPTY, PANE_EXITED, PANE_STATUSDRAWN};
use crate::shared::posix_io::{_PATH_BSHELL, STDERR_FILENO, STDIN_FILENO};
use crate::shared::posix_terminal::{winsize, TCSANOW, VERASE};
use crate::shared::screen::{MODE_CRLF, MODE_CURSOR};
use crate::shared::signal::{sigset_t, SIGHUP, SIG_BLOCK, SIG_SETMASK};
use crate::shared::spawn::spawn_editor_state;
use crate::shared::spawn::{
    SPAWN_BEFORE, SPAWN_DETACHED, SPAWN_EMPTY, SPAWN_KILL, SPAWN_NONOTIFY, SPAWN_RESPAWN,
};
use crate::shared::terminal::*;

use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;

use crate::shared::spawn::EditorHandle;
use crate::spawn::{set_spawn_cause, spawn_log, _PATH_DEFPATH, IUTF8};
unsafe fn spawn_fire_pane_created(
    sc: &spawn_context,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let mut wp = wp_owner.get();
    let session_owner = sc.s.upgrade();
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
        (sc.winlink_handle()).clone(),
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
        (sc.winlink_handle()).get_unchecked().idx,
    );
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    let cmd = if !(*wp).argv.is_empty() {
        cmd_stringify_argv_cstring(&(*wp).argv)
    } else {
        None
    };
    if let Some(cmd) = cmd.as_ref().filter(|text| !text.as_bytes().is_empty()) {
        event_payload_set_string(&mut ep, c"pane_command", |out| {
            write_cstr(out, cmd.as_c_str())
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
    if sc.flags & SPAWN_EMPTY != 0 {
        event_payload_set_int(&mut ep, c"created_empty", 1 as ::core::ffi::c_int);
    } else {
        event_payload_set_int(&mut ep, c"created_empty", 0 as ::core::ffi::c_int);
    }
    if sc.flags & SPAWN_RESPAWN != 0 {
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
    let mut panes = window
        .pane_snapshot()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .collect::<Vec<_>>();
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
    let hlimit = options_get_number(
        &sc.s
            .upgrade()
            .expect("spawn context session")
            .borrow_options_mut(),
        c"history-limit",
    ) as u_int;
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
    sc: &mut spawn_context,
    cause: &mut Option<CString>,
) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    let source_pane_owner = sc.wp0.upgrade();
    let source_pane = source_pane_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let item_owner = sc.item.upgrade();
    let mut c: Option<ClientRef> = None;
    let mut loop_0: Option<ClientRef> = None;
    let session_owner = sc.s.upgrade().expect("spawn context session");
    let original_window = sc
        .winlink_handle()
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("spawn window");
    let result = (|| {
        let created_pane = if sc.flags & SPAWN_RESPAWN != 0 {
            None
        } else {
            match place_new_pane(sc, &original_window) {
                Ok(pane) => Some(pane),
                Err(reason) => {
                    set_spawn_cause(Some(&mut *cause), &[reason.to_bytes()]);
                    return None;
                }
            }
        };
        let target_session_owner;
        let new_pane_owner;
        let mut new_wp: *mut window_pane = ::core::ptr::null_mut::<window_pane>();
        let mut argvp: *mut *mut ::core::ffi::c_char =
            ::core::ptr::null_mut::<*mut ::core::ffi::c_char>();
        let mut cwd: Option<CString> = None;
        let mut path: [::core::ffi::c_char; 4096] = [0; 4096];
        let _cmd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut home: *const ::core::ffi::c_char =
            find_home_cstr().map_or(::core::ptr::null(), CStr::as_ptr);
        let mut actual_cwd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut idx: u_int = 0;
        let mut ws: winsize = winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let mut set: sigset_t = unsafe { ::core::mem::zeroed() };
        let mut oldset: sigset_t = unsafe { ::core::mem::zeroed() };
        let mut key: key_code = 0;
        let c_owner;
        if let Some(item_owner) = item_owner.as_ref() {
            let item = item_owner.borrow();
            target_session_owner = crate::cmd::queue::cmdq_get_target(&item).session_handle();
            c_owner = cmdq_get_client(Some(&*item));
            c = c_owner.clone();
        } else {
            target_session_owner = Some(session_owner.clone());
            c_owner = sc.tc.upgrade();
            c = c_owner.clone();
        }
        spawn_log(c"spawn_pane", sc);
        if let Some(requested_cwd) = sc.cwd.as_ref() {
            if item_owner.is_some() {
                cwd = Some(format_single_cstring(
                    item_owner.as_ref(),
                    requested_cwd,
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
        } else if !sc.flags & SPAWN_RESPAWN != 0 {
            cwd = match c_owner.as_ref() {
                Some(client) => client.cwd(target_session_owner.as_ref()),
                None => ClientRef::working_directory(None, target_session_owner.as_ref()),
            };
        }
        if sc.flags & SPAWN_RESPAWN != 0 {
            if (*source_pane).fd.is_some() && !sc.flags & SPAWN_KILL != 0 {
                idx = window_pane_index(source_pane_owner.as_ref().expect("respawn pane"))
                    .expect("pane belongs to window ordering");
                set_spawn_cause(
                    Some(&mut *cause),
                    &[
                        b"pane ",
                        session_owner.name().as_bytes(),
                        b":",
                        (sc.winlink_handle())
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
            screen_reinit(&mut (*source_pane).base);
            if let Some(ictx) = (*source_pane).ictx.take() {
                drop(ictx);
            }
            (*source_pane).offset.used = 0 as size_t;
            (*source_pane).base_offset = 0 as size_t;
            (*source_pane).pipe_offset.used = 0 as size_t;
            let mut registry_loop_0_owner = ClientRegistry::first();
            loop_0 = registry_loop_0_owner.clone();
            while !loop_0.is_none() {
                if loop_0.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
                    control_reset_pane(
                        &loop_0.clone().expect("live client"),
                        source_pane_owner.as_ref().expect("respawn pane"),
                    );
                }
                registry_loop_0_owner = ClientRegistry::next(
                    registry_loop_0_owner
                        .as_ref()
                        .expect("current registry client"),
                );
                loop_0 = registry_loop_0_owner.clone();
            }
            new_pane_owner = source_pane_owner.as_ref().expect("respawn pane").clone();
            new_wp = new_pane_owner.get();
            (*new_wp).flags &= !PANE_STATUSDRAWN;
            (*new_wp).exit_status = None;
        } else {
            new_pane_owner = created_pane.expect("new pane placed");
            new_wp = new_pane_owner.get();
        }
        if sc.argv.is_empty() {
            if sc.flags & SPAWN_RESPAWN == 0 {
                let command = crate::options::options_get_string_optional(
                    &session_owner.borrow_options_mut(),
                    c"default-command",
                );
                if let Some(command) = command.filter(|command| !command.as_bytes().is_empty()) {
                    (*new_wp).argv = vec![command];
                } else {
                    (*new_wp).argv.clear();
                }
            }
        } else {
            (*new_wp).argv = sc.argv.clone();
        }
        if let Some(cwd) = cwd.take() {
            (*new_wp).cwd = Some(cwd);
        }
        let mut child_owner = Some(environ_for_session(Some(&session_owner), 0));
        let child = child_owner.as_deref_mut().expect("spawn environment");
        if let Some(overrides) = sc.environ.as_deref() {
            environ_copy(overrides, child);
        }
        environ_set(child, c"TMUX_PANE", 0 as ::core::ffi::c_int, |out| {
            write!(out, "%{}", ((*new_wp).id) as u32)
        });
        if let Some(client) = c
            .as_ref()
            .filter(|client| client.attached_session().upgrade().is_none())
        {
            let path = client.with_environment(|env| {
                env.and_then(|env| env.find(c"PATH"))
                    .map(|entry| entry.value.clone())
            });
            if let Some(value) = path {
                environ_set(child, c"PATH", 0, |out| write_cstr(out, value.as_deref()));
            }
        }
        if child.find(c"PATH").is_none() {
            environ_set(child, c"PATH", 0 as ::core::ffi::c_int, |out| {
                write_cstr(out, _PATH_DEFPATH)
            });
        }
        if !sc.flags & SPAWN_RESPAWN != 0 {
            let shell = options_get_string(&session_owner.borrow_options_mut(), c"default-shell");
            (*new_wp).shell = Some(if checkshell(Some(&shell)) == 0 {
                _PATH_BSHELL.to_owned()
            } else {
                shell
            });
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
        ws.ws_xpixel = (((sc.winlink_handle())
            .get_unchecked()
            .window_handle()
            .as_ref())
        .expect("live window"))
        .cell_size()
        .0
        .wrapping_mul(ws.ws_col as u_int) as ::core::ffi::c_ushort;
        ws.ws_ypixel = (((sc.winlink_handle())
            .get_unchecked()
            .window_handle()
            .as_ref())
        .expect("live window"))
        .cell_size()
        .1
        .wrapping_mul(ws.ws_row as u_int) as ::core::ffi::c_ushort;
        sigfillset(&raw mut set);
        sigprocmask(SIG_BLOCK, &raw mut set, &raw mut oldset);
        if sc.flags & SPAWN_EMPTY != 0 {
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
                (*new_wp).tty.as_mut_ptr().cast(),
                ::core::ptr::null_mut::<termios>(),
                &raw mut ws,
            );
            if (*new_wp).pid == -(1 as ::core::ffi::c_int) {
                let error = ErrorText(&std::io::Error::last_os_error()).to_string();
                set_spawn_cause(Some(&mut *cause), &[b"fork failed: ", error.as_bytes()]);
                (*new_wp).fd = None;
                if !sc.flags & SPAWN_RESPAWN != 0 {
                    ClientRef::forget_pane(&new_pane_owner);
                    WindowRef::clone(
                        ((sc.winlink_handle())
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
                (*new_wp).fd = Some(PanePty::new(OwnedFd::from_raw_fd(master)));
                if !actual_cwd.is_null()
                    && chdir(&raw mut path as *mut ::core::ffi::c_char) != 0 as ::core::ffi::c_int
                    && (home.is_null() || chdir(home) != 0 as ::core::ffi::c_int)
                {
                    chdir(c"/".as_ptr());
                }
            } else {
                #[cfg(target_os = "linux")]
                {
                    if let Err(systemd_error) = systemd_move_to_new_cgroup() {
                        log_debug(format_args!(
                            "{}: moving pane to new cgroup failed: {}",
                            "spawn_pane",
                            log_cstr(&systemd_error)
                        ));
                    }
                }
                if !actual_cwd.is_null() {
                    environ_set(child, c"PWD", 0 as ::core::ffi::c_int, |out| {
                        write_cstr(out, nullable_cstr(actual_cwd))
                    });
                }
                let Ok(mut now) = crate::shared::terminal::read_attributes(STDIN_FILENO) else {
                    _exit(1 as ::core::ffi::c_int);
                };
                if let Some(terminal) = session_owner.termios() {
                    now.c_cc = terminal.c_cc;
                }
                key = OptionsScope::GlobalServer
                    .with_local(|oo| options_get_number(oo, c"backspace"))
                    as key_code;
                if key >= 0x7f as key_code {
                    now.c_cc[VERASE] = '\u{7f}' as i32 as cc_t;
                } else {
                    now.c_cc[VERASE] = key as cc_t;
                }
                now.c_iflag |= IUTF8 as tcflag_t;
                if crate::shared::terminal::set_attributes(STDIN_FILENO, TCSANOW, &now).is_err() {
                    _exit(1 as ::core::ffi::c_int);
                }
                proc_clear_signals(server_proc, 1 as ::core::ffi::c_int);
                tiri_rt::unix::close_from(STDERR_FILENO + 1 as ::core::ffi::c_int);
                sigprocmask(
                    SIG_SETMASK,
                    &raw mut oldset,
                    ::core::ptr::null_mut::<sigset_t>(),
                );
                log_close();
                environ_push(child);
                drop(child_owner.take());
                if (*new_wp).argv.len() > 1 {
                    let mut exec_argv: Vec<_> =
                        (*new_wp).argv.iter().map(|arg| arg.as_ptr()).collect();
                    exec_argv.push(::core::ptr::null());
                    argvp = exec_argv.as_mut_ptr().cast();
                    execvp(
                        *argvp.offset(0 as ::core::ffi::c_int as isize),
                        argvp as *const *const ::core::ffi::c_char,
                    );
                    _exit(1 as ::core::ffi::c_int);
                }
                let shell = (*new_wp).shell.as_deref().expect("pane shell");
                let shell_name = match shell.to_bytes().iter().rposition(|&byte| byte == b'/') {
                    Some(slash) if slash + 1 < shell.count_bytes() => cstr_suffix(shell, slash + 1),
                    _ => shell,
                };
                if (*new_wp).argv.len() == 1 {
                    execl(
                        shell.as_ptr(),
                        shell_name.as_ptr(),
                        c"-c".as_ptr(),
                        (&(*new_wp).argv)[0].as_ptr(),
                        NULL as *mut ::core::ffi::c_char,
                    );
                    _exit(1 as ::core::ffi::c_int);
                }
                let mut login_name = vec![b'-'];
                login_name.extend_from_slice(shell_name.to_bytes());
                let argv0 = CString::new(login_name).expect("shell name contains no NUL");
                execl(
                    shell.as_ptr(),
                    argv0.as_ptr(),
                    NULL as *mut ::core::ffi::c_char,
                );
                _exit(1 as ::core::ffi::c_int);
            }
        }
        (*new_wp).flags &= !PANE_EXITED;
        new_pane_owner.refresh_utmp();
        sigprocmask(
            SIG_SETMASK,
            &raw mut oldset,
            ::core::ptr::null_mut::<sigset_t>(),
        );
        window_pane_set_event(&new_pane_owner);
        drop(child_owner.take());
        spawn_fire_pane_created(sc, &new_pane_owner);
        if sc.flags & SPAWN_RESPAWN != 0 {
            return Some(new_pane_owner);
        }
        if !sc.flags & SPAWN_DETACHED != 0
            || (((sc.winlink_handle())
                .get_unchecked()
                .window_handle()
                .as_ref())
            .expect("live window"))
            .active_pane()
            .upgrade()
            .is_none()
        {
            if sc.flags & SPAWN_NONOTIFY != 0 {
                WindowRef::clone(
                    ((sc.winlink_handle())
                        .get_unchecked()
                        .window_handle()
                        .as_ref())
                    .expect("live window"),
                )
                .select_pane(&new_pane_owner, false);
            } else {
                WindowRef::clone(
                    ((sc.winlink_handle())
                        .get_unchecked()
                        .window_handle()
                        .as_ref())
                    .expect("live window"),
                )
                .select_pane(&new_pane_owner, true);
            }
        }
        if !sc.flags & SPAWN_NONOTIFY != 0 {
            events_fire_window(
                c"window-layout-changed",
                WindowRef::clone(
                    ((sc.winlink_handle())
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
    let status = match (*wp).exit_status {
        Some(exit_status) => exit_status
            .code()
            .or(exit_status.signal().map(|signal| signal + 128))
            .unwrap_or(0),
        None => 128 + SIGHUP,
    };
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
    let result = std::fs::read(OsStr::from_bytes((*es).path.to_bytes())).ok();
    (*es).cb.take().expect("non-null editor callback")(std::ptr::NonNull::new(es).unwrap(), result);
}
pub(super) unsafe fn install_editor(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut editor: Box<spawn_editor_state>,
) -> EditorHandle {
    options_set_number(&mut pane.borrow_options_mut(), c"remain-on-exit", 0);
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
    use crate::shared::spawn::spawn_finish_edit_cb;
    use crate::shared::spawn::EditorHandle;
    use crate::spawn::spawn_editor_finish;
    use libc::mkstemp;
    use std::ffi::CStr;
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::process::ExitStatus;
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

    const CHILD_CASE: &str = "TIRI_EDITOR_FD_OWNER_CASE";
    const SUCCESS_CASE: &str = "success";
    const SUCCESS_TEST: &str = "window_pane::spawning::tests::editor_completion_reads_and_unlinks";

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
        let mut template = b"/tmp/tiri-editor-owner-XXXXXX\0".to_vec();
        let fd = mkstemp(template.as_mut_ptr() as *mut ::core::ffi::c_char);
        assert!(
            fd >= 0,
            "mkstemp failed: {}",
            std::io::Error::last_os_error()
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
                let mut file = fs::File::from(fd_owner);
                let original = b"created through mkstemp";
                assert!(crate::spawn::spawn_editor_write(&mut file, original));
                drop(file);
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
                (*wp).exit_status = Some(ExitStatus::from_raw(0));
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
                (*wp).exit_status = Some(ExitStatus::from_raw(0));
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
                (*wp).exit_status = Some(ExitStatus::from_raw(1 << 8));
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
