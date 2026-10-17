//! Operations on an opaque, retained window.
//!
//! The holder preserves allocation lifetime, not exclusive access. Callers must
//! preserve the server's single-threaded lifecycle and callback ordering. Scoped
//! option callbacks must not reenter the window, free/reparent the component, or
//! let references or pointers escape.

use super::*;
use crate::src::server_client::ClientRegistry;
use crate::src::shared::client::ClientRef;
use crate::src::shared::menu::menu_data;
use crate::src::shared::window::{WindowRef, WindowWeak};
use crate::src::shared::window::{WINDOW_MAXIMUM, WINDOW_MINIMUM, WINDOW_RESIZE};
use std::time::{Duration, Instant, SystemTime};

pub trait Window {
    unsafe fn update_focus(&self);
    unsafe fn redraw_active_switch(&self, pane: Option<&Rc<UnsafeCell<window_pane>>>);
    unsafe fn pane_at(&self, x: u32, y: u32) -> Weak<UnsafeCell<window_pane>>;
    unsafe fn find_pane(&self, name: &CStr) -> Weak<UnsafeCell<window_pane>>;

    /// Selection history is separate from pane order. Only pure history edits
    /// may run while this component is borrowed.
    type PaneHistoryMut<'a>: std::ops::DerefMut<Target = PaneHistory>
    where
        Self: 'a;
    unsafe fn borrow_pane_history_mut(&self) -> Self::PaneHistoryMut<'_>;
    /// The window's panes become exactly `panes`, in this order: the one way
    /// panes are added, removed and reordered. A list that brings in a pane
    /// must be one the layout takes, or the layout's reason comes back and
    /// nothing has changed; a removal or a reorder is never refused. Arriving
    /// panes are already reparented to this window. Leaving panes are not
    /// destroyed. window-layout-changed fires when panes leave; the caller
    /// announces panes that arrive.
    unsafe fn rearrange_panes(
        &self,
        panes: &[Rc<UnsafeCell<window_pane>>],
    ) -> Result<(), &'static CStr>;
    /// Rearrange the panes without `pane`, then destroy it: the common case
    /// of `rearrange_panes`, for closing a pane.
    unsafe fn remove_pane(&self, pane: &Rc<UnsafeCell<window_pane>>);
    /// Apply a layout action to one pane: the layout gives the pane's new
    /// metadata. Refused, with nothing changed, by a layout that can't do it
    /// or doesn't take the result; window-layout-changed fires otherwise.
    unsafe fn layout_action(
        &self,
        pane: &Rc<UnsafeCell<window_pane>>,
        action: LayoutAction,
    ) -> Result<(), &'static CStr>;
    unsafe fn layout(&self) -> LayoutKind;
    /// Arrange the panes in a fresh `kind` layout, in pane order, and redraw;
    /// window-layout-changed fires. A different layout forgets every pane's
    /// layout metadata; the current one keeps it.
    unsafe fn set_layout(&self, kind: LayoutKind);
    /// The pane in `direction` from `pane`: the previous or next pane of the
    /// strip, or the tile across the separator.
    unsafe fn pane_in_direction(
        &self,
        pane: &Rc<UnsafeCell<window_pane>>,
        direction: Direction,
    ) -> Weak<UnsafeCell<window_pane>>;
    /// The layout's rectangles in pane order, before pane border and
    /// scrollbar adjustments. Pane identities and copied geometry only.
    unsafe fn pane_cells(&self) -> Vec<(Weak<UnsafeCell<window_pane>>, layout_geometry)>;
    /// The area clients clip and pan across, as the layout computes it from
    /// the panes and the window size. A strip's width is its last pane's
    /// first column plus the window width.
    unsafe fn logical_size(&self) -> (u32, u32);
    /// The layout as a layout string. `legacy` selects the checksummed format
    /// of older control clients.
    unsafe fn layout_string(&self, legacy: bool) -> Option<CString>;

