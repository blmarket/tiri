use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::queue::cmdq_error;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::options::options_get_number;
use crate::resize::recalculate_sizes;
use crate::server_fn::{server_link_window, server_status_session, server_unlink_window};

use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::command::{CMD_FIND_QUIET, CMD_FIND_WINDOW_INDEX};
use crate::shared::session::SessionRef;
use crate::shared::window::winlink;
use crate::window::winlink_shuffle_up;
pub static cmd_move_window_entry: cmd_entry = {
    cmd_entry {
        name: c"move-window",
        alias: Some(c"movew"),
        args: args_parse {
            template: c"abdkrs:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-abdkr] [-s src-window] [-t dst-window]",
        source: cmd_entry_flag {
            flag: b's',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_move_window_exec),
    }
};
pub static cmd_link_window_entry: cmd_entry = {
    cmd_entry {
        name: c"link-window",
        alias: Some(c"linkw"),
        args: args_parse {
            template: c"abdks:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-abdk] [-s src-window] [-t dst-window]",
        source: cmd_entry_flag {
            flag: b's',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_move_window_exec),
    }
};
unsafe fn cmd_move_window_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let source = crate::cmd::queue::cmdq_get_source(&item);
    let mut target: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut tflag: *const ::core::ffi::c_char =
        args_get(args, 't' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut src: Option<SessionRef> = source.session_handle();
    let mut dst: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = source.winlink_handle();
    let mut idx: ::core::ffi::c_int = 0;
    let mut kflag: ::core::ffi::c_int = 0;
    let mut dflag: ::core::ffi::c_int = 0;
    let mut sflag: ::core::ffi::c_int = 0;
    let mut before: ::core::ffi::c_int = 0;
    if args_has(args, 'r' as i32 as u_char) != 0 {
        if target.find_target(
            Some(item_handle),
            nullable_cstr(tflag),
            CMD_FIND_SESSION,
            CMD_FIND_QUIET,
        ) != 0 as ::core::ffi::c_int
        {
            return CmdOutcome::Error;
        }
        target
            .session_handle()
            .expect("live session")
            .renumber_windows();
        recalculate_sizes();
        server_status_session(&target.session_handle().expect("live session"));
        return CmdOutcome::Normal;
    }
    if target.find_target(
        Some(item_handle),
        nullable_cstr(tflag),
        CMD_FIND_WINDOW,
        CMD_FIND_WINDOW_INDEX,
    ) != 0 as ::core::ffi::c_int
    {
        return CmdOutcome::Error;
    }
    dst = target.session_handle();
    idx = target.idx;
    kflag = args_has(args, 'k' as i32 as u_char);
    dflag = args_has(args, 'd' as i32 as u_char);
    sflag = args_has(args, 's' as i32 as u_char);
    before = args_has(args, 'b' as i32 as u_char);
    if args_has(args, 'a' as i32 as u_char) != 0 || before != 0 {
        if target.winlink_handle().is_alive() {
            idx = winlink_shuffle_up(
                dst.as_ref().expect("live session"),
                (target.winlink_handle()).clone(),
                before,
            );
        } else {
            idx = winlink_shuffle_up(
                dst.as_ref().expect("live session"),
                (dst.as_ref().expect("live session").current_winlink()).clone(),
                before,
            );
        }
        if idx == -(1 as ::core::ffi::c_int) {
            return CmdOutcome::Error;
        }
    }
    if let Err(cause) = server_link_window(
        src.as_ref().expect("live session"),
        wl.clone(),
        dst.as_ref().expect("live session"),
        idx,
        kflag,
        (dflag == 0) as ::core::ffi::c_int,
    ) {
        cmdq_error(item_handle, |out| write_cstr(out, &*cause));
        return CmdOutcome::Error;
    }
    if std::ptr::eq(cmd_get_entry(&cmd_ref), &cmd_move_window_entry) {
        server_unlink_window(src.as_ref().expect("live session"), wl.clone());
    }
    if sflag == 0
        && options_get_number(
            &src.as_ref().expect("live session").borrow_options_mut(),
            c"renumber-windows",
        ) != 0
    {
        (src.as_ref().expect("live session")).renumber_windows();
    }
    recalculate_sizes();
    CmdOutcome::Normal
}
