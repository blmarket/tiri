//! Operations on retained panes. Logical destruction remains explicit.
use super::*;
use crate::grid::{grid_get_cell, grid_set_cell};
use crate::hyperlinks::{hyperlinks_get, hyperlinks_put};
use crate::options::OptionsScope;
use crate::reactor::Interests;
use crate::server_client::Client;
use crate::server_client::ClientRegistry;
use crate::shared::client::ClientRef;
use crate::shared::colour::colour_palette;
use crate::shared::command::{cmd, CmdOutcome};
use crate::shared::pane::{
    PANE_ACTIVITY, PANE_DROP, PANE_MINIMUM, PANE_NEWSTATUS, PANE_STATUS_BOTTOM, PANE_STATUS_TOP,
};
use crate::shared::screen::MODE_SYNC;
use crate::shared::window::{WindowRef, WindowWeak};
use std::ffi::CStr;
use std::os::fd::AsRawFd;
use std::time::Duration;

/// Access a pane without lending its model storage.
///
/// Methods retain the legacy event-loop exclusivity and logical-lifetime
/// preconditions. Component closures must not reenter model code, destroy an
/// owner, alter component parent links, or let component references escape.
pub trait WindowPane {
    /// Visible geometry including the reserved scrollbar area, copied for hit testing.
    unsafe fn outer_geometry(&self) -> (i32, i32, u32, u32);
    unsafe fn pane_lines(&self) -> pane_lines;
    type LayoutMeta<'a>: std::ops::DerefMut<Target = Option<Box<dyn std::any::Any>>>
    where
        Self: 'a;
    /// This pane's layout metadata; only the Window writes it. The guard
    /// permits metadata-only work and must end before dispatch.
    unsafe fn borrow_layout_meta(&self) -> Self::LayoutMeta<'_>;
    unsafe fn border_status(&self) -> i32;
    unsafe fn has_pending_change(&self) -> bool;
    unsafe fn acknowledge_change(&self);
    /// Output copy mode has not yet refreshed from; kept apart from
    /// [`Self::has_pending_change`], which automatic rename consumes.
    unsafe fn has_unseen_changes(&self) -> bool;
    unsafe fn acknowledge_unseen_changes(&self);
    /// Derive an owned default Window name from this pane's command or shell.
    unsafe fn default_window_name(&self) -> CString;
    /// A borrow can become Ref::map on one RefCell containing the whole pane.
    type Palette<'a>: std::ops::Deref<Target = colour_palette>
    where
        Self: 'a;
    /// Do not reenter the pane, dispatch callbacks or let component pointers
    /// escape while the returned guard is alive.
    unsafe fn borrow_palette(&self) -> Self::Palette<'_>;
    type PaletteMut<'a>: std::ops::DerefMut<Target = colour_palette>
    where
        Self: 'a;
    /// The mutation guard permits palette-only work and must end before dispatch.
    unsafe fn borrow_palette_mut(&self) -> Self::PaletteMut<'_>;

    unsafe fn id(&self) -> u32;
    fn find_by_id(id: u32) -> Option<Self>
    where
        Self: Sized;
    fn find_by_id_str(id: &CStr) -> Option<Self>
    where
        Self: Sized;
    unsafe fn from_observer(observer: &std::rc::Weak<UnsafeCell<window_pane>>) -> Option<Self>
    where
        Self: Sized;
    unsafe fn tty_name(&self) -> CString;
    unsafe fn control_colours(&self) -> (i32, i32);
    unsafe fn update_control_colours(&self, fg: i32, bg: i32);
    unsafe fn has_prompt(&self) -> bool;
    unsafe fn set_prompt(
        &self,
        client: Option<&ClientRef>,
        find: Option<&cmd_find_state>,
        message: &CStr,
        input: Option<&CStr>,
        inputcb: status_prompt_input_cb,
        freecb: prompt_free_cb,
        flags: i32,
        kind: prompt_type,
    );
    unsafe fn update_prompt(&self, message: &CStr, input: Option<&CStr>);
    /// Write a paste buffer to the pane stream with explicit line separators.
    unsafe fn paste_buffer(&self, bytes: &[u8], separator: &[u8], bracket: bool, raw: bool);
    unsafe fn capture(
        &self,
        arguments: &args,
        item: &Rc<std::cell::RefCell<cmdq_item>>,
    ) -> Result<Vec<u8>, CString>;
    /// Clear complete history after retiring modes; optionally reset link identities.
    unsafe fn clear_history(&self, reset_links: bool);
    unsafe fn window_observer(&self) -> WindowWeak;
    /// Move the parent identity and option inheritance together, before the
    /// caller publishes membership or dispatches move/layout notifications.
    unsafe fn reparent(&self, window: &WindowRef);
    unsafe fn geometry(&self) -> (u32, u32, i32, i32);
    unsafe fn send_theme_update(&self);
    unsafe fn mark_theme_changed(&self);
    unsafe fn update_history_limit(&self, limit: u32);
    unsafe fn compare_for_sort(
        &self,
        other: &Self,
        criteria: &crate::shared::sort::sort_criteria,
    ) -> std::cmp::Ordering;
    unsafe fn matches_terminal_name(&self, name: &CStr) -> bool;
    unsafe fn close_after_key(&self, key: key_code) -> bool;
    unsafe fn has_exited(&self) -> bool;
    unsafe fn mode_entry(&self) -> refbox::Weak<window_mode_entry>;
    unsafe fn notify_style_changed(&self);
    unsafe fn needs_redraw(&self, scrollbar: bool) -> bool;
    unsafe fn screen_mode(&self, displayed: bool) -> crate::shared::screen::ScreenMode;
    unsafe fn prompt_position(&self, top: bool) -> Option<(i32, i32)>;
    unsafe fn screen_path(&self) -> CString;
    unsafe fn screen_progress(&self) -> crate::shared::display::progress_bar;

    unsafe fn in_scrollbar_area(&self, x: i32, y: i32) -> bool;
    unsafe fn mouse_location(&self, x: i32, y: i32, slider: &mut u32) -> key_code_mouse_location;
    unsafe fn update_scrollbar_hover(&self, x: i32, y: i32);

    /// Apply a strip rectangle, including pane border and scrollbar policy.
    /// Only the Window's arrange step calls this. Returns whether the visible
    /// geometry changed; resize callbacks run after the pane's placement
    /// fields have been released.
    unsafe fn apply_layout(
        &self,
        geometry: layout_geometry,
        scrollbars: &crate::shared::window::WindowRef,
    ) -> bool;
    /// Mark redraw when the cached active and inactive appearance differs.
    unsafe fn redraw_selection_change(&self);
    unsafe fn refresh_palette(&self);
    unsafe fn mark_changed(&self);
    unsafe fn invalidate_style(&self);
    unsafe fn mark_style_changed(&self, theme: bool);
    unsafe fn set_input_enabled(&self, enabled: bool);
    unsafe fn set_title(&self, title: &CStr) -> bool;
    /// Trim history below the base cursor when no mode owns the displayed screen.
    unsafe fn trim_history(&self);
    /// Finish `waiter` with the exit status once this pane closes.
    unsafe fn wait_until_close(&self, waiter: CmdqWaiter);
    /// Start stdin delivery to an empty pane. The command suspends on the
    /// returned continuation, or does not wait without one.
    unsafe fn start_input(
        &self,
        item: &Rc<std::cell::RefCell<cmdq_item>>,
    ) -> Result<Option<impl std::future::Future<Output = ()> + 'static>, CString>;
    /// Translate a mouse report after copying pane geometry; no pane borrow
    /// survives the coordinate calculation or the caller's following work.
    unsafe fn mouse_position(&self, mouse: &mouse_event, last: bool) -> Option<(u32, u32)>;
    /// Visible cursor in window coordinates, for terminal viewport following.
    unsafe fn visible_cursor_in_window(&self) -> Option<(u32, u32)>;
    /// Whether the pane still has a PTY, including an exited process being drained.
    unsafe fn has_tty(&self) -> bool;
    unsafe fn is_synchronized(&self) -> bool;
    /// Enter sync mode on `base`, the pane's base screen as borrowed by the
    /// write that requested it.
    unsafe fn start_sync(&self, base: &mut screen);
    /// Cancel the timer and leave sync mode before drawing the accumulated rows.
    /// `writing` is the pane's base screen when a write context currently
    /// borrows it; `None` reaches it through the pane. No pane reference is
    /// retained during terminal callbacks.
    unsafe fn stop_sync(&self, writing: Option<&mut screen>);
    unsafe fn clear_sync_dirty(&self);
    /// Decide whether a screen write draws immediately; synchronized writes
    /// mark rows in the pane-owned bitmap, including resize invalidation.
    unsafe fn should_draw_rows(&self, synchronized: bool, y: u32, count: u32, height: u32) -> bool;

