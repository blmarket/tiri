use crate::src::arguments::{args_count, args_has, args_string, args_strtonum_result};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::compat::strtonum::strtonum;
use crate::src::format::bytes::write_cstr;
use crate::src::resize::{default_window_size, recalculate_size};
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::limits::INT_MAX;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::winlink;
use crate::src::shared::window::{WINDOW_MAXIMUM, WINDOW_MINIMUM, WINDOW_SIZE_LARGEST};
use crate::src::window::Window as _;

pub const WINDOW_SIZE_SMALLEST: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub static cmd_resize_window_entry: cmd_entry = {
    cmd_entry {
        name: c"resize-window",
        alias: Some(c"resizew"),
        args: args_parse {
            template: c"aADLRt:Ux:y:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-aADLRU] [-x width] [-y height] [-t target-window] [adjustment]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_resize_window_exec),
    }
};
unsafe fn cmd_resize_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let window_owner = wl
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("resize target window");
    let result = (|| {
        let mut s: Option<SessionRef> = (*target).session_handle();
        let mut adjust: u_int = 0;
        let mut sx: u_int = 0;
        let mut sy: u_int = 0;
        let mut xpixel: u_int = 0 as u_int;
        let mut ypixel: u_int = 0 as u_int;
        if args_count(args) == 0 as u_int {
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
                return CMD_RETURN_ERROR;
            }
        }
        sx = window_owner.size().0;
        sy = window_owner.size().1;
        if args_has(args, 'x' as i32 as u_char) != 0 {
            sx = match args_strtonum_result(
                args,
                'x' as i32 as u_char,
                WINDOW_MINIMUM as ::core::ffi::c_longlong,
                WINDOW_MAXIMUM as ::core::ffi::c_longlong,
            ) {
                Ok(value) => value as u_int,
                Err(error) => {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"width ")?;
                        write_cstr(out, &*(error.message()))
                    });
                    return CMD_RETURN_ERROR;
                }
            };
        }
        if args_has(args, 'y' as i32 as u_char) != 0 {
            sy = match args_strtonum_result(
                args,
                'y' as i32 as u_char,
                WINDOW_MINIMUM as ::core::ffi::c_longlong,
                WINDOW_MAXIMUM as ::core::ffi::c_longlong,
            ) {
                Ok(value) => value as u_int,
                Err(error) => {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"height ")?;
                        write_cstr(out, &*(error.message()))
                    });
                    return CMD_RETURN_ERROR;
                }
            };
        }
        if args_has(args, 'L' as i32 as u_char) != 0 {
            if sx >= adjust {
                sx = sx.wrapping_sub(adjust);
            }
        } else if args_has(args, 'R' as i32 as u_char) != 0 {
            sx = sx.wrapping_add(adjust);
        } else if args_has(args, 'U' as i32 as u_char) != 0 {
            if sy >= adjust {
                sy = sy.wrapping_sub(adjust);
            }
        } else if args_has(args, 'D' as i32 as u_char) != 0 {
            sy = sy.wrapping_add(adjust);
        }
        if args_has(args, 'A' as i32 as u_char) != 0 {
            default_window_size(
                None,
                s.as_ref().expect("live session"),
                Some(&window_owner),
                &raw mut sx,
                &raw mut sy,
                &raw mut xpixel,
                &raw mut ypixel,
                WINDOW_SIZE_LARGEST,
            );
        } else if args_has(args, 'a' as i32 as u_char) != 0 {
            default_window_size(
                None,
                s.as_ref().expect("live session"),
                Some(&window_owner),
                &raw mut sx,
                &raw mut sy,
                &raw mut xpixel,
                &raw mut ypixel,
                WINDOW_SIZE_SMALLEST,
            );
        }
        window_owner.set_manual_size(sx, sy);
        recalculate_size(&window_owner, 1 as ::core::ffi::c_int);
        CMD_RETURN_NORMAL
    })();
    drop(window_owner);
    result
}
