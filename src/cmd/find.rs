use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_event, cmdq_get_state_owned};
use crate::cmd::{cmd_mouse_pane, cmd_mouse_window};
use crate::compat::strtonum::strtonum;
use crate::environ::environ_find;
use crate::format::bytes::cstr_suffix;
use crate::format::bytes::write_cstr;
use crate::log::{fatalx, log_bytes, log_cstr, log_debug, log_pointer};
use crate::server::{marked_pane, server_check_marked};
use crate::server_client::Client;
use crate::server_client::ClientRegistry;
use crate::session::sessions;
use crate::session::Session;
use crate::shared::client::ClientRef;
use crate::shared::layout::Direction;
use crate::shared::rc::same;
use crate::shared::session::SessionRef;
use crate::shared::window::WindowRef;
use crate::shared::window::WindowWeak;
use crate::window::Window;
use crate::window::{
    winlink_find_by_index, winlink_next_by_number, winlink_previous_by_number, winlinks_minmax,
    winlinks_next,
};
use crate::window_pane::WindowPane as _;
use libc::fnmatch;
use std::ffi::{CStr, CString};
use std::rc::Rc;

use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::command::{
    CMD_FIND_CANFAIL, CMD_FIND_DEFAULT_MARKED, CMD_FIND_EXACT_SESSION, CMD_FIND_EXACT_WINDOW,
    CMD_FIND_PREFER_UNATTACHED, CMD_FIND_QUIET, CMD_FIND_WINDOW_INDEX,
};
use crate::shared::limits::INT_MAX;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::tree::{RB_INF, RB_NEGINF};
use crate::shared::window::winlink;