    unsafe fn resize(&self, sx: u32, sy: u32);
    unsafe fn update_focus(&self, focused: bool);
    /// Return 0 for a pane outside its window's strip, -1 for deferred drawing, or 1 after
    /// installing pane offsets. Client applies terminal/status offsets afterward.
    /// With `window_redraw` false, the pane is never changed; a disposable context
    /// can therefore probe placement even when dirty drawing would be deferred.
    unsafe fn prepare_render(
        &self,
        context: &mut crate::shared::tty::tty_ctx,
        window_redraw: bool,
    ) -> i32;
    unsafe fn request_redraw(&self, scrollbar: bool);
    /// Copied appearance and slider geometry; no pane storage is lent.
    unsafe fn scrollbar_visible(&self) -> bool;
    unsafe fn scrollbar_overlay(&self) -> bool;
    unsafe fn scrollbar_width(&self) -> i32;
    unsafe fn scrollbar_pad(&self) -> i32;
    unsafe fn scrollbar_slider_y(&self) -> u32;
    unsafe fn scrollbar_slider_height(&self) -> u32;
    unsafe fn redraw_scrollbar(&self);
    unsafe fn hide_scrollbar(&self);
    unsafe fn refresh_scrollbar_style(&self);
    unsafe fn reset_default_cursor(&self);
    /// Record the pane in utmp while it runs a command and the server option
    /// allows it; otherwise drop its record. Closing the pty drops it too.
    unsafe fn refresh_utmp(&self);
    unsafe fn reset_border_cache(&self);
    /// Draw a separator of `cell_type`, a `CELL_*` line kind, in this pane's
    /// border lines.
    unsafe fn border_cell(&self, cell_type: i32, cell: &mut grid_cell);
    unsafe fn border_style(&self, client: &ClientRef) -> grid_cell;
    /// Build the status line for a `width`-column status row whose border
    /// cells are `spans` from `first`; returns whether it changed.
    unsafe fn make_status(
        &self,
        client: &ClientRef,
        width: u32,
        spans: &crate::shared::redraw::redraw_spans,
        first: usize,
    ) -> bool;
    unsafe fn has_new_status(&self) -> bool;
    unsafe fn set_new_status(&self, redraw: bool);
    unsafe fn default_colours(&self) -> (grid_cell, u32);
    unsafe fn is_obscured(&self) -> bool;
    unsafe fn visible_ranges(
        base: Option<&Self>,
        x: i32,
        y: i32,
        width: u32,
        ranges: &mut Vec<visible_range>,
    ) where
        Self: Sized;
    unsafe fn draws_inactive_screen(&self) -> bool;
    unsafe fn window_size(&self) -> (u32, u32);
    unsafe fn schedule_offset_update(&self);
    unsafe fn output_suppressed(&self) -> bool;
    unsafe fn alternate_screen_allowed(&self) -> bool;
    unsafe fn alternate_screen_changed(&self, entered: bool);
    /// Draw with screen ownership temporarily removed from pane storage, from
    /// the status line when `status`, else the displayed screen.
    unsafe fn draw_line(
        &self,
        client: &ClientRef,
        source: (u32, u32),
        width: u32,
        destination: (u32, u32),
        status: bool,
    );
    unsafe fn draw_scrollbar(
        &self,
        client: &ClientRef,
        span: &crate::shared::redraw::redraw_span,
        x: u32,
        y: u32,
        width: u32,
    );
    unsafe fn draw_prompt(&self, context: &mut crate::screen_redraw::redraw_draw_ctx<'_>);

    /// Normal selection records activity; fallback selection only marks change.
    unsafe fn on_selected(&self, record_activity: bool);
    unsafe fn key(
        &self,
        client: Option<&ClientRef>,
        link: refbox::Weak<winlink>,
        key: key_code,
        mouse: Option<&mut mouse_event>,
    ) -> i32;
    unsafe fn paste(&self, key: key_code, bytes: &[u8]);
    unsafe fn set_mode(
        &self,
        source: Option<&Rc<UnsafeCell<window_pane>>>,
        mode: &'static window_mode,
        item: Option<&Rc<std::cell::RefCell<cmdq_item>>>,
        find: Option<&cmd_find_state>,
        arguments: Option<&args>,
    ) -> i32;
    unsafe fn reset_mode(&self);
    unsafe fn reset_all_modes(&self);
    /// Replace destination with an independent rectangular copy. Coordinates
    /// are relative to the visible screen, excluding history. `displayed`
    /// selects the active mode screen; false selects the process's base screen.
    /// The destination must not alias any pane screen; caller must drop it.
    unsafe fn copy_screen(
        &self,
        destination: &mut screen,
        x: u32,
        y: u32,
        sx: u32,
        sy: u32,
        displayed: bool,
    );
    unsafe fn copy_output(&self, offset: &window_pane_offset, destination: &mut [u8]) -> usize;
    /// Start a new output consumer at the parser's current position.
    unsafe fn output_offset(&self) -> window_pane_offset;
    unsafe fn advance_output(&self, offset: &mut window_pane_offset, count: usize);
    /// Run once after all clients have consumed the current event-loop cycle.
    unsafe fn finish_cycle(&self);
    unsafe fn borrow_options_mut(&self) -> std::cell::RefMut<'_, options>;
    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::shared::format::format_tree,
    ) -> Option<crate::format::FormatValue>;
    /// Execute pipe-pane while process descriptors and callbacks stay owned here.
    unsafe fn pipe(
        &self,
        command: refbox::Weak<cmd>,
        item: &Rc<std::cell::RefCell<cmdq_item>>,
    ) -> CmdOutcome;
    unsafe fn process_id(&self) -> pid_t;
    /// Record an exit before finishing waits/editors; false means a different process.
    unsafe fn process_exited(&self, pid: pid_t, status: std::process::ExitStatus) -> bool;
    unsafe fn kill_process(&self);
    /// Close descriptors before exit notifications and remain-on-exit rendering.
    unsafe fn finish_process(&self, notify: i32);
    /// Create a pane beside the context's source pane (before it with
    /// SPAWN_BEFORE), or as the first pane of an empty window, place it and
    /// start its process; with SPAWN_RESPAWN restart the context's source
    /// pane instead. The layout's refusal or the spawn's failure comes back
    /// in `cause`; a refusal comes before any side effect.
    unsafe fn spawn_process(
        context: &mut crate::shared::spawn::spawn_context,
        cause: &mut Option<CString>,
    ) -> Option<Self>
    where
        Self: Sized;
    unsafe fn install_editor(
        &self,
        editor: Box<crate::shared::spawn::spawn_editor_state>,
    ) -> crate::shared::spawn::EditorHandle;
    unsafe fn finish_editing(&self);
    unsafe fn editor_identity(&self) -> Option<crate::shared::spawn::EditorId>;
    unsafe fn cancel_editor(&self, identity: crate::shared::spawn::EditorId);
    unsafe fn editor_process_id(&self, identity: crate::shared::spawn::EditorId) -> pid_t;
    /// Encode a key or mouse report for this pane's process stream.
    unsafe fn write_key(&self, key: key_code, mouse: Option<&mouse_event>) -> i32;
    /// Compare the active mode with its immutable descriptor.
    unsafe fn is_mode(&self, mode: &'static window_mode) -> bool;
    /// Invoke the active mode's format callback after releasing pane storage.
    unsafe fn add_mode_formats(&self, context: &mut crate::shared::format::format_tree);
    /// Reset palette/parser state and invalidate pane styling as one operation.
    unsafe fn reset_input(&self);
    unsafe fn screen_size(&self, displayed: bool) -> (u32, u32);
    unsafe fn clone_history(
        &self,
        hint: &screen,
        cursor: Option<(&mut u32, &mut u32)>,
        trim: bool,
    ) -> Box<screen>;
    unsafe fn sync_history(&self, target: &mut screen, sync: (u32, u32, u32)) -> bool;
    unsafe fn history_scroll(&self) -> (u32, u32, u32);
    unsafe fn share_screen_hyperlinks(&self, target: &mut screen);
    unsafe fn grid_hyperlink(
        &self,
        grid: &crate::shared::grid::grid,
        x: u32,
        y: u32,
    ) -> Option<CString>;
    unsafe fn saved_search(&self) -> (Option<CString>, i32);
    unsafe fn save_search(&self, value: CString, regex: i32);
    unsafe fn show_scrollbar(&self);
    unsafe fn take_pending_redraw(&self) -> bool;
    unsafe fn process_name(&self) -> Option<CString>;
    /// Draw the editor wait indicator on the currently displayed screen.
    unsafe fn draw_editor_waiting(&self, pid: pid_t);
    unsafe fn search(&self, term: &CStr, regex: bool, ignore: bool) -> u32;
    unsafe fn status_range(&self, x: u32, y: u32) -> Option<style_range>;
    unsafe fn prompt_key(
        &self,
        client: Option<&ClientRef>,
        key: key_code,
        mouse: Option<&mut mouse_event>,
    ) -> prompt_key_result;
    unsafe fn notify_moved(
        &self,
        old_window: &WindowRef,
        old_index: i32,
        new_window: &WindowRef,
        new_index: i32,
    );
    unsafe fn destroy_ready(&self) -> bool;
    unsafe fn destroy(&self);
}

impl WindowPane for Rc<UnsafeCell<window_pane>> {
    unsafe fn send_theme_update(&self) {
        window_pane_send_theme_update(self);
    }
    unsafe fn mark_theme_changed(&self) {
        (*self.get()).flags |= PANE_THEMECHANGED;
    }
    unsafe fn update_history_limit(&self, limit: u32) {
        let pane = &mut *self.get();
        let id = pane.id;
        let grid = pane.base.grid_mut();
        let old_size = grid.hsize;
        grid.hlimit = limit;
        crate::grid::grid_collect_history(grid, 1);
        if grid.hsize != old_size {
            log_debug(format_args!(
                "session_update_history: %{} {} -> {}",
                id, old_size, grid.hsize
            ));
        }
    }
    unsafe fn compare_for_sort(
        &self,
        other: &Self,
        criteria: &crate::shared::sort::sort_criteria,
    ) -> std::cmp::Ordering {
        sort::compare(self, other, criteria)
    }
    unsafe fn matches_terminal_name(&self, name: &CStr) -> bool {
        (*self.get()).tty() == name
    }
    unsafe fn close_after_key(&self, key: key_code) -> bool {
        if !self.has_exited()
            || key & KEYC_MASK_KEY == KEYC_MOUSE
            || (KEYC_TYPE_MOUSEMOVE as key_code) << 32 <= key & KEYC_MASK_TYPE
                && key & KEYC_MASK_TYPE <= (KEYC_TYPE_TRIPLECLICK as key_code) << 32
            || key & KEYC_MASK_TYPE == (KEYC_TYPE_FUNCTION as key_code) << 32
                && (key & KEYC_MASK_KEY == KEYC_PASTE_START
                    || key & KEYC_MASK_KEY == KEYC_PASTE_END)
        {
            return false;
        }
        let close = {
            let mut options = self.borrow_options_mut();
            let remain = options_get_number(&options, c"remain-on-exit");
            if remain == 3 || remain == 4 {
                crate::options::options_set_number(&mut options, c"remain-on-exit", 0);
                true
            } else {
                false
            }
        };
        if close {
            self.finish_process(0);
        }
        close
    }
    unsafe fn has_exited(&self) -> bool {
        (*self.get()).flags & PANE_EXITED != 0
    }
    unsafe fn mode_entry(&self) -> refbox::Weak<window_mode_entry> {
        (*self.get()).active_mode_entry()
    }
    unsafe fn notify_style_changed(&self) {
        if (*self.get()).flags & PANE_STYLECHANGED == 0 {
            return;
        }
        let entry = self.mode_entry();
        if !entry.is_alive() {
            return;
        }
        let callback = entry
            .try_borrow_mut()
            .expect("style mode not borrowed")
            .mode
            .style_changed;
        if let Some(callback) = callback {
            callback(entry);
        }
    }
    unsafe fn needs_redraw(&self, scrollbar: bool) -> bool {
        (*self.get()).flags
            & if scrollbar {
                PANE_REDRAWSCROLLBAR
            } else {
                PANE_REDRAW
            }
            != 0
    }
    unsafe fn screen_mode(&self, displayed: bool) -> crate::shared::screen::ScreenMode {
        let pane = &*self.get();
        if displayed {
            pane.with_screen(|screen| crate::shared::screen::ScreenMode::from(screen))
                .expect("displayed pane screen")
        } else {
            crate::shared::screen::ScreenMode::from(&pane.base)
        }
    }
    unsafe fn prompt_position(&self, top: bool) -> Option<(i32, i32)> {
        let pane = &*self.get();
        if window_pane_has_prompt(pane) == 0 {
            return None;
        }
        let y = if top {
            pane.yoff
        } else {
            (pane.yoff as u32).wrapping_add(pane.sy).wrapping_sub(1) as i32
        };
        Some(((pane.xoff as u32).wrapping_add(pane.prompt_cx) as i32, y))
    }
    unsafe fn screen_path(&self) -> CString {
        (*self.get()).base.path.as_deref().unwrap_or(c"").to_owned()
    }
    unsafe fn screen_progress(&self) -> crate::shared::display::progress_bar {
        (*self.get()).base.progress_bar
    }

