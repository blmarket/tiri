//! Session alert delivery preserves association order and per-cycle deduplication.
use super::*;
use crate::events::events_fire_winlink;
use crate::server_client::Client;
use crate::server_client::ClientRegistry;
use crate::server_fn::server_status_session;
use crate::shared::alerts::{ALERT_ANY, ALERT_CURRENT, ALERT_OTHER};
use crate::shared::window::WindowRef;
use crate::shared::window::{
    WINDOW_ACTIVITY, WINDOW_BELL, WINDOW_SILENCE, WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE,
};
use crate::window::Window;
use std::ffi::CStr;

const SESSION_ALERTED: ::core::ffi::c_int = 0x1;

unsafe fn alerts_action_applies(
    mut wl: refbox::Weak<winlink>,
    name: &'static CStr,
) -> ::core::ffi::c_int {
    let Some(session_owner) = wl.get_unchecked().session.upgrade() else {
        return 0;
    };
    let s = session_owner.as_ptr();
    let mut action: ::core::ffi::c_int = 0;
    action = options_get_number(&session_owner.borrow_options_mut(), name) as ::core::ffi::c_int;
    if action == ALERT_ANY {
        return 1 as ::core::ffi::c_int;
    }
    if action == ALERT_CURRENT {
        return (wl == (*s).current_winlink()) as ::core::ffi::c_int;
    }
    if action == ALERT_OTHER {
        return (wl != (*s).current_winlink()) as ::core::ffi::c_int;
    }
    0 as ::core::ffi::c_int
}

pub(super) unsafe fn alerts_check_all(w_owner: &WindowRef) -> ::core::ffi::c_int {
    let mut alerts: ::core::ffi::c_int = 0;
    alerts = alerts_check_bell(w_owner);
    alerts |= alerts_check_activity(w_owner);
    alerts |= alerts_check_silence(w_owner);
    alerts
}

unsafe fn alerts_check_bell(w_owner: &WindowRef) -> ::core::ffi::c_int {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s: *mut session = ::core::ptr::null_mut::<session>();
    if !w_owner.pending_alerts() & WINDOW_BELL != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if options_get_number(&w_owner.borrow_options_mut(), c"monitor-bell") == 0 {
        return 0 as ::core::ffi::c_int;
    }
    wl = w_owner.next_winlink(None);
    while wl.is_alive() {
        if let Some(session_owner) = wl.get_unchecked().session.upgrade() {
            (*session_owner.as_ptr()).flags &= !SESSION_ALERTED;
        }
        wl = w_owner.next_winlink(Some(wl.clone()));
    }
    wl = w_owner.next_winlink(None);
    while wl.is_alive() {
        let Some(session_owner) = wl.get_unchecked().session.upgrade() else {
            wl = w_owner.next_winlink(Some(wl.clone()));
            continue;
        };
        s = session_owner.as_ptr();
        if (*s).current_winlink() != wl || (*s).attached == 0 as u_int {
            wl.get_mut_unchecked().flags |= WINLINK_BELL;
            server_status_session(&session_owner);
        }
        if !(alerts_action_applies(wl.clone(), c"bell-action") == 0) {
            events_fire_winlink(c"alert-bell", wl.clone());
            if !((*s).flags & SESSION_ALERTED != 0) {
                (*s).flags |= SESSION_ALERTED;
                alerts_set_message(wl.clone(), c"Bell", c"visual-bell");
            }
        }
        wl = w_owner.next_winlink(Some(wl.clone()));
    }
    0x1 as ::core::ffi::c_int
}

