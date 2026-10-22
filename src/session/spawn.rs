//! Session-owned window creation/replacement transaction. Link index, current
//! selection, MRU removal, and group synchronization remain inside Session.
use crate::shared::window::{WindowRef, WindowWeak};
use crate::winlink::{winlink_index, winlink_window};

use super::*;
use crate::events::events_fire_window;
use crate::resize::default_window_size;
use crate::shared::spawn::{
    spawn_context, SPAWN_DETACHED, SPAWN_KILL, SPAWN_NONOTIFY, SPAWN_RESPAWN,
};
use crate::spawn::{initialize_spawned_window, prepare_respawn_window, set_spawn_cause, spawn_log};
use crate::window::Window;

pub(super) unsafe fn spawn_window(
    sc: &mut spawn_context,
    cause: &mut Option<CString>,
) -> refbox::Weak<winlink> {
    let session = sc.s.upgrade().expect("spawn context session");
    let target_client = sc.tc.upgrade();
    let mut index = sc.idx;
    let mut created_window = WindowWeak::new();
    // A replaced window is released only once its replacement is current, so
    // its window-closed hook resolves a target as it does in tmux.
    let mut replaced_window = None;
    spawn_log(c"spawn_window", sc);

    if sc.flags & SPAWN_RESPAWN != 0 && !prepare_respawn_window(sc, cause) {
        return refbox::Weak::new();
    }
    if sc.flags & SPAWN_RESPAWN == 0 && index != -1 {
        let mut link = session.with_winlinks(|links| winlink_find_by_index(links, index));
        if link.is_alive() && sc.flags & SPAWN_KILL == 0 {
            set_spawn_cause(
                Some(cause),
                &[b"index ", index.to_string().as_bytes(), b" in use"],
            );
            return refbox::Weak::new();
        }
        if link.is_alive() {
            link.try_borrow_mut().expect("live winlink").flags &= !WINLINK_ALERTFLAGS;
            events_fire_winlink(c"window-unlinked", link.clone());
            winlink_stack_remove(&mut session.borrow_mut().lastw, link.clone());
            replaced_window = winlink_window(&link);
            session_winlink_remove(&session, link.clone());
            if session.current_winlink() == link {
                session.borrow_mut().curw = refbox::Weak::new();
                sc.flags &= !SPAWN_DETACHED;
            }
        }
    }
    if sc.flags & SPAWN_RESPAWN == 0 {
        if index == -1 {
            index =
                (-1i64 - options_get_number(&session.borrow_options_mut(), c"base-index")) as i32;
        }
        sc.set_wl(winlink_add(&session.borrow().windows, index));
        if !sc.winlink_handle().is_alive() {
            set_spawn_cause(
                Some(cause),
                &[b"couldn't add window ", index.to_string().as_bytes()],
            );
            return refbox::Weak::new();
        }
        let (mut sx, mut sy, mut xpixel, mut ypixel) = (0, 0, 0, 0);
        default_window_size(
            target_client.as_ref(),
            &session,
            None,
            &mut sx,
            &mut sy,
            &mut xpixel,
            &mut ypixel,
            -1,
        );
        let window = crate::window::window_create(sx, sy, xpixel, ypixel);
        created_window = WindowRef::downgrade(&window);
        if !session.current_winlink().is_alive() {
            session.borrow_mut().curw = sc.winlink_handle();
        }
        sc.winlink_handle()
            .try_borrow_mut()
            .expect("live winlink")
            .session = Rc::downgrade(&session);
        window.set_latest_client(target_client.as_ref());
        winlink_set_window(sc.winlink_handle(), &window);
        drop(window);
    }

    sc.flags |= SPAWN_NONOTIFY;
    let pane =
        <Rc<std::cell::RefCell<window_pane>> as crate::window_pane::WindowPane>::spawn_process(
            sc, cause,
        );
    if pane.is_none() {
        if sc.flags & SPAWN_RESPAWN == 0 {
            session_winlink_remove(&session, sc.winlink_handle());
        }
        return refbox::Weak::new();
    }
    if sc.flags & SPAWN_RESPAWN == 0 {
        let window = created_window
            .upgrade()
            .expect("spawned window remains live");
        initialize_spawned_window(sc, &window);
        drop(window);
    }
    if sc.flags & SPAWN_DETACHED == 0 {
        session_select(&session, winlink_index(&sc.winlink_handle()));
    }
    drop(replaced_window);
    if sc.flags & SPAWN_RESPAWN == 0 {
        let window = created_window
            .upgrade()
            .expect("spawned window remains live");
        events_fire_window(c"window-created", window);
        events_fire_winlink(c"window-linked", sc.winlink_handle());
    }
    session_group_synchronize_from(&session);
    sc.winlink_handle()
}
