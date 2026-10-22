use crate::format::bytes::cstr_suffix;
use crate::options::OptionsScope;
use crate::shared::window::WindowWeak;
use crate::window::Window as _;
use crate::winlink::winlink_window;
mod alerts;
mod api;
use alerts::alerts_check_all;
mod model;
mod size;
mod sort;
mod spawn;
pub use model::{session, sessions};
mod format;
use crate::session_group::session_group_remove;
pub use crate::session_group::{
    session_group_add, session_group_attached_count, session_group_count, session_group_members,
    session_group_name, session_group_synchronize_from, session_group_synchronize_to,
};
pub use crate::shared::session::session_groups;
pub use api::Session;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::compat::strtonum::strtonum;
use crate::events::{events_fire, events_fire_session, events_fire_winlink};
use crate::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_session,
    event_payload_set_string, event_payload_set_target, event_payload_set_window,
};
use crate::log::{log_bytes, log_cstr, log_debug};
use crate::options::options;
use crate::options::options_get_number;
use crate::resize::recalculate_sizes;
use crate::server::{marked_pane, server_clear_marked};
use crate::server_fn::server_lock_session;
use crate::shared::abi::*;
use crate::shared::command::cmd_find_state;
use crate::shared::environment::environ;
use crate::shared::event::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::pane::window_pane;
use crate::shared::session::{SessionRef, SessionWeak};
use crate::shared::sort::sort_criteria;
use crate::shared::terminal::*;
use crate::shared::tree::{RB_INF, RB_NEGINF};
use crate::shared::window::WindowRef;
use crate::shared::window::WINLINK_ALERTFLAGS;
use crate::shared::window::{winlink, winlink_stack, winlinks};
use crate::tty::tty_update_window_offset;
use crate::window::{
    winlink_add, winlink_clear_flags, winlink_count, winlink_find_by_index, winlink_find_by_window,
    winlink_find_by_window_id, winlink_next, winlink_previous, winlink_remove, winlink_set_window,
    winlink_stack_push, winlink_stack_remove, winlinks_minmax, winlinks_next,
};
use crate::window_pane::WindowPane as _;
use size::recalculate_size_state;
use size::status_update_cache;
use sort::sort_get_sessions;
use std::ffi::{CStr, CString};
use std::rc::Rc;
thread_local! {
    static SESSIONS: std::cell::RefCell<sessions> = const {
        std::cell::RefCell::new(sessions {
            storage: std::collections::BTreeMap::new(),
        })
    };
    static NEXT_SESSION_ID: std::cell::Cell<u_int> = const { std::cell::Cell::new(0) };
}

/// The session index holds the strong owner of each registered session. Each
/// call borrows the index for its own duration and returns owned handles;
/// removed owners are dropped by the caller after the borrow ends.
impl sessions {
    /// Index a session under its current name. An existing entry with the same
    /// name wins and is returned.
    pub fn insert(session: SessionRef) -> Option<SessionRef> {
        let key = session.borrow().name.clone();
        SESSIONS.with_borrow_mut(|index| match index.storage.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => Some(entry.get().clone()),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(session);
                None
            }
        })
    }

    /// Remove and return the index owner, only if `session` is the indexed one.
    pub fn remove(session: &SessionRef) -> Option<SessionRef> {
        let key = session.borrow().name.clone();
        let key = key.as_c_str();
        SESSIONS.with_borrow_mut(|index| {
            if !index
                .storage
                .get(key)
                .is_some_and(|candidate| Rc::ptr_eq(candidate, session))
            {
                return None;
            }
            index.storage.remove(key)
        })
    }

    pub fn first() -> Option<SessionRef> {
        SESSIONS.with_borrow(|index| index.storage.values().next().cloned())
    }

    /// Resume a potentially destructive walk using a saved name and the live
    /// index. The named session and any of its successors may already have
    /// been removed.
    pub fn after(name: &CStr) -> Option<SessionRef> {
        SESSIONS.with_borrow(|index| {
            index
                .storage
                .range::<CStr, _>((std::ops::Bound::Excluded(name), std::ops::Bound::Unbounded))
                .next()
                .map(|(_, node)| Rc::clone(node))
        })
    }

    pub fn find(name: &CStr) -> Option<SessionRef> {
        SESSIONS.with_borrow(|index| index.storage.get(name).cloned())
    }

    pub fn find_by_id(id: u_int) -> Option<SessionRef> {
        SESSIONS.with_borrow(|index| {
            index
                .storage
                .values()
                .find(|owner| owner.borrow().id == id)
                .cloned()
        })
    }

    /// Resolve an observer only while its session remains in this index.
    /// A removed session can still be retained by queued events or other owners.
    pub fn resolve(observer: &SessionWeak) -> Option<SessionRef> {
        let owner = observer.upgrade()?;
        SESSIONS
            .with_borrow(|index| {
                index
                    .storage
                    .values()
                    .any(|candidate| Rc::ptr_eq(candidate, &owner))
            })
            .then_some(owner)
    }

    pub fn is_empty() -> bool {
        SESSIONS.with_borrow(|index| index.storage.is_empty())
    }

    /// The id the next created session will receive.
    pub fn next_id() -> u_int {
        NEXT_SESSION_ID.get()
    }

    fn alloc_id() -> u_int {
        NEXT_SESSION_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        })
    }
}

