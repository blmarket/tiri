//! Pane process exit and explicit teardown transactions.
use super::window_pane_wait_finish;
use crate::events::events_fire;
use crate::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_window,
};
use crate::format::format_single_cstring;
use crate::format_draw::format_draw;
use crate::grid::grid_default_cell;
use crate::log::log_debug;
use crate::options::{options_get_number, options_get_string};
use crate::reactor::BufferEvent;
use crate::resize::recalculate_sizes;
use crate::screen_write::{
    screen_write_cursormove, screen_write_linefeed, screen_write_scrollregion,
    screen_write_start_pane, screen_write_stop,
};
use crate::server_client::Client;
use crate::shared::client::ClientRef;
use crate::shared::window::{WindowRef, WindowWeak};
use crate::tmux::sig2name;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
use std::ffi::CStr;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::SystemTime;

use crate::shared::abi::*;
use crate::shared::command::cmd_find_state;
use crate::shared::grid::*;
use crate::shared::pane::window_pane;
use crate::shared::pane::{PANE_REDRAW, PANE_STATUSDRAWN};
use crate::shared::screen::MODE_CURSOR;
use crate::shared::style::*;

use crate::server_fn::{server_kill_window, server_redraw_window};
use crate::shared::pane::PANE_EXITED;
use crate::spawn::spawn_editor_finish;

unsafe fn server_fire_pane_exit(
    name: &CStr,
    wp_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let mut wp = wp_owner.get();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
        idx: 0,
    };
    let status = (*wp).exit_status.unwrap_or_default();
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_pane(wp_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(wp_owner));
    event_payload_set_window(
        &mut ep,
        c"window",
        WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window")),
    );
    if let Some(code) = status.code() {
        event_payload_set_int(&mut ep, c"exit_status", code);
    } else if let Some(signal) = status.signal() {
        event_payload_set_string(&mut ep, c"exit_signal", |out| {
            out.write_all(sig2name(signal).as_bytes())
        });
    }
    event_payload_set_int(
        &mut ep,
        c"exit_success",
        status.success() as ::core::ffi::c_int,
    );
    events_fire(name, ep);
}
pub(super) unsafe fn kill_process(pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>) {
    let wp = pane_owner.get();
    let window_owner = pane_owner
        .window_observer()
        .upgrade()
        .expect("live pane window");
    if window_owner.pane_snapshot().len() == 1 {
        server_kill_window(window_owner, 1);
        recalculate_sizes();
    } else {
        drop(window_owner);
        ClientRef::forget_pane(pane_owner);
        WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window"))
            .remove_pane(pane_owner);
        server_redraw_window(((*wp).window_handle().as_ref()).expect("live window"));
    };
}
pub(super) unsafe fn finish_process(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut notify: ::core::ffi::c_int,
) {
    let wp = pane_owner.get();
    let window_owner = pane_owner
        .window_observer()
        .upgrade()
        .expect("live pane window");
    let mut gc: grid_cell = grid_cell {
        data: utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        },
        attr: 0,
        flags: 0,
        fg: 0,
        bg: 0,
        us: 0,
        link: 0,
    };
    let mut remain_on_exit: ::core::ffi::c_int = 0;
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut sx: u_int = (*wp).base.grid().sx;
    let mut sy: u_int = (*wp).base.grid().sy;
    if (*wp).fd.is_some() {
        std::mem::take(&mut (*wp).event).free();
        drop((*wp).fd.take());
    }
    if (*wp).pipe_fd.is_some() {
        std::mem::take(&mut (*wp).pipe_event).free();
        drop((*wp).pipe_fd.take());
    }
    let Some(exit_status) = (*wp).exit_status else {
        drop(window_owner);
        return;
    };
    remain_on_exit =
        options_get_number(&(*wp).borrow_options_mut(), c"remain-on-exit") as ::core::ffi::c_int;
    let mut current_block_37: u64;
    match remain_on_exit {
        2 | 4 => {
            if exit_status.success() {
                current_block_37 = 3275366147856559585;
            } else {
                current_block_37 = 2300157484894416861;
            }
        }
        1 | 3 => {
            current_block_37 = 2300157484894416861;
        }
        _ => {
            current_block_37 = 3275366147856559585;
        }
    }
    match current_block_37 {
        3275366147856559585 => {}
        _ => {
            if (*wp).flags & PANE_STATUSDRAWN != 0 {
                drop(window_owner);
                return;
            }
            (*wp).flags |= PANE_STATUSDRAWN;
            (*wp).dead_time = SystemTime::now();
            if notify != 0 {
                server_fire_pane_exit(c"pane-died", pane_owner);
            }
            let format =
                options_get_string(&pane_owner.borrow_options_mut(), c"remain-on-exit-format");
            s = format.as_ptr();
            if *s as ::core::ffi::c_int != '\0' as i32 {
                let mut ctx = screen_write_start_pane(pane_owner, &mut (*wp).base);
                screen_write_scrollregion(&mut ctx, 0 as u_int, sy.wrapping_sub(1 as u_int));
                screen_write_cursormove(
                    &mut ctx,
                    0 as ::core::ffi::c_int,
                    sy.wrapping_sub(1 as u_int) as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                );
                screen_write_linefeed(&mut ctx, 1 as ::core::ffi::c_int, 8 as u_int);
                gc = grid_default_cell;
                let expanded = format_single_cstring(
                    None,
                    s,
                    None,
                    None,
                    (refbox::Weak::new()).clone(),
                    Some(pane_owner),
                );
                format_draw(
                    &mut ctx,
                    &gc,
                    sx,
                    expanded.as_ptr(),
                    ::core::ptr::null_mut::<style_ranges>(),
                    0 as ::core::ffi::c_int,
                );
                screen_write_stop(ctx);
            }
            (*wp).base.mode &= !MODE_CURSOR;
            (*wp).flags |= PANE_REDRAW;
            drop(window_owner);
            return;
        }
    }
    if notify != 0 {
        server_fire_pane_exit(c"pane-exited", pane_owner);
    }
    ClientRef::forget_pane(pane_owner);
    WindowRef::clone(((*wp).window_handle().as_ref()).expect("live window"))
        .remove_pane(pane_owner);
    if window_owner.pane_snapshot().is_empty() {
        server_kill_window(
            WindowRef::downgrade(((*wp).window_handle().as_ref()).expect("live window"))
                .upgrade()
                .expect("live pane window"),
            1,
        );
    } else {
        server_redraw_window(((*wp).window_handle().as_ref()).expect("live window"));
    };
    drop(window_owner);
}
pub(super) unsafe fn process_exited(
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    pid: pid_t,
    status: ExitStatus,
) -> bool {
    {
        let state = &mut *pane.get();
        if state.pid != pid {
            return false;
        }
        state.exit_status = Some(status);
        log_debug(format_args!("%{} exited", state.id));
        state.flags |= PANE_EXITED;
    }
    window_pane_wait_finish(pane);
    spawn_editor_finish(pane);
    if pane.destroy_ready() {
        finish_process(pane, 1);
    }
    true
}
