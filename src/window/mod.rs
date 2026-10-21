use crate::format::bytes::cstr_suffix;
use crate::format::bytes::nullable_cstr;
use crate::options::OptionsScope;
use crate::server_client::ClientRegistry;
use crate::session::Session;
use std::collections::BTreeMap;
use std::time::SystemTime;
mod alerts;
mod api;
pub use api::Window;

mod layout;
mod model;
pub use crate::window_pane::*;
pub use crate::winlink::*;
pub use layout::{LayoutAction, LayoutKind};
pub use model::window;
mod owner;
pub use owner::{WindowRef, WindowWeak};

use crate::alerts::alerts_queue;
use crate::compat::strtonum::strtonum;
use crate::events::{events_fire, events_fire_window};
use crate::events_payload::{
    event_payload_create, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_uint, event_payload_set_window,
};
use crate::format::bytes::write_cstr;
use crate::log::log_debug;
use crate::menu::{menu_destroy, menu_resize};
use crate::options::options_get_number;

use crate::server::{marked_pane, server_check_marked, server_clear_marked};
use crate::server_client::Client;
use crate::server_fn::server_redraw_window;
use crate::tmux::clean_name_cstring;
use crate::tty::tty_update_window_offset;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

use crate::options::options;
use crate::shared::abi::*;
use crate::shared::command::cmd_find_state;
use crate::shared::event::*;
use crate::shared::grid::*;
use crate::shared::layout::layout_geometry;
use crate::shared::layout::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::pane::{window_panes, PaneHistory};
use crate::shared::pane::{PANE_STATUS_BOTTOM, PANE_STATUS_TOP};
pub use crate::shared::window::{windows, winlink};
use crate::shared::window::{WINDOW_ACTIVITY, WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE};

pub const DEFAULT_XPIXEL: ::core::ffi::c_int = 16 as ::core::ffi::c_int;
pub const DEFAULT_YPIXEL: ::core::ffi::c_int = 32 as ::core::ffi::c_int;
thread_local! {
    static WINDOWS: std::cell::RefCell<windows> = const {
        std::cell::RefCell::new(windows {
            storage: BTreeMap::new(),
        })
    };
    static NEXT_WINDOW_ID: std::cell::Cell<u_int> = const { std::cell::Cell::new(0) };
}

/// The window index only observes: entries are weak and windows are owned
/// elsewhere. Each call borrows the index for its own duration and returns
/// owned handles, so callbacks may reenter it between calls.
impl windows {
    /// Register an observer, without adding a strong reference to the window.
    /// An existing live entry for the same id wins and is returned.
    pub fn insert(window: &WindowRef) -> Option<WindowRef> {
        let id = unsafe { (*window.as_ptr()).id };
        WINDOWS.with_borrow_mut(|index| {
            if let Some(existing) = index.storage.get(&id).and_then(WindowWeak::upgrade) {
                return Some(existing);
            }
            index.storage.insert(id, WindowRef::downgrade(window));
            None
        })
    }

    /// Remove the matching observer without retaining or releasing the window.
    pub fn remove(window: &WindowRef) {
        let id = unsafe { (*window.as_ptr()).id };
        WINDOWS.with_borrow_mut(|index| {
            if index
                .storage
                .get(&id)
                .is_some_and(|weak| weak.ptr_eq(&WindowRef::downgrade(window)))
            {
                index.storage.remove(&id)
            } else {
                None
            }
        });
    }

    pub fn first() -> Option<WindowRef> {
        WINDOWS.with_borrow(|index| index.storage.values().find_map(WindowWeak::upgrade))
    }

    /// The live window with the next greater id; `after` need not be indexed.
    pub fn next(after: &window) -> WindowWeak {
        WINDOWS.with_borrow(|index| {
            index
                .storage
                .range((
                    std::ops::Bound::Excluded(&after.id),
                    std::ops::Bound::Unbounded,
                ))
                .map(|(_, weak)| weak)
                .find(|weak| weak.strong_count() != 0)
                .cloned()
                .unwrap_or_default()
        })
    }

