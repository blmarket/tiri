use crate::arguments::{args, args_count, args_has, args_parse, args_string};
use crate::cfg::{cfg_finished, cfg_print_causes, load_cfg_from_buffer};
use crate::cmd::queue::{
    cmdq_error, cmdq_get_callback_owned, cmdq_get_client, cmdq_insert_after, cmdq_resumed,
};
use crate::cmd::{cmd_get_args_mut, cmd_get_parse_flags};
use crate::compat::glob::GlobResult;
use crate::file::{file_read_wait, FileOutcome, FileWait};
use crate::format::bytes::write_cstr;
use crate::format::format_single_from_target_cstring;
use crate::log::{log_cstr, log_debug};
use crate::reactor::evbuffer_pullup;
use crate::server_client::Client as _;

use crate::shared::abi::__size_t;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_CONTROL;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{
    CMD_FIND_CANFAIL, CMD_PARSE_PARSEONLY, CMD_PARSE_QUIET, CMD_PARSE_VERBOSE,
    CMD_SOURCE_FILE_DEPTH_LIMIT,
};
use crate::shared::errno::ErrorText;
use libc::{EINVAL, ENOENT, ENOMEM};
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

pub struct cmd_source_file_data {
    pub client: Option<ClientRef>,
    depth_active: bool,
    pub flags: ::core::ffi::c_int,
    pub after: Weak<std::cell::RefCell<cmdq_item>>,
    pub failed: bool,
    pub current: u_int,
    pub files: Vec<CString>,
}

pub use ::libc::{GLOB_NOMATCH, GLOB_NOSPACE};
static mut cmd_source_file_depth: u_int = 0;
pub static cmd_source_file_entry: cmd_entry = {
    cmd_entry {
        name: c"source-file",
        alias: Some(c"source"),
        args: args_parse {
            template: c"t:Fnqv",
            lower: 1 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-Fnqv] [-t target-pane] path ...",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_source_file_exec),
    }
};
impl cmd_source_file_data {
    fn client_handle(&self) -> Option<&ClientRef> {
        self.client.as_ref()
    }

    fn decrement_depth(&mut self) {
        if !std::mem::replace(&mut self.depth_active, false) {
            return;
        }
        unsafe {
            let depth = if let Some(client) = &self.client {
                client.leave_source_file()
            } else {
                {
                    cmd_source_file_depth = cmd_source_file_depth.wrapping_sub(1);
                    cmd_source_file_depth
                }
            };
            log_debug(format_args!(
                "{}: depth now {}",
                "cmd_source_file_complete_cb", depth
            ));
        }
    }
}

impl Drop for cmd_source_file_data {
    fn drop(&mut self) {
        self.decrement_depth();
        drop(std::mem::take(&mut self.files));
        if let Some(client) = self.client.take() {
            drop(client);
        }
    }
}

unsafe fn cmd_source_file_complete_cb(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut cdata: cmd_source_file_data,
) -> CmdOutcome {
    cdata.decrement_depth();
    cfg_print_causes(item_handle);
    CmdOutcome::Normal
}

unsafe fn cmd_source_file_complete(
    mut cdata: Box<cmd_source_file_data>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) {
    let c = cdata.client_handle();
    if cfg_finished == 0 {
        cdata.depth_active = false;
        return;
    }
    if cdata.failed {
        if let Some(client) = c {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(1);
            }
        }
    }
    let after_owner = cdata
        .after
        .upgrade()
        .unwrap_or_else(|| Rc::clone(item_handle));
    let new_item_allocation = cmdq_get_callback_owned(
        c"cmd_source_file_complete_cb",
        Some(Box::new(move |item| unsafe {
            cmd_source_file_complete_cb(item, *cdata)
        })),
    );
    cmdq_insert_after(&after_owner, new_item_allocation);
}

