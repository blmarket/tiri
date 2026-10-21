//! Pane storage belongs to the pane implementation.
use super::*;
use crate::shared::window::WindowWeak;
use std::time::{SystemTime, UNIX_EPOCH};

#[repr(C)]
pub struct window_pane {
    pub(super) id: u_int,
    pub(super) active_point: u_int,
    /// Nonowning parent; final window teardown provides a scoped fallback.
    pub(super) window: WindowWeak,
    pub(super) options: Option<options>,
    pub(super) sx: u_int,
    pub(super) sy: u_int,
    pub(super) xoff: ::core::ffi::c_int,
    pub(super) yoff: ::core::ffi::c_int,
    pub(super) flags: ::core::ffi::c_int,
    /// Owns the bitmap for synchronized output; readers borrow its bytes.
    pub(super) sync_dirty: Option<Box<[bitstr_t]>>,
    pub(super) sync_dirty_size: u_int,
    pub(super) sb_slider_y: u_int,
    pub(super) sb_slider_h: u_int,
    pub(super) sb_auto_visible: ::core::ffi::c_int,
    pub(super) sb_auto_timer: Option<Timer>,
    pub(super) argv: Vec<std::ffi::CString>,
    pub(super) shell: Option<CString>,
    pub(super) cwd: Option<CString>,
    pub(super) pid: pid_t,
    pub(super) tty: [u8; 32],
    /// Set once the process is reaped; cleared on respawn.
    pub(super) exit_status: Option<std::process::ExitStatus>,
    pub(super) dead_time: SystemTime,
    /// The `split-window -W` command waiting for this pane to close.
    pub(super) wait_close: Option<CmdqWaiter>,
    pub(super) editor: Option<Box<spawn_editor_state>>,
    pub(super) output_generation: uint64_t,
    pub(super) last_output_time: time_t,
    pub(super) last_prompt_time: time_t,
    pub(super) cmd_start_time: time_t,
    pub(super) cmd_end_time: time_t,
    pub(super) cmd_status: ::core::ffi::c_int,
    pub(super) fd: Option<super::pty::PanePty>,
    /// Observes the runtime-owned pane stream, including empty pane buffers.
    pub(super) event: crate::reactor::StreamHandle,
    pub(super) offset: window_pane_offset,
    pub(super) base_offset: size_t,
    pub(super) resize_queue: window_pane_resizes,
    pub(super) resize_timer: Option<Timer>,
    pub(super) sync_timer: Option<Timer>,
    pub(super) ictx: Option<Box<input_ctx>>,
    pub(super) cached_gc: grid_cell,
    pub(super) cached_active_gc: grid_cell,
    pub(super) cached_dim: u_int,
    pub(super) cached_active_dim: u_int,
    pub(super) palette: colour_palette,
    pub(super) last_theme: client_theme,
    pub(super) border_status_line: style_line_entry,
    pub(super) pipe_fd: Option<std::os::fd::OwnedFd>,
    pub(super) pipe_pid: pid_t,
    /// Observes the runtime-owned pipe stream; pipe_fd controls its lifetime.
    pub(super) pipe_event: crate::reactor::StreamHandle,
    pub(super) pipe_offset: window_pane_offset,
    pub(super) screen_source: PaneScreenSource,
    pub(super) base: screen,
    /// Layout-specific metadata, opaque to everything but the layout that
    /// wrote it. Only the Window writes it.
    pub(super) layout_meta: Option<Box<dyn std::any::Any>>,
    pub(super) status_screen: screen,
    /// Pane-owned non-intrusive mode stack, drained before pane free.
    pub(super) modes: window_pane_modes,
    pub(super) searchstr: Option<CString>,
    pub(super) searchregex: ::core::ffi::c_int,
    pub(super) prompt: Option<refbox::RefBox<crate::shared::prompt::prompt>>,
    /// Callback-owned data; this observer cannot keep a closed prompt alive.
    pub(super) prompt_data: refbox::Weak<crate::shared::pane::window_pane_prompt>,
    pub(super) prompt_cx: u_int,
    pub(super) border_gc_set: ::core::ffi::c_int,
    pub(super) border_gc: grid_cell,
    pub(super) active_border_gc_set: ::core::ffi::c_int,
    pub(super) active_border_gc: grid_cell,
    pub(super) control_bg: ::core::ffi::c_int,
    pub(super) control_fg: ::core::ffi::c_int,
    pub(super) scrollbar_style: style,
}
impl Default for window_pane {
    fn default() -> Self {
        Self {
            id: Default::default(),
            active_point: Default::default(),
            window: Default::default(),
            options: Default::default(),
            sx: Default::default(),
            sy: Default::default(),
            xoff: Default::default(),
            yoff: Default::default(),
            flags: Default::default(),
            sync_dirty: Default::default(),
            sync_dirty_size: Default::default(),
            sb_slider_y: Default::default(),
            sb_slider_h: Default::default(),
            sb_auto_visible: Default::default(),
            sb_auto_timer: Default::default(),
            argv: Default::default(),
            shell: Default::default(),
            cwd: Default::default(),
            pid: Default::default(),
            tty: Default::default(),
            exit_status: None,
            dead_time: UNIX_EPOCH,
            wait_close: None,
            editor: Default::default(),
            output_generation: Default::default(),
            last_output_time: Default::default(),
            last_prompt_time: Default::default(),
            cmd_start_time: Default::default(),
            cmd_end_time: Default::default(),
            cmd_status: Default::default(),
            fd: Default::default(),
            event: Default::default(),
            offset: Default::default(),
            base_offset: Default::default(),
            resize_queue: Default::default(),
            resize_timer: Default::default(),
            sync_timer: Default::default(),
            ictx: Default::default(),
            cached_gc: Default::default(),
            cached_active_gc: Default::default(),
            cached_dim: Default::default(),
            cached_active_dim: Default::default(),
            palette: Default::default(),
            last_theme: Default::default(),
            border_status_line: Default::default(),
            pipe_fd: Default::default(),
            pipe_pid: Default::default(),
            pipe_event: Default::default(),
            pipe_offset: Default::default(),
            screen_source: Default::default(),
            base: Default::default(),
            layout_meta: Default::default(),
            status_screen: Default::default(),
            modes: Default::default(),
            searchstr: Default::default(),
            searchregex: Default::default(),
            prompt: Default::default(),
            prompt_data: Default::default(),
            prompt_cx: Default::default(),
            border_gc_set: Default::default(),
            border_gc: Default::default(),
            active_border_gc_set: Default::default(),
            active_border_gc: Default::default(),
            control_bg: Default::default(),
            control_fg: Default::default(),
            scrollbar_style: Default::default(),
        }
    }
}

