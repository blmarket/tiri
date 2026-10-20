use crate::events::events_add_sink;
use crate::events_payload::{
    event_payload_get_client, event_payload_get_pane, event_payload_get_session,
    event_payload_get_string, event_payload_get_window, event_payload_print_owned,
};
use crate::format::bytes::nullable_cstr;
use crate::format::bytes::write_cstr;
use crate::format::{format_create, format_defaults, format_expand_cstring};
use crate::server_client::Client;
use crate::server_client::ClientRegistry;
use crate::session::Session;
use crate::shared::abi::u_int;
use crate::shared::client::ClientRef;
use crate::shared::events::{event_payload, events_callback};
use crate::shared::format::FORMAT_NONE;
use crate::window::{winlink_find_by_window_id, Window, WindowPane};
use std::{ffi::CStr, rc::Rc};

#[derive(Copy, Clone)]
pub struct C2RustUnnamed_35 {
    pub name: &'static CStr,
    pub cb: unsafe fn(&CStr, &mut event_payload),
}

unsafe fn recipients(mut visit: impl FnMut(&ClientRef)) {
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if client.receives_notifications() {
            visit(&client);
        }
        cursor = ClientRegistry::next(&client);
    }
}

unsafe fn control_pane_mode_changed_cb(_name: &CStr, payload: &mut event_payload) {
    if let Some(pane) = event_payload_get_pane(payload) {
        recipients(|client| client.notify(|out| write!(out, "%pane-mode-changed %{}", pane.id())));
    } else if let Some(value) = event_payload_print_owned(payload) {
        recipients(|client| {
            client.notify(|out| {
                out.write_all(b"%pane-mode-changed ")?;
                write_cstr(out, nullable_cstr(value.as_ptr().cast()))
            })
        });
    }
}

unsafe fn control_window_layout_changed_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(window) = event_payload_get_window(payload) else {
        return;
    };
    if !window.next_winlink(None).is_alive() || window.pane_snapshot().is_empty() {
        return;
    }
    recipients(|client| {
        let Some(session) = client.attached_session().upgrade() else {
            return;
        };
        let id = window.id();
        let link = session.with_winlinks(|links| winlink_find_by_window_id(links, id));
        if !link.is_alive() {
            return;
        }
        let mut format = format_create(Some(client), None, FORMAT_NONE, 0);
        format_defaults(&mut *format, Some(client), Some(&session), link, None);
        let message = format_expand_cstring(
            &mut *format,
            c"%layout-change #{window_id} #{window_layout} #{window_visible_layout} #{window_raw_flags}",
        );
        drop(format);
        client.notify(|out| out.write_all(message.as_bytes()));
    });
}

unsafe fn control_window_pane_changed_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(window) = event_payload_get_window(payload) else {
        return;
    };
    if window.active_pane().upgrade().is_none() {
        return;
    }
    recipients(|client| {
        client.notify(|out| {
            write!(
                out,
                "%window-pane-changed @{} %{}",
                window.id(),
                window.active_pane().upgrade().expect("active pane").id()
            )
        })
    });
}

unsafe fn control_window_membership(payload: &event_payload, action: &str, renamed: bool) {
    let Some(window) = event_payload_get_window(payload) else {
        return;
    };
    recipients(|client| {
        let Some(session) = client.attached_session().upgrade() else {
            return;
        };
        let id = window.id();
        let linked = session.with_winlinks(|links| winlink_find_by_window_id(links, id).is_alive());
        client.notify(|out| {
            write!(
                out,
                "%{}window-{} @{}",
                if linked { "" } else { "unlinked-" },
                action,
                window.id()
            )?;
            if renamed {
                out.write_all(b" ")?;
                out.write_all(window.name().as_bytes())?;
            }
            Ok(())
        });
    });
}

unsafe fn control_window_unlinked_cb(_name: &CStr, payload: &mut event_payload) {
    control_window_membership(payload, "close", false);
}
unsafe fn control_window_linked_cb(_name: &CStr, payload: &mut event_payload) {
    control_window_membership(payload, "add", false);
}
unsafe fn control_window_renamed_cb(_name: &CStr, payload: &mut event_payload) {
    control_window_membership(payload, "renamed", true);
}

unsafe fn control_client_session_changed_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(changed) = event_payload_get_client(payload) else {
        return;
    };
    let Some(session) = changed.attached_session().upgrade() else {
        return;
    };
    recipients(|client| {
        if client.attached_session().upgrade().is_none() {
            return;
        }
        client.notify(|out| {
            if Rc::ptr_eq(changed, client) {
                write!(out, "%session-changed $")?;
            } else {
                out.write_all(b"%client-session-changed ")?;
                write_cstr(out, changed.name().as_deref())?;
                out.write_all(b" $")?;
            }
            write!(out, "{} ", session.id())?;
            out.write_all(session.name().as_bytes())
        });
    });
}

