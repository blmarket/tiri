use crate::src::shared::window::WindowWeak;
use crate::src::window::Window as _;
mod alerts;
mod api;
use alerts::alerts_check_all;
mod model;
mod size;
mod sort;
mod spawn;
pub use model::{session, sessions};
mod format;
use crate::src::session_group::session_group_remove;
pub use crate::src::session_group::{
    session_group_add, session_group_attached_count, session_group_count, session_group_members,
    session_group_name, session_group_synchronize_from, session_group_synchronize_to,
};
pub use crate::src::shared::session::session_groups;
pub use api::{Session, SessionIndex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::src::compat::strtonum::strtonum;
use crate::src::events::{events_fire, events_fire_session, events_fire_winlink};
use crate::src::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_session,
    event_payload_set_string, event_payload_set_target, event_payload_set_window,
};
use crate::src::ffi::libc::strcmp;
use crate::src::log::{log_bytes, log_cstr, log_debug};
use crate::src::options::options_get_number;
use crate::src::options::options_owner_ptr;
use crate::src::resize::recalculate_sizes;
use crate::src::server::{marked_pane, server_clear_marked};
use crate::src::server_fn::server_lock_session;
use crate::src::shared::abi::*;
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::environment::environ;
use crate::src::shared::event::*;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::options::options;
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::{SessionRef, SessionWeak};
use crate::src::shared::sort::sort_criteria;
use crate::src::shared::terminal::*;
use crate::src::shared::tree::{RB_INF, RB_NEGINF};
use crate::src::shared::window::WindowRef;
use crate::src::shared::window::{winlink, winlink_stack, winlinks};
use crate::src::shared::window::{WINLINK_ALERTFLAGS, WINLINK_VISITED};
use crate::src::tmux::global_options;
use crate::src::tty::tty_update_window_offset;
use crate::src::window::{
    winlink_add, winlink_clear_flags, winlink_find_by_index, winlink_find_by_window,
    winlink_find_by_window_id, winlink_next, winlink_previous, winlink_remove, winlink_set_window,
    winlink_stack_push, winlink_stack_remove, winlinks_minmax, winlinks_next,
};
use crate::src::window_pane::WindowPane as _;
use size::recalculate_size_state;
use size::status_update_cache;
use sort::sort_get_sessions;
use std::ffi::{CStr, CString};
use std::rc::Rc;
pub static mut sessions: sessions = sessions {
    storage: std::collections::BTreeMap::new(),
};
static mut next_session_id: u_int = 0;

/// `session.cwd` borrows this value until replacement or early destruction.
fn session_set_cwd(s: &mut session, cwd: Option<CString>) {
    s.cwd = cwd;
}

/// Replace the borrowed public name after callers remove the old map key.
fn session_replace_name(s: &mut session, name: CString) -> CString {
    std::mem::replace(&mut s.name, name)
}
fn sessions_key(elm: &session) -> Vec<u8> {
    elm.name.as_bytes().to_vec()
}
fn sessions_find(head: &sessions, elm: &session) -> Option<SessionRef> {
    head.storage.get(elm.name.as_bytes()).cloned()
}
unsafe fn sessions_insert(head: &mut sessions, session: SessionRef) -> Option<SessionRef> {
    let key = (*session.get()).name.as_bytes().to_vec();
    match head.storage.entry(key) {
        std::collections::btree_map::Entry::Occupied(entry) => Some(entry.get().clone()),
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(session);
            None
        }
    }
}
unsafe fn sessions_remove(head: &mut sessions, elm: &SessionRef) -> Option<SessionRef> {
    let key = (*elm.get()).name.as_bytes();
    if !head
        .storage
        .get(key)
        .is_some_and(|candidate| Rc::ptr_eq(candidate, elm))
    {
        return None;
    }
    head.storage.remove(key)
}
fn sessions_minmax(head: &sessions) -> Option<SessionRef> {
    head.storage.values().next().cloned()
}
/// Resume a potentially destructive walk using a saved name and the live index.
/// The named session and any of its successors may already have been removed.
fn sessions_after(head: &sessions, name: &[u8]) -> Option<SessionRef> {
    head.storage
        .range::<[u8], _>((std::ops::Bound::Excluded(name), std::ops::Bound::Unbounded))
        .next()
        .map(|(_, node)| Rc::clone(node))
}

