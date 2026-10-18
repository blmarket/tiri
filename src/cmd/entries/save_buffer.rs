use crate::src::arguments::{args, args_get, args_has, args_parse, args_string};
use crate::src::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print_data, cmdq_resumed};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::file::{file_write_wait, FileOutcome};
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::paste::{paste_buffer_data, paste_get_name, paste_get_top};
use crate::src::reactor::{evbuffer_add, evbuffer_new};
use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_CONTROL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::command::{CMD_AFTERHOOK, CMD_BUFFER_USAGE};
use crate::src::shared::posix_io::{O_APPEND, O_TRUNC};
use std::ffi::CStr;
pub static cmd_save_buffer_entry: cmd_entry = {
    cmd_entry {
        name: c"save-buffer",
        alias: Some(c"saveb"),
        args: args_parse {
            template: c"ab:",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-a] [-b buffer-name] path",
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_save_buffer_exec),
    }
};
pub static cmd_show_buffer_entry: cmd_entry = {
    cmd_entry {
        name: c"show-buffer",
        alias: Some(c"showb"),
        args: args_parse {
            template: c"b:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: CMD_BUFFER_USAGE,
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_save_buffer_exec),
    }
};
unsafe fn cmd_save_buffer_done(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    outcome: &FileOutcome,
) {
    let error = outcome.error;
    if error != 0 as ::core::ffi::c_int {
        cmdq_error(item_handle, |out| {
            write!(out, "{}", errno::Errno(error))?;
            out.write_all(b": ")?;
            write_cstr(out, outcome.path.as_deref())
        });
    }
}
unsafe fn cmd_save_buffer_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let queue_client = cmdq_get_client(Some(&*item));
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let pb;
    let mut flags: ::core::ffi::c_int = 0;
    if let Some(bufname) = args_get(&*(args), 'b' as i32 as u_char) {
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
        if pb.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"no buffers"));
            return CmdOutcome::Error;
        }
    }
    let pb = pb.expect("buffer lookup checked above");
    let show_buffer = std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_show_buffer_entry,
    );
    if show_buffer
        && (!c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
            || c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0)
    {
        let mut evb = evbuffer_new();
        {
            let buffer = pb.borrow();
            let bufdata = paste_buffer_data(&buffer).unwrap_or_default();
            evbuffer_add(&mut evb, bufdata);
        }
        cmdq_print_data(item_handle, &mut evb);
        return CmdOutcome::Normal;
    }
    let expanded_path = (!show_buffer).then(|| {
        format_single_from_target_cstring(
            item_handle,
            args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
        )
    });
    let dash_path = c"-";
    let path: &CStr = expanded_path.as_deref().unwrap_or(dash_path);
    if args_has(&*args, 'a' as i32 as u_char) != 0 {
        flags = O_APPEND;
    } else {
        flags = O_TRUNC;
    }
    let buffer = pb.borrow();
    let bufdata = paste_buffer_data(&buffer).unwrap_or_default();
    let wait = file_write_wait(queue_client.as_ref(), path, flags, bufdata);
    let waiting_item = std::rc::Rc::downgrade(item_handle);
    CmdOutcome::Suspend(Box::pin(async move {
        let Some(outcome) = wait.await else {
            return std::future::pending().await;
        };
        cmd_save_buffer_done(&cmdq_resumed(&waiting_item), &outcome);
    }))
}
