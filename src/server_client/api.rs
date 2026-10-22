//! Operations on retained clients. Component guards are RefCell borrows of the
//! client, so a reentrant borrow while one is held panics instead of aliasing.
use crate::options::OptionsScope;
use crate::shared::window::WindowWeak;

use super::model::{MouseDragRelease, MouseDragUpdate};
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
use std::cell::RefCell;
use std::os::fd::{AsFd, OwnedFd};
use std::rc::{Rc, Weak};
use std::time::{Duration, SystemTime};

/// The caller serializes access on the server thread and releases model and
/// component borrows before callbacks. Logical client loss remains explicit;
/// explicit owner-release sites must still release their retained reference.
pub trait Client {
    /// Create and register a protocol client, preserving the registry's owner.
    fn create(fd: OwnedFd) -> Self
    where
        Self: Sized;
    fn attached_count() -> u32
    where
        Self: Sized;
    /// Run the existing server-cycle checks over the retained client registry.
    fn run_cycle()
    where
        Self: Sized;
    /// Select the directory for an optional client using startup and session
    /// precedence. The returned value owns its bytes across owner release.
    fn working_directory(client: Option<&Self>, fallback: Option<&SessionRef>) -> Option<CString>
    where
        Self: Sized;
    fn print_to(client: Option<&Self>, parse: bool, buffer: &mut SegmentedBuf)
    where
        Self: Sized;
    fn forget_pane(pane: &Rc<RefCell<window_pane>>)
    where
        Self: Sized;
    fn open_terminal(&self) -> Result<(), CString>;
    fn parse_flags(&self, flags: &CStr);
    fn update_theme_colours(&self);
    fn handle_key(&self, event: Box<key_event>) -> i32;

