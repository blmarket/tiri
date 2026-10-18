//! Authoritative client/server message identifiers.

use crate::shared::abi::{pid_t, size_t, uint32_t};
use std::collections::VecDeque;
use std::os::fd::OwnedFd;
pub type msgtype = ::core::ffi::c_uint;

pub const MSG_WRITE_DONE: msgtype = 308;
pub const MSG_READ_CANCEL: msgtype = 307;
pub const MSG_WRITE_CLOSE: msgtype = 306;
pub const MSG_WRITE_READY: msgtype = 305;
pub const MSG_WRITE: msgtype = 304;
pub const MSG_WRITE_OPEN: msgtype = 303;
pub const MSG_READ_DONE: msgtype = 302;
pub const MSG_READ: msgtype = 301;
pub const MSG_READ_OPEN: msgtype = 300;
pub const MSG_FLAGS: msgtype = 218;
pub const MSG_EXEC: msgtype = 217;
pub const MSG_WAKEUP: msgtype = 216;
pub const MSG_UNLOCK: msgtype = 215;
pub const MSG_SUSPEND: msgtype = 214;
// Legacy stream messages, recognized to reject incompatible servers.
pub const MSG_STDOUT: msgtype = 213;
pub const MSG_STDIN: msgtype = 212;
pub const MSG_STDERR: msgtype = 211;
pub const MSG_SHUTDOWN: msgtype = 210;
pub const MSG_SHELL: msgtype = 209;
pub const MSG_RESIZE: msgtype = 208;
pub const MSG_READY: msgtype = 207;
pub const MSG_LOCK: msgtype = 206;
pub const MSG_EXITING: msgtype = 205;
pub const MSG_EXITED: msgtype = 204;
pub const MSG_EXIT: msgtype = 203;
pub const MSG_DETACHKILL: msgtype = 202;
pub const MSG_DETACH: msgtype = 201;
pub const MSG_COMMAND: msgtype = 200;
pub const MSG_IDENTIFY_TERMINFO: msgtype = 112;
pub const MSG_IDENTIFY_LONGFLAGS: msgtype = 111;
pub const MSG_IDENTIFY_STDOUT: msgtype = 110;
pub const MSG_IDENTIFY_FEATURES: msgtype = 109;
pub const MSG_IDENTIFY_CWD: msgtype = 108;
pub const MSG_IDENTIFY_CLIENTPID: msgtype = 107;
pub const MSG_IDENTIFY_DONE: msgtype = 106;
pub const MSG_IDENTIFY_ENVIRON: msgtype = 105;
pub const MSG_IDENTIFY_STDIN: msgtype = 104;
pub const MSG_IDENTIFY_TTYNAME: msgtype = 102;
pub const MSG_IDENTIFY_TERM: msgtype = 101;
pub const MSG_IDENTIFY_FLAGS: msgtype = 100;
pub const MSG_VERSION: msgtype = 12;

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn message_domain_matches_translated_c_baseline() {
        assert_eq!(size_of::<msgtype>(), 4);
        assert_eq!(align_of::<msgtype>(), 4);
        assert_eq!(MSG_VERSION, 12);
        assert_eq!(MSG_IDENTIFY_FLAGS, 100);
        assert_eq!(MSG_COMMAND, 200);
        assert_eq!(MSG_WRITE_DONE, 308);
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct imsg_hdr {
    pub type_0: msgtype,
    pub len: uint32_t,
    pub peerid: uint32_t,
    pub pid: uint32_t,
}

pub(crate) const IMSG_HEADER_SIZE: usize = ::core::mem::size_of::<imsg_hdr>();

pub(crate) const MAX_IMSGSIZE: ::core::ffi::c_int = 16384 as ::core::ffi::c_int;

pub(crate) const PROTOCOL_VERSION: ::core::ffi::c_int = 8 as ::core::ffi::c_int;

pub struct imsg {
    pub hdr: imsg_hdr,
    pub data: Vec<u8>,
    pub(super) fd: Option<OwnedFd>,
}

pub(super) struct Outgoing {
    pub(super) bytes: Vec<u8>,
    pub(super) offset: usize,
    pub(super) fd: Option<OwnedFd>,
}

/// Protocol storage only. Transport operations belong to the peer's runtime task.
#[derive(Default)]
pub(crate) struct imsgbuf {
    pub(super) input: Vec<u8>,
    pub(super) input_fds: VecDeque<OwnedFd>,
    pub(super) incoming: VecDeque<imsg>,
    pub(super) outgoing: VecDeque<Outgoing>,
    pub(super) pid: pid_t,
    /// Owns the socket until the peer explicitly calls imsgbuf_clear.
    pub(crate) fd: Option<OwnedFd>,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub(crate) struct msg_command {
    pub argc: ::core::ffi::c_int,
}