/// `session.cwd` borrows this value until replacement or early destruction.
fn session_set_cwd(s: &mut session, cwd: Option<CString>) {
    s.cwd = cwd;
}

/// Replace the borrowed public name after callers remove the old map key.
fn session_replace_name(s: &mut session, name: CString) -> CString {
    std::mem::replace(&mut s.name, name)
}
fn session_find_by_id_str(s: &CStr) -> Option<SessionRef> {
    let mut id: u_int = 0;
    if s.to_bytes().first() != Some(&b'$') {
        return None;
    }
    let parsed = strtonum(
        cstr_suffix(s, 1),
        0 as ::core::ffi::c_longlong,
        UINT_MAX as ::core::ffi::c_longlong,
    );
    id = parsed.unwrap_or(0) as u_int;
    if parsed.is_err() {
        return None;
    }
    sessions::find_by_id(id)
}
fn session_create(
    prefix: Option<&CStr>,
    name: Option<&CStr>,
    cwd: &CStr,
    env: Box<environ>,
    oo: Option<options>,
    tio: Option<&termios>,
) -> SessionRef {
    let owner = session::new();
    {
        let mut s = owner.borrow_mut();
        s.tio = tio.copied().map(Box::new);
        s.cwd = Some(cwd.to_owned());

        s.flags = 0 as ::core::ffi::c_int;
        s.lastw.clear();
        s.environ = Some(env);
        s.options = oo;
    }
    status_update_cache(&owner);
    if let Some(name) = name {
        let mut s = owner.borrow_mut();
        drop(session_replace_name(&mut s, name.to_owned()));
        s.id = sessions::alloc_id();
    } else {
        loop {
            let id = sessions::alloc_id();
            let mut generated = prefix.map_or_else(Vec::new, |prefix| {
                let mut bytes = prefix.to_bytes().to_vec();
                bytes.push(b'-');
                bytes
            });
            generated.extend_from_slice(id.to_string().as_bytes());
            {
                let mut s = owner.borrow_mut();
                s.id = id;
                drop(session_replace_name(
                    &mut s,
                    CString::new(generated).expect("generated name has no NUL"),
                ));
            }
            if sessions::find(&owner.name()).is_none() {
                break;
            }
        }
    }
    sessions::insert(Rc::clone(&owner));
    log_debug(format_args!(
        "new session {} ${}",
        log_bytes(owner.name().as_bytes()),
        owner.id()
    ));
    let created = SystemTime::now();
    owner.borrow_mut().creation_time = created;
    session_update_activity(&owner, Some(created));
    owner
}
/// Consume one session owner; the final owner performs cleanup immediately.
fn session_remove_ref(s: SessionRef, from: &CStr) {
    log_debug(format_args!(
        "release session {} ({})",
        log_bytes(s.name().as_bytes()),
        log_bytes(from.to_bytes())
    ));
    drop(s);
}
fn session_free(s: &mut session) {
    log_debug(format_args!(
        "session {} freed",
        log_bytes(s.name.as_bytes())
    ));
    drop(s.environ.take());
    drop(s.options.take());
}
fn session_destroy(s_owner: &SessionRef, mut notify: ::core::ffi::c_int, from: &CStr) {
    log_debug(format_args!(
        "session {} destroyed ({})",
        log_bytes(s_owner.name().as_bytes()),
        log_cstr(from)
    ));
    if s_owner.borrow().curw.is_empty() {
        return;
    }
    s_owner.borrow_mut().set_curw((refbox::Weak::new()).clone());
    let owner = sessions::remove(s_owner).expect("registered session owner");
    if notify != 0 {
        events_fire_session(c"session-closed", std::rc::Rc::clone(s_owner));
    }
    let lock_timer = {
        let mut s = s_owner.borrow_mut();
        s.tio = None;
        s.lock_timer.take()
    };
    drop(lock_timer);
    session_group_remove(s_owner);
    s_owner.borrow_mut().lastw.clear();
    while !s_owner.with_winlinks(crate::window::winlinks_is_empty) {
        let wl = s_owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        events_fire_winlink(c"window-unlinked", wl.clone());
        winlink_remove(wl.clone());
    }
    session_set_cwd(&mut s_owner.borrow_mut(), None);
    session_remove_ref(owner, c"session_destroy");
}
fn session_lock_timer(owner: &SessionRef) {
    if !owner.is_registered() || !owner.is_attached() {
        return;
    }
    log_debug(format_args!(
        "session {} locked, activity time {}",
        log_bytes(owner.name().as_bytes()),
        crate::shared::time::unix_seconds(owner.activity_time()) as ::core::ffi::c_longlong
    ));
    server_lock_session(owner);
    recalculate_sizes();
}
fn session_update_activity(owner: &SessionRef, from: Option<SystemTime>) {
    let lock_timer = {
        let mut session = owner.borrow_mut();
        if let Some(from) = from {
            session.activity_time = from;
        } else {
            session.activity_time = SystemTime::now();
        }
        log_debug(format_args!(
            "session ${} {} activity {}.{:06}",
            session.id,
            log_bytes(session.name.as_bytes()),
            crate::shared::time::unix_seconds(session.activity_time) as ::core::ffi::c_longlong,
            session
                .activity_time
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_micros() as ::core::ffi::c_int
        ));
        session.lock_timer.take()
    };
    drop(lock_timer);
    if owner.is_attached() {
        let timeout = Duration::from_secs(
            (crate::options::options_get_number(&owner.borrow_options_mut(), c"lock-after-time"))
                as u64,
        );
        if timeout.as_secs() != 0 {
            let observer = Rc::downgrade(owner);
            let timer = Timer::new(timeout, move || {
                if let Some(owner) = observer.upgrade() {
                    session_lock_timer(&owner);
                }
            })
            .expect("arm timer");
            owner.borrow_mut().lock_timer = Some(timer);
        }
    }
}
fn session_adjacent(
    s: Option<&SessionRef>,
    sort_crit: &sort_criteria,
    previous: bool,
) -> Option<SessionRef> {
    let s = s?;
    let sorted = sort_get_sessions(sort_crit);
    let index = sorted.iter().position(|owner| Rc::ptr_eq(owner, s))?;
    let selected = if previous {
        if index == 0 {
            sorted.len() - 1
        } else {
            index - 1
        }
    } else {
        (index + 1) % sorted.len()
    };
    Some(sorted[selected].clone())
}
fn session_attach(
    s_owner: &SessionRef,
    window_owner: &WindowRef,
    mut idx: ::core::ffi::c_int,
) -> Result<refbox::Weak<winlink>, std::ffi::CString> {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = winlink_add(&s_owner.borrow().windows, idx);
    if !wl.is_alive() {
        return Err(std::ffi::CString::new(format!("index in use: {idx}"))
            .expect("numeric diagnostic contains no NUL"));
    }
    wl.try_borrow_mut().expect("live winlink").session = std::rc::Rc::downgrade(s_owner);
    winlink_set_window(wl.clone(), window_owner);
    events_fire_winlink(c"window-linked", wl.clone());
    session_group_synchronize_from(s_owner);
    Ok(wl)
}
fn session_detach(s_owner: &SessionRef, mut wl: refbox::Weak<winlink>) -> ::core::ffi::c_int {
    if s_owner.with_winlinks(|links| {
        winlinks_minmax(links, RB_NEGINF) == wl && winlinks_minmax(links, RB_INF) == wl
    }) {
        return 1 as ::core::ffi::c_int;
    }
    if s_owner.current_winlink() == wl
        && session_last(s_owner) != 0 as ::core::ffi::c_int
        && session_previous(s_owner, 0 as ::core::ffi::c_int) != 0 as ::core::ffi::c_int
    {
        session_next(s_owner, 0 as ::core::ffi::c_int);
    }
    wl.try_borrow_mut().expect("live winlink").flags &= !WINLINK_ALERTFLAGS;
    events_fire_winlink(c"window-unlinked", wl.clone());
    winlink_stack_remove(&mut s_owner.borrow_mut().lastw, wl.clone());
    winlink_remove(wl.clone());
    session_group_synchronize_from(s_owner);
    0 as ::core::ffi::c_int
}

