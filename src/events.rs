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

/// The sink registry. Callbacks may add or remove sinks while an event is
/// dispatched: added sinks carry a newer generation and are skipped by that
/// dispatch, removed ones are marked dead and freed once dispatch unwinds.
struct events {
    sinks: Vec<events_sink>,
    dispatching: u_int,
    generation: u_int,
    next_sink_id: u64,
}

thread_local! {
    static EVENTS: std::cell::RefCell<events> = const {
        std::cell::RefCell::new(events {
            sinks: Vec::new(),
            dispatching: 0,
            generation: 0,
            next_sink_id: 0,
        })
    };
}

/// Each call borrows the registry only for its own duration; callbacks and
/// sink drops run after the borrow ends.
impl events {
    fn add_sink(name: &CStr, cb: events_cb) -> EventSinkId {
        EVENTS.with_borrow_mut(|events| {
            events.generation = events.generation.wrapping_add(1);
            events.next_sink_id = events
                .next_sink_id
                .checked_add(1)
                .expect("event sink IDs exhausted");
            let id = EventSinkId(events.next_sink_id);
            events.sinks.push(events_sink {
                id,
                name: name.to_owned(),
                cb,
                dead: 0,
                generation: events.generation,
            });
            id
        })
    }

    /// Removes the sink now, or marks it dead while a dispatch is running.
    fn remove_sink(id: EventSinkId) -> Option<events_sink> {
        EVENTS.with_borrow_mut(|events| {
            let index = events.sinks.iter().position(|sink| sink.id == id)?;
            if events.sinks[index].dead != 0 {
                return None;
            }
            if events.dispatching != 0 {
                events.sinks[index].dead = 1;
                None
            } else {
                Some(events.sinks.remove(index))
            }
        })
    }

    /// Enters a dispatch and returns the generation it may deliver to.
    fn begin_dispatch() -> u_int {
        EVENTS.with_borrow_mut(|events| {
            events.dispatching = events.dispatching.wrapping_add(1);
            events.generation
        })
    }

    /// The callback at `index`: `None` past the end, `Some(None)` if that
    /// sink does not receive this event.
    fn callback_at(index: usize, name: &CStr, generation: u_int) -> Option<Option<events_cb>> {
        EVENTS.with_borrow(|events| {
            let sink = events.sinks.get(index)?;
            Some(
                (sink.dead == 0 && sink.generation <= generation && sink.name.as_c_str() == name)
                    .then(|| sink.cb.clone()),
            )
        })
    }

    /// Leaves a dispatch; the outermost one takes out the dead sinks.
    fn end_dispatch() -> Vec<events_sink> {
        EVENTS.with_borrow_mut(|events| {
            events.dispatching = events.dispatching.wrapping_sub(1);
            if events.dispatching != 0 {
                return Vec::new();
            }
            let (dead, live) = std::mem::take(&mut events.sinks)
                .into_iter()
                .partition(|sink| sink.dead != 0);
            events.sinks = live;
            dead
        })
    }
}

pub unsafe fn events_add_sink(name: &CStr, cb: events_cb) -> EventSinkId {
    events::add_sink(name, cb)
}
pub unsafe fn events_remove_sink(id: EventSinkId) {
    if id == EventSinkId::default() {
        return;
    }
    drop(events::remove_sink(id));
}
/// Whether the sink `id` is registered and not removed.
#[cfg(test)]
pub(crate) fn events_has_sink(id: EventSinkId) -> bool {
    EVENTS.with_borrow(|events| {
        events
            .sinks
            .iter()
            .any(|sink| sink.id == id && sink.dead == 0)
    })
}
pub unsafe fn events_fire(name: &CStr, mut ep: Box<event_payload>) {
    event_payload_set_string(&mut ep, c"event", |out| write_cstr(out, &*name));
    if log_get_level() != 0 as ::core::ffi::c_int {
        event_payload_log(&ep, |out| {
            write_cstr(out, c"events_fire")?;
            out.write_all(b": ")?;
            write_cstr(out, &*name)?;
            out.write_all(b": ")
        });
    }
    let generation = events::begin_dispatch();
    let mut index = 0;
    while let Some(callback) = events::callback_at(index, name, generation) {
        index += 1;
        if let Some(callback) = callback {
            callback(name, &mut ep);
        }
    }
    drop(events::end_dispatch());
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
    fs = cmd_find_state::find_from_client(Some(&owner), 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
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
        fs = cmd_find_state::find_from_session(&owner, 0 as ::core::ffi::c_int);
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
    fs = cmd_find_state::find_from_window(&WindowRef::clone(&(owner)), 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
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
    fs = cmd_find_state::find_from_pane(&owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
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
    fs = cmd_find_state::find_from_winlink(wl.clone(), 0 as ::core::ffi::c_int);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::shared::events::events_callback;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn callbacks_may_add_and_remove_sinks_during_dispatch() {
        unsafe {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let added = Rc::new(Cell::new(EventSinkId::default()));
            let victim = {
                let calls = calls.clone();
                events_add_sink(
                    c"test-event",
                    events_callback(move |_, _| calls.borrow_mut().push("victim")),
                )
            };
            let first = {
                let calls = calls.clone();
                let added = added.clone();
                events_add_sink(
                    c"test-event",
                    events_callback(move |_, _| {
                        calls.borrow_mut().push("first");
                        events_remove_sink(victim);
                        if added.get() == EventSinkId::default() {
                            let calls = calls.clone();
                            added.set(events_add_sink(
                                c"test-event",
                                events_callback(move |_, _| calls.borrow_mut().push("added")),
                            ));
                        }
                    }),
                )
            };
            // victim runs first, then first removes it and adds a sink that
            // this dispatch must skip.
            events_fire(c"test-event", event_payload_create());
            assert_eq!(*calls.borrow(), ["victim", "first"]);
            assert_eq!(EVENTS.with_borrow(|events| events.sinks.len()), 2);
            assert_eq!(EVENTS.with_borrow(|events| events.dispatching), 0);

            calls.borrow_mut().clear();
            events_fire(c"test-event", event_payload_create());
            assert_eq!(*calls.borrow(), ["first", "added"]);

            events_remove_sink(first);
            events_remove_sink(added.get());
            assert!(EVENTS.with_borrow(|events| events.sinks.is_empty()));
        }
    }
}
