//! Authoritative process model declarations.

use super::abi::{gid_t, uid_t};
use crate::compat::imsg::{imsg, imsgbuf};

pub enum PeerMessage<'a> {
    Disconnected,
    Message(&'a mut imsg),
}

/// Receives each message (or disconnect) delivered to a peer.
pub type peer_dispatch_cb = Box<dyn for<'a> FnMut(PeerMessage<'a>)>;

/// Owned by the parent process's peer list until `proc_remove_peer`. Clients
/// and transfers hold weak handles; each access borrows for a single step.
pub struct tmuxpeer {
    pub parent: *mut tmuxproc,
    /// Declared before `ibuf` so the task is cancelled before the socket closes.
    pub io_task: Option<tiri_rt::mio::Task>,
    pub(crate) ibuf: imsgbuf,
    pub io_writable: bool,
    pub uid: uid_t,
    pub gid: gid_t,
    pub flags: ::core::ffi::c_int,
    pub dispatchcb: Option<peer_dispatch_cb>,
}

impl Default for tmuxpeer {
    fn default() -> Self {
        Self {
            parent: std::ptr::null_mut(),
            ibuf: imsgbuf::default(),
            io_task: None,
            io_writable: false,
            uid: 0,
            gid: 0,
            flags: 0,
            dispatchcb: None,
        }
    }
}

#[repr(C)]
pub struct tmuxproc {
    pub name: std::ffi::CString,
    pub exit: ::core::ffi::c_int,
    pub signalcb: Option<Box<dyn FnMut(super::signal::ProcessSignal)>>,
    pub signal_task: Option<tiri_rt::mio::Task>,
    /// Owns each peer until `proc_remove_peer` removes it.
    pub peers: Vec<refbox::RefBox<tmuxpeer>>,
}