    pub fn resolve(id: u32) -> Option<WindowRef> {
        WINDOWS.with_borrow(|index| index.storage.get(&id)?.upgrade())
    }

    fn alloc_id() -> u_int {
        NEXT_WINDOW_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        })
    }
}

/// Return the first winlink in a window's association order.
fn window_winlinks_first(w: &window) -> refbox::Weak<winlink> {
    w.winlinks
        .first()
        .filter(|link| link.is_alive())
        .map_or(refbox::Weak::new(), |link| link.clone())
}

/// Return the next winlink in `w` after `wl`, or an empty handle when it is no longer in
/// that window. Taking the owner explicitly keeps iteration correct if a
/// callback moves `wl` to another window while the original list is traversed.
fn window_winlinks_next(w: &window, wl: refbox::Weak<winlink>) -> refbox::Weak<winlink> {
    if !wl.is_alive() {
        return refbox::Weak::new();
    }
    let links = &w.winlinks;
    links
        .iter()
        .position(|link| *link == wl)
        .and_then(|position| links.get(position + 1))
        .filter(|link| link.is_alive())
        .map_or(refbox::Weak::new(), |link| link.clone())
}

/// Append a non-owning winlink handle to the window's association order.
fn window_winlinks_append(w: &mut window, wl: refbox::Weak<winlink>) {
    assert!(wl.is_alive());
    let links = &mut w.winlinks;
    assert!(
        !links.contains(&wl),
        "winlink is already present in this window"
    );
    links.push(wl);
}

/// Remove a non-owning winlink handle from its window's association order.
fn window_winlinks_remove(w: &mut window, wl: refbox::Weak<winlink>) {
    assert!(wl.is_alive());
    let links = &mut w.winlinks;
    let position = links
        .iter()
        .position(|link| *link == wl)
        .expect("winlink must belong to its window");
    links.remove(position);
}