    unsafe fn id(&self) -> u32;
    /// Registry successor.
    unsafe fn next_window(&self) -> WindowWeak;
    /// Live association traversal; callbacks may change the list between calls.
    unsafe fn next_winlink(&self, after: Option<refbox::Weak<winlink>>) -> refbox::Weak<winlink>;
    /// Immediate association edits; the Session index retains the link itself.
    unsafe fn add_winlink(&self, link: refbox::Weak<winlink>);
    unsafe fn remove_winlink(&self, link: refbox::Weak<winlink>);
    unsafe fn contains_pane(&self, pane: &Weak<UnsafeCell<window_pane>>) -> bool;
    /// Only alert bits, never the window's other bookkeeping flags.
    unsafe fn pending_alerts(&self) -> i32;
    unsafe fn reset_alert_timer(&self);
    /// Record flags and claim queue membership. Some(true) transfers one release duty
    /// to the alert queue. None means disabled; Some(false) means already queued.
    unsafe fn queue_alerts(&self, flags: i32) -> Option<bool>;
    /// Called after delivery, while the queue still retains this window.
    unsafe fn finish_alerts(&self);
    /// Clear pending flag bits without changing alert-queue membership.
    unsafe fn clear_alert_flags(&self);

    /// Name a new window without a rename event; an explicit name disables
    /// automatic naming. Creation notification belongs to the caller.
    unsafe fn initialize_name(&self, name: CString, explicit: bool);
    unsafe fn name(&self) -> CString;
    unsafe fn rename(&self, name: &CStr, untrusted: bool);
    /// Commit a due automatic-name check, or queue its remaining delay. No
    /// formatting or pane callbacks execute while the timer state is borrowed.
    unsafe fn begin_name_check(&self, now: Instant) -> bool;
    /// Coalesce cursor-driven offset updates at the existing 10 ms deadline.
    /// The timer observes this window weakly; dispatch borrows only on demand.
    unsafe fn schedule_offset_update(&self);
    /// Copy identity without upgrading it or changing pane ownership.
    unsafe fn active_pane(&self) -> Weak<UnsafeCell<window_pane>>;
    /// Most recently active pane from selection history.
    unsafe fn last_active_pane(&self) -> Weak<UnsafeCell<window_pane>>;
    /// Copy the current pane order without retaining a Window borrow, for
    /// sorting, rendering and other walks that may reenter the Window. Walks
    /// resolve each pane as they reach it.
    unsafe fn pane_snapshot(&self) -> Vec<Weak<UnsafeCell<window_pane>>>;
    /// The pane before or after `from` in pane order, without wrapping; `None`
    /// gives the first or last pane. Empty past either end, or when `from` is
    /// not in this window.
    unsafe fn step_pane(
        &self,
        after: Option<&Weak<UnsafeCell<window_pane>>>,
        reverse: bool,
    ) -> Weak<UnsafeCell<window_pane>>;
    unsafe fn scrollbar_mode(&self) -> i32;
    unsafe fn scrollbar_position(&self) -> i32;
    /// Refresh cached scrollbar policy after options change. Pane resizing is
    /// performed by the caller after this borrow ends.
    unsafe fn refresh_scrollbars(&self);
    /// The window's pane-border-status: off, top or bottom.
    unsafe fn pane_border_status(&self) -> i32;
    unsafe fn pane_border_lines(&self) -> pane_lines;
    unsafe fn pane_at_index(&self, index: u32) -> Weak<UnsafeCell<window_pane>>;
    unsafe fn pane_index(&self, pane: &Weak<UnsafeCell<window_pane>>) -> Option<u32>;
    unsafe fn pane_history_index(&self, pane: &Weak<UnsafeCell<window_pane>>) -> Option<u32>;
    /// Advance in pane order with wrapping, retaining the original zero-step result.
    unsafe fn pane_by_number(
        &self,
        pane: Option<&Rc<UnsafeCell<window_pane>>>,
        count: u32,
        reverse: bool,
    ) -> Weak<UnsafeCell<window_pane>>;
    unsafe fn creation_time(&self) -> SystemTime;
    unsafe fn activity_time(&self) -> SystemTime;
    unsafe fn select_pane(&self, pane: &Rc<UnsafeCell<window_pane>>, notify: bool) -> i32;
    unsafe fn size(&self) -> (u32, u32);
    unsafe fn manual_size(&self) -> (u32, u32);
    /// Select manual sizing and record both requested dimensions together.
    unsafe fn set_manual_size(&self, sx: u32, sy: u32);
    unsafe fn pending_resize(&self) -> Option<(u32, u32)>;
    unsafe fn apply_pending_resize(&self);
    unsafe fn defer_resize(&self, sx: u32, sy: u32, xpixel: u32, ypixel: u32);
    /// Pixel dimensions of a terminal cell, for the pane's PTY resize protocol.
    unsafe fn cell_size(&self) -> (u32, u32);
    unsafe fn resize(&self, sx: u32, sy: u32, xpixel: i32, ypixel: i32);
    /// Arrange the layout again after options or pane modes it reads changed.
    /// Silent; returns whether any pane moved.
    unsafe fn refit(&self);
    unsafe fn update_activity(&self);
    /// Return whether the identity changed. Attachment and input dispatch have
    /// different notifications and keep that orchestration in their callers.
    unsafe fn set_latest_client(&self, client: Option<&ClientRef>) -> bool;
    unsafe fn is_latest_client(&self, client: &ClientRef) -> bool;
    /// Window-owned menu/selection policy used when a pane's focus changes.
    unsafe fn pane_is_focused(&self, pane: &Rc<UnsafeCell<window_pane>>) -> bool;
    /// Observe the independently owned menu; no Window/component pointer escapes.
    unsafe fn menu_observer(&self) -> Option<refbox::Weak<menu_data>>;
    /// Detach only the expected menu (or any menu for None). The caller performs
    /// explicit cancellation after this operation, outside the Window borrow.
    unsafe fn take_menu(
        &self,
        expected: Option<&refbox::Weak<menu_data>>,
    ) -> Option<refbox::RefBox<menu_data>>;
    /// Publish first, then let the caller cancel a displaced menu outside the borrow.
    unsafe fn replace_menu(
        &self,
        menu: refbox::RefBox<menu_data>,
    ) -> Option<refbox::RefBox<menu_data>>;
    /// Clamp to the current dimensions and remember the resulting menu position.
    unsafe fn place_menu(&self, position: (u32, u32), size: (u32, u32)) -> (u32, u32);
    unsafe fn last_menu_position(&self) -> (u32, u32);
    unsafe fn invalidate_scene(&self);
    unsafe fn scene_generation(&self) -> u64;
    /// Render each fill cell outside the Window borrow, publishing its fallback
    /// first, in inside/outside order as observed by format callbacks.
    unsafe fn refresh_fill_cells(&self);
    unsafe fn fill_cell(&self, inside: bool) -> grid_cell;
    unsafe fn with_options_mut<R>(&self, edit: impl FnOnce(&mut options) -> R) -> R;
    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::src::shared::format::format_tree,
    ) -> Option<crate::src::format::FormatValue>;
}