pub const _PATH_DEV: &std::ffi::CStr = c"/dev/";
const cmd_find_window_table: &[(&CStr, &CStr)] = &[
    (c"{start}", c"^"),
    (c"{last}", c"!"),
    (c"{end}", c"$"),
    (c"{next}", c"+"),
    (c"{previous}", c"-"),
];
const cmd_find_pane_table: &[(&CStr, &CStr)] = &[
    (c"{last}", c"!"),
    (c"{next}", c"+"),
    (c"{previous}", c"-"),
    (c"{top}", c"top"),
    (c"{bottom}", c"bottom"),
    (c"{left}", c"left"),
    (c"{right}", c"right"),
    (c"{top-left}", c"top-left"),
    (c"{top-right}", c"top-right"),
    (c"{bottom-left}", c"bottom-left"),
    (c"{bottom-right}", c"bottom-right"),
    (c"{left-of}", c"{left-of}"),
    (c"{right-of}", c"{right-of}"),
    (c"{up-of}", c"{up-of}"),
    (c"{down-of}", c"{down-of}"),
];
unsafe fn cmd_find_inside_pane(
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
fn cmd_find_client_better(c: &ClientRef, than: Option<&ClientRef>) -> bool {
    than.is_none_or(|than| {
        let (c, than) = (c.activity_time(), than.activity_time());
        c > than
    })
}
pub fn cmd_find_best_client(s: &SessionRef) -> Option<ClientRef> {
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
        if cmd_find_client_better(&owner, best.as_ref()) {
            best = Some(owner);
        }
    }
    best
}
fn cmd_find_session_better(s: &SessionRef, than: Option<&SessionRef>, flags: i32) -> bool {
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
unsafe fn cmd_find_session_valid(s: &SessionRef) -> i32 {
    if !s.is_registered() {
        return 0;
    }
    let link = s.current_winlink();
    if !link.is_alive() {
        return 0;
    }
    link.get_unchecked()
        .window_handle()
        .is_some_and(|window| window.active_pane().upgrade().is_some()) as i32
}
unsafe fn cmd_find_best_session(
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
        if cmd_find_session_valid(owner) != 0
            && cmd_find_session_better(owner, best.as_ref(), flags)
        {
            best = Some(owner.clone());
        }
    }
    best
}
fn cmd_find_map_table<'a>(table: &[(&'static CStr, &'static CStr)], target: &'a CStr) -> &'a CStr {
    table
        .iter()
        .find_map(|(source, mapped)| (*source == target).then_some(*mapped))
        .unwrap_or(target)
}
impl cmd_find_state {
    unsafe fn best_session_with_window(&mut self) -> ::core::ffi::c_int {
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
        let Some(best) = cmd_find_best_session(Some(&candidates), self.flags) else {
            return -1;
        };
        self.s = std::rc::Rc::downgrade(&best);
        self.best_winlink_with_window()
    }
    unsafe fn best_winlink_with_window(&mut self) -> ::core::ffi::c_int {
        let session = self.session_handle().expect("window target session");
        let window = self.window_handle().expect("target window");
        log_debug(format_args!(
            "{}: window is @{}",
            "cmd_find_best_winlink_with_window",
            window.id()
        ));
        let current = session.current_winlink();
        let mut selected = refbox::Weak::new();
        if current.is_alive() && same(current.get_unchecked().window_handle(), Some(&window)) {
            selected = current;
        } else {
            let mut candidate = session.with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
            while candidate.is_alive() {
                if same(candidate.get_unchecked().window_handle(), Some(&window)) {
                    selected = candidate;
                    break;
                }
                candidate = winlinks_next(candidate.get_unchecked());
            }
        }
        drop(window);
        if !selected.is_alive() {
            return -1;
        }
        self.set_wl(selected);
        self.idx = self.winlink_handle().get_unchecked().idx;
        0
    }
    unsafe fn get_session(&mut self, target: &CStr) -> i32 {
        log_debug(format_args!("cmd_find_get_session: {}", log_cstr(target)));
        if target.to_bytes().first() == Some(&b'$') {
            self.set_s(crate::shared::session::SessionRef::find_by_id_str(target).as_ref());
            return if self.session_handle().is_some() {
                0
            } else {
                -1
            };
        }
        self.set_s(crate::session::sessions::find(target).as_ref());
        if self.session_handle().is_some() {
            return 0;
        }
        if let Some(client) = cmd_find_client(None, Some(target), 1) {
            if let Some(session) = client.attached_session().upgrade() {
                self.set_s(Some(&session));
                return 0;
            }
        }
        if self.flags & CMD_FIND_EXACT_SESSION != 0 {
            return -1;
        }
        for glob in [false, true] {
            let mut matched = None;
            let mut cursor = sessions::first();
            while let Some(session) = cursor {
                let name = session.name();
                if if glob {
                    fnmatch(target.as_ptr(), name.as_ptr(), 0) == 0
                } else {
                    name.as_bytes().starts_with(target.to_bytes())
                } {
                    if matched.is_some() {
                        return -1;
                    }
                    matched = Some(session.clone());
                }
                cursor = sessions::after(session.name().as_c_str());
            }
            if let Some(session) = matched {
                self.set_s(Some(&session));
                return 0;
            }
        }
        -1
    }
    unsafe fn get_window(
        &mut self,
        window: &CStr,
        mut only: ::core::ffi::c_int,
        current: &cmd_find_state,
    ) -> ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: {}",
            "cmd_find_get_window",
            log_cstr(window)
        ));
        if window.to_bytes().first() == Some(&b'@') {
            let window_owner = crate::window::window_find_by_id_str(window);
            let result = (|| {
                self.set_w(window_owner.as_ref());
                if self.window_handle().is_none() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.best_session_with_window()
            })();
            if let Some(window) = window_owner {
                drop(window);
            }
            return result;
        }
        self.s = current.s.clone();
        if self.get_window_with_session(window) == 0 as ::core::ffi::c_int {
            return 0 as ::core::ffi::c_int;
        }
        if only == 0 && self.get_session(window) == 0 as ::core::ffi::c_int {
            self.set_wl(
                (self
                    .session_handle()
                    .expect("live session")
                    .current_winlink())
                .clone(),
            );
            self.set_w((self.winlink_handle()).get_unchecked().window_handle());
            if !self.flags & CMD_FIND_WINDOW_INDEX != 0 {
                self.idx = (self.winlink_handle()).get_unchecked().idx;
            }
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
    unsafe fn get_window_with_session(&mut self, window: &CStr) -> ::core::ffi::c_int {
        let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
        let mut idx: ::core::ffi::c_int = 0;
        let mut n: ::core::ffi::c_int = 0;
        let mut exact: ::core::ffi::c_int = 0;
        log_debug(format_args!(
            "{}: {}",
            "cmd_find_get_window_with_session",
            log_cstr(window)
        ));
        exact = self.flags & CMD_FIND_EXACT_WINDOW;
        self.set_wl(
            (self
                .session_handle()
                .expect("live session")
                .current_winlink())
            .clone(),
        );
        self.set_w((self.winlink_handle()).get_unchecked().window_handle());
        let first = window.to_bytes().first().copied();
        if first == Some(b'@') {
            let window_owner = crate::window::window_find_by_id_str(window);
            let result = (|| {
                self.set_w(window_owner.as_ref());
                if self.window_handle().is_none()
                    || !self
                        .session_handle()
                        .expect("target session")
                        .contains_window(self.window_handle().as_ref().unwrap())
                {
                    return -(1 as ::core::ffi::c_int);
                }
                self.best_winlink_with_window()
            })();
            if let Some(window) = window_owner {
                drop(window);
            }
            return result;
        }
        if exact == 0 && (first == Some(b'+') || first == Some(b'-')) {
            if window.to_bytes().len() > 1 {
                let parsed = strtonum(
                    cstr_suffix(window, 1),
                    1 as ::core::ffi::c_longlong,
                    INT_MAX as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as ::core::ffi::c_int;
                if parsed.is_err() {
                    return -(1 as ::core::ffi::c_int);
                }
            } else {
                n = 1 as ::core::ffi::c_int;
            }
            let s = self.session_handle().expect("target session");
            if self.flags & CMD_FIND_WINDOW_INDEX != 0 {
                if first == Some(b'+') {
                    if INT_MAX - (s.current_winlink()).get_unchecked().idx < n {
                        return -(1 as ::core::ffi::c_int);
                    }
                    self.idx = (s.current_winlink()).get_unchecked().idx + n;
                } else {
                    if n > (s.current_winlink()).get_unchecked().idx {
                        return -(1 as ::core::ffi::c_int);
                    }
                    self.idx = (s.current_winlink()).get_unchecked().idx - n;
                }
                return 0 as ::core::ffi::c_int;
            }
            if first == Some(b'+') {
                self.set_wl((winlink_next_by_number((s.current_winlink()).clone(), &s, n)).clone());
            } else {
                self.set_wl(
                    (winlink_previous_by_number((s.current_winlink()).clone(), &s, n)).clone(),
                );
            }
            if self.winlink_handle().is_alive() {
                self.idx = (self.winlink_handle()).get_unchecked().idx;
                self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                return 0 as ::core::ffi::c_int;
            }
        }
        if exact == 0 {
            if window == c"!" {
                let session = self.session_handle().expect("window target session");
                self.set_wl(crate::session::Session::last_winlink(&session));
                if !self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.idx = (self.winlink_handle()).get_unchecked().idx;
                self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                return 0 as ::core::ffi::c_int;
            } else if window == c"^" {
                self.set_wl(
                    (self
                        .session_handle()
                        .expect("window target session")
                        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF)))
                    .clone(),
                );
                if !self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.idx = (self.winlink_handle()).get_unchecked().idx;
                self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                return 0 as ::core::ffi::c_int;
            } else if window == c"$" {
                self.set_wl(
                    (self
                        .session_handle()
                        .expect("window target session")
                        .with_winlinks(|links| winlinks_minmax(links, RB_INF)))
                    .clone(),
                );
                if !self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.idx = (self.winlink_handle()).get_unchecked().idx;
                self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                return 0 as ::core::ffi::c_int;
            }
        }
        if first != Some(b'+') && first != Some(b'-') {
            let parsed = strtonum(
                window,
                0 as ::core::ffi::c_longlong,
                INT_MAX as ::core::ffi::c_longlong,
            );
            idx = parsed.unwrap_or(0) as ::core::ffi::c_int;
            if parsed.is_ok() {
                self.set_wl(
                    (self
                        .session_handle()
                        .expect("window target session")
                        .with_winlinks(|links| winlink_find_by_index(links, idx)))
                    .clone(),
                );
                if self.winlink_handle().is_alive() {
                    self.idx = (self.winlink_handle()).get_unchecked().idx;
                    self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                    return 0 as ::core::ffi::c_int;
                }
                if self.flags & CMD_FIND_WINDOW_INDEX != 0 {
                    self.idx = idx;
                    return 0 as ::core::ffi::c_int;
                }
            }
        }
        self.set_wl((refbox::Weak::new()).clone());
        wl = self
            .session_handle()
            .expect("window target session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if window
                == wl
                    .get_unchecked()
                    .window_handle()
                    .expect("linked window")
                    .name()
                    .as_c_str()
            {
                if self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.set_wl(wl.clone());
            }
            wl = winlinks_next(wl.get_unchecked());
        }
        if self.winlink_handle().is_alive() {
            self.idx = (self.winlink_handle()).get_unchecked().idx;
            self.set_w((self.winlink_handle()).get_unchecked().window_handle());
            return 0 as ::core::ffi::c_int;
        }
        if exact != 0 {
            return -(1 as ::core::ffi::c_int);
        }
        self.set_wl((refbox::Weak::new()).clone());
        wl = self
            .session_handle()
            .expect("window target session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if wl
                .get_unchecked()
                .window_handle()
                .expect("linked window")
                .name()
                .as_bytes()
                .starts_with(window.to_bytes())
            {
                if self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.set_wl(wl.clone());
            }
            wl = winlinks_next(wl.get_unchecked());
        }
        if self.winlink_handle().is_alive() {
            self.idx = (self.winlink_handle()).get_unchecked().idx;
            self.set_w((self.winlink_handle()).get_unchecked().window_handle());
            return 0 as ::core::ffi::c_int;
        }
        self.set_wl((refbox::Weak::new()).clone());
        wl = self
            .session_handle()
            .expect("window target session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if fnmatch(
                window.as_ptr(),
                wl.get_unchecked()
                    .window_handle()
                    .expect("linked window")
                    .name()
                    .as_ptr(),
                0 as ::core::ffi::c_int,
            ) == 0 as ::core::ffi::c_int
            {
                if self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.set_wl(wl.clone());
            }
            wl = winlinks_next(wl.get_unchecked());
        }
        if self.winlink_handle().is_alive() {
            self.idx = (self.winlink_handle()).get_unchecked().idx;
            self.set_w((self.winlink_handle()).get_unchecked().window_handle());
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
    unsafe fn get_pane(
        &mut self,
        pane: &CStr,
        mut only: ::core::ffi::c_int,
        current: &cmd_find_state,
    ) -> ::core::ffi::c_int {
        log_debug(format_args!("{}: {}", "cmd_find_get_pane", log_cstr(pane)));
        if pane.to_bytes().first() == Some(&b'%') {
            let pane_owner = std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id_str(pane);
            self.wp = pane_owner
                .as_ref()
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
            if self.pane_handle().is_none() {
                return -(1 as ::core::ffi::c_int);
            }
            self.w = self.pane_handle().expect("target pane").window_observer();
            return self.best_session_with_window();
        }
        self.s = current.s.clone();
        self.wl = current.wl.clone();
        self.idx = current.idx;
        self.w = current.w.clone();
        if self.get_pane_with_window(pane) == 0 as ::core::ffi::c_int {
            return 0 as ::core::ffi::c_int;
        }
        if only == 0
            && self.get_window(pane, 0 as ::core::ffi::c_int, current) == 0 as ::core::ffi::c_int
        {
            self.set_wp(
                (((self.window_handle().as_ref()).expect("live window"))
                    .active_pane()
                    .upgrade())
                .as_ref(),
            );
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
    unsafe fn get_pane_with_session(&mut self, pane: &CStr) -> ::core::ffi::c_int {
        log_debug(format_args!(
            "{}: {}",
            "cmd_find_get_pane_with_session",
            log_cstr(pane)
        ));
        if pane.to_bytes().first() == Some(&b'%') {
            let pane_owner = std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id_str(pane);
            self.wp = pane_owner
                .as_ref()
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
            if self.pane_handle().is_none() {
                return -(1 as ::core::ffi::c_int);
            }
            self.w = self.pane_handle().expect("target pane").window_observer();
            return self.best_winlink_with_window();
        }
        self.set_wl(
            (self
                .session_handle()
                .expect("live session")
                .current_winlink())
            .clone(),
        );
        self.idx = (self.winlink_handle()).get_unchecked().idx;
        self.set_w((self.winlink_handle()).get_unchecked().window_handle());
        self.get_pane_with_window(pane)
    }
    unsafe fn get_pane_with_window(&mut self, pane: &CStr) -> ::core::ffi::c_int {
        let mut idx: ::core::ffi::c_int = 0;
        let mut n: u_int = 0;
        log_debug(format_args!(
            "{}: {}",
            "cmd_find_get_pane_with_window",
            log_cstr(pane)
        ));
        let first = pane.to_bytes().first().copied();
        if first == Some(b'%') {
            let pane_owner = std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id_str(pane);
            self.wp = pane_owner
                .as_ref()
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
            let Some(pane_owner) = pane_owner else {
                return -1;
            };
            if !pane_owner.window_observer().ptr_eq(&self.w) {
                return -1;
            }
            return 0 as ::core::ffi::c_int;
        }
        if pane == c"!" {
            let window = self.window_handle().expect("target window");
            self.wp = window.last_active_pane();
            drop(window);
            if self.pane_handle().is_none() {
                return -(1 as ::core::ffi::c_int);
            }
            return 0 as ::core::ffi::c_int;
        } else if let Some(direction) = [
            (c"{left-of}", Direction::Left),
            (c"{right-of}", Direction::Right),
            (c"{up-of}", Direction::Up),
            (c"{down-of}", Direction::Down),
        ]
        .into_iter()
        .find(|(token, _)| pane == *token)
        .map(|(_, direction)| direction)
        {
            let window_owner = self.w.upgrade().expect("target window");
            self.wp = window_owner
                .active_pane()
                .upgrade()
                .map_or_else(std::rc::Weak::new, |active| {
                    window_owner.pane_in_direction(&active, direction)
                });
            if self.pane_handle().is_none() {
                return -(1 as ::core::ffi::c_int);
            }
            return 0 as ::core::ffi::c_int;
        }
        if first == Some(b'+') || first == Some(b'-') {
            if pane.to_bytes().len() > 1 {
                let parsed = strtonum(
                    cstr_suffix(pane, 1),
                    1 as ::core::ffi::c_longlong,
                    INT_MAX as ::core::ffi::c_longlong,
                );
                n = parsed.unwrap_or(0) as u_int;
                if parsed.is_err() {
                    return -(1 as ::core::ffi::c_int);
                }
            } else {
                n = 1 as u_int;
            }
            let window_owner = self.w.upgrade().expect("target window");
            let active_owner = window_owner.active_pane().upgrade();
            self.wp = if first == Some(b'+') {
                window_owner.pane_by_number(active_owner.as_ref(), n, false)
            } else {
                window_owner.pane_by_number(active_owner.as_ref(), n, true)
            };
            if !self.pane_handle().is_none() {
                return 0 as ::core::ffi::c_int;
            }
        }
        let parsed = strtonum(
            pane,
            0 as ::core::ffi::c_longlong,
            INT_MAX as ::core::ffi::c_longlong,
        );
        idx = parsed.unwrap_or(0) as ::core::ffi::c_int;
        if parsed.is_ok() {
            let window_owner = self.w.upgrade().expect("target window");
            self.wp = window_owner.pane_at_index(idx as u_int);
            if !self.pane_handle().is_none() {
                return 0 as ::core::ffi::c_int;
            }
        }
        let window_owner = self.w.upgrade().expect("target window");
        self.wp = window_owner.find_pane(pane);
        if !self.pane_handle().is_none() {
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
    pub unsafe fn is_valid(&self) -> bool {
        let (Some(s), Some(w), Some(_wp)) = (self.s.upgrade(), self.w.upgrade(), self.wp.upgrade())
        else {
            return false;
        };
        let Ok(wl) = self.wl.try_borrow_mut() else {
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
    pub fn copy_from(&mut self, src: &Self) {
        let source = src.clone();
        self.s = source.s;
        self.wl = source.wl;
        self.idx = source.idx;
        self.w = source.w;
        self.wp = source.wp;
    }
    unsafe fn log_state(&self, prefix: &CStr) {
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
                self.winlink_handle().get_unchecked().idx as u32,
                same(
                    self.winlink_handle().get_unchecked().window_handle(),
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
    pub fn is_empty(&self) -> bool {
        self.s.ptr_eq(&std::rc::Weak::new())
            && self.wl.is_empty()
            && self.w.ptr_eq(&WindowWeak::new())
            && self.wp.ptr_eq(&std::rc::Weak::new())
    }
    pub unsafe fn find_from_session(s_owner: &SessionRef, flags: ::core::ffi::c_int) -> Self {
        let mut fs = Self::clear_state(flags);
        fs.set_s(Some(s_owner));
        fs.set_wl(s_owner.current_winlink().clone());
        fs.set_w(fs.winlink_handle().get_unchecked().window_handle());
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
    pub unsafe fn find_from_winlink(wl: refbox::Weak<winlink>, flags: ::core::ffi::c_int) -> Self {
        let mut fs = Self::clear_state(flags);
        fs.s = wl.get_unchecked().session.clone();
        fs.set_w(wl.get_unchecked().window_handle());
        fs.set_wp(
            wl.get_unchecked()
                .window_handle()
                .expect("live window")
                .active_pane()
                .upgrade()
                .as_ref(),
        );
        fs.set_wl(wl);
        fs.log_state(c"cmd_find_from_winlink");
        fs
    }
    pub unsafe fn find_from_session_window(
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
    pub unsafe fn find_from_window(w_owner: &WindowRef, flags: ::core::ffi::c_int) -> Option<Self> {
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
    pub unsafe fn find_from_winlink_pane(
        wl: refbox::Weak<winlink>,
        wp_owner: &std::rc::Rc<std::cell::RefCell<window_pane>>,
        flags: ::core::ffi::c_int,
    ) -> Self {
        let mut fs = Self::clear_state(flags);
        fs.s = wl.get_unchecked().session.clone();
        fs.idx = wl.get_unchecked().idx;
        fs.set_w(wl.get_unchecked().window_handle());
        fs.set_wl(wl);
        fs.set_wp(Some(wp_owner));
        fs.log_state(c"cmd_find_from_winlink_pane");
        fs
    }
    pub unsafe fn find_from_pane(
        pane: &std::rc::Rc<std::cell::RefCell<window_pane>>,
        flags: ::core::ffi::c_int,
    ) -> Option<Self> {
        let window = pane.window_observer().upgrade().expect("pane window");
        let mut fs = Self::find_from_window(&window, flags)?;
        drop(window);
        fs.set_wp(Some(pane));
        fs.log_state(c"cmd_find_from_pane");
        Some(fs)
    }
    pub unsafe fn find_from_nothing(flags: ::core::ffi::c_int) -> Option<Self> {
        let mut fs = Self::clear_state(flags);
        let session = cmd_find_best_session(None, flags)?;
        fs.set_s(Some(&session));
        fs.set_wl(session.current_winlink().clone());
        fs.idx = fs.winlink_handle().get_unchecked().idx;
        fs.set_w(fs.winlink_handle().get_unchecked().window_handle());
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
    pub unsafe fn find_from_mouse(m: &mouse_event) -> Option<Self> {
        if m.valid == 0 {
            return None;
        }
        let mut fs = Self::clear_state(0);
        let mut wl = refbox::Weak::new();
        let mut mouse_session_owner = None;
        let mouse_pane_owner = cmd_mouse_pane(m, Some(&mut mouse_session_owner), Some(&mut wl));
        mouse_pane_owner.as_ref()?;
        fs.set_s(mouse_session_owner.as_ref());
        fs.set_wl(wl);
        fs.set_wp(mouse_pane_owner.as_ref());
        fs.set_w(fs.winlink_handle().get_unchecked().window_handle());
        fs.log_state(c"cmd_find_from_mouse");
        Some(fs)
    }
    pub unsafe fn find_from_client(
        c_owner: Option<&ClientRef>,
        flags: ::core::ffi::c_int,
    ) -> Option<Self> {
        let Some(c) = c_owner else {
            return Self::find_from_nothing(flags);
        };
        if let Some(session) = c.attached_session().upgrade() {
            let wl = session.current_winlink().clone();
            let Some(pane) = wl
                .get_unchecked()
                .window_handle()
                .expect("live window")
                .active_pane()
                .upgrade()
            else {
                return Some(Self::find_from_session(&session, flags));
            };
            let mut fs = Self::clear_state(flags);
            fs.set_s(Some(&session));
            fs.set_w(wl.get_unchecked().window_handle());
            fs.set_wl(wl);
            fs.set_wp(Some(&pane));
            fs.log_state(c"cmd_find_from_client");
            return Some(fs);
        }
        if let Some(pane) = cmd_find_inside_pane(c_owner) {
            let mut fs = Self::clear_state(flags);
            fs.w = pane.window_observer();
            if fs.best_session_with_window() == 0 {
                fs.set_wl(
                    fs.session_handle()
                        .expect("live session")
                        .current_winlink()
                        .clone(),
                );
                fs.set_w(fs.winlink_handle().get_unchecked().window_handle());
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
        Self::find_from_nothing(flags)
    }
    pub unsafe fn find_target(
        &mut self,
        item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
        target: Option<&CStr>,
        mut type_0: cmd_find_type,
        mut flags: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int {
        let item = item_handle.map(|item| item.borrow());
        let mouse_pane_owner;
        let queue_client = cmdq_get_client(item.as_deref());
        let mut queue_client_ptr: Option<ClientRef> = queue_client.clone();
        let mut current_block: u64;
        let mut c: Option<ClientRef> = None;

        let mut window_only: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
        let mut pane_only: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
        if flags & CMD_FIND_CANFAIL != 0 {
            flags |= CMD_FIND_QUIET;
        }
        let s: &CStr = if type_0 as ::core::ffi::c_uint
            == CMD_FIND_PANE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            c"pane"
        } else if type_0 as ::core::ffi::c_uint
            == CMD_FIND_WINDOW as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            c"window"
        } else if type_0 as ::core::ffi::c_uint
            == CMD_FIND_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            c"session"
        } else {
            c"unknown"
        };
        let mut tmp = Vec::new();
        for (flag, name) in [
            (CMD_FIND_PREFER_UNATTACHED, b"PREFER_UNATTACHED,".as_slice()),
            (CMD_FIND_QUIET, b"QUIET,"),
            (CMD_FIND_WINDOW_INDEX, b"WINDOW_INDEX,"),
            (CMD_FIND_DEFAULT_MARKED, b"DEFAULT_MARKED,"),
            (CMD_FIND_EXACT_SESSION, b"EXACT_SESSION,"),
            (CMD_FIND_EXACT_WINDOW, b"EXACT_WINDOW,"),
            (CMD_FIND_CANFAIL, b"CANFAIL,"),
        ] {
            if flags & flag != 0 {
                tmp.extend_from_slice(name);
            }
        }
        if tmp.pop().is_none() {
            tmp.extend_from_slice(b"NONE");
        }
        log_debug(format_args!(
            "{}: target {}, type {}, item {}, flags {}",
            "cmd_find_target",
            log_cstr(target.unwrap_or(c"none")),
            log_cstr(s),
            log_pointer(item_handle.map_or(std::ptr::null(), |item| item.as_ptr())),
            log_bytes(&tmp)
        ));
        let item = item.expect("command queue item");
        let queue_current = cmdq_get_state_owned(&item).current_snapshot();
        let mut queue_event = cmdq_get_event(&item);
        drop(item);
        let mut current_context: Option<cmd_find_state> = None;
        *self = Self::clear_state(flags);
        if server_check_marked() != 0 && flags & CMD_FIND_DEFAULT_MARKED != 0 {
            current_context = Some(marked_pane.clone());
            log_debug(format_args!(
                "{}: current is marked pane",
                "cmd_find_target"
            ));
            current_block = 1836292691772056875;
        } else if queue_current.is_valid() {
            current_context = Some(queue_current);
            log_debug(format_args!("{}: current is from queue", "cmd_find_target"));
            current_block = 1836292691772056875;
        } else if let Some(current) =
            cmd_find_state::find_from_client(queue_client_ptr.as_ref(), flags)
        {
            current_context = Some(current);
            log_debug(format_args!(
                "{}: current is from client",
                "cmd_find_target"
            ));
            current_block = 1836292691772056875;
        } else {
            if !flags & CMD_FIND_QUIET != 0 {
                cmdq_error(item_handle.expect("command queue item"), |out| {
                    out.write_all(b"no current target")
                });
            }
            current_block = 5193823237153215208;
        }
        if current_block == 1836292691772056875 {
            let current_context = current_context.as_ref().expect("selected current target");
            if !current_context.is_valid() {
                fatalx(|out| out.write_all(b"invalid current find state"));
            }
            if target.is_none_or(|target| target.is_empty()) {
                current_block = 6284300254771030961;
            } else if target == Some(c"@")
                || target == Some(c"{active}")
                || target == Some(c"{current}")
            {
                c = queue_client_ptr;
                if let Some(client) = c
                    .as_ref()
                    .filter(|client| client.attached_session().upgrade().is_some())
                {
                    self.set_wl(
                        (client
                            .attached_session()
                            .upgrade()
                            .expect("live session")
                            .current_winlink())
                        .clone(),
                    );
                    self.set_wp(
                        ((((client
                            .attached_session()
                            .upgrade()
                            .expect("live session")
                            .current_winlink())
                        .get_unchecked()
                        .window_handle()
                        .as_ref())
                        .expect("live window"))
                        .active_pane()
                        .upgrade())
                        .as_ref(),
                    );
                    self.set_w(
                        (client
                            .attached_session()
                            .upgrade()
                            .expect("live session")
                            .current_winlink())
                        .get_unchecked()
                        .window_handle(),
                    );
                    current_block = 15319680530019787978;
                } else {
                    cmdq_error(item_handle.expect("command queue item"), |out| {
                        out.write_all(b"no current client")
                    });
                    current_block = 5193823237153215208;
                }
            } else if target == Some(c"=") || target == Some(c"{mouse}") {
                let m = &queue_event.m;
                let mut current_block_56: u64;
                match type_0 as ::core::ffi::c_uint {
                    0 => {
                        {
                            let mut wl = refbox::Weak::new();
                            let mut mouse_session_owner = None;
                            mouse_pane_owner =
                                cmd_mouse_pane(m, Some(&mut mouse_session_owner), Some(&mut wl));
                            self.set_s(mouse_session_owner.as_ref());
                            self.set_wl(wl.clone());
                            self.set_wp(mouse_pane_owner.as_ref());
                        }
                        if !self.pane_handle().is_none() {
                            self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                            current_block_56 = 7343950298149844727;
                        } else {
                            current_block_56 = 2308649987175926278;
                        }
                    }
                    1 | 2 => {
                        current_block_56 = 2308649987175926278;
                    }
                    _ => {
                        current_block_56 = 7343950298149844727;
                    }
                }
                if current_block_56 == 2308649987175926278 {
                    {
                        let mut mouse_session_owner = None;
                        let wl = cmd_mouse_window(m, Some(&mut mouse_session_owner));
                        self.set_s(mouse_session_owner.as_ref());
                        self.set_wl(wl.clone());
                    }
                    if !self.winlink_handle().is_alive() && !self.session_handle().is_none() {
                        self.set_wl(
                            (self
                                .session_handle()
                                .expect("live session")
                                .current_winlink())
                            .clone(),
                        );
                    }
                    if self.winlink_handle().is_alive() {
                        self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                        self.set_wp(
                            (((self.window_handle().as_ref()).expect("live window"))
                                .active_pane()
                                .upgrade())
                            .as_ref(),
                        );
                    }
                }
                if self.pane_handle().is_none() {
                    if !flags & CMD_FIND_QUIET != 0 {
                        cmdq_error(item_handle.expect("command queue item"), |out| {
                            out.write_all(b"no mouse target")
                        });
                    }
                    current_block = 5193823237153215208;
                } else {
                    current_block = 15319680530019787978;
                }
            } else if target == Some(c"~") || target == Some(c"{marked}") {
                if server_check_marked() == 0 {
                    if !flags & CMD_FIND_QUIET != 0 {
                        cmdq_error(item_handle.expect("command queue item"), |out| {
                            out.write_all(b"no marked target")
                        });
                    }
                    current_block = 5193823237153215208;
                } else {
                    self.copy_from(&marked_pane);
                    current_block = 15319680530019787978;
                }
            } else {
                let target = target.expect("non-empty target");
                fn split(bytes: &[u8], separator: u8) -> (&[u8], Option<&[u8]>) {
                    match bytes.iter().position(|&byte| byte == separator) {
                        Some(at) => (&bytes[..at], Some(&bytes[at + 1..])),
                        None => (bytes, None),
                    }
                }
                let (copy, colon) = split(target.to_bytes(), b':');
                let (copy, colon, period) = match colon {
                    Some(colon) => {
                        let (colon, period) = split(colon, b'.');
                        (copy, Some(colon), period)
                    }
                    None => {
                        let (copy, period) = split(copy, b'.');
                        (copy, None, period)
                    }
                };
                let (mut session, mut window, pane) = match (colon, period) {
                    (Some(colon), Some(period)) => {
                        window_only = 1 as ::core::ffi::c_int;
                        pane_only = 1 as ::core::ffi::c_int;
                        (Some(copy), Some(colon), Some(period))
                    }
                    (Some(colon), None) => {
                        window_only = 1 as ::core::ffi::c_int;
                        (Some(copy), Some(colon), None)
                    }
                    (None, Some(period)) => {
                        pane_only = 1 as ::core::ffi::c_int;
                        (None, Some(copy), Some(period))
                    }
                    (None, None) => match copy.first() {
                        Some(b'$') => (Some(copy), None, None),
                        Some(b'@') => (None, Some(copy), None),
                        Some(b'%') => (None, None, Some(copy)),
                        _ => match type_0 as ::core::ffi::c_uint {
                            2 => (Some(copy), None, None),
                            1 => (None, Some(copy), None),
                            0 => (None, None, Some(copy)),
                            _ => (None, None, None),
                        },
                    },
                };
                if let Some(exact) = session.and_then(|session| session.strip_prefix(b"=")) {
                    session = Some(exact);
                    self.flags |= CMD_FIND_EXACT_SESSION;
                }
                if let Some(exact) = window.and_then(|window| window.strip_prefix(b"=")) {
                    window = Some(exact);
                    self.flags |= CMD_FIND_EXACT_WINDOW;
                }
                let owned = |part: Option<&[u8]>| {
                    part.filter(|part| !part.is_empty())
                        .map(|part| CString::new(part).expect("target came from a C string"))
                };
                let (session_owned, window_owned, pane_owned) =
                    (owned(session), owned(window), owned(pane));
                let session = session_owned.as_deref();
                let window = window_owned
                    .as_deref()
                    .map(|window| cmd_find_map_table(cmd_find_window_table, window));
                let pane = pane_owned
                    .as_deref()
                    .map(|pane| cmd_find_map_table(cmd_find_pane_table, pane));
                if session.is_some() || window.is_some() || pane.is_some() {
                    log_debug(format_args!(
                        "{}: target {} is {}{}{}{}{}{}",
                        "cmd_find_target",
                        log_cstr(target),
                        log_cstr(if session.is_none() { c"" } else { c"session " }),
                        log_cstr(session.unwrap_or(c"")),
                        log_cstr(if window.is_none() { c"" } else { c"window " }),
                        log_cstr(window.unwrap_or(c"")),
                        log_cstr(if pane.is_none() { c"" } else { c"pane " }),
                        log_cstr(pane.unwrap_or(c""))
                    ));
                }
                if pane.is_some() && flags & CMD_FIND_WINDOW_INDEX != 0 {
                    if !flags & CMD_FIND_QUIET != 0 {
                        cmdq_error(item_handle.expect("command queue item"), |out| {
                            out.write_all(b"can't specify pane here")
                        });
                    }
                    current_block = 5193823237153215208;
                } else {
                    match (session, window, pane) {
                        (Some(session), _, _) if self.get_session(session) != 0 => {
                            if !flags & CMD_FIND_QUIET != 0 {
                                cmdq_error(item_handle.expect("command queue item"), |out| {
                                    out.write_all(b"can't find session: ")?;
                                    write_cstr(out, session)
                                });
                            }
                            current_block = 5193823237153215208;
                        }
                        (Some(_), None, None) => {
                            self.set_wl(
                                (self
                                    .session_handle()
                                    .expect("live session")
                                    .current_winlink())
                                .clone(),
                            );
                            self.idx = -(1 as ::core::ffi::c_int);
                            self.set_w((self.winlink_handle()).get_unchecked().window_handle());
                            self.set_wp(
                                (((self.window_handle().as_ref()).expect("live window"))
                                    .active_pane()
                                    .upgrade())
                                .as_ref(),
                            );
                            current_block = 15319680530019787978;
                        }
                        (Some(_), Some(window), None) => {
                            if self.get_window_with_session(window) != 0 as ::core::ffi::c_int {
                                current_block = 2743676411188200708;
                            } else {
                                if self.winlink_handle().is_alive() {
                                    self.set_wp(
                                        ((((self.winlink_handle())
                                            .get_unchecked()
                                            .window_handle()
                                            .as_ref())
                                        .expect("live window"))
                                        .active_pane()
                                        .upgrade())
                                        .as_ref(),
                                    );
                                }
                                current_block = 15319680530019787978;
                            }
                        }
                        (Some(_), None, Some(pane)) => {
                            if self.get_pane_with_session(pane) != 0 as ::core::ffi::c_int {
                                current_block = 14917847580669770662;
                            } else {
                                current_block = 15319680530019787978;
                            }
                        }
                        (Some(_), Some(window), Some(pane)) => {
                            if self.get_window_with_session(window) != 0 as ::core::ffi::c_int {
                                current_block = 2743676411188200708;
                            } else if self.get_pane_with_window(pane) != 0 as ::core::ffi::c_int {
                                current_block = 14917847580669770662;
                            } else {
                                current_block = 15319680530019787978;
                            }
                        }
                        (None, Some(window), Some(pane)) => {
                            if self.get_window(window, window_only, current_context)
                                != 0 as ::core::ffi::c_int
                            {
                                current_block = 2743676411188200708;
                            } else if self.get_pane_with_window(pane) != 0 as ::core::ffi::c_int {
                                current_block = 14917847580669770662;
                            } else {
                                current_block = 15319680530019787978;
                            }
                        }
                        (None, Some(window), None) => {
                            if self.get_window(window, window_only, current_context)
                                != 0 as ::core::ffi::c_int
                            {
                                current_block = 2743676411188200708;
                            } else {
                                if self.winlink_handle().is_alive() {
                                    self.set_wp(
                                        ((((self.winlink_handle())
                                            .get_unchecked()
                                            .window_handle()
                                            .as_ref())
                                        .expect("live window"))
                                        .active_pane()
                                        .upgrade())
                                        .as_ref(),
                                    );
                                }
                                current_block = 15319680530019787978;
                            }
                        }
                        (None, None, Some(pane)) => {
                            if self.get_pane(pane, pane_only, current_context)
                                != 0 as ::core::ffi::c_int
                            {
                                current_block = 14917847580669770662;
                            } else {
                                current_block = 15319680530019787978;
                            }
                        }
                        (None, None, None) => {
                            current_block = 6284300254771030961;
                        }
                    }
                    match current_block {
                        5193823237153215208 => {}
                        15319680530019787978 => {}
                        6284300254771030961 => {}
                        _ => {
                            match current_block {
                                2743676411188200708 => {
                                    if !flags & CMD_FIND_QUIET != 0 {
                                        cmdq_error(
                                            item_handle.expect("command queue item"),
                                            |out| {
                                                out.write_all(b"can't find window: ")?;
                                                write_cstr(out, window)
                                            },
                                        );
                                    }
                                }
                                _ => {
                                    if !flags & CMD_FIND_QUIET != 0 {
                                        cmdq_error(
                                            item_handle.expect("command queue item"),
                                            |out| {
                                                out.write_all(b"can't find pane: ")?;
                                                write_cstr(out, pane)
                                            },
                                        );
                                    }
                                }
                            }
                            current_block = 5193823237153215208;
                        }
                    }
                }
            }
            match current_block {
                5193823237153215208 => {}
                _ => {
                    if current_block == 6284300254771030961 {
                        self.copy_from(current_context);
                        if flags & CMD_FIND_WINDOW_INDEX != 0 {
                            self.idx = -(1 as ::core::ffi::c_int);
                        }
                    }
                    self.log_state(c"cmd_find_target");
                    return 0 as ::core::ffi::c_int;
                }
            }
        }
        log_debug(format_args!("{}: error", "cmd_find_target"));
        if flags & CMD_FIND_CANFAIL != 0 {
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
}
unsafe fn cmd_find_current_client(
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    mut quiet: ::core::ffi::c_int,
) -> Option<ClientRef> {
    let inside_pane_owner;
    let mut c: Option<ClientRef> = None;
    let mut found = None;
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut c_owner = None;
    if let Some(item) = item_handle {
        c_owner = cmdq_get_client(Some(&item.borrow()));
        c = c_owner.clone();
    }
    if !c.is_none()
        && !c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
    {
        return c_owner;
    }
    found = None;
    if !c.is_none() && {
        inside_pane_owner = cmd_find_inside_pane(c.as_ref());

        inside_pane_owner.is_some()
    } {
        fs = cmd_find_state::clear_state(CMD_FIND_QUIET);
        fs.w = inside_pane_owner
            .as_ref()
            .expect("inside pane")
            .window_observer();
        if fs.best_session_with_window() == 0 as ::core::ffi::c_int {
            if let Some(session_owner) = fs.s.upgrade() {
                found = cmd_find_best_client(&session_owner);
            }
        }
    } else {
        if let Some(session_owner) = cmd_find_best_session(None, CMD_FIND_QUIET) {
            found = cmd_find_best_client(&session_owner);
        }
    }
    if found.is_none() && quiet == 0 {
        if let Some(item) = item_handle {
            cmdq_error(item, |out| out.write_all(b"no current client"));
        }
    }
    log_debug(format_args!(
        "{}: no target, return {}",
        "cmd_find_current_client",
        log_pointer(found.as_ref().map_or(std::ptr::null(), std::rc::Rc::as_ptr))
    ));
    found
}
pub unsafe fn cmd_find_client(
    item_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    target: Option<&CStr>,
    mut quiet: ::core::ffi::c_int,
) -> Option<ClientRef> {
    let Some(target) = target else {
        return cmd_find_current_client(item_handle, quiet);
    };
    let target_bytes = target.to_bytes();
    let trimmed = target_bytes.strip_suffix(b":").unwrap_or(target_bytes);
    let copy = CString::new(trimmed).expect("client target came from a C string");
    let mut found = ClientRegistry::first();
    while let Some(client) = found.as_ref() {
        if client.attached_session().upgrade().is_some() {
            if client
                .name()
                .as_deref()
                .is_some_and(|name| name == copy.as_c_str())
            {
                break;
            }
            if let Some(tty) = client.tty_name() {
                let tty = tty.as_bytes();
                if !tty.is_empty()
                    && (tty == trimmed || tty.strip_prefix(b"/dev/") == Some(trimmed))
                {
                    break;
                }
            }
        }
        found = ClientRegistry::next(client);
    }
    if found.is_none() && quiet == 0 {
        cmdq_error(item_handle.expect("command queue item"), |out| {
            out.write_all(b"can't find client: ")?;
            out.write_all(copy.as_bytes())
        });
    }
    log_debug(format_args!(
        "cmd_find_client: target {}, return {}",
        log_cstr(target),
        log_pointer(found.as_ref().map_or(std::ptr::null(), Rc::as_ptr))
    ));
    found
}
