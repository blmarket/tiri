use crate::src::options::{options_get_number, options_get_string};
use crate::src::resize::recalculate_sizes;
use crate::src::server::marked_pane;
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use crate::src::session::session_group_count;
use crate::src::session::sessions;
use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::WindowRef;
use crate::src::window::{winlink_find_by_index, winlink_find_by_window};
use crate::src::window_pane::WindowPane as _;

use crate::src::shared::abi::*;
use crate::src::shared::client::{
    CLIENT_ALLREDRAWFLAGS, CLIENT_CONTROL, CLIENT_NO_DETACH_ON_DESTROY, CLIENT_REDRAWBORDERS,
    CLIENT_REDRAWMENU, CLIENT_REDRAWSTATUS, CLIENT_SUSPENDED,
};
use crate::src::shared::pane::window_pane;
use crate::src::shared::sort::sort_criteria;
use crate::src::shared::sort::*;
use crate::src::shared::window::winlink;

pub unsafe fn server_redraw_client(c: &ClientRef) {
    c.request_redraw(CLIENT_ALLREDRAWFLAGS as u64);
}
pub unsafe fn server_status_client(c: &ClientRef) {
    c.request_redraw(CLIENT_REDRAWSTATUS as u64);
}
pub unsafe fn server_redraw_session(session: &SessionRef) {
    let mut next = ClientRegistry::first();
    while let Some(client_owner) = next {
        next = ClientRegistry::next(&client_owner);
        let client = &client_owner;
        if client
            .attached_session()
            .ptr_eq(&std::rc::Rc::downgrade(session))
        {
            server_redraw_client(client);
        }
    }
}
pub unsafe fn server_redraw_session_group(session: &SessionRef) {
    let group = crate::src::session::session_groups::for_session(&std::rc::Rc::downgrade(session));
    if !group.is_alive() {
        server_redraw_session(session);
    } else {
        for owner in crate::src::session::session_group_members(&group) {
            server_redraw_session(&owner);
        }
    }
}

pub unsafe fn server_status_session(session: &SessionRef) {
    let mut next = ClientRegistry::first();
    while let Some(client_owner) = next {
        next = ClientRegistry::next(&client_owner);
        let client = &client_owner;
        if client
            .attached_session()
            .ptr_eq(&std::rc::Rc::downgrade(session))
        {
            client.request_redraw(CLIENT_REDRAWSTATUS as u64);
        }
    }
}
pub unsafe fn server_status_session_group(session: &SessionRef) {
    let group = crate::src::session::session_groups::for_session(&std::rc::Rc::downgrade(session));
    if !group.is_alive() {
        server_status_session(session);
    } else {
        for owner in crate::src::session::session_group_members(&group) {
            server_status_session(&owner);
        }
    }
}

pub unsafe fn server_redraw_window(window: &WindowRef) {
    let mut next = ClientRegistry::first();
    while let Some(client_owner) = next {
        next = ClientRegistry::next(&client_owner);
        let client = &client_owner;
        let matches = client
            .attached_session()
            .upgrade()
            .and_then(|session| {
                session
                    .current_winlink()
                    .try_borrow_mut()
                    .ok()
                    .and_then(|link| link.window_owner.clone())
            })
            .is_some_and(|current| WindowRef::ptr_eq(window, &current));
        if matches {
            server_redraw_client(client);
        }
    }
}
pub unsafe fn server_redraw_window_menu(window_owner: &WindowRef) {
    let mut next = ClientRegistry::first();
    while let Some(client_owner) = next {
        next = ClientRegistry::next(&client_owner);
        let client = &client_owner;
        let matches = client
            .attached_session()
            .upgrade()
            .and_then(|session| {
                session
                    .current_winlink()
                    .try_borrow_mut()
                    .ok()
                    .and_then(|link| link.window_owner.clone())
            })
            .is_some_and(|current| WindowRef::ptr_eq(&current, window_owner));
        if matches {
            client.request_redraw(CLIENT_REDRAWMENU as u64);
        }
    }
}
pub unsafe fn server_redraw_window_borders(window: &WindowRef) {
    let mut next = ClientRegistry::first();
    while let Some(client_owner) = next {
        next = ClientRegistry::next(&client_owner);
        let client = &client_owner;
        let matches = client
            .attached_session()
            .upgrade()
            .and_then(|session| {
                session
                    .current_winlink()
                    .try_borrow_mut()
                    .ok()
                    .and_then(|link| link.window_owner.clone())
            })
            .is_some_and(|current| WindowRef::ptr_eq(window, &current));
        if matches {
            client.request_redraw(CLIENT_REDRAWBORDERS as u64);
        }
    }
}
pub unsafe fn server_status_window(window: &WindowRef) {
    let mut next = sessions::first();
    while let Some(session_owner) = next {
        if session_owner.contains_window(window) {
            server_status_session(&session_owner);
        }
        next = sessions::after(session_owner.name().as_c_str());
    }
}
pub unsafe fn server_lock() {
    let mut next = ClientRegistry::first();
    while let Some(owner) = next {
        if !owner.attached_session().upgrade().is_none() {
            server_lock_client(&owner);
        }
        next = ClientRegistry::next(&owner);
    }
}
pub unsafe fn server_lock_session(session_owner: &SessionRef) {
    let observer = std::rc::Rc::downgrade(session_owner);
    let mut next = ClientRegistry::first();
    while let Some(owner) = next {
        let matches = owner.attached_session().ptr_eq(&observer);
        if matches {
            server_lock_client(&owner);
        }
        next = ClientRegistry::next(&owner);
    }
}
pub unsafe fn server_lock_client(client_owner: &ClientRef) {
    if client_owner.flags() & (CLIENT_CONTROL | CLIENT_SUSPENDED) as u64 != 0 {
        return;
    }
    let command = client_owner
        .attached_session()
        .upgrade()
        .expect("live session")
        .with_options_mut(|options| options_get_string(options, c"lock-command"));
    client_owner.lock(&command);
}

