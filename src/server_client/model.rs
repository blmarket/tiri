//! Authoritative client objects, file transfers, overlays, and scalar domains.
use crate::shared::client::ClientWeak;
use crate::shared::session::{SessionRef, SessionWeak};
use crate::shared::window::WindowWeak;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::compat::imsg::msgtype;
use crate::shared::abi::{pid_t, size_t, time_t, u_int, uint64_t};
use crate::shared::colour::client_theme;
use crate::shared::command::cmdq_list;
use crate::shared::control::control_state;
use crate::shared::display::progress_bar;
use crate::shared::environment::environ;
use crate::shared::event::Timer;
use crate::shared::format::format_job_tree;
use crate::shared::key::{key_code, key_table};
use crate::shared::mouse::mouse_event;
use crate::shared::process::tmuxpeer;
use crate::shared::redraw::redraw_scene;
use crate::shared::status::status_line;
use crate::tty::tty;

use crate::shared::client::*;
pub type MouseDragUpdate = Box<dyn FnMut(&mut mouse_event)>;
pub type MouseDragRelease = Box<dyn FnOnce(&mut mouse_event)>;
pub struct client {
    pub(super) registry_next: ClientWeak,
    pub(super) name: Option<std::ffi::CString>,
    pub(super) peer: *mut tmuxpeer,
    pub(super) user: Option<std::ffi::CString>,
    pub(super) queue: Option<Box<cmdq_list>>,
    pub(super) control_state: Option<Box<control_state>>,
    pub(super) pause_age: u_int,
    pub(super) pid: pid_t,
    pub(super) fd: Option<std::os::fd::OwnedFd>,
    pub(super) out_fd: Option<std::os::fd::OwnedFd>,
    pub(super) retval: ::core::ffi::c_int,
    pub(super) creation_time: SystemTime,
    pub(super) activity_time: SystemTime,
    pub(super) last_activity_time: SystemTime,
    pub(super) environ: Option<Box<environ>>,
    /// Sole owner of this client's format-job cache, empty until first use.
    pub(super) jobs: format_job_tree,
    pub(super) title: Option<std::ffi::CString>,
    pub(super) path: Option<std::ffi::CString>,
    pub(super) cwd: Option<std::ffi::CString>,
    pub(super) progress_bar: progress_bar,
    pub(super) term_name: Option<std::ffi::CString>,
    pub(super) term_features: ::core::ffi::c_int,
    pub(super) term_nofeatures: ::core::ffi::c_int,
    pub(super) term_type: Option<std::ffi::CString>,
    pub(super) term_caps: Vec<std::ffi::CString>,
    pub(super) ttyname: Option<std::ffi::CString>,
    pub(super) tty: tty,
    pub(super) mouse_drag_flag: ::core::ffi::c_int,
    pub(super) mouse_drag_status: ::core::ffi::c_int,
    pub(super) mouse_drag_x: u_int,
    pub(super) mouse_drag_y: u_int,
    pub(super) mouse_scrolling_flag: ::core::ffi::c_int,
    pub(super) mouse_slider_mpos: ::core::ffi::c_int,
    pub(super) mouse_last_pane: ::core::ffi::c_int,
    pub(super) mouse_drag_update: Option<MouseDragUpdate>,
    pub(super) mouse_drag_release: Option<MouseDragRelease>,
    pub(super) written: size_t,
    pub(super) discarded: size_t,
    pub(super) redraw: size_t,
    pub(super) redraw_scene: Option<Box<redraw_scene>>,
    pub(super) repeat_timer: Option<Timer>,
    pub(super) click_timer: Option<Timer>,
    pub(super) click_loc: ::core::ffi::c_int,
    pub(super) click_wp: ::core::ffi::c_int,
    pub(super) exit_timer: Option<Timer>,
    pub(super) click_button: u_int,
    pub(super) click_event: mouse_event,
    pub(super) status: status_line,
    pub(super) cycle_timer: Option<Timer>,
    pub(super) theme: client_theme,
    pub(super) input_requests: Vec<*mut crate::shared::input::input_request>,
    pub(super) flags: uint64_t,
    pub(super) exit_type: client_exit_type,
    pub(super) exit_msgtype: msgtype,
    pub(super) exit_session: Option<std::ffi::CString>,
    pub(super) exit_message: Option<std::ffi::CString>,
    pub(super) keytable: Option<std::rc::Rc<std::cell::RefCell<key_table>>>,
    pub(super) last_key: key_code,
    pub(super) paste_time: time_t,
    pub(super) message_ignore_keys: ::core::ffi::c_int,
    pub(super) message_ignore_styles: ::core::ffi::c_int,
    pub(super) message_string: Option<std::ffi::CString>,
    pub(super) message_timer: Option<Timer>,
    pub(super) prompt: Option<refbox::RefBox<crate::shared::prompt::prompt>>,
    /// Attached session identity; the session index owns the Rc allocation.
    pub(super) session: SessionWeak,
    /// Previous session observer; this link never keeps a session alive.
    pub(super) last_session: SessionWeak,
    pub(super) theme_colours: [::core::ffi::c_int; 10],
    /// Window whose manual pan offsets are active; this does not retain it.
    pub(super) pan_window: WindowWeak,
    pub(super) pan_ox: u_int,
    pub(super) pan_oy: u_int,
    pub(super) overlay: Option<super::client_overlay::Overlay>,
    pub(super) overlay_generation: u64,
    pub(super) files: client_files,
    pub(super) source_file_depth: u_int,
}

