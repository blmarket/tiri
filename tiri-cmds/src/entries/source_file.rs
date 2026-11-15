use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::parse::cmd_parse_from_buffer;
use crate::cmd::{cmd_get_args, cmd_get_parse_flags};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{
    CMD_FIND_CANFAIL, CMD_PARSE_PARSEONLY, CMD_PARSE_QUIET, CMD_PARSE_VERBOSE,
    CMD_SOURCE_FILE_DEPTH_LIMIT,
};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_list, cmd_parse_input};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use tiri_core::arguments::{args_count, args_has, args_parse, args_string};
use tiri_core::cfg::{cfg_add_cause, cfg_finished, cfg_print_causes};
use tiri_core::file::{FileOutcome, FileWait, file_read_wait};
use tiri_core::format::bytes::write_cstr;
use tiri_core::libc::{EINVAL, ENOENT, ENOMEM, GLOB_NOMATCH, GLOB_NOSPACE};
use tiri_core::log::{log_cstr, log_debug};
use tiri_core::reactor::{LaneTarget, evbuffer_pullup};
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int, uint64_t};
use tiri_core::shared::client::CLIENT_CONTROL;
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::errno::ErrorText;
use tiri_core::shared::target::Target as cmd_find_state;

pub struct cmd_source_file_data {
    pub client: Option<ClientRef>,
    depth_active: bool,
    pub flags: ::core::ffi::c_int,
    pub failed: bool,
    pub current: u_int,
    pub files: Vec<CString>,
}

thread_local! {
    static SOURCE_FILE_DEPTH: std::cell::Cell<u_int> = const { std::cell::Cell::new(0) };
}

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
            flag: b't',
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
        let depth = if let Some(client) = &self.client {
            client.leave_source_file()
        } else {
            let depth = SOURCE_FILE_DEPTH.get().wrapping_sub(1);
            SOURCE_FILE_DEPTH.set(depth);
            depth
        };
        log_debug(format_args!(
            "{}: depth now {}",
            "cmd_source_file_complete_cb", depth
        ));
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

fn cmd_source_file_read(cdata: &cmd_source_file_data) -> FileWait {
    let path = &cdata.files[cdata.current as usize];
    file_read_wait(cdata.client.as_ref(), path, |_| None)
}

async fn cmd_source_file_chain<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
    first: impl Future<Output = Option<FileOutcome>>,
    mut cdata: Box<cmd_source_file_data>,
) -> ExecOutcome {
    inv.suspend(self_0);
    let mut loaded: Vec<(Rc<cmd_list>, Option<cmd_find_state>, CString)> = Vec::new();
    let mut outcome = first.await;
    loop {
        let Some(mut read) = outcome else {
            drop(loaded);
            drop(cdata);
            return std::future::pending().await;
        };
        let path = read.path.as_deref();
        let error = read.error;
        let bdata: &[u8] = evbuffer_pullup(&mut read.buffer, -1).map_or(&[], |bytes| bytes);
        let target_state = inv.target().clone();
        if error != 0 {
            inv.error(|out| {
                write!(
                    out,
                    "{}",
                    ErrorText(&std::io::Error::from_raw_os_error(error))
                )?;
                out.write_all(b": ")?;
                write_cstr(out, path)
            });
        } else if !bdata.is_empty() {
            let file_path = path.expect("file read sets the path");
            let mut pi = cmd_parse_input {
                flags: cdata.flags,
                file: Some(file_path.to_owned()),
                line: 1 as u_int,
                format: Some(inv.format()),
                c: cdata
                    .client
                    .as_ref()
                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                fs: Default::default(),
            };
            log_debug(format_args!("loading {}", log_cstr(file_path)));
            match cmd_parse_from_buffer(bdata, Some(&mut pi)) {
                Ok(commands) => {
                    if cdata.flags & CMD_PARSE_PARSEONLY == 0 {
                        let current_file = pi.file.unwrap_or_else(|| c"(null)".to_owned());
                        loaded.push((commands, Some(target_state), current_file));
                    }
                }
                Err(error) => {
                    cfg_add_cause(|out| write_cstr(out, error.as_c_str()));
                    cdata.failed = true;
                }
            }
        }
        cdata.current = cdata.current.wrapping_add(1);
        if (cdata.current as usize) >= cdata.files.len() {
            break;
        }
        outcome = cmd_source_file_read(&cdata).await;
    }

    let cdata_opt = if !cfg_finished() {
        cdata.depth_active = false;
        drop(cdata);
        None
    } else {
        if cdata.failed {
            if let Some(client) = cdata.client_handle() {
                if client.attached_session().upgrade().is_none() {
                    client.set_return_value(1);
                }
            }
        }
        Some(cdata)
    };
    if loaded.is_empty() && cdata_opt.is_none() {
        return ExecOutcome::ok();
    }
    ExecOutcome::tail(move |ctx| {
        Box::pin(async move {
            for (commands, current, current_file) in loaded {
                let mut sub = ctx.subcontext(current.as_ref());
                sub.add_format(c"current_file", &current_file);
                let _ = crate::cmd::run::run_list(&commands, &mut sub).await;
            }
            if let Some(mut cdata) = cdata_opt {
                let lane = LaneTarget::for_client(ctx.client.upgrade().as_ref());
                if lane.try_with(|_| ()).is_none() {
                    drop(cdata);
                    return std::future::pending().await;
                }
                cdata.decrement_depth();
                let queue_client = ctx.client.upgrade();
                cfg_print_causes(queue_client.as_ref());
                lane.step();
            }
        })
    })
}

