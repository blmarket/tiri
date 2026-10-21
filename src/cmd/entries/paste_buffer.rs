use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::format::bytes::write_cstr;
use crate::paste::{paste_buffer_data, paste_free, paste_get_name, paste_get_top};
use crate::shared::abi::*;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::window_pane::WindowPane as _;
use std::ffi::CStr;
pub static cmd_paste_buffer_entry: cmd_entry = {
    cmd_entry {
        name: c"paste-buffer",
        alias: Some(c"pasteb"),
        args: args_parse {
            template: c"db:prSs:t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-dprS] [-s separator] [-b buffer-name] [-t target-pane]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_paste_buffer_exec),
    }
};
unsafe fn cmd_paste_buffer_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let pane_owner = target.pane_handle().expect("paste target pane");
    let pb;
    let mut bufname: Option<&CStr> = None;
    let mut bracket: ::core::ffi::c_int = args_has(args, 'p' as i32 as u_char);
    if !pane_owner.has_tty() || pane_owner.has_exited() {
        cmdq_error(item_handle, |out| out.write_all(b"target pane has exited"));
        return CmdOutcome::Error;
    }
    bufname = None;
    if args_has(args, 'b' as i32 as u_char) != 0 {
        bufname = args_get(args, 'b' as i32 as u_char);
    }
    if let Some(bufname) = bufname {
        pb = paste_get_name(bufname);
        if pb.is_none() {
            cmdq_error(item_handle, |out| {
                out.write_all(b"no buffer ")?;
                write_cstr(out, bufname)
            });
            return CmdOutcome::Error;
        }
    } else {
        pb = paste_get_top(None);
    }
    let Some(pb) = pb else {
        return CmdOutcome::Normal;
    };
    let separator = args_get(args, b's')
        .unwrap_or(if args_has(args, b'r') != 0 {
            c"\n"
        } else {
            c"\r"
        })
        .to_bytes();
    let bytes = {
        let buffer = pb.borrow();
        paste_buffer_data(&buffer).unwrap_or_default().to_vec()
    };
    pane_owner.paste_buffer(&bytes, separator, bracket != 0, args_has(args, b'S') != 0);
    if args_has(args, 'd' as i32 as u_char) != 0 {
        paste_free(&pb);
    }
    CmdOutcome::Normal
}
