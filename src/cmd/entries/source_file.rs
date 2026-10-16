use crate::src::arguments::{args_count, args_has, args_string};
use crate::src::cfg::{cfg_finished, cfg_print_causes, load_cfg_from_buffer};
use crate::src::cmd::queue::{
    cmdq_error, cmdq_get_callback_owned, cmdq_get_client, cmdq_insert_after, cmdq_resumed,
    cmdq_suspend,
};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_parse_flags};
use crate::src::compat::glob::GlobResult;
use crate::src::file::{file_read_wait, FileOutcome, FileWait};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::log::{log_cstr, log_debug};
use crate::src::reactor::evbuffer_pullup;
use crate::src::server_client::Client as _;
use libc::{strcmp, strerror};

use crate::src::shared::abi::__size_t;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_CONTROL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::src::shared::command::{
    CMD_FIND_CANFAIL, CMD_PARSE_PARSEONLY, CMD_PARSE_QUIET, CMD_PARSE_VERBOSE,
    CMD_SOURCE_FILE_DEPTH_LIMIT,
};
use crate::src::shared::errno::{EINVAL, ENOENT, ENOMEM};
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

pub struct cmd_source_file_data {
    pub client: Option<ClientRef>,
    depth_active: bool,
    pub flags: ::core::ffi::c_int,
    pub after: Weak<UnsafeCell<cmdq_item>>,
    pub retval: cmd_retval,
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
        // Preserve cleanup order: depth, path copies, then client release.
        drop(std::mem::take(&mut self.files));
        if let Some(client) = self.client.take() {
            (client).release();
        }
    }
}

unsafe fn cmd_source_file_complete_cb(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut cdata: cmd_source_file_data,
) -> cmd_retval {
    cdata.decrement_depth();
    cfg_print_causes(item_handle);
    CMD_RETURN_NORMAL
}

