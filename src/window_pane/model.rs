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
    pub(super) options: std::cell::RefCell<Option<options>>,
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
    /// Borrow the option table. Borrowing it again while held panics.
    pub(super) fn borrow_options_mut(&self) -> std::cell::RefMut<'_, options> {
        std::cell::RefMut::map(self.options.borrow_mut(), |table| {
            table.as_mut().expect("live pane options")
        })
    }
    /// The pane's terminal name, filled in by forkpty; empty before spawning.
    pub(super) fn tty(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.tty).unwrap_or(c"")
    }
    /// Retain the parent for the current operation.
    pub(super) fn window_handle(&self) -> Option<WindowRef> {
        self.window.upgrade()
    }

    /// Resolve the displayed screen while the pane and selected mode are live.
    pub(super) unsafe fn screen_ptr(&self) -> *mut screen {
        match &self.screen_source {
            PaneScreenSource::Base => (&raw const self.base).cast_mut(),
            PaneScreenSource::Mode(observer) => {
                if !observer.is_alive() {
                    return std::ptr::null_mut();
                }
                let mode = observer.get_unchecked().mode;
                mode.display_screen.expect("mode display screen getter")(observer.clone())
            }
        }
    }

    pub(super) fn new() -> std::rc::Rc<std::cell::UnsafeCell<Self>> {
        Self::empty().into_shared()
    }

    pub(super) fn into_shared(self) -> std::rc::Rc<std::cell::UnsafeCell<Self>> {
        std::rc::Rc::new(std::cell::UnsafeCell::new(self))
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
    /// Append a separately allocated resize and return its stable address.
    pub(super) fn push_resize(&mut self, resize: window_pane_resize) -> *mut window_pane_resize {
        let resize = Box::new(resize);
        let pointer = (&*resize) as *const window_pane_resize as *mut window_pane_resize;
        self.resize_queue.push_back(resize);
        pointer
    }

    /// Retain only `except`; removed entry pointers are invalid after return.
    pub(super) fn clear_resizes_except(&mut self, except: *mut window_pane_resize) {
        if except.is_null() {
            self.resize_queue = Default::default();
            return;
        }
        self.resize_queue
            .retain(|resize| std::ptr::eq(&**resize, except));
    }
}