impl window_pane {
    /// The pane's terminal name, filled in by forkpty; empty before spawning.
    pub(super) fn tty(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.tty).unwrap_or(c"")
    }
    /// Retain the parent for the current operation.
    pub(super) fn window_handle(&self) -> Option<WindowRef> {
        self.window.upgrade()
    }

    /// Visit the displayed screen: the base screen, or the selected mode's.
    /// `None` when the selected mode has no screen.
    pub(super) unsafe fn with_screen<R>(&self, visit: impl FnOnce(&screen) -> R) -> Option<R> {
        match &self.screen_source {
            PaneScreenSource::Base => Some(visit(&self.base)),
            PaneScreenSource::Mode(entry) => visit_mode_screen(entry, |screen| visit(screen)),
        }
    }

    /// Mutable form of [`Self::with_screen`].
    pub(super) unsafe fn with_screen_mut<R>(
        &mut self,
        visit: impl FnOnce(&mut screen) -> R,
    ) -> Option<R> {
        match &self.screen_source {
            PaneScreenSource::Base => Some(visit(&mut self.base)),
            PaneScreenSource::Mode(entry) => visit_mode_screen(entry, visit),
        }
    }

    pub(super) fn new() -> std::rc::Rc<std::cell::RefCell<Self>> {
        Self::empty().into_shared()
    }

    pub(super) fn into_shared(self) -> std::rc::Rc<std::cell::RefCell<Self>> {
        std::rc::Rc::new(std::cell::RefCell::new(self))
    }

    pub(super) fn empty() -> Self {
        Self::default()
    }
}

impl window_pane {
    pub(super) fn active_mode(&self) -> Option<&'static window_mode> {
        let entry = self.modes.last()?;
        Some(
            entry
                .try_borrow_mut()
                .expect("active pane mode already borrowed")
                .mode,
        )
    }

    /// Observe the current entry without retaining the pane-owned allocation.
    pub(super) fn active_mode_entry(&self) -> refbox::Weak<window_mode_entry> {
        self.modes
            .last()
            .map_or_else(refbox::Weak::new, refbox::RefBox::downgrade)
    }
}

impl window_pane {
    pub(super) fn push_resize(&mut self, resize: window_pane_resize) {
        self.resize_queue.push_back(Box::new(resize));
    }

    /// Drop queued resizes, optionally retaining the most recent one.
    pub(super) fn clear_resizes(&mut self, keep_last: bool) {
        let last = keep_last.then(|| self.resize_queue.pop_back()).flatten();
        self.resize_queue = Default::default();
        self.resize_queue.extend(last);
    }
}

/// The selected mode's screen lives in the mode payload, not in the pane.
unsafe fn visit_mode_screen<R>(
    entry: &refbox::Weak<window_mode_entry>,
    visit: impl FnOnce(&mut screen) -> R,
) -> Option<R> {
    if !entry.is_alive() {
        return None;
    }
    let display = entry
        .get_unchecked()
        .mode
        .display_screen
        .expect("mode display screen getter");
    crate::shared::window::window_mode_visit_screen(display, entry.clone(), visit)
}