    unsafe fn in_scrollbar_area(&self, x: i32, y: i32) -> bool {
        super::mouse::in_scrollbar_area(self, x, y) != 0
    }
    unsafe fn mouse_location(&self, x: i32, y: i32, slider: &mut u32) -> key_code_mouse_location {
        super::mouse::mouse_location(self, x, y, slider)
    }
    unsafe fn update_scrollbar_hover(&self, x: i32, y: i32) {
        if self.in_scrollbar_area(x, y) {
            window_pane_scrollbar_show(self);
        } else {
            window_pane_scrollbar_start_timer(self);
        }
    }

    unsafe fn scrollbar_visible(&self) -> bool {
        window_pane_scrollbar_visible(&*self.get()) != 0
    }
    unsafe fn scrollbar_overlay(&self) -> bool {
        window_pane_scrollbar_overlay(&*self.get()) != 0
    }
    unsafe fn scrollbar_width(&self) -> i32 {
        (*self.get()).scrollbar_style.width
    }
    unsafe fn scrollbar_pad(&self) -> i32 {
        (*self.get()).scrollbar_style.pad
    }
    unsafe fn scrollbar_slider_y(&self) -> u32 {
        (*self.get()).sb_slider_y
    }
    unsafe fn scrollbar_slider_height(&self) -> u32 {
        (*self.get()).sb_slider_h
    }
    unsafe fn redraw_scrollbar(&self) {
        window_pane_scrollbar_redraw(self);
    }
    unsafe fn hide_scrollbar(&self) {
        window_pane_scrollbar_hide(self);
    }
    unsafe fn refresh_scrollbar_style(&self) {
        super::render::refresh_scrollbar_style(self);
    }
    unsafe fn reset_default_cursor(&self) {
        window_pane_default_cursor(self);
    }

