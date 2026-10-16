use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_add;
use crate::src::log::{log_cstr, log_cstr_n, log_debug};
use crate::src::reactor::{
    evbuffer_add_formatted, evbuffer_get_length, evbuffer_new, evbuffer_pullup,
};
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::events::{
    event_payload, event_payload_item, event_payload_tree, event_payload_type,
    EventPayloadIdentity, EventPayloadValue,
};
use crate::src::shared::format::format_tree;
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::WindowRef;
use crate::src::shared::window::WindowWeak;
use crate::src::window::{winlink_find_by_index, Window};
use crate::src::{server_client::Client, session::Session, window::WindowPane};
use std::ffi::{CStr, CString};
use tiri_buffer::SegmentedBuf;

pub const EVENT_PAYLOAD_WINDOW: event_payload_type = 6;
pub const EVENT_PAYLOAD_SESSION: event_payload_type = 5;

fn event_payload_find<'a>(ep: &'a event_payload, name: &CStr) -> Option<&'a event_payload_item> {
    ep.items.get(name).map(Box::as_ref)
}

pub fn event_payload_items(ep: &event_payload) -> impl Iterator<Item = &event_payload_item> {
    ep.items.values().map(Box::as_ref)
}

unsafe fn event_payload_free_target(ep: &mut event_payload) {
    let target = &mut ep.target;
    if let Some(session) = ep.target_session.take() {
        session.release(c"event_payload_free_target");
    }
    if let Some(window) = ep.target_window.take() {
        drop(window);
    }
    if let Some(pane) = ep.target_pane.take() {
        pane.release(c"event_payload_free_target");
    }
    *target = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
}
impl Drop for event_payload_item {
    fn drop(&mut self) {
        // Release retained models while the item's name is still alive.
        let value = std::mem::replace(
            &mut self.value,
            EventPayloadValue::String(Default::default()),
        );
        unsafe {
            match value {
                EventPayloadValue::Client(client) => client.release(),
                EventPayloadValue::Session(session) => session.release(c"event_payload_free_value"),
                EventPayloadValue::Window(window) => drop(window),
                EventPayloadValue::Pane(pane) => pane.release(c"event_payload_free_value"),
                _ => {}
            }
        }
    }
}

unsafe fn event_payload_set_item(ep: &mut event_payload, name: &CStr, value: EventPayloadValue) {
    // A legacy caller may borrow the old item's name. Copy it before removing
    // that owner, and finish removal before releasing references can dispatch.
    let name = name.to_owned();
    let key = name.clone();
    let new = Box::new(event_payload_item { name, value });
    let old = ep.items.remove(&key);
    drop(old);
    ep.items.insert(key, new);
}

pub fn event_payload_create() -> Box<event_payload> {
    Box::new(event_payload {
        target_pane: None,
        target_window: None,
        target_session: None,
        items: event_payload_tree::default(),
        target: cmd_find_state {
            idx: -1,
            ..Default::default()
        },
    })
}

impl Drop for event_payload {
    fn drop(&mut self) {
        // Match tmux: remove each item in key order before releasing its models,
        // then release the target after all item-triggered dispatch has finished.
        while let Some((_, item)) = self.items.pop_first() {
            drop(item);
        }
        unsafe { event_payload_free_target(self) };
    }
}

