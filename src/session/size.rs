//! Session-owned cache publication used by creation and the sizing pass.
use super::*;
use crate::server_client::Client;
use crate::server_client::ClientRegistry;

/// Publish the Session-owned state used by the subsequent client/window sizing
/// passes. A removed Session can still be retained by a closing client: preserve
/// the legacy accumulation for that Session without refreshing its cached layout.
/// Neither registry traversal nor numeric option lookup invokes callbacks.
pub(super) fn recalculate_size_state() {
    let mut cursor = sessions::first();
    while let Some(owner) = cursor {
        owner.borrow_mut().attached = 0;
        status_update_cache(&owner);
        cursor = sessions::after(owner.borrow().name.as_c_str());
    }
    let mut cursor = ClientRegistry::first();
    while let Some(owner) = cursor {
        if owner.counts_as_attached() {
            if let Some(session) = owner.attached_session().upgrade() {
                let mut state = session.borrow_mut();
                state.attached = state.attached.wrapping_add(1);
            }
        }
        cursor = ClientRegistry::next(&owner);
    }
}

pub(super) fn status_update_cache(owner: &SessionRef) {
    let statuslines = options_get_number(&owner.borrow_options_mut(), c"status") as u_int;
    let statusat = if statuslines == 0 as u_int {
        -(1 as ::core::ffi::c_int)
    } else if options_get_number(&owner.borrow_options_mut(), c"status-position")
        == 0 as ::core::ffi::c_longlong
    {
        0 as ::core::ffi::c_int
    } else {
        1 as ::core::ffi::c_int
    };
    let mut s = owner.borrow_mut();
    s.statuslines = statuslines;
    s.statusat = statusat;
}
