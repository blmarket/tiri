//! Authoritative command objects, queues, parsing records, and scalar domains.
pub use tiri_core::actions::{ExecutionState as cmdq_state, TaskOutcome as CmdOutcome};

use super::abi::u_int;
use crate::arguments::{args, args_parse};
use crate::shared::client::ClientWeak;
pub type cmd_find_type = ::core::ffi::c_uint;
pub const CMD_FIND_SESSION: cmd_find_type = 2;
pub const CMD_FIND_WINDOW: cmd_find_type = 1;
pub const CMD_FIND_PANE: cmd_find_type = 0;

pub use tiri_core::compat::imsg::{unpack_argv, InvalidArgv};
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
    use super::{CMD_FIND_PANE, CMD_FIND_SESSION, CMD_FIND_WINDOW, cmd_find_type};
    use ::core::mem::{align_of, size_of};

    #[test]
    fn command_domains_match_translated_c_baseline() {
        assert_eq!(size_of::<cmd_find_type>(), 4);
        assert_eq!(align_of::<cmd_find_type>(), 4);
        assert_eq!(CMD_FIND_PANE, 0);
        assert_eq!(CMD_FIND_WINDOW, 1);
        assert_eq!(CMD_FIND_SESSION, 2);
    }
}

pub use crate::cmd::queue::{cmdq_item, cmdq_list};

pub use tiri_core::shared::target::Target as cmd_find_state;
#[repr(C)]
/// Built with single ownership, then shared through Rc handles.
#[derive(Default)]
pub struct cmd_list {
    /// Commands owned by this list.
    pub groups: Vec<Vec<cmd>>,
}

#[repr(C)]
pub struct cmd {
    pub entry: &'static cmd_entry,
    pub args: Box<args>,
    pub file: Option<std::ffi::CString>,
    pub line: u_int,
    pub parse_flags: ::core::ffi::c_int,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct cmd_entry_flag {
    pub flag: u8,
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
    pub exec: Option<cmd_exec_fn>,
}

/// Command entry point; the dispatching queue item retains the command-list
/// owner for the duration of the call.
pub type cmd_exec_fn = fn(&cmd, &std::rc::Rc<cmdq_item>) -> CmdOutcome;

/// Queue items and callers share ownership through ordinary Rc handles.

pub type cmdq_cb = Option<Box<dyn FnOnce(&std::rc::Rc<cmdq_item>) -> CmdOutcome>>;

#[derive(Clone, Default)]
#[repr(C)]
pub struct cmd_parse_input {
    pub flags: ::core::ffi::c_int,
    pub file: Option<::std::ffi::CString>,
    pub line: u_int,
    pub item: std::rc::Weak<cmdq_item>,
    /// Parser context observes the client; prepared commands own it separately.
    pub c: ClientWeak,
    pub fs: cmd_find_state,
}

impl cmd_parse_input {
    pub fn set_item(&mut self, item: Option<&std::rc::Rc<cmdq_item>>) {
        self.item = item.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }
}