    unsafe fn refresh_utmp(&self) {
        if !cfg!(target_os = "linux") {
            return;
        }
        let pane = &mut *self.get();
        let Some(pty) = pane.fd.as_mut() else { return };
        if pane.flags & (PANE_EMPTY | PANE_EXITED) == 0
            && OptionsScope::GlobalServer.with_local(|oo| options_get_number(oo, c"utmp")) != 0
        {
            pty.utmp_add(pane.id);
        } else {
            pty.utmp_remove();
        }
    }

    unsafe fn reset_border_cache(&self) {
        let pane = &mut *self.get();
        pane.border_gc_set = 0;
        pane.active_border_gc_set = 0;
    }
    unsafe fn border_cell(&self, cell_type: i32, cell: &mut grid_cell) {
        super::border::cell(self, cell_type, cell);
    }
    unsafe fn border_style(&self, client: &ClientRef) -> grid_cell {
        super::border::border_style(self, client)
    }
    unsafe fn make_status(
        &self,
        client: &ClientRef,
        width: u32,
        spans: &crate::shared::redraw::redraw_spans,
        first: usize,
    ) -> bool {
        super::border::make_status(self, client, width, spans, first)
    }
    unsafe fn has_new_status(&self) -> bool {
        (*self.get()).flags & PANE_NEWSTATUS != 0
    }
    unsafe fn set_new_status(&self, redraw: bool) {
        if redraw {
            (*self.get()).flags |= PANE_NEWSTATUS;
        } else {
            (*self.get()).flags &= !PANE_NEWSTATUS;
        }
    }
    unsafe fn default_colours(&self) -> (grid_cell, u32) {
        super::render::default_colours(self)
    }
    unsafe fn is_obscured(&self) -> bool {
        super::render::is_obscured(self)
    }
    unsafe fn visible_ranges(
        base: Option<&Self>,
        x: i32,
        y: i32,
        width: u32,
        ranges: &mut Vec<visible_range>,
    ) {
        super::render::visible_ranges(base, x, y, width, ranges);
    }
    unsafe fn window_size(&self) -> (u32, u32) {
        let window = self.window_observer().upgrade().expect("live pane parent");
        let size = window.logical_size();
        drop(window);
        size
    }
    unsafe fn schedule_offset_update(&self) {
        let window = self.window_observer().upgrade().expect("live pane parent");
        window.schedule_offset_update();
        drop(window);
    }
    unsafe fn draws_inactive_screen(&self) -> bool {
        let window = self.window_observer().upgrade().expect("live pane parent");
        let inactive = !window.active_pane().ptr_eq(&Rc::downgrade(self));
        drop(window);
        inactive || !matches!((*self.get()).screen_source, PaneScreenSource::Base)
    }
    unsafe fn output_suppressed(&self) -> bool {
        (*self.get()).flags & (PANE_REDRAW | PANE_DROP) != 0
    }
    unsafe fn alternate_screen_allowed(&self) -> bool {
        options_get_number(&self.borrow_options_mut(), c"alternate-screen") != 0
    }
    unsafe fn alternate_screen_changed(&self, entered: bool) {
        super::render::alternate_screen_changed(self, entered);
    }
    unsafe fn draw_line(
        &self,
        client: &ClientRef,
        source: (u32, u32),
        width: u32,
        destination: (u32, u32),
        status: bool,
    ) {
        super::render::draw_line(self, client, source, width, destination, status);
    }
    unsafe fn draw_scrollbar(
        &self,
        client: &ClientRef,
        span: &crate::shared::redraw::redraw_span,
        x: u32,
        y: u32,
        width: u32,
    ) {
        super::render::draw_scrollbar(self, client, span, x, y, width);
    }
    unsafe fn draw_prompt(&self, context: &mut crate::screen_redraw::redraw_draw_ctx<'_>) {
        super::render::draw_prompt(self, context);
    }