pub unsafe fn event_payload_set_target(ep: &mut event_payload, fs: &cmd_find_state) {
    event_payload_free_target(ep);
    let target = &mut ep.target;
    ep.target_session = fs.s.upgrade();
    if ep.target_session.is_some() {
        target.s = fs.s.clone();
    }
    ep.target_window = fs.w.upgrade();
    if ep.target_window.is_some() {
        target.w = fs.w.clone();
    }
    let link = match fs.wl.try_borrow_mut() {
        Ok(link) => Some(link),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("event target winlink already borrowed"),
    };
    if let Some(link) = link {
        target.idx = link.idx;
        if ep.target_session.is_none() {
            ep.target_session = link.session.upgrade();
            target.s = ep
                .target_session
                .as_ref()
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
        }
        if ep.target_window.is_none() {
            ep.target_window = link.window_owner.clone();
            target.w = ep
                .target_window
                .as_ref()
                .map_or_else(WindowWeak::new, WindowRef::downgrade);
        }
    } else {
        target.idx = -1;
    }
    ep.target_pane = fs.wp.upgrade();
    if ep.target_pane.is_some() {
        target.wp = fs.wp.clone();
    }
}
pub unsafe fn event_payload_get_target(ep: &event_payload, fs: &mut cmd_find_state) -> i32 {
    let target = &ep.target;
    let session = target.s.upgrade();
    let window = target.w.upgrade();
    let pane = target.wp.upgrade();
    let flags = fs.flags;
    let mut link = refbox::Weak::new();
    if target.idx != -1 {
        if let (Some(session), Some(window)) = (&session, &window) {
            if session.is_registered() {
                link = session.with_winlinks(|links| winlink_find_by_index(links, target.idx));
                if link.is_alive()
                    && !link
                        .get_unchecked()
                        .window_handle()
                        .is_some_and(|owner| WindowRef::ptr_eq(owner, window))
                {
                    link = refbox::Weak::new();
                }
            }
        }
    }
    *fs = cmd_find_state::clear_state(flags);
    fs.s = target.s.clone();
    fs.w = target.w.clone();
    fs.wp = target.wp.clone();
    fs.set_wl(link.clone());
    fs.idx = if link.is_alive() {
        link.get_unchecked().idx
    } else {
        -1
    };
    if fs.is_valid() {
        return 1;
    }
    if let Some(pane) = pane.as_ref() {
        if link.is_alive()
            && link
                .get_unchecked()
                .window_handle()
                .expect("linked window")
                .contains_pane(&target.wp)
        {
            *fs = cmd_find_state::find_from_winlink_pane(link.clone(), pane, flags);
            if fs.is_valid() {
                return 1;
            }
        }
        if let Some(found) = cmd_find_state::find_from_pane(pane, flags) {
            *fs = found;
            if fs.is_valid() {
                return 1;
            }
        }
    }
    if link.is_alive() {
        *fs = cmd_find_state::find_from_winlink(link, flags);
        if fs.is_valid() {
            return 1;
        }
    }
    if let (Some(session), Some(window)) = (&session, &window) {
        if session.is_registered() {
            if let Some(found) = cmd_find_state::find_from_session_window(session, window, flags) {
                *fs = found;
                if fs.is_valid() {
                    return 1;
                }
            }
        }
    }
    if let Some(session) = session.as_ref() {
        if session.is_registered() {
            *fs = cmd_find_state::find_from_session(session, flags);
            if fs.is_valid() {
                return 1;
            }
        }
    }
    if let Some(found) = cmd_find_state::find_from_nothing(flags) {
        *fs = found;
        return 1;
    }
    *fs = cmd_find_state::clear_state(flags);
    0
}
pub unsafe fn event_payload_set_string(
    ep: &mut event_payload,
    name: &CStr,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let string = format_message_with(write);
    event_payload_set_item(&mut *ep, name, EventPayloadValue::String(string));
}
pub unsafe fn event_payload_set_time(ep: &mut event_payload, name: &CStr, mut value: time_t) {
    event_payload_set_item(&mut *ep, name, EventPayloadValue::Time(value));
}
pub unsafe fn event_payload_set_int(
    ep: &mut event_payload,
    name: &CStr,
    mut value: ::core::ffi::c_int,
) {
    event_payload_set_item(&mut *ep, name, EventPayloadValue::Int(value));
}
pub unsafe fn event_payload_set_uint(ep: &mut event_payload, name: &CStr, mut value: u_int) {
    event_payload_set_item(&mut *ep, name, EventPayloadValue::Uint(value));
}
/// Transfer a retained client into the payload; payload cleanup releases it explicitly.
pub unsafe fn event_payload_set_client(ep: &mut event_payload, owner: ClientRef) {
    event_payload_set_item(ep, c"client", EventPayloadValue::Client(owner));
}
/// Transfer a retained session into the payload; payload cleanup releases it explicitly.
pub unsafe fn event_payload_set_session(ep: &mut event_payload, name: &CStr, owner: SessionRef) {
    event_payload_set_item(ep, name, EventPayloadValue::Session(owner));
}
/// Transfer a retained window into the payload; payload cleanup releases it explicitly.
pub unsafe fn event_payload_set_window(ep: &mut event_payload, name: &CStr, owner: WindowRef) {
    event_payload_set_item(ep, name, EventPayloadValue::Window(owner));
}
/// Transfer a retained window_pane into the payload; payload cleanup releases it explicitly.
pub unsafe fn event_payload_set_pane(
    ep: &mut event_payload,
    name: &CStr,
    owner: std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    event_payload_set_item(ep, name, EventPayloadValue::Pane(owner));
}
pub unsafe fn event_payload_set_identity(
    ep: &mut event_payload,
    name: &CStr,
    identity: EventPayloadIdentity,
) {
    event_payload_set_item(&mut *ep, name, EventPayloadValue::Identity(identity));
}
pub fn event_payload_get_string(ep: &event_payload) -> Option<&CStr> {
    match event_payload_find(ep, c"paste_buffer").map(|item| &item.value) {
        Some(EventPayloadValue::String(value)) => Some(value),
        _ => None,
    }
}
unsafe fn event_payload_add_item(epi: &event_payload_item, evb: &mut SegmentedBuf) {
    match epi.type_0() as ::core::ffi::c_uint {
        0 => {
            evbuffer_add_formatted(evb, |out| out.write_all(epi.value.string().to_bytes()));
        }
        1 => {
            evbuffer_add_formatted(evb, |out| {
                write!(out, "{}", { epi.value.time() as ::core::ffi::c_longlong })
            });
        }
        2 => {
            evbuffer_add_formatted(evb, |out| write!(out, "{}", { epi.value.number() }));
        }
        3 => {
            evbuffer_add_formatted(evb, |out| {
                write!(out, "{}", { epi.value.unsigned_number() })
            });
        }
        4 => {
            evbuffer_add_formatted(evb, |out| {
                write_cstr(out, epi.value.client().name().as_deref())
            });
        }
        5 => {
            evbuffer_add_formatted(evb, |out| write!(out, "${}", { epi.value.session().id() }));
        }
        6 => {
            evbuffer_add_formatted(evb, |out| write!(out, "@{}", { epi.value.window().id() }));
        }
        7 => {
            evbuffer_add_formatted(evb, |out| write!(out, "%{}", { epi.value.pane().id() }));
        }
        8 => {
            evbuffer_add_formatted(evb, |out| {
                let address = epi.value.identity().address();
                if address == 0 {
                    out.write_all(b"(nil)")
                } else {
                    write!(out, "{:p}", address as *const ())
                }
            });
        }
        _ => {}
    };
}
/// Printed payload bytes with one trailing NUL for synchronous C consumers.
/// The bytes before that terminator may themselves contain NULs.
pub(crate) unsafe fn event_payload_item_print_owned(epi: &event_payload_item) -> Vec<u8> {
    let mut size: size_t = 0;
    let mut evb = evbuffer_new();
    event_payload_add_item(epi, &mut evb);
    size = evbuffer_get_length(&evb);
    let mut value = Vec::with_capacity(size + 1);
    value.extend_from_slice(evbuffer_pullup(&mut evb, -1).unwrap_or_default());
    value.push(0);
    value
}

