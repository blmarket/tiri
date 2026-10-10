//! Session-owned window creation/replacement transaction. Link index, current
//! selection, MRU removal, and group synchronization remain inside Session.

use super::*;
use crate::src::events::events_fire_window;
use crate::src::resize::default_window_size;
use crate::src::shared::spawn::{
    spawn_context, SPAWN_DETACHED, SPAWN_KILL, SPAWN_NONOTIFY, SPAWN_RESPAWN,
};
use crate::src::spawn::{
    initialize_spawned_window, prepare_respawn_window, set_spawn_cause, spawn_log,
};
use crate::src::window::Window;

pub(super) unsafe fn spawn_window(
    sc: &mut spawn_context,
    cause: &mut Option<CString>,
) -> refbox::Weak<winlink> {
    let session = sc.s.upgrade().expect("spawn context session");
    let target_client = sc.tc.upgrade();
    let s = session.get();
    let mut index = sc.idx;
    let mut created_window = std::rc::Weak::new();
    spawn_log(c"spawn_window", sc);

    if sc.flags & SPAWN_RESPAWN != 0 && !prepare_respawn_window(sc, cause) {
        return refbox::Weak::new();
    }
    if sc.flags & SPAWN_RESPAWN == 0 && index != -1 {
        let mut link = winlink_find_by_index(&(*s).windows, index);
        if link.is_alive() && sc.flags & SPAWN_KILL == 0 {
            set_spawn_cause(
                Some(cause),
                &[b"index ", index.to_string().as_bytes(), b" in use"],
            );
            return refbox::Weak::new();
        }
        if link.is_alive() {
            link.get_mut_unchecked().flags &= !WINLINK_ALERTFLAGS;
            events_fire_winlink(c"window-unlinked", link.clone());
            winlink_stack_remove(&mut (*s).lastw, link.clone());
            winlink_remove(&mut (*s).windows, link.clone());
            if (*s).curw == link {
                (*s).curw = refbox::Weak::new();
                sc.flags &= !SPAWN_DETACHED;
            }
        }
    }
    if sc.flags & SPAWN_RESPAWN == 0 {
        if index == -1 {
            index = (-1i64
                - options_get_number(
                    options_owner_ptr(&mut (*s).options).expect("session options"),
                    c"base-index",
                )) as i32;
        }
        sc.set_wl(winlink_add(&mut (*s).windows, index));
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
        let window = crate::src::window::window_create(sx, sy, xpixel, ypixel);
        created_window = Rc::downgrade(&window);
        if !(*s).curw.is_alive() {
            (*s).curw = sc.winlink_handle();
        }
        sc.winlink_handle().get_mut_unchecked().session = Rc::downgrade(&session);
        window.set_latest_client(target_client.as_ref());
        winlink_set_window(sc.winlink_handle(), &window);
        // The new link owns the window before the constructor owner is released.
        crate::src::window::window_remove_ref(window, c"spawn_window");
    }

    sc.flags |= SPAWN_NONOTIFY;
    let pane =
        <Rc<std::cell::UnsafeCell<window_pane>> as crate::src::window_pane::WindowPane>::spawn_process(
            sc, cause,
        );
    if pane.is_none() {
        if sc.flags & SPAWN_RESPAWN == 0 {
            winlink_remove(&mut (*s).windows, sc.winlink_handle());
        }
        return refbox::Weak::new();
    }
    if sc.flags & SPAWN_RESPAWN == 0 {
        let window = created_window
            .upgrade()
            .expect("spawned window remains live");
        initialize_spawned_window(sc, &window);
        crate::src::window::window_remove_ref(window, c"spawn_window initialization");
    }
    if sc.flags & SPAWN_DETACHED == 0 {
        session_select(&session, sc.winlink_handle().get_unchecked().idx);
    }
    if sc.flags & SPAWN_RESPAWN == 0 {
        // Pane and selection callbacks can change the link's association. The
        // creation notification still describes the originally created window.
        let window = created_window
            .upgrade()
            .expect("spawned window remains live");
        events_fire_window(c"window-created", window);
        events_fire_winlink(c"window-linked", sc.winlink_handle());
    }
    session_group_synchronize_from(&session);
    sc.winlink_handle()
}
