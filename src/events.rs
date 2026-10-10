use crate::src::cmd::find::{
    cmd_find_from_client, cmd_find_from_pane, cmd_find_from_session, cmd_find_from_window,
    cmd_find_from_winlink,
};
use crate::src::events_payload::{
    event_payload_create, event_payload_log, event_payload_set_client, event_payload_set_int,
    event_payload_set_pane, event_payload_set_session, event_payload_set_string,
    event_payload_set_target, event_payload_set_window,
};
use crate::src::format::bytes::write_cstr;
use crate::src::log::log_get_level;
use crate::src::session::Session as _;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::events::{event_payload, events_cb, events_sink, EventSinkId};
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::winlink;
use crate::src::shared::window::WindowRef;
use crate::src::window_pane::WindowPane as _;
use std::ffi::CStr;

static mut events_sinks: Vec<Box<events_sink>> = Vec::new();
static mut events_dispatching: u_int = 0;
static mut events_generation: u_int = 0;
static mut events_next_sink_id: u64 = 0;

unsafe fn events_free_dead() {
    loop {
        let removed = {
            let sinks = &mut events_sinks;
            sinks
                .iter()
                .position(|sink| sink.dead != 0)
                .map(|index| sinks.remove(index))
        };
        let Some(owner) = removed else { break };
        drop(owner);
    }
}
pub unsafe fn events_add_sink(name: &CStr, cb: events_cb) -> EventSinkId {
    events_generation = events_generation.wrapping_add(1);
    events_next_sink_id = events_next_sink_id
        .checked_add(1)
        .expect("event sink IDs exhausted");
    let id = EventSinkId(events_next_sink_id);
    let owner = Box::new(events_sink {
        id,
        name: name.to_owned(),
        cb,
        dead: 0,
        generation: events_generation,
    });

    events_sinks.push(owner);
    id
}
pub unsafe fn events_remove_sink(id: EventSinkId) {
    if id == EventSinkId::default() {
        return;
    }
    let removed = {
        let sinks = &mut events_sinks;
        let Some(index) = sinks.iter().position(|sink| sink.id == id) else {
            return;
        };
        if sinks[index].dead != 0 {
            return;
        }
        if events_dispatching != 0 {
            sinks[index].dead = 1;
            None
        } else {
            Some(sinks.remove(index))
        }
    };
    drop(removed);
}
pub unsafe fn events_fire(name: &CStr, mut ep: Box<event_payload>) {
    let mut generation: u_int = events_generation;
    event_payload_set_string(&mut ep, c"event", |out| write_cstr(out, &*name));
    if log_get_level() != 0 as ::core::ffi::c_int {
        event_payload_log(&ep, |out| {
            write_cstr(out, c"events_fire")?;
            out.write_all(b": ")?;
            write_cstr(out, &*name)?;
            out.write_all(b": ")
        });
    }
    events_dispatching = events_dispatching.wrapping_add(1);
    let mut index = 0;
    while index < events_sinks.len() {
        let callback = {
            let sinks = &events_sinks;
            let sink = &sinks[index];
            if sink.dead == 0 && sink.generation <= generation && sink.name.as_c_str() == name {
                Some(sink.cb.clone())
            } else {
                None
            }
        };
        index += 1;
        if let Some(callback) = callback {
            callback(name, &mut ep);
        }
    }
    events_dispatching = events_dispatching.wrapping_sub(1);
    if events_dispatching == 0 as u_int {
        events_free_dead();
    }
    drop(ep);
}
pub unsafe fn events_fire_client(name: &CStr, owner: ClientRef) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    cmd_find_from_client(&raw mut fs, Some(&owner), 0 as ::core::ffi::c_int);
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_client(&mut ep, owner);
    if !fs.session_handle().is_none() {
        event_payload_set_session(
            &mut ep,
            c"session",
            fs.session_handle().expect("live session"),
        );
    }
    if !fs.window_handle().is_none() {
        event_payload_set_window(
            &mut ep,
            c"window",
            WindowRef::clone((fs.window_handle().as_ref()).expect("live window")),
        );
    }
    if fs.winlink_handle().is_alive() {
        event_payload_set_int(
            &mut ep,
            c"window_index",
            (fs.winlink_handle()).get_unchecked().idx,
        );
    } else if fs.idx != -(1 as ::core::ffi::c_int) {
        event_payload_set_int(&mut ep, c"window_index", fs.idx);
    }
    if !fs.pane_handle().is_none() {
        event_payload_set_pane(&mut ep, c"pane", fs.pane_handle().expect("event pane"));
    }
    events_fire(name, ep);
}
pub unsafe fn events_fire_session(name: &CStr, owner: SessionRef) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    if owner.is_registered() {
        cmd_find_from_session(&raw mut fs, &owner, 0 as ::core::ffi::c_int);
        event_payload_set_target(&mut ep, &fs);
    }
    event_payload_set_session(&mut ep, c"session", owner);
    events_fire(name, ep);
}
pub unsafe fn events_fire_window(name: &CStr, owner: WindowRef) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    cmd_find_from_window(
        &raw mut fs,
        &WindowRef::clone(&(owner)),
        0 as ::core::ffi::c_int,
    );
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_window(&mut ep, c"window", owner);
    events_fire(name, ep);
}
pub unsafe fn events_fire_pane(
    name: &CStr,
    owner: std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    cmd_find_from_pane(&raw mut fs, &owner, 0 as ::core::ffi::c_int);
    event_payload_set_target(&mut ep, &fs);
    let window = owner
        .window_observer()
        .upgrade()
        .expect("event pane parent");
    event_payload_set_pane(&mut ep, c"pane", owner);
    event_payload_set_window(&mut ep, c"window", window);
    events_fire(name, ep);
}
pub unsafe fn events_fire_winlink(name: &CStr, mut wl: refbox::Weak<winlink>) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    cmd_find_from_winlink(&raw mut fs, wl.clone(), 0 as ::core::ffi::c_int);
    event_payload_set_target(&mut ep, &fs);
    if let Some(session_owner) = wl.get_unchecked().session.upgrade() {
        event_payload_set_session(&mut ep, c"session", session_owner.clone());
    }
    event_payload_set_window(
        &mut ep,
        c"window",
        WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_int(&mut ep, c"window_index", wl.get_unchecked().idx);
    events_fire(name, ep);
}