unsafe fn alerts_check_activity(w_owner: &WindowRef) -> ::core::ffi::c_int {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s: *mut session = ::core::ptr::null_mut::<session>();
    if !w_owner.pending_alerts() & WINDOW_ACTIVITY != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if options_get_number(&w_owner.borrow_options_mut(), c"monitor-activity") == 0 {
        return 0 as ::core::ffi::c_int;
    }
    wl = w_owner.next_winlink(None);
    while wl.is_alive() {
        if let Some(session_owner) = wl.get_unchecked().session.upgrade() {
            (*session_owner.as_ptr()).flags &= !SESSION_ALERTED;
        }
        wl = w_owner.next_winlink(Some(wl.clone()));
    }
    wl = w_owner.next_winlink(None);
    while wl.is_alive() {
        if !(wl.get_unchecked().flags & WINLINK_ACTIVITY != 0) {
            let Some(session_owner) = wl.get_unchecked().session.upgrade() else {
                wl = w_owner.next_winlink(Some(wl.clone()));
                continue;
            };
            s = session_owner.as_ptr();
            if (*s).current_winlink() != wl || (*s).attached == 0 as u_int {
                wl.get_mut_unchecked().flags |= WINLINK_ACTIVITY;
                server_status_session(&session_owner);
            }
            if !(alerts_action_applies(wl.clone(), c"activity-action") == 0) {
                events_fire_winlink(c"alert-activity", wl.clone());
                if !((*s).flags & SESSION_ALERTED != 0) {
                    (*s).flags |= SESSION_ALERTED;
                    alerts_set_message(wl.clone(), c"Activity", c"visual-activity");
                }
            }
        }
        wl = w_owner.next_winlink(Some(wl.clone()));
    }
    0x2 as ::core::ffi::c_int
}

unsafe fn alerts_check_silence(w_owner: &WindowRef) -> ::core::ffi::c_int {
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s: *mut session = ::core::ptr::null_mut::<session>();
    if !w_owner.pending_alerts() & WINDOW_SILENCE != 0 {
        return 0 as ::core::ffi::c_int;
    }
    if options_get_number(&w_owner.borrow_options_mut(), c"monitor-silence")
        == 0 as ::core::ffi::c_longlong
    {
        return 0 as ::core::ffi::c_int;
    }
    wl = w_owner.next_winlink(None);
    while wl.is_alive() {
        if let Some(session_owner) = wl.get_unchecked().session.upgrade() {
            (*session_owner.as_ptr()).flags &= !SESSION_ALERTED;
        }
        wl = w_owner.next_winlink(Some(wl.clone()));
    }
    wl = w_owner.next_winlink(None);
    while wl.is_alive() {
        if !(wl.get_unchecked().flags & WINLINK_SILENCE != 0) {
            let Some(session_owner) = wl.get_unchecked().session.upgrade() else {
                wl = w_owner.next_winlink(Some(wl.clone()));
                continue;
            };
            s = session_owner.as_ptr();
            if (*s).current_winlink() != wl || (*s).attached == 0 as u_int {
                wl.get_mut_unchecked().flags |= WINLINK_SILENCE;
                server_status_session(&session_owner);
            }
            if !(alerts_action_applies(wl.clone(), c"silence-action") == 0) {
                events_fire_winlink(c"alert-silence", wl.clone());
                if !((*s).flags & SESSION_ALERTED != 0) {
                    (*s).flags |= SESSION_ALERTED;
                    alerts_set_message(wl.clone(), c"Silence", c"visual-silence");
                }
            }
        }
        wl = w_owner.next_winlink(Some(wl.clone()));
    }
    0x4 as ::core::ffi::c_int
}

unsafe fn alerts_set_message(mut wl: refbox::Weak<winlink>, type_0: &CStr, option: &'static CStr) {
    let Some(session_owner) = wl.get_unchecked().session.upgrade() else {
        return;
    };
    let visual = options_get_number(&session_owner.borrow_options_mut(), option) as i32;
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if client
            .attached_session()
            .ptr_eq(&Rc::downgrade(&session_owner))
            && !client.is_control()
        {
            client.alert(
                type_0,
                visual,
                session_owner.current_winlink() == wl,
                wl.get_unchecked().idx,
            );
        }
        cursor = ClientRegistry::next(&client);
    }
}
