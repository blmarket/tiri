use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::control::control_write;
use crate::src::file::{file_can_print, file_print, file_print_buffer};
use crate::src::format::bytes::write_cstr;
use crate::src::paste::paste_set_owned;
use crate::src::server_client::Client as _;
use crate::src::server_fn::server_redraw_window;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::{ClientRef, CLIENT_CONTROL};
use crate::src::shared::command::*;
use crate::src::window_pane::WindowPane as _;
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
            flag: 't' as i32 as ::core::ffi::c_char,
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
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_capture_pane_exec),
    }
};
unsafe fn cmd_capture_pane_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let pane_owner = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item)
        .wp
        .upgrade()
        .expect("capture target pane");
    let mut buf: Vec<u8>;
    let mut cause: Option<CString> = None;
    let mut bufname: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_clear_history_entry,
    ) {
        pane_owner.clear_history(args_has(args, b'H') != 0);
        let window = pane_owner
            .window_observer()
            .upgrade()
            .expect("clear-history pane window");
        server_redraw_window(&window);
        drop(window);
        return CMD_RETURN_NORMAL;
    }
    buf = match pane_owner.capture(&mut *args, item_handle) {
        Ok(bytes) => bytes,
        Err(cause) => {
            cmdq_error(item_handle, |out| write_cstr(out, &*cause));
            return CMD_RETURN_ERROR;
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
                return CMD_RETURN_ERROR;
            }
            file_print_buffer(c_owner.as_ref(), &buf[..len]);
            file_print(c_owner.as_ref(), |out| out.write_all(b"\n"));
        }
    } else {
        bufname = ::core::ptr::null::<::core::ffi::c_char>();
        if args_has(args, 'b' as i32 as u_char) != 0 {
            bufname = args_get(&*(args), 'b' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr());
        }
        if paste_set_owned(
            buf.into_boxed_slice(),
            (!bufname.is_null()).then(|| CStr::from_ptr(bufname)),
            Some(&mut cause),
        ) != 0 as ::core::ffi::c_int
        {
            cmdq_error(item_handle, |out| {
                write_cstr(out, (&*(cause.as_ref().unwrap())).as_c_str())
            });
            return CMD_RETURN_ERROR;
        }
    }
    CMD_RETURN_NORMAL
}
