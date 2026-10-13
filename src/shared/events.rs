//! Authoritative events model declarations.

use super::abi::{time_t, u_int};
use super::command::{cmd_find_state, cmdq_item};
use super::pane::window_pane;
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::WindowRef;
use std::collections::BTreeMap;

/// Box-owned until `events_fire` consumes it. Sinks borrow the payload during
/// dispatch; Drop releases items in key order before the target references.
pub struct event_payload {
    pub items: event_payload_tree,
    pub target: cmd_find_state,
    pub target_pane: Option<std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
    pub target_window: Option<WindowRef>,
    pub target_session: Option<SessionRef>,
}

pub struct event_payload_item {
    pub name: std::ffi::CString,
    pub value: EventPayloadValue,
}

impl event_payload_item {
    pub fn type_0(&self) -> event_payload_type {
        self.value.kind()
    }
}

pub struct events_sink {
    pub id: EventSinkId,
    pub name: std::ffi::CString,
    pub cb: events_cb,
    pub dead: ::core::ffi::c_int,
    pub generation: u_int,
}

/// A cancellation identity; ownership remains with the event-sink registry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventSinkId(pub(crate) u64);

pub type events_cb = std::rc::Rc<dyn Fn(&std::ffi::CStr, &mut event_payload)>;

pub fn events_callback(
    callback: impl Fn(&std::ffi::CStr, &mut event_payload) + 'static,
) -> events_cb {
    std::rc::Rc::new(callback)
}

/// Name keys preserve C string ordering for the owned payload items.
pub type event_payload_tree = BTreeMap<std::ffi::CString, Box<event_payload_item>>;

pub enum EventPayloadValue {
    String(std::ffi::CString),
    Time(time_t),
    Int(::core::ffi::c_int),
    Uint(u_int),
    Client(ClientRef),
    Session(SessionRef),
    Window(WindowRef),
    Pane(std::rc::Rc<std::cell::UnsafeCell<window_pane>>),
    Identity(EventPayloadIdentity),
}
impl EventPayloadValue {
    pub fn kind(&self) -> event_payload_type {
        match self {
            Self::String(_) => 0,
            Self::Time(_) => 1,
            Self::Int(_) => 2,
            Self::Uint(_) => 3,
            Self::Client(_) => 4,
            Self::Session(_) => 5,
            Self::Window(_) => 6,
            Self::Pane(_) => 7,
            Self::Identity(_) => 8,
        }
    }
    pub fn string(&self) -> &std::ffi::CStr {
        let Self::String(value) = self else {
            panic!("incorrect event payload type")
        };
        value
    }
    pub fn time(&self) -> time_t {
        let Self::Time(value) = self else {
            panic!("incorrect event payload type")
        };
        *value
    }
    pub fn number(&self) -> ::core::ffi::c_int {
        let Self::Int(value) = self else {
            panic!("incorrect event payload type")
        };
        *value
    }
    pub fn unsigned_number(&self) -> u_int {
        let Self::Uint(value) = self else {
            panic!("incorrect event payload type")
        };
        *value
    }
    pub fn client(&self) -> &ClientRef {
        let Self::Client(value) = self else {
            panic!("incorrect event payload type")
        };
        value
    }
    pub fn session(&self) -> &SessionRef {
        let Self::Session(value) = self else {
            panic!("incorrect event payload type")
        };
        value
    }
    pub fn window(&self) -> &WindowRef {
        let Self::Window(value) = self else {
            panic!("incorrect event payload type")
        };
        value
    }
    pub fn pane(&self) -> &std::rc::Rc<std::cell::UnsafeCell<window_pane>> {
        let Self::Pane(value) = self else {
            panic!("incorrect event payload type")
        };
        value
    }
    pub fn identity(&self) -> &EventPayloadIdentity {
        let Self::Identity(value) = self else {
            panic!("incorrect event payload type")
        };
        value
    }
}

pub enum EventPayloadIdentity {
    QueueItem(std::rc::Weak<std::cell::UnsafeCell<cmdq_item>>),
    HookMonitor(usize),
}

impl EventPayloadIdentity {
    pub fn address(&self) -> usize {
        match self {
            Self::QueueItem(item) => item.as_ptr().addr(),
            Self::HookMonitor(address) => *address,
        }
    }
}

pub type event_payload_type = ::core::ffi::c_uint;
