use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_PANE, cmd, cmd_entry, cmd_entry_flag};
use std::ffi::{CStr, CString};
use tiri_core::arguments::{args_get, args_has, args_parse};
use tiri_core::control::control_write;
use tiri_core::file::{file_can_print, file_print, file_print_buffer};
use tiri_core::format::bytes::write_cstr;
use tiri_core::paste::paste_set_owned;
use tiri_core::server_client::Client as _;
use tiri_core::server_fn::server_redraw_window;
use tiri_core::shared::abi::{u_char, uint64_t};
use tiri_core::shared::client::{CLIENT_CONTROL, ClientRef};
use tiri_core::window_pane::WindowPane as _;
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
fn cmd_capture_pane_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let c_owner = inv.client();
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut target_state = inv.target().clone();
    let pane_owner = &mut target_state.wp.upgrade().expect("capture target pane");
    let mut buf: Vec<u8>;
    let mut cause: Option<CString> = None;
    let mut bufname: Option<&CStr> = None;
    if std::ptr::eq(cmd_get_entry(self_0), &cmd_clear_history_entry) {
        pane_owner.clear_history(args_has(args, b'H') != 0);
        let window = pane_owner
            .window_observer()
            .upgrade()
            .expect("clear-history pane window");
        server_redraw_window(&window);
        drop(window);
        return ExecOutcome::ok();
    }
    let cf = inv.format();
    let tc = inv.target_client();
    buf = match pane_owner.capture(args, Some(&cf), tc.as_ref(), &target_state) {
        Ok(bytes) => bytes,
        Err(cause) => {
            inv.error( |out| write_cstr(out, &*cause));
            return ExecOutcome::err();
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
                inv.error( |out| out.write_all(b"can't write to client"));
                return ExecOutcome::err();
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
            inv.error( |out| {
                write_cstr(out, (cause.as_ref().unwrap()).as_c_str())
            });
            return ExecOutcome::err();
        }
    }
    ExecOutcome::ok()
})
}
