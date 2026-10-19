//! Operations on retained clients. Legacy dispatch remains explicitly unsafe:
//! an Rc keeps storage alive, but does not make reentrant model borrows exclusive.
use crate::options::OptionsScope;
use crate::shared::window::WindowWeak;

use super::*;
use crate::control::{control_get_window_size, control_write_output};
use crate::reactor::BufferEvent;
use crate::session::Session;
use crate::shared::client::ClientRef;
use crate::shared::control::control_state;
use crate::shared::display::visible_ranges;
use crate::shared::environment::environ;
use crate::shared::layout::Direction;
use crate::shared::screen::screen;
use crate::shared::session::{SessionRef, SessionWeak};
use crate::shared::terminal::termios;
use crate::shared::window::WindowRef;
use crate::window::Window;
use std::cell::UnsafeCell;
use std::os::fd::{AsRawFd, OwnedFd};
use std::rc::{Rc, Weak};
use std::time::{Duration, SystemTime};

/// The caller serializes access on the server thread and releases model and
/// component borrows before callbacks. Logical client loss remains explicit;
/// explicit owner-release sites must still release their retained reference.
pub trait Client {
    /// Create and register a protocol client, preserving the registry's owner.
    unsafe fn create(fd: OwnedFd) -> Self
    where
        Self: Sized;
    unsafe fn attached_count() -> u32
    where
        Self: Sized;
    /// Run the existing server-cycle checks over the retained client registry.
    unsafe fn run_cycle()
    where
        Self: Sized;
    /// Select the directory for an optional client using startup and session
    /// precedence. The returned value owns its bytes across owner release.
    unsafe fn working_directory(
        client: Option<&Self>,
        fallback: Option<&SessionRef>,
    ) -> Option<CString>
    where
        Self: Sized;
    unsafe fn print_to(client: Option<&Self>, parse: bool, buffer: &mut SegmentedBuf)
    where
        Self: Sized;
    unsafe fn forget_pane(pane: &Rc<UnsafeCell<window_pane>>)
    where
        Self: Sized;
    unsafe fn open_terminal(&self) -> Result<(), CString>;
    unsafe fn parse_flags(&self, flags: &CStr);
    unsafe fn update_theme_colours(&self);
    unsafe fn handle_key(&self, event: Box<key_event>) -> i32;

