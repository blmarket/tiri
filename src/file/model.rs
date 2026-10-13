use super::*;
use crate::src::shared::client::{ClientRef, ClientWeak};
use tiri_buffer::SegmentedBuf;

/// The stream index owns active file records until completion unlinks them.
/// Its holder is either the server's ClientRef or the process client singleton.
pub type client_files =
    std::collections::BTreeMap<i32, std::rc::Rc<std::cell::UnsafeCell<client_file>>>;

#[derive(Clone, Copy, Default)]
pub(super) enum FileRegistration {
    #[default]
    Unlinked,
    Client,
    Peer,
}

pub struct client_file {
    pub(super) c: Option<ClientRef>,
    pub(super) peer: *mut tmuxpeer,
    pub(super) stream: ::core::ffi::c_int,
    pub(super) path: Option<std::ffi::CString>,
    pub(super) buffer: SegmentedBuf,
    pub(super) event: crate::src::reactor::StreamHandle,
    pub(super) fd: Option<OwnedFd>,
    pub(super) error: ::core::ffi::c_int,
    pub(super) closed: ::core::ffi::c_int,
    pub(super) cb: client_file_cb,
    /// Which existing holder owns the stream; c retains Client registrations.
    pub(super) registration: FileRegistration,
    pub(super) wait_item:
        std::rc::Weak<std::cell::UnsafeCell<crate::src::shared::command::cmdq_item>>,
    pub(super) wait_active: bool,
    pub(super) wait_client: ClientWeak,
    pub(super) cancel_data: Option<Box<dyn FnOnce()>>,
    pub(super) completed: bool,
    pub(super) done_task: Option<tiri_rt::mio::Task>,
    pub(super) push_task: Option<tiri_rt::mio::Task>,
    pub(super) read: super::stream::ReadState,
}

pub struct client_file_event<'a> {
    /// Borrowed retained client for this callback; clone to retain it afterward.
    pub client: Option<&'a ClientRef>,
    pub path: Option<&'a CStr>,
    pub error: i32,
    pub closed: bool,
    pub buffer: Option<&'a mut SegmentedBuf>,
}

impl client_file {
    pub(super) fn new() -> std::rc::Rc<std::cell::UnsafeCell<Self>> {
        std::rc::Rc::new(std::cell::UnsafeCell::new(Self::empty()))
    }

    pub(super) fn empty() -> Self {
        Self {
            c: Default::default(),
            peer: Default::default(),
            stream: Default::default(),
            path: Default::default(),
            buffer: Default::default(),
            event: Default::default(),
            fd: Default::default(),
            error: Default::default(),
            closed: Default::default(),
            cb: Default::default(),
            registration: FileRegistration::Unlinked,
            wait_item: Default::default(),
            wait_active: false,
            wait_client: Default::default(),
            cancel_data: Default::default(),
            completed: Default::default(),
            done_task: Default::default(),
            push_task: Default::default(),
            read: Default::default(),
        }
    }
}

pub type client_file_cb = Option<Box<dyn for<'a> FnMut(client_file_event<'a>)>>;
