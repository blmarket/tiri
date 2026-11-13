use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_event, cmdq_get_state_owned};
use crate::cmd::{cmd_mouse_pane, cmd_mouse_window};
use crate::compat::strtonum::strtonum;
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
use crate::tiri_sys::fnmatch;
use crate::window::Window;
use crate::window::{
    winlink_find_by_index, winlink_next, winlink_next_by_number, winlink_previous_by_number,
    winlinks_minmax,
};
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use std::ffi::{CStr, CString};
use std::rc::Rc;
pub use tiri_core::shared::target::{
    best_client as cmd_find_best_client, best_session as cmd_find_best_session,
    inside_pane as cmd_find_inside_pane,
};

use crate::shared::abi::u_int;
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_SESSION, CMD_FIND_WINDOW, cmd_find_type};
use crate::shared::command::{cmd_find_state, cmdq_item};
use crate::shared::command::{
    CMD_FIND_CANFAIL, CMD_FIND_DEFAULT_MARKED, CMD_FIND_EXACT_SESSION, CMD_FIND_EXACT_WINDOW,
    CMD_FIND_PREFER_UNATTACHED, CMD_FIND_QUIET, CMD_FIND_WINDOW_INDEX,
};
use crate::shared::limits::INT_MAX;
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

fn cmd_find_map_table<'a>(table: &[(&'static CStr, &'static CStr)], target: &'a CStr) -> &'a CStr {
    table
        .iter()
        .find_map(|(source, mapped)| (*source == target).then_some(*mapped))
        .unwrap_or(target)
}
pub trait TargetLookup: Sized {
    fn get_session(&mut self, target: &CStr) -> i32;
    fn get_window(
        &mut self,
        window: &CStr,
        only: ::core::ffi::c_int,
        current: &cmd_find_state,
    ) -> ::core::ffi::c_int;
    fn get_window_with_session(&mut self, window: &CStr) -> ::core::ffi::c_int;
    fn get_pane(
        &mut self,
        pane: &CStr,
        only: ::core::ffi::c_int,
        current: &cmd_find_state,
    ) -> ::core::ffi::c_int;
    fn get_pane_with_session(&mut self, pane: &CStr) -> ::core::ffi::c_int;
    fn get_pane_with_window(&mut self, pane: &CStr) -> ::core::ffi::c_int;