pub(crate) unsafe fn event_payload_print_owned(ep: &event_payload) -> Option<Vec<u8>> {
    event_payload_find(ep, c"pane").map(|item| event_payload_item_print_owned(item))
}
pub unsafe fn event_payload_add_formats(
    ep: &event_payload,
    mut ft: *mut format_tree,
    prefix: Option<&CStr>,
) {
    let prefix = prefix.unwrap_or(c"").to_bytes();
    for epi in event_payload_items(ep) {
        let key = epi.name.as_ptr();
        if !(*key as ::core::ffi::c_int == '_' as i32) {
            let value = event_payload_item_print_owned(epi);
            let key_bytes = CStr::from_ptr(key).to_bytes();
            let mut name_bytes = Vec::with_capacity(prefix.len() + key_bytes.len());
            name_bytes.extend_from_slice(prefix);
            name_bytes.extend_from_slice(key_bytes);
            let name = CString::new(name_bytes).expect("C string parts contain no NUL");
            // format_add copies the key into its format entry before returning.
            format_add(ft, &name, |out| {
                write_cstr(
                    out,
                    nullable_cstr(value.as_ptr().cast::<::core::ffi::c_char>()),
                )
            });
            let named = if epi.type_0() as ::core::ffi::c_uint
                == EVENT_PAYLOAD_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                Some(epi.value.session().name())
            } else if epi.type_0() as ::core::ffi::c_uint
                == EVENT_PAYLOAD_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                Some(epi.value.window().name())
            } else {
                None
            };
            if let Some(named) = named {
                let mut suffixed = name.as_bytes().to_vec();
                suffixed.extend_from_slice(b"_name");
                let suffixed = CString::new(suffixed).expect("C string parts contain no NUL");
                format_add(ft, &suffixed, |out| write_cstr(out, &*named));
            }
        }
    }
}
pub fn event_payload_item_name(epi: &event_payload_item) -> &CStr {
    &epi.name
}

