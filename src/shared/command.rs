//! Authoritative command objects, queues, parsing records, and scalar domains.

use super::abi::u_int;
use super::arguments::{args, args_parse};
use super::key::key_event;
use super::pane::window_pane;
use super::window::winlink;
use crate::src::shared::client::ClientWeak;
use crate::src::shared::session::{SessionRef, SessionWeak};
use crate::src::shared::window::{WindowRef, WindowWeak};
use std::ffi::{CStr, CString};
pub type cmd_retval = ::core::ffi::c_int;
pub const CMD_RETURN_STOP: cmd_retval = 2;
pub const CMD_RETURN_WAIT: cmd_retval = 1;
pub const CMD_RETURN_NORMAL: cmd_retval = 0;
pub const CMD_RETURN_ERROR: cmd_retval = -1;

pub type cmd_find_type = ::core::ffi::c_uint;
pub const CMD_FIND_SESSION: cmd_find_type = 2;
pub const CMD_FIND_WINDOW: cmd_find_type = 1;
pub const CMD_FIND_PANE: cmd_find_type = 0;

/// The argv wire representation is malformed: the count is out of range or
/// the buffer holds fewer NUL-terminated arguments than declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidArgv;

/// Decode the length-delimited argv wire representation. The final byte is
/// forced to NUL before token scans, matching the old C decoder.
pub fn unpack_argv(
    buffer: &mut [u8],
    argc: ::core::ffi::c_int,
) -> Result<Vec<CString>, InvalidArgv> {
    if argc == 0 {
        return Ok(Vec::new());
    }
    if !(0..=1000).contains(&argc) || buffer.is_empty() {
        return Err(InvalidArgv);
    }
    *buffer.last_mut().expect("nonempty buffer") = 0;
    let mut result = Vec::with_capacity(argc as usize);
    let mut start = 0;
    for _ in 0..argc {
        if start >= buffer.len() {
            return Err(InvalidArgv);
        }
        let tail = &buffer[start..];
        let end = tail.iter().position(|byte| *byte == 0).ok_or(InvalidArgv)?;
        result.push(
            CStr::from_bytes_with_nul(&tail[..=end])
                .map_err(|_| InvalidArgv)?
                .to_owned(),
        );
        start += end + 1;
    }
    Ok(result)
}

#[cfg(test)]
mod owned_argv_tests {
    use super::unpack_argv;

    #[test]
    fn unpack_preserves_empty_and_non_utf8_arguments_and_terminates_view() {
        let mut bytes = [b'a', 0, 0, 0xff, 0, b'x'];
        let argv = unpack_argv(&mut bytes, 3).expect("three arguments fit");
        assert_eq!(bytes[5], 0);
        assert_eq!(argv.len(), 3);
        assert_eq!(argv[0].as_bytes(), b"a");
        assert_eq!(argv[1].as_bytes(), b"");
        assert_eq!(argv[2].as_bytes(), &[0xff]);
        assert!(unpack_argv(&mut [], 0)
            .expect("empty argv is valid")
            .is_empty());
    }

    #[test]
    fn unpack_rejects_more_arguments_than_the_wire_buffer_contains() {
        let mut bytes = [b'x', 0];
        assert!(unpack_argv(&mut bytes, 2).is_err());
    }
}

pub type cmd_parse_status = ::core::ffi::c_uint;
pub const CMD_PARSE_SUCCESS: cmd_parse_status = 1;
pub const CMD_PARSE_ERROR: cmd_parse_status = 0;