impl client {
    #[cfg(test)]
    pub(super) unsafe fn with_session_for_test(session: Option<&SessionRef>) -> ClientRef {
        let owner = Self::new();
        (*owner.get()).set_session(session);
        owner
    }

    /// Retain the attached session for the current operation.
    pub(super) fn session_handle(&self) -> Option<SessionRef> {
        self.session.upgrade()
    }

    /// The caller supplies a live Rc-backed session.
    pub(super) fn set_session(&mut self, session: Option<&SessionRef>) {
        self.session = session.map_or_else(std::rc::Weak::new, Rc::downgrade);
    }

    pub(super) fn empty() -> Self {
        Self {
            registry_next: std::rc::Weak::new(),
            name: Default::default(),
            peer: Default::default(),
            user: Default::default(),
            queue: Default::default(),
            control_state: Default::default(),
            pause_age: Default::default(),
            pid: Default::default(),
            fd: Default::default(),
            out_fd: Default::default(),
            retval: Default::default(),
            creation_time: UNIX_EPOCH,
            activity_time: UNIX_EPOCH,
            last_activity_time: UNIX_EPOCH,
            environ: Default::default(),
            jobs: Default::default(),
            title: Default::default(),
            path: Default::default(),
            cwd: Default::default(),
            progress_bar: Default::default(),
            term_name: Default::default(),
            term_features: Default::default(),
            term_nofeatures: Default::default(),
            term_type: Default::default(),
            term_caps: Vec::new(),
            ttyname: Default::default(),
            tty: tty::empty(),
            mouse_drag_flag: 0,
            mouse_drag_status: 0,
            mouse_drag_x: 0,
            mouse_drag_y: 0,
            mouse_scrolling_flag: 0,
            mouse_slider_mpos: 0,
            mouse_last_pane: -1,
            mouse_drag_update: None,
            mouse_drag_release: None,
            written: Default::default(),
            discarded: Default::default(),
            redraw: Default::default(),
            redraw_scene: Default::default(),
            repeat_timer: Default::default(),
            click_timer: Default::default(),
            click_loc: Default::default(),
            click_wp: Default::default(),
            exit_timer: Default::default(),
            click_button: Default::default(),
            click_event: Default::default(),
            status: status_line::empty(),
            cycle_timer: Default::default(),
            theme: Default::default(),
            input_requests: Vec::new(),
            flags: Default::default(),
            exit_type: Default::default(),
            exit_msgtype: Default::default(),
            exit_session: Default::default(),
            exit_message: Default::default(),
            keytable: Default::default(),
            last_key: Default::default(),
            paste_time: Default::default(),
            message_ignore_keys: Default::default(),
            message_ignore_styles: Default::default(),
            message_string: None,
            message_timer: Default::default(),
            prompt: Default::default(),
            session: Default::default(),
            last_session: Default::default(),
            theme_colours: Default::default(),
            pan_window: Default::default(),
            pan_ox: Default::default(),
            pan_oy: Default::default(),
            overlay: None,
            overlay_generation: 0,
            files: Default::default(),
            source_file_depth: Default::default(),
        }
    }
}

#[cfg(test)]
impl client {
    /// No descriptors or monitors are installed; tests explicitly stop control
    /// after populating the component through its borrow API.
    pub(super) unsafe fn with_control_for_test(
        name: Option<&std::ffi::CStr>,
        session: Option<&SessionRef>,
    ) -> ClientRef {
        let owner = Self::with_session_for_test(session);
        (*owner.get()).name = name.map(ToOwned::to_owned);
        (*owner.get()).control_state =
            Some(Box::new(crate::shared::control::control_state::empty()));
        owner
    }
}