pub unsafe fn server_kill_pane(pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>) {
    pane_owner.kill_process();
}
pub unsafe fn server_kill_window(owner: WindowRef, mut renumber: ::core::ffi::c_int) {
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        let name = s.as_ref().expect("live session").name();
        if !(!s.as_ref().expect("live session").contains_window(&owner)) {
            loop {
                wl = s.as_ref().expect("live session").with_winlinks(|links| {
                    winlink_find_by_window(links, &WindowRef::clone(&(owner)))
                });
                if !wl.is_alive() {
                    break;
                }
                if (s.as_ref().expect("live session")).detach_window(wl.clone()) != 0 {
                    server_destroy_session_group(s.as_ref().expect("live session"));
                    break;
                } else {
                    server_redraw_session_group(s.as_ref().expect("live session"));
                }
            }
            if renumber != 0
                && (s.as_ref().is_some_and(|session| session.is_registered()) as i32) != 0
            {
                server_renumber_session(s.as_ref().expect("live session"));
            }
        }
        s_owner = sessions::after(&name);
        s = s_owner.clone();
    }
    recalculate_sizes();
    drop(owner);
}
pub unsafe fn server_renumber_session(s_owner: &SessionRef) {
    let s = Some(s_owner.clone());
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    if s.as_ref()
        .expect("live session")
        .with_options_mut(|options| options_get_number(options, c"renumber-windows"))
        != 0
    {
        sg = crate::src::session::session_groups::for_session(
            &s.as_ref()
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
        );
        if sg.is_alive() {
            for session_owner in crate::src::session::session_group_members(&sg) {
                session_owner.renumber_windows();
            }
        } else {
            (s.as_ref().expect("live session")).renumber_windows();
        }
    }
}
pub unsafe fn server_renumber_all() {
    let mut s: Option<SessionRef> = None;
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        server_renumber_session(s.as_ref().expect("live session"));
        s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
        s = s_owner.clone();
    }
}
pub unsafe fn server_link_window(
    src_owner: &SessionRef,
    mut srcwl: refbox::Weak<winlink>,
    dst_owner: &SessionRef,
    mut dstidx: ::core::ffi::c_int,
    mut killflag: ::core::ffi::c_int,
    mut selectflag: ::core::ffi::c_int,
) -> Result<(), std::ffi::CString> {
    let src = Some(src_owner.clone());
    let dst = Some(dst_owner.clone());
    let mut dstwl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut srcsg = crate::src::shared::session::SessionGroupWeak::new();
    let mut dstsg = crate::src::shared::session::SessionGroupWeak::new();
    srcsg = crate::src::session::session_groups::for_session(
        &src.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    dstsg = crate::src::session::session_groups::for_session(
        &dst.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    if !std::rc::Rc::ptr_eq(src_owner, dst_owner)
        && srcsg.is_alive()
        && dstsg.is_alive()
        && srcsg == dstsg
    {
        return Err(c"sessions are grouped".to_owned());
    }
    dstwl = refbox::Weak::new();
    if dstidx != -(1 as ::core::ffi::c_int) {
        dstwl = dst_owner.with_winlinks(|links| winlink_find_by_index(links, dstidx));
    }
    if dstwl.is_alive() {
        if crate::src::shared::rc::same(
            dstwl.get_unchecked().window_handle(),
            srcwl.get_unchecked().window_handle(),
        ) {
            return Err(std::ffi::CString::new(format!("same index: {dstidx}"))
                .expect("numeric diagnostic contains no NUL"));
        }
        if killflag != 0 && dst_owner.remove_replaced_window(dstwl.clone()) {
            selectflag = 1;
        }
    }
    if dstidx == -(1 as ::core::ffi::c_int) {
        dstidx = (-(1 as ::core::ffi::c_int) as ::core::ffi::c_longlong
            - dst
                .as_ref()
                .expect("live session")
                .with_options_mut(|options| options_get_number(options, c"base-index")))
            as ::core::ffi::c_int;
    }
    let window_owner = srcwl
        .get_unchecked()
        .window_owner
        .as_ref()
        .expect("source window")
        .clone();
    let attached = (dst.as_ref().expect("live session")).attach_window(&window_owner, dstidx);
    drop(window_owner);
    dstwl = attached?;
    if marked_pane.winlink_handle() == srcwl {
        marked_pane.set_wl((dstwl).clone());
    }
    if selectflag != 0 {
        (dst.as_ref().expect("live session")).select_index(dstwl.get_unchecked().idx);
    }
    server_redraw_session_group(dst.as_ref().expect("live session"));
    Ok(())
}
pub unsafe fn server_unlink_window(s_owner: &SessionRef, mut wl: refbox::Weak<winlink>) {
    let s = Some(s_owner.clone());
    if (s.as_ref().expect("live session")).detach_window(wl.clone()) != 0 {
        server_destroy_session_group(s_owner);
    } else {
        server_redraw_session_group(s.as_ref().expect("live session"));
    };
}
pub unsafe fn server_destroy_pane(
    pane_owner: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    mut notify: ::core::ffi::c_int,
) {
    pane_owner.finish_process(notify);
}
unsafe fn server_destroy_session_group(s_owner: &SessionRef) {
    let s = Some(s_owner.clone());
    let source = s_owner.clone();
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    sg = crate::src::session::session_groups::for_session(
        &s.as_ref()
            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
    );
    if !sg.is_alive() {
        server_destroy_session(&source);
        (&source).destroy(
            (1 as ::core::ffi::c_int) != 0,
            std::ffi::CStr::from_ptr(c"server_destroy_session_group".as_ptr()),
        );
    } else {
        for session_owner in crate::src::session::session_group_members(&sg) {
            server_destroy_session(&session_owner);
            (&session_owner).destroy(
                (1 as ::core::ffi::c_int) != 0,
                std::ffi::CStr::from_ptr(c"server_destroy_session_group".as_ptr()),
            );
        }
    };
}
unsafe fn server_find_session(
    excluded: &SessionRef,
    mut choose: impl FnMut(&SessionRef, Option<&SessionRef>) -> bool,
) -> Option<SessionRef> {
    let mut selected: Option<SessionRef> = None;
    let mut current = sessions::first();
    while let Some(owner) = current {
        current = sessions::after(owner.name().as_c_str());
        if !std::rc::Rc::ptr_eq(&owner, excluded) && choose(&owner, selected.as_ref()) {
            selected = Some(owner);
        }
    }
    selected
}
unsafe fn server_newer_session(s: &SessionRef, other: Option<&SessionRef>) -> bool {
    let Some(other) = other else { return true };
    let (a, b) = (s.activity_time(), other.activity_time());
    a > b
}
unsafe fn server_newer_detached_session(s: &SessionRef, other: Option<&SessionRef>) -> bool {
    !s.is_attached() && server_newer_session(s, other)
}
pub unsafe fn server_destroy_session(source: &SessionRef) {
    let s = Some(source.clone());
    let mut c: Option<ClientRef> = None;
    let mut sort_crit: sort_criteria = sort_criteria {
        order: SORT_NAME,
        reversed: 0,
        order_seq: &[],
    };
    let mut detach_on_destroy: ::core::ffi::c_int = 0;
    detach_on_destroy = s
        .as_ref()
        .expect("live session")
        .with_options_mut(|options| options_get_number(options, c"detach-on-destroy"))
        as ::core::ffi::c_int;
    let mut replacement = match detach_on_destroy {
        0 => server_find_session(s.as_ref().expect("live session"), |a, b| {
            server_newer_session(a, b)
        }),
        2 => server_find_session(s.as_ref().expect("live session"), |a, b| {
            server_newer_detached_session(a, b)
        }),
        3 => (Some(s.as_ref().expect("live session")))
            .and_then(|session| session.adjacent_session(&sort_crit, true)),
        4 => (Some(s.as_ref().expect("live session")))
            .and_then(|session| session.adjacent_session(&sort_crit, false)),
        _ => None,
    };
    if replacement
        .as_ref()
        .is_some_and(|owner| std::rc::Rc::ptr_eq(owner, source))
    {
        replacement = None;
    }
    let fallback = if replacement.is_none() && matches!(detach_on_destroy, 1 | 2) {
        server_find_session(s.as_ref().expect("live session"), |a, b| {
            server_newer_session(a, b)
        })
    } else {
        None
    };
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        if !(!crate::src::shared::rc::same(
            c.as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .as_ref(),
            s.as_ref(),
        )) {
            let target = replacement.as_ref().or_else(|| {
                (c.as_ref().expect("live client").flags() & CLIENT_NO_DETACH_ON_DESTROY != 0)
                    .then_some(fallback.as_ref())
                    .flatten()
            });
            registry_c_owner
                .as_ref()
                .expect("current registry client")
                .reattach_after_session_destroy(target);
        }
        registry_c_owner =
            ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
    recalculate_sizes();
}
pub unsafe fn server_check_unattached() {
    let mut s: Option<SessionRef> = None;
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    let mut current_block_4: u64;
    let mut s_owner = sessions::first();
    s = s_owner.clone();
    while !s.is_none() {
        let name = s.as_ref().expect("live session").name();
        if !(s.as_ref().expect("live session").is_attached()) {
            match s
                .as_ref()
                .expect("live session")
                .with_options_mut(|options| options_get_number(options, c"destroy-unattached"))
            {
                0 => {}
                2 => {
                    current_block_4 = 6116987625208566775;
                    match current_block_4 {
                        11000743977270914936 => {
                            sg = crate::src::session::session_groups::for_session(
                                &s.as_ref()
                                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                            );
                            if sg.is_alive() && session_group_count(&sg) == 1 as u_int {
                                current_block_4 = 16668937799742929182;
                            } else {
                                current_block_4 = 13109137661213826276;
                            }
                        }
                        6116987625208566775 => {
                            sg = crate::src::session::session_groups::for_session(
                                &s.as_ref()
                                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                            );
                            if !sg.is_alive() || session_group_count(&sg) <= 1 as u_int {
                                current_block_4 = 16668937799742929182;
                            } else {
                                current_block_4 = 13109137661213826276;
                            }
                        }
                        _ => {}
                    }
                    match current_block_4 {
                        16668937799742929182 => {}
                        _ => {
                            server_destroy_session(s_owner.as_ref().expect("registered session"));
                            (s_owner.as_ref().expect("registered session")).destroy(
                                (1 as ::core::ffi::c_int) != 0,
                                std::ffi::CStr::from_ptr(c"server_check_unattached".as_ptr()),
                            );
                        }
                    }
                }
                3 => {
                    current_block_4 = 11000743977270914936;
                    match current_block_4 {
                        11000743977270914936 => {
                            sg = crate::src::session::session_groups::for_session(
                                &s.as_ref()
                                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                            );
                            if sg.is_alive() && session_group_count(&sg) == 1 as u_int {
                                current_block_4 = 16668937799742929182;
                            } else {
                                current_block_4 = 13109137661213826276;
                            }
                        }
                        6116987625208566775 => {
                            sg = crate::src::session::session_groups::for_session(
                                &s.as_ref()
                                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                            );
                            if !sg.is_alive() || session_group_count(&sg) <= 1 as u_int {
                                current_block_4 = 16668937799742929182;
                            } else {
                                current_block_4 = 13109137661213826276;
                            }
                        }
                        _ => {}
                    }
                    match current_block_4 {
                        16668937799742929182 => {}
                        _ => {
                            server_destroy_session(s_owner.as_ref().expect("registered session"));
                            (s_owner.as_ref().expect("registered session")).destroy(
                                (1 as ::core::ffi::c_int) != 0,
                                std::ffi::CStr::from_ptr(c"server_check_unattached".as_ptr()),
                            );
                        }
                    }
                }
                _ => {
                    current_block_4 = 13109137661213826276;
                    match current_block_4 {
                        11000743977270914936 => {
                            sg = crate::src::session::session_groups::for_session(
                                &s.as_ref()
                                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                            );
                            if sg.is_alive() && session_group_count(&sg) == 1 as u_int {
                                current_block_4 = 16668937799742929182;
                            } else {
                                current_block_4 = 13109137661213826276;
                            }
                        }
                        6116987625208566775 => {
                            sg = crate::src::session::session_groups::for_session(
                                &s.as_ref()
                                    .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                            );
                            if !sg.is_alive() || session_group_count(&sg) <= 1 as u_int {
                                current_block_4 = 16668937799742929182;
                            } else {
                                current_block_4 = 13109137661213826276;
                            }
                        }
                        _ => {}
                    }
                    match current_block_4 {
                        16668937799742929182 => {}
                        _ => {
                            server_destroy_session(s_owner.as_ref().expect("registered session"));
                            (s_owner.as_ref().expect("registered session")).destroy(
                                (1 as ::core::ffi::c_int) != 0,
                                std::ffi::CStr::from_ptr(c"server_check_unattached".as_ptr()),
                            );
                        }
                    }
                }
            }
        }
        s_owner = sessions::after(&name);
        s = s_owner.clone();
    }
}
