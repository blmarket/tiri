use crate::src::arguments::{
    args, args_count, args_flag_values, args_get, args_has, args_parse, args_string,
};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_target_client};
use crate::src::compat::strtonum::strtonum;
use crate::src::control::{
    control_add_sub, control_clear_window_size, control_continue_pane, control_pause_pane,
    control_remove_sub, control_set_pane_off, control_set_pane_on, control_set_window_size,
};
use crate::src::format::bytes::cstr_suffix;
use crate::src::format::bytes::write_cstr;
use crate::src::log::{log_cstr, log_debug};
use crate::src::monitor::monitor_parse_owned;
use crate::src::resize::recalculate_sizes_now;
use libc::sscanf;

use crate::src::server_client::Client as _;
use crate::src::server_fn::{server_redraw_client, server_status_client};
use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::{CLIENT_CONTROL, CLIENT_STATUSFORCE, CLIENT_WINDOWSIZECHANGED};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_TFLAG};
use crate::src::shared::layout::Direction;
use crate::src::shared::limits::INT_MAX;
use crate::src::shared::pane::window_pane;
use crate::src::shared::window::{WINDOW_MAXIMUM, WINDOW_MINIMUM};
use crate::src::tty::{tty_clipboard_query, tty_update_client_offset};
use crate::src::tty_keys::tty_keys_colours;
use crate::src::window_pane::WindowPane as _;
use std::ffi::{CStr, CString};
pub static cmd_refresh_client_entry: cmd_entry = {
    cmd_entry {
        name: c"refresh-client",
        alias: Some(c"refresh"),
        args: args_parse {
            template: c"A:B:cC:Df:r:F:lLRSt:U",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-cDlLRSU] [-A pane:state] [-B name:what:format] [-C XxY] [-f flags] [-r pane:report] [-t target-client] [adjustment]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_TFLAG,
        exec: Some(cmd_refresh_client_exec),
    }
};
unsafe fn cmd_refresh_client_update_subscription(tc_owner: &ClientRef, value: &CStr) {
    let Some(parsed) = monitor_parse_owned(value) else {
        control_remove_sub(tc_owner, value);
        return;
    };
    control_add_sub(
        tc_owner,
        &parsed.name,
        parsed.type_0,
        parsed.id,
        &parsed.format,
    );
}
unsafe fn cmd_refresh_client_control_client_size(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut size: *const ::core::ffi::c_char =
        args_get(&*(args), 'C' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut w: u_int = 0;
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    if sscanf(
        size,
        c"@%u:%ux%u".as_ptr(),
        &raw mut w,
        &raw mut x,
        &raw mut y,
    ) == 3 as ::core::ffi::c_int
    {
        if x < WINDOW_MINIMUM as u_int
            || x > WINDOW_MAXIMUM as u_int
            || y < WINDOW_MINIMUM as u_int
            || y > WINDOW_MAXIMUM as u_int
        {
            cmdq_error(item_handle, |out| {
                out.write_all(b"size too small or too big")
            });
            return CmdOutcome::Error;
        }
        log_debug(format_args!(
            "{}: client {} window @{}: size {}x{}",
            "cmd_refresh_client_control_client_size",
            log_cstr(
                (tc.as_ref().expect("live client").name())
                    .as_deref()
                    .unwrap_or(c"(null)")
            ),
            (w) as u32,
            (x) as u32,
            (y) as u32
        ));
        control_set_window_size(tc.as_ref().expect("live client"), w, x, y);
        tc.as_ref()
            .expect("live client")
            .update_flags(CLIENT_WINDOWSIZECHANGED as uint64_t, 0);
        recalculate_sizes_now(1 as ::core::ffi::c_int);
        return CmdOutcome::Normal;
    }
    if sscanf(size, c"@%u:".as_ptr(), &raw mut w) == 1 as ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: client {} window @{}: no size",
            "cmd_refresh_client_control_client_size",
            log_cstr(
                (tc.as_ref().expect("live client").name())
                    .as_deref()
                    .unwrap_or(c"(null)")
            ),
            (w) as u32
        ));
        control_clear_window_size(tc.as_ref().expect("live client"), w);
        recalculate_sizes_now(1 as ::core::ffi::c_int);
        return CmdOutcome::Normal;
    }
    if sscanf(size, c"%u,%u".as_ptr(), &raw mut x, &raw mut y) != 2 as ::core::ffi::c_int
        && sscanf(size, c"%ux%u".as_ptr(), &raw mut x, &raw mut y) != 2 as ::core::ffi::c_int
    {
        cmdq_error(item_handle, |out| out.write_all(b"bad size argument"));
        return CmdOutcome::Error;
    }
    if x < WINDOW_MINIMUM as u_int
        || x > WINDOW_MAXIMUM as u_int
        || y < WINDOW_MINIMUM as u_int
        || y > WINDOW_MAXIMUM as u_int
    {
        cmdq_error(item_handle, |out| {
            out.write_all(b"size too small or too big")
        });
        return CmdOutcome::Error;
    }
    tc.as_ref().expect("live client").set_control_size(x, y);
    recalculate_sizes_now(1 as ::core::ffi::c_int);
    CmdOutcome::Normal
}
fn cmd_refresh_parse_pane(value: &CStr) -> Option<(u_int, &CStr)> {
    let bytes = value.to_bytes();
    if bytes.first() != Some(&b'%') {
        return None;
    }
    let colon = bytes.iter().position(|&byte| byte == b':')?;
    let prefix = CString::new(&bytes[..colon]).expect("CStr prefix contains no NUL");
    let suffix = cstr_suffix(value, colon + 1);
    let mut pane = 0;
    let matched = unsafe { sscanf(prefix.as_ptr(), c"%%%u".as_ptr(), &mut pane) };
    (matched == 1).then_some((pane, suffix))
}