fn cmd_source_file_add(cdata: &mut cmd_source_file_data, path: &CStr) {
    log_debug(format_args!(
        "{}: {}",
        "cmd_source_file_add",
        log_cstr(path)
    ));
    cdata.files.push(path.to_owned());
}

fn cmd_source_file_quote_for_glob(path: &CStr) -> CString {
    let mut quoted = Vec::new();
    for &byte in path.to_bytes() {
        if byte < 128 && !byte.is_ascii_alphanumeric() && byte != b'/' {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    CString::new(quoted).expect("C string path has no interior NUL")
}

fn cmd_source_file_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let args = cmd_get_args(self_0);
        let c = inv.client();
        let mut failed = false;
        match &c {
            None => {
                if SOURCE_FILE_DEPTH.get() >= CMD_SOURCE_FILE_DEPTH_LIMIT as u_int {
                    inv.error(|out| out.write_all(b"too many nested files"));
                    return ExecOutcome::err();
                }
                let depth = SOURCE_FILE_DEPTH.get().wrapping_add(1);
                SOURCE_FILE_DEPTH.set(depth);
                log_debug(format_args!(
                    "{}: depth now {}",
                    "cmd_source_file_exec", depth
                ));
            }
            Some(client) => {
                let Some(depth) = client.enter_source_file(CMD_SOURCE_FILE_DEPTH_LIMIT as u32)
                else {
                    inv.error(|out| out.write_all(b"too many nested files"));
                    return ExecOutcome::err();
                };
                log_debug(format_args!(
                    "{}: depth now {}",
                    "cmd_source_file_exec", depth
                ));
            }
        }
        let mut cdata = Box::new(cmd_source_file_data {
            client: c.clone(),
            depth_active: true,
            flags: 0,
            failed: false,
            current: 0,
            files: Vec::new(),
        });
        if args_has(args, 'q' as i32 as u_char) != 0 {
            cdata.flags |= CMD_PARSE_QUIET;
        }
        if args_has(args, 'n' as i32 as u_char) != 0 {
            cdata.flags |= CMD_PARSE_PARSEONLY;
        }
        if c.is_none()
            || !c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
        {
            let parse_flags = cmd_get_parse_flags(self_0);
            if args_has(args, 'v' as i32 as u_char) != 0 || parse_flags & CMD_PARSE_VERBOSE != 0 {
                cdata.flags |= CMD_PARSE_VERBOSE;
            }
        }
        let cwd_owner =
            ClientRef::working_directory(c.as_ref(), None).expect("source working directory");
        let cwd = cmd_source_file_quote_for_glob(&cwd_owner);
        let mut i: u_int = 0;
        while i < args_count(args) {
            let mut path: &CStr = args_string(args, i).expect("argument is present");
            let expanded = if args_has(args, 'F' as i32 as u_char) != 0 {
                Some(inv.format_single(path))
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
                match tiri_core::tiri_sys::glob(pattern.as_c_str()) {
                    Err(result) => {
                        if result != GLOB_NOMATCH || !cdata.flags & CMD_PARSE_QUIET != 0 {
                            let error = if result == GLOB_NOMATCH {
                                ENOENT
                            } else if result == GLOB_NOSPACE {
                                ENOMEM
                            } else {
                                EINVAL
                            };
                            inv.error(|out| {
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
                    }
                    Ok(matches) => {
                        for path in &matches {
                            cmd_source_file_add(&mut cdata, path);
                        }
                    }
                }
                drop(pattern);
            }
            i = i.wrapping_add(1);
        }
        cdata.failed = failed;
        if !cdata.files.is_empty() {
            let first = cmd_source_file_read(&cdata);
            drop(c);
            return cmd_source_file_chain(self_0, inv, first, cdata).await;
        }
        if !cfg_finished() {
            cdata.depth_active = false;
        } else {
            if cdata.failed {
                if let Some(client) = cdata.client_handle() {
                    if client.attached_session().upgrade().is_none() {
                        client.set_return_value(1);
                    }
                }
            }
            cdata.decrement_depth();
            cfg_print_causes(c.as_ref());
        }
        drop(cdata);
        if failed {
            ExecOutcome::err()
        } else {
            ExecOutcome::ok()
        }
    })
}
