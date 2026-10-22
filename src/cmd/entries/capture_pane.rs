use crate::arguments::{args_get, args_has, args_parse};
use crate::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::control::control_write;
use crate::file::{file_can_print, file_print, file_print_buffer};
use crate::format::bytes::write_cstr;
use crate::paste::paste_set_owned;
use crate::server_client::Client as _;
use crate::server_fn::server_redraw_window;
use crate::shared::abi::*;
use crate::shared::client::{ClientRef, CLIENT_CONTROL};
use crate::shared::command::*;
use crate::window_pane::WindowPane as _;
use std::ffi::{CStr, CString};
pub static cmd_capture_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"capture-pane",
        alias: Some(c"capturep"),
        args: args_parse {
            template: c"ab:CeE:FHIJLMNpPqRS:Tt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage:
            c"[-aCeFHIJLMNpPqRT] [-b buffer-name] [-E end-line] [-S start-line] [-t target-pane]",
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
        exec: Some(cmd_capture_pane_exec),
    }
};
pub static cmd_clear_history_entry: cmd_entry = {
    cmd_entry {
        name: c"clear-history",
        alias: Some(c"clearhist"),
        args: args_parse {
            template: c"Ht:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-H] [-t target-pane]",
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
        exec: Some(cmd_capture_pane_exec),
    }
};
fn cmd_capture_pane_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let pane_owner = &mut target_state.wp.upgrade().expect("capture target pane");
    let mut buf: Vec<u8>;
    let mut cause: Option<CString> = None;
    let mut bufname: Option<&CStr> = None;
    if std::ptr::eq(cmd_get_entry(&cmd_ref), &cmd_clear_history_entry) {
        pane_owner.clear_history(args_has(args, b'H') != 0);
        let window = pane_owner
            .window_observer()
            .upgrade()
            .expect("clear-history pane window");
        server_redraw_window(&window);
        drop(window);
        return CmdOutcome::Normal;
    }
    buf = match pane_owner.capture(args, item_handle) {
        Ok(bytes) => bytes,
        Err(cause) => {
            cmdq_error(item_handle, |out| write_cstr(out, &*cause));
            return CmdOutcome::Error;
        }
    };
    if args_has(args, 'p' as i32 as u_char) != 0 {
        let len = if buf.last() == Some(&b'\n') {
            buf.len() - 1
        } else {
            buf.len()
        };
        if c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
            control_write(&c.clone().expect("live client"), |out| {
                out.write_all(&buf[..len])
            });
        } else {
            if file_can_print(c.as_ref()) == 0 {
                cmdq_error(item_handle, |out| out.write_all(b"can't write to client"));
                return CmdOutcome::Error;
            }
            file_print_buffer(c_owner.as_ref(), &buf[..len]);
            file_print(c_owner.as_ref(), |out| out.write_all(b"\n"));
        }
    } else {
        bufname = None;
        if args_has(args, 'b' as i32 as u_char) != 0 {
            bufname = args_get(args, 'b' as i32 as u_char);
        }
        if paste_set_owned(buf.into_boxed_slice(), bufname, Some(&mut cause))
            != 0 as ::core::ffi::c_int
        {
            cmdq_error(item_handle, |out| {
                write_cstr(out, (cause.as_ref().unwrap()).as_c_str())
            });
            return CmdOutcome::Error;
        }
    }
    CmdOutcome::Normal
}