unsafe fn cmd_refresh_client_update_offset(tc_owner: &ClientRef, value: &CStr) {
    let Some((pane, action)) = cmd_refresh_parse_pane(value) else {
        return;
    };
    let Some(pane_owner) = std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::find_by_id(pane)
    else {
        return;
    };
    match action.to_bytes() {
        b"on" => control_set_pane_on(tc_owner, &pane_owner),
        b"off" => control_set_pane_off(tc_owner, &pane_owner),
        b"continue" => control_continue_pane(tc_owner, &pane_owner),
        b"pause" => control_pause_pane(tc_owner, &pane_owner),
        _ => {}
    }
}

unsafe fn cmd_refresh_report(client: &ClientRef, value: &CStr) {
    let Some((pane, report)) = cmd_refresh_parse_pane(value) else {
        return;
    };
    let Some(pane_owner) = std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::find_by_id(pane)
    else {
        return;
    };
    let (mut fg, mut bg) = pane_owner.control_colours();
    let mut size: size_t = 0;
    let diagnostic_name = client.name();
    let parsed = {
        let mut tty = client.borrow_terminal_mut();
        tty_keys_colours(
            &mut tty.flags,
            diagnostic_name.as_deref(),
            report.to_bytes(),
            &mut size,
            &mut fg,
            &mut bg,
        )
    };
    if parsed == 0 {
        pane_owner.update_control_colours(fg, bg);
    }
}