unsafe fn window_fire_renamed(w_owner: &WindowRef, old_name: &CStr) {
    let mut w = w_owner.as_ptr();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        w: WindowWeak::new(),
        wp: std::rc::Weak::new(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_window(w_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_window(&mut ep, c"window", WindowRef::clone(w_owner));
    event_payload_set_string(&mut ep, c"old_name", |out| {
        write_cstr(out, nullable_cstr(old_name.as_ptr()))
    });
    event_payload_set_string(&mut ep, c"new_name", |out| write_cstr(out, &*((*w).name)));
    events_fire(c"window-renamed", ep);
}
unsafe fn window_fire_pane_changed(
    window: &WindowRef,
    pane: &Rc<RefCell<window_pane>>,
    previous: Option<&Rc<RefCell<window_pane>>>,
) {
    let mut find = cmd_find_state::default();
    let mut payload = event_payload_create();
    find =
        cmd_find_state::find_from_pane(pane, 0).unwrap_or_else(|| cmd_find_state::clear_state(0));
    event_payload_set_target(&mut payload, &find);
    event_payload_set_window(&mut payload, c"window", window.clone());
    event_payload_set_pane(&mut payload, c"pane", pane.clone());
    event_payload_set_pane(&mut payload, c"new_pane", pane.clone());
    if let Some(previous) = previous {
        event_payload_set_pane(&mut payload, c"old_pane", previous.clone());
    }
    events_fire(c"window-pane-changed", payload);
}

pub fn window_find_by_id_str(s: &CStr) -> Option<WindowRef> {
    let mut id: u_int = 0;
    if s.to_bytes().first() != Some(&b'@') {
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
    windows::resolve(id)
}
unsafe fn window_update_activity(w_owner: &WindowRef) {
    let mut w = w_owner.as_ptr();
    (*w).activity_time = SystemTime::now();
    alerts_queue(w_owner, WINDOW_ACTIVITY);
}

/// Replace the window's owned name, returning the previous value for callers
/// that need to keep it alive across synchronous callbacks.
unsafe fn window_replace_name(w_owner: &WindowRef, name: CString) -> CString {
    let mut w = w_owner.as_ptr();
    ::core::mem::replace(&mut (*w).name, name)
}
/// Return one owned reference; callers must release it after linking the window.
pub unsafe fn window_create(
    mut sx: u_int,
    mut sy: u_int,
    mut xpixel: u_int,
    mut ypixel: u_int,
) -> WindowRef {
    if xpixel == 0 as u_int {
        xpixel = DEFAULT_XPIXEL as u_int;
    }
    if ypixel == 0 as u_int {
        ypixel = DEFAULT_YPIXEL as u_int;
    }
    let owner = window::new_ref();
    let w = owner.as_ptr();
    (*w).flags = 0 as ::core::ffi::c_int;
    (*w).panes = window_panes::default();
    (*w).last_panes = PaneHistory::default();
    (*w).set_active(None);
    (*w).sx = sx;
    (*w).sy = sy;
    (*w).manual_sx = sx;
    (*w).manual_sy = sy;
    (*w).xpixel = xpixel;
    (*w).ypixel = ypixel;
    (*w).options = Some(crate::options::options_create(Some(
        crate::options::OptionsScope::GlobalWindow,
    )));
    (*w).sb =
        options_get_number(&owner.borrow_options_mut(), c"pane-scrollbars") as ::core::ffi::c_int;
    (*w).sb_pos = options_get_number(&owner.borrow_options_mut(), c"pane-scrollbars-position")
        as ::core::ffi::c_int;
    (*w).winlinks = Default::default();
    (*w).id = windows::alloc_id();
    windows::insert(&owner);
    (*w).creation_time = SystemTime::now();
    window_update_activity(&owner);
    log_debug(format_args!(
        "{}: @{} create {}x{} ({}x{})",
        "window_create",
        { (*w).id },
        { sx },
        { sy },
        { (*w).xpixel },
        { (*w).ypixel }
    ));
    owner
}

unsafe fn window_destroy(w_owner: &WindowRef) {
    let mut w = w_owner.as_ptr();
    log_debug(format_args!("window @{} destroyed", { (*w).id }));
    windows::remove(w_owner);
    menu_destroy((*w).menu.take());
    window_destroy_panes(w_owner);
    drop((*w).options.take());
}

unsafe fn window_set_name(w_owner: &WindowRef, new_name: &CStr, mut untrusted: ::core::ffi::c_int) {
    let _w = w_owner.as_ptr();
    if let Some(name) = clean_name_cstring(new_name, untrusted) {
        let last = window_replace_name(w_owner, name);
        window_fire_renamed(w_owner, &last);
    }
}
unsafe fn window_resize(
    w_owner: &WindowRef,
    mut sx: u_int,
    mut sy: u_int,
    mut xpixel: ::core::ffi::c_int,
    mut ypixel: ::core::ffi::c_int,
) {
    let mut w = w_owner.as_ptr();
    if xpixel == 0 as ::core::ffi::c_int {
        xpixel = DEFAULT_XPIXEL;
    }
    if ypixel == 0 as ::core::ffi::c_int {
        ypixel = DEFAULT_YPIXEL;
    }
    log_debug(format_args!(
        "{}: @{} resize {}x{} ({}x{})",
        "window_resize",
        { (*w).id },
        { sx },
        { sy },
        {
            if xpixel == -(1 as ::core::ffi::c_int) {
                (*w).xpixel
            } else {
                xpixel as u_int
            }
        },
        {
            if ypixel == -(1 as ::core::ffi::c_int) {
                (*w).ypixel
            } else {
                ypixel as u_int
            }
        }
    ));
    (*w).sx = sx;
    (*w).sy = sy;
    window_arrange(w_owner);
    if let Some(menu) = (*w).menu.as_ref().map(|menu| menu.downgrade()) {
        let (menu_sx, menu_sy) = w_owner.logical_size();
        menu_resize(
            &mut menu.try_borrow_mut().expect("live unborrowed menu"),
            menu_sx,
            menu_sy,
        );
        server_redraw_window(w_owner);
    }
    if xpixel != -(1 as ::core::ffi::c_int) {
        (*w).xpixel = xpixel as u_int;
    }
    if ypixel != -(1 as ::core::ffi::c_int) {
        (*w).ypixel = ypixel as u_int;
    }
    (*w).invalidate_scene();
}

fn window_has_pane(w: &window, pane: &std::rc::Weak<std::cell::RefCell<window_pane>>) -> bool {
    pane.strong_count() != 0 && w.panes.position(pane).is_some()
}

/// Whether `pane` is shown: every pane is, except while the window is zoomed,
/// when only the zoomed pane is.
fn window_pane_is_visible(w: &window, pane: &Weak<RefCell<window_pane>>) -> bool {
    w.zoomed.strong_count() == 0 || w.zoomed.ptr_eq(pane)
}

/// Zoom `pane`: it becomes the active pane and takes the whole window, and
/// the other panes are hidden in their places. Refused, returning false,
/// when the window is already zoomed or has a single pane.
unsafe fn window_zoom(window: &WindowRef, pane: &Rc<RefCell<window_pane>>) -> bool {
    let w = window.as_ptr();
    if (*w).zoomed.strong_count() != 0 || (*w).panes.storage.len() == 1 {
        return false;
    }
    if !(*w).active.ptr_eq(&Rc::downgrade(pane)) {
        window_set_active_pane(window, pane, 1);
    }
    (*w).zoomed = Rc::downgrade(pane);
    (*w).invalidate_scene();
    window_arrange(window);
    events_fire_window(c"window-zoomed", window.clone());
    events_fire_window(c"window-layout-changed", window.clone());
    true
}

/// Show every pane in its place again. Returns false when the window was not
/// zoomed.
unsafe fn window_unzoom(window: &WindowRef, notify: bool) -> bool {
    let w = window.as_ptr();
    if (*w).zoomed.strong_count() == 0 {
        return false;
    }
    (*w).zoomed = Weak::new();
    (*w).invalidate_scene();
    window_arrange(window);
    if notify {
        events_fire_window(c"window-unzoomed", window.clone());
        events_fire_window(c"window-layout-changed", window.clone());
    }
    true
}

/// Unzoom for an operation that rearranges or selects panes, remembering to
/// zoom again afterwards when `flag` asks and the window was zoomed, or
/// whether or not it was with `always`. Returns whether the window unzoomed.
unsafe fn window_push_zoom(window: &WindowRef, always: bool, flag: bool) -> bool {
    let w = window.as_ptr();
    (*w).was_zoomed = flag && (always || (*w).zoomed.strong_count() != 0);
    window_unzoom(window, true)
}

/// After `window_push_zoom`, zoom the active pane when it said to. Returns
/// whether the window zoomed.
unsafe fn window_pop_zoom(window: &WindowRef) -> bool {
    let w = window.as_ptr();
    if !std::mem::take(&mut (*w).was_zoomed) {
        return false;
    }
    (*w).active_pane()
        .is_some_and(|active| window_zoom(window, &active))
}

unsafe fn window_update_focus(w_owner: Option<&WindowRef>) {
    if let Some(window) = w_owner {
        if let Some(pane) = window.active_pane().upgrade() {
            pane.update_focus(window.pane_is_focused(&pane));
        }
    }
}

unsafe fn window_set_active_pane(
    window: &WindowRef,
    pane: &Rc<RefCell<window_pane>>,
    notify: i32,
) -> i32 {
    let w = window.as_ptr();
    let observer = Rc::downgrade(pane);
    log_debug(format_args!("window_set_active_pane: pane %{}", pane.id()));
    if (*w).active.ptr_eq(&observer) {
        return 0;
    }
    if !window_pane_is_visible(&*w, &observer) {
        window_unzoom(window, true);
    }
    let previous = window.active_pane().upgrade();
    (*w).last_panes.remove(&observer);
    if let Some(previous) = previous.as_ref() {
        (*w).last_panes.push(Rc::downgrade(previous));
    }
    (*w).active = observer;
    let mut client = ClientRegistry::first();
    while let Some(current) = client {
        current.reset_pan(Some(window));
        client = ClientRegistry::next(&current);
    }
    pane.on_selected(true);
    if OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"focus-events")) != 0 {
        if let Some(previous) = previous.as_ref() {
            previous.update_focus(false);
        }
        window_update_focus(Some(window));
    }
    tty_update_window_offset(window);
    server_redraw_window(window);
    if notify != 0 {
        if let Some(active) = window.active_pane().upgrade() {
            window_fire_pane_changed(window, &active, previous.as_ref());
        }
    }
    1
}
unsafe fn window_redraw_active_switch(
    window: &WindowRef,
    previous: Option<&Rc<RefCell<window_pane>>>,
) {
    let state = window.as_ptr();
    let active = (*state).active_pane();
    if previous.is_some_and(|pane| {
        active
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(pane, active))
    }) {
        return;
    }
    if let Some(previous) = previous {
        previous.redraw_selection_change();
    }
    if let Some(active) = active {
        active.redraw_selection_change();
    }
}
unsafe fn window_get_active_at(
    window_owner: &WindowRef,
    mut x: u_int,
    mut y: u_int,
) -> Weak<std::cell::RefCell<window_pane>> {
    let w = window_owner.as_ptr();
    let mut xoff: ::core::ffi::c_int = 0;
    let mut yoff: ::core::ffi::c_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let status = window_get_pane_status(window_owner);
    for observer in (*w).panes.storage.clone() {
        if !window_pane_is_visible(&*w, &observer) {
            continue;
        }
        let candidate = observer.upgrade().expect("live pane in ordering");
        (xoff, yoff, sx, sy) = candidate.outer_geometry();
        if (x as ::core::ffi::c_int) < xoff || x > (xoff as u_int).wrapping_add(sx) {
            continue;
        }
        let top = yoff - i32::from(status == PANE_STATUS_TOP);
        let bottom = (yoff as u_int)
            .wrapping_add(sy)
            .wrapping_add(u_int::from(status == PANE_STATUS_BOTTOM));
        if (y as ::core::ffi::c_int) < top || y >= bottom {
            continue;
        }
        return observer;
    }
    Weak::new()
}
unsafe fn window_find_string(
    window_owner: &WindowRef,
    name: &CStr,
) -> Weak<std::cell::RefCell<window_pane>> {
    let w = window_owner.as_ptr();
    let wsx = if (*w).zoomed.strong_count() != 0 {
        (*w).sx
    } else {
        layout::content_width(&window_arrangement(window_owner))
    };
    let wsy = (*w).sy;
    let s = name.to_bytes();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut top: u_int = 0 as u_int;
    let mut bottom: u_int = wsy.wrapping_sub(1 as u_int);
    x = wsx.wrapping_div(2 as u_int);
    y = wsy.wrapping_div(2 as u_int);
    let status = window_get_pane_status(window_owner);
    if status == PANE_STATUS_TOP {
        top = top.wrapping_add(1);
    } else if status == PANE_STATUS_BOTTOM {
        bottom = bottom.wrapping_sub(1);
    }
    if s.eq_ignore_ascii_case(b"top") {
        y = top;
    } else if s.eq_ignore_ascii_case(b"bottom") {
        y = bottom;
    } else if s.eq_ignore_ascii_case(b"left") {
        x = 0 as u_int;
    } else if s.eq_ignore_ascii_case(b"right") {
        x = wsx.wrapping_sub(1 as u_int);
    } else if s.eq_ignore_ascii_case(b"top-left") {
        x = 0 as u_int;
        y = top;
    } else if s.eq_ignore_ascii_case(b"top-right") {
        x = wsx.wrapping_sub(1 as u_int);
        y = top;
    } else if s.eq_ignore_ascii_case(b"bottom-left") {
        x = 0 as u_int;
        y = bottom;
    } else if s.eq_ignore_ascii_case(b"bottom-right") {
        x = wsx.wrapping_sub(1 as u_int);
        y = bottom;
    } else {
        return Weak::new();
    }
    window_get_active_at(window_owner, x, y)
}
/// Make `panes` the window's pane list, in this order: the one way panes are
/// added, removed and reordered. A list that brings in a pane must be one the
/// layout takes; otherwise the layout's reason comes back and nothing has
/// changed. A removal or a reorder is never refused. Panes arriving must
/// already be reparented to this window; panes leaving give up their place in
/// the selection history, and an active pane that leaves hands over to the
/// pane taking its place, or else to the last pane selected or its
/// neighbour. A window without panes takes its first one as active, without
/// selection callbacks. Any change to the list unzooms the window first.
/// window-layout-changed fires when panes leave; a pane arriving is announced
/// by its caller, once its process has started or its move has finished.
unsafe fn window_rearrange_panes(
    window: &WindowRef,
    panes: &[Rc<RefCell<window_pane>>],
) -> Result<(), &'static CStr> {
    let w = window.as_ptr();
    let old = (*w).panes.snapshot();
    let present = |list: &[Rc<RefCell<window_pane>>], pane: &Rc<RefCell<window_pane>>| {
        list.iter().any(|candidate| Rc::ptr_eq(candidate, pane))
    };
    let arrived = panes
        .iter()
        .filter(|pane| !present(&old, pane))
        .collect::<Vec<_>>();
    let left = old
        .iter()
        .filter(|pane| !present(panes, pane))
        .collect::<Vec<_>>();
    for pane in &arrived {
        assert!(
            pane.window_observer().ptr_eq(&WindowRef::downgrade(window)),
            "an arriving pane is reparented to the window"
        );
    }
    if !arrived.is_empty() {
        (*w).layout.admits(panes, window.size())?;
    }
    if old.len() != panes.len() || old.iter().zip(panes).any(|(a, b)| !Rc::ptr_eq(a, b)) {
        window_unzoom(window, true);
    }
    for pane in &left {
        log_debug(format_args!(
            "window_rearrange_panes: @{} loses %{}",
            window.id(),
            pane.id()
        ));
        if marked_pane
            .pane_handle()
            .is_some_and(|marked| Rc::ptr_eq(&marked, pane))
        {
            server_clear_marked();
        }
        (*w).last_panes.remove(&Rc::downgrade(pane));
    }
    let previous_active = (*w).active_pane();
    let replacement = match previous_active.as_ref() {
        Some(active) if !present(panes, active) => {
            let slot = old
                .iter()
                .position(|pane| Rc::ptr_eq(pane, active))
                .expect("active pane is in the list");
            panes
                .get(slot)
                .filter(|pane| present(&arrived.iter().cloned().cloned().collect::<Vec<_>>(), pane))
                .cloned()
                .or_else(|| (*w).last_panes.first().upgrade())
                .or_else(|| {
                    old[..slot]
                        .iter()
                        .rev()
                        .chain(&old[slot + 1..])
                        .find(|pane| present(panes, pane))
                        .cloned()
                })
        }
        Some(_) => None,
        None => None,
    };
    (*w).panes.storage = panes.iter().map(Rc::downgrade).collect();
    if previous_active.is_none() {
        (*w).active = panes.first().map_or_else(Weak::new, Rc::downgrade);
        if let Some(first) = panes.first() {
            first.on_selected(true);
        }
    } else if previous_active
        .as_ref()
        .is_some_and(|active| !present(panes, active))
    {
        (*w).active = replacement.as_ref().map_or_else(Weak::new, Rc::downgrade);
        if let Some(replacement) = replacement.as_ref() {
            (*w).last_panes.remove(&Rc::downgrade(replacement));
            replacement.on_selected(false);
        }
    }
    (*w).invalidate_scene();
    window_arrange(window);
    if !left.is_empty() {
        events_fire_window(c"window-layout-changed", window.clone());
    }
    if let Some(replacement) = replacement {
        window_fire_pane_changed(window, &replacement, previous_active.as_ref());
        window_update_focus(Some(window));
    }
    Ok(())
}