fn session_is_linked(s: Option<&SessionRef>, w: &WindowRef) -> bool {
    let mut next = w.next_winlink(None);
    let mut n = 0;
    while next.is_alive() {
        n += 1;
        next = w.next_winlink(Some(next));
    }

    let group = crate::session::session_groups::for_session(
        &s.map_or_else(std::rc::Weak::new, Rc::downgrade),
    );
    if !group.is_alive() {
        return n != 1;
    }

    n != session_group_count(&group)
}

fn session_next_alert(mut wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    while wl.is_alive() {
        if wl.try_borrow().expect("live winlink").flags & WINLINK_ALERTFLAGS != 0 {
            break;
        }
        wl = winlink_next(&wl);
    }
    wl
}
fn session_next(s_owner: &SessionRef, mut alert: ::core::ffi::c_int) -> ::core::ffi::c_int {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if !s_owner.current_winlink().is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    wl = winlink_next(&s_owner.current_winlink());
    if alert != 0 {
        wl = session_next_alert(wl.clone());
    }
    if !wl.is_alive() {
        wl = s_owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        if alert != 0 && {
            wl = session_next_alert(wl.clone());
            !wl.is_alive()
        } {
            return -(1 as ::core::ffi::c_int);
        }
    }
    session_set_current(s_owner, wl.clone())
}
fn session_previous_alert(mut wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    while wl.is_alive() {
        if wl.try_borrow().expect("live winlink").flags & WINLINK_ALERTFLAGS != 0 {
            break;
        }
        wl = winlink_previous(&wl);
    }
    wl
}
fn session_previous(s_owner: &SessionRef, mut alert: ::core::ffi::c_int) -> ::core::ffi::c_int {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if !s_owner.current_winlink().is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    wl = winlink_previous(&s_owner.current_winlink());
    if alert != 0 {
        wl = session_previous_alert(wl.clone());
    }
    if !wl.is_alive() {
        wl = s_owner.with_winlinks(|links| winlinks_minmax(links, RB_INF));
        if alert != 0 && {
            wl = session_previous_alert(wl.clone());
            !wl.is_alive()
        } {
            return -(1 as ::core::ffi::c_int);
        }
    }
    session_set_current(s_owner, wl.clone())
}
fn session_select(s_owner: &SessionRef, mut idx: ::core::ffi::c_int) -> ::core::ffi::c_int {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = s_owner.with_winlinks(|links| winlink_find_by_index(links, idx));
    session_set_current(s_owner, wl.clone())
}
fn session_last(s_owner: &SessionRef) -> ::core::ffi::c_int {
    let wl = s_owner.last_winlink();
    if !wl.is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    if wl == s_owner.current_winlink() {
        return 1 as ::core::ffi::c_int;
    }
    s_owner.select_winlink(wl)
}
fn session_fire_window_changed(
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
    mut old: refbox::Weak<winlink>,
) {
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_winlink(wl.clone(), 0 as ::core::ffi::c_int);
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_session(&mut ep, c"session", std::rc::Rc::clone(s_owner));
    event_payload_set_window(
        &mut ep,
        c"window",
        winlink_window(&wl).expect("live window"),
    );
    event_payload_set_window(
        &mut ep,
        c"new_window",
        winlink_window(&wl).expect("live window"),
    );
    event_payload_set_int(
        &mut ep,
        c"window_index",
        wl.try_borrow().expect("live winlink").idx,
    );
    event_payload_set_int(
        &mut ep,
        c"new_window_index",
        wl.try_borrow().expect("live winlink").idx,
    );
    if old.is_alive() {
        event_payload_set_window(
            &mut ep,
            c"old_window",
            winlink_window(&old).expect("live window"),
        );
        event_payload_set_int(
            &mut ep,
            c"old_window_index",
            old.try_borrow().expect("live winlink").idx,
        );
    }
    events_fire(c"session-window-changed", ep);
}
fn session_set_current(s_owner: &SessionRef, mut wl: refbox::Weak<winlink>) -> ::core::ffi::c_int {
    let mut old: refbox::Weak<winlink> = s_owner.current_winlink();
    if !wl.is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    if wl == old {
        return 1 as ::core::ffi::c_int;
    }
    {
        let mut s = s_owner.borrow_mut();
        winlink_stack_remove(&mut s.lastw, wl.clone());
        winlink_stack_push(&mut s.lastw, old.clone());
        s.set_curw(wl.clone());
    }
    if OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"focus-events")) != 0 {
        if old.is_alive() {
            let window = winlink_window(&old);
            if let Some(window) = window {
                window.update_focus();
            }
        }
        let window = winlink_window(&wl);
        if let Some(window) = window {
            window.update_focus();
        }
    }
    winlink_clear_flags(wl.clone());
    winlink_window(&wl).expect("live window").update_activity();
    tty_update_window_offset(&winlink_window(&wl).expect("live window"));
    session_fire_window_changed(s_owner, wl.clone(), (old).clone());
    0 as ::core::ffi::c_int
}
/// Rebuild Session-owned associations. Group membership and group traversal are
/// deliberately outside this operation. Each model loan ends before notification.
fn session_synchronize_windows(source: &SessionRef, destination: &SessionRef) {
    assert!(
        !Rc::ptr_eq(source, destination),
        "synchronized sessions must be distinct"
    );
    if source.with_winlinks(crate::window::winlinks_is_empty) {
        return;
    }
    let current = destination.current_winlink();
    if current.is_alive()
        && !source
            .with_winlinks(|links| {
                winlink_find_by_index(links, current.try_borrow().expect("live winlink").idx)
            })
            .is_alive()
        && session_last(destination) != 0
        && session_previous(destination, 0) != 0
    {
        session_next(destination, 0);
    }

    let mut old_windows = std::mem::take(&mut destination.borrow_mut().windows);
    let mut link = source.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() {
        let index = link.try_borrow().expect("live winlink").idx;
        let mut replacement = winlink_add(&destination.borrow().windows, index);
        replacement.try_borrow_mut().expect("live winlink").session = Rc::downgrade(destination);
        let window = winlink_window(&link).expect("source window").clone();
        winlink_set_window(replacement.clone(), &window);
        drop(window);
        events_fire_winlink(c"window-linked", replacement.clone());
        replacement.try_borrow_mut().expect("live winlink").flags |=
            link.try_borrow().expect("live winlink").flags & WINLINK_ALERTFLAGS;
        link = winlink_next(&link);
    }

    let current = destination.current_winlink();
    if current.is_alive() {
        let index = current.try_borrow().expect("live winlink").idx;
        let replacement = destination.with_winlinks(|links| winlink_find_by_index(links, index));
        destination.borrow_mut().curw = replacement;
    } else {
        let current = source.current_winlink();
        if current.is_alive() {
            let index = current.try_borrow().expect("live winlink").idx;
            let replacement =
                destination.with_winlinks(|links| winlink_find_by_index(links, index));
            destination.borrow_mut().curw = replacement;
        }
    }
    if !destination.current_winlink().is_alive() {
        let first = destination.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        destination.borrow_mut().curw = first;
    }

    let mut old_history = std::mem::take(&mut destination.borrow_mut().lastw);
    for index in crate::window::winlink_stack_indices(&old_history) {
        let replacement = destination.with_winlinks(|links| winlink_find_by_index(links, index));
        if replacement.is_alive() {
            crate::window::winlink_stack_append(&mut destination.borrow_mut().lastw, replacement);
        }
    }
    while !crate::window::winlinks_is_empty(&old_windows) {
        let old = winlinks_minmax(&old_windows, RB_NEGINF);
        let id = winlink_window(&old).expect("old window").id();
        let replacement = destination.with_winlinks(|links| winlink_find_by_window_id(links, id));
        if !replacement.is_alive() {
            events_fire_winlink(c"window-unlinked", old.clone());
        }
        winlink_remove(old);
    }
}