unsafe fn cmd_source_file_read(cdata: &cmd_source_file_data) -> FileWait {
    let path = &cdata.files[cdata.current as usize];
    file_read_wait(cdata.client.as_ref(), path, |_| None)
}

/// Load each file once it is read, after the previous file's commands, then
/// queue the completion. A file whose client died never reports (tmux's
/// file_fire_done_cb): release the data, which leaves the nesting depth, and
/// park.
async fn cmd_source_file_chain(
    first: impl std::future::Future<Output = Option<FileOutcome>>,
    mut cdata: Box<cmd_source_file_data>,
    item: Weak<std::cell::RefCell<cmdq_item>>,
) {
    let mut outcome = first.await;
    loop {
        let Some(read) = outcome else {
            drop(cdata);
            return std::future::pending().await;
        };
        let item_owner = unsafe { cmdq_resumed(&item) };
        unsafe { cmd_source_file_done(&mut cdata, &item_owner, read) };
        cdata.current = cdata.current.wrapping_add(1);
        if (cdata.current as usize) >= cdata.files.len() {
            unsafe { cmd_source_file_complete(cdata, &item_owner) };
            return;
        }
        drop(item_owner);
        outcome = unsafe { cmd_source_file_read(&cdata) }.await;
    }
}

unsafe fn cmd_source_file_done(
    cdata: &mut cmd_source_file_data,
    item_owner: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut read: FileOutcome,
) {
    let item = item_owner.borrow();
    let path = read.path.as_deref();
    let error = read.error;
    let bdata: &[u8] = evbuffer_pullup(&mut read.buffer, -1).map_or(&[], |bytes| bytes);
    let bsize = bdata.len();
    let mut new_item = Weak::new();
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let target = &mut target_state;
    if error != 0 {
        cmdq_error(item_owner, |out| {
            write!(
                out,
                "{}",
                ErrorText(&std::io::Error::from_raw_os_error(error))
            )?;
            out.write_all(b": ")?;
            write_cstr(out, path)
        });
    } else if bsize != 0 {
        let after_owner = cdata
            .after
            .upgrade()
            .unwrap_or_else(|| Rc::clone(item_owner));
        if load_cfg_from_buffer(
            bdata,
            path.expect("file read sets the path"),
            cdata.client.as_ref(),
            Some(&after_owner),
            Some(&*target),
            cdata.flags,
            Some(&mut new_item),
        ) < 0
        {
            cdata.failed = true;
        } else if new_item.strong_count() != 0 {
            cdata.after = new_item;
        }
    }
}
fn cmd_source_file_add(cdata: &mut cmd_source_file_data, path: &CStr) {
    log_debug(format_args!(
        "{}: {}",
        "cmd_source_file_add",
        log_cstr(path)
    ));
    cdata.files.push(path.to_owned());
}
unsafe fn cmd_source_file_quote_for_glob(path: &CStr) -> CString {
    let mut quoted = Vec::new();
    for &byte in path.to_bytes() {
        if byte < 128 && !byte.is_ascii_alphanumeric() && byte != b'/' {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    CString::new(quoted).expect("C string path has no interior NUL")
}
unsafe fn cmd_source_file_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut failed = false;
    let mut parse_flags: ::core::ffi::c_int = 0;
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    match &c {
        None => {
            if cmd_source_file_depth >= CMD_SOURCE_FILE_DEPTH_LIMIT as u_int {
                cmdq_error(item_handle, |out| out.write_all(b"too many nested files"));
                return CmdOutcome::Error;
            }
            cmd_source_file_depth = cmd_source_file_depth.wrapping_add(1);
            log_debug(format_args!("{}: depth now {}", "cmd_source_file_exec", {
                cmd_source_file_depth
            }));
        }
        Some(client) => {
            let Some(depth) = client.enter_source_file(CMD_SOURCE_FILE_DEPTH_LIMIT as u32) else {
                cmdq_error(item_handle, |out| out.write_all(b"too many nested files"));
                return CmdOutcome::Error;
            };
            log_debug(format_args!(
                "{}: depth now {}",
                "cmd_source_file_exec", depth
            ));
        }
    }
    let mut cdata = Box::new(cmd_source_file_data {
        client: if c.is_none() { None } else { c.clone() },
        depth_active: true,
        flags: 0,
        after: Weak::new(),
        failed: false,
        current: 0,
        files: Vec::new(),
    });
    if args_has(&*args, 'q' as i32 as u_char) != 0 {
        cdata.flags |= CMD_PARSE_QUIET;
    }
    if args_has(&*args, 'n' as i32 as u_char) != 0 {
        cdata.flags |= CMD_PARSE_PARSEONLY;
    }
    if c.is_none() || !c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
        parse_flags = cmd_get_parse_flags(self_0.clone());
        if args_has(&*args, 'v' as i32 as u_char) != 0 || parse_flags & CMD_PARSE_VERBOSE != 0 {
            cdata.flags |= CMD_PARSE_VERBOSE;
        }
    }
    let cwd_owner =
        ClientRef::working_directory(c.as_ref(), None).expect("source working directory");
    let cwd = cmd_source_file_quote_for_glob(&cwd_owner);
    i = 0 as u_int;
    while i < args_count(&*args) {
        let mut path: &CStr = args_string(&mut *(args), i).expect("argument is present");
        let expanded = if args_has(&*args, 'F' as i32 as u_char) != 0 {
            Some(format_single_from_target_cstring(
                item_handle,
                path.as_ptr(),
            ))
        } else {
            None
        };
        if let Some(expanded) = expanded.as_deref() {
            path = expanded;
        }
        if path == c"-" {
            cmd_source_file_add(&mut cdata, c"-");
        } else {
            let pattern = if path.to_bytes().first() == Some(&b'/') {
                path.to_owned()
            } else {
                let cwd_bytes = cwd.to_bytes();
                let path_bytes = path.to_bytes();
                let mut bytes = Vec::with_capacity(cwd_bytes.len() + 1 + path_bytes.len());
                bytes.extend_from_slice(cwd_bytes);
                bytes.push(b'/');
                bytes.extend_from_slice(path_bytes);
                CString::new(bytes).expect("C strings have no interior NUL")
            };
            log_debug(format_args!(
                "{}: {}",
                "cmd_source_file_exec",
                log_cstr(&pattern)
            ));
            let (matches, result) = GlobResult::run(pattern.as_c_str());
            if result != 0 as ::core::ffi::c_int {
                if result != GLOB_NOMATCH || !cdata.flags & CMD_PARSE_QUIET != 0 {
                    let error = if result == GLOB_NOMATCH {
                        ENOENT
                    } else if result == GLOB_NOSPACE {
                        ENOMEM
                    } else {
                        EINVAL
                    };
                    cmdq_error(item_handle, |out| {
                        write!(
                            out,
                            "{}",
                            ErrorText(&std::io::Error::from_raw_os_error(error))
                        )?;
                        out.write_all(b": ")?;
                        write_cstr(out, path)
                    });
                    failed = true;
                }
                drop(pattern);
            } else {
                drop(pattern);
                j = 0 as u_int;
                while (j as __size_t) < matches.len() {
                    cmd_source_file_add(&mut cdata, matches.path(j as usize));
                    j = j.wrapping_add(1);
                }
            }
        }
        i = i.wrapping_add(1);
    }
    cdata.after = std::rc::Rc::downgrade(item_handle);
    cdata.failed = failed;
    if !cdata.files.is_empty() {
        let first = cmd_source_file_read(&cdata);
        return CmdOutcome::Suspend(Box::pin(cmd_source_file_chain(
            first,
            cdata,
            std::rc::Rc::downgrade(item_handle),
        )));
    }
    cmd_source_file_complete(cdata, item_handle);
    if failed {
        CmdOutcome::Error
    } else {
        CmdOutcome::Normal
    }
}