/// Take `pane` out of its window and destroy it.
unsafe fn window_remove_pane(window: &WindowRef, pane: &Rc<RefCell<window_pane>>) {
    let rest = (*window.as_ptr())
        .panes
        .snapshot()
        .into_iter()
        .filter(|candidate| !Rc::ptr_eq(candidate, pane))
        .collect::<Vec<_>>();
    window_rearrange_panes(window, &rest).expect("a removal is never refused");
    pane.destroy();
}

/// The layout's rectangles for the current panes, in pane order.
unsafe fn window_arrangement(window: &WindowRef) -> Vec<layout_geometry> {
    let size = window.size();
    let w = &*window.as_ptr();
    w.layout.arrange(&w.panes.snapshot(), size)
}

/// The rectangles for the current panes, in pane order.
unsafe fn window_pane_cells(
    window: &WindowRef,
) -> Vec<(Weak<RefCell<window_pane>>, layout_geometry)> {
    let panes = (*window.as_ptr()).panes.storage.clone();
    panes.into_iter().zip(window_arrangement(window)).collect()
}

/// The arrange step: the only writer of pane geometry. Every operation that
/// changes the pane order, the layout or its record of a pane, the zoom, the
/// window size or the options the layout reads ends here, and the clients
/// showing the window follow the result. A zoomed pane takes the whole
/// window; the hidden panes keep their places in the layout, and the size
/// they had, until the window unzooms.
unsafe fn window_arrange(window: &WindowRef) {
    let (sx, sy) = window.size();
    let zoomed = (*window.as_ptr()).zoomed.clone();
    let cells = (*window.as_ptr())
        .panes
        .snapshot()
        .into_iter()
        .zip(window_arrangement(window));
    let mut moved = false;
    for (pane, cell) in cells {
        let cell = if zoomed.strong_count() == 0 {
            cell
        } else if zoomed.ptr_eq(&Rc::downgrade(&pane)) {
            layout_geometry {
                sx,
                sy,
                xoff: 0,
                yoff: 0,
            }
        } else {
            continue;
        };
        if window_has_pane(&*window.as_ptr(), &Rc::downgrade(&pane)) {
            moved |= pane.apply_layout(cell, window);
        }
    }
    if moved {
        (*window.as_ptr()).invalidate_scene();
    }
    tty_update_window_offset(window);
}