pub unsafe fn event_payload_log(
    ep: &event_payload,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let prefix = format_message_with(write);
    let mut evb = evbuffer_new();
    for epi in event_payload_items(ep) {
        if evbuffer_get_length(&evb) != 0 as size_t {
            evbuffer_add_formatted(&mut evb, |out| out.write_all(b", "));
        }
        evbuffer_add_formatted(&mut evb, |out| {
            write_cstr(out, &*epi.name)?;
            out.write_all(b"=")
        });
        event_payload_add_item(epi, &mut evb);
    }

    log_debug(format_args!(
        "{}{}",
        log_cstr(&prefix),
        log_cstr_n(evbuffer_pullup(&mut evb, -1).map_or(&[][..], |bytes| bytes))
    ));
}
pub fn event_payload_get_client(ep: &event_payload) -> Option<&ClientRef> {
    match event_payload_find(ep, c"client").map(|item| &item.value) {
        Some(EventPayloadValue::Client(value)) => Some(value),
        _ => None,
    }
}
pub fn event_payload_get_session(ep: &event_payload) -> Option<&SessionRef> {
    match event_payload_find(ep, c"session").map(|item| &item.value) {
        Some(EventPayloadValue::Session(value)) => Some(value),
        _ => None,
    }
}
pub fn event_payload_get_window(ep: &event_payload) -> Option<&WindowRef> {
    match event_payload_find(ep, c"window").map(|item| &item.value) {
        Some(EventPayloadValue::Window(value)) => Some(value),
        _ => None,
    }
}
pub fn event_payload_get_pane(
    ep: &event_payload,
) -> Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
    match event_payload_find(ep, c"pane").map(|item| &item.value) {
        Some(EventPayloadValue::Pane(value)) => Some(value),
        _ => None,
    }
}
pub fn event_payload_get_identity<'a>(
    ep: &'a event_payload,
    name: &CStr,
) -> Option<&'a EventPayloadIdentity> {
    match event_payload_find(ep, name).map(|item| &item.value) {
        Some(EventPayloadValue::Identity(value)) => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ffi::CString;

    #[test]
    fn replacement_accepts_the_previous_items_borrowed_name() {
        unsafe {
            let mut ep = event_payload_create();
            let name = CString::new(vec![b'k', 0xff]).unwrap();
            event_payload_set_string(&mut ep, &name, |out| write_cstr(out, c"old"));
            let old_name =
                event_payload_item_name(event_payload_items(&ep).next().unwrap()).as_ptr();
            event_payload_set_int(&mut ep, CStr::from_ptr(old_name), 42);

            let replacement = event_payload_items(&ep).next().unwrap();
            assert_eq!(
                event_payload_item_name(replacement).to_bytes(),
                name.as_bytes()
            );
            assert_eq!(replacement.value.number(), 42);
            assert_eq!(event_payload_items(&ep).count(), 1);
            drop(ep);
        }
    }

    #[test]
    fn boxed_items_preserve_order_and_addresses_when_the_payload_moves() {
        unsafe {
            let mut payload = event_payload {
                target_pane: None,
                target_window: None,
                target_session: None,
                items: event_payload_tree::default(),
                target: Default::default(),
            };
            event_payload_set_int(&mut payload, c"beta", 2);
            event_payload_set_int(&mut payload, c"alpha", 1);
            let address = event_payload_find(&payload, c"alpha").unwrap()
                as *const event_payload_item as usize;
            let mut moved = payload;
            for i in 0..128 {
                let key = CString::new(format!("extra-{i}")).unwrap();
                event_payload_set_int(&mut moved, &key, i);
            }
            assert_eq!(
                event_payload_items(&moved).next().unwrap() as *const event_payload_item as usize,
                address
            );
            assert_eq!(
                event_payload_items(&moved)
                    .take(2)
                    .map(|item| item.name.to_bytes())
                    .collect::<Vec<_>>(),
                [b"alpha".as_slice(), b"beta".as_slice()]
            );
            let item = moved.items.remove(c"alpha").unwrap();
            assert_eq!(item.value.number(), 1);
            assert!(event_payload_find(&moved, c"alpha").is_none());
            drop(moved);
            assert_eq!(item.name.as_bytes(), b"alpha");
        }
    }
}