fn session_renumber_windows(s_owner: &SessionRef) {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wl1: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wl_new: refbox::Weak<winlink> = refbox::Weak::new();
    let mut old_lastw: winlink_stack;
    let mut new_idx: ::core::ffi::c_int = 0;
    let mut new_curw_idx: ::core::ffi::c_int = 0;
    let mut marked_idx: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    new_idx =
        options_get_number(&s_owner.borrow_options_mut(), c"base-index") as ::core::ffi::c_int;
    let n = s_owner.with_winlinks(winlink_count);
    if n > (i32::MAX as u_int) - (new_idx as u_int) + 1 {
        return;
    }
    let mut old_wins: winlinks = std::mem::take(&mut s_owner.borrow_mut().windows);
    new_curw_idx = 0 as ::core::ffi::c_int;
    wl = winlinks_minmax(&old_wins, RB_NEGINF);
    while wl.is_alive() {
        wl_new = winlink_add(&s_owner.borrow().windows, new_idx);
        wl_new.try_borrow_mut().expect("live winlink").session = std::rc::Rc::downgrade(s_owner);
        winlink_set_window((wl_new).clone(), &winlink_window(&wl).expect("live window"));
        wl_new.try_borrow_mut().expect("live winlink").flags |=
            wl.try_borrow().expect("live winlink").flags & WINLINK_ALERTFLAGS;
        if wl == marked_pane.with_borrow(|marked| marked.winlink_handle()) {
            marked_idx = wl_new.try_borrow().expect("live winlink").idx;
        }
        if wl == s_owner.current_winlink() {
            new_curw_idx = wl_new.try_borrow().expect("live winlink").idx;
        }
        wl = winlink_next(&wl);
        if wl.is_alive() {
            new_idx += 1;
        }
    }
    old_lastw = std::mem::take(&mut s_owner.borrow_mut().lastw);
    for old_idx in crate::window::winlink_stack_indices(&old_lastw) {
        wl = winlink_find_by_index(&old_wins, old_idx);
        if !wl.is_alive() {
            continue;
        }
        let window = winlink_window(&wl).expect("live window");
        wl_new = s_owner.with_winlinks(|links| winlink_find_by_window(links, &window));
        if wl_new.is_alive() {
            crate::window::winlink_stack_append(&mut s_owner.borrow_mut().lastw, (wl_new).clone());
        }
    }
    if marked_idx != -(1 as ::core::ffi::c_int) {
        let marked_wl = s_owner.with_winlinks(|links| winlink_find_by_index(links, marked_idx));
        marked_pane.with_borrow_mut(|marked| marked.set_wl(marked_wl.clone()));
        if !marked_wl.is_alive() {
            server_clear_marked();
        }
    }
    let curw = s_owner.with_winlinks(|links| winlink_find_by_index(links, new_curw_idx));
    s_owner.borrow_mut().set_curw(curw);
    wl = winlinks_minmax(&old_wins, RB_NEGINF);
    while wl.is_alive() && {
        wl1 = winlinks_next(&wl.try_borrow().expect("live winlink"));
        1 as ::core::ffi::c_int != 0
    } {
        winlink_remove(wl.clone());
        wl = wl1;
    }
}
fn session_theme_changed(owner: &SessionRef) {
    let mut link = owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() {
        let panes = link
            .try_borrow()
            .expect("live winlink")
            .window_handle()
            .map(|window| window.pane_snapshot())
            .unwrap_or_default();
        for owner in panes.iter().filter_map(std::rc::Weak::upgrade) {
            owner.mark_theme_changed();
        }
        link = winlink_next(&link);
    }
}
fn session_update_history(owner: &SessionRef) {
    let limit =
        crate::options::options_get_number(&owner.borrow_options_mut(), c"history-limit") as u_int;
    let mut link = owner.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() {
        let panes = link
            .try_borrow()
            .expect("live winlink")
            .window_handle()
            .map(|window| window.pane_snapshot())
            .unwrap_or_default();
        for owner in panes.iter().filter_map(std::rc::Weak::upgrade) {
            owner.update_history_limit(limit);
        }
        link = winlink_next(&link);
    }
}

/// An unregistered session with no windows or options, for tests elsewhere.
#[cfg(test)]
pub(crate) fn session_new_for_test() -> SessionRef {
    session::new()
}

#[cfg(test)]
mod session_index_tests {
    use super::*;

    #[test]
    fn current_winlink_observer_expires_when_index_removes_it() {
        let owner = session::new();
        let mut session = owner.borrow_mut();
        let link = winlink_add(&session.windows, 4);
        session.set_curw(link.clone());
        assert_eq!(session.current_winlink(), link);
        winlink_remove(link.clone());
        assert!(!session.current_winlink().is_alive());
        assert!(!session.curw.is_empty());
        session.set_curw(refbox::Weak::new());
        assert!(session.curw.is_empty());
    }
}

impl Drop for session {
    fn drop(&mut self) {
        session_free(self)
    }
}

#[cfg(test)]
mod winlink_storage_tests;

#[cfg(test)]
mod session_lastw_queue_tests;

#[cfg(test)]
mod storage_tests;
