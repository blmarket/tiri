use super::session::{SessionRef, SessionWeak};
use super::window::{WindowRef, WindowWeak};
use super::{pane::window_pane, window::winlink};
use crate::session::sessions;
use crate::session::Session as _;
use crate::window::winlink_find_by_index;
use crate::window::Window as _;

/// Saved, nonowning target identities. An empty observer means unspecified;
/// an expired observer still identifies a target that has disappeared.
/// Clone/drop these handles normally: this record must never be zeroed or
/// bitwise-copied, including when embedded in a mode, prompt, or queue item.
#[derive(Clone, Default)]
#[repr(C)]
pub struct Target {
    pub flags: ::core::ffi::c_int,
    pub s: SessionWeak,
    pub wl: refbox::Weak<winlink>,
    pub w: WindowWeak,
    pub wp: std::rc::Weak<std::cell::RefCell<window_pane>>,
    pub idx: ::core::ffi::c_int,
}

impl Target {
    pub fn clear_state(flags: ::core::ffi::c_int) -> Self {
        Self {
            flags,
            idx: -1,
            ..Default::default()
        }
    }

    /// Retain the target allocation; callbacks may still invalidate it logically.
    pub fn session_handle(&self) -> Option<SessionRef> {
        self.s.upgrade()
    }

    pub fn set_s(&mut self, value: Option<&SessionRef>) {
        self.s = value.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }

    /// Retain the target allocation; callbacks may still invalidate it logically.
    pub fn window_handle(&self) -> Option<WindowRef> {
        self.w.upgrade()
    }

    pub fn set_w(&mut self, value: Option<&WindowRef>) {
        self.w = value.map_or_else(WindowWeak::new, WindowRef::downgrade);
    }

    /// Retain the target allocation; callbacks may still invalidate it logically.
    pub fn pane_handle(&self) -> Option<std::rc::Rc<std::cell::RefCell<window_pane>>> {
        self.wp.upgrade()
    }

    pub fn set_wp(&mut self, value: Option<&std::rc::Rc<std::cell::RefCell<window_pane>>>) {
        self.wp = value.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    }

    /// Observe the selected link without extending its lifetime.
    pub fn winlink_handle(&self) -> refbox::Weak<winlink> {
        self.wl.clone()
    }

    /// Store the selected link identity; the session index retains ownership.
    pub fn set_wl(&mut self, wl: refbox::Weak<winlink>) {
        self.wl = wl;
    }
}

impl Target {
    pub fn is_valid(&self) -> bool {
        let (Some(s), Some(w), Some(_wp)) = (self.s.upgrade(), self.w.upgrade(), self.wp.upgrade())
        else {
            return false;
        };
        let Ok(wl) = self.wl.try_borrow() else {
            return false;
        };
        if sessions::resolve(&self.s).is_none()
            || !wl
                .window_handle()
                .is_some_and(|window| WindowRef::ptr_eq(window, &w))
        {
            return false;
        }
        let member = s.with_winlinks(|links| winlink_find_by_index(links, wl.idx));
        if member != self.wl {
            return false;
        }
        w.contains_pane(&self.wp)
    }
}
impl Target {
    pub fn copy_from(&mut self, src: &Self) {
        let source = src.clone();
        self.s = source.s;
        self.wl = source.wl;
        self.idx = source.idx;
        self.w = source.w;
        self.wp = source.wp;
    }
}
impl Target {
    pub fn is_empty(&self) -> bool {
        self.s.ptr_eq(&std::rc::Weak::new())
            && self.wl.is_empty()
            && self.w.ptr_eq(&WindowWeak::new())
            && self.wp.ptr_eq(&std::rc::Weak::new())
    }
}