    type FormatJobsMut<'a>: std::ops::DerefMut<Target = crate::shared::format::format_job_tree>
    where
        Self: 'a;
    /// Immediate cache edits only. Release before expansion, process startup,
    /// job_free, or status notification. Callback identity must not be a pointer
    /// into this cache; removed entries retain their explicit cleanup duty.
    unsafe fn borrow_format_jobs_mut(&self) -> Self::FormatJobsMut<'_>;
    type QueueMut<'a>: std::ops::DerefMut<Target = crate::cmd::queue::cmdq_list>
    where
        Self: 'a;
    /// Release before executing commands, invoking cancellation, or releasing
    /// queued owners. Queue identity must never be a pointer into this borrow.
    unsafe fn borrow_queue_mut(&self) -> Self::QueueMut<'_>;
    type Terminal<'a>: std::ops::Deref<Target = tty>
    where
        Self: 'a;
    type TerminalMut<'a>: std::ops::DerefMut<Target = tty>
    where
        Self: 'a;
    /// Read terminal properties within the guard; release it before model calls,
    /// terminal mutation, formatting, or callback dispatch.
    unsafe fn borrow_terminal(&self) -> Self::Terminal<'_>;
    /// Only component-only helpers may run under this guard. Release before
    /// terminal IO helpers that query Client, callback dispatch, or model calls.
    unsafe fn borrow_terminal_mut(&self) -> Self::TerminalMut<'_>;
    /// Enqueue terminal bytes and account for output within one model borrow.
    unsafe fn write_terminal(&self, bytes: &[u8]);
    unsafe fn terminal_fd(&self) -> i32;
    unsafe fn record_terminal_discard(&self, bytes: usize);
    /// Some(0) completes the redraw write; backpressure starts on a later write.
    unsafe fn acknowledge_terminal_redraw(&self, bytes: usize) -> Option<usize>;
    unsafe fn terminal_theme_colour(&self, index: usize) -> i32;
    /// Forget any drag in progress and its callbacks.
    unsafe fn reset_mouse_drag(&self);
    unsafe fn mouse_drag_active(&self) -> bool;
    unsafe fn mouse_slider_position(&self) -> i32;
    unsafe fn set_mouse_drag_callbacks(
        &self,
        update: Box<dyn FnMut(&mut mouse_event)>,
        release: Box<dyn FnOnce(&mut mouse_event)>,
    );
    /// Terminfo construction may query Client; copy its source before starting.
    unsafe fn terminal_description_source(&self) -> (Option<CString>, Vec<CString>);
    unsafe fn parse_terminal_features(&self, features: &CStr, separators: &CStr);
    unsafe fn terminal_feature_mask(&self) -> i32;
    unsafe fn record_terminal_type(&self, name: &CStr);
    unsafe fn set_control_size(&self, width: u32, height: u32);
    /// Clear explicit panning; given a window, only panning of that window.
    unsafe fn reset_pan(&self, window: Option<&WindowRef>);
    /// Apply and clamp this window's explicit pan, if active, to a viewport.
    /// Window dimensions are read before borrowing Client state.
    unsafe fn apply_pan(&self, window: &WindowRef, view: &mut tty_window_view) -> bool;
    unsafe fn pan_window(&self, window: &WindowRef, direction: Direction, amount: u32);
    type Status<'a>: std::ops::Deref<Target = crate::shared::status::status_line>
    where
        Self: 'a;
    type StatusMut<'a>: std::ops::DerefMut<Target = crate::shared::status::status_line>
    where
        Self: 'a;
    /// Release before formatting, querying Client, callbacks or screen replacement.
    unsafe fn borrow_status(&self) -> Self::Status<'_>;
    unsafe fn borrow_status_mut(&self) -> Self::StatusMut<'_>;
    type ControlMut<'a>: std::ops::DerefMut<Target = control_state>
    where
        Self: 'a;
    /// Release before calling models, formatting, dispatching callbacks, or
    /// stopping control mode. No component pointer may escape the guard.
    unsafe fn borrow_control_mut(&self) -> Option<Self::ControlMut<'_>>;
    unsafe fn start_control(&self);
    unsafe fn stop_control(&self);
    unsafe fn control_pause_after(&self) -> Option<u64>;
    /// Check reply pressure and publish exit/discard state before discarding
    /// pending pane output. Returns false for a stopped or discarding client.
    unsafe fn accept_control_reply(&self, added: usize) -> bool;
    unsafe fn attached_session(&self) -> SessionWeak;
    unsafe fn set_session(&self, session: Option<&SessionRef>);
    unsafe fn reattach_after_session_destroy(&self, target: Option<&SessionRef>);
    unsafe fn is_dead(&self) -> bool;
    unsafe fn is_control(&self) -> bool;
    unsafe fn is_read_only(&self) -> bool;
    unsafe fn exec(&self, command: &CStr);
    /// Whether this client currently receives control protocol notifications.
    unsafe fn receives_notifications(&self) -> bool;
    /// Formatting runs without a client borrow and may reenter the client.
    unsafe fn notify(&self, write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>);
    unsafe fn name(&self) -> Option<CString>;
    /// Observe the complete protocol/state flag word for legacy mask decisions.
    unsafe fn flags(&self) -> u64;
    /// Apply an immediate flag transition, setting bits before clearing bits.
    unsafe fn update_flags(&self, set: u64, clear: u64);
    unsafe fn activity_time(&self) -> SystemTime;
    unsafe fn creation_time(&self) -> SystemTime;
    unsafe fn tty_name(&self) -> Option<CString>;
    unsafe fn terminal_theme(&self) -> crate::shared::colour::client_theme;
    unsafe fn colour_escape(&self, colour: i32, background: bool) -> Option<CString>;
    unsafe fn is_nested(&self) -> bool;
    unsafe fn peer_uid(&self) -> uid_t;
    unsafe fn peer_gid(&self) -> gid_t;
    /// Publish readiness at the caller's existing point in attachment ordering.
    unsafe fn send_ready(&self);
    /// Copy a protocol message into the peer's output queue. This never exposes
    /// the peer or retains a model borrow during protocol processing.
    unsafe fn send_message(
        &self,
        kind: crate::compat::imsg::msgtype,
        fd: Option<OwnedFd>,
        data: &[u8],
    ) -> i32;
    unsafe fn register_file(&self, file: &Rc<std::cell::RefCell<crate::file::client_file>>);
    unsafe fn unregister_file(&self, stream: i32, identity: *const crate::file::client_file);
    /// Stream-ordered observations; each yield resolves the current holder.
    unsafe fn file_handles(
        &self,
    ) -> impl std::iter::FusedIterator<Item = Weak<std::cell::RefCell<crate::file::client_file>>> + 'static;
    unsafe fn find_file(&self, stream: i32) -> Weak<std::cell::RefCell<crate::file::client_file>>;
    /// Observe a request owned by input_ctx. Its owner must explicitly retire
    /// this observation before freeing the request.
    unsafe fn observe_input_request(&self, request: &mut crate::shared::input::input_request);
    unsafe fn forget_input_request(&self, request: &crate::shared::input::input_request);
    unsafe fn has_input_requests(&self) -> bool;
    /// Select in client-index order; no Client borrow spans parser replies or
    /// explicit request cleanup (which itself removes index observations).
    unsafe fn reply_input_request(&self, reply: crate::input::InputRequestReply<'_>);
    unsafe fn cancel_input_requests(&self);
    unsafe fn remember_session(&self);
    unsafe fn previous_session(&self) -> SessionWeak;
    unsafe fn has_input_fd(&self) -> bool;
    /// Capture terminal attributes before attaching; retain the existing fatal error policy.
    unsafe fn capture_termios(&self) -> termios;
    /// Reserve nesting before starting a source-file chain. Completion or
    /// cancellation must explicitly balance a successful reservation.
    unsafe fn enter_source_file(&self, limit: u32) -> Option<u32>;
    unsafe fn leave_source_file(&self) -> u32;

    /// Attachment accounting excludes suspended, dead and exiting clients.
    unsafe fn counts_as_attached(&self) -> bool;
    /// Deliver the audible/visual part after Session has deduplicated the alert.
    unsafe fn alert(&self, kind: &CStr, visual: i32, current: bool, index: i32);

    unsafe fn uses_legacy_layout_format(&self) -> bool;
    unsafe fn focuses_window(&self, window: &WindowRef) -> bool;
    unsafe fn participates_in_window_sizing(&self) -> bool;
    unsafe fn window_size(&self, window: Option<&WindowRef>) -> (u32, u32, u32, u32);
    unsafe fn constrain_window_size(&self, window: &WindowRef, sx: &mut u32, sy: &mut u32);
    /// `flags` contains only CLIENT_*REDRAW* bits, including status-force.
    unsafe fn request_redraw(&self, flags: u64);
    /// Ordered injection used by send-keys -K. The queue insertion point must
    /// survive until dispatch finishes, under the existing command lifetime rules.
    unsafe fn handle_key_after(
        &self,
        event: Box<key_event>,
        after: Option<&Rc<std::cell::RefCell<cmdq_item>>>,
        next: Option<&mut Weak<std::cell::RefCell<cmdq_item>>>,
    ) -> i32;
    unsafe fn control_write_output(&self, pane: &Rc<UnsafeCell<window_pane>>);
    /// Scoped access to this control client's consumer offset. Call only for an
    /// attached control client; the bool is the existing output-disabled flag.
    /// Do not reenter models, release owners, or let the reference escape.
    unsafe fn with_output_offset<R>(
        &self,
        pane: u32,
        read: impl FnOnce(Option<&mut window_pane_offset>, bool) -> R,
    ) -> R;
    unsafe fn set_return_value(&self, value: i32);
    unsafe fn request_exit(&self, value: i32);
    unsafe fn exit_with_message(&self, message: CString, return_value: Option<i32>);
    /// Preserve shutdown's immediate loss of suspended clients and otherwise
    /// queue the shutdown reason, then clear attachment without notifications.
    unsafe fn shutdown(&self);
    /// The closure cannot reenter models, destroy owners, or leak component
    /// references. Clone the environment before invoking another entity.
    unsafe fn with_environment<R>(&self, read: impl FnOnce(Option<&environ>) -> R) -> R;
    unsafe fn cwd(&self, fallback: Option<&SessionRef>) -> Option<CString>;
    unsafe fn set_key_table(&self, name: Option<&CStr>);
    /// Switch to an already resolved table without refreshing its activity time.
    unsafe fn select_key_table(&self, table: Rc<std::cell::RefCell<key_table>>);
    unsafe fn uses_key_table(&self, table: &Rc<std::cell::RefCell<key_table>>) -> bool;
    /// Observe the separately owned prompt, then borrow it only while needed.
    /// A callback may retire it and install another prompt on this client.
    unsafe fn prompt_observer(&self) -> refbox::Weak<crate::shared::prompt::prompt>;
    unsafe fn install_prompt(
        &self,
        prompt: refbox::RefBox<crate::shared::prompt::prompt>,
        flags: i32,
    );
    unsafe fn clear_prompt(&self);
    unsafe fn clear_status_message(&self);
    unsafe fn redraw_status_if_unobscured(&self);
    unsafe fn show_status_message(
        &self,
        message: CString,
        delay: i32,
        ignore_styles: i32,
        ignore_keys: i32,
        no_freeze: i32,
    );
    unsafe fn status_message_text(&self) -> (Option<CString>, bool);
    /// Install callbacks and typed payload as one owned overlay. Callback
    /// retirement remains explicit and runs without a live client borrow.
    unsafe fn set_overlay(&self, overlay: Overlay);
    unsafe fn clear_overlay(&self);
    unsafe fn has_overlay(&self) -> bool;
    unsafe fn clips_terminal_output(&self) -> bool;
    unsafe fn draw_overlay(&self);
    unsafe fn overlay_ranges(&self, px: u_int, py: u_int, nx: u_int) -> Option<visible_ranges>;
    /// Return only the popup's nonowning identity; acquire its state afterwards.
    unsafe fn popup_overlay(&self) -> Option<crate::popup::PopupHandle>;
    /// Temporarily remove clipping for an overlay's own output. Restore only if
    /// callbacks did not retire/replace the overlay during `draw`; no model
    /// borrow spans that closure. The supplied callback replaces the old one.
    unsafe fn with_overlay_check_disabled<R>(
        &self,
        restore: overlay_check_cb,
        draw: impl FnOnce() -> R,
    ) -> R;
    unsafe fn terminal_size(&self) -> (u32, u32);
    unsafe fn terminal_started(&self) -> bool;
    unsafe fn terminal_view(&self) -> crate::shared::tty::tty_window_view;
    /// An active render owns its scene across formatting and overlay callbacks.
    unsafe fn take_redraw_scene(&self) -> Option<Box<crate::shared::redraw::redraw_scene>>;
    /// Keep a replacement installed by a nested render, if one exists.
    unsafe fn restore_redraw_scene(&self, scene: Box<crate::shared::redraw::redraw_scene>);
    unsafe fn schedule_format_cycle(&self, interval_ms: i32);
    unsafe fn refresh_terminal_size(&self);
    /// Query the owned descriptor without exposing it to terminal consumers.
    unsafe fn query_terminal_size(&self) -> Option<crate::shared::posix_terminal::winsize>;
    unsafe fn draw_overlay_screen(
        &self,
        screen: &screen,
        x: u32,
        y: u32,
        sx: u32,
        sy: u32,
        style: &tty_style_ctx,
    );
    /// Draw from the current status screen and terminal under one Client borrow.
    /// Clipping/overlay callbacks must have completed before calling this.
    unsafe fn draw_status_line(&self, row: u32, x: u32, width: u32, y: u32);
    /// Prepare direct overlay output, deferring it when a full overlay redraw
    /// is already pending. Coordinates refer to the complete terminal.
    unsafe fn prepare_overlay_render(&self, context: &mut tty_ctx, x: u32, y: u32) -> bool;
    /// Select this client's view of a pane for a terminal command: 0 skips it,
    /// -1 defers to full redraw, and 1 permits immediate output.
    unsafe fn prepare_pane_render(
        &self,
        context: &mut tty_ctx,
        pane: &Rc<UnsafeCell<window_pane>>,
    ) -> i32;
    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut format_tree,
    ) -> Option<crate::format::FormatValue>;
    unsafe fn detach(&self, message: msgtype);
    unsafe fn suspend(&self);
    /// Stop terminal output, restore the lock screen, publish suspension, then
    /// send the lock command. The caller selects and snapshots the command.
    unsafe fn lock(&self, command: &CStr);
    unsafe fn lost(&self);
}