pub const CMD_PARSE_QUIET: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CMD_PARSE_PARSEONLY: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CMD_STARTSERVER: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CMD_LIST_PRINT_ESCAPED: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CMD_LIST_PRINT_NO_GROUPS: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CMD_FIND_PREFER_UNATTACHED: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CMDQ_STATE_REPEAT: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const CMD_READONLY: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CMD_AFTERHOOK: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const CMD_FIND_WINDOW_INDEX: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const CMD_CLIENT_TFLAG: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const CMD_TARGET_PANE_USAGE: &std::ffi::CStr = c"[-t target-pane]";
pub const CMD_FIND_CANFAIL: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const CMD_TARGET_CLIENT_USAGE: &std::ffi::CStr = c"[-t target-client]";
pub const CMD_CLIENT_CFLAG: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const CMD_CLIENT_CANFAIL: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const CMD_FIND_QUIET: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CMD_FIND_DEFAULT_MARKED: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const CMD_FIND_EXACT_SESSION: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const CMD_FIND_EXACT_WINDOW: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const CMD_TARGET_SESSION_USAGE: &std::ffi::CStr = c"[-t target-session]";
pub const CMD_PARSE_NOALIAS: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const CMD_PARSE_VERBOSE: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const CMD_PARSE_ONEGROUP: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const CMD_PARSE_MAX_ENVIRON_LEN: ::core::ffi::c_int = 16384 as ::core::ffi::c_int;
pub const CMDQ_STATE_CONTROL: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const CMDQ_STATE_NOHOOKS: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const CMD_BUFFER_USAGE: &std::ffi::CStr = c"[-b buffer-name]";
pub const CMD_SOURCE_FILE_DEPTH_LIMIT: ::core::ffi::c_int = 50 as ::core::ffi::c_int;

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn command_domains_match_translated_c_baseline() {
        assert_eq!(size_of::<cmd_retval>(), 4);
        assert_eq!(align_of::<cmd_retval>(), 4);
        assert_eq!(size_of::<cmd_find_type>(), 4);
        assert_eq!(align_of::<cmd_find_type>(), 4);
        assert_eq!(CMD_RETURN_ERROR, -1);
        assert_eq!(CMD_RETURN_NORMAL, 0);
        assert_eq!(CMD_RETURN_WAIT, 1);
        assert_eq!(CMD_RETURN_STOP, 2);
        assert_eq!(CMD_FIND_PANE, 0);
        assert_eq!(CMD_FIND_WINDOW, 1);
        assert_eq!(CMD_FIND_SESSION, 2);
        assert_eq!(size_of::<cmd_parse_status>(), 4);
        assert_eq!(align_of::<cmd_parse_status>(), 4);
        assert_eq!(CMD_PARSE_ERROR, 0);
        assert_eq!(CMD_PARSE_SUCCESS, 1);
    }
}

// Queue allocation and lifecycle state are defined behind the queue module.
pub use crate::src::cmd::queue::{cmdq_item, cmdq_list};

/// Saved, nonowning target identities. An empty observer means unspecified;
/// an expired observer still identifies a target that has disappeared.
/// Clone/drop these handles normally: this record must never be zeroed or
/// bitwise-copied, including when embedded in a mode, prompt, or queue item.
#[derive(Clone, Default)]
#[repr(C)]
pub struct cmd_find_state {
    pub flags: ::core::ffi::c_int,
    pub s: SessionWeak,
    pub wl: refbox::Weak<winlink>,
    pub w: WindowWeak,
    pub wp: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub idx: ::core::ffi::c_int,
}

impl cmd_find_state {
    pub fn clear_state(flags: ::core::ffi::c_int) -> Self {
        Self {
            flags,
            idx: -1,
            ..Default::default()
        }
    }

    /// Retain the target allocation; callbacks may still invalidate it logically.
    pub fn session_handle(&self) -> Option<SessionRef> {
        self.s.upgrade()
    }

    pub fn set_s(&mut self, value: Option<&SessionRef>) {
        self.s = value.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }

    /// Retain the target allocation; callbacks may still invalidate it logically.
    pub fn window_handle(&self) -> Option<WindowRef> {
        self.w.upgrade()
    }

    pub fn set_w(&mut self, value: Option<&WindowRef>) {
        self.w = value.map_or_else(WindowWeak::new, WindowRef::downgrade);
    }

