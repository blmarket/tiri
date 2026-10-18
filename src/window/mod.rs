use crate::src::format::bytes::nullable_cstr;
use crate::src::server_client::ClientRegistry;
use crate::src::session::Session;
use std::collections::BTreeMap;
use std::time::SystemTime;
mod alerts;
mod api;
pub use api::Window;

mod layout;
mod model;
pub use crate::src::window_pane::*;
pub use crate::src::winlink::*;
pub use layout::{LayoutAction, LayoutKind};
pub use model::window;
mod owner;
pub use owner::{WindowRef, WindowWeak};

use crate::src::alerts::alerts_queue;
use crate::src::compat::strtonum::strtonum;
use crate::src::events::{events_fire, events_fire_window};
use crate::src::events_payload::{
    event_payload_create, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_uint, event_payload_set_window,
};
use crate::src::format::bytes::write_cstr;
use crate::src::log::log_debug;
use crate::src::menu::{menu_destroy, menu_resize};
use crate::src::options::options_get_number;
use libc::strcasecmp;

use crate::src::server::{marked_pane, server_check_marked, server_clear_marked};
use crate::src::server_client::Client;
use crate::src::server_fn::server_redraw_window;
use crate::src::tmux::{clean_name_cstring, global_options};
use crate::src::tty::tty_update_window_offset;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

use crate::src::options::options;
use crate::src::shared::abi::*;
use crate::src::shared::command::cmd_find_state;
use crate::src::shared::event::*;
use crate::src::shared::grid::*;
use crate::src::shared::layout::layout_geometry;
use crate::src::shared::layout::*;
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::pane::{window_panes, PaneHistory};
use crate::src::shared::pane::{PANE_STATUS_BOTTOM, PANE_STATUS_TOP};
pub use crate::src::shared::window::{windows, winlink};
use crate::src::shared::window::{
    WINDOW_ACTIVITY, WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE,
};

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
        let id = unsafe { (*window.get()).id };
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
        let id = unsafe { (*window.get()).id };
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
unsafe fn window_winlinks_first(w_value: Option<&window>) -> refbox::Weak<winlink> {
    let w: *mut window = w_value.map_or(std::ptr::null_mut(), |value| value as *const _ as *mut _);
    if w.is_null() {
        return refbox::Weak::new();
    }
    (*w).winlinks
        .first()
        .filter(|link| link.is_alive())
        .map_or(refbox::Weak::new(), |link| link.clone())
}

/// Return the next winlink in `w` after `wl`, or an empty handle when it is no longer in
/// that window. Taking the owner explicitly keeps iteration correct if a
/// callback moves `wl` to another window while the original list is traversed.
unsafe fn window_winlinks_next(
    w_value: Option<&window>,
    wl: refbox::Weak<winlink>,
) -> refbox::Weak<winlink> {
    let w: *mut window = w_value.map_or(std::ptr::null_mut(), |value| value as *const _ as *mut _);
    if w.is_null() || !wl.is_alive() {
        return refbox::Weak::new();
    }
    let links = &(*w).winlinks;
    links
        .iter()
        .position(|link| *link == wl)
        .and_then(|position| links.get(position + 1))
        .filter(|link| link.is_alive())
        .map_or(refbox::Weak::new(), |link| link.clone())
}

/// Append a non-owning winlink handle to the window's association order.
unsafe fn window_winlinks_append(w_value: &mut window, wl: refbox::Weak<winlink>) {
    let w: *mut window = w_value as *mut _;
    assert!(!w.is_null() && wl.is_alive());
    let links = &mut (*w).winlinks;
    assert!(
        !links.contains(&wl),
        "winlink is already present in this window"
    );
    links.push(wl);
}

/// Remove a non-owning winlink handle from its window's association order.
unsafe fn window_winlinks_remove(w_value: &mut window, wl: refbox::Weak<winlink>) {
    let w: *mut window = w_value as *mut _;
    assert!(!w.is_null() && wl.is_alive());
    let links = &mut (*w).winlinks;
    let position = links
        .iter()
        .position(|link| *link == wl)
        .expect("winlink must belong to its window");
    links.remove(position);
}