    fn find_by_id(id: u32) -> Option<Self> {
        window_pane_find_by_id(id)
    }
    fn find_by_id_str(id: &CStr) -> Option<Self> {
        window_pane_find_by_id_str(id)
    }
    unsafe fn from_observer(observer: &std::rc::Weak<UnsafeCell<window_pane>>) -> Option<Self> {
        window_pane_upgrade(observer)
    }
    unsafe fn tty_name(&self) -> CString {
        (*self.get()).tty().to_owned()
    }
    unsafe fn control_colours(&self) -> (i32, i32) {
        ((*self.get()).control_fg, (*self.get()).control_bg)
    }
    unsafe fn update_control_colours(&self, fg: i32, bg: i32) {
        let pane = &mut *self.get();
        if bg != pane.control_bg {
            pane.flags |= PANE_THEMECHANGED;
        }
        pane.control_fg = fg;
        pane.control_bg = bg;
    }
    unsafe fn has_prompt(&self) -> bool {
        (*self.get()).prompt.is_some()
    }
    unsafe fn set_prompt(
        &self,
        client: Option<&ClientRef>,
        find: Option<&cmd_find_state>,
        message: &CStr,
        input: Option<&CStr>,
        inputcb: status_prompt_input_cb,
        freecb: prompt_free_cb,
        flags: i32,
        kind: prompt_type,
    ) {
        window_pane_set_prompt(
            self, client, find, message, input, inputcb, freecb, flags, kind,
        );
    }
    unsafe fn update_prompt(&self, message: &CStr, input: Option<&CStr>) {
        window_pane_update_prompt(self, message, input);
    }
    unsafe fn paste_buffer(&self, bytes: &[u8], separator: &[u8], bracket: bool, raw: bool) {
        let pane = &*self.get();
        if pane.flags & PANE_INPUTOFF != 0 {
            return;
        }
        let bracket = bracket
            && pane
                .with_screen(|screen| screen.mode & crate::shared::screen::MODE_BRACKETPASTE != 0)
                .expect("displayed pane screen");
        let write = |bytes: &[u8]| {
            let _ = pane
                .event
                .with_ptr(|event| crate::reactor::bufferevent_write(event, bytes));
        };
        if bracket {
            write(b"\x1b[200~");
        }
        for chunk in bytes.split_inclusive(|&byte| byte == b'\n') {
            let line = chunk.strip_suffix(b"\n").unwrap_or(chunk);
            if raw {
                write(line);
            } else {
                let escaped = crate::text::utf8::utf8_stravisx_bytes(
                    line,
                    crate::shared::vis::VIS_SAFE | crate::shared::vis::VIS_NOSLASH,
                );
                write(&escaped);
            }
            if line.len() != chunk.len() {
                write(separator);
            }
        }
        if bracket {
            write(b"\x1b[201~");
        }
    }
    unsafe fn capture(
        &self,
        arguments: &args,
        item: &Rc<std::cell::RefCell<cmdq_item>>,
    ) -> Result<Vec<u8>, CString> {
        super::capture::capture(self, arguments, item)
    }
    unsafe fn clear_history(&self, reset_links: bool) {
        super::capture::clear_history(self, reset_links);
    }
    unsafe fn refresh_palette(&self) {
        crate::style::colour::colour_palette_from_option(
            Some(&mut (*self.get()).palette),
            &OptionsScope::Pane(Rc::downgrade(self)),
        );
    }
    unsafe fn mark_changed(&self) {
        (*self.get()).flags |= PANE_CHANGED;
    }
    unsafe fn mark_style_changed(&self, theme: bool) {
        (*self.get()).flags |= PANE_STYLECHANGED;
        if theme {
            (*self.get()).flags |= PANE_THEMECHANGED;
        }
    }
    unsafe fn invalidate_style(&self) {
        (*self.get()).flags |= PANE_REDRAW | PANE_STYLECHANGED | PANE_THEMECHANGED;
    }
    unsafe fn set_input_enabled(&self, enabled: bool) {
        if enabled {
            (*self.get()).flags &= !PANE_INPUTOFF;
        } else {
            (*self.get()).flags |= PANE_INPUTOFF;
        }
    }
    unsafe fn set_title(&self, title: &CStr) -> bool {
        crate::screen::screen_set_title(&mut (*self.get()).base, title, 0) != 0
    }
    unsafe fn trim_history(&self) {
        let pane = &mut *self.get();
        if !pane.modes.is_empty() {
            return;
        }
        let adjust = pane
            .base
            .grid()
            .sy
            .wrapping_sub(1)
            .wrapping_sub(pane.base.cy)
            .min(pane.base.grid().hsize);
        crate::grid::grid_remove_history(pane.base.grid_mut(), adjust);
        pane.base.cy = pane.base.cy.wrapping_add(adjust);
        pane.flags |= PANE_REDRAW;
    }
    unsafe fn wait_until_close(&self, waiter: CmdqWaiter) {
        (*self.get()).wait_close = Some(waiter);
    }
    unsafe fn start_input(
        &self,
        item: &Rc<std::cell::RefCell<cmdq_item>>,
    ) -> Result<Option<impl std::future::Future<Output = ()> + 'static>, CString> {
        window_pane_start_input(self, item)
    }
    unsafe fn apply_layout(
        &self,
        geometry: layout_geometry,
        scrollbars: &crate::shared::window::WindowRef,
    ) -> bool {
        let old_geometry = self.geometry();
        let (mut sx, mut sy) = (geometry.sx, geometry.sy);
        let (mut xoff, mut yoff) = (geometry.xoff, geometry.yoff);
        let status = self.border_status();
        let at_edge = match status {
            PANE_STATUS_TOP => yoff == 0,
            PANE_STATUS_BOTTOM => (yoff as u32).saturating_add(sy) >= scrollbars.size().1,
            _ => false,
        };
        if at_edge && sy > 1 {
            if status == PANE_STATUS_TOP {
                yoff += 1;
            }
            sy -= 1;
        }
        let reserve = window_pane_scrollbar_reserve(&*self.get()) != 0;
        if reserve {
            let (width, pad) = {
                let pane = &*self.get();
                (
                    pane.scrollbar_style.width.max(1),
                    pane.scrollbar_style.pad.max(0),
                )
            };
            if scrollbars.scrollbar_position() == PANE_SCROLLBARS_LEFT {
                if sx as i32 - width - pad < PANE_MINIMUM {
                    xoff += sx as i32 - PANE_MINIMUM;
                    sx = PANE_MINIMUM as u32;
                } else {
                    sx = sx.wrapping_sub(width as u32).wrapping_sub(pad as u32);
                    xoff += width + pad;
                }
            } else if sx as i32 - width - pad < PANE_MINIMUM {
                sx = PANE_MINIMUM as u32;
            } else {
                sx = sx.wrapping_sub(width as u32).wrapping_sub(pad as u32);
            }
        }
        {
            let pane = &mut *self.get();
            pane.xoff = xoff;
            pane.yoff = yoff;
            if reserve {
                pane.flags |= PANE_REDRAWSCROLLBAR;
            }
        }
        self.resize(sx, sy);
        self.geometry() != old_geometry
    }
    unsafe fn redraw_selection_change(&self) {
        let pane = &mut *self.get();
        if !grid_cells_look_equal(&pane.cached_gc, &pane.cached_active_gc)
            || pane.cached_dim != pane.cached_active_dim
            || colour_palette_get(Some(&pane.palette), pane.cached_gc.fg)
                != colour_palette_get(Some(&pane.palette), pane.cached_active_gc.fg)
            || colour_palette_get(Some(&pane.palette), pane.cached_gc.bg)
                != colour_palette_get(Some(&pane.palette), pane.cached_active_gc.bg)
        {
            pane.flags |= PANE_REDRAW;
        }
    }
    unsafe fn outer_geometry(&self) -> (i32, i32, u32, u32) {
        window_pane_full_size_offset(self)
    }
    unsafe fn pane_lines(&self) -> pane_lines {
        window_pane_get_pane_lines(&*self.get())
    }
    type LayoutMeta<'a> = &'a mut Option<Box<dyn std::any::Any>>;
    unsafe fn borrow_layout_meta(&self) -> Self::LayoutMeta<'_> {
        &mut (*self.get()).layout_meta
    }
    unsafe fn border_status(&self) -> i32 {
        window_pane_get_pane_status(&*self.get())
    }
    unsafe fn has_pending_change(&self) -> bool {
        (*self.get()).flags & PANE_CHANGED != 0
    }
    unsafe fn acknowledge_change(&self) {
        (*self.get()).flags &= !PANE_CHANGED;
    }
    unsafe fn has_unseen_changes(&self) -> bool {
        (*self.get()).flags & PANE_UNSEENCHANGES != 0
    }
    unsafe fn acknowledge_unseen_changes(&self) {
        (*self.get()).flags &= !PANE_UNSEENCHANGES;
    }
    unsafe fn default_window_name(&self) -> CString {
        let pane = &*self.get();
        let command = crate::cmd::cmd_stringify_argv_cstring(&pane.argv);
        let source = command
            .as_deref()
            .filter(|text| !text.to_bytes().is_empty())
            .unwrap_or_else(|| pane.shell.as_deref().expect("pane shell"));
        crate::names::parse_window_name_cstring(source)
    }
    type Palette<'a> = &'a colour_palette;
    type PaletteMut<'a> = &'a mut colour_palette;
    unsafe fn borrow_palette_mut(&self) -> Self::PaletteMut<'_> {
        &mut (*self.get()).palette
    }
    unsafe fn borrow_palette(&self) -> Self::Palette<'_> {
        &(*self.get()).palette
    }

    unsafe fn is_synchronized(&self) -> bool {
        (*self.get()).base.mode & MODE_SYNC != 0
    }
    unsafe fn start_sync(&self, base: &mut screen) {
        super::pane_sync::start(self, base);
    }
    unsafe fn stop_sync(&self, writing: Option<&mut screen>) {
        super::pane_sync::stop(self, writing);
    }
    unsafe fn clear_sync_dirty(&self) {
        super::pane_sync::clear_dirty(&mut *self.get());
    }
    unsafe fn should_draw_rows(&self, synchronized: bool, y: u32, count: u32, height: u32) -> bool {
        super::pane_sync::should_draw_rows(&mut *self.get(), synchronized, y, count, height)
    }

    unsafe fn id(&self) -> u32 {
        (*self.get()).id
    }

    unsafe fn window_observer(&self) -> WindowWeak {
        (*self.get()).window.clone()
    }
    unsafe fn reparent(&self, window: &WindowRef) {
        let state = &mut *self.get();
        state.window = WindowRef::downgrade(window);
        state.borrow_options_mut().parent = Some(crate::options::OptionsScope::Window(
            WindowRef::downgrade(window),
        ));
        state.flags |= PANE_STYLECHANGED | PANE_THEMECHANGED;
    }

    unsafe fn geometry(&self) -> (u32, u32, i32, i32) {
        let pane = &*self.get();
        (pane.sx, pane.sy, pane.xoff, pane.yoff)
    }

    unsafe fn mouse_position(&self, mouse: &mouse_event, last: bool) -> Option<(u32, u32)> {
        let (width, height, xoff, yoff) = self.geometry();
        let (mx, my) = if last {
            (mouse.lx, mouse.ly)
        } else {
            (mouse.x, mouse.y)
        };
        let x = mx.wrapping_add(mouse.ox);
        let mut y = my.wrapping_add(mouse.oy);
        if mouse.statusat == 0 && y >= mouse.statuslines {
            y = y.wrapping_sub(mouse.statuslines);
        }
        if (x as i32) < xoff
            || (x as i32) >= xoff + width as i32
            || (y as i32) < yoff
            || (y as i32) >= yoff + height as i32
        {
            return None;
        }
        Some((x.wrapping_sub(xoff as u32), y.wrapping_sub(yoff as u32)))
    }

    unsafe fn visible_cursor_in_window(&self) -> Option<(u32, u32)> {
        let pane = &*self.get();
        let screen = pane
            .with_screen(|screen| crate::shared::screen::ScreenMode::from(screen))
            .expect("displayed pane screen");
        (screen.mode & crate::shared::screen::MODE_CURSOR != 0).then_some((
            (pane.xoff as u32).wrapping_add(screen.cx),
            (pane.yoff as u32).wrapping_add(screen.cy),
        ))
    }

    unsafe fn has_tty(&self) -> bool {
        (*self.get()).fd.is_some()
    }

    unsafe fn resize(&self, sx: u32, sy: u32) {
        window_pane_resize(self, sx, sy);
    }

    unsafe fn update_focus(&self, focused: bool) {
        let pane = self.get();
        if (*pane).flags & PANE_EXITED != 0 {
            return;
        }
        let was_focused = (*pane).flags & PANE_FOCUSED != 0;
        if focused == was_focused {
            log_debug(format_args!(
                "window_pane_update_focus: %{} focus unchanged",
                self.id()
            ));
            return;
        }
        log_debug(format_args!(
            "window_pane_update_focus: %{} focus {}",
            self.id(),
            if focused { "in" } else { "out" }
        ));
        if (*pane).base.mode & MODE_FOCUSON != 0 {
            let _ = (*pane)
                .event
                .write(if focused { b"\x1b[I" } else { b"\x1b[O" });
        }
        events_fire_pane(
            if focused {
                c"pane-focus-in"
            } else {
                c"pane-focus-out"
            },
            Rc::clone(self),
        );
        if focused {
            (*pane).flags |= PANE_FOCUSED;
        } else {
            (*pane).flags &= !PANE_FOCUSED;
        }
    }

    unsafe fn on_selected(&self, record_activity: bool) {
        if record_activity {
            (*self.get()).active_point = window_pane_tree::next_active_point();
        }
        (*self.get()).flags |= PANE_CHANGED;
    }

    unsafe fn prepare_render(
        &self,
        context: &mut crate::shared::tty::tty_ctx,
        window_redraw: bool,
    ) -> i32 {
        let pane = self.get();
        if !(*pane)
            .window_handle()
            .is_some_and(|window| window.contains_pane(&Rc::downgrade(self)))
        {
            return 0;
        }
        if (*pane).flags & (PANE_REDRAW | crate::shared::pane::PANE_DROP) != 0 {
            return -1;
        }
        if window_redraw {
            log_debug(format_args!(
                "screen_write_set_client_cb: adding %{} to deferred redraw",
                self.id()
            ));
            self.request_redraw(true);
            return -1;
        }
        context.rxoff = (*pane).xoff;
        context.xoff = context.rxoff;
        context.ryoff = (*pane).yoff;
        context.yoff = context.ryoff;
        1
    }

    unsafe fn request_redraw(&self, scrollbar: bool) {
        (*self.get()).flags |= PANE_REDRAW;
        if scrollbar {
            (*self.get()).flags |= PANE_REDRAWSCROLLBAR;
        }
    }

    unsafe fn key(
        &self,
        client: Option<&ClientRef>,
        link: refbox::Weak<winlink>,
        key: key_code,
        mouse: Option<&mut mouse_event>,
    ) -> i32 {
        window_pane_key(self, client, link, key, mouse)
    }

    unsafe fn paste(&self, key: key_code, bytes: &[u8]) {
        window_pane_paste(self, key, bytes);
    }

    unsafe fn set_mode(
        &self,
        source: Option<&Rc<UnsafeCell<window_pane>>>,
        mode: &'static window_mode,
        item: Option<&Rc<std::cell::RefCell<cmdq_item>>>,
        find: Option<&cmd_find_state>,
        arguments: Option<&args>,
    ) -> i32 {
        window_pane_set_mode(self, source, mode, item, find, arguments)
    }

    unsafe fn reset_mode(&self) {
        window_pane_reset_mode(self);
    }
    unsafe fn reset_all_modes(&self) {
        window_pane_reset_mode_all(self);
    }

    unsafe fn copy_screen(
        &self,
        destination: &mut screen,
        x: u32,
        y: u32,
        sx: u32,
        sy: u32,
        displayed: bool,
    ) {
        let pane = &*self.get();
        let mut copy = |source: &screen| {
            assert!(
                !std::ptr::eq(source, destination),
                "screen copy cannot alias its source"
            );
            assert!(x <= source.grid().sx && sx <= source.grid().sx - x);
            assert!(y <= source.grid().sy && sy <= source.grid().sy - y);
            let mut cells = Vec::with_capacity(sx as usize * sy as usize);
            let mut links = std::collections::HashMap::new();
            for row in 0..sy {
                for column in 0..sx {
                    let mut cell = grid_cell::default();
                    grid_get_cell(
                        source.grid(),
                        x + column,
                        source.grid().hsize + y + row,
                        &mut cell,
                    );
                    if crate::screen::screen_check_selection(source, x + column, y + row) != 0 {
                        if let Some(selected) = crate::screen::screen_select_cell(source, &cell) {
                            cell = selected;
                        }
                    }
                    if cell.link != 0 {
                        links.entry(cell.link).or_insert_with(|| {
                            source.hyperlinks.as_ref().and_then(|table| {
                                hyperlinks_get(table, cell.link)
                                    .map(|link| (link.uri.clone(), link.internal_id.clone()))
                            })
                        });
                    }
                    cells.push(cell);
                }
            }
            drop(std::mem::take(destination));
            screen_init(destination, sx, sy, 0);
            destination.title = source.title.clone();
            destination.path = source.path.clone();
            destination.mode = source.mode;
            destination.cstyle = source.cstyle;
            destination.default_cstyle = source.default_cstyle;
            destination.ccolour = source.ccolour;
            destination.default_ccolour = source.default_ccolour;
            destination.cx = source.cx.saturating_sub(x).min(sx.saturating_sub(1));
            destination.cy = source.cy.saturating_sub(y).min(sy.saturating_sub(1));
            if source.cx < x || source.cy < y || source.cx >= x + sx || source.cy >= y + sy {
                destination.mode &= !crate::shared::screen::MODE_CURSOR;
            }
            let links: std::collections::HashMap<_, _> = links
                .into_iter()
                .map(|(id, text)| {
                    let copied = text.map_or(0, |(uri, internal_id)| {
                        hyperlinks_put(
                            destination
                                .hyperlinks
                                .as_ref()
                                .expect("copy hyperlink table"),
                            &uri,
                            Some(&internal_id),
                        )
                    });
                    (id, copied)
                })
                .collect();
            for (index, mut cell) in cells.into_iter().enumerate() {
                if cell.link != 0 {
                    cell.link = links[&cell.link];
                }
                grid_set_cell(
                    destination.grid_mut(),
                    index as u32 % sx,
                    index as u32 / sx,
                    &cell,
                );
            }
        };
        if displayed {
            pane.with_screen(copy).expect("displayed pane screen");
        } else {
            copy(&pane.base);
        }
    }

    unsafe fn copy_output(&self, offset: &window_pane_offset, destination: &mut [u8]) -> usize {
        let pane = &*self.get();
        let used = offset.used.wrapping_sub(pane.base_offset);
        match pane.event.copy_input(used, destination) {
            Ok(copied) => copied,
            Err(crate::reactor::StreamError::Freed) => 0,
            Err(crate::reactor::StreamError::InvalidRange) => {
                panic!("pane offset is within input buffer")
            }
        }
    }

    unsafe fn output_offset(&self) -> window_pane_offset {
        (*self.get()).offset
    }

    unsafe fn advance_output(&self, offset: &mut window_pane_offset, count: usize) {
        let pane = &*self.get();
        if let Ok(available) = pane.event.input_len() {
            let used = offset.used.wrapping_sub(pane.base_offset);
            offset.used = offset
                .used
                .wrapping_add(count.min(available.saturating_sub(used)));
        }
    }

    unsafe fn finish_cycle(&self) {
        if (*self.get()).fd.is_some() {
            finish_resize(self);
            finish_buffer(self);
        }
        (*self.get()).flags &= !(PANE_REDRAW | PANE_REDRAWSCROLLBAR | PANE_ACTIVITY);
    }

    unsafe fn borrow_options_mut(&self) -> std::cell::RefMut<'_, options> {
        (*self.get()).borrow_options_mut()
    }

    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut crate::shared::format::format_tree,
    ) -> Option<crate::format::FormatValue> {
        super::format::format_value(self, key, context)
    }

    unsafe fn pipe(
        &self,
        command: refbox::Weak<cmd>,
        item: &Rc<std::cell::RefCell<cmdq_item>>,
    ) -> CmdOutcome {
        process::pipe_pane(self, command, item)
    }
    unsafe fn process_id(&self) -> pid_t {
        (*self.get()).pid
    }
    unsafe fn process_exited(&self, pid: pid_t, status: std::process::ExitStatus) -> bool {
        lifecycle::process_exited(self, pid, status)
    }
    unsafe fn kill_process(&self) {
        lifecycle::kill_process(self)
    }
    unsafe fn finish_process(&self, notify: i32) {
        lifecycle::finish_process(self, notify)
    }
    unsafe fn spawn_process(
        context: &mut crate::shared::spawn::spawn_context,
        cause: &mut Option<CString>,
    ) -> Option<Self> {
        spawning::spawn_pane(context, cause)
    }
    unsafe fn install_editor(
        &self,
        editor: Box<crate::shared::spawn::spawn_editor_state>,
    ) -> crate::shared::spawn::EditorHandle {
        spawning::install_editor(self, editor)
    }
    unsafe fn finish_editing(&self) {
        spawning::finish_editing(self)
    }
    unsafe fn editor_identity(&self) -> Option<crate::shared::spawn::EditorId> {
        (*self.get()).editor.as_ref().map(|editor| editor.id)
    }
    unsafe fn cancel_editor(&self, identity: crate::shared::spawn::EditorId) {
        let pane = &mut *self.get();
        if let Some(editor) = pane.editor.as_mut().filter(|editor| editor.id == identity) {
            editor.cb = None;
        }
    }
    unsafe fn editor_process_id(&self, identity: crate::shared::spawn::EditorId) -> pid_t {
        (*self.get())
            .editor
            .as_ref()
            .filter(|editor| editor.id == identity)
            .map_or(-1, |editor| editor.pid)
    }
    unsafe fn write_key(&self, key: key_code, mouse: Option<&mouse_event>) -> i32 {
        super::keys::write_key(self, key, mouse)
    }
    unsafe fn is_mode(&self, mode: &'static window_mode) -> bool {
        let entry = self.mode_entry();
        entry.is_alive() && std::ptr::eq(entry.get_unchecked().mode, mode)
    }
    unsafe fn add_mode_formats(&self, context: &mut crate::shared::format::format_tree) {
        let entry = self.mode_entry();
        let callback = entry
            .is_alive()
            .then(|| entry.get_unchecked().mode.formats)
            .flatten();
        if let Some(callback) = callback {
            callback(entry, context);
        }
    }
    unsafe fn reset_input(&self) {
        let context = {
            let pane = &mut *self.get();
            crate::style::colour::colour_palette_clear(Some(&mut pane.palette));
            pane.ictx.as_deref_mut().expect("pane input context")
        };
        super::input::input_reset(context, 1);
        (*self.get()).flags |= PANE_STYLECHANGED | PANE_THEMECHANGED | PANE_REDRAW;
    }
    unsafe fn screen_size(&self, displayed: bool) -> (u32, u32) {
        let pane = &*self.get();
        let size = |source: &screen| (source.grid().sx, source.grid().sy);
        if displayed {
            pane.with_screen(size).expect("displayed pane screen")
        } else {
            size(&pane.base)
        }
    }
    unsafe fn clone_history(
        &self,
        hint: &screen,
        cursor: Option<(&mut u32, &mut u32)>,
        trim: bool,
    ) -> Box<screen> {
        crate::window_copy::window_copy_clone_screen(&(*self.get()).base, hint, cursor, trim)
    }
    unsafe fn sync_history(&self, target: &mut screen, sync: (u32, u32, u32)) -> bool {
        crate::window_copy::window_copy_sync_screen(&(*self.get()).base, target, sync)
    }
    unsafe fn history_scroll(&self) -> (u32, u32, u32) {
        let grid = (*self.get()).base.grid();
        (
            grid.scroll_added,
            grid.scroll_collected,
            grid.scroll_generation,
        )
    }
    unsafe fn share_screen_hyperlinks(&self, target: &mut screen) {
        let source = &(*self.get()).base;
        if source.hyperlinks.is_some() {
            crate::screen::screen_share_hyperlinks(target, source);
        }
    }
    unsafe fn grid_hyperlink(
        &self,
        grid: &crate::shared::grid::grid,
        x: u32,
        y: u32,
    ) -> Option<CString> {
        (*self.get())
            .with_screen(|screen| crate::format::format_grid_hyperlink_cstring(grid, x, y, screen))
            .expect("displayed pane screen")
    }
    unsafe fn saved_search(&self) -> (Option<CString>, i32) {
        let pane = &*self.get();
        (pane.searchstr.clone(), pane.searchregex)
    }
    unsafe fn save_search(&self, value: CString, regex: i32) {
        let pane = &mut *self.get();
        pane.searchstr = Some(value);
        pane.searchregex = regex;
    }
    unsafe fn show_scrollbar(&self) {
        window_pane_scrollbar_show(self);
    }
    unsafe fn take_pending_redraw(&self) -> bool {
        let pane = &mut *self.get();
        let pending = pane.flags & PANE_REDRAW != 0;
        pane.flags &= !PANE_REDRAW;
        pending
    }
    unsafe fn process_name(&self) -> Option<CString> {
        let descriptor = (*self.get()).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd);
        crate::osdep::osdep_get_name_cstring(descriptor)
    }
    unsafe fn draw_editor_waiting(&self, pid: pid_t) {
        super::mode_visuals::draw_editor_waiting(self, pid);
    }
    unsafe fn search(&self, term: &CStr, regex: bool, ignore: bool) -> u32 {
        window_pane_search(&*self.get(), term, regex as i32, ignore as i32)
    }
    unsafe fn status_range(&self, x: u32, y: u32) -> Option<style_range> {
        window_pane_status_get_range(self, x, y)
    }
    unsafe fn prompt_key(
        &self,
        client: Option<&ClientRef>,
        key: key_code,
        mouse: Option<&mut mouse_event>,
    ) -> prompt_key_result {
        window_pane_prompt_key(self, client, key, mouse)
    }
    unsafe fn notify_moved(
        &self,
        old_window: &WindowRef,
        old_index: i32,
        new_window: &WindowRef,
        new_index: i32,
    ) {
        window_fire_pane_moved(self, old_window, old_index, new_window, new_index);
    }
    unsafe fn destroy_ready(&self) -> bool {
        window_pane_destroy_ready(self) != 0
    }
    unsafe fn destroy(&self) {
        window_pane_destroy(self);
    }
}