    /// Retain the target allocation; callbacks may still invalidate it logically.
    pub fn pane_handle(&self) -> Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
        self.wp.upgrade()
    }

    pub fn set_wp(&mut self, value: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>) {
        self.wp = value.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }

    /// Observe the selected link without extending its lifetime.
    pub fn winlink_handle(&self) -> refbox::Weak<winlink> {
        self.wl.clone()
    }

    /// Store the selected link identity; the session index retains ownership.
    pub fn set_wl(&mut self, wl: refbox::Weak<winlink>) {
        self.wl = wl;
    }
}

#[repr(C)]
/// Owned by ordinary Rc handles from construction through final drop.
#[derive(Default)]
pub struct cmd_list {
    pub group: u_int,
    /// Sole command owners, released with the final command-list reference.
    pub list: Vec<refbox::RefBox<cmd>>,
}

#[repr(C)]
pub struct cmd {
    pub entry: &'static cmd_entry,
    pub args: Option<Box<args>>,
    pub group: u_int,
    pub file: Option<std::ffi::CString>,
    pub line: u_int,
    pub parse_flags: ::core::ffi::c_int,
}

impl cmd {
    pub fn new(entry: &'static cmd_entry) -> Self {
        Self {
            entry,
            args: Default::default(),
            group: Default::default(),
            file: Default::default(),
            line: Default::default(),
            parse_flags: Default::default(),
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct cmd_entry_flag {
    pub flag: ::core::ffi::c_char,
    pub type_0: cmd_find_type,
    pub flags: ::core::ffi::c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct cmd_entry {
    pub name: &'static std::ffi::CStr,
    pub alias: Option<&'static std::ffi::CStr>,
    pub args: args_parse,
    pub usage: &'static std::ffi::CStr,
    pub source: cmd_entry_flag,
    pub target: cmd_entry_flag,
    pub flags: ::core::ffi::c_int,
    // The dispatching queue item retains the command-list owner through this call.
    pub exec: Option<cmd_exec_fn>,
}

/// Command entry point; the dispatching queue item retains the command-list
/// owner for the duration of the call.
pub type cmd_exec_fn =
    unsafe fn(refbox::Weak<cmd>, &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) -> cmd_retval;

#[repr(C)]
/// Queue items and callers share ownership through ordinary Rc handles.
pub struct cmdq_state {
    pub flags: ::core::ffi::c_int,
    pub formats: std::cell::RefCell<Option<Box<super::format::format_tree>>>,
    pub event: key_event,
    pub current: std::cell::RefCell<cmd_find_state>,
}

impl cmdq_state {
    /// Copy the weak target handles and release the borrow before callbacks run.
    pub fn current_snapshot(&self) -> cmd_find_state {
        self.current.borrow().clone()
    }
}

pub type cmdq_cb =
    Option<Box<dyn FnOnce(&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>) -> cmd_retval>>;

#[derive(Clone, Default)]
#[repr(C)]
pub struct cmd_parse_input {
    pub flags: ::core::ffi::c_int,
    pub file: Option<::std::ffi::CString>,
    pub line: u_int,
    pub item: std::rc::Weak<std::cell::UnsafeCell<cmdq_item>>,
    /// Parser context observes the client; prepared commands own it separately.
    pub c: ClientWeak,
    pub fs: cmd_find_state,
}

impl cmd_parse_input {
    pub fn set_item(&mut self, item: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>) {
        self.item = item.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }
}

pub struct cmd_parse_result {
    pub status: cmd_parse_status,
    pub cmdlist: Option<std::rc::Rc<std::cell::RefCell<cmd_list>>>,
    pub error: Option<CString>,
}

impl cmd_parse_result {
    pub fn take_cmdlist(&mut self) -> Option<std::rc::Rc<std::cell::RefCell<cmd_list>>> {
        self.cmdlist.take()
    }

    pub fn empty() -> Self {
        Self {
            status: CMD_PARSE_ERROR,
            cmdlist: None,
            error: None,
        }
    }
}

impl Drop for cmdq_state {
    fn drop(&mut self) {
        if let Some(formats) = self.formats.get_mut().take() {
            drop(formats);
        }
    }
}