impl Client for ClientRef {
    unsafe fn create(fd: OwnedFd) -> Self {
        server_client_create(fd)
    }

    unsafe fn attached_count() -> u32 {
        server_client_how_many()
    }

    unsafe fn run_cycle() {
        server_client_loop();
    }

    unsafe fn working_directory(
        client: Option<&Self>,
        fallback: Option<&SessionRef>,
    ) -> Option<CString> {
        server_client_get_cwd(client, fallback)
    }

    unsafe fn print_to(client: Option<&Self>, parse: bool, buffer: &mut SegmentedBuf) {
        server_client_print(client, i32::from(parse), buffer);
    }

    unsafe fn forget_pane(pane: &Rc<UnsafeCell<window_pane>>) {
        server_client_remove_pane(pane);
    }

    unsafe fn open_terminal(&self) -> Result<(), CString> {
        server_client_open(self)
    }

    unsafe fn parse_flags(&self, flags: &CStr) {
        server_client_set_flags(self, flags);
    }

    unsafe fn update_theme_colours(&self) {
        server_client_update_theme_colours(Some(self));
    }

    unsafe fn handle_key(&self, event: Box<key_event>) -> i32 {
        server_client_handle_key(self, event)
    }

    unsafe fn terminal_fd(&self) -> i32 {
        (*self.get()).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd)
    }
    unsafe fn record_terminal_discard(&self, bytes: usize) {
        let discarded = &mut (*self.get()).discarded;
        *discarded = discarded.wrapping_add(bytes);
    }
    unsafe fn acknowledge_terminal_redraw(&self, bytes: usize) -> Option<usize> {
        let redraw = &mut (*self.get()).redraw;
        if *redraw == 0 {
            return None;
        }
        *redraw = redraw.saturating_sub(bytes);
        Some(*redraw)
    }
    unsafe fn reset_mouse_drag(&self) {
        let (update, release) = {
            let state = &mut *self.get();
            state.mouse_drag_flag = 0;
            (
                state.mouse_drag_update.take(),
                state.mouse_drag_release.take(),
            )
        };
        drop(update);
        drop(release);
    }
    unsafe fn mouse_drag_active(&self) -> bool {
        (*self.get()).mouse_drag_flag != 0
    }
    unsafe fn mouse_slider_position(&self) -> i32 {
        (*self.get()).mouse_slider_mpos
    }
    unsafe fn set_mouse_drag_callbacks(
        &self,
        update: Box<dyn FnMut(&mut mouse_event)>,
        release: Box<dyn FnOnce(&mut mouse_event)>,
    ) {
        let state = &mut *self.get();
        state.mouse_drag_update = Some(update);
        state.mouse_drag_release = Some(release);
    }
    unsafe fn terminal_theme_colour(&self, index: usize) -> i32 {
        (*self.get())
            .theme_colours
            .get(index)
            .copied()
            .unwrap_or(-1)
    }

    type FormatJobsMut<'a> = &'a mut crate::shared::format::format_job_tree;
    unsafe fn borrow_format_jobs_mut(&self) -> Self::FormatJobsMut<'_> {
        &mut (*self.get()).jobs
    }
    unsafe fn record_terminal_type(&self, name: &CStr) {
        server_client_set_term_type(&mut *self.get(), Some(name.to_owned()));
    }
    unsafe fn observe_input_request(&self, request: &mut crate::shared::input::input_request) {
        (*self.get()).input_requests.push(request);
    }
    unsafe fn forget_input_request(&self, request: &crate::shared::input::input_request) {
        let requests = &mut (*self.get()).input_requests;
        let index = requests
            .iter()
            .position(|candidate| std::ptr::eq(*candidate, request))
            .expect("request missing from its client handle collection");
        requests.remove(index);
    }
    unsafe fn has_input_requests(&self) -> bool {
        !(*self.get()).input_requests.is_empty()
    }
    unsafe fn reply_input_request(&self, reply: crate::input::InputRequestReply<'_>) {
        let requests = (*self.get()).input_requests.clone();
        for request in requests {
            if crate::input::input_request_matches(&*request, reply) {
                crate::input::input_complete_request(request, reply);
                return;
            }
            crate::input::input_free_request(request);
        }
    }
    unsafe fn cancel_input_requests(&self) {
        let requests = std::mem::take(&mut (*self.get()).input_requests);
        for request in requests {
            (*request).c = Weak::new();
            crate::input::input_free_request(request);
        }
    }
    type Terminal<'a> = &'a tty;
    type TerminalMut<'a> = &'a mut tty;
    unsafe fn borrow_terminal(&self) -> Self::Terminal<'_> {
        &(*self.get()).tty
    }
    unsafe fn borrow_terminal_mut(&self) -> Self::TerminalMut<'_> {
        &mut (*self.get()).tty
    }
    unsafe fn write_terminal(&self, bytes: &[u8]) {
        let state = &mut *self.get();
        crate::tty::tty_enqueue_bytes(
            Rc::downgrade(self),
            &mut state.tty,
            state.name.as_deref(),
            &mut state.written,
            bytes,
        );
    }
    unsafe fn terminal_description_source(&self) -> (Option<CString>, Vec<CString>) {
        let state = &*self.get();
        (state.term_name.clone(), state.term_caps.clone())
    }
    unsafe fn parse_terminal_features(&self, features: &CStr, separators: &CStr) {
        let state = &mut *self.get();
        crate::tty::tty_parse_features(
            features,
            separators,
            &mut state.term_features,
            Some(&mut state.term_nofeatures),
        );
    }
    unsafe fn terminal_feature_mask(&self) -> i32 {
        let state = &*self.get();
        state.term_features & !state.term_nofeatures
    }
    unsafe fn set_control_size(&self, width: u32, height: u32) {
        crate::tty::tty_set_size(&raw mut (*self.get()).tty, width, height, 0, 0);
        (*self.get()).flags |= CLIENT_SIZECHANGED as u64;
    }
    unsafe fn reset_pan(&self, window: Option<&WindowRef>) {
        let state = &mut *self.get();
        if window.is_none_or(|window| state.pan_window.ptr_eq(&WindowRef::downgrade(window))) {
            state.pan_window = WindowWeak::new();
        }
    }
    unsafe fn apply_pan(&self, window: &WindowRef, view: &mut tty_window_view) -> bool {
        let (sx, sy) = window.logical_size();
        let observer = WindowRef::downgrade(window);
        let state = &mut *self.get();
        if !state.pan_window.ptr_eq(&observer) {
            return false;
        }
        if view.sx >= sx {
            state.pan_ox = 0;
        } else if state.pan_ox.wrapping_add(view.sx) > sx {
            state.pan_ox = sx.wrapping_sub(view.sx);
        }
        view.ox = state.pan_ox;
        if view.sy >= sy {
            state.pan_oy = 0;
        } else if state.pan_oy.wrapping_add(view.sy) > sy {
            state.pan_oy = sy.wrapping_sub(view.sy);
        }
        view.oy = state.pan_oy;
        true
    }
    unsafe fn pan_window(&self, window: &WindowRef, direction: Direction, amount: u32) {
        let (width, height) = window.logical_size();
        let observer = WindowRef::downgrade(window);
        let state = &mut *self.get();
        if !state.pan_window.ptr_eq(&observer) {
            state.pan_window = observer;
            let view = tty_window_offset(&state.tty);
            state.pan_ox = view.ox;
            state.pan_oy = view.oy;
        }
        let view = tty_window_offset(&state.tty);
        match direction {
            Direction::Left => state.pan_ox = state.pan_ox.saturating_sub(amount),
            Direction::Right => {
                state.pan_ox = state
                    .pan_ox
                    .wrapping_add(amount)
                    .min(width.wrapping_sub(view.sx));
            }
            Direction::Up => state.pan_oy = state.pan_oy.saturating_sub(amount),
            Direction::Down => {
                state.pan_oy = state
                    .pan_oy
                    .wrapping_add(amount)
                    .min(height.wrapping_sub(view.sy));
            }
        }
    }
    unsafe fn schedule_format_cycle(&self, interval_ms: i32) {
        if (*self.get()).cycle_timer.is_some() {
            return;
        }
        let timeout = Duration::from_millis(interval_ms.max(0) as u64);
        let observer = Rc::downgrade(self);
        (*self.get()).cycle_timer = Some(
            Timer::new(timeout, move || {
                if let Some(owner) = observer.upgrade() {
                    let state = &mut *owner.get();
                    drop(state.cycle_timer.take());
                    if state.message_string.is_none() && state.prompt.is_none() {
                        state.flags |= CLIENT_REDRAWSTATUS as u64;
                    }
                }
            })
            .expect("arm timer"),
        );
    }
    type QueueMut<'a> = &'a mut crate::cmd::queue::cmdq_list;
    unsafe fn borrow_queue_mut(&self) -> Self::QueueMut<'_> {
        (*self.get())
            .queue
            .as_deref_mut()
            .expect("client command queue")
    }
    type Status<'a> = &'a crate::shared::status::status_line;
    type StatusMut<'a> = &'a mut crate::shared::status::status_line;
    unsafe fn borrow_status(&self) -> Self::Status<'_> {
        &(*self.get()).status
    }
    unsafe fn borrow_status_mut(&self) -> Self::StatusMut<'_> {
        &mut (*self.get()).status
    }
    unsafe fn terminal_started(&self) -> bool {
        (*self.get()).tty.is_started()
    }
    unsafe fn terminal_view(&self) -> crate::shared::tty::tty_window_view {
        tty_window_offset(&(*self.get()).tty)
    }
    unsafe fn take_redraw_scene(&self) -> Option<Box<crate::shared::redraw::redraw_scene>> {
        (*self.get()).redraw_scene.take()
    }
    unsafe fn restore_redraw_scene(&self, scene: Box<crate::shared::redraw::redraw_scene>) {
        let state = &mut *self.get();
        if state.redraw_scene.is_none() {
            state.redraw_scene = Some(scene);
        }
    }
    type ControlMut<'a> = &'a mut control_state;
    unsafe fn borrow_control_mut(&self) -> Option<Self::ControlMut<'_>> {
        (*self.get()).control_state.as_deref_mut()
    }
    unsafe fn control_pause_after(&self) -> Option<u64> {
        let state = &*self.get();
        (state.flags & CLIENT_CONTROL_PAUSEAFTER != 0).then_some(state.pause_age as u64)
    }
    unsafe fn start_control(&self) {
        use crate::reactor::{
            bufferevent_new, bufferevent_setwatermark, bufferevent_write, StreamHandle,
        };
        let control_control = self.flags() & CLIENT_CONTROLCONTROL as u64 != 0;
        let fd = (*self.get()).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd);
        let out_fd = (*self.get()).out_fd.as_ref().map_or(-1, AsRawFd::as_raw_fd);
        if control_control {
            drop((*self.get()).out_fd.take());
        } else {
            setblocking(out_fd, 0);
        }
        setblocking(fd, 0);
        (*self.get()).control_state = Some(Box::new(control_state::empty()));
        let subs = crate::control::control_subscriptions(self);
        self.borrow_control_mut()
            .expect("control client state")
            .subs = Some(subs);
        let (read, write, error) = crate::control::control_stream_callbacks(self);
        let input = bufferevent_new(fd, read, write.clone(), error.clone())
            .unwrap_or_else(|_| crate::log::fatalx(|out| out.write_all(b"out of memory")));
        let input = StreamHandle::from_ptr(input);
        self.borrow_control_mut()
            .expect("control client state")
            .read_event = input.clone();
        let output = if control_control {
            input
        } else {
            let output = bufferevent_new(out_fd, None, write, error)
                .unwrap_or_else(|_| crate::log::fatalx(|out| out.write_all(b"out of memory")));
            StreamHandle::from_ptr(output)
        };
        self.borrow_control_mut()
            .expect("control client state")
            .write_event = output.clone();
        let _ = output.with_ptr(|stream| bufferevent_setwatermark(stream));
        if control_control {
            let _ = output.with_ptr(|stream| {
                bufferevent_write(stream, b"\x1bP1000p");
                bufferevent_enable(stream, EV_WRITE as i16)
            });
        }
    }
    unsafe fn stop_control(&self) {
        let subs = {
            let Some(mut state) = self.borrow_control_mut() else {
                return;
            };
            state.subs.take()
        };
        if let Some(subs) = subs {
            crate::monitor::monitor_destroy(subs);
        }
        let shared_stream = self.flags() & CLIENT_CONTROLCONTROL as u64 != 0;
        let output = {
            std::mem::take(
                &mut self
                    .borrow_control_mut()
                    .expect("control client state")
                    .write_event,
            )
        };
        if !shared_stream {
            output.free();
        } else {
            drop(output);
        }
        let input = {
            std::mem::take(
                &mut self
                    .borrow_control_mut()
                    .expect("control client state")
                    .read_event,
            )
        };
        input.free();
        drop((*self.get()).control_state.take());
    }
    unsafe fn accept_control_reply(&self, added: usize) -> bool {
        use crate::shared::client::CLIENT_CONTROL_DISCARD;
        let state = &mut *self.get();
        let Some(control) = state.control_state.as_deref_mut() else {
            return false;
        };
        if state.flags & CLIENT_CONTROL_DISCARD != 0 {
            return false;
        }
        let size = control
            .write_event
            .with_ptr(|stream| crate::reactor::evbuffer_get_length(&(*stream).output))
            .unwrap_or(0)
            .wrapping_add(control.queued_reply_bytes)
            .wrapping_add(added);
        if size < crate::control::CONTROL_MAXIMUM_REPLY_BUFFER as usize {
            return true;
        }
        log_debug(format_args!(
            "control_check_reply_buffer: {}: {} bytes of replies buffered",
            log_cstr(state.name.as_deref().unwrap_or(c"(null)")),
            size
        ));
        if state.flags & CLIENT_EXIT as u64 == 0 {
            state.exit_message = Some(c"too far behind".to_owned());
            state.flags |= CLIENT_EXIT as u64;
            crate::control::control_discard_pane_output(control);
        }
        state.flags |= CLIENT_CONTROL_DISCARD;
        false
    }

    unsafe fn notify(&self, write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
        crate::control::control_notify_write(self, write);
    }
    unsafe fn receives_notifications(&self) -> bool {
        let state = &*self.get();
        state.flags & CLIENT_CONTROL as u64 != 0
            && state.flags & CLIENT_EXIT as u64 == 0
            && state.control_state.is_some()
    }
    unsafe fn flags(&self) -> u64 {
        (*self.get()).flags
    }
    unsafe fn update_flags(&self, set: u64, clear: u64) {
        (*self.get()).flags = ((*self.get()).flags | set) & !clear;
    }
    unsafe fn activity_time(&self) -> SystemTime {
        (*self.get()).activity_time
    }
    unsafe fn creation_time(&self) -> SystemTime {
        (*self.get()).creation_time
    }
    unsafe fn tty_name(&self) -> Option<CString> {
        (*self.get()).ttyname.clone()
    }
    unsafe fn terminal_theme(&self) -> crate::shared::colour::client_theme {
        (*self.get()).theme
    }
    unsafe fn colour_escape(&self, mut colour: i32, background: bool) -> Option<CString> {
        use crate::shared::colour::{COLOUR_FLAG_THEME, COLOUR_THEME_COUNT};
        let flags = {
            let state = &*self.get();
            if colour & COLOUR_FLAG_THEME != 0 {
                let index = (colour & 0xff) as usize;
                colour = if index < COLOUR_THEME_COUNT as usize {
                    state.theme_colours[index]
                } else {
                    colour_theme_terminal_colour(index as u32)
                };
            }
            state.tty.colour_flags()
        };
        crate::style::colour::colour_format_escape_resolved(colour, background, flags)
    }
    unsafe fn is_nested(&self) -> bool {
        server_client_check_nested(&*self.get()) != 0
    }
    unsafe fn peer_uid(&self) -> uid_t {
        crate::proc::proc_get_peer_uid((*self.get()).peer)
    }
    unsafe fn peer_gid(&self) -> gid_t {
        crate::proc::proc_get_peer_gid((*self.get()).peer)
    }
    unsafe fn send_ready(&self) {
        let peer = (*self.get()).peer;
        proc_send(peer, MSG_READY, None, &[]);
    }
    unsafe fn send_message(
        &self,
        kind: crate::compat::imsg::msgtype,
        fd: Option<OwnedFd>,
        data: &[u8],
    ) -> i32 {
        let peer = (*self.get()).peer;
        proc_send(peer, kind, fd, data)
    }
    unsafe fn register_file(&self, file: &Rc<std::cell::RefCell<crate::file::client_file>>) {
        crate::file::client_files_insert(&mut (*self.get()).files, file.clone());
    }
    unsafe fn unregister_file(&self, stream: i32, identity: *const crate::file::client_file) {
        let removed =
            crate::file::client_files_remove_identity(&mut (*self.get()).files, stream, identity);
        drop(removed);
    }
    unsafe fn file_handles(
        &self,
    ) -> impl std::iter::FusedIterator<Item = Weak<std::cell::RefCell<crate::file::client_file>>> + 'static
    {
        crate::file::client_files_iter(&(*self.get()).files)
    }
    unsafe fn find_file(&self, stream: i32) -> Weak<std::cell::RefCell<crate::file::client_file>> {
        (*self.get())
            .files
            .get(&stream)
            .map_or_else(Weak::new, Rc::downgrade)
    }
    unsafe fn remember_session(&self) {
        let state = &mut *self.get();
        state.last_session = state.session.clone();
    }
    unsafe fn previous_session(&self) -> SessionWeak {
        (*self.get()).last_session.clone()
    }
    unsafe fn has_input_fd(&self) -> bool {
        (*self.get()).fd.is_some()
    }
    unsafe fn enter_source_file(&self, limit: u32) -> Option<u32> {
        let depth = &mut (*self.get()).source_file_depth;
        if *depth >= limit {
            return None;
        }
        *depth = depth.wrapping_add(1);
        Some(*depth)
    }
    unsafe fn leave_source_file(&self) -> u32 {
        let depth = &mut (*self.get()).source_file_depth;
        *depth = depth.wrapping_sub(1);
        *depth
    }
    unsafe fn capture_termios(&self) -> termios {
        crate::shared::terminal::read_attributes(
            (*self.get()).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd),
        )
        .unwrap_or_else(|error| fatal(&error, |out| out.write_all(b"tcgetattr failed")))
    }
    unsafe fn name(&self) -> Option<CString> {
        (*self.get()).name.clone()
    }
    unsafe fn counts_as_attached(&self) -> bool {
        (*self.get()).flags & crate::shared::client::CLIENT_UNATTACHEDFLAGS as u64 == 0
    }
    unsafe fn alert(&self, kind: &CStr, visual: i32, current: bool, index: i32) {
        use crate::shared::alerts::{VISUAL_BOTH, VISUAL_OFF};
        if visual == VISUAL_OFF || visual == VISUAL_BOTH {
            crate::tty::tty_bell(self);
        }
        if visual != VISUAL_OFF {
            crate::status::status_message_set(Some(self), -1, 1, 0, 0, |out| {
                out.write_all(kind.to_bytes())?;
                if current {
                    out.write_all(b" in current window")
                } else {
                    write!(out, " in window {}", index)
                }
            });
        }
    }

    unsafe fn attached_session(&self) -> SessionWeak {
        (*self.get()).session.clone()
    }

    unsafe fn set_session(&self, session: Option<&SessionRef>) {
        server_client_set_session(self, session);
    }

    unsafe fn reattach_after_session_destroy(&self, target: Option<&SessionRef>) {
        (*self.get()).session = Weak::new();
        (*self.get()).last_session = Weak::new();
        self.set_session(target);
        if target.is_none() {
            (*self.get()).flags |= CLIENT_EXIT as u64;
        }
    }

    unsafe fn is_dead(&self) -> bool {
        (*self.get()).flags & CLIENT_DEAD as u64 != 0
    }

    unsafe fn is_read_only(&self) -> bool {
        (*self.get()).flags & CLIENT_READONLY as u64 != 0
    }
    unsafe fn exec(&self, command: &CStr) {
        server_client_exec(self, command);
    }
    unsafe fn is_control(&self) -> bool {
        (*self.get()).flags & CLIENT_CONTROL as u64 != 0
    }

    unsafe fn uses_legacy_layout_format(&self) -> bool {
        self.is_control() && (*self.get()).flags & CLIENT_CONTROL_NEWLAYOUTS == 0
    }

    unsafe fn focuses_window(&self, window: &WindowRef) -> bool {
        let flags = (*self.get()).flags;
        if flags & CLIENT_FOCUSED as u64 == 0
            || (*self.get())
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.draw.is_some())
        {
            return false;
        }
        let Some(session) = self.attached_session().upgrade() else {
            return false;
        };
        if !session.is_attached() {
            return false;
        }
        let link = session.current_winlink();
        link.get_unchecked()
            .window_handle()
            .is_some_and(|owner| WindowRef::ptr_eq(owner, window))
    }

    unsafe fn participates_in_window_sizing(&self) -> bool {
        let client = &*self.get();
        if client.session.upgrade().is_none() || client.flags & CLIENT_NOSIZEFLAGS as u64 != 0 {
            return false;
        }
        if client.flags & CLIENT_IGNORESIZE as u64 != 0 {
            let mut next = ClientRegistry::first();
            while let Some(owner) = next {
                next = ClientRegistry::next(&owner);
                let other = &*owner.get();
                if other.session.upgrade().is_some()
                    && other.flags & (CLIENT_NOSIZEFLAGS | CLIENT_IGNORESIZE) as u64 == 0
                {
                    return false;
                }
            }
        }
        client.flags & CLIENT_CONTROL as u64 == 0
            || client.flags & (CLIENT_SIZECHANGED as u64 | CLIENT_WINDOWSIZECHANGED) != 0
    }

    unsafe fn window_size(&self, window: Option<&WindowRef>) -> (u32, u32, u32, u32) {
        let (mut sx, mut sy) = (0, 0);
        let overridden = window.is_some_and(|window| {
            control_get_window_size(self, window.id(), &mut sx, &mut sy) != 0 && sx != 0 && sy != 0
        });
        if !overridden {
            let status_lines =
                if (*self.get()).flags & (CLIENT_STATUSOFF | CLIENT_CONTROL) as u64 != 0 {
                    0
                } else if let Some(session) = self.attached_session().upgrade() {
                    session.status_layout().1
                } else {
                    OptionsScope::GlobalSession.with_local(|oo| options_get_number(oo, c"status"))
                        as u32
                };
            (sx, sy) = (*self.get()).tty.size();
            sy = sy.wrapping_sub(status_lines);
        }
        let (xpixel, ypixel) = (*self.get()).tty.pixel_size();
        (sx, sy, xpixel, ypixel)
    }

    unsafe fn constrain_window_size(&self, window: &WindowRef, sx: &mut u32, sy: &mut u32) {
        if (*self.get()).flags & CLIENT_WINDOWSIZECHANGED == 0 {
            return;
        }
        let (mut cx, mut cy) = (0, 0);
        if control_get_window_size(self, window.id(), &mut cx, &mut cy) != 0 {
            log_debug(format_args!(
                "clients_calculate_size: {} size for @{} is {}x{}",
                log_cstr((*self.get()).name.as_deref().unwrap_or(c"(null)")),
                window.id(),
                cx,
                cy,
            ));
            if cx != 0 && *sx > cx {
                *sx = cx;
            }
            if cy != 0 && *sy > cy {
                *sy = cy;
            }
        }
    }

    unsafe fn request_redraw(&self, flags: u64) {
        assert_eq!(
            flags
                & !(CLIENT_ALLREDRAWFLAGS as u64
                    | CLIENT_REDRAWSCROLLBARS
                    | CLIENT_STATUSFORCE as u64),
            0,
            "only redraw flags may be requested"
        );
        (*self.get()).flags |= flags;
    }

    unsafe fn handle_key_after(
        &self,
        event: Box<key_event>,
        after: Option<&Rc<std::cell::RefCell<cmdq_item>>>,
        next: Option<&mut Weak<std::cell::RefCell<cmdq_item>>>,
    ) -> i32 {
        server_client_handle_key_after(self, event, after, next)
    }

    unsafe fn control_write_output(&self, pane: &Rc<UnsafeCell<window_pane>>) {
        if self.attached_session().upgrade().is_some() && self.is_control() {
            control_write_output(self, pane);
        }
    }

    unsafe fn with_output_offset<R>(
        &self,
        pane: u32,
        read: impl FnOnce(Option<&mut window_pane_offset>, bool) -> R,
    ) -> R {
        let client = &mut *self.get();
        let mut off = 0;
        let offset = control_pane_offset(
            client
                .control_state
                .as_deref_mut()
                .expect("control client state"),
            client.flags,
            pane,
            &mut off,
        );
        read(offset, off != 0)
    }

    unsafe fn set_return_value(&self, value: i32) {
        (*self.get()).retval = value;
    }

    unsafe fn request_exit(&self, value: i32) {
        self.set_return_value(value);
        (*self.get()).flags |= CLIENT_EXIT as u64;
    }
    unsafe fn exit_with_message(&self, message: CString, return_value: Option<i32>) {
        let state = &mut *self.get();
        state.exit_message = Some(message);
        if let Some(value) = return_value {
            state.retval = value;
        }
        state.flags |= CLIENT_EXIT as u64;
    }
    unsafe fn shutdown(&self) {
        if self.flags() & CLIENT_SUSPENDED as u64 != 0 {
            self.lost();
        } else {
            let state = &mut *self.get();
            state.flags |= CLIENT_EXIT as u64;
            state.exit_type = CLIENT_EXIT_SHUTDOWN;
        }
        (*self.get()).set_session(None);
    }

    unsafe fn with_environment<R>(&self, read: impl FnOnce(Option<&environ>) -> R) -> R {
        read((*self.get()).environ.as_deref())
    }

    unsafe fn cwd(&self, fallback: Option<&SessionRef>) -> Option<CString> {
        if cfg_finished == 0 {
            if let Some(startup) = cfg_client.upgrade() {
                return (*startup.get()).cwd.clone();
            }
        }
        if self.attached_session().upgrade().is_none() && (*self.get()).cwd.is_some() {
            return (*self.get()).cwd.clone();
        }
        if let Some(path) = fallback.and_then(|session| session.cwd()) {
            return Some(path);
        }
        if let Some(path) = self
            .attached_session()
            .upgrade()
            .and_then(|session| session.cwd())
        {
            return Some(path);
        }
        Some(find_home_cstr().unwrap_or(c"/").to_owned())
    }

    unsafe fn set_key_table(&self, name: Option<&CStr>) {
        server_client_set_key_table(self, name);
    }
    unsafe fn select_key_table(&self, table: Rc<std::cell::RefCell<key_table>>) {
        (*self.get()).keytable = Some(table);
    }
    unsafe fn uses_key_table(&self, table: &Rc<std::cell::RefCell<key_table>>) -> bool {
        (*self.get())
            .keytable
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, table))
    }
    unsafe fn prompt_observer(&self) -> refbox::Weak<crate::shared::prompt::prompt> {
        (*self.get())
            .prompt
            .as_ref()
            .map_or_else(refbox::Weak::new, |prompt| prompt.downgrade())
    }
    unsafe fn install_prompt(
        &self,
        prompt: refbox::RefBox<crate::shared::prompt::prompt>,
        flags: i32,
    ) {
        use crate::shared::prompt::{PROMPT_INCREMENTAL, PROMPT_NOFREEZE};
        let state = &mut *self.get();
        state.prompt = Some(prompt);
        if flags & (PROMPT_INCREMENTAL | PROMPT_NOFREEZE) == 0 {
            state.tty.set_frozen(true);
        }
        state.flags |= CLIENT_REDRAWSTATUS as u64;
    }
    unsafe fn clear_prompt(&self) {
        let prompt = {
            let state = &mut *self.get();
            let Some(prompt) = state.prompt.take() else {
                return;
            };
            state.tty.set_frozen(false);
            state.tty.set_cursor_hidden(false);
            state.flags |= CLIENT_ALLREDRAWFLAGS as u64;
            prompt
        };
        crate::status::status_pop_screen(self);
        crate::prompt::prompt_free(&prompt.downgrade());
    }
    unsafe fn clear_status_message(&self) {
        {
            let state = &mut *self.get();
            if state.message_string.take().is_none() {
                return;
            }
            if state.prompt.is_none() {
                state.tty.set_frozen(false);
                state.tty.set_cursor_hidden(false);
            }
            state.flags |= CLIENT_ALLREDRAWFLAGS as u64;
        }
        crate::status::status_pop_screen(self);
    }
    unsafe fn redraw_status_if_unobscured(&self) {
        let state = &mut *self.get();
        if state.message_string.is_none() && state.prompt.is_none() {
            state.flags |= CLIENT_REDRAWSTATUS as u64;
        }
    }
    unsafe fn status_message_text(&self) -> (Option<CString>, bool) {
        let state = &*self.get();
        (
            state.message_string.clone(),
            state.message_ignore_styles != 0,
        )
    }
    unsafe fn show_status_message(
        &self,
        message: CString,
        mut delay: i32,
        ignore_styles: i32,
        ignore_keys: i32,
        no_freeze: i32,
    ) {
        self.clear_status_message();
        crate::status::status_push_screen(self);
        (*self.get()).message_string = Some(message.clone());
        let name = self.name();
        crate::server::server_add_message(|out| {
            write_cstr(out, name.as_deref())?;
            out.write_all(b" message: ")?;
            out.write_all(message.as_bytes())
        });
        if delay == -1 {
            delay = options_get_number(
                &self
                    .attached_session()
                    .upgrade()
                    .expect("live session")
                    .borrow_options_mut(),
                c"display-time",
            ) as i32;
        }
        let observer = Rc::downgrade(self);
        let state = &mut *self.get();
        if delay > 0 {
            let timeout = Duration::from_millis(delay as u64);
            drop(state.message_timer.take());
            state.message_timer = Some(
                Timer::new(timeout, move || unsafe {
                    if let Some(owner) = observer.upgrade() {
                        owner.clear_status_message();
                    }
                })
                .expect("arm timer"),
            );
        }
        if delay != 0 {
            state.message_ignore_keys = ignore_keys;
        }
        state.message_ignore_styles = ignore_styles;
        if no_freeze == 0 {
            state.tty.set_frozen(true);
        }
        state.tty.set_cursor_hidden(true);
        state.flags |= CLIENT_REDRAWSTATUS as u64;
    }

    unsafe fn set_overlay(&self, overlay: Overlay) {
        server_client_set_overlay(self, overlay);
    }

    unsafe fn clear_overlay(&self) {
        server_client_clear_overlay(self);
    }
    unsafe fn has_overlay(&self) -> bool {
        (*self.get())
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.draw.is_some())
    }
    unsafe fn clips_terminal_output(&self) -> bool {
        (*self.get())
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.check.is_some())
    }

    unsafe fn draw_overlay(&self) {
        server_client_overlay_draw(self);
    }
    unsafe fn overlay_ranges(&self, px: u_int, py: u_int, nx: u_int) -> Option<visible_ranges> {
        server_client_overlay_check(self, px, py, nx)
    }

    unsafe fn popup_overlay(&self) -> Option<crate::popup::PopupHandle> {
        (*self.get()).overlay.as_ref()?.popup_handle()
    }

    unsafe fn with_overlay_check_disabled<R>(
        &self,
        restore: overlay_check_cb,
        draw: impl FnOnce() -> R,
    ) -> R {
        let generation = (*self.get()).overlay_generation;
        let displaced = (*self.get())
            .overlay
            .as_mut()
            .and_then(|overlay| overlay.check.take());
        drop(displaced);
        let result = draw();
        if (*self.get()).overlay_generation == generation && (*self.get()).overlay.is_some() {
            let displaced =
                std::mem::replace(&mut (*self.get()).overlay.as_mut().unwrap().check, restore);
            drop(displaced);
        }
        result
    }

    unsafe fn terminal_size(&self) -> (u32, u32) {
        (*self.get()).tty.size()
    }

    unsafe fn query_terminal_size(&self) -> Option<crate::shared::posix_terminal::winsize> {
        crate::shared::terminal::read_size((*self.get()).fd.as_ref().map_or(-1, AsRawFd::as_raw_fd))
            .ok()
    }

    unsafe fn refresh_terminal_size(&self) {
        tty_resize(self);
    }

    unsafe fn draw_overlay_screen(
        &self,
        source: &screen,
        x: u32,
        y: u32,
        sx: u32,
        sy: u32,
        style: &tty_style_ctx,
    ) {
        for row in 0..sy {
            crate::tty::tty_draw_line(
                self,
                source,
                0,
                row,
                sx,
                x,
                y.wrapping_add(row),
                Some(style),
            );
        }
    }

    unsafe fn draw_status_line(&self, row: u32, x: u32, width: u32, y: u32) {
        let (source, active) = {
            let mut status = self.borrow_status_mut();
            if let Some(screen) = status.active.as_deref_mut() {
                (std::mem::take(screen), true)
            } else {
                (std::mem::take(&mut status.screen), false)
            }
        };
        crate::tty::tty_draw_line(self, &source, x, row, width, x, y, None);
        let mut status = self.borrow_status_mut();
        if active {
            *status.active.as_deref_mut().expect("active status screen") = source;
        } else {
            status.screen = source;
        }
    }

    unsafe fn prepare_overlay_render(&self, context: &mut tty_ctx, x: u32, y: u32) -> bool {
        if (*self.get()).flags & CLIENT_REDRAWOVERLAY as u64 != 0 {
            return false;
        }
        context.wox = 0;
        context.woy = 0;
        (context.wsx, context.wsy) = self.terminal_size();
        context.rxoff = x as i32;
        context.xoff = context.rxoff;
        context.ryoff = y as i32;
        context.yoff = context.ryoff;
        true
    }

    unsafe fn prepare_pane_render(
        &self,
        context: &mut tty_ctx,
        pane: &Rc<UnsafeCell<window_pane>>,
    ) -> i32 {
        use crate::window::WindowPane;
        let session = self
            .attached_session()
            .upgrade()
            .expect("terminal client session");
        let window = pane.window_observer().upgrade().expect("live pane parent");
        let invisible = context.flags & TTY_CTX_INVISIBLE_PANES != 0;
        let matches = if invisible {
            session.with_winlinks(|links| {
                let mut link =
                    crate::window::winlinks_minmax(links, crate::shared::tree::RB_NEGINF);
                while link.is_alive() {
                    let entry = link.get_unchecked();
                    if entry
                        .window_handle()
                        .is_some_and(|owner| WindowRef::ptr_eq(owner, &window))
                    {
                        return true;
                    }
                    link = crate::window::winlinks_next(entry);
                }
                false
            })
        } else {
            let link = session.current_winlink();
            link.get_unchecked()
                .window_handle()
                .is_some_and(|owner| WindowRef::ptr_eq(owner, &window))
        };
        drop(window);
        if invisible || !matches {
            return i32::from(matches);
        }
        let result = pane.prepare_render(
            context,
            (*self.get()).flags & CLIENT_REDRAWWINDOW as u64 != 0,
        );
        if result != 1 {
            return result;
        }
        let view = tty_window_offset(&(*self.get()).tty);
        context.wox = view.ox;
        context.woy = view.oy;
        context.wsx = view.sx;
        context.wsy = view.sy;
        if view.bigger {
            context.flags |= TTY_CTX_WINDOW_BIGGER;
        } else {
            context.flags &= !TTY_CTX_WINDOW_BIGGER;
        }
        if (*self.get()).flags & (CLIENT_STATUSOFF | CLIENT_CONTROL) as u64 == 0 {
            let (position, lines) = session.status_layout();
            if position == 0 {
                context.yoff = (context.yoff as u32).wrapping_add(lines) as i32;
            }
        }
        1
    }

    unsafe fn format_value(
        &self,
        key: &CStr,
        context: &mut format_tree,
    ) -> Option<crate::format::FormatValue> {
        super::format::value(self, key, context)
    }

    unsafe fn detach(&self, message: msgtype) {
        server_client_detach(self, message);
    }

    unsafe fn suspend(&self) {
        server_client_suspend(self);
    }

    unsafe fn lock(&self, command: &CStr) {
        if command.to_bytes().is_empty()
            || command.to_bytes_with_nul().len()
                > crate::compat::imsg::MAX_IMSGSIZE as usize - crate::compat::imsg::IMSG_HEADER_SIZE
        {
            return;
        }
        crate::tty::tty_lock(self);
        self.update_flags(CLIENT_SUSPENDED as u64, 0);
        let peer = (*self.get()).peer;
        proc_send(peer, MSG_LOCK, None, command.to_bytes_with_nul());
    }

    unsafe fn lost(&self) {
        server_client_lost(self);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reply_limit_preserves_an_existing_exit_reason_only_after_exit_started() {
        use super::*;
        unsafe {
            for exiting in [false, true] {
                let owner = client::with_control_for_test(None, None);
                (*owner.get()).exit_message = Some(c"previous".to_owned());
                if exiting {
                    owner.update_flags(CLIENT_EXIT as u64, 0);
                }
                owner.borrow_control_mut().unwrap().queued_reply_bytes =
                    crate::control::CONTROL_MAXIMUM_REPLY_BUFFER as usize - 1;
                assert!(owner.accept_control_reply(0));
                assert!(!owner.accept_control_reply(1));
                assert_eq!(
                    (*owner.get()).exit_message.as_deref(),
                    Some(if exiting {
                        c"previous"
                    } else {
                        c"too far behind"
                    })
                );
                owner.stop_control();
            }
        }
    }
    use super::*;

    #[test]
    fn source_file_reservations_preserve_limit_and_explicit_completion() {
        unsafe {
            let owner = client::new();
            assert_eq!(owner.enter_source_file(2), Some(1));
            assert_eq!(owner.enter_source_file(2), Some(2));
            assert_eq!(owner.enter_source_file(2), None);
            assert_eq!(owner.leave_source_file(), 1);
            assert_eq!(owner.enter_source_file(2), Some(2));
            assert_eq!(owner.leave_source_file(), 1);
            assert_eq!(owner.leave_source_file(), 0);
        }
    }
}