unsafe fn finish_resize(owner: &Rc<UnsafeCell<window_pane>>) {
    let pane = owner.get();
    if (*pane).resize_queue.is_empty() {
        return;
    }
    if (*pane).resize_timer.is_some() {
        return;
    }
    log_debug(format_args!(
        "server_client_check_pane_resize: %{} needs to be resized",
        owner.id()
    ));
    let (sx, sy, keep) = {
        let queue = &(*pane).resize_queue;
        for resize in queue {
            log_debug(format_args!(
                "queued resize: {}x{} -> {}x{}",
                resize.osx, resize.osy, resize.sx, resize.sy
            ));
        }
        let first = queue.front().expect("nonempty resize queue");
        let last = queue.back().expect("nonempty resize queue");
        if queue.len() == 1 || last.sx != first.osx || last.sy != first.osy {
            (last.sx, last.sy, std::ptr::null_mut())
        } else {
            let previous = &queue[queue.len() - 2];
            (
                previous.sx,
                previous.sy,
                (&**last as *const window_pane_resize).cast_mut(),
            )
        }
    };
    window_pane_send_resize(&*pane, sx, sy);
    (*pane).clear_resizes_except(keep);
    let delay = Duration::from_micros((if keep.is_null() { 250000 } else { 10000 }) as u64);
    let observer = Rc::downgrade(owner);
    (*pane).resize_timer = Some(
        Timer::new(delay, move || {
            if let Some(owner) = observer.upgrade() {
                log_debug(format_args!(
                    "server_client_resize_timer: %{} resize timer expired",
                    owner.id()
                ));
                drop((*owner.get()).resize_timer.take());
            }
        })
        .expect("arm timer"),
    );
}