unsafe fn control_client_detached_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(changed) = event_payload_get_client(payload) else {
        return;
    };
    recipients(|client| {
        client.notify(|out| {
            out.write_all(b"%client-detached ")?;
            write_cstr(out, changed.name().as_deref())
        })
    });
}

unsafe fn control_session_renamed_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(session) = event_payload_get_session(payload) else {
        return;
    };
    recipients(|client| {
        client.notify(|out| {
            write!(out, "%session-renamed ${} ", session.id())?;
            out.write_all(session.name().as_bytes())
        })
    });
}
unsafe fn control_session_created_cb(_name: &CStr, _payload: &mut event_payload) {
    recipients(|client| client.notify(|out| out.write_all(b"%sessions-changed")));
}
unsafe fn control_session_closed_cb(_name: &CStr, _payload: &mut event_payload) {
    recipients(|client| client.notify(|out| out.write_all(b"%sessions-changed")));
}
unsafe fn control_session_window_changed_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(session) = event_payload_get_session(payload) else {
        return;
    };
    if !session.current_winlink().is_alive() {
        return;
    }
    recipients(|client| {
        client.notify(|out| {
            let link = session.current_winlink();
            let window = link.get_unchecked().window_handle().expect("linked window");
            write!(
                out,
                "%session-window-changed ${} @{}",
                session.id(),
                window.id()
            )
        })
    });
}
unsafe fn control_paste_buffer_changed_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(name) = event_payload_get_string(payload) else {
        return;
    };
    recipients(|client| {
        client.notify(|out| {
            out.write_all(b"%paste-buffer-changed ")?;
            out.write_all(name.to_bytes())
        })
    });
}
unsafe fn control_paste_buffer_deleted_cb(_name: &CStr, payload: &mut event_payload) {
    let Some(name) = event_payload_get_string(payload) else {
        return;
    };
    recipients(|client| {
        client.notify(|out| {
            out.write_all(b"%paste-buffer-deleted ")?;
            out.write_all(name.to_bytes())
        })
    });
}
pub unsafe fn control_build_events() {
    static events: [C2RustUnnamed_35; 14] = {
        [
            C2RustUnnamed_35 {
                name: c"pane-mode-changed",
                cb: control_pane_mode_changed_cb,
            },
            C2RustUnnamed_35 {
                name: c"window-layout-changed",
                cb: control_window_layout_changed_cb,
            },
            C2RustUnnamed_35 {
                name: c"window-pane-changed",
                cb: control_window_pane_changed_cb,
            },
            C2RustUnnamed_35 {
                name: c"window-unlinked",
                cb: control_window_unlinked_cb,
            },
            C2RustUnnamed_35 {
                name: c"window-linked",
                cb: control_window_linked_cb,
            },
            C2RustUnnamed_35 {
                name: c"window-renamed",
                cb: control_window_renamed_cb,
            },
            C2RustUnnamed_35 {
                name: c"client-session-changed",
                cb: control_client_session_changed_cb,
            },
            C2RustUnnamed_35 {
                name: c"client-detached",
                cb: control_client_detached_cb,
            },
            C2RustUnnamed_35 {
                name: c"session-renamed",
                cb: control_session_renamed_cb,
            },
            C2RustUnnamed_35 {
                name: c"session-created",
                cb: control_session_created_cb,
            },
            C2RustUnnamed_35 {
                name: c"session-closed",
                cb: control_session_closed_cb,
            },
            C2RustUnnamed_35 {
                name: c"session-window-changed",
                cb: control_session_window_changed_cb,
            },
            C2RustUnnamed_35 {
                name: c"paste-buffer-changed",
                cb: control_paste_buffer_changed_cb,
            },
            C2RustUnnamed_35 {
                name: c"paste-buffer-deleted",
                cb: control_paste_buffer_deleted_cb,
            },
        ]
    };
    let mut i: u_int = 0;
    i = 0 as u_int;
    while (i as usize)
        < (::core::mem::size_of::<[C2RustUnnamed_35; 14]>() as usize)
            .wrapping_div(::core::mem::size_of::<C2RustUnnamed_35>() as usize)
    {
        let callback = events[i as usize].cb;
        events_add_sink(
            events[i as usize].name,
            events_callback(move |name, payload| unsafe { callback(name, payload) }),
        );
        i = i.wrapping_add(1);
    }
}
