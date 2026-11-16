use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{CMD_AFTERHOOK, CMD_BUFFER_USAGE};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::CStr;
use std::future::Future;
use std::pin::Pin;
use tiri_core::arguments::{args_get, args_has, args_parse, args_string};
use tiri_core::file::{FileOutcome, file_write_wait};
use tiri_core::format::bytes::write_cstr;
use tiri_core::paste::{paste_buffer_data, paste_get_name, paste_get_top};
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int, uint64_t};
use tiri_core::shared::client::CLIENT_CONTROL;
use tiri_core::shared::errno::ErrorText;
use tiri_core::shared::posix_io::{O_APPEND, O_TRUNC};
use tiri_core::tiri_buffer::SegmentedBuf;

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

fn cmd_save_buffer_done(inv: &Invocation<'_>, outcome: &FileOutcome) {
    let error = outcome.error;
    if error != 0 as ::core::ffi::c_int {
        inv.error(|out| {
            write!(
                out,
                "{}",
                ErrorText(&std::io::Error::from_raw_os_error(error))
            )?;
            out.write_all(b": ")?;
            write_cstr(out, outcome.path.as_deref())
        });
    }
}

fn cmd_save_buffer_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let queue_client = inv.client();
        let args = cmd_get_args(self_0);
        let pb = if let Some(bufname) = args_get(args, 'b' as i32 as u_char) {
            let Some(pb) = paste_get_name(bufname) else {
                inv.error(|out| {
                    out.write_all(b"no buffer ")?;
                    write_cstr(out, bufname)
                });
                return ExecOutcome::err();
            };
            pb
        } else {
            let Some(pb) = paste_get_top(None) else {
                inv.error(|out| out.write_all(b"no buffers"));
                return ExecOutcome::err();
            };
            pb
        };
        let show_buffer = std::ptr::eq(cmd_get_entry(self_0), &cmd_show_buffer_entry);
        if show_buffer {
            let c_ref = queue_client.as_ref().expect("live client");
            if c_ref.attached_session().upgrade().is_some()
                || c_ref.flags() & CLIENT_CONTROL as uint64_t != 0
            {
                let mut evb = SegmentedBuf::default();
                {
                    let buffer = pb.try_borrow().expect("live paste buffer");
                    let bufdata = paste_buffer_data(&buffer).unwrap_or_default();
                    evb.extend_from_slice(bufdata);
                }
                inv.print_data(&mut evb);
                return ExecOutcome::ok();
            }
        }
        let expanded_path = (!show_buffer)
            .then(|| inv.format_single(args_string(args, 0 as u_int).unwrap_or(c"")));
        let path: &CStr = expanded_path.as_deref().unwrap_or(c"-");
        let flags = if args_has(args, 'a' as i32 as u_char) != 0 {
            O_APPEND
        } else {
            O_TRUNC
        };
        let wait = {
            let buffer = pb.try_borrow().expect("live paste buffer");
            let bufdata = paste_buffer_data(&buffer).unwrap_or_default();
            file_write_wait(queue_client.as_ref(), path, flags, bufdata)
        };
        drop(pb);
        drop(queue_client);
        inv.suspend(self_0);
        let Some(outcome) = wait.await else {
            return std::future::pending().await;
        };
        cmd_save_buffer_done(inv, &outcome);
        ExecOutcome::ok()
    })
}
