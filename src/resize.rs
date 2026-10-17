use crate::src::log::{log_cstr, log_debug};
use crate::src::options::{options_get_number, options_get_string};
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use crate::src::session::Session as _;
use crate::src::shared::abi::*;
use crate::src::shared::client::{ClientRef, CLIENT_CONTROL, CLIENT_STATUSOFF};
use crate::src::shared::limits::UINT_MAX;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::WindowRef;
use crate::src::shared::window::{
    WINDOW_MAXIMUM, WINDOW_MINIMUM, WINDOW_SIZE_LARGEST, WINDOW_SIZE_LATEST, WINDOW_SIZE_MANUAL,
};
use crate::src::tmux::global_w_options;
use crate::src::tty::tty_update_window_offset;
use crate::src::window::windows;
use crate::src::window::Window as _;
use libc::sscanf;

pub unsafe fn resize_window(window: &WindowRef, sx: u_int, sy: u_int, xpixel: i32, ypixel: i32) {
    use crate::src::window::Window;
    window.resize(sx, sy, xpixel, ypixel);
}
unsafe fn clients_with_window(w_owner: &WindowRef) -> u_int {
    let mut loop_0: Option<ClientRef> = None;
    let mut n: u_int = 0 as u_int;
    let mut registry_loop_0_owner = ClientRegistry::first();
    loop_0 = registry_loop_0_owner.clone();
    while !loop_0.is_none() {
        if !(!registry_loop_0_owner
            .as_ref()
            .expect("current registry client")
            .participates_in_window_sizing()
            || (loop_0
                .as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .expect("live session")
                .contains_window(w_owner) as i32)
                == 0)
        {
            n = n.wrapping_add(1);
            if n > 1 as u_int {
                break;
            }
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    n
}
unsafe fn clients_calculate_size(
    mut type_0: ::core::ffi::c_int,
    c_owner: Option<&ClientRef>,
    w_owner: Option<&WindowRef>,
    mut skip_client: impl FnMut(&ClientRef) -> bool,
    mut sx: *mut u_int,
    mut sy: *mut u_int,
    mut xpixel: *mut u_int,
    mut ypixel: *mut u_int,
) -> ::core::ffi::c_int {
    let mut c: Option<ClientRef> = c_owner.cloned();
    let mut loop_0: Option<ClientRef> = None;
    let mut cx: u_int = 0;
    let mut cy: u_int = 0;
    let mut n: u_int = 0 as u_int;
    if type_0 == WINDOW_SIZE_LARGEST {
        *sx = 0 as u_int;
        *sy = 0 as u_int;
    } else if w_owner.is_some() && type_0 == WINDOW_SIZE_MANUAL {
        (*sx, *sy) = w_owner.expect("manual window").manual_size();
        log_debug(format_args!(
            "{}: manual size {}x{}",
            "clients_calculate_size",
            { *sx },
            { *sy }
        ));
    } else {
        *sx = UINT_MAX as u_int;
        *sy = UINT_MAX as u_int;
    }
    *ypixel = 0 as u_int;
    *xpixel = *ypixel;
    if let Some(w) = w_owner.filter(|_| type_0 == WINDOW_SIZE_LATEST) {
        n = clients_with_window(w);
    }
    if !(type_0 == WINDOW_SIZE_MANUAL) {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            if !crate::src::shared::rc::same(loop_0.as_ref(), c.as_ref())
                && !registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client")
                    .participates_in_window_sizing()
            {
                log_debug(format_args!(
                    "{}: ignoring {} (1)",
                    "clients_calculate_size",
                    log_cstr(
                        (loop_0.as_ref().expect("live client").name())
                            .as_deref()
                            .unwrap_or(c"(null)")
                    )
                ));
            } else if !crate::src::shared::rc::same(loop_0.as_ref(), c.as_ref())
                && skip_client(loop_0.as_ref().expect("live client"))
            {
                log_debug(format_args!(
                    "{}: skipping {} (1)",
                    "clients_calculate_size",
                    log_cstr(
                        (loop_0.as_ref().expect("live client").name())
                            .as_deref()
                            .unwrap_or(c"(null)")
                    )
                ));
            } else if type_0 == WINDOW_SIZE_LATEST
                && n > 1 as u_int
                && !w_owner
                    .expect("latest window")
                    .is_latest_client(loop_0.as_ref().expect("live client"))
            {
                log_debug(format_args!(
                    "{}: {} is not latest",
                    "clients_calculate_size",
                    log_cstr(
                        (loop_0.as_ref().expect("live client").name())
                            .as_deref()
                            .unwrap_or(c"(null)")
                    )
                ));
            } else {
                let (client_sx, client_sy, client_xpixel, client_ypixel) = registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client")
                    .window_size(w_owner);
                cx = client_sx;
                cy = client_sy;
                if type_0 == WINDOW_SIZE_LARGEST {
                    if cx > *sx {
                        *sx = cx;
                    }
                    if cy > *sy {
                        *sy = cy;
                    }
                } else {
                    if cx < *sx {
                        *sx = cx;
                    }
                    if cy < *sy {
                        *sy = cy;
                    }
                }
                if client_xpixel > *xpixel && client_ypixel > *ypixel {
                    *xpixel = client_xpixel;
                    *ypixel = client_ypixel;
                }
                log_debug(format_args!(
                    "{}: after {} ({}x{}), size is {}x{}",
                    "clients_calculate_size",
                    log_cstr(
                        (loop_0.as_ref().expect("live client").name())
                            .as_deref()
                            .unwrap_or(c"(null)")
                    ),
                    (cx) as u32,
                    (cy) as u32,
                    { *sx },
                    { *sy }
                ));
            }
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
        if *sx != UINT_MAX && *sy != UINT_MAX {
            log_debug(format_args!(
                "{}: calculated size {}x{}",
                "clients_calculate_size",
                { *sx },
                { *sy }
            ));
        } else {
            log_debug(format_args!(
                "{}: no calculated size",
                "clients_calculate_size"
            ));
        }
    }
    if let Some(w_owner_value) = w_owner {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            if !(!crate::src::shared::rc::same(loop_0.as_ref(), c.as_ref())
                && !registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client")
                    .participates_in_window_sizing())
                && !(!crate::src::shared::rc::same(loop_0.as_ref(), c.as_ref())
                    && skip_client(loop_0.as_ref().expect("live client")))
            {
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client")
                    .constrain_window_size(w_owner_value, &mut *sx, &mut *sy);
            }
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if *sx != UINT_MAX && *sy != UINT_MAX {
        log_debug(format_args!(
            "{}: calculated size {}x{}",
            "clients_calculate_size",
            { *sx },
            { *sy }
        ));
    } else {
        log_debug(format_args!(
            "{}: no calculated size",
            "clients_calculate_size"
        ));
    }
    if type_0 == WINDOW_SIZE_MANUAL {
        log_debug(format_args!("{}: type is manual", "clients_calculate_size"));
        return w_owner.is_some() as ::core::ffi::c_int;
    }
    if type_0 == WINDOW_SIZE_LARGEST {
        log_debug(format_args!(
            "{}: type is largest",
            "clients_calculate_size"
        ));
        return (*sx != 0 as u_int && *sy != 0 as u_int) as ::core::ffi::c_int;
    }
    if type_0 == WINDOW_SIZE_LATEST {
        log_debug(format_args!("{}: type is latest", "clients_calculate_size"));
    } else {
        log_debug(format_args!(
            "{}: type is smallest",
            "clients_calculate_size"
        ));
    }
    (*sx != UINT_MAX && *sy != UINT_MAX) as ::core::ffi::c_int
}
pub unsafe fn default_window_size(
    c_owner: Option<&ClientRef>,
    s_owner: &SessionRef,
    w_owner: Option<&WindowRef>,
    mut sx: *mut u_int,
    mut sy: *mut u_int,
    mut xpixel: *mut u_int,
    mut ypixel: *mut u_int,
    mut type_0: ::core::ffi::c_int,
) {
    let mut c: Option<ClientRef> = c_owner.cloned();
    let s = Some(s_owner.clone());
    if type_0 == -(1 as ::core::ffi::c_int) {
        type_0 = options_get_number(global_w_options(), c"window-size") as ::core::ffi::c_int;
    }
    if let Some(sizing_client) = c_owner
        .filter(|client| type_0 == WINDOW_SIZE_LATEST && client.participates_in_window_sizing())
    {
        let dimensions = sizing_client.window_size(None);
        (*sx, *sy, *xpixel, *ypixel) = dimensions;
        log_debug(format_args!(
            "{}: using {}x{} from {}",
            "default_window_size",
            { *sx },
            { *sy },
            log_cstr(sizing_client.name().as_deref().unwrap_or(c"(null)"))
        ));
    } else {
        if !c.is_none()
            && c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
        {
            c = None;
        }
        if clients_calculate_size(
            type_0,
            c.as_ref(),
            w_owner,
            |candidate| unsafe {
                (w_owner.is_some()
                    && (candidate
                        .attached_session()
                        .upgrade()
                        .expect("live session")
                        .contains_window(w_owner.expect("live window"))
                        as i32)
                        == 0)
                    || (w_owner.is_none()
                        && !crate::src::shared::rc::same(
                            candidate.attached_session().upgrade().as_ref(),
                            s.as_ref(),
                        ))
            },
            sx,
            sy,
            xpixel,
            ypixel,
        ) == 0
        {
            {
                let options = s_owner.borrow_options_mut();
                let value = options_get_string(options, c"default-size");
                if sscanf(value.as_ptr(), c"%ux%u".as_ptr(), sx, sy) != 2 {
                    *sx = 80;
                    *sy = 24;
                }
            }
            log_debug(format_args!(
                "{}: using {}x{} from default-size",
                "default_window_size",
                { *sx },
                { *sy }
            ));
        }
    }
    if *sx < WINDOW_MINIMUM as u_int {
        *sx = WINDOW_MINIMUM as u_int;
    }
    if *sx > WINDOW_MAXIMUM as u_int {
        *sx = WINDOW_MAXIMUM as u_int;
    }
    if *sy < WINDOW_MINIMUM as u_int {
        *sy = WINDOW_MINIMUM as u_int;
    }
    if *sy > WINDOW_MAXIMUM as u_int {
        *sy = WINDOW_MAXIMUM as u_int;
    }
    log_debug(format_args!(
        "{}: resulting size is {}x{}",
        "default_window_size",
        { *sx },
        { *sy }
    ));
}
pub unsafe fn recalculate_size(w_owner: &WindowRef, mut now: ::core::ffi::c_int) {
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut xpixel: u_int = 0 as u_int;
    let mut ypixel: u_int = 0 as u_int;
    let mut type_0: ::core::ffi::c_int = 0;
    let mut current: ::core::ffi::c_int = 0;
    let mut changed: ::core::ffi::c_int = 0;
    if (w_owner).active_pane().upgrade().is_none() {
        return;
    }
    log_debug(format_args!(
        "{}: @{} is {}x{}",
        "recalculate_size",
        { (w_owner).id() },
        { (w_owner).size().0 },
        { (w_owner).size().1 }
    ));
    (type_0, current) = {
        let options = w_owner.borrow_options_mut();
        (
            options_get_number(options, c"window-size") as i32,
            options_get_number(options, c"aggressive-resize") as i32,
        )
    };
    changed = clients_calculate_size(
        type_0,
        None,
        Some(w_owner),
        |candidate| unsafe {
            let Some(session) = candidate.attached_session().upgrade() else {
                return true;
            };
            if !session.current_winlink().is_alive() {
                return true;
            }
            if current != 0 {
                session
                    .current_winlink()
                    .get_unchecked()
                    .window_handle()
                    .is_none_or(|current| !WindowRef::ptr_eq(current, w_owner))
            } else {
                !session.contains_window(w_owner)
            }
        },
        &raw mut sx,
        &raw mut sy,
        &raw mut xpixel,
        &raw mut ypixel,
    );
    if let Some(pending) = w_owner.pending_resize() {
        if now == 0 && changed != 0 && pending == (sx, sy) {
            changed = 0 as ::core::ffi::c_int;
        }
    } else if now == 0 && changed != 0 && (w_owner).size().0 == sx && (w_owner).size().1 == sy {
        changed = 0 as ::core::ffi::c_int;
    }
    if changed == 0 {
        log_debug(format_args!(
            "{}: @{} no size change",
            "recalculate_size",
            { (w_owner).id() }
        ));
        tty_update_window_offset(w_owner);
        return;
    }
    log_debug(format_args!(
        "{}: @{} new size {}x{}",
        "recalculate_size",
        { (w_owner).id() },
        (sx) as u32,
        (sy) as u32
    ));
    if now != 0 || type_0 == WINDOW_SIZE_MANUAL {
        resize_window(
            w_owner,
            sx,
            sy,
            xpixel as ::core::ffi::c_int,
            ypixel as ::core::ffi::c_int,
        );
    } else {
        w_owner.defer_resize(sx, sy, xpixel, ypixel);
        tty_update_window_offset(w_owner);
    };
}
pub unsafe fn recalculate_sizes() {
    recalculate_sizes_now(0 as ::core::ffi::c_int);
}
pub unsafe fn recalculate_sizes_now(mut now: ::core::ffi::c_int) {
    let mut _s: Option<SessionRef> = None;
    let mut c: Option<ClientRef> = None;
    crate::src::shared::session::SessionRef::recalculate_attachment_status();
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        _s = c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade();
        if !(!registry_c_owner
            .as_ref()
            .expect("current registry client")
            .participates_in_window_sizing())
        {
            if c.as_ref().expect("live client").terminal_size().1
                <= registry_c_owner
                    .as_ref()
                    .expect("sizing client")
                    .attached_session()
                    .upgrade()
                    .expect("attached sizing client")
                    .status_layout()
                    .1
                || c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0
            {
                c.as_ref()
                    .expect("live client")
                    .update_flags(CLIENT_STATUSOFF as uint64_t, 0);
            } else {
                c.as_ref()
                    .expect("live client")
                    .update_flags(0, !(!CLIENT_STATUSOFF as uint64_t));
            }
        }
        registry_c_owner =
            ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
    let mut window_cursor = windows::first();
    while let Some(window_owner) = window_cursor.take() {
        recalculate_size(&window_owner, now);
        window_cursor = window_owner.next_window().upgrade();
        drop(window_owner);
    }
}