    type FormatJobsMut<'a>: std::ops::DerefMut<Target = crate::shared::format::format_job_tree>
    where
        Self: 'a;
    /// Immediate cache edits only. Release before expansion, process startup,
    /// job_free, or status notification. Callback identity must not be a pointer
    /// into this cache; removed entries retain their explicit cleanup duty.
    fn borrow_format_jobs_mut(&self) -> Self::FormatJobsMut<'_>;
    type QueueMut<'a>: std::ops::DerefMut<Target = crate::cmd::queue::cmdq_list>
    where
        Self: 'a;
    /// Release before executing commands, invoking cancellation, or releasing
    /// queued owners. Queue identity must never be a pointer into this borrow.
    fn borrow_queue_mut(&self) -> Self::QueueMut<'_>;
    type Terminal<'a>: std::ops::Deref<Target = tty>
    where
        Self: 'a;
    type TerminalMut<'a>: std::ops::DerefMut<Target = tty>
    where
        Self: 'a;
    /// Read terminal properties within the guard; release it before model calls,
    /// terminal mutation, formatting, or callback dispatch.
    fn borrow_terminal(&self) -> Self::Terminal<'_>;
    /// Only component-only helpers may run under this guard. Release before
    /// terminal IO helpers that query Client, callback dispatch, or model calls.
    fn borrow_terminal_mut(&self) -> Self::TerminalMut<'_>;
    /// Enqueue terminal bytes and account for output within one model borrow.
    fn write_terminal(&self, bytes: &[u8]);
    /// A duplicate of the terminal descriptor, independent of later client borrows.
    fn terminal_fd(&self) -> Option<OwnedFd>;
    fn record_terminal_discard(&self, bytes: usize);
    /// Some(0) completes the redraw write; backpressure starts on a later write.
    fn acknowledge_terminal_redraw(&self, bytes: usize) -> Option<usize>;
    fn terminal_theme_colour(&self, index: usize) -> i32;
    /// Forget any drag in progress and its callbacks.
    fn reset_mouse_drag(&self);
    fn mouse_drag_active(&self) -> bool;
    fn mouse_slider_position(&self) -> i32;
    fn set_mouse_drag_callbacks(&self, update: MouseDragUpdate, release: MouseDragRelease);
    /// Terminfo construction may query Client; copy its source before starting.
    fn terminal_description_source(&self) -> (Option<CString>, Vec<CString>);
    fn parse_terminal_features(&self, features: &CStr, separators: &CStr);
    fn terminal_feature_mask(&self) -> i32;
    fn record_terminal_type(&self, name: &CStr);
    fn set_control_size(&self, width: u32, height: u32);
    /// Clear explicit panning; given a window, only panning of that window.
    fn reset_pan(&self, window: Option<&WindowRef>);
    /// Apply and clamp this window's explicit pan, if active, to a viewport.
    /// Window dimensions are read before borrowing Client state.
    fn apply_pan(&self, window: &WindowRef, view: &mut tty_window_view) -> bool;
    fn pan_window(&self, window: &WindowRef, direction: Direction, amount: u32);
    type Status<'a>: std::ops::Deref<Target = crate::shared::status::status_line>
    where
        Self: 'a;
    type StatusMut<'a>: std::ops::DerefMut<Target = crate::shared::status::status_line>
    where
        Self: 'a;
    /// Release before formatting, querying Client, callbacks or screen replacement.
    fn borrow_status(&self) -> Self::Status<'_>;
    fn borrow_status_mut(&self) -> Self::StatusMut<'_>;
    type ControlMut<'a>: std::ops::DerefMut<Target = control_state>
    where
        Self: 'a;
    /// Release before calling models, formatting, dispatching callbacks, or
    /// stopping control mode. No component pointer may escape the guard.
    fn borrow_control_mut(&self) -> Option<Self::ControlMut<'_>>;
    fn start_control(&self);
    fn stop_control(&self);
    fn control_pause_after(&self) -> Option<u64>;
    /// Check reply pressure and publish exit/discard state before discarding
    /// pending pane output. Returns false for a stopped or discarding client.
    fn accept_control_reply(&self, added: usize) -> bool;
    fn attached_session(&self) -> SessionWeak;
    fn set_session(&self, session: Option<&SessionRef>);
    fn reattach_after_session_destroy(&self, target: Option<&SessionRef>);
    fn is_dead(&self) -> bool;
    fn is_control(&self) -> bool;
    fn is_read_only(&self) -> bool;
    fn exec(&self, command: &CStr);
    /// Whether this client currently receives control protocol notifications.
    fn receives_notifications(&self) -> bool;
    /// Formatting runs without a client borrow and may reenter the client.
    fn notify(&self, write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>);
    fn name(&self) -> Option<CString>;
    /// Observe the complete protocol/state flag word for legacy mask decisions.
    fn flags(&self) -> u64;
    /// Apply an immediate flag transition, setting bits before clearing bits.
    fn update_flags(&self, set: u64, clear: u64);
    fn activity_time(&self) -> SystemTime;
    fn creation_time(&self) -> SystemTime;
    fn tty_name(&self) -> Option<CString>;
    fn terminal_theme(&self) -> crate::shared::colour::client_theme;
    fn colour_escape(&self, colour: i32, background: bool) -> Option<CString>;
    fn is_nested(&self) -> bool;
    fn peer_uid(&self) -> uid_t;
    fn peer_gid(&self) -> gid_t;
    /// Publish readiness at the caller's existing point in attachment ordering.
    fn send_ready(&self);
    /// Copy a protocol message into the peer's output queue. This never exposes
    /// the peer or retains a model borrow during protocol processing.
    fn send_message(
        &self,
        kind: crate::compat::imsg::msgtype,
        fd: Option<OwnedFd>,
        data: &[u8],
    ) -> i32;
    fn register_file(&self, file: &Rc<std::cell::RefCell<crate::file::client_file>>);
    fn unregister_file(&self, stream: i32, identity: *const crate::file::client_file);
    /// Stream-ordered observations; each yield resolves the current holder.
    fn file_handles(
        &self,
    ) -> impl std::iter::FusedIterator<Item = Weak<std::cell::RefCell<crate::file::client_file>>> + 'static;
    fn find_file(&self, stream: i32) -> Weak<std::cell::RefCell<crate::file::client_file>>;
    /// Observe a request owned by input_ctx. Its owner must explicitly retire
    /// this observation before freeing the request.
    fn observe_input_request(&self, request: refbox::Weak<crate::shared::input::input_request>);
    fn forget_input_request(&self, request: &refbox::Weak<crate::shared::input::input_request>);
    fn has_input_requests(&self) -> bool;
    /// Select in client-index order; no Client borrow spans parser replies or
    /// explicit request cleanup (which itself removes index observations).
    fn reply_input_request(&self, reply: crate::input::InputRequestReply<'_>);
    fn cancel_input_requests(&self);
    fn remember_session(&self);
    fn previous_session(&self) -> SessionWeak;
    fn has_input_fd(&self) -> bool;
    /// Capture terminal attributes before attaching; retain the existing fatal error policy.
    fn capture_termios(&self) -> termios;
    /// Reserve nesting before starting a source-file chain. Completion or
    /// cancellation must explicitly balance a successful reservation.
    fn enter_source_file(&self, limit: u32) -> Option<u32>;
    fn leave_source_file(&self) -> u32;

    /// Attachment accounting excludes suspended, dead and exiting clients.
    fn counts_as_attached(&self) -> bool;
    /// Deliver the audible/visual part after Session has deduplicated the alert.
    fn alert(&self, kind: &CStr, visual: i32, current: bool, index: i32);

    fn uses_legacy_layout_format(&self) -> bool;
    fn focuses_window(&self, window: &WindowRef) -> bool;
    fn participates_in_window_sizing(&self) -> bool;
    fn window_size(&self, window: Option<&WindowRef>) -> (u32, u32, u32, u32);
    fn constrain_window_size(&self, window: &WindowRef, sx: &mut u32, sy: &mut u32);
    /// `flags` contains only CLIENT_*REDRAW* bits, including status-force.
    fn request_redraw(&self, flags: u64);
    /// Ordered injection used by send-keys -K. The queue insertion point must
    /// survive until dispatch finishes, under the existing command lifetime rules.
    fn handle_key_after(
        &self,
        event: Box<key_event>,
        after: Option<&Rc<std::cell::RefCell<cmdq_item>>>,
        next: Option<&mut Weak<std::cell::RefCell<cmdq_item>>>,
    ) -> i32;
    fn control_write_output(&self, pane: &Rc<RefCell<window_pane>>);
    /// Scoped access to this control client's consumer offset. Call only for an
    /// attached control client; the bool is the existing output-disabled flag.
    /// Do not reenter models, release owners, or let the reference escape.
    fn with_output_offset<R>(
        &self,
        pane: u32,
        read: impl FnOnce(Option<&mut window_pane_offset>, bool) -> R,
    ) -> R;
    fn set_return_value(&self, value: i32);
    fn request_exit(&self, value: i32);
    fn exit_with_message(&self, message: CString, return_value: Option<i32>);
    /// Preserve shutdown's immediate loss of suspended clients and otherwise
    /// queue the shutdown reason, then clear attachment without notifications.
    fn shutdown(&self);
    /// The closure cannot reenter models, destroy owners, or leak component
    /// references. Clone the environment before invoking another entity.
    fn with_environment<R>(&self, read: impl FnOnce(Option<&environ>) -> R) -> R;
    fn cwd(&self, fallback: Option<&SessionRef>) -> Option<CString>;
    fn set_key_table(&self, name: Option<&CStr>);
    /// Switch to an already resolved table without refreshing its activity time.
    fn select_key_table(&self, table: Rc<std::cell::RefCell<key_table>>);
    fn uses_key_table(&self, table: &Rc<std::cell::RefCell<key_table>>) -> bool;
    /// Observe the separately owned prompt, then borrow it only while needed.
    /// A callback may retire it and install another prompt on this client.
    fn prompt_observer(&self) -> refbox::Weak<crate::shared::prompt::prompt>;
    fn install_prompt(&self, prompt: refbox::RefBox<crate::shared::prompt::prompt>, flags: i32);
    fn clear_prompt(&self);
    fn clear_status_message(&self);
    fn redraw_status_if_unobscured(&self);
    fn show_status_message(
        &self,
        message: CString,
        delay: i32,
        ignore_styles: i32,
        ignore_keys: i32,
        no_freeze: i32,
    );
    fn status_message_text(&self) -> (Option<CString>, bool);
    /// Install callbacks and typed payload as one owned overlay. Callback
    /// retirement remains explicit and runs without a live client borrow.
    fn set_overlay(&self, overlay: Overlay);
    fn clear_overlay(&self);
    fn has_overlay(&self) -> bool;
    fn clips_terminal_output(&self) -> bool;
    fn draw_overlay(&self);
    fn overlay_ranges(&self, px: u_int, py: u_int, nx: u_int) -> Option<visible_ranges>;
    /// Return only the popup's nonowning identity; acquire its state afterwards.
    fn popup_overlay(&self) -> Option<crate::popup::PopupHandle>;
    /// Temporarily remove clipping for an overlay's own output. Restore only if
    /// callbacks did not retire/replace the overlay during `draw`; no model
    /// borrow spans that closure. The supplied callback replaces the old one.
    fn with_overlay_check_disabled<R>(
        &self,
        restore: overlay_check_cb,
        draw: impl FnOnce() -> R,
    ) -> R;
    fn terminal_size(&self) -> (u32, u32);
    fn terminal_started(&self) -> bool;
    fn terminal_view(&self) -> crate::shared::tty::tty_window_view;
    /// An active render owns its scene across formatting and overlay callbacks.
    fn take_redraw_scene(&self) -> Option<Box<crate::shared::redraw::redraw_scene>>;
    /// Keep a replacement installed by a nested render, if one exists.
    fn restore_redraw_scene(&self, scene: Box<crate::shared::redraw::redraw_scene>);
    fn schedule_format_cycle(&self, interval_ms: i32);
    fn refresh_terminal_size(&self);
    /// Query the owned descriptor without exposing it to terminal consumers.
    fn query_terminal_size(&self) -> Option<crate::shared::posix_terminal::winsize>;
    fn draw_overlay_screen(
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
    fn draw_status_line(&self, row: u32, x: u32, width: u32, y: u32);
    /// Prepare direct overlay output, deferring it when a full overlay redraw
    /// is already pending. Coordinates refer to the complete terminal.
    fn prepare_overlay_render(&self, context: &mut tty_ctx, x: u32, y: u32) -> bool;
    /// Select this client's view of a pane for a terminal command: 0 skips it,
    /// -1 defers to full redraw, and 1 permits immediate output.
    fn prepare_pane_render(&self, context: &mut tty_ctx, pane: &Rc<RefCell<window_pane>>) -> i32;
    fn format_value(
        &self,
        key: &CStr,
        context: &mut format_tree,
    ) -> Option<crate::format::FormatValue>;
    fn detach(&self, message: msgtype);
    fn suspend(&self);
    /// Stop terminal output, restore the lock screen, publish suspension, then
    /// send the lock command. The caller selects and snapshots the command.
    fn lock(&self, command: &CStr);
    fn lost(&self);
}

impl Client for ClientRef {
    fn create(fd: OwnedFd) -> Self {
        server_client_create(fd)
    }

    fn attached_count() -> u32 {
        server_client_how_many()
    }

    fn run_cycle() {
        server_client_loop();
    }

    fn working_directory(client: Option<&Self>, fallback: Option<&SessionRef>) -> Option<CString> {
        server_client_get_cwd(client, fallback)
    }

    fn print_to(client: Option<&Self>, parse: bool, buffer: &mut SegmentedBuf) {
        server_client_print(client, i32::from(parse), buffer);
    }

    fn forget_pane(pane: &Rc<RefCell<window_pane>>) {
        server_client_remove_pane(pane);
    }

    fn open_terminal(&self) -> Result<(), CString> {
        server_client_open(self)
    }

    fn parse_flags(&self, flags: &CStr) {
        server_client_set_flags(self, flags);
    }

    fn update_theme_colours(&self) {
        server_client_update_theme_colours(Some(self));
    }

    fn handle_key(&self, event: Box<key_event>) -> i32 {
        server_client_handle_key(self, event)
    }

    fn terminal_fd(&self) -> Option<OwnedFd> {
        self.borrow().fd.as_ref().and_then(|fd| fd.try_clone().ok())
    }
    fn record_terminal_discard(&self, bytes: usize) {
        let mut client = self.borrow_mut();
        client.discarded = client.discarded.wrapping_add(bytes);
    }
    fn acknowledge_terminal_redraw(&self, bytes: usize) -> Option<usize> {
        let mut client = self.borrow_mut();
        if client.redraw == 0 {
            return None;
        }
        client.redraw = client.redraw.saturating_sub(bytes);
        Some(client.redraw)
    }
    fn reset_mouse_drag(&self) {
        let (update, release) = {
            let mut state = self.borrow_mut();
            state.mouse_drag_flag = 0;
            (
                state.mouse_drag_update.take(),
                state.mouse_drag_release.take(),
            )
        };
        drop(update);
        drop(release);
    }
    fn mouse_drag_active(&self) -> bool {
        self.borrow().mouse_drag_flag != 0
    }
    fn mouse_slider_position(&self) -> i32 {
        self.borrow().mouse_slider_mpos
    }
    fn set_mouse_drag_callbacks(&self, update: MouseDragUpdate, release: MouseDragRelease) {
        let replaced = {
            let mut state = self.borrow_mut();
            (
                state.mouse_drag_update.replace(update),
                state.mouse_drag_release.replace(release),
            )
        };
        drop(replaced);
    }
    fn terminal_theme_colour(&self, index: usize) -> i32 {
        self.borrow()
            .theme_colours
            .get(index)
            .copied()
            .unwrap_or(-1)
    }

    type FormatJobsMut<'a> = std::cell::RefMut<'a, crate::shared::format::format_job_tree>;
    fn borrow_format_jobs_mut(&self) -> Self::FormatJobsMut<'_> {
        std::cell::RefMut::map(self.borrow_mut(), |state| &mut state.jobs)
    }
    fn record_terminal_type(&self, name: &CStr) {
        server_client_set_term_type(&mut self.borrow_mut(), Some(name.to_owned()));
    }
    fn observe_input_request(&self, request: refbox::Weak<crate::shared::input::input_request>) {
        self.borrow_mut().input_requests.push(request);
    }
    fn forget_input_request(&self, request: &refbox::Weak<crate::shared::input::input_request>) {
        let mut state = self.borrow_mut();
        let requests = &mut state.input_requests;
        let index = requests
            .iter()
            .position(|candidate| candidate == request)
            .expect("request missing from its client handle collection");
        requests.remove(index);
    }
    fn has_input_requests(&self) -> bool {
        !self.borrow().input_requests.is_empty()
    }
    fn reply_input_request(&self, reply: crate::input::InputRequestReply<'_>) {
        let requests = self.borrow().input_requests.clone();
        crate::input::input_reply_observed_requests(requests, reply);
    }
    fn cancel_input_requests(&self) {
        let requests = std::mem::take(&mut self.borrow_mut().input_requests);
        crate::input::input_cancel_observed_requests(requests);
    }
    type Terminal<'a> = std::cell::Ref<'a, tty>;
    type TerminalMut<'a> = std::cell::RefMut<'a, tty>;
    fn borrow_terminal(&self) -> Self::Terminal<'_> {
        std::cell::Ref::map(self.borrow(), |state| &state.tty)
    }
    fn borrow_terminal_mut(&self) -> Self::TerminalMut<'_> {
        std::cell::RefMut::map(self.borrow_mut(), |state| &mut state.tty)
    }
    fn write_terminal(&self, bytes: &[u8]) {
        let mut state = self.borrow_mut();
        let state = &mut *state;
        crate::tty::tty_enqueue_bytes(
            Rc::downgrade(self),
            &mut state.tty,
            state.name.as_deref(),
            &mut state.written,
            bytes,
        );
    }
    fn terminal_description_source(&self) -> (Option<CString>, Vec<CString>) {
        let state = self.borrow();
        (state.term_name.clone(), state.term_caps.clone())
    }
    fn parse_terminal_features(&self, features: &CStr, separators: &CStr) {
        let mut state = self.borrow_mut();
        let state = &mut *state;
        crate::tty::tty_parse_features(
            features,
            separators,
            &mut state.term_features,
            Some(&mut state.term_nofeatures),
        );
    }
    fn terminal_feature_mask(&self) -> i32 {
        let state = self.borrow();
        state.term_features & !state.term_nofeatures
    }
    fn set_control_size(&self, width: u32, height: u32) {
        let mut state = self.borrow_mut();
        crate::tty::tty_set_size(&mut state.tty, width, height, 0, 0);
        state.flags |= CLIENT_SIZECHANGED as u64;
    }
    fn reset_pan(&self, window: Option<&WindowRef>) {
        let mut state = self.borrow_mut();
        if window.is_none_or(|window| state.pan_window.ptr_eq(&WindowRef::downgrade(window))) {
            state.pan_window = WindowWeak::new();
        }
    }
    fn apply_pan(&self, window: &WindowRef, view: &mut tty_window_view) -> bool {
        let (sx, sy) = window.logical_size();
        let observer = WindowRef::downgrade(window);
        let mut state = self.borrow_mut();
        let state = &mut *state;
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
    fn pan_window(&self, window: &WindowRef, direction: Direction, amount: u32) {
        let (width, height) = window.logical_size();
        let observer = WindowRef::downgrade(window);
        let mut state = self.borrow_mut();
        let state = &mut *state;
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
    fn schedule_format_cycle(&self, interval_ms: i32) {
        if self.borrow().cycle_timer.is_some() {
            return;
        }
        let timeout = Duration::from_millis(interval_ms.max(0) as u64);
        let observer = Rc::downgrade(self);
        let timer = Timer::new(timeout, move || {
            if let Some(owner) = observer.upgrade() {
                let mut state = owner.borrow_mut();
                let fired = state.cycle_timer.take();
                if state.message_string.is_none() && state.prompt.is_none() {
                    state.flags |= CLIENT_REDRAWSTATUS as u64;
                }
                drop(state);
                drop(fired);
            }
        })
        .expect("arm timer");
        self.borrow_mut().cycle_timer = Some(timer);
    }
    type QueueMut<'a> = std::cell::RefMut<'a, crate::cmd::queue::cmdq_list>;
    fn borrow_queue_mut(&self) -> Self::QueueMut<'_> {
        std::cell::RefMut::map(self.borrow_mut(), |state| {
            state.queue.as_deref_mut().expect("client command queue")
        })
    }
    type Status<'a> = std::cell::Ref<'a, crate::shared::status::status_line>;
    type StatusMut<'a> = std::cell::RefMut<'a, crate::shared::status::status_line>;
    fn borrow_status(&self) -> Self::Status<'_> {
        std::cell::Ref::map(self.borrow(), |state| &state.status)
    }
    fn borrow_status_mut(&self) -> Self::StatusMut<'_> {
        std::cell::RefMut::map(self.borrow_mut(), |state| &mut state.status)
    }
    fn terminal_started(&self) -> bool {
        self.borrow().tty.is_started()
    }
    fn terminal_view(&self) -> crate::shared::tty::tty_window_view {
        tty_window_offset(&self.borrow().tty)
    }
    fn take_redraw_scene(&self) -> Option<Box<crate::shared::redraw::redraw_scene>> {
        self.borrow_mut().redraw_scene.take()
    }
    fn restore_redraw_scene(&self, scene: Box<crate::shared::redraw::redraw_scene>) {
        let mut state = self.borrow_mut();
        if state.redraw_scene.is_none() {
            state.redraw_scene = Some(scene);
        }
    }
    type ControlMut<'a> = std::cell::RefMut<'a, control_state>;
    fn borrow_control_mut(&self) -> Option<Self::ControlMut<'_>> {
        std::cell::RefMut::filter_map(self.borrow_mut(), |state| {
            state.control_state.as_deref_mut()
        })
        .ok()
    }
    fn control_pause_after(&self) -> Option<u64> {
        let state = self.borrow();
        (state.flags & CLIENT_CONTROL_PAUSEAFTER != 0).then_some(state.pause_age as u64)
    }
    fn start_control(&self) {
        use crate::reactor::{bufferevent_new, bufferevent_setwatermark, bufferevent_write};
        let control_control = self.flags() & CLIENT_CONTROLCONTROL as u64 != 0;
        if control_control {
            let out_fd = self.borrow_mut().out_fd.take();
            drop(out_fd);
        }
        self.borrow_mut().control_state = Some(Box::new(control_state::empty()));
        let subs = crate::control::control_subscriptions(self);
        self.borrow_control_mut()
            .expect("control client state")
            .subs = Some(subs);
        let (read, write, error) = crate::control::control_stream_callbacks(self);
        let (input, output) = {
            let state = self.borrow();
            let fd = state.fd.as_ref().map(AsFd::as_fd);
            let out_fd = state.out_fd.as_ref().map(AsFd::as_fd);
            for fd in [out_fd, fd].into_iter().flatten() {
                setblocking(fd, 0);
            }
            let input = bufferevent_new(fd, read, write.clone(), error.clone())
                .unwrap_or_else(|_| crate::log::fatalx(|out| out.write_all(b"out of memory")));
            let output = if control_control {
                input.clone()
            } else {
                bufferevent_new(out_fd, None, write, error)
                    .unwrap_or_else(|_| crate::log::fatalx(|out| out.write_all(b"out of memory")))
            };
            (input, output)
        };
        self.borrow_control_mut()
            .expect("control client state")
            .read_event = input;
        self.borrow_control_mut()
            .expect("control client state")
            .write_event = output.clone();
        let _ = output.with_stream(|stream| bufferevent_setwatermark(stream));
        if control_control {
            let _ = output.with_stream(|stream| {
                bufferevent_write(stream, b"\x1bP1000p");
                bufferevent_enable(stream, EV_WRITE as i16)
            });
        }
    }
    fn stop_control(&self) {
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
        let control_state = self.borrow_mut().control_state.take();
        drop(control_state);
    }
    fn accept_control_reply(&self, added: usize) -> bool {
        use crate::shared::client::CLIENT_CONTROL_DISCARD;
        let mut state = self.borrow_mut();
        let state = &mut *state;
        let Some(control) = state.control_state.as_deref_mut() else {
            return false;
        };
        if state.flags & CLIENT_CONTROL_DISCARD != 0 {
            return false;
        }
        let size = control
            .write_event
            .with_stream(|stream| crate::reactor::evbuffer_get_length(&(*stream).output))
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

    fn notify(&self, write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
        crate::control::control_notify_write(self, write);
    }
    fn receives_notifications(&self) -> bool {
        let state = self.borrow();
        state.flags & CLIENT_CONTROL as u64 != 0
            && state.flags & CLIENT_EXIT as u64 == 0
            && state.control_state.is_some()
    }
    fn flags(&self) -> u64 {
        self.borrow().flags
    }
    fn update_flags(&self, set: u64, clear: u64) {
        let mut client = self.borrow_mut();
        client.flags = (client.flags | set) & !clear;
    }
    fn activity_time(&self) -> SystemTime {
        self.borrow().activity_time
    }
    fn creation_time(&self) -> SystemTime {
        self.borrow().creation_time
    }
    fn tty_name(&self) -> Option<CString> {
        self.borrow().ttyname.clone()
    }
    fn terminal_theme(&self) -> crate::shared::colour::client_theme {
        self.borrow().theme
    }
    fn colour_escape(&self, mut colour: i32, background: bool) -> Option<CString> {
        use crate::shared::colour::{COLOUR_FLAG_THEME, COLOUR_THEME_COUNT};
        let flags = {
            let state = self.borrow();
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
    fn is_nested(&self) -> bool {
        server_client_check_nested(&self.borrow()) != 0
    }
    fn peer_uid(&self) -> uid_t {
        crate::proc::proc_get_peer_uid(&self.borrow().peer.clone())
    }
    fn peer_gid(&self) -> gid_t {
        crate::proc::proc_get_peer_gid(&self.borrow().peer.clone())
    }
    fn send_ready(&self) {
        let peer = self.borrow().peer.clone();
        proc_send(&peer, MSG_READY, None, &[]);
    }
    fn send_message(
        &self,
        kind: crate::compat::imsg::msgtype,
        fd: Option<OwnedFd>,
        data: &[u8],
    ) -> i32 {
        let peer = self.borrow().peer.clone();
        proc_send(&peer, kind, fd, data)
    }
    fn register_file(&self, file: &Rc<std::cell::RefCell<crate::file::client_file>>) {
        crate::file::client_files_insert(&mut self.borrow_mut().files, file.clone());
    }
    fn unregister_file(&self, stream: i32, identity: *const crate::file::client_file) {
        let removed = crate::file::client_files_remove_identity(
            &mut self.borrow_mut().files,
            stream,
            identity,
        );
        drop(removed);
    }
    fn file_handles(
        &self,
    ) -> impl std::iter::FusedIterator<Item = Weak<std::cell::RefCell<crate::file::client_file>>> + 'static
    {
        crate::file::client_files_iter(&self.borrow().files)
    }
    fn find_file(&self, stream: i32) -> Weak<std::cell::RefCell<crate::file::client_file>> {
        self.borrow()
            .files
            .get(&stream)
            .map_or_else(Weak::new, Rc::downgrade)
    }
    fn remember_session(&self) {
        let mut state = self.borrow_mut();
        state.last_session = state.session.clone();
    }
    fn previous_session(&self) -> SessionWeak {
        self.borrow().last_session.clone()
    }
    fn has_input_fd(&self) -> bool {
        self.borrow().fd.is_some()
    }
    fn enter_source_file(&self, limit: u32) -> Option<u32> {
        let mut state = self.borrow_mut();
        if state.source_file_depth >= limit {
            return None;
        }
        state.source_file_depth = state.source_file_depth.wrapping_add(1);
        Some(state.source_file_depth)
    }
    fn leave_source_file(&self) -> u32 {
        let mut state = self.borrow_mut();
        state.source_file_depth = state.source_file_depth.wrapping_sub(1);
        state.source_file_depth
    }
    fn capture_termios(&self) -> termios {
        let state = self.borrow();
        state
            .fd
            .as_ref()
            .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EBADF))
            .and_then(|fd| tiri_rt::unix::terminal_attributes(fd.as_fd()))
            .unwrap_or_else(|error| fatal(&error, |out| out.write_all(b"tcgetattr failed")))
    }
    fn name(&self) -> Option<CString> {
        self.borrow().name.clone()
    }
    fn counts_as_attached(&self) -> bool {
        self.borrow().flags & crate::shared::client::CLIENT_UNATTACHEDFLAGS as u64 == 0
    }
    fn alert(&self, kind: &CStr, visual: i32, current: bool, index: i32) {
        use crate::shared::alerts::{VISUAL_BOTH, VISUAL_OFF};
        if visual == VISUAL_OFF || visual == VISUAL_BOTH {
            crate::tty::tty_bell(self);
        }
        if visual != VISUAL_OFF {
            {
                crate::status::status_message_set(Some(self), -1, 1, 0, 0, |out| {
                    out.write_all(kind.to_bytes())?;
                    if current {
                        out.write_all(b" in current window")
                    } else {
                        write!(out, " in window {}", index)
                    }
                })
            };
        }
    }

    fn attached_session(&self) -> SessionWeak {
        self.borrow().session.clone()
    }

    fn set_session(&self, session: Option<&SessionRef>) {
        server_client_set_session(self, session);
    }

    fn reattach_after_session_destroy(&self, target: Option<&SessionRef>) {
        {
            let mut state = self.borrow_mut();
            state.session = Weak::new();
            state.last_session = Weak::new();
        }
        self.set_session(target);
        if target.is_none() {
            self.borrow_mut().flags |= CLIENT_EXIT as u64;
        }
    }

    fn is_dead(&self) -> bool {
        self.borrow().flags & CLIENT_DEAD as u64 != 0
    }

    fn is_read_only(&self) -> bool {
        self.borrow().flags & CLIENT_READONLY as u64 != 0
    }
    fn exec(&self, command: &CStr) {
        server_client_exec(self, command);
    }
    fn is_control(&self) -> bool {
        self.borrow().flags & CLIENT_CONTROL as u64 != 0
    }

    fn uses_legacy_layout_format(&self) -> bool {
        self.is_control() && self.borrow().flags & CLIENT_CONTROL_NEWLAYOUTS == 0
    }

    fn focuses_window(&self, window: &WindowRef) -> bool {
        let flags = self.flags();
        if flags & CLIENT_FOCUSED as u64 == 0
            || self
                .borrow()
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
        let shows_window = link
            .try_borrow()
            .expect("live winlink")
            .window_handle()
            .is_some_and(|owner| WindowRef::ptr_eq(owner, window));
        shows_window
    }

    fn participates_in_window_sizing(&self) -> bool {
        let client = self.borrow();
        if client.session.upgrade().is_none() || client.flags & CLIENT_NOSIZEFLAGS as u64 != 0 {
            return false;
        }
        if client.flags & CLIENT_IGNORESIZE as u64 != 0 {
            let mut next = ClientRegistry::first();
            while let Some(owner) = next {
                next = ClientRegistry::next(&owner);
                let other = owner.borrow();
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

    fn window_size(&self, window: Option<&WindowRef>) -> (u32, u32, u32, u32) {
        let (mut sx, mut sy) = (0, 0);
        let overridden = window.is_some_and(|window| {
            control_get_window_size(self, window.id(), &mut sx, &mut sy) != 0 && sx != 0 && sy != 0
        });
        if !overridden {
            let status_lines = if self.flags() & (CLIENT_STATUSOFF | CLIENT_CONTROL) as u64 != 0 {
                0
            } else if let Some(session) = self.attached_session().upgrade() {
                session.status_layout().1
            } else {
                OptionsScope::GlobalSession.with_local(|oo| options_get_number(oo, c"status"))
                    as u32
            };
            (sx, sy) = self.terminal_size();
            sy = sy.wrapping_sub(status_lines);
        }
        let (xpixel, ypixel) = self.borrow().tty.pixel_size();
        (sx, sy, xpixel, ypixel)
    }

    fn constrain_window_size(&self, window: &WindowRef, sx: &mut u32, sy: &mut u32) {
        if self.flags() & CLIENT_WINDOWSIZECHANGED == 0 {
            return;
        }
        let (mut cx, mut cy) = (0, 0);
        if control_get_window_size(self, window.id(), &mut cx, &mut cy) != 0 {
            let name = self.name();
            log_debug(format_args!(
                "clients_calculate_size: {} size for @{} is {}x{}",
                log_cstr(name.as_deref().unwrap_or(c"(null)")),
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

    fn request_redraw(&self, flags: u64) {
        assert_eq!(
            flags
                & !(CLIENT_ALLREDRAWFLAGS as u64
                    | CLIENT_REDRAWSCROLLBARS
                    | CLIENT_STATUSFORCE as u64),
            0,
            "only redraw flags may be requested"
        );
        self.borrow_mut().flags |= flags;
    }

    fn handle_key_after(
        &self,
        event: Box<key_event>,
        after: Option<&Rc<std::cell::RefCell<cmdq_item>>>,
        next: Option<&mut Weak<std::cell::RefCell<cmdq_item>>>,
    ) -> i32 {
        server_client_handle_key_after(self, event, after, next)
    }

    fn control_write_output(&self, pane: &Rc<RefCell<window_pane>>) {
        if self.attached_session().upgrade().is_some() && self.is_control() {
            control_write_output(self, pane);
        }
    }

    fn with_output_offset<R>(
        &self,
        pane: u32,
        read: impl FnOnce(Option<&mut window_pane_offset>, bool) -> R,
    ) -> R {
        let mut client = self.borrow_mut();
        let client = &mut *client;
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

    fn set_return_value(&self, value: i32) {
        self.borrow_mut().retval = value;
    }

    fn request_exit(&self, value: i32) {
        self.set_return_value(value);
        self.update_flags(CLIENT_EXIT as u64, 0);
    }
    fn exit_with_message(&self, message: CString, return_value: Option<i32>) {
        let mut state = self.borrow_mut();
        state.exit_message = Some(message);
        if let Some(value) = return_value {
            state.retval = value;
        }
        state.flags |= CLIENT_EXIT as u64;
    }
    fn shutdown(&self) {
        if self.flags() & CLIENT_SUSPENDED as u64 != 0 {
            self.lost();
        } else {
            let mut state = self.borrow_mut();
            state.flags |= CLIENT_EXIT as u64;
            state.exit_type = CLIENT_EXIT_SHUTDOWN;
        }
        self.borrow_mut().set_session(None);
    }

    fn with_environment<R>(&self, read: impl FnOnce(Option<&environ>) -> R) -> R {
        read(self.borrow().environ.as_deref())
    }

    fn cwd(&self, fallback: Option<&SessionRef>) -> Option<CString> {
        if !cfg_finished() {
            if let Some(startup) = cfg_client() {
                return startup.borrow().cwd.clone();
            }
        }
        if self.attached_session().upgrade().is_none() && self.borrow().cwd.is_some() {
            return self.borrow().cwd.clone();
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

    fn set_key_table(&self, name: Option<&CStr>) {
        server_client_set_key_table(self, name);
    }
    fn select_key_table(&self, table: Rc<std::cell::RefCell<key_table>>) {
        let old = self.borrow_mut().keytable.replace(table);
        drop(old);
    }
    fn uses_key_table(&self, table: &Rc<std::cell::RefCell<key_table>>) -> bool {
        self.borrow()
            .keytable
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, table))
    }
    fn prompt_observer(&self) -> refbox::Weak<crate::shared::prompt::prompt> {
        self.borrow()
            .prompt
            .as_ref()
            .map_or_else(refbox::Weak::new, |prompt| prompt.downgrade())
    }
    fn install_prompt(&self, prompt: refbox::RefBox<crate::shared::prompt::prompt>, flags: i32) {
        use crate::shared::prompt::{PROMPT_INCREMENTAL, PROMPT_NOFREEZE};
        let mut state = self.borrow_mut();
        let replaced = state.prompt.replace(prompt);
        if flags & (PROMPT_INCREMENTAL | PROMPT_NOFREEZE) == 0 {
            state.tty.set_frozen(true);
        }
        state.flags |= CLIENT_REDRAWSTATUS as u64;
        drop(state);
        drop(replaced);
    }
    fn clear_prompt(&self) {
        let prompt = {
            let mut state = self.borrow_mut();
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
    fn clear_status_message(&self) {
        {
            let mut state = self.borrow_mut();
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
    fn redraw_status_if_unobscured(&self) {
        let mut state = self.borrow_mut();
        if state.message_string.is_none() && state.prompt.is_none() {
            state.flags |= CLIENT_REDRAWSTATUS as u64;
        }
    }
    fn status_message_text(&self) -> (Option<CString>, bool) {
        let state = self.borrow();
        (
            state.message_string.clone(),
            state.message_ignore_styles != 0,
        )
    }
    fn show_status_message(
        &self,
        message: CString,
        mut delay: i32,
        ignore_styles: i32,
        ignore_keys: i32,
        no_freeze: i32,
    ) {
        self.clear_status_message();
        crate::status::status_push_screen(self);
        self.borrow_mut().message_string = Some(message.clone());
        let name = self.name();
        {
            crate::server::server_add_message(|out| {
                write_cstr(out, name.as_deref())?;
                out.write_all(b" message: ")?;
                out.write_all(message.as_bytes())
            })
        };
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
        let mut state = self.borrow_mut();
        let mut replaced = None;
        if delay > 0 {
            let timeout = Duration::from_millis(delay as u64);
            replaced = state.message_timer.replace(
                Timer::new(timeout, move || {
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
        drop(state);
        drop(replaced);
    }

    fn set_overlay(&self, overlay: Overlay) {
        server_client_set_overlay(self, overlay);
    }

    fn clear_overlay(&self) {
        server_client_clear_overlay(self);
    }
    fn has_overlay(&self) -> bool {
        self.borrow()
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.draw.is_some())
    }
    fn clips_terminal_output(&self) -> bool {
        self.borrow()
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.check.is_some())
    }

    fn draw_overlay(&self) {
        server_client_overlay_draw(self);
    }
    fn overlay_ranges(&self, px: u_int, py: u_int, nx: u_int) -> Option<visible_ranges> {
        server_client_overlay_check(self, px, py, nx)
    }

    fn popup_overlay(&self) -> Option<crate::popup::PopupHandle> {
        self.borrow().overlay.as_ref()?.popup_handle()
    }

    fn with_overlay_check_disabled<R>(
        &self,
        restore: overlay_check_cb,
        draw: impl FnOnce() -> R,
    ) -> R {
        let generation = self.borrow().overlay_generation;
        let displaced = self
            .borrow_mut()
            .overlay
            .as_mut()
            .and_then(|overlay| overlay.check.take());
        drop(displaced);
        let result = draw();
        let mut state = self.borrow_mut();
        if state.overlay_generation == generation {
            if let Some(overlay) = state.overlay.as_mut() {
                let displaced = std::mem::replace(&mut overlay.check, restore);
                drop(state);
                drop(displaced);
            }
        }
        result
    }

    fn terminal_size(&self) -> (u32, u32) {
        self.borrow().tty.size()
    }

    fn query_terminal_size(&self) -> Option<crate::shared::posix_terminal::winsize> {
        let state = self.borrow();
        crate::shared::terminal::read_size(state.fd.as_ref()?.as_fd()).ok()
    }

    fn refresh_terminal_size(&self) {
        tty_resize(self);
    }

    fn draw_overlay_screen(
        &self,
        source: &screen,
        x: u32,
        y: u32,
        sx: u32,
        sy: u32,
        style: &tty_style_ctx,
    ) {
        for row in 0..sy {
            {
                crate::tty::tty_draw_line(
                    self,
                    source,
                    0,
                    row,
                    sx,
                    x,
                    y.wrapping_add(row),
                    Some(style),
                )
            };
        }
    }

    fn draw_status_line(&self, row: u32, x: u32, width: u32, y: u32) {
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

    fn prepare_overlay_render(&self, context: &mut tty_ctx, x: u32, y: u32) -> bool {
        if self.flags() & CLIENT_REDRAWOVERLAY as u64 != 0 {
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

    fn prepare_pane_render(&self, context: &mut tty_ctx, pane: &Rc<RefCell<window_pane>>) -> i32 {
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
                    if link
                        .try_borrow()
                        .expect("live winlink")
                        .window_handle()
                        .is_some_and(|owner| WindowRef::ptr_eq(owner, &window))
                    {
                        return true;
                    }
                    link = crate::window::winlink_next(&link);
                }
                false
            })
        } else {
            let link = session.current_winlink();
            let shows_window = link
                .try_borrow()
                .expect("live winlink")
                .window_handle()
                .is_some_and(|owner| WindowRef::ptr_eq(owner, &window));
            shows_window
        };
        drop(window);
        if invisible || !matches {
            return i32::from(matches);
        }
        let redraw_window = self.flags() & CLIENT_REDRAWWINDOW as u64 != 0;
        let result = pane.prepare_render(context, redraw_window);
        if result != 1 {
            return result;
        }
        let view = self.terminal_view();
        context.wox = view.ox;
        context.woy = view.oy;
        context.wsx = view.sx;
        context.wsy = view.sy;
        if view.bigger {
            context.flags |= TTY_CTX_WINDOW_BIGGER;
        } else {
            context.flags &= !TTY_CTX_WINDOW_BIGGER;
        }
        if self.flags() & (CLIENT_STATUSOFF | CLIENT_CONTROL) as u64 == 0 {
            let (position, lines) = session.status_layout();
            if position == 0 {
                context.yoff = (context.yoff as u32).wrapping_add(lines) as i32;
            }
        }
        1
    }

    fn format_value(
        &self,
        key: &CStr,
        context: &mut format_tree,
    ) -> Option<crate::format::FormatValue> {
        super::format::value(self, key, context)
    }

    fn detach(&self, message: msgtype) {
        server_client_detach(self, message);
    }

    fn suspend(&self) {
        server_client_suspend(self);
    }

    fn lock(&self, command: &CStr) {
        if command.to_bytes().is_empty()
            || command.to_bytes_with_nul().len()
                > crate::compat::imsg::MAX_IMSGSIZE as usize - crate::compat::imsg::IMSG_HEADER_SIZE
        {
            return;
        }
        crate::tty::tty_lock(self);
        self.update_flags(CLIENT_SUSPENDED as u64, 0);
        let peer = self.borrow().peer.clone();
        proc_send(&peer, MSG_LOCK, None, command.to_bytes_with_nul());
    }

    fn lost(&self) {
        server_client_lost(self);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reply_limit_preserves_an_existing_exit_reason_only_after_exit_started() {
        use super::*;
        {
            for exiting in [false, true] {
                let owner = client::with_control_for_test(None, None);
                owner.borrow_mut().exit_message = Some(c"previous".to_owned());
                if exiting {
                    owner.update_flags(CLIENT_EXIT as u64, 0);
                }
                owner.borrow_control_mut().unwrap().queued_reply_bytes =
                    crate::control::CONTROL_MAXIMUM_REPLY_BUFFER as usize - 1;
                assert!(owner.accept_control_reply(0));
                assert!(!owner.accept_control_reply(1));
                assert_eq!(
                    owner.borrow().exit_message.as_deref(),
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