unsafe fn window_fire_renamed(w_owner: &WindowRef, old_name: &CStr) {
    let mut w = w_owner.get();
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
    pane: &Rc<UnsafeCell<window_pane>>,
    previous: Option<&Rc<UnsafeCell<window_pane>>>,
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

pub unsafe fn window_find_by_id_str(s: &CStr) -> Option<WindowRef> {
    let s: *const ::core::ffi::c_char = s.as_ptr();
    let mut id: u_int = 0;
    if *s as ::core::ffi::c_int != '@' as i32 {
        return None;
    }
    let parsed = strtonum(
        CStr::from_ptr(s.offset(1 as ::core::ffi::c_int as isize)),
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
    let mut w = w_owner.get();
    (*w).activity_time = SystemTime::now();
    alerts_queue(w_owner, WINDOW_ACTIVITY);
}

/// Replace the window's owned name, returning the previous value for callers
/// that need to keep it alive across synchronous callbacks.
unsafe fn window_replace_name(w_owner: &WindowRef, name: CString) -> CString {
    let mut w = w_owner.get();
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
    let w = owner.get();
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
    (*w).options
        .replace(Some(crate::src::options::options_create_owned(Some(
            crate::src::options::OptionsScope::GlobalWindow,
        ))));
    (*w).sb =
        options_get_number(&(*w).borrow_options_mut(), c"pane-scrollbars") as ::core::ffi::c_int;
    (*w).sb_pos = options_get_number(&(*w).borrow_options_mut(), c"pane-scrollbars-position")
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
    let mut w = w_owner.get();
    log_debug(format_args!("window @{} destroyed", { (*w).id }));
    windows::remove(w_owner);
    menu_destroy((*w).menu.take());
    window_destroy_panes(w_owner);
    drop((*w).options.take());
}

unsafe fn window_set_name(w_owner: &WindowRef, new_name: &CStr, mut untrusted: ::core::ffi::c_int) {
    let _w = w_owner.get();
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
    let mut w = w_owner.get();
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

fn window_has_pane(w: &window, pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>) -> bool {
    pane.strong_count() != 0 && w.panes.position(pane).is_some()
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
    pane: &Rc<UnsafeCell<window_pane>>,
    notify: i32,
) -> i32 {
    let w = window.get();
    let observer = Rc::downgrade(pane);
    log_debug(format_args!("window_set_active_pane: pane %{}", pane.id()));
    if (*w).active.ptr_eq(&observer) {
        return 0;
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
    if options_get_number(&global_options(), c"focus-events") != 0 {
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
    previous: Option<&Rc<UnsafeCell<window_pane>>>,
) {
    let state = window.get();
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
) -> Weak<std::cell::UnsafeCell<window_pane>> {
    let w = window_owner.get();
    let mut xoff: ::core::ffi::c_int = 0;
    let mut yoff: ::core::ffi::c_int = 0;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let status = window_get_pane_status(&*w);
    for observer in (*w).panes.storage.clone() {
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
) -> Weak<std::cell::UnsafeCell<window_pane>> {
    let w = window_owner.get();
    let (wsx, wsy) = (
        layout::content_width(&window_arrangement(window_owner)),
        (*w).sy,
    );
    let s = name.as_ptr();
    let mut x: u_int = 0;
    let mut y: u_int = 0;
    let mut top: u_int = 0 as u_int;
    let mut bottom: u_int = wsy.wrapping_sub(1 as u_int);
    x = wsx.wrapping_div(2 as u_int);
    y = wsy.wrapping_div(2 as u_int);
    let status = window_get_pane_status(&*w);
    if status == PANE_STATUS_TOP {
        top = top.wrapping_add(1);
    } else if status == PANE_STATUS_BOTTOM {
        bottom = bottom.wrapping_sub(1);
    }
    if strcasecmp(s, c"top".as_ptr()) == 0 as ::core::ffi::c_int {
        y = top;
    } else if strcasecmp(s, c"bottom".as_ptr()) == 0 as ::core::ffi::c_int {
        y = bottom;
    } else if strcasecmp(s, c"left".as_ptr()) == 0 as ::core::ffi::c_int {
        x = 0 as u_int;
    } else if strcasecmp(s, c"right".as_ptr()) == 0 as ::core::ffi::c_int {
        x = wsx.wrapping_sub(1 as u_int);
    } else if strcasecmp(s, c"top-left".as_ptr()) == 0 as ::core::ffi::c_int {
        x = 0 as u_int;
        y = top;
    } else if strcasecmp(s, c"top-right".as_ptr()) == 0 as ::core::ffi::c_int {
        x = wsx.wrapping_sub(1 as u_int);
        y = top;
    } else if strcasecmp(s, c"bottom-left".as_ptr()) == 0 as ::core::ffi::c_int {
        x = 0 as u_int;
        y = bottom;
    } else if strcasecmp(s, c"bottom-right".as_ptr()) == 0 as ::core::ffi::c_int {
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
/// selection callbacks. window-layout-changed fires when panes leave; a pane
/// arriving is announced by its caller, once its process has started or its
/// move has finished.
unsafe fn window_rearrange_panes(
    window: &WindowRef,
    panes: &[Rc<UnsafeCell<window_pane>>],
) -> Result<(), &'static CStr> {
    let w = window.get();
    let old = (*w).panes.snapshot();
    let present = |list: &[Rc<UnsafeCell<window_pane>>], pane: &Rc<UnsafeCell<window_pane>>| {
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
unsafe fn window_remove_pane(window: &WindowRef, pane: &Rc<UnsafeCell<window_pane>>) {
    let rest = (*window.get())
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
    let w = &*window.get();
    w.layout.arrange(&w.panes.snapshot(), size)
}

/// The rectangles for the current panes, in pane order.
unsafe fn window_pane_cells(
    window: &WindowRef,
) -> Vec<(Weak<UnsafeCell<window_pane>>, layout_geometry)> {
    let panes = (*window.get()).panes.storage.clone();
    panes.into_iter().zip(window_arrangement(window)).collect()
}

/// The arrange step: the only writer of pane geometry. Every operation that
/// changes the pane order, the layout or its record of a pane, the window size
/// or the options the layout reads ends here, and the clients showing the
/// window follow the result.
unsafe fn window_arrange(window: &WindowRef) {
    let cells = (*window.get())
        .panes
        .snapshot()
        .into_iter()
        .zip(window_arrangement(window));
    let mut moved = false;
    for (pane, cell) in cells {
        if window_has_pane(&*window.get(), &Rc::downgrade(&pane)) {
            moved |= pane.apply_layout(cell, window);
        }
    }
    if moved {
        (*window.get()).invalidate_scene();
    }
    tty_update_window_offset(window);
}

unsafe fn window_destroy_panes(window: &WindowRef) {
    let w = window.get();
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
    let mut flags: [::core::ffi::c_char; 32] = [0; 32];
    let mut pos: u_int = 0 as u_int;
    if wl.get_unchecked().flags & WINLINK_ACTIVITY != 0 {
        let fresh3 = pos;
        pos = pos.wrapping_add(1);
        flags[fresh3 as usize] = '#' as i32 as ::core::ffi::c_char;
        if escape {
            let fresh4 = pos;
            pos = pos.wrapping_add(1);
            flags[fresh4 as usize] = '#' as i32 as ::core::ffi::c_char;
        }
    }
    if wl.get_unchecked().flags & WINLINK_BELL != 0 {
        let fresh5 = pos;
        pos = pos.wrapping_add(1);
        flags[fresh5 as usize] = '!' as i32 as ::core::ffi::c_char;
    }
    if wl.get_unchecked().flags & WINLINK_SILENCE != 0 {
        let fresh6 = pos;
        pos = pos.wrapping_add(1);
        flags[fresh6 as usize] = '~' as i32 as ::core::ffi::c_char;
    }
    if session_owner
        .as_ref()
        .is_some_and(|owner| wl == owner.current_winlink())
    {
        let fresh7 = pos;
        pos = pos.wrapping_add(1);
        flags[fresh7 as usize] = '*' as i32 as ::core::ffi::c_char;
    }
    if session_owner
        .as_ref()
        .is_some_and(|owner| wl == owner.last_winlink())
    {
        let fresh8 = pos;
        pos = pos.wrapping_add(1);
        flags[fresh8 as usize] = '-' as i32 as ::core::ffi::c_char;
    }
    if server_check_marked() != 0 && wl == marked_pane.winlink_handle() {
        let fresh9 = pos;
        pos = pos.wrapping_add(1);
        flags[fresh9 as usize] = 'M' as i32 as ::core::ffi::c_char;
    }
    flags[pos as usize] = '\0' as i32 as ::core::ffi::c_char;
    std::ffi::CStr::from_ptr(flags.as_ptr()).to_owned()
}

fn window_pane_first(w: Option<&window>) -> Option<Rc<std::cell::UnsafeCell<window_pane>>> {
    Some(w?.panes.first()?.upgrade().expect("live pane in ordering"))
}

unsafe fn window_get_pane_status(w: &window) -> ::core::ffi::c_int {
    options_get_number(&w.borrow_options_mut(), c"pane-border-status") as ::core::ffi::c_int
}

unsafe fn window_get_pane_lines(w: &window) -> pane_lines {
    options_get_number(&w.borrow_options_mut(), c"pane-border-lines") as pane_lines
}