/// Successor of `elm` by name; `elm` itself need not still be indexed.
fn sessions_next(head: &sessions, elm: &session) -> Option<SessionRef> {
    sessions_after(head, elm.name.as_bytes())
}
/// Resolve an observer only while its session remains in this index.
/// A removed session can still be retained by queued events or other owners.
fn sessions_resolve(head: &sessions, observer: &SessionWeak) -> Option<SessionRef> {
    let owner = observer.upgrade()?;
    head.storage
        .values()
        .any(|candidate| Rc::ptr_eq(candidate, &owner))
        .then_some(owner)
}

unsafe fn session_find(name: &CStr) -> Option<SessionRef> {
    sessions.storage.get(name.to_bytes()).cloned()
}
unsafe fn session_find_by_id_str(s: &CStr) -> Option<SessionRef> {
    let mut id: u_int = 0;
    if s.to_bytes().first() != Some(&b'$') {
        return None;
    }
    let parsed = strtonum(
        CStr::from_ptr(s.as_ptr().add(1)),
        0 as ::core::ffi::c_longlong,
        UINT_MAX as ::core::ffi::c_longlong,
    );
    id = parsed.unwrap_or(0) as u_int;
    if parsed.is_err() {
        return None;
    }
    session_find_by_id(id)
}
unsafe fn session_find_by_id(mut id: u_int) -> Option<SessionRef> {
    let mut s: *mut session = ::core::ptr::null_mut::<session>();
    let mut s_owner = sessions_minmax(&sessions);
    s = s_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    while !s.is_null() {
        if (*s).id == id {
            return s_owner;
        }
        s_owner = sessions_next(&sessions, &*s);
        s = s_owner
            .as_ref()
            .map_or(std::ptr::null_mut(), |owner| owner.get());
    }
    None
}
unsafe fn session_create(
    prefix: Option<&CStr>,
    name: Option<&CStr>,
    cwd: &CStr,
    env: Box<environ>,
    oo: Option<Box<options>>,
    tio: Option<&termios>,
) -> SessionRef {
    let owner = session::new();
    let s = owner.get();
    (*s).tio = tio.copied().map(Box::new);
    (*s).cwd = Some(cwd.to_owned());

    (*s).flags = 0 as ::core::ffi::c_int;
    (*s).lastw.clear();
    (*s).environ = Some(env);
    (*s).options = oo;
    status_update_cache(&mut *(s));
    if let Some(name) = name {
        drop(session_replace_name(&mut *s, name.to_owned()));
        let fresh0 = next_session_id;
        next_session_id = next_session_id.wrapping_add(1);
        (*s).id = fresh0;
    } else {
        loop {
            let fresh1 = next_session_id;
            next_session_id = next_session_id.wrapping_add(1);
            (*s).id = fresh1;
            let mut generated = prefix.map_or_else(Vec::new, |prefix| {
                let mut bytes = prefix.to_bytes().to_vec();
                bytes.push(b'-');
                bytes
            });
            generated.extend_from_slice((*s).id.to_string().as_bytes());
            drop(session_replace_name(
                &mut *s,
                CString::new(generated).expect("generated name has no NUL"),
            ));
            if sessions_find(&sessions, &*s).is_none() {
                break;
            }
        }
    }
    sessions_insert(&mut sessions, Rc::clone(&owner));
    log_debug(format_args!(
        "new session {} ${}",
        log_bytes((*s).name.as_bytes()),
        { (*s).id }
    ));
    (*s).creation_time = SystemTime::now();
    let created = (*s).creation_time;
    session_update_activity(&owner, Some(created));
    owner
}
/// Consume one session owner; the final owner performs cleanup immediately.
unsafe fn session_remove_ref(s: SessionRef, from: &CStr) {
    log_debug(format_args!(
        "release session {} ({})",
        log_bytes((*s.get()).name.as_bytes()),
        log_bytes(from.to_bytes())
    ));
    drop(s);
}
unsafe fn session_free(s: &mut session) {
    log_debug(format_args!(
        "session {} freed",
        log_bytes(s.name.as_bytes())
    ));
    drop(s.environ.take());
    drop(s.options.take());
    crate::src::window::winlink_stack_clear(&mut s.lastw);
}
unsafe fn session_destroy(s_owner: &SessionRef, mut notify: ::core::ffi::c_int, from: &CStr) {
    let s = s_owner.get();

    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    log_debug(format_args!(
        "session {} destroyed ({})",
        log_bytes((*s).name.as_bytes()),
        log_cstr(from)
    ));
    // This field also marks explicit session teardown. An expired observer
    // still needs the normal destruction path if the index owner remains.
    if (*s).curw.is_empty() {
        return;
    }
    (*s).set_curw((refbox::Weak::new()).clone());
    let owner = sessions_remove(&mut sessions, s_owner).expect("registered session owner");
    if notify != 0 {
        events_fire_session(c"session-closed", std::rc::Rc::clone(s_owner));
    }
    (*s).tio = None;
    (*s).tio = None;
    drop((*s).lock_timer.take());
    session_group_remove(s_owner);
    while crate::src::window::winlink_stack_first(&(*s).lastw).is_alive() {
        let first = crate::src::window::winlink_stack_first(&(*s).lastw);
        winlink_stack_remove(&raw mut (*s).lastw, (first).clone());
    }
    crate::src::window::winlink_stack_clear(&mut (*s).lastw);
    while !crate::src::window::winlinks_is_empty(&(*s).windows) {
        wl = winlinks_minmax(&(*s).windows, RB_NEGINF);
        events_fire_winlink(c"window-unlinked", wl.clone());
        winlink_remove(&raw mut (*s).windows, wl.clone());
    }
    session_set_cwd(&mut *s, None);
    session_remove_ref(owner, c"session_destroy");
}
unsafe fn session_lock_timer(owner: &SessionRef) {
    let session = &*owner.get();
    if !owner.is_registered() || session.attached == 0 {
        return;
    }
    log_debug(format_args!(
        "session {} locked, activity time {}",
        log_bytes(session.name.as_bytes()),
        crate::src::shared::time::unix_seconds(session.activity_time) as ::core::ffi::c_longlong
    ));
    server_lock_session(owner);
    recalculate_sizes();
}
unsafe fn session_update_activity(owner: &SessionRef, from: Option<SystemTime>) {
    let session = &mut *owner.get();
    if let Some(from) = from {
        session.activity_time = from;
    } else {
        session.activity_time = SystemTime::now();
    }
    log_debug(format_args!(
        "session ${} {} activity {}.{:06}",
        session.id,
        log_bytes(session.name.as_bytes()),
        crate::src::shared::time::unix_seconds(session.activity_time) as ::core::ffi::c_longlong,
        session
            .activity_time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_micros() as ::core::ffi::c_int
    ));
    drop(session.lock_timer.take());
    if session.attached != 0 {
        let timeout = Duration::from_secs(
            (crate::src::options::options_get_number_ref(
                session.options.as_deref().expect("session options"),
                c"lock-after-time",
            )) as u64,
        );
        if timeout.as_secs() != 0 {
            let observer = Rc::downgrade(owner);
            session.lock_timer = Some(
                Timer::new(timeout, move || unsafe {
                    if let Some(owner) = observer.upgrade() {
                        session_lock_timer(&owner);
                    }
                })
                .expect("arm timer"),
            );
        }
    }
}
unsafe fn session_next_session(
    s: Option<&SessionRef>,
    sort_crit: &sort_criteria,
) -> Option<SessionRef> {
    session_adjacent(s, sort_crit, false)
}
unsafe fn session_previous_session(
    s: Option<&SessionRef>,
    sort_crit: &sort_criteria,
) -> Option<SessionRef> {
    session_adjacent(s, sort_crit, true)
}
unsafe fn session_adjacent(
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
unsafe fn session_attach(
    s_owner: &SessionRef,
    window_owner: &WindowRef,
    mut idx: ::core::ffi::c_int,
) -> Result<refbox::Weak<winlink>, std::ffi::CString> {
    let s = s_owner.get();

    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = winlink_add(&raw mut (*s).windows, idx);
    if !wl.is_alive() {
        return Err(std::ffi::CString::new(format!("index in use: {idx}"))
            .expect("numeric diagnostic contains no NUL"));
    }
    wl.get_mut_unchecked().session = std::rc::Rc::downgrade(s_owner);
    winlink_set_window(wl.clone(), window_owner);
    events_fire_winlink(c"window-linked", wl.clone());
    session_group_synchronize_from(s_owner);
    Ok(wl)
}
unsafe fn session_detach(
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
) -> ::core::ffi::c_int {
    let s = s_owner.get();

    if winlinks_minmax(&(*s).windows, RB_NEGINF) == wl
        && winlinks_minmax(&(*s).windows, RB_INF) == wl
    {
        return 1 as ::core::ffi::c_int;
    }
    if (*s).current_winlink() == wl
        && session_last(s_owner) != 0 as ::core::ffi::c_int
        && session_previous(s_owner, 0 as ::core::ffi::c_int) != 0 as ::core::ffi::c_int
    {
        session_next(s_owner, 0 as ::core::ffi::c_int);
    }
    wl.get_mut_unchecked().flags &= !WINLINK_ALERTFLAGS;
    events_fire_winlink(c"window-unlinked", wl.clone());
    winlink_stack_remove(&raw mut (*s).lastw, wl.clone());
    winlink_remove(&raw mut (*s).windows, wl.clone());
    session_group_synchronize_from(s_owner);
    0 as ::core::ffi::c_int
}

unsafe fn session_is_linked(s: Option<&SessionRef>, w: &WindowRef) -> bool {
    let mut next = w.next_winlink(None);
    let mut n = 0;
    while next.is_alive() {
        n += 1;
        next = w.next_winlink(Some(next));
    }

    let group = crate::src::session::session_groups::for_session(
        &s.map_or_else(std::rc::Weak::new, Rc::downgrade),
    );
    if !group.is_alive() {
        return n != 1;
    }

    n != session_group_count(&group)
}

unsafe fn session_next_alert(mut wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    while wl.is_alive() {
        if wl.get_unchecked().flags & WINLINK_ALERTFLAGS != 0 {
            break;
        }
        wl = winlink_next(wl.clone());
    }
    wl
}
unsafe fn session_next(s_owner: &SessionRef, mut alert: ::core::ffi::c_int) -> ::core::ffi::c_int {
    let s = s_owner.get();

    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if !(*s).current_winlink().is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    wl = winlink_next(((*s).current_winlink()).clone());
    if alert != 0 {
        wl = session_next_alert(wl.clone());
    }
    if !wl.is_alive() {
        wl = winlinks_minmax(&(*s).windows, RB_NEGINF);
        if alert != 0 && {
            wl = session_next_alert(wl.clone());
            !wl.is_alive()
        } {
            return -(1 as ::core::ffi::c_int);
        }
    }
    session_set_current(s_owner, wl.clone())
}
unsafe fn session_previous_alert(mut wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    while wl.is_alive() {
        if wl.get_unchecked().flags & WINLINK_ALERTFLAGS != 0 {
            break;
        }
        wl = winlink_previous(wl.clone());
    }
    wl
}
unsafe fn session_previous(
    s_owner: &SessionRef,
    mut alert: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let s = s_owner.get();

    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if !(*s).current_winlink().is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    wl = winlink_previous(((*s).current_winlink()).clone());
    if alert != 0 {
        wl = session_previous_alert(wl.clone());
    }
    if !wl.is_alive() {
        wl = winlinks_minmax(&(*s).windows, RB_INF);
        if alert != 0 && {
            wl = session_previous_alert(wl.clone());
            !wl.is_alive()
        } {
            return -(1 as ::core::ffi::c_int);
        }
    }
    session_set_current(s_owner, wl.clone())
}
unsafe fn session_select(s_owner: &SessionRef, mut idx: ::core::ffi::c_int) -> ::core::ffi::c_int {
    let s = s_owner.get();

    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    wl = winlink_find_by_index(&(*s).windows, idx);
    session_set_current(s_owner, wl.clone())
}
unsafe fn session_last(s_owner: &SessionRef) -> ::core::ffi::c_int {
    let wl = s_owner.last_winlink();
    if !wl.is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    if wl == s_owner.current_winlink() {
        return 1 as ::core::ffi::c_int;
    }
    s_owner.select_winlink(wl)
}
unsafe fn session_fire_window_changed(
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
        WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_window(
        &mut ep,
        c"new_window",
        WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window")),
    );
    event_payload_set_int(&mut ep, c"window_index", wl.get_unchecked().idx);
    event_payload_set_int(&mut ep, c"new_window_index", wl.get_unchecked().idx);
    if old.is_alive() {
        event_payload_set_window(
            &mut ep,
            c"old_window",
            WindowRef::clone((old.get_unchecked().window_handle().as_ref()).expect("live window")),
        );
        event_payload_set_int(&mut ep, c"old_window_index", old.get_unchecked().idx);
    }
    events_fire(c"session-window-changed", ep);
}
unsafe fn session_set_current(
    s_owner: &SessionRef,
    mut wl: refbox::Weak<winlink>,
) -> ::core::ffi::c_int {
    let s = s_owner.get();

    let mut old: refbox::Weak<winlink> = (*s).current_winlink();
    if !wl.is_alive() {
        return -(1 as ::core::ffi::c_int);
    }
    if wl == (*s).current_winlink() {
        return 1 as ::core::ffi::c_int;
    }
    winlink_stack_remove(&raw mut (*s).lastw, wl.clone());
    winlink_stack_push(&raw mut (*s).lastw, ((*s).current_winlink()).clone());
    (*s).set_curw(wl.clone());
    if options_get_number(global_options, c"focus-events") != 0 {
        if old.is_alive() {
            if let Some(window) = old.get_unchecked().window_handle().cloned() {
                window.update_focus();
            }
        }
        if let Some(window) = wl.get_unchecked().window_handle().cloned() {
            window.update_focus();
        }
    }
    winlink_clear_flags(wl.clone());
    WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window"))
        .update_activity();
    tty_update_window_offset(&WindowRef::clone(
        (wl.get_unchecked().window_handle().as_ref()).expect("live window"),
    ));
    session_fire_window_changed(s_owner, wl.clone(), (old).clone());
    0 as ::core::ffi::c_int
}
/// Rebuild Session-owned associations. Group membership and group traversal are
/// deliberately outside this operation. Each model loan ends before notification.
unsafe fn session_synchronize_windows(source: &SessionRef, destination: &SessionRef) {
    assert!(
        !Rc::ptr_eq(source, destination),
        "synchronized sessions must be distinct"
    );
    if source.with_winlinks(crate::src::window::winlinks_is_empty) {
        return;
    }
    let current = destination.current_winlink();
    if current.is_alive()
        && !source
            .with_winlinks(|links| winlink_find_by_index(links, current.get_unchecked().idx))
            .is_alive()
        && session_last(destination) != 0
        && session_previous(destination, 0) != 0
    {
        session_next(destination, 0);
    }

    // Retain the old index locally. Window cleanup may reenter either Session,
    // so it cannot run through a pointer into the destination's model storage.
    let mut old_windows = std::mem::take(&mut (*destination.get()).windows);
    let mut link = source.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while link.is_alive() {
        let index = link.get_unchecked().idx;
        let mut replacement = {
            let state = &mut *destination.get();
            winlink_add(&mut state.windows, index)
        };
        replacement.get_mut_unchecked().session = Rc::downgrade(destination);
        let window = link
            .get_unchecked()
            .window_handle()
            .expect("source window")
            .clone();
        winlink_set_window(replacement.clone(), &window);
        // Both source and newly published links retain this window now.
        drop(window);
        events_fire_winlink(c"window-linked", replacement.clone());
        // Notification may change alerts or add a later source link. Observe
        // both at the original post-notification point in the live traversal.
        replacement.get_mut_unchecked().flags |= link.get_unchecked().flags & WINLINK_ALERTFLAGS;
        link = winlinks_next(link.get_unchecked());
    }

    let current = destination.current_winlink();
    if current.is_alive() {
        let index = current.get_unchecked().idx;
        let replacement = destination.with_winlinks(|links| winlink_find_by_index(links, index));
        (*destination.get()).curw = replacement;
    } else {
        let current = source.current_winlink();
        if current.is_alive() {
            let index = current.get_unchecked().idx;
            let replacement =
                destination.with_winlinks(|links| winlink_find_by_index(links, index));
            (*destination.get()).curw = replacement;
        }
    }
    if !destination.current_winlink().is_alive() {
        let first = destination.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        (*destination.get()).curw = first;
    }

    let mut old_history = std::mem::take(&mut (*destination.get()).lastw);
    for index in crate::src::window::winlink_stack_indices(&old_history) {
        let replacement = destination.with_winlinks(|links| winlink_find_by_index(links, index));
        if replacement.is_alive() {
            crate::src::window::winlink_stack_append(&mut (*destination.get()).lastw, replacement);
        }
    }
    crate::src::window::winlink_stack_clear(&mut old_history);
    while !crate::src::window::winlinks_is_empty(&old_windows) {
        let old = winlinks_minmax(&old_windows, RB_NEGINF);
        let id = old
            .get_unchecked()
            .window_handle()
            .expect("old window")
            .id();
        let replacement = destination.with_winlinks(|links| winlink_find_by_window_id(links, id));
        if !replacement.is_alive() {
            events_fire_winlink(c"window-unlinked", old.clone());
        }
        winlink_remove(&mut old_windows, old);
    }
}

unsafe fn session_renumber_windows(s_owner: &SessionRef) {
    let s = s_owner.get();

    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wl1: refbox::Weak<winlink> = refbox::Weak::new();
    let mut wl_new: refbox::Weak<winlink> = refbox::Weak::new();
    let mut old_wins: winlinks = std::mem::take(&mut (*s).windows);
    let mut old_lastw: winlink_stack;
    let mut new_idx: ::core::ffi::c_int = 0;
    let mut new_curw_idx: ::core::ffi::c_int = 0;
    let mut marked_idx: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    new_idx = options_get_number(
        options_owner_ptr(&mut (*s).options).map_or(std::ptr::null_mut(), |options| options),
        c"base-index",
    ) as ::core::ffi::c_int;
    new_curw_idx = 0 as ::core::ffi::c_int;
    wl = winlinks_minmax(&old_wins, RB_NEGINF);
    while wl.is_alive() {
        wl_new = winlink_add(&raw mut (*s).windows, new_idx);
        wl_new.get_mut_unchecked().session = std::rc::Rc::downgrade(s_owner);
        winlink_set_window(
            (wl_new).clone(),
            &WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window")),
        );
        wl_new.get_mut_unchecked().flags |= wl.get_unchecked().flags & WINLINK_ALERTFLAGS;
        if wl == marked_pane.winlink_handle() {
            marked_idx = wl_new.get_unchecked().idx;
        }
        if wl == (*s).current_winlink() {
            new_curw_idx = wl_new.get_unchecked().idx;
        }
        wl = winlinks_next(wl.get_unchecked());
        if wl.is_alive() {
            // The final window may use i32::MAX; no next index is needed.
            new_idx += 1;
        }
    }
    old_lastw = std::ptr::replace(&raw mut (*s).lastw, Default::default());
    for old_idx in crate::src::window::winlink_stack_indices(&old_lastw) {
        wl = winlink_find_by_index(&old_wins, old_idx);
        if !wl.is_alive() {
            continue;
        }
        wl.get_mut_unchecked().flags &= !WINLINK_VISITED;
        wl_new = winlink_find_by_window(
            &(*s).windows,
            &WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window")),
        );
        if wl_new.is_alive() {
            crate::src::window::winlink_stack_append(&mut (*s).lastw, (wl_new).clone());
        }
    }
    crate::src::window::winlink_stack_clear(&mut old_lastw);
    if marked_idx != -(1 as ::core::ffi::c_int) {
        marked_pane.set_wl((winlink_find_by_index(&(*s).windows, marked_idx)).clone());
        if !marked_pane.winlink_handle().is_alive() {
            server_clear_marked();
        }
    }
    (*s).set_curw((winlink_find_by_index(&(*s).windows, new_curw_idx)).clone());
    wl = winlinks_minmax(&old_wins, RB_NEGINF);
    while wl.is_alive() && {
        wl1 = winlinks_next(wl.get_unchecked());
        1 as ::core::ffi::c_int != 0
    } {
        winlink_remove(&raw mut old_wins, wl.clone());
        wl = wl1;
    }
}
unsafe fn session_theme_changed(session: Option<&session>) {
    let Some(session) = session else {
        return;
    };
    let mut link = winlinks_minmax(&session.windows, RB_NEGINF);
    while link.is_alive() {
        let wl = link.get_unchecked();
        let mut next = wl.window_handle().and_then(|window| window.next_pane(None));
        while let Some(owner) = next {
            owner.mark_theme_changed();
            next = owner.next_in_window();
        }
        link = winlinks_next(wl);
    }
}
unsafe fn session_update_history(session: &session) {
    let limit = crate::src::options::options_get_number_ref(
        session.options.as_deref().expect("session options"),
        c"history-limit",
    ) as u_int;
    let mut link = winlinks_minmax(&session.windows, RB_NEGINF);
    while link.is_alive() {
        let wl = link.get_unchecked();
        let mut next = wl.window_handle().and_then(|window| window.next_pane(None));
        while let Some(owner) = next {
            owner.update_history_limit(limit);
            next = owner.next_in_window();
        }
        link = winlinks_next(wl);
    }
}

#[cfg(test)]
mod session_index_tests {
    use super::*;

    #[test]
    fn current_winlink_observer_expires_when_index_removes_it() {
        unsafe {
            let owner = session::new();
            let session = &mut *owner.get();
            let link = winlink_add(&raw mut session.windows, 4);
            session.set_curw(link.clone());
            assert_eq!(session.current_winlink(), link);
            winlink_remove(&raw mut session.windows, link.clone());
            assert!(!session.current_winlink().is_alive());
            assert!(!session.curw.is_empty());
            session.set_curw(refbox::Weak::new());
            assert!(session.curw.is_empty());
        }
    }
}

impl Drop for session {
    fn drop(&mut self) {
        unsafe { session_free(self) }
    }
}

#[cfg(test)]
mod sorted_session_owners_tests;

#[cfg(test)]
mod winlink_storage_tests;

#[cfg(test)]
mod session_lastw_queue_tests;

#[cfg(test)]
mod storage_tests;
