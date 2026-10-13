//! Session-owned cache publication used by creation and the sizing pass.
use super::*;
use crate::src::server_client::Client;
use crate::src::server_client::ClientRegistry;

/// Publish the Session-owned state used by the subsequent client/window sizing
/// passes. A removed Session can still be retained by a closing client: preserve
/// the legacy accumulation for that Session without refreshing its cached layout.
/// Neither registry traversal nor numeric option lookup invokes callbacks.
pub(super) unsafe fn recalculate_size_state() {
    let mut cursor = sessions::first();
    while let Some(owner) = cursor {
        (*owner.get()).attached = 0;
        status_update_cache(&mut *owner.get());
        cursor = sessions::after((*owner.get()).name.as_bytes());
    }
    let mut cursor = ClientRegistry::first();
    while let Some(owner) = cursor {
        if owner.counts_as_attached() {
            if let Some(session) = owner.attached_session().upgrade() {
                let state = &mut *session.get();
                state.attached = state.attached.wrapping_add(1);
            }
        }
        cursor = ClientRegistry::next(&owner);
    }
}

pub(super) unsafe fn status_update_cache(s_value: &mut session) {
    let s: *mut session = s_value as *mut _;
    (*s).statuslines = options_get_number(
        options_owner_ptr(&mut (*s).options).map_or(std::ptr::null_mut(), |options| options),
        c"status",
    ) as u_int;
    if (*s).statuslines == 0 as u_int {
        (*s).statusat = -(1 as ::core::ffi::c_int);
    } else if options_get_number(
        options_owner_ptr(&mut (*s).options).map_or(std::ptr::null_mut(), |options| options),
        c"status-position",
    ) == 0 as ::core::ffi::c_longlong
    {
        (*s).statusat = 0 as ::core::ffi::c_int;
    } else {
        (*s).statusat = 1 as ::core::ffi::c_int;
    };
}