unsafe fn finish_buffer(owner: &Rc<UnsafeCell<window_pane>>) {
    let pane = owner.get();
    let mut minimum = (*pane).offset.used;
    if (*pane).pipe_fd.is_some() {
        minimum = minimum.min((*pane).pipe_offset.used);
    }
    let mut off = true;
    let mut attached = false;
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if client.attached_session().upgrade().is_some() {
            attached = true;
            if !client.is_control() {
                off = false;
            } else {
                let (offset, blocked) = client
                    .with_output_offset(owner.id(), |offset, blocked| (offset.copied(), blocked));
                if !blocked {
                    off = false;
                }
                if let Some(offset) = offset {
                    minimum = minimum.min(offset.used);
                }
            }
        }
        cursor = ClientRegistry::next(&client);
    }
    if !attached {
        off = false;
    }
    let count = minimum.wrapping_sub((*pane).base_offset);
    if count != 0 {
        log_debug(format_args!(
            "server_client_check_pane_buffer: %{} has {} minimum (of {}) bytes used",
            owner.id(),
            count,
            (*pane).event.input_len().unwrap_or(0)
        ));
        let _ = (*pane).event.drain_input(count);
        if (*pane).base_offset > usize::MAX.wrapping_sub(count) {
            let base = (*pane).base_offset;
            (*pane).offset.used = (*pane).offset.used.wrapping_sub(base);
            if (*pane).pipe_fd.is_some() {
                (*pane).pipe_offset.used = (*pane).pipe_offset.used.wrapping_sub(base);
            }
            let mut cursor = ClientRegistry::first();
            while let Some(client) = cursor {
                if client.attached_session().upgrade().is_some() && client.is_control() {
                    client.with_output_offset(owner.id(), |offset, blocked| {
                        if !blocked {
                            if let Some(offset) = offset {
                                offset.used = offset.used.wrapping_sub(base);
                            }
                        }
                    });
                }
                cursor = ClientRegistry::next(&client);
            }
            (*pane).base_offset = count;
        } else {
            (*pane).base_offset = (*pane).base_offset.wrapping_add(count);
        }
    }
    log_debug(format_args!(
        "server_client_check_pane_buffer: pane %{} is {}",
        owner.id(),
        if off { "off" } else { "on" }
    ));
    if off {
        let _ = (*pane).event.disable(Interests::READ);
    } else {
        let _ = (*pane).event.enable(Interests::READ);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::grid::grid_create;
    use crate::reactor::{evbuffer_add, StreamHandle};

    unsafe fn buffered_pane(bytes: &[u8]) -> Rc<UnsafeCell<window_pane>> {
        let pane = window_pane::new();
        (*pane.get()).fd = None;
        (*pane.get()).pipe_fd = None;
        let stream = bufferevent_new(-1, None, None, None).unwrap();
        (*pane.get()).event = StreamHandle::from_ptr(stream);
        evbuffer_add(&mut (*stream).input, bytes);
        pane
    }

    #[test]
    fn fallback_selection_preserves_the_last_activity_point() {
        unsafe {
            let pane = window_pane::new();
            (*pane.get()).fd = None;
            (*pane.get()).pipe_fd = None;
            let previous = window_pane_tree::peek_active_point();
            pane.on_selected(true);
            assert_eq!((*pane.get()).active_point, previous);
            assert_eq!(
                window_pane_tree::peek_active_point(),
                previous.wrapping_add(1)
            );
            (*pane.get()).flags &= !PANE_CHANGED;
            pane.on_selected(false);
            assert_eq!((*pane.get()).active_point, previous);
            assert_eq!(
                window_pane_tree::peek_active_point(),
                previous.wrapping_add(1)
            );
            assert_ne!((*pane.get()).flags & PANE_CHANGED, 0);
            drop(pane);
        }
    }

    #[test]
    fn output_cursors_preserve_unread_bytes_and_clamp_skips() {
        unsafe {
            let pane = buffered_pane(b"abcdef");
            (*pane.get()).base_offset = usize::MAX - 2;
            let mut first = window_pane_offset {
                used: usize::MAX - 2,
            };
            let second = first;
            let mut bytes = [0; 3];
            assert_eq!(pane.copy_output(&first, &mut bytes), 3);
            assert_eq!(&bytes, b"abc");
            pane.advance_output(&mut first, 3);
            assert_eq!(first.used, 0);
            assert_eq!(pane.copy_output(&first, &mut bytes), 3);
            assert_eq!(&bytes, b"def");
            assert_eq!(pane.copy_output(&second, &mut bytes), 3);
            assert_eq!(&bytes, b"abc");
            pane.advance_output(&mut first, usize::MAX);
            assert_eq!(first.used, 3);
            assert_eq!(pane.copy_output(&first, &mut bytes), 0);
            std::mem::take(&mut (*pane.get()).event).free();
            assert_eq!(pane.copy_output(&first, &mut bytes), 0);
            pane.advance_output(&mut first, usize::MAX);
            assert_eq!(first.used, 3);
            drop(pane);
        }
    }

    #[test]
    fn copied_screen_and_hyperlinks_survive_source_changes() {
        unsafe {
            let definition = crate::options::options_table
                .iter()
                .find(|entry| entry.name == Some(c"extended-keys"))
                .unwrap();
            OptionsScope::GlobalServer.reset_default(definition);
            let pane = window_pane::new();
            (*pane.get()).fd = None;
            (*pane.get()).pipe_fd = None;
            let source = &mut (*pane.get()).base;
            source.grid = Some(grid_create(3, 2, 0));
            source.hyperlinks = Some(crate::hyperlinks::hyperlinks_init());
            source.mode = crate::shared::screen::MODE_CURSOR;
            source.cx = 2;
            source.cy = 1;
            let mut cell = grid_cell::default();
            crate::text::utf8::utf8_set(&mut cell.data, b'X');
            cell.link = hyperlinks_put(
                source.hyperlinks.as_ref().unwrap(),
                c"https://example.test/pane",
                Some(c"pane-link"),
            );
            grid_set_cell(source.grid_mut(), 1, 1, &cell);
            let mut copied = screen::empty();
            pane.copy_screen(&mut copied, 1, 1, 2, 1, true);
            assert_eq!(
                (copied.grid().sx, copied.grid().sy, copied.cx, copied.cy),
                (2, 1, 1, 0)
            );
            grid_set_cell((*pane.get()).base.grid_mut(), 1, 1, &grid_cell::default());
            crate::hyperlinks::hyperlinks_reset((*pane.get()).base.hyperlinks.as_ref().unwrap());
            drop(pane);
            let mut copied_cell = grid_cell::default();
            grid_get_cell(copied.grid(), 0, 0, &mut copied_cell);
            assert_eq!(copied_cell.data.data[0], b'X');
            assert_eq!(
                hyperlinks_get(copied.hyperlinks.as_ref().unwrap(), copied_cell.link)
                    .unwrap()
                    .uri
                    .as_c_str(),
                c"https://example.test/pane"
            );
            drop(copied);
        }
    }

    #[test]
    fn destroy_closes_streams_before_the_last_holder_is_dropped() {
        unsafe {
            let pane = buffered_pane(b"pending");
            let stream = (*pane.get()).event.clone();
            let observer = Rc::downgrade(&pane);
            window_pane_tree::insert(Rc::clone(&pane));
            pane.destroy();
            assert!(!stream.is_alive());
            assert!(window_pane_upgrade(&observer).is_none());
            assert!(observer.upgrade().is_some());
            drop(pane);
            assert!(observer.upgrade().is_none());
        }
    }
}
