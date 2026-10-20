use super::types::{
    monitor_cb, monitor_change, monitor_item, monitor_items, monitor_pane, monitor_type,
    monitor_window, MONITOR_ALL_PANES, MONITOR_ALL_WINDOWS, MONITOR_NOTIFY_INITIAL,
    MONITOR_NOTIFY_TRUE, MONITOR_PANE, MONITOR_SESSION, MONITOR_WINDOW,
};
use crate::format::{format_create, format_defaults, format_expand_cstring, format_true};
use crate::log::{log_cstr, log_debug};
use crate::reactor::Timer;
use crate::server::current_time;
use crate::server_client::Client as _;
use crate::session::{sessions, Session as _};
use crate::shared::abi::*;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::format::{format_tree, FORMAT_NOJOBS};
use crate::shared::pane::window_pane;
use crate::shared::session::SessionRef;
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::winlink;
use crate::window::Window as _;
use crate::window::{winlinks_minmax, winlinks_next};
use crate::window_pane::WindowPane as _;
use libc::sscanf;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// Owned by one control client or hook, which calls monitor_destroy on removal.
/// Operations and timers use weak observers and do not prolong this ownership.
pub struct monitor_set {
    client: ClientWeak,
    session: Option<SessionRef>,
    callback: Option<monitor_cb>,
    items: monitor_items,
    timer: Option<Timer>,
    generation: u_int,
}

fn monitor_borrow(observer: &refbox::Weak<monitor_set>) -> Option<refbox::Borrow<'_, monitor_set>> {
    match observer.try_borrow_mut() {
        Ok(state) => Some(state),
        Err(refbox::BorrowError::Dropped) => None,
        Err(refbox::BorrowError::Borrowed) => panic!("monitor state already borrowed"),
    }
}