unsafe fn cmd_refresh_client_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut adjust: u_int = 0;
    if args_has(&*args, 'c' as i32 as u_char) != 0
        || args_has(&*args, 'L' as i32 as u_char) != 0
        || args_has(&*args, 'R' as i32 as u_char) != 0
        || args_has(&*args, 'U' as i32 as u_char) != 0
        || args_has(&*args, 'D' as i32 as u_char) != 0
    {
        if args_count(&*args) == 0 as u_int {
            adjust = 1 as u_int;
        } else {
            let parsed = strtonum(
                args_string(&mut *(args), 0 as u_int).expect("numeric argument"),
                1 as ::core::ffi::c_longlong,
                INT_MAX as ::core::ffi::c_longlong,
            );
            adjust = parsed.unwrap_or(0) as u_int;
            if let Err(errstr) = parsed {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"adjustment ")?;
                    write_cstr(out, errstr)
                });
                return CmdOutcome::Error;
            }
        }
        let client = tc.as_ref().expect("live client");
        if args_has(&*args, b'c') != 0 {
            client.reset_pan(None);
        } else {
            let window = client
                .attached_session()
                .upgrade()
                .expect("live session")
                .current_winlink()
                .get_unchecked()
                .window_handle()
                .cloned()
                .expect("current window");
            let direction = if args_has(&*args, b'L') != 0 {
                Direction::Left
            } else if args_has(&*args, b'R') != 0 {
                Direction::Right
            } else if args_has(&*args, b'U') != 0 {
                Direction::Up
            } else {
                Direction::Down
            };
            client.pan_window(&window, direction, adjust);
        }
        tty_update_client_offset(&tc.clone().expect("live client"));
        server_redraw_client(tc.as_ref().expect("live client"));
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'l' as i32 as u_char) != 0 {
        tty_clipboard_query(tc.as_ref().expect("live client"));
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'F' as i32 as u_char) != 0 {
        tc.clone()
            .expect("live client")
            .parse_flags(args_get(&*(args), 'F' as i32 as u_char).expect("-F has a value"));
    }
    if args_has(&*args, 'f' as i32 as u_char) != 0 {
        tc.clone()
            .expect("live client")
            .parse_flags(args_get(&*(args), 'f' as i32 as u_char).expect("-f has a value"));
    }
    if args_has(&*args, 'r' as i32 as u_char) != 0 {
        cmd_refresh_report(
            tc.as_ref().expect("live client"),
            args_get(&*(args), 'r' as i32 as u_char).expect("-r takes a value"),
        );
    }
    if args_has(&*args, 'A' as i32 as u_char) != 0 {
        if !(!tc.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0) {
            for av in args_flag_values(&*args, 'A' as i32 as u_char) {
                cmd_refresh_client_update_offset(
                    &tc.clone().expect("live client"),
                    av.as_string().expect("-A takes a value"),
                );
            }
            return CmdOutcome::Normal;
        }
    } else if args_has(&*args, 'B' as i32 as u_char) != 0 {
        if !(!tc.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0) {
            for av in args_flag_values(&*args, 'B' as i32 as u_char) {
                cmd_refresh_client_update_subscription(
                    &tc.clone().expect("live client"),
                    av.as_string().expect("-B takes a value"),
                );
            }
            return CmdOutcome::Normal;
        }
    } else if args_has(&*args, 'C' as i32 as u_char) != 0 {
        if !(!tc.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0) {
            return cmd_refresh_client_control_client_size(self_0.clone(), item_handle);
        }
    } else {
        if args_has(&*args, 'S' as i32 as u_char) != 0 {
            tc.as_ref()
                .expect("live client")
                .update_flags(CLIENT_STATUSFORCE as uint64_t, 0);
            server_status_client(tc.as_ref().expect("live client"));
        } else {
            tc.as_ref()
                .expect("live client")
                .update_flags(CLIENT_STATUSFORCE as uint64_t, 0);
            server_redraw_client(tc.as_ref().expect("live client"));
        }
        return CmdOutcome::Normal;
    }
    cmdq_error(item_handle, |out| out.write_all(b"not a control client"));
    CmdOutcome::Error
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[test]
    fn pane_argument_keeps_scanf_and_byte_boundaries() {
        for (input, pane, suffix) in [
            (b"%42:on".as_slice(), 42, b"on".as_slice()),
            (b"% +7junk:off", 7, b"off"),
            (b"%-1:pause", u_int::MAX, b"pause"),
            (b"%0:", 0, b""),
            (b"%9:\xff:rest", 9, b"\xff:rest"),
        ] {
            let input = CString::new(input).unwrap();
            let (actual_pane, actual_suffix) = cmd_refresh_parse_pane(&input).unwrap();
            assert_eq!(actual_pane, pane);
            assert_eq!(actual_suffix.to_bytes(), suffix);
        }
        for input in [c"", c"1:on", c"%1", c"%:on", c"%x:on"] {
            assert!(cmd_refresh_parse_pane(input).is_none(), "{input:?}");
        }
        let input = CStr::from_bytes_until_nul(b"%1:on\0:off").unwrap();
        assert_eq!(cmd_refresh_parse_pane(input).unwrap(), (1, c"on"));
    }
}