impl Window for WindowRef {
    unsafe fn update_focus(&self) {
        window_update_focus(Some(self));
    }
    unsafe fn redraw_active_switch(&self, pane: Option<&Rc<UnsafeCell<window_pane>>>) {
        window_redraw_active_switch(self, pane);
    }
    unsafe fn pane_at(&self, x: u32, y: u32) -> Weak<UnsafeCell<window_pane>> {
        window_get_active_at(self, x, y)
    }
    unsafe fn find_pane(&self, name: &CStr) -> Weak<UnsafeCell<window_pane>> {
        window_find_string(self, name)
    }

    type PaneHistoryMut<'a> = &'a mut PaneHistory;
    unsafe fn borrow_pane_history_mut(&self) -> Self::PaneHistoryMut<'_> {
        &mut (*self.get()).last_panes
    }
    unsafe fn rearrange_panes(
        &self,
        panes: &[Rc<UnsafeCell<window_pane>>],
    ) -> Result<(), &'static CStr> {
        window_rearrange_panes(self, panes)
    }
    unsafe fn remove_pane(&self, pane: &Rc<UnsafeCell<window_pane>>) {
        window_remove_pane(self, pane);
    }
    unsafe fn layout_action(
        &self,
        pane: &Rc<UnsafeCell<window_pane>>,
        action: LayoutAction,
    ) -> Result<(), &'static CStr> {
        let state = &*self.get();
        let meta = state
            .layout
            .apply(action, pane.borrow_layout_meta().as_deref())?;
        let panes = state.panes.snapshot();
        let admitted = state.layout.admits(&panes, self.size()).is_ok();
        let old = std::mem::replace(&mut *pane.borrow_layout_meta(), meta);
        if let Err(reason) = state.layout.admits(&panes, self.size()) {
            if admitted {
                *pane.borrow_layout_meta() = old;
                return Err(reason);
            }
        }
        (*self.get()).invalidate_scene();
        window_arrange(self);
        server_redraw_window(self);
        events_fire_window(c"window-layout-changed", self.clone());
        Ok(())
    }
    unsafe fn layout(&self) -> LayoutKind {
        (*self.get()).layout
    }
    unsafe fn set_layout(&self, kind: LayoutKind) {
        let state = &mut *self.get();
        if state.layout != kind {
            for pane in state.panes.snapshot() {
                *pane.borrow_layout_meta() = None;
            }
        }
        state.layout = kind;
        state.invalidate_scene();
        window_arrange(self);
        server_redraw_window(self);
        events_fire_window(c"window-layout-changed", self.clone());
    }
    unsafe fn pane_in_direction(
        &self,
        pane: &Rc<UnsafeCell<window_pane>>,
        direction: Direction,
    ) -> Weak<UnsafeCell<window_pane>> {
        let pane = Rc::downgrade(pane);
        let cells = window_pane_cells(self);
        let from = cells
            .iter()
            .position(|(candidate, _)| candidate.ptr_eq(&pane))
            .expect("pane belongs to window");
        let geometry = cells.iter().map(|&(_, cell)| cell).collect::<Vec<_>>();
        layout::adjacent(&geometry, from, direction)
            .map_or_else(Weak::new, |to| cells[to].0.clone())
    }
    unsafe fn pane_cells(&self) -> Vec<(Weak<UnsafeCell<window_pane>>, layout_geometry)> {
        window_pane_cells(self)
    }
    unsafe fn logical_size(&self) -> (u32, u32) {
        let state = &*self.get();
        state.layout.logical_size(
            state
                .panes
                .storage
                .iter()
                .map(|pane| pane.upgrade().expect("live pane in ordering")),
            (state.sx, state.sy),
        )
    }
    unsafe fn layout_string(&self, legacy: bool) -> Option<CString> {
        let active = self.active_pane();
        let entries = self
            .pane_cells()
            .into_iter()
            .map(|(observer, geometry)| {
                let pane = observer.upgrade().expect("laid out pane is live");
                layout::LayoutEntry {
                    geometry,
                    id: pane.id(),
                    index: self
                        .pane_index(&observer)
                        .expect("laid out pane has an index"),
                    active: active.ptr_eq(&observer),
                    last: self.pane_history_index(&observer),
                }
            })
            .collect::<Vec<_>>();
        let size = self.size();
        let state = &*self.get();
        let tree = state.layout.tree(&state.panes.snapshot(), size);
        layout::layout_string(tree.as_ref(), &entries, legacy)
    }

    unsafe fn refresh_fill_cells(&self) {
        for inside in [true, false] {
            let mut fallback = crate::src::grid::grid_default_cell;
            fallback.attr |= crate::src::shared::grid::GRID_ATTR_CHARSET as u16;
            crate::src::text::utf8::utf8_set(
                &mut fallback.data,
                crate::src::shared::borders::CELL_BORDERS
                    [crate::src::shared::borders::CELL_NONE as usize] as u8,
            );
            {
                let state = &mut *self.get();
                if inside {
                    state.inside_cell = fallback;
                } else {
                    state.outside_cell = fallback;
                }
            }
            if let Some(rendered) = crate::src::window_border::window_render_fill_cell(self, inside)
            {
                let state = &mut *self.get();
                if inside {
                    state.inside_cell = rendered;
                } else {
                    state.outside_cell = rendered;
                }
            }
        }
    }
    unsafe fn fill_cell(&self, inside: bool) -> grid_cell {
        let state = &*self.get();
        if inside {
            state.inside_cell
        } else {
            state.outside_cell
        }
    }

    unsafe fn menu_observer(&self) -> Option<refbox::Weak<menu_data>> {
        (*self.get()).menu.as_ref().map(refbox::RefBox::downgrade)
    }
    unsafe fn take_menu(
        &self,
        expected: Option<&refbox::Weak<menu_data>>,
    ) -> Option<refbox::RefBox<menu_data>> {
        let state = &mut *self.get();
        if expected.is_some_and(|expected| {
            !state
                .menu
                .as_ref()
                .is_some_and(|current| expected.is(current))
        }) {
            return None;
        }
        state.menu.take()
    }
    unsafe fn replace_menu(
        &self,
        menu: refbox::RefBox<menu_data>,
    ) -> Option<refbox::RefBox<menu_data>> {
        (*self.get()).menu.replace(menu)
    }
    unsafe fn place_menu(&self, (mut px, mut py): (u32, u32), (sx, sy): (u32, u32)) -> (u32, u32) {
        let (wsx, wsy) = self.logical_size();
        let state = &mut *self.get();
        if sx >= wsx {
            px = 0;
        } else if px.wrapping_add(sx) > wsx {
            px = wsx.wrapping_sub(sx);
        }
        if sy >= wsy {
            py = 0;
        } else if py.wrapping_add(sy) > wsy {
            py = wsy.wrapping_sub(sy);
        }
        state.menu_last_px = px;
        state.menu_last_py = py;
        (px, py)
    }
    unsafe fn last_menu_position(&self) -> (u32, u32) {
        let state = &*self.get();
        (state.menu_last_px, state.menu_last_py)
    }
    unsafe fn invalidate_scene(&self) {
        (*self.get()).invalidate_scene();
    }
    unsafe fn scene_generation(&self) -> u64 {
        (*self.get()).redraw_scene_generation
    }
    unsafe fn add_winlink(&self, link: refbox::Weak<winlink>) {
        window_winlinks_append(&mut *self.get(), link);
    }
    unsafe fn remove_winlink(&self, link: refbox::Weak<winlink>) {
        window_winlinks_remove(&mut *self.get(), link);
    }
    unsafe fn next_window(&self) -> WindowWeak {
        windows::next(&*self.get())
    }
    unsafe fn next_winlink(&self, after: Option<refbox::Weak<winlink>>) -> refbox::Weak<winlink> {
        if let Some(after) = after {
            window_winlinks_next(Some(&*self.get()), after)
        } else {
            window_winlinks_first(Some(&*self.get()))
        }
    }
    unsafe fn contains_pane(&self, pane: &Weak<UnsafeCell<window_pane>>) -> bool {
        window_has_pane(&*self.get(), pane)
    }
    unsafe fn pending_alerts(&self) -> i32 {
        (*self.get()).flags & crate::src::shared::window::WINDOW_ALERTFLAGS
    }
    unsafe fn reset_alert_timer(&self) {
        super::alerts::reset_timer(self);
    }
    unsafe fn queue_alerts(&self, flags: i32) -> Option<bool> {
        super::alerts::queue(self, flags)
    }
    unsafe fn finish_alerts(&self) {
        (*self.get()).alerts_queued = 0;
        (*self.get()).flags &= !crate::src::shared::window::WINDOW_ALERTFLAGS;
    }
    unsafe fn clear_alert_flags(&self) {
        (*self.get()).flags &= !crate::src::shared::window::WINDOW_ALERTFLAGS;
    }

    unsafe fn id(&self) -> u32 {
        (*self.get()).id
    }
    unsafe fn initialize_name(&self, name: CString, explicit: bool) {
        drop(window_replace_name(self, name));
        if explicit {
            self.with_options_mut(|options| {
                crate::src::options::options_set_number(options, c"automatic-rename", 0);
            });
        }
    }
    unsafe fn name(&self) -> CString {
        (*self.get()).name.clone()
    }
    unsafe fn rename(&self, name: &CStr, untrusted: bool) {
        window_set_name(self, name, untrusted as i32);
    }
    unsafe fn begin_name_check(&self, now: Instant) -> bool {
        let state = &mut *self.get();
        let left = crate::src::names::name_time_left(state.name_time, now);
        if !left.is_zero() {
            if state.name_event.is_none() {
                log_debug(format_args!(
                    "@{} name timer queued ({} left)",
                    state.id,
                    left.as_micros()
                ));
                let observer = WindowRef::downgrade(self);
                state.name_event = Some(
                    Timer::new(left, move || {
                        if let Some(owner) = observer.upgrade() {
                            drop((*owner.get()).name_event.take());
                            log_debug(format_args!("@{} name timer expired", owner.id()));
                        }
                    })
                    .expect("arm timer"),
                );
            } else {
                log_debug(format_args!(
                    "@{} name timer already queued ({} left)",
                    state.id,
                    left.as_micros()
                ));
            }
            return false;
        }
        state.name_time = Some(now);
        drop(state.name_event.take());
        true
    }
    unsafe fn schedule_offset_update(&self) {
        let state = &mut *self.get();
        if state.offset_timer.is_none() {
            let delay = Duration::from_micros(10_000);
            let observer = WindowRef::downgrade(self);
            state.offset_timer = Some(
                Timer::new(delay, move || {
                    if let Some(window) = observer.upgrade() {
                        drop((*window.get()).offset_timer.take());
                        crate::src::tty::tty_update_window_offset(&window);
                        drop(window);
                    }
                })
                .expect("arm timer"),
            );
        }
    }
    unsafe fn active_pane(&self) -> Weak<UnsafeCell<window_pane>> {
        (*self.get()).active.clone()
    }
    unsafe fn last_active_pane(&self) -> Weak<UnsafeCell<window_pane>> {
        (*self.get()).last_panes.first()
    }
    unsafe fn pane_snapshot(&self) -> Vec<Weak<UnsafeCell<window_pane>>> {
        (*self.get()).panes.storage.clone()
    }
    unsafe fn step_pane(
        &self,
        after: Option<&Weak<UnsafeCell<window_pane>>>,
        reverse: bool,
    ) -> Weak<UnsafeCell<window_pane>> {
        let panes = &(*self.get()).panes;
        match (after, reverse) {
            (None, false) => panes.first(),
            (None, true) => panes.last(),
            (Some(pane), false) => panes.next(pane),
            (Some(pane), true) => panes.previous(pane),
        }
        .unwrap_or_default()
    }
    unsafe fn scrollbar_mode(&self) -> i32 {
        (*self.get()).sb
    }
    unsafe fn scrollbar_position(&self) -> i32 {
        (*self.get()).sb_pos
    }
    unsafe fn refresh_scrollbars(&self) {
        let state = &mut *self.get();
        let options = state.options.as_deref_mut().expect("live window options");
        state.sb = options_get_number(options, c"pane-scrollbars") as i32;
        state.sb_pos = options_get_number(options, c"pane-scrollbars-position") as i32;
    }
    unsafe fn pane_border_status(&self) -> i32 {
        window_get_pane_status(&*self.get())
    }
    unsafe fn pane_border_lines(&self) -> pane_lines {
        window_get_pane_lines(&*self.get())
    }
    unsafe fn pane_at_index(&self, index: u32) -> Weak<UnsafeCell<window_pane>> {
        let state = &mut *self.get();
        let base = options_get_number(
            state.options.as_deref_mut().expect("window options"),
            c"pane-base-index",
        ) as u32;
        state
            .panes
            .storage
            .get(index.wrapping_sub(base) as usize)
            .cloned()
            .unwrap_or_default()
    }
    unsafe fn pane_index(&self, pane: &Weak<UnsafeCell<window_pane>>) -> Option<u32> {
        let state = &mut *self.get();
        let base = options_get_number(
            state.options.as_deref_mut().expect("window options"),
            c"pane-base-index",
        ) as u32;
        state
            .panes
            .position(pane)
            .map(|index| base.wrapping_add(index as u32))
    }
    unsafe fn pane_history_index(&self, pane: &Weak<UnsafeCell<window_pane>>) -> Option<u32> {
        (*self.get())
            .last_panes
            .position(pane)
            .map(|index| index as u32)
    }
    unsafe fn pane_by_number(
        &self,
        pane: Option<&Rc<UnsafeCell<window_pane>>>,
        count: u32,
        reverse: bool,
    ) -> Weak<UnsafeCell<window_pane>> {
        let panes = &(*self.get()).panes;
        let mut current = pane.map(Rc::downgrade);
        for _ in 0..count {
            current = current
                .and_then(|pane| {
                    if reverse {
                        panes.previous(&pane)
                    } else {
                        panes.next(&pane)
                    }
                })
                .or_else(|| if reverse { panes.last() } else { panes.first() });
        }
        current.unwrap_or_default()
    }
    unsafe fn creation_time(&self) -> SystemTime {
        (*self.get()).creation_time
    }
    unsafe fn activity_time(&self) -> SystemTime {
        (*self.get()).activity_time
    }
    unsafe fn select_pane(&self, pane: &Rc<UnsafeCell<window_pane>>, notify: bool) -> i32 {
        assert!(
            window_has_pane(&*self.get(), &Rc::downgrade(pane)),
            "pane belongs to window"
        );
        window_set_active_pane(self, pane, notify as i32)
    }
    unsafe fn size(&self) -> (u32, u32) {
        ((*self.get()).sx, (*self.get()).sy)
    }
    unsafe fn manual_size(&self) -> (u32, u32) {
        let state = &*self.get();
        (state.manual_sx, state.manual_sy)
    }
    unsafe fn set_manual_size(&self, sx: u32, sy: u32) {
        let state = &mut *self.get();
        crate::src::options::options_set_number(
            state.options.as_deref_mut().expect("live window options"),
            c"window-size",
            crate::src::shared::window::WINDOW_SIZE_MANUAL as _,
        );
        state.manual_sx = sx;
        state.manual_sy = sy;
    }
    unsafe fn pending_resize(&self) -> Option<(u32, u32)> {
        let state = &*self.get();
        (state.flags & WINDOW_RESIZE != 0).then_some((state.new_sx, state.new_sy))
    }
    unsafe fn apply_pending_resize(&self) {
        if (*self.get()).flags & WINDOW_RESIZE != 0 {
            resize_window(
                self,
                (*self.get()).new_sx,
                (*self.get()).new_sy,
                (*self.get()).new_xpixel as i32,
                (*self.get()).new_ypixel as i32,
            );
        }
    }
    unsafe fn defer_resize(&self, sx: u32, sy: u32, xpixel: u32, ypixel: u32) {
        let state = &mut *self.get();
        state.new_sx = sx;
        state.new_sy = sy;
        state.new_xpixel = xpixel;
        state.new_ypixel = ypixel;
        state.flags |= WINDOW_RESIZE;
    }
    unsafe fn cell_size(&self) -> (u32, u32) {
        ((*self.get()).xpixel, (*self.get()).ypixel)
    }
    unsafe fn resize(&self, sx: u32, sy: u32, xpixel: i32, ypixel: i32) {
        resize_window(self, sx, sy, xpixel, ypixel);
    }
    unsafe fn refit(&self) {
        window_arrange(self);
    }
    unsafe fn update_activity(&self) {
        window_update_activity(self);
    }
    unsafe fn set_latest_client(&self, client: Option<&ClientRef>) -> bool {
        let latest = client.map_or_else(Weak::new, Rc::downgrade);
        if (*self.get()).latest.ptr_eq(&latest) {
            return false;
        }
        (*self.get()).latest = latest;
        true
    }
    unsafe fn is_latest_client(&self, client: &ClientRef) -> bool {
        (*self.get()).latest.ptr_eq(&Rc::downgrade(client))
    }
    unsafe fn pane_is_focused(&self, pane: &Rc<UnsafeCell<window_pane>>) -> bool {
        use crate::src::server_client::Client;
        if (*self.get()).menu.is_some() || !self.active_pane().ptr_eq(&Rc::downgrade(pane)) {
            return false;
        }
        let mut cursor = ClientRegistry::first();
        while let Some(client) = cursor {
            if client.focuses_window(self) {
                return true;
            }
            cursor = ClientRegistry::next(&client);
        }
        false
    }
    unsafe fn with_options_mut<R>(&self, edit: impl FnOnce(&mut options) -> R) -> R {
        edit(
            (*self.get())
                .options
                .as_deref_mut()
                .expect("live window options"),
        )
    }
    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::src::shared::format::format_tree,
    ) -> Option<crate::src::format::FormatValue> {
        crate::src::format::window_format_value(self, key, context)
    }
}