fn monitor_first_item(observer: &refbox::Weak<monitor_set>) -> Option<CString> {
    monitor_borrow(observer)?
        .items
        .first_key_value()
        .map(|(_, item)| item.name.clone())
}
fn monitor_next_item(observer: &refbox::Weak<monitor_set>, after: &CStr) -> Option<CString> {
    monitor_borrow(observer)?
        .items
        .range::<CStr, _>((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .next()
        .map(|(_, item)| item.name.clone())
}
fn monitor_with_item<R>(
    observer: &refbox::Weak<monitor_set>,
    name: &CStr,
    operation: impl FnOnce(&mut monitor_item) -> R,
) -> Option<R> {
    let mut state = monitor_borrow(observer)?;
    state.items.get_mut(name).map(|item| operation(item))
}
unsafe fn monitor_expand(
    monitor: &refbox::Weak<monitor_set>,
    name: &CStr,
    formats: &mut format_tree,
) -> Option<CString> {
    let expression = monitor_with_item(monitor, name, |item| item.format.clone())?;
    Some(format_expand_cstring(formats, &expression))
}
fn monitor_next_generation(observer: &refbox::Weak<monitor_set>) -> Option<u32> {
    let mut state = monitor_borrow(observer)?;
    state.generation = state.generation.wrapping_add(1).max(1);
    Some(state.generation)
}

fn monitor_visit(
    monitor: &refbox::Weak<monitor_set>,
    mut visit: impl FnMut(CString, monitor_type),
) {
    let mut cursor = monitor_first_item(monitor);
    while let Some(name) = cursor {
        if !monitor.is_alive() {
            break;
        }
        let next = monitor_next_item(monitor, &name);
        if let Some(kind) = monitor_with_item(monitor, &name, |item| item.type_0) {
            visit(name, kind);
        }
        cursor = next;
    }
}

fn monitor_has_client(monitor: &refbox::Weak<monitor_set>) -> bool {
    monitor_borrow(monitor).is_some_and(|state| !state.client.ptr_eq(&Weak::new()))
}
unsafe fn monitor_client(monitor: &refbox::Weak<monitor_set>) -> Option<ClientRef> {
    let observer = monitor_borrow(monitor)?.client.clone();
    let client = observer.upgrade()?;
    (!client.is_dead()).then_some(client)
}
unsafe fn monitor_get_session(
    monitor: &refbox::Weak<monitor_set>,
    client: Option<&ClientRef>,
) -> Option<SessionRef> {
    let (has_client, session) = {
        let state = monitor_borrow(monitor)?;
        (!state.client.ptr_eq(&Weak::new()), state.session.clone())
    };
    if has_client {
        return client?.attached_session().upgrade();
    }
    let Some(session) = session else {
        return sessions::first();
    };
    let indexed = crate::session::sessions::find_by_id(session.id())?;
    Rc::ptr_eq(&session, &indexed).then_some(indexed)
}
unsafe fn monitor_context(
    monitor: &refbox::Weak<monitor_set>,
) -> Option<(Option<ClientRef>, SessionRef)> {
    if !monitor.is_alive() {
        return None;
    }
    let client = monitor_client(monitor);
    if monitor_has_client(monitor) && client.is_none() {
        return None;
    }
    let session = monitor_get_session(monitor, client.as_ref())?;
    Some((client, session))
}
unsafe fn monitor_create_formats(
    client: Option<&ClientRef>,
    session: Option<&SessionRef>,
    link: refbox::Weak<winlink>,
    pane: Option<&Rc<UnsafeCell<window_pane>>>,
) -> Box<format_tree> {
    let mut formats = format_create(None, None, 0, FORMAT_NOJOBS);
    format_defaults(&mut formats, client, session, link, pane);
    formats
}

#[derive(Clone, Copy)]
enum MonitorValueTarget {
    Session,
    Pane(u32, u32),
    Window(u32, u32),
}
unsafe fn monitor_check_value(
    monitor: &refbox::Weak<monitor_set>,
    name: &CStr,
    session: Option<&SessionRef>,
    link: refbox::Weak<winlink>,
    pane: Option<&Rc<UnsafeCell<window_pane>>>,
    value: &CStr,
    target: MonitorValueTarget,
    generation: Option<u32>,
) {
    let truth = format_true(value);
    let Some(state) = monitor_borrow(monitor) else {
        return;
    };
    let generation = generation.map(|_| state.generation);
    drop(state);
    let record = monitor_with_item(monitor, name, |item| {
        let last = match target {
            MonitorValueTarget::Session => &mut item.last,
            MonitorValueTarget::Pane(pane, index) => {
                let record = item.panes.entry((pane, index)).or_insert_with(|| {
                    Box::new(monitor_pane {
                        pane,
                        idx: index,
                        ..monitor_pane::empty()
                    })
                });
                if let Some(generation) = generation {
                    record.generation = generation;
                }
                &mut record.last
            }
            MonitorValueTarget::Window(window, index) => {
                let record = item.windows.entry((window, index)).or_insert_with(|| {
                    Box::new(monitor_window {
                        window,
                        idx: index,
                        ..monitor_window::empty()
                    })
                });
                if let Some(generation) = generation {
                    record.generation = generation;
                }
                &mut record.last
            }
        };
        if last.as_deref() == Some(value) {
            return None;
        }
        let previous = last.replace(value.to_owned());
        if (previous.is_none() && item.flags & MONITOR_NOTIFY_INITIAL == 0)
            || (item.flags & MONITOR_NOTIFY_TRUE != 0 && !truth)
        {
            return None;
        }
        item.fire_count = item.fire_count.wrapping_add(1);
        item.fire_time = current_time;
        Some((item.name.clone(), previous))
    })
    .flatten();
    let Some((name, previous)) = record else {
        return;
    };
    let (callback, client) = {
        let Some(state) = monitor_borrow(monitor) else {
            return;
        };
        (
            state
                .callback
                .as_ref()
                .expect("live monitor callback")
                .clone(),
            state.client.clone(),
        )
    };
    log_debug(format_args!(
        "monitor_report: {} changed to {}",
        log_cstr(&name),
        log_cstr(value)
    ));
    let change = monitor_change {
        name: &name,
        value,
        last: previous.as_deref(),
        c: client,
        s: session.map_or_else(Weak::new, Rc::downgrade),
        wl: link,
        wp: pane.map_or_else(Weak::new, Rc::downgrade),
    };
    callback(&change);
}
unsafe fn monitor_check_session(
    monitor: &refbox::Weak<monitor_set>,
    name: &CStr,
    formats: &mut format_tree,
) {
    let Some((_client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(value) = monitor_expand(monitor, name, formats) else {
        return;
    };
    monitor_check_value(
        monitor,
        name,
        Some(&session),
        refbox::Weak::new(),
        None,
        &value,
        MonitorValueTarget::Session,
        None,
    );
}
unsafe fn monitor_check_pane(monitor: &refbox::Weak<monitor_set>, name: &CStr) {
    let Some(target) = monitor_with_item(monitor, name, |item| item.id) else {
        return;
    };
    let Some((client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(pane) = Rc::<UnsafeCell<window_pane>>::find_by_id(target) else {
        return;
    };
    if !pane.has_tty() {
        return;
    }
    let Some(window) = pane.window_observer().upgrade() else {
        return;
    };
    let mut link = window.next_winlink(None);
    while link.is_alive() && monitor_with_item(monitor, name, |_| ()).is_some() {
        let matches = link
            .get_unchecked()
            .session
            .ptr_eq(&Rc::downgrade(&session));
        if matches {
            let mut formats =
                monitor_create_formats(client.as_ref(), Some(&session), link.clone(), Some(&pane));
            let value = monitor_expand(monitor, name, &mut formats);
            drop(formats);
            let Some(value) = value else { break };
            if link.is_alive() {
                let index = link.get_unchecked().idx as u32;
                monitor_check_value(
                    monitor,
                    name,
                    Some(&session),
                    link.clone(),
                    Some(&pane),
                    &value,
                    MonitorValueTarget::Pane(pane.id(), index),
                    None,
                );
            }
        }
        if !link.is_alive() {
            break;
        }
        link = window.next_winlink(Some(link));
    }
    drop(window);
}
unsafe fn monitor_check_window(monitor: &refbox::Weak<monitor_set>, name: &CStr) {
    let Some(target) = monitor_with_item(monitor, name, |item| item.id) else {
        return;
    };
    let Some((client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(window) = crate::window::windows::resolve(target) else {
        return;
    };
    let mut link = window.next_winlink(None);
    while link.is_alive() && monitor_with_item(monitor, name, |_| ()).is_some() {
        let matches = link
            .get_unchecked()
            .session
            .ptr_eq(&Rc::downgrade(&session));
        if matches {
            let mut formats =
                monitor_create_formats(client.as_ref(), Some(&session), link.clone(), None);
            let value = monitor_expand(monitor, name, &mut formats);
            drop(formats);
            let Some(value) = value else { break };
            if link.is_alive() {
                let index = link.get_unchecked().idx as u32;
                monitor_check_value(
                    monitor,
                    name,
                    Some(&session),
                    link.clone(),
                    None,
                    &value,
                    MonitorValueTarget::Window(window.id(), index),
                    None,
                );
            }
        }
        if !link.is_alive() {
            break;
        }
        link = window.next_winlink(Some(link));
    }
    drop(window);
}
unsafe fn monitor_check_all_panes_one(
    monitor: &refbox::Weak<monitor_set>,
    name: &CStr,
    formats: &mut format_tree,
    link: refbox::Weak<winlink>,
    pane: &Rc<UnsafeCell<window_pane>>,
    generation: u32,
) {
    let Some((_client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(value) = monitor_expand(monitor, name, formats) else {
        return;
    };
    if !link.is_alive() {
        return;
    }
    let index = link.get_unchecked().idx as u32;
    monitor_check_value(
        monitor,
        name,
        Some(&session),
        link,
        Some(pane),
        &value,
        MonitorValueTarget::Pane(pane.id(), index),
        Some(generation),
    );
}
unsafe fn monitor_check_all_windows_one(
    monitor: &refbox::Weak<monitor_set>,
    name: &CStr,
    formats: &mut format_tree,
    link: refbox::Weak<winlink>,
    generation: u32,
) {
    let Some((_client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(value) = monitor_expand(monitor, name, formats) else {
        return;
    };
    if !link.is_alive() {
        return;
    }
    let (index, window) = {
        let target = link.get_unchecked();
        (
            target.idx as u32,
            target.window_handle().expect("monitor link window").id(),
        )
    };
    monitor_check_value(
        monitor,
        name,
        Some(&session),
        link,
        None,
        &value,
        MonitorValueTarget::Window(window, index),
        Some(generation),
    );
}
fn monitor_sweep_all_panes(item: &mut monitor_item, generation: u32) {
    item.panes
        .retain(|_, record| record.generation == generation);
}
fn monitor_sweep_all_windows(item: &mut monitor_item, generation: u32) {
    item.windows
        .retain(|_, record| record.generation == generation);
}
unsafe fn monitor_check_sessions(monitor: &refbox::Weak<monitor_set>) {
    let Some((client, session)) = monitor_context(monitor) else {
        return;
    };
    let mut formats =
        monitor_create_formats(client.as_ref(), Some(&session), refbox::Weak::new(), None);
    monitor_visit(monitor, |name, kind| {
        if kind == MONITOR_SESSION {
            monitor_check_session(monitor, &name, &mut formats);
        }
    });
    drop(formats);
}
unsafe fn monitor_check_panes_windows(monitor: &refbox::Weak<monitor_set>) {
    monitor_visit(monitor, |name, kind| match kind {
        MONITOR_PANE => monitor_check_pane(monitor, &name),
        MONITOR_WINDOW => monitor_check_window(monitor, &name),
        _ => {}
    });
}
unsafe fn monitor_check_all_panes(monitor: &refbox::Weak<monitor_set>) {
    let Some((client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(generation) = monitor_next_generation(monitor) else {
        return;
    };
    let mut link = session.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() && monitor.is_alive() {
        let window = link.get_unchecked().window_handle().cloned();
        let mut cursor = window
            .as_ref()
            .and_then(|window| window.step_pane(None, false).upgrade());
        while let Some(pane) = cursor {
            if !link.is_alive() || !monitor.is_alive() {
                break;
            }
            let mut formats =
                monitor_create_formats(client.as_ref(), Some(&session), link.clone(), Some(&pane));
            monitor_visit(monitor, |name, kind| {
                if kind == MONITOR_ALL_PANES && link.is_alive() {
                    monitor_check_all_panes_one(
                        monitor,
                        &name,
                        &mut formats,
                        link.clone(),
                        &pane,
                        generation,
                    );
                }
            });
            drop(formats);
            let next_window = pane.window_observer().upgrade();
            cursor = next_window.as_ref().and_then(|window| {
                window
                    .step_pane(Some(&std::rc::Rc::downgrade(&pane)), false)
                    .upgrade()
            });
            if let Some(window) = next_window {
                drop(window);
            }
        }
        if let Some(window) = window {
            drop(window);
        }
        if !link.is_alive() {
            break;
        }
        link = winlinks_next(link.get_unchecked());
    }
    monitor_visit(monitor, |name, kind| {
        if kind == MONITOR_ALL_PANES {
            let generation = monitor_borrow(monitor)
                .expect("live monitor traversal")
                .generation;
            monitor_with_item(monitor, &name, |item| {
                monitor_sweep_all_panes(item, generation)
            });
        }
    });
}
unsafe fn monitor_check_all_windows(monitor: &refbox::Weak<monitor_set>) {
    let Some((client, session)) = monitor_context(monitor) else {
        return;
    };
    let Some(generation) = monitor_next_generation(monitor) else {
        return;
    };
    let mut link = session.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() && monitor.is_alive() {
        let mut formats =
            monitor_create_formats(client.as_ref(), Some(&session), link.clone(), None);
        monitor_visit(monitor, |name, kind| {
            if kind == MONITOR_ALL_WINDOWS && link.is_alive() {
                monitor_check_all_windows_one(
                    monitor,
                    &name,
                    &mut formats,
                    link.clone(),
                    generation,
                );
            }
        });
        drop(formats);
        if !link.is_alive() {
            break;
        }
        link = winlinks_next(link.get_unchecked());
    }
    monitor_visit(monitor, |name, kind| {
        if kind == MONITOR_ALL_WINDOWS {
            let generation = monitor_borrow(monitor)
                .expect("live monitor traversal")
                .generation;
            monitor_with_item(monitor, &name, |item| {
                monitor_sweep_all_windows(item, generation)
            });
        }
    });
}
unsafe fn monitor_timer(monitor: &refbox::Weak<monitor_set>) {
    if let Some(mut state) = monitor_borrow(monitor) {
        drop(state.timer.take());
    }
    let client = monitor_client(monitor);
    if monitor_has_client(monitor) && client.is_none() {
        return;
    }
    if !monitor.is_alive() {
        return;
    }
    log_debug(format_args!("monitor_timer: timer fired"));
    {
        let Some(mut state) = monitor_borrow(monitor) else {
            return;
        };
        let timeout = Duration::from_secs(1);
        let observer = monitor.clone();
        state.timer = Some(
            Timer::new(timeout, move || {
                monitor_timer(&observer);
            })
            .expect("arm timer"),
        );
    }
    let Some(_session) = monitor_get_session(monitor, client.as_ref()) else {
        return;
    };
    let mut have_session = false;
    let mut have_all_panes = false;
    let mut have_all_windows = false;
    monitor_visit(monitor, |_, kind| match kind {
        MONITOR_SESSION => have_session = true,
        MONITOR_ALL_PANES => have_all_panes = true,
        MONITOR_ALL_WINDOWS => have_all_windows = true,
        _ => {}
    });
    if have_session {
        monitor_check_sessions(monitor);
    }
    if !monitor.is_alive() {
        return;
    }
    monitor_check_panes_windows(monitor);
    if !monitor.is_alive() {
        return;
    }
    if have_all_panes {
        monitor_check_all_panes(monitor);
    }
    if !monitor.is_alive() {
        return;
    }
    if have_all_windows {
        monitor_check_all_windows(monitor);
    }
}
fn monitor_create(callback: monitor_cb) -> refbox::RefBox<monitor_set> {
    refbox::RefBox::new(monitor_set {
        client: Weak::new(),
        session: None,
        callback: Some(callback),
        items: Default::default(),
        timer: None,
        generation: 0,
    })
}
pub fn monitor_create_client(
    client: Option<&ClientRef>,
    callback: monitor_cb,
) -> refbox::RefBox<monitor_set> {
    let owner = monitor_create(callback);
    owner.try_borrow_mut().expect("new monitor").client =
        client.map_or_else(Weak::new, Rc::downgrade);
    owner
}
pub fn monitor_create_session(
    session: Option<&SessionRef>,
    callback: monitor_cb,
) -> refbox::RefBox<monitor_set> {
    let owner = monitor_create(callback);
    owner.try_borrow_mut().expect("new monitor").session = session.cloned();
    owner
}
pub unsafe fn monitor_destroy(owner: refbox::RefBox<monitor_set>) {
    let (session, callback) = {
        let mut state = owner
            .try_borrow_mut()
            .expect("monitor destruction outside a borrow");
        (state.session.take(), state.callback.take())
    };
    drop(owner);
    if let Some(session) = session {
        session.release(c"monitor_clear");
    }
    drop(callback);
}
pub fn monitor_create_client_owned(
    client: Option<&ClientRef>,
    callback: monitor_cb,
) -> refbox::RefBox<monitor_set> {
    monitor_create_client(client, callback)
}
pub fn monitor_create_session_owned(
    session: Option<&SessionRef>,
    callback: monitor_cb,
) -> refbox::RefBox<monitor_set> {
    monitor_create_session(session, callback)
}

pub struct ParsedMonitor {
    pub name: CString,
    pub type_0: monitor_type,
    pub id: ::core::ffi::c_int,
    pub format: CString,
}

unsafe fn monitor_parse_parts(value: &CStr) -> Option<ParsedMonitor> {
    let value_bytes = value.to_bytes();
    let mut type_0;
    let mut id = -1;
    let first_colon = value_bytes.iter().position(|&byte| byte == b':')?;
    let target_start = first_colon + 1;
    let target_bytes = &value_bytes[target_start..];
    let second_colon = target_bytes.iter().position(|&byte| byte == b':')?;
    let target_bytes = &target_bytes[..second_colon];
    let format_start = target_start + second_colon + 1;

    if target_bytes == b"%*" {
        type_0 = MONITOR_ALL_PANES;
    } else if target_bytes == b"@*" {
        type_0 = MONITOR_ALL_WINDOWS;
    } else if target_bytes.is_empty() {
        type_0 = MONITOR_SESSION;
    } else {
        let target = CString::new(target_bytes).expect("monitor target contains no NUL");
        if sscanf(target.as_ptr(), c"%%%d".as_ptr(), &raw mut id) == 1 as ::core::ffi::c_int
            && id >= 0 as ::core::ffi::c_int
        {
            type_0 = MONITOR_PANE;
        } else if sscanf(target.as_ptr(), c"@%d".as_ptr(), &raw mut id) == 1 as ::core::ffi::c_int
            && id >= 0 as ::core::ffi::c_int
        {
            type_0 = MONITOR_WINDOW;
        } else {
            return None;
        }
    }

    Some(ParsedMonitor {
        name: CString::new(&value_bytes[..first_colon]).expect("monitor name contains no NUL"),
        type_0,
        id,
        format: CString::new(&value_bytes[format_start..]).expect("monitor format contains no NUL"),
    })
}

pub fn monitor_parse_owned(value: &CStr) -> Option<ParsedMonitor> {
    unsafe { monitor_parse_parts(value) }
}

pub unsafe fn monitor_add(
    monitor: &refbox::Weak<monitor_set>,
    name: &CStr,
    type_0: monitor_type,
    id: i32,
    format: &CStr,
    flags: i32,
) {
    let name = name.to_owned();
    let format = format.to_owned();
    let observer = monitor.clone();
    {
        let Some(mut state) = monitor_borrow(monitor) else {
            return;
        };
        state.items.remove(&name);
        let item = Box::new(monitor_item {
            name,
            format,
            type_0,
            id: id as u32,
            flags,
            ..monitor_item::empty()
        });
        assert!(monitor_items_insert(&mut state.items, item).is_ok());
        if state.timer.is_none() {
            let timeout = Duration::from_secs(1);
            state.timer = Some(
                Timer::new(timeout, move || {
                    monitor_timer(&observer);
                })
                .expect("arm timer"),
            );
        }
    }
}
pub fn monitor_remove(monitor: &refbox::Weak<monitor_set>, name: &CStr) {
    {
        let Some(mut state) = monitor_borrow(monitor) else {
            return;
        };
        state.items.remove(name);
        if state.items.is_empty() {
            drop(state.timer.take());
        }
    }
}
pub fn monitor_get_fire_count(monitor: &refbox::Weak<monitor_set>, name: &CStr) -> u_int {
    monitor_borrow(monitor)
        .and_then(|state| state.items.get(name).map(|item| item.fire_count))
        .unwrap_or(0)
}
pub fn monitor_get_fire_time(monitor: &refbox::Weak<monitor_set>, name: &CStr) -> time_t {
    monitor_borrow(monitor)
        .and_then(|state| state.items.get(name).map(|item| item.fire_time))
        .unwrap_or(0)
}

/// Insert `elm` unless its name is taken; a duplicate is handed back.
fn monitor_items_insert(
    head: &mut monitor_items,
    elm: Box<monitor_item>,
) -> Result<(), Box<monitor_item>> {
    let key = elm.name.clone();
    match head.entry(key) {
        std::collections::btree_map::Entry::Occupied(_) => Err(elm),
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(elm);
            Ok(())
        }
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use tiri_rt::Runtime as _;

    #[test]
    fn traversal_preserves_prefetched_successor_order_and_resolves_live_names() {
        let _runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let owner = monitor_create(Rc::new(|_| {}));
            for name in [c"a", c"c", c"e"] {
                monitor_add(&owner.downgrade(), name, MONITOR_SESSION, -1, c"", 0);
            }
            let mut visited = Vec::new();
            monitor_visit(&owner.downgrade(), |name, _| {
                visited.push(name.to_bytes().to_vec());
                if name.as_c_str() == c"a" {
                    for name in [c"b", c"d"] {
                        monitor_add(&owner.downgrade(), name, MONITOR_SESSION, -1, c"", 0);
                    }
                    monitor_add(&owner.downgrade(), c"c", MONITOR_SESSION, -1, c"new", 0);
                    monitor_remove(&owner.downgrade(), c"a");
                }
                if name.as_c_str() == c"d" {
                    monitor_remove(&owner.downgrade(), c"e");
                }
            });
            assert_eq!(visited, [b"a".to_vec(), b"c".to_vec(), b"d".to_vec()]);
            assert_eq!(
                monitor_first_item(&owner.downgrade()).unwrap().as_c_str(),
                c"b"
            );
            monitor_destroy(owner);
        }
    }

    #[test]
    fn notify_true_updates_last_without_firing_until_truth_changes() {
        use std::cell::RefCell;
        let _runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let changes = Rc::new(RefCell::new(Vec::new()));
            let callback_changes = changes.clone();
            let owner = monitor_create(Rc::new(move |change| {
                callback_changes
                    .borrow_mut()
                    .push((change.value.to_owned(), change.last.map(CStr::to_owned)))
            }));
            monitor_add(
                &owner.downgrade(),
                c"name",
                MONITOR_SESSION,
                -1,
                c"",
                MONITOR_NOTIFY_INITIAL | MONITOR_NOTIFY_TRUE,
            );
            let item = monitor_first_item(&owner.downgrade()).unwrap();
            for value in [c"0", c"0", c"yes", c"yes", c"0", c"next"] {
                monitor_check_value(
                    &owner.downgrade(),
                    &item,
                    None,
                    refbox::Weak::new(),
                    None,
                    value,
                    MonitorValueTarget::Session,
                    None,
                );
            }
            assert_eq!(
                &*changes.borrow(),
                &[
                    (c"yes".to_owned(), Some(c"0".to_owned())),
                    (c"next".to_owned(), Some(c"0".to_owned()))
                ]
            );
            assert_eq!(monitor_get_fire_count(&owner.downgrade(), c"name"), 2);
            assert_eq!(
                monitor_get_fire_time(&owner.downgrade(), c"name"),
                current_time
            );
            monitor_destroy(owner);
        }
    }

    struct Capture {
        set: refbox::Weak<monitor_set>,
        name: Vec<u8>,
        value: Vec<u8>,
        last: Vec<u8>,
    }

    #[test]
    fn changed_value_survives_reentrant_item_removal() {
        let _runtime = tiri_rt::mio::Runtime::new().unwrap();
        unsafe {
            let capture = std::rc::Rc::new(std::cell::RefCell::new(Capture {
                set: refbox::Weak::new(),
                name: Vec::new(),
                value: Vec::new(),
                last: Vec::new(),
            }));
            let callback_capture = capture.clone();
            let set_owner = monitor_create_client(
                None,
                crate::monitor::monitor_callback(move |change| {
                    let mut capture = callback_capture.borrow_mut();
                    capture.value = change.value.to_bytes().to_vec();
                    capture.last = change.last.expect("previous value").to_bytes().to_vec();
                    monitor_remove(&capture.set, change.name);
                    capture.name = change.name.to_bytes().to_vec();
                }),
            );
            let set_observer = set_owner.downgrade();
            let set = &set_observer;
            capture.borrow_mut().set = set.clone();
            monitor_add(set, c"reentrant-last", MONITOR_SESSION, -1, c"value", 0);
            let item = monitor_first_item(set).unwrap();
            let first = CString::from_vec_with_nul(b"\xffold\0".to_vec()).unwrap();
            monitor_check_value(
                set,
                &item,
                None,
                refbox::Weak::new(),
                None,
                &first,
                MonitorValueTarget::Session,
                None,
            );
            assert_eq!(
                monitor_with_item(set, &item, |item| item.last.clone())
                    .unwrap()
                    .as_deref()
                    .expect("string is present")
                    .to_bytes(),
                first.to_bytes()
            );
            monitor_check_value(
                set,
                &item,
                None,
                refbox::Weak::new(),
                None,
                &first,
                MonitorValueTarget::Session,
                None,
            );
            assert!(capture.borrow().value.is_empty());

            let second = CString::from_vec_with_nul(b"\xfeneW\0".to_vec()).unwrap();
            monitor_check_value(
                set,
                &item,
                None,
                refbox::Weak::new(),
                None,
                &second,
                MonitorValueTarget::Session,
                None,
            );
            assert_eq!(capture.borrow().value.as_slice(), second.to_bytes());
            assert_eq!(capture.borrow().last.as_slice(), first.to_bytes());
            assert_eq!(capture.borrow().name.as_slice(), b"reentrant-last");
            assert!(monitor_first_item(set).is_none());
            monitor_destroy(set_owner);
        }
    }

    #[test]
    fn monitor_item_insert_returns_duplicate_to_caller() {
        fn item(name: &str) -> Box<monitor_item> {
            let mut item = Box::new(monitor_item::empty());
            item.name = CString::new(name).unwrap();
            item
        }

        let mut head = monitor_items::new();
        let mut first = item("alpha");
        first.id = 1;
        assert!(monitor_items_insert(&mut head, first).is_ok());
        assert!(monitor_items_insert(&mut head, item("beta")).is_ok());
        let mut second = item("alpha");
        second.id = 2;
        let duplicate =
            monitor_items_insert(&mut head, second).expect_err("duplicate returned to caller");
        assert_eq!(duplicate.id, 2);
        assert_eq!(head[c"alpha"].id, 1);
    }
}