    fn find_target(
        &mut self,
        item_handle: Option<&std::rc::Rc<cmdq_item>>,
        target: Option<&CStr>,
        type_0: cmd_find_type,
        flags: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
}

impl TargetLookup for cmd_find_state {
    fn get_session(&mut self, target: &CStr) -> i32 {
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
                    fnmatch(target, &name, 0)
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
    fn get_window(
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
            self.set_w(winlink_window(&self.winlink_handle()).as_ref());
            if !self.flags & CMD_FIND_WINDOW_INDEX != 0 {
                self.idx = (self.winlink_handle())
                    .try_borrow()
                    .expect("live winlink")
                    .idx;
            }
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
    fn get_window_with_session(&mut self, window: &CStr) -> ::core::ffi::c_int {
        let mut wl: crate::refbox::Weak<winlink> = crate::refbox::Weak::new();
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
        self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                    if INT_MAX
                        - (s.current_winlink())
                            .try_borrow()
                            .expect("live winlink")
                            .idx
                        < n
                    {
                        return -(1 as ::core::ffi::c_int);
                    }
                    self.idx = (s.current_winlink())
                        .try_borrow()
                        .expect("live winlink")
                        .idx
                        + n;
                } else {
                    if n > (s.current_winlink())
                        .try_borrow()
                        .expect("live winlink")
                        .idx
                    {
                        return -(1 as ::core::ffi::c_int);
                    }
                    self.idx = (s.current_winlink())
                        .try_borrow()
                        .expect("live winlink")
                        .idx
                        - n;
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
                self.idx = (self.winlink_handle())
                    .try_borrow()
                    .expect("live winlink")
                    .idx;
                self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                self.idx = (self.winlink_handle())
                    .try_borrow()
                    .expect("live winlink")
                    .idx;
                self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                self.idx = (self.winlink_handle())
                    .try_borrow()
                    .expect("live winlink")
                    .idx;
                self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                self.idx = (self.winlink_handle())
                    .try_borrow()
                    .expect("live winlink")
                    .idx;
                self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                    self.idx = (self.winlink_handle())
                        .try_borrow()
                        .expect("live winlink")
                        .idx;
                    self.set_w(winlink_window(&self.winlink_handle()).as_ref());
                    return 0 as ::core::ffi::c_int;
                }
                if self.flags & CMD_FIND_WINDOW_INDEX != 0 {
                    self.idx = idx;
                    return 0 as ::core::ffi::c_int;
                }
            }
        }
        self.set_wl((crate::refbox::Weak::new()).clone());
        wl = self
            .session_handle()
            .expect("window target session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if window
                == winlink_window(&wl)
                    .expect("linked window")
                    .name()
                    .as_c_str()
            {
                if self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.set_wl(wl.clone());
            }
            wl = winlink_next(&wl);
        }
        if self.winlink_handle().is_alive() {
            self.idx = (self.winlink_handle())
                .try_borrow()
                .expect("live winlink")
                .idx;
            self.set_w(winlink_window(&self.winlink_handle()).as_ref());
            return 0 as ::core::ffi::c_int;
        }
        if exact != 0 {
            return -(1 as ::core::ffi::c_int);
        }
        self.set_wl((crate::refbox::Weak::new()).clone());
        wl = self
            .session_handle()
            .expect("window target session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if winlink_window(&wl)
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
            wl = winlink_next(&wl);
        }
        if self.winlink_handle().is_alive() {
            self.idx = (self.winlink_handle())
                .try_borrow()
                .expect("live winlink")
                .idx;
            self.set_w(winlink_window(&self.winlink_handle()).as_ref());
            return 0 as ::core::ffi::c_int;
        }
        self.set_wl((crate::refbox::Weak::new()).clone());
        wl = self
            .session_handle()
            .expect("window target session")
            .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
        while wl.is_alive() {
            if fnmatch(
                window,
                &winlink_window(&wl).expect("linked window").name(),
                0,
            ) {
                if self.winlink_handle().is_alive() {
                    return -(1 as ::core::ffi::c_int);
                }
                self.set_wl(wl.clone());
            }
            wl = winlink_next(&wl);
        }
        if self.winlink_handle().is_alive() {
            self.idx = (self.winlink_handle())
                .try_borrow()
                .expect("live winlink")
                .idx;
            self.set_w(winlink_window(&self.winlink_handle()).as_ref());
            return 0 as ::core::ffi::c_int;
        }
        -(1 as ::core::ffi::c_int)
    }
    fn get_pane(
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
    fn get_pane_with_session(&mut self, pane: &CStr) -> ::core::ffi::c_int {
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
        self.idx = (self.winlink_handle())
            .try_borrow()
            .expect("live winlink")
            .idx;
        self.set_w(winlink_window(&self.winlink_handle()).as_ref());
        self.get_pane_with_window(pane)
    }
    fn get_pane_with_window(&mut self, pane: &CStr) -> ::core::ffi::c_int {
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

    fn find_target(
        &mut self,
        item_handle: Option<&std::rc::Rc<cmdq_item>>,
        target: Option<&CStr>,
        mut type_0: cmd_find_type,
        mut flags: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int {
        let item = item_handle.map(|item| item.borrow());
        let mouse_pane_owner;
        let queue_client = cmdq_get_client(item.as_deref().map(|item| &item.context));
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
            log_pointer(item_handle.map_or(std::ptr::null(), std::rc::Rc::as_ptr)),
            log_bytes(&tmp)
        ));
        let item = item.expect("command queue item");
        let queue_current = cmdq_get_state_owned(&item).current_snapshot();
        let mut queue_event = cmdq_get_event(&item);
        drop(item);
        let mut current_context: Option<cmd_find_state> = None;
        *self = Self::clear_state(flags);
        if server_check_marked() != 0 && flags & CMD_FIND_DEFAULT_MARKED != 0 {
            current_context = Some(marked_pane.with_borrow(Clone::clone));
            log_debug(format_args!(
                "{}: current is marked pane",
                "cmd_find_target"
            ));
            current_block = 1836292691772056875;
        } else if queue_current.is_valid() {
            current_context = Some(queue_current);
            log_debug(format_args!("{}: current is from queue", "cmd_find_target"));
            current_block = 1836292691772056875;
        } else if let Some(current) = cmd_find_state::from_client(queue_client_ptr.as_ref(), flags)
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
                        (((winlink_window(
                            &client
                                .attached_session()
                                .upgrade()
                                .expect("live session")
                                .current_winlink(),
                        )
                        .as_ref())
                        .expect("live window"))
                        .active_pane()
                        .upgrade())
                        .as_ref(),
                    );
                    self.set_w(
                        winlink_window(
                            &client
                                .attached_session()
                                .upgrade()
                                .expect("live session")
                                .current_winlink(),
                        )
                        .as_ref(),
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
                            let mut wl = crate::refbox::Weak::new();
                            let mut mouse_session_owner = None;
                            mouse_pane_owner =
                                cmd_mouse_pane(m, Some(&mut mouse_session_owner), Some(&mut wl));
                            self.set_s(mouse_session_owner.as_ref());
                            self.set_wl(wl.clone());
                            self.set_wp(mouse_pane_owner.as_ref());
                        }
                        if !self.pane_handle().is_none() {
                            self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                        self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                    marked_pane.with_borrow(|marked| self.copy_from(marked));
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
                            self.set_w(winlink_window(&self.winlink_handle()).as_ref());
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
                                        (((winlink_window(&self.winlink_handle()).as_ref())
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
                                        (((winlink_window(&self.winlink_handle()).as_ref())
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
fn cmd_find_current_client(
    item_handle: Option<&std::rc::Rc<cmdq_item>>,
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
pub fn cmd_find_client(
    item_handle: Option<&std::rc::Rc<cmdq_item>>,
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