unsafe fn window_destroy_panes(window: &WindowRef) {
    let w = window.as_ptr();
    while let Some(pane) = window_pane_first(w.as_ref()) {
        (*w).panes.remove(&Rc::downgrade(&pane));
        pane.destroy();
    }
}
pub unsafe fn window_printable_flags(
    mut wl: refbox::Weak<winlink>,
    escape: bool,
) -> std::ffi::CString {
    let session_owner = wl.get_unchecked().session.upgrade();
    let mut flags = Vec::new();
    if wl.get_unchecked().flags & WINLINK_ACTIVITY != 0 {
        flags.push(b'#');
        if escape {
            flags.push(b'#');
        }
    }
    if wl.get_unchecked().flags & WINLINK_BELL != 0 {
        flags.push(b'!');
    }
    if wl.get_unchecked().flags & WINLINK_SILENCE != 0 {
        flags.push(b'~');
    }
    if session_owner
        .as_ref()
        .is_some_and(|owner| wl == owner.current_winlink())
    {
        flags.push(b'*');
    }
    if session_owner
        .as_ref()
        .is_some_and(|owner| wl == owner.last_winlink())
    {
        flags.push(b'-');
    }
    if server_check_marked() != 0 && wl == marked_pane.winlink_handle() {
        flags.push(b'M');
    }
    if wl
        .get_unchecked()
        .window_handle()
        .is_some_and(|window| window.zoomed_pane().strong_count() != 0)
    {
        flags.push(b'Z');
    }
    std::ffi::CString::new(flags).expect("window flags contain no NUL")
}

fn window_pane_first(w: Option<&window>) -> Option<Rc<std::cell::RefCell<window_pane>>> {
    Some(w?.panes.first()?.upgrade().expect("live pane in ordering"))
}

fn window_get_pane_status(w: &WindowRef) -> ::core::ffi::c_int {
    options_get_number(&w.borrow_options_mut(), c"pane-border-status") as ::core::ffi::c_int
}

fn window_get_pane_lines(w: &WindowRef) -> pane_lines {
    options_get_number(&w.borrow_options_mut(), c"pane-border-lines") as pane_lines
}