unsafe fn cmd_source_file_complete(
    mut cdata: Box<cmd_source_file_data>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) {
    let c = cdata.client_handle();
    if cfg_finished == 0 {
        // Startup completion does not decrement nesting depth in tmux.
        // Cancellation during an active read still decrements it.
        cdata.depth_active = false;
        return;
    }
    if cdata.retval == CMD_RETURN_ERROR {
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
    item: Weak<UnsafeCell<cmdq_item>>,
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
    item_owner: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut read: FileOutcome,
) {
    let item = item_owner.get();
    let path = read.path.as_deref().map_or(::core::ptr::null(), CStr::as_ptr);
    let error = read.error;
    // Progress does not coalesce the buffer; only complete files are parsed.
    let bdata: &[u8] = evbuffer_pullup(&mut read.buffer, -1).map_or(&[], |bytes| bytes);
    let bsize = bdata.len();
    let mut new_item = Weak::new();
    let target = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    if error != 0 {
        cmdq_error(item_owner, |out| {
            write_cstr(out, nullable_cstr(strerror(error)))?;
            out.write_all(b": ")?;
            write_cstr(out, nullable_cstr(path))
        });
    } else if bsize != 0 {
        let after_owner = cdata
            .after
            .upgrade()
            .unwrap_or_else(|| Rc::clone(item_owner));
        if load_cfg_from_buffer(
            bdata,
            CStr::from_ptr(path),
            cdata.client.as_ref(),
            Some(&after_owner),
            target,
            cdata.flags,
            Some(&mut new_item),
        ) < 0
        {
            cdata.retval = CMD_RETURN_ERROR;
        } else if new_item.strong_count() != 0 {
            cdata.after = new_item;
        }
    }
}
unsafe fn cmd_source_file_add(cdata: &mut cmd_source_file_data, path: &CStr) {
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
        if byte < 128 && ::libc::isalnum(byte as ::core::ffi::c_int) == 0 && byte != b'/' {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    CString::new(quoted).expect("C string path has no interior NUL")
}
unsafe fn cmd_source_file_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut retval: cmd_retval = CMD_RETURN_NORMAL;
    let mut path: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut error: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut parse_flags: ::core::ffi::c_int = 0;
    let mut i: u_int = 0;
    let mut j: u_int = 0;
    match &c {
        None => {
            if cmd_source_file_depth >= CMD_SOURCE_FILE_DEPTH_LIMIT as u_int {
                cmdq_error(item_handle, |out| out.write_all(b"too many nested files"));
                return CMD_RETURN_ERROR;
            }
            cmd_source_file_depth = cmd_source_file_depth.wrapping_add(1);
            log_debug(format_args!("{}: depth now {}", "cmd_source_file_exec", {
                cmd_source_file_depth
            }));
        }
        Some(client) => {
            let Some(depth) = client.enter_source_file(CMD_SOURCE_FILE_DEPTH_LIMIT as u32) else {
                cmdq_error(item_handle, |out| out.write_all(b"too many nested files"));
                return CMD_RETURN_ERROR;
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
        retval: CMD_RETURN_NORMAL,
        current: 0,
        files: Vec::new(),
    });
    if args_has(args, 'q' as i32 as u_char) != 0 {
        cdata.flags |= CMD_PARSE_QUIET;
    }
    if args_has(args, 'n' as i32 as u_char) != 0 {
        cdata.flags |= CMD_PARSE_PARSEONLY;
    }
    if c.is_none() || !c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
        parse_flags = cmd_get_parse_flags(self_0.clone());
        if args_has(args, 'v' as i32 as u_char) != 0 || parse_flags & CMD_PARSE_VERBOSE != 0 {
            cdata.flags |= CMD_PARSE_VERBOSE;
        }
    }
    let cwd_owner =
        ClientRef::working_directory(c.as_ref(), None).expect("source working directory");
    let cwd = cmd_source_file_quote_for_glob(&cwd_owner);
    i = 0 as u_int;
    while i < args_count(args) {
        path = args_string(&mut *(args), i).map_or(std::ptr::null(), |value| value.as_ptr());
        let expanded = if args_has(args, 'F' as i32 as u_char) != 0 {
            Some(format_single_from_target_cstring(item_handle, path))
        } else {
            None
        };
        if let Some(expanded) = expanded.as_ref() {
            path = expanded.as_ptr();
        }
        if strcmp(path, c"-".as_ptr()) == 0 as ::core::ffi::c_int {
            cmd_source_file_add(&mut cdata, c"-");
        } else {
            let pattern = if *path as ::core::ffi::c_int == '/' as i32 {
                CStr::from_ptr(path).to_owned()
            } else {
                let cwd_bytes = cwd.to_bytes();
                let path_bytes = CStr::from_ptr(path).to_bytes();
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
                    if result == GLOB_NOMATCH {
                        error = strerror(ENOENT);
                    } else if result == GLOB_NOSPACE {
                        error = strerror(ENOMEM);
                    } else {
                        error = strerror(EINVAL);
                    }
                    cmdq_error(item_handle, |out| {
                        write_cstr(out, nullable_cstr(error))?;
                        out.write_all(b": ")?;
                        write_cstr(out, nullable_cstr(path))
                    });
                    retval = CMD_RETURN_ERROR;
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
    cdata.retval = retval;
    if !cdata.files.is_empty() {
        let first = cmd_source_file_read(&cdata);
        retval = cmdq_suspend(
            item_handle,
            cmd_source_file_chain(first, cdata, std::rc::Rc::downgrade(item_handle)),
        );
    } else {
        cmd_source_file_complete(cdata, item_handle);
    }
    drop(cwd);
    retval
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::cmd::queue::{cmdq_append, cmdq_labels, cmdq_next};
    use crate::src::server_client::queue_test_clients as clients;
    use std::time::Duration;
    use tiri_rt::Runtime as _;

    /// A file in the temporary directory, removed on drop.
    struct TempFile(CString);
    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("tiri-source-{}-{name}", std::process::id()));
            std::fs::write(&path, contents).unwrap();
            Self(CString::new(path.into_os_string().into_encoded_bytes()).unwrap())
        }
    }
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.to_str().unwrap());
        }
    }

    /// Empty server options for parsing, restored on drop.
    struct Options(*mut crate::src::shared::options::options, Box<crate::src::shared::options::options>);
    impl Options {
        unsafe fn new() -> Self {
            let previous = crate::src::tmux::global_options;
            let mut owner = crate::src::options::options_create(None);
            crate::src::tmux::global_options = &raw mut *owner;
            Self(previous, owner)
        }
    }
    impl Drop for Options {
        fn drop(&mut self) {
            unsafe { crate::src::tmux::global_options = self.0 };
        }
    }

    /// Source data for a clientless chain that has entered one nesting level.
    unsafe fn data(files: &[&TempFile]) -> Box<cmd_source_file_data> {
        cfg_finished = 1;
        cmd_source_file_depth = 1;
        Box::new(cmd_source_file_data {
            client: None,
            depth_active: true,
            flags: 0,
            after: Weak::new(),
            retval: CMD_RETURN_NORMAL,
            current: 0,
            files: files.iter().map(|file| file.0.clone()).collect(),
        })
    }

    /// Queue a head on `owner` that suspends on the chain, reading the first
    /// file with `first`.
    unsafe fn append_chain<F: std::future::Future<Output = Option<FileOutcome>> + 'static>(
        owner: &ClientRef,
        cdata: Box<cmd_source_file_data>,
        first: impl FnOnce(&cmd_source_file_data) -> F + 'static,
    ) -> Weak<UnsafeCell<cmdq_item>> {
        let head = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"source",
                Some(Box::new(move |item| {
                    let mut cdata = cdata;
                    cdata.after = Rc::downgrade(item);
                    let first = first(&cdata);
                    cmdq_suspend(item, cmd_source_file_chain(first, cdata, Rc::downgrade(item)))
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(owner)), 0);
        head
    }

    #[test]
    fn a_chain_inserts_each_file_after_the_previous_one() {
        let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let _options = Options::new();
            let (one, two) = (
                TempFile::new("one", "set -g @a 1\n"),
                TempFile::new("two", "setw -g @b 2\n"),
            );
            let owner = clients::with_queue();
            let head = append_chain(&owner, data(&[&one, &two]), |cdata| {
                cmd_source_file_read(cdata)
            });
            assert_eq!(cmdq_labels(Some(&owner)), ["source"], "local reads are deferred");
            // Each read reports in a task, which starts the next read.
            runtime.poll(Some(Duration::ZERO)).unwrap();
            assert_eq!(
                cmdq_labels(Some(&owner)),
                [
                    "source",
                    "set-option",
                    "set-window-option",
                    "cmd_source_file_complete_cb"
                ]
            );
            assert_eq!(cmd_source_file_depth, 1, "the completion leaves the depth");
            drop(owner);
            assert!(head.upgrade().is_none());
            assert_eq!(cmd_source_file_depth, 0);
        }
        crate::src::reactor::shutdown_runtime(runtime);
    }

    #[test]
    fn dropping_the_chain_between_reads_leaves_the_depth() {
        let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let _options = Options::new();
            let (one, two) = (
                TempFile::new("one", "set -g @a 1\n"),
                TempFile::new("two", "setw -g @b 2\n"),
            );
            let owner = clients::with_queue();
            let (tx, rx) = tiri_rt::oneshot::channel();
            let head = append_chain(&owner, data(&[&one, &two]), move |_| rx);
            let outcome = FileOutcome {
                path: Some(one.0.clone()),
                error: 0,
                buffer: tiri_buffer::SegmentedBuf::from(b"set -g @a 1\n".to_vec()),
            };
            // The first file loads inside the send, which starts the second read.
            assert!(tx.send(outcome).is_ok());
            assert_eq!(cmdq_labels(Some(&owner)), ["source", "set-option"]);
            drop(owner);
            assert!(head.upgrade().is_none());
            assert_eq!(cmd_source_file_depth, 0);
            // The second read was cancelled with the chain.
            runtime.poll(Some(Duration::ZERO)).unwrap();
        }
        crate::src::reactor::shutdown_runtime(runtime);
    }

    #[test]
    fn a_dead_file_client_leaves_the_depth_and_parks() {
        unsafe {
            let one = TempFile::new("one", "set -g @a 1\n");
            let owner = clients::with_queue();
            let (tx, rx) = tiri_rt::oneshot::channel();
            let head = append_chain(&owner, data(&[&one]), move |_| rx);
            // The dead file client drops the waiter unsent.
            drop(tx);
            assert_eq!(cmd_source_file_depth, 0, "the data is released");
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert_eq!(cmdq_labels(Some(&owner)), ["source"], "the command stays blocked");
            drop(owner);
            assert!(head.upgrade().is_none());
        }
    }
}