unsafe fn resize_fire_window_resized(w_owner: &WindowRef, mut old_sx: u_int, mut old_sy: u_int) {
    let mut w = w_owner.get();
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    fs = cmd_find_state::find_from_window(w_owner, 0 as ::core::ffi::c_int)
        .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    event_payload_set_target(&mut ep, &fs);
    event_payload_set_window(&mut ep, c"window", WindowRef::clone(w_owner));
    event_payload_set_uint(&mut ep, c"width", (*w).sx);
    event_payload_set_uint(&mut ep, c"height", (*w).sy);
    event_payload_set_uint(&mut ep, c"old_width", old_sx);
    event_payload_set_uint(&mut ep, c"old_height", old_sy);
    events_fire(c"window-resized", ep);
}
unsafe fn resize_window(
    w_owner: &WindowRef,
    mut sx: u_int,
    mut sy: u_int,
    mut xpixel: ::core::ffi::c_int,
    mut ypixel: ::core::ffi::c_int,
) {
    let mut w = w_owner.get();
    let mut old_sx: u_int = (*w).sx;
    let mut old_sy: u_int = (*w).sy;
    if sx < WINDOW_MINIMUM as u_int {
        sx = WINDOW_MINIMUM as u_int;
    }
    if sx > WINDOW_MAXIMUM as u_int {
        sx = WINDOW_MAXIMUM as u_int;
    }
    if sy < WINDOW_MINIMUM as u_int {
        sy = WINDOW_MINIMUM as u_int;
    }
    if sy > WINDOW_MAXIMUM as u_int {
        sy = WINDOW_MAXIMUM as u_int;
    }
    window_resize(w_owner, sx, sy, xpixel, ypixel);
    log_debug(format_args!(
        "{}: @{} resized to {}x{}",
        "resize_window",
        { (*w).id },
        { sx },
        { sy }
    ));
    server_redraw_window(w_owner);
    events_fire_window(c"window-layout-changed", WindowRef::clone(w_owner));
    resize_fire_window_resized(w_owner, old_sx, old_sy);
    (*w).flags &= !WINDOW_RESIZE;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::options::options_create;

    #[test]
    fn fill_rendering_keeps_inside_outside_formats_and_composes_border_style() {
        unsafe {
            let mut server_options = options_create(None);
            crate::src::options::options_default(
                &mut *server_options,
                crate::src::options_table::options_table
                    .iter()
                    .find(|entry| entry.name == Some(c"extended-keys"))
                    .unwrap(),
            );
            let previous_server_options =
                std::mem::replace(&mut crate::src::tmux::global_options, &mut *server_options);
            let owner = window::new_ref();
            let mut options = options_create(None);
            let entry = crate::src::options_table::options_table
                .iter()
                .find(|entry| entry.name == Some(c"fill-character"))
                .unwrap();
            crate::src::options::options_default(&mut *options, entry);
            (*owner.get()).options = Some(options);
            owner.with_options_mut(|options| {
                crate::src::options::options_set_string(options, c"fill-character", 0, |out| {
                    out.write_all(b"#[fg=red]#{?is_inside,I,O}")
                });
            });
            owner.refresh_fill_cells();
            let original = owner.fill_cell(true);
            assert_eq!(original.data.data[0], b'I');
            assert_eq!(owner.fill_cell(false).data.data[0], b'O');
            let mut border = crate::src::grid::grid_default_cell;
            border.fg = 4;
            border.bg = 3;
            border.attr = crate::src::shared::grid::GRID_ATTR_UNDERSCORE as u16;
            crate::src::window_border::window_get_fill_cell(&owner, 1, &mut border);
            assert_eq!(border.data.data[0], b'I');
            assert_eq!(border.fg, 1, "fill overrides an explicit foreground");
            assert_eq!(
                border.bg, 3,
                "default fill background preserves border style"
            );
            assert_ne!(
                border.attr & crate::src::shared::grid::GRID_ATTR_UNDERSCORE as u16,
                0
            );
            owner.with_options_mut(|options| {
                crate::src::options::options_set_string(options, c"fill-character", 0, |out| {
                    out.write_all(b"#{?is_outside,X,Y}")
                });
            });
            owner.refresh_fill_cells();
            assert_eq!(owner.fill_cell(true).data.data[0], b'Y');
            assert_eq!(owner.fill_cell(false).data.data[0], b'X');
            assert_eq!(
                original.data.data[0], b'I',
                "fill snapshots survive refresh"
            );
            drop(owner);
            crate::src::tmux::global_options = previous_server_options;
            drop(server_options);
        }
    }
}