use crate::environ::environ_find;
use crate::log::{log_cstr, log_debug};
use crate::server_client::{Client as _, ClientRegistry};
use crate::shared::client::ClientRef;
use crate::shared::mouse::mouse_event;
use crate::shared::rc::same;
use crate::shared::tree::RB_NEGINF;
use crate::text::action_helpers::mouse_pane;
use crate::window::{winlink_next, winlinks_minmax};
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use std::ffi::CStr;
use std::rc::Rc;
const CMD_FIND_PREFER_UNATTACHED: i32 = 1;
pub fn inside_pane(
    client: Option<&ClientRef>,
) -> Option<std::rc::Rc<std::cell::RefCell<window_pane>>> {
    let client = client?;
    let terminal = client.tty_name();
    let indexed = terminal.as_deref().and_then(|name| {
        crate::shared::pane::window_pane_tree::snapshot()
            .into_iter()
            .find(|pane| pane.has_tty() && pane.matches_terminal_name(name))
    });
    let inside = indexed.or_else(|| {
        let pane_id = client.with_environment(|environment| {
            environ_find(environment.expect("environment"), c"TMUX_PANE")
                .and_then(|entry| entry.value.clone())
        });
        pane_id
            .as_deref()
            .and_then(std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id_str)
    });
    if let Some(pane) = inside.as_ref() {
        let terminal = pane.tty_name();
        log_debug(format_args!(
            "cmd_find_inside_pane: got pane %{} ({})",
            pane.id(),
            log_cstr(&terminal)
        ));
    }
    inside
}
pub fn client_better(c: &ClientRef, than: Option<&ClientRef>) -> bool {
    than.is_none_or(|than| {
        let (c, than) = (c.activity_time(), than.activity_time());
        c > than
    })
}
pub fn best_client(s: &SessionRef) -> Option<ClientRef> {
    let mut best: Option<ClientRef> = None;
    let mut cursor = ClientRegistry::first();
    while let Some(owner) = cursor {
        cursor = ClientRegistry::next(&owner);
        let Some(attached) = owner.attached_session().upgrade() else {
            continue;
        };
        if s.is_attached() && !Rc::ptr_eq(&attached, s) {
            continue;
        }
        if client_better(&owner, best.as_ref()) {
            best = Some(owner);
        }
    }
    best
}
pub fn session_better(s: &SessionRef, than: Option<&SessionRef>, flags: i32) -> bool {
    let Some(than) = than else { return true };
    if flags & CMD_FIND_PREFER_UNATTACHED != 0 {
        if than.is_attached() && !s.is_attached() {
            return true;
        }
        if !than.is_attached() && s.is_attached() {
            return false;
        }
    }
    let (s, than) = (s.activity_time(), than.activity_time());
    s > than
}
pub fn session_valid(s: &SessionRef) -> i32 {
    if !s.is_registered() {
        return 0;
    }
    let link = s.current_winlink();
    if !link.is_alive() {
        return 0;
    }
    let has_pane = link
        .try_borrow()
        .expect("live winlink")
        .window_handle()
        .is_some_and(|window| window.active_pane().upgrade().is_some());
    has_pane as i32
}
pub fn best_session(
    candidates: Option<&[SessionRef]>,
    flags: ::core::ffi::c_int,
) -> Option<SessionRef> {
    let mut all = Vec::new();
    let candidates = match candidates {
        Some(candidates) => candidates,
        None => {
            let mut cursor = sessions::first();
            while let Some(owner) = cursor {
                cursor = sessions::after(owner.name().as_c_str());
                all.push(owner);
            }
            &all
        }
    };
    log_debug(format_args!(
        "cmd_find_best_session: {} sessions to try",
        candidates.len()
    ));
    let mut best: Option<SessionRef> = None;
    for owner in candidates {
        if session_valid(owner) != 0 && session_better(owner, best.as_ref(), flags) {
            best = Some(owner.clone());
        }
    }
    best
}
impl Target {
    pub fn best_session_with_window(&mut self) -> ::core::ffi::c_int {
        let Some(window_owner) = self.w.upgrade() else {
            return -1;
        };
        let mut candidates = Vec::new();
        let mut cursor = sessions::first();
        while let Some(owner) = cursor {
            cursor = sessions::after(owner.name().as_c_str());
            if owner.contains_window(&window_owner) {
                candidates.push(owner);
            }
        }
        let Some(best) = best_session(Some(&candidates), self.flags) else {
            return -1;
        };
        self.s = std::rc::Rc::downgrade(&best);
        self.best_winlink_with_window()
    }
    pub fn best_winlink_with_window(&mut self) -> ::core::ffi::c_int {
        let session = self.session_handle().expect("window target session");
        let window = self.window_handle().expect("target window");
        log_debug(format_args!(
            "{}: window is @{}",
            "cmd_find_best_winlink_with_window",
            window.id()
        ));
        let current = session.current_winlink();
        let mut selected = refbox::Weak::new();
        if current.is_alive() && same(winlink_window(&current).as_ref(), Some(&window)) {
            selected = current;
        } else {
            let mut candidate = session.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
            while candidate.is_alive() {
                if same(winlink_window(&candidate).as_ref(), Some(&window)) {
                    selected = candidate;
                    break;
                }
                candidate = winlink_next(&candidate);
            }
        }
        drop(window);
        if !selected.is_alive() {
            return -1;
        }
        self.set_wl(selected);
        self.idx = self
            .winlink_handle()
            .try_borrow()
            .expect("live winlink")
            .idx;
        0
    }
    pub fn log_state(&self, prefix: &CStr) {
        if !self.session_handle().is_none() {
            log_debug(format_args!(
                "{}: s=${} {}",
                log_cstr(prefix),
                { self.session_handle().expect("live session").id() },
                log_cstr(&self.session_handle().expect("live session").name())
            ));
        } else {
            log_debug(format_args!("{}: s=none", log_cstr(prefix)));
        }
        if self.winlink_handle().is_alive() {
            log_debug(format_args!(
                "{}: wl={} {} w=@{} {}",
                log_cstr(prefix),
                self.winlink_handle()
                    .try_borrow()
                    .expect("live winlink")
                    .idx as u32,
                same(
                    winlink_window(&self.winlink_handle()).as_ref(),
                    self.window_handle().as_ref(),
                ) as ::core::ffi::c_int,
                { ((self.window_handle().as_ref()).expect("live window")).id() },
                crate::log::log_bytes(
                    self.window_handle()
                        .expect("target window")
                        .name()
                        .as_bytes()
                )
            ));
        } else {
            log_debug(format_args!("{}: wl=none", log_cstr(prefix)));
        }
        if !self.pane_handle().is_none() {
            log_debug(format_args!(
                "{}: wp=%{}",
                log_cstr(prefix),
                self.pane_handle().expect("target pane").id()
            ));
        } else {
            log_debug(format_args!("{}: wp=none", log_cstr(prefix)));
        }
        if self.idx != -(1 as ::core::ffi::c_int) {
            log_debug(format_args!("{}: idx={}", log_cstr(prefix), { self.idx }));
        } else {
            log_debug(format_args!("{}: idx=none", log_cstr(prefix)));
        };
    }
    pub fn from_session(s_owner: &SessionRef, flags: ::core::ffi::c_int) -> Self {
        let mut fs = Self::clear_state(flags);
        fs.set_s(Some(s_owner));
        fs.set_wl(s_owner.current_winlink().clone());
        fs.set_w(winlink_window(&fs.winlink_handle()).as_ref());
        fs.set_wp(
            fs.window_handle()
                .expect("live window")
                .active_pane()
                .upgrade()
                .as_ref(),
        );
        fs.log_state(c"cmd_find_from_session");
        fs
    }
    pub fn from_winlink(wl: refbox::Weak<winlink>, flags: ::core::ffi::c_int) -> Self {
        let mut fs = Self::clear_state(flags);
        fs.s = wl.try_borrow().expect("live winlink").session.clone();
        fs.set_w(winlink_window(&wl).as_ref());
        fs.set_wp(
            winlink_window(&wl)
                .expect("live window")
                .active_pane()
                .upgrade()
                .as_ref(),
        );
        fs.set_wl(wl);
        fs.log_state(c"cmd_find_from_winlink");
        fs
    }
    pub fn from_session_window(
        s_owner: &SessionRef,
        w_owner: &WindowRef,
        flags: ::core::ffi::c_int,
    ) -> Option<Self> {
        let mut fs = Self::clear_state(flags);
        fs.set_s(Some(s_owner));
        fs.set_w(Some(w_owner));
        if fs.best_winlink_with_window() != 0 {
            return None;
        }
        fs.set_wp(w_owner.active_pane().upgrade().as_ref());
        fs.log_state(c"cmd_find_from_session_window");
        Some(fs)
    }
    pub fn from_window(w_owner: &WindowRef, flags: ::core::ffi::c_int) -> Option<Self> {
        let mut fs = Self::clear_state(flags);
        fs.set_w(Some(w_owner));
        if fs.best_session_with_window() != 0 {
            return None;
        }
        if fs.best_winlink_with_window() != 0 {
            return None;
        }
        fs.set_wp(w_owner.active_pane().upgrade().as_ref());
        fs.log_state(c"cmd_find_from_window");
        Some(fs)
    }
    pub fn from_winlink_pane(
        wl: refbox::Weak<winlink>,
        wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
        flags: ::core::ffi::c_int,
    ) -> Self {
        let mut fs = Self::clear_state(flags);
        fs.s = wl.try_borrow().expect("live winlink").session.clone();
        fs.idx = wl.try_borrow().expect("live winlink").idx;
        fs.set_w(winlink_window(&wl).as_ref());
        fs.set_wl(wl);
        fs.set_wp(Some(wp_owner));
        fs.log_state(c"cmd_find_from_winlink_pane");
        fs
    }
    pub fn from_pane(
        pane: &std::rc::Rc<std::cell::RefCell<window_pane>>,
        flags: ::core::ffi::c_int,
    ) -> Option<Self> {
        let window = pane.window_observer().upgrade().expect("pane window");
        let mut fs = Self::from_window(&window, flags)?;
        drop(window);
        fs.set_wp(Some(pane));
        fs.log_state(c"cmd_find_from_pane");
        Some(fs)
    }
    pub fn from_nothing(flags: ::core::ffi::c_int) -> Option<Self> {
        let mut fs = Self::clear_state(flags);
        let session = best_session(None, flags)?;
        fs.set_s(Some(&session));
        fs.set_wl(session.current_winlink().clone());
        fs.idx = fs.winlink_handle().try_borrow().expect("live winlink").idx;
        fs.set_w(winlink_window(&fs.winlink_handle()).as_ref());
        fs.set_wp(
            fs.window_handle()
                .expect("live window")
                .active_pane()
                .upgrade()
                .as_ref(),
        );
        fs.log_state(c"cmd_find_from_nothing");
        Some(fs)
    }
    pub fn from_mouse(m: &mouse_event) -> Option<Self> {
        if m.valid == 0 {
            return None;
        }
        let mut fs = Self::clear_state(0);
        let mut wl = refbox::Weak::new();
        let mut mouse_session_owner = None;
        let mouse_pane_owner = mouse_pane(m, Some(&mut mouse_session_owner), Some(&mut wl));
        mouse_pane_owner.as_ref()?;
        fs.set_s(mouse_session_owner.as_ref());
        fs.set_wl(wl);
        fs.set_wp(mouse_pane_owner.as_ref());
        fs.set_w(winlink_window(&fs.winlink_handle()).as_ref());
        fs.log_state(c"cmd_find_from_mouse");
        Some(fs)
    }
    pub fn from_client(c_owner: Option<&ClientRef>, flags: ::core::ffi::c_int) -> Option<Self> {
        let Some(c) = c_owner else {
            return Self::from_nothing(flags);
        };
        if let Some(session) = c.attached_session().upgrade() {
            let wl = session.current_winlink().clone();
            let Some(pane) = winlink_window(&wl)
                .expect("live window")
                .active_pane()
                .upgrade()
            else {
                return Some(Self::from_session(&session, flags));
            };
            let mut fs = Self::clear_state(flags);
            fs.set_s(Some(&session));
            fs.set_w(winlink_window(&wl).as_ref());
            fs.set_wl(wl);
            fs.set_wp(Some(&pane));
            fs.log_state(c"cmd_find_from_client");
            return Some(fs);
        }
        if let Some(pane) = inside_pane(c_owner) {
            let mut fs = Self::clear_state(flags);
            fs.w = pane.window_observer();
            if fs.best_session_with_window() == 0 {
                fs.set_wl(
                    fs.session_handle()
                        .expect("live session")
                        .current_winlink()
                        .clone(),
                );
                fs.set_w(winlink_window(&fs.winlink_handle()).as_ref());
                fs.set_wp(
                    fs.window_handle()
                        .expect("live window")
                        .active_pane()
                        .upgrade()
                        .as_ref(),
                );
                fs.log_state(c"cmd_find_from_client");
                return Some(fs);
            }
        }
        Self::from_nothing(flags)
    }
}
