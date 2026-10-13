//! Session-owned lazy formatting. Returned values own their bytes or timestamp.
use super::*;
use crate::src::ffi::libc::strlcat;
use crate::src::format::bytes::xformat;
use crate::src::format::FORMAT_TYPE_SESSION;
use crate::src::format::{format_tree, FormatValue};
use crate::src::server::{clients, server_check_marked};
use crate::src::server_client::Client as _;
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::{WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE};
use crate::src::window::winlink_count;

pub(super) unsafe fn format_value(
    owner: &SessionRef,
    key: &CStr,
    context: &mut format_tree,
) -> Option<FormatValue> {
    if !context.s.ptr_eq(&Rc::downgrade(owner)) {
        return None;
    }
    match key.to_bytes() {
        b"session_attached_list" => {
            format_cb_session_attached_list(context).map(FormatValue::String)
        }
        b"session_alert" => format_cb_session_alert(context).map(FormatValue::String),
        b"session_alerts" => format_cb_session_alerts(context).map(FormatValue::String),
        b"session_stack" => format_cb_session_stack(context).map(FormatValue::String),
        b"session_group_list" => format_cb_session_group_list(context).map(FormatValue::String),
        b"session_group_attached_list" => {
            format_cb_session_group_attached_list(context).map(FormatValue::String)
        }
        b"session_active" => format_cb_session_active(context).map(FormatValue::String),
        b"session_activity_flag" => {
            format_cb_session_activity_flag(context).map(FormatValue::String)
        }
        b"session_bell_flag" => format_cb_session_bell_flag(context).map(FormatValue::String),
        b"session_silence_flag" => format_cb_session_silence_flag(context).map(FormatValue::String),
        b"session_attached" => format_cb_session_attached(context).map(FormatValue::String),
        b"session_format" => format_cb_session_format(context).map(FormatValue::String),
        b"session_group" => format_cb_session_group(context).map(FormatValue::String),
        b"session_group_attached" => {
            format_cb_session_group_attached(context).map(FormatValue::String)
        }
        b"session_group_many_attached" => {
            format_cb_session_group_many_attached(context).map(FormatValue::String)
        }
        b"session_group_size" => format_cb_session_group_size(context).map(FormatValue::String),
        b"session_grouped" => format_cb_session_grouped(context).map(FormatValue::String),
        b"session_id" => format_cb_session_id(context).map(FormatValue::String),
        b"session_many_attached" => {
            format_cb_session_many_attached(context).map(FormatValue::String)
        }
        b"session_marked" => format_cb_session_marked(context).map(FormatValue::String),
        b"session_name" => format_cb_session_name(context).map(FormatValue::String),
        b"session_path" => format_cb_session_path(context).map(FormatValue::String),
        b"session_windows" => format_cb_session_windows(context).map(FormatValue::String),
        b"session_activity" => format_cb_session_activity(context).map(FormatValue::Time),
        b"session_created" => format_cb_session_created(context).map(FormatValue::Time),
        b"session_last_attached" => format_cb_session_last_attached(context).map(FormatValue::Time),
        b"active_window_index" => format_cb_active_window_index(context).map(FormatValue::String),
        b"last_window_index" => format_cb_last_window_index(context).map(FormatValue::String),
        b"window_stack_index" => format_cb_window_stack_index(context).map(FormatValue::String),
        _ => None,
    }
}

unsafe fn format_cb_session_attached_list(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut s: *mut session = format_session;
    let mut loop_0: Option<ClientRef> = None;
    if s.is_null() {
        return None;
    }
    let mut names = Vec::<u8>::new();
    let mut registry_loop_0_owner = clients.first();
    loop_0 = registry_loop_0_owner.clone();
    while !loop_0.is_none() {
        if loop_0
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .as_ref()
            .map_or(std::ptr::null_mut(), |owner| owner.get())
            == s
        {
            if !names.is_empty() {
                names.push(b',');
            }
            names.extend_from_slice(
                (loop_0.as_ref().expect("live client").name())
                    .as_deref()
                    .expect("string is present")
                    .to_bytes(),
            );
        }
        registry_loop_0_owner = clients.next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

unsafe fn format_cb_session_alert(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut s: *mut session = format_session;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut alerts: [::core::ffi::c_char; 1024] = [0; 1024];
    let mut alerted: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if s.is_null() {
        return None;
    }
    *(&raw mut alerts as *mut ::core::ffi::c_char) = '\0' as i32 as ::core::ffi::c_char;
    wl = winlinks_minmax(&(*s).windows, RB_NEGINF);
    while wl.is_alive() {
        if !(wl.get_unchecked().flags & WINLINK_ALERTFLAGS == 0 as ::core::ffi::c_int) {
            if !alerted & wl.get_unchecked().flags & WINLINK_ACTIVITY != 0 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c"#".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
                alerted |= WINLINK_ACTIVITY;
            }
            if !alerted & wl.get_unchecked().flags & WINLINK_BELL != 0 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c"!".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
                alerted |= WINLINK_BELL;
            }
            if !alerted & wl.get_unchecked().flags & WINLINK_SILENCE != 0 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c"~".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
                alerted |= WINLINK_SILENCE;
            }
        }
        wl = winlinks_next(wl.get_unchecked());
    }
    Some(CStr::from_ptr(&raw mut alerts as *mut ::core::ffi::c_char).to_owned())
}

unsafe fn format_cb_session_alerts(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut s: *mut session = format_session;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut alerts: [::core::ffi::c_char; 1024] = [0; 1024];
    let mut tmp: [u8; 16] = [0; 16];
    if s.is_null() {
        return None;
    }
    *(&raw mut alerts as *mut ::core::ffi::c_char) = '\0' as i32 as ::core::ffi::c_char;
    wl = winlinks_minmax(&(*s).windows, RB_NEGINF);
    while wl.is_alive() {
        if !(wl.get_unchecked().flags & WINLINK_ALERTFLAGS == 0 as ::core::ffi::c_int) {
            xformat(
                &mut tmp,
                format_args!("{}", (wl.get_unchecked().idx) as u32),
            );
            if *(&raw mut alerts as *mut ::core::ffi::c_char) as ::core::ffi::c_int != '\0' as i32 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c",".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
            }
            strlcat(
                &raw mut alerts as *mut ::core::ffi::c_char,
                &raw mut tmp as *mut ::core::ffi::c_char,
                ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
            );
            if wl.get_unchecked().flags & WINLINK_ACTIVITY != 0 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c"#".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
            }
            if wl.get_unchecked().flags & WINLINK_BELL != 0 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c"!".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
            }
            if wl.get_unchecked().flags & WINLINK_SILENCE != 0 {
                strlcat(
                    &raw mut alerts as *mut ::core::ffi::c_char,
                    c"~".as_ptr(),
                    ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
                );
            }
        }
        wl = winlinks_next(wl.get_unchecked());
    }
    Some(CStr::from_ptr(&raw mut alerts as *mut ::core::ffi::c_char).to_owned())
}

unsafe fn format_cb_session_stack(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut s: *mut session = format_session;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut result: [u8; 1024] = [0; 1024];
    let mut tmp: [u8; 16] = [0; 16];
    if s.is_null() {
        return None;
    }
    xformat(
        &mut result,
        format_args!("{}", (((*s).current_winlink()).get_unchecked().idx) as u32),
    );
    wl = crate::src::window::winlink_stack_first(&(*s).lastw);
    while wl.is_alive() {
        xformat(
            &mut tmp,
            format_args!("{}", (wl.get_unchecked().idx) as u32),
        );
        if *(&raw mut result as *mut ::core::ffi::c_char) as ::core::ffi::c_int != '\0' as i32 {
            strlcat(
                &raw mut result as *mut ::core::ffi::c_char,
                c",".as_ptr(),
                ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
            );
        }
        strlcat(
            &raw mut result as *mut ::core::ffi::c_char,
            &raw mut tmp as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 1024]>() as size_t,
        );
        wl = crate::src::window::winlink_stack_next(&(*s).lastw, wl.clone());
    }
    Some(
        CStr::from_bytes_until_nul(&result)
            .expect("stack list is terminated")
            .to_owned(),
    )
}

unsafe fn format_cb_session_group_list(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut s: *mut session = format_session;
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    if s.is_null() {
        return None;
    }
    sg = session_groups::for_session(&(*ft).s);
    if !sg.is_alive() {
        return None;
    }
    let mut names = Vec::<u8>::new();
    for loop_0 in crate::src::session::session_group_members(&sg) {
        if !names.is_empty() {
            names.push(b',');
        }
        names.extend_from_slice((*loop_0.get()).name.as_bytes());
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

unsafe fn format_cb_session_group_attached_list(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut s: *mut session = format_session;
    let mut client_session: *mut session = ::core::ptr::null_mut::<session>();
    let _session_loop: *mut session = ::core::ptr::null_mut::<session>();
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    let mut loop_0: Option<ClientRef> = None;
    if s.is_null() {
        return None;
    }
    sg = session_groups::for_session(&(*ft).s);
    if !sg.is_alive() {
        return None;
    }
    let mut names = Vec::<u8>::new();
    let mut registry_loop_0_owner = clients.first();
    loop_0 = registry_loop_0_owner.clone();
    while !loop_0.is_none() {
        client_session = loop_0
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .as_ref()
            .map_or(std::ptr::null_mut(), |owner| owner.get());
        if !client_session.is_null() {
            for session_loop in crate::src::session::session_group_members(&sg) {
                if session_loop.get() == client_session {
                    if !names.is_empty() {
                        names.push(b',');
                    }
                    names.extend_from_slice(
                        (loop_0.as_ref().expect("live client").name())
                            .as_deref()
                            .expect("string is present")
                            .to_bytes(),
                    );
                }
            }
        }
        registry_loop_0_owner = clients.next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

unsafe fn format_cb_session_active(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let format_client_owner = (*ft).c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    if format_session.is_null() || format_client.is_none() {
        return None;
    }
    if format_client
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade()
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get())
        == format_session
    {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

/// Whether any winlink in the format's session carries `flag`, as tmux 3.8's
/// session flag callbacks walk the whole session rather than its current
/// winlink.
unsafe fn format_session_winlink_flag(ft: *mut format_tree, flag: i32) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if format_session.is_null() {
        return None;
    }
    let mut wl = winlinks_minmax(&(*format_session).windows, RB_NEGINF);
    while wl.is_alive() {
        if wl.get_unchecked().flags & flag != 0 {
            return Some(c"1".to_owned());
        }
        wl = winlinks_next(wl.get_unchecked());
    }
    Some(c"0".to_owned())
}

unsafe fn format_cb_session_activity_flag(ft: *mut format_tree) -> Option<CString> {
    format_session_winlink_flag(ft, WINLINK_ACTIVITY)
}

unsafe fn format_cb_session_bell_flag(ft: *mut format_tree) -> Option<CString> {
    format_session_winlink_flag(ft, WINLINK_BELL)
}

unsafe fn format_cb_session_silence_flag(ft: *mut format_tree) -> Option<CString> {
    format_session_winlink_flag(ft, WINLINK_SILENCE)
}

unsafe fn format_cb_session_attached(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(
            CString::new(format!("{}", ((*format_session).attached) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_session_format(mut ft: *mut format_tree) -> Option<CString> {
    if (*ft).type_0 as ::core::ffi::c_uint
        == FORMAT_TYPE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

unsafe fn format_cb_session_group(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    if !format_session.is_null() && {
        sg = session_groups::for_session(&(*ft).s);
        sg.is_alive()
    } {
        return Some(crate::src::session::session_group_name(&sg));
    }
    None
}

unsafe fn format_cb_session_group_attached(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    if !format_session.is_null() && {
        sg = session_groups::for_session(&(*ft).s);
        sg.is_alive()
    } {
        return Some(
            CString::new(format!("{}", (session_group_attached_count(&sg)) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_session_group_many_attached(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    if !format_session.is_null() && {
        sg = session_groups::for_session(&(*ft).s);
        sg.is_alive()
    } {
        if session_group_attached_count(&sg) > 1 as u_int {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_session_group_size(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    if !format_session.is_null() && {
        sg = session_groups::for_session(&(*ft).s);
        sg.is_alive()
    } {
        return Some(
            CString::new(format!("{}", (session_group_count(&sg)) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_session_grouped(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        if session_groups::for_session(&(*ft).s).is_alive() {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_session_id(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(
            CString::new(format!("${}", ((*format_session).id) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_session_many_attached(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        if (*format_session).attached > 1 as u_int {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_session_marked(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        if server_check_marked() != 0
            && marked_pane
                .session_handle()
                .as_ref()
                .map_or(std::ptr::null_mut(), |owner| owner.get())
                == format_session
        {
            return Some(c"1".to_owned());
        }
        return Some(c"0".to_owned());
    }
    None
}

unsafe fn format_cb_session_name(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some((*format_session).name.clone());
    }
    None
}

unsafe fn format_cb_session_path(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(
            ((*format_session).cwd)
                .as_deref()
                .expect("string is present")
                .to_owned(),
        );
    }
    None
}

unsafe fn format_cb_session_windows(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(
            CString::new(format!(
                "{}",
                (winlink_count(&(*format_session).windows)) as u32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_session_activity(mut ft: *mut format_tree) -> Option<time_t> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(crate::src::shared::time::unix_seconds(
            (*format_session).activity_time,
        ));
    }
    None
}

unsafe fn format_cb_session_created(mut ft: *mut format_tree) -> Option<time_t> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(crate::src::shared::time::unix_seconds(
            (*format_session).creation_time,
        ));
    }
    None
}

unsafe fn format_cb_session_last_attached(mut ft: *mut format_tree) -> Option<time_t> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(crate::src::shared::time::unix_seconds(
            (*format_session).last_attached_time,
        ));
    }
    None
}

unsafe fn format_cb_active_window_index(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    if !format_session.is_null() {
        return Some(
            CString::new(format!(
                "{}",
                (((*format_session).current_winlink()).get_unchecked().idx) as u32
            ))
            .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_last_window_index(mut ft: *mut format_tree) -> Option<CString> {
    let format_session_owner = (*ft).s.upgrade();
    let format_session = format_session_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if !format_session.is_null() {
        wl = winlinks_minmax(&(*format_session).windows, RB_INF);
        return Some(
            CString::new(format!("{}", (wl.get_unchecked().idx) as u32))
                .expect("formatted numbers contain no NUL"),
        );
    }
    None
}

unsafe fn format_cb_window_stack_index(mut ft: *mut format_tree) -> Option<CString> {
    let mut s: *mut session = ::core::ptr::null_mut::<session>();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut idx: u_int = 0;
    let mut value = None;
    if !(*ft).winlink_handle().is_alive() {
        return None;
    }
    let session_owner = (*ft).s.upgrade()?;
    s = session_owner.get();
    idx = 0 as u_int;
    wl = crate::src::window::winlink_stack_first(&(*s).lastw);
    while wl.is_alive() {
        idx = idx.wrapping_add(1);
        if wl == (*ft).winlink_handle() {
            break;
        }
        wl = crate::src::window::winlink_stack_next(&(*s).lastw, wl.clone());
    }
    if !wl.is_alive() {
        return Some(c"0".to_owned());
    }
    value =
        Some(CString::new(format!("{}", (idx) as u32)).expect("formatted numbers contain no NUL"));
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::format::format_create;

    #[test]
    fn session_formats_observe_context_session_without_retaining_it() {
        unsafe {
            let owner = session::new();
            (*owner.get()).name = c"observed-session".to_owned();
            let observer = std::rc::Rc::downgrade(&owner);
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &raw mut *ft_owner;
            (*ft).s = Rc::downgrade(&owner);
            assert!(observer.ptr_eq(&(*ft).s));
            assert_eq!(
                format_cb_session_name(ft).unwrap().as_c_str(),
                c"observed-session"
            );
            drop(owner);
            assert!(observer.upgrade().is_none());
            assert!(format_cb_session_name(ft).is_none());
            assert!(format_cb_session_id(ft).is_none());
            drop(ft_owner);
        }
    }

    #[test]
    fn winlink_formats_expire_with_the_session_index_entry() {
        unsafe {
            let owner = session::new();
            let session = owner.get();
            let link = crate::src::window::winlink_add(&raw mut (*session).windows, 7);
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &raw mut *ft_owner;
            (*ft).wl = link.clone();
            assert_eq!((*ft).winlink_handle(), link);
            assert_eq!(
                crate::src::format::format_expand_cstring(ft, c"#{window_index}".as_ptr())
                    .as_c_str(),
                c"7"
            );
            crate::src::window::winlink_remove(&raw mut (*session).windows, link.clone());
            assert!(!(*ft).winlink_handle().is_alive());
            assert_eq!(
                crate::src::format::format_expand_cstring(ft, c"#{window_index}".as_ptr())
                    .as_c_str(),
                c""
            );
            drop(ft_owner);
        }
    }

    #[test]
    fn model_format_dispatch_checks_target_identity_and_preserves_typed_times() {
        use crate::src::session::Session;
        unsafe {
            let session = session::new();
            let other = session::new();
            (*session.get()).options = Some(crate::src::options::options_create(None));
            (*session.get()).creation_time = UNIX_EPOCH + Duration::from_secs(123);
            let mut context = format_tree::default();
            context.s = Rc::downgrade(&session);
            assert!(matches!(
                session.format_value(c"session_created", &mut context),
                Some(FormatValue::Time(123))
            ));
            assert_eq!(
                crate::src::format::format_expand_cstring(
                    &mut context,
                    c"#{session_created}".as_ptr()
                )
                .as_c_str(),
                c"123"
            );
            assert!(other
                .format_value(c"session_created", &mut context)
                .is_none());
            assert!(session
                .format_value(c"client_created", &mut context)
                .is_none());
            assert!(context.s.ptr_eq(&Rc::downgrade(&session)));
            drop((*session.get()).options.take().unwrap());
        }
    }

    #[test]
    fn window_history_format_uses_link_session_without_retargeting_the_context() {
        unsafe {
            let owner = session::new();
            let other = session::new();
            (*other.get()).options = Some(crate::src::options::options_create(None));
            let mut link = winlink_add(&raw mut (*owner.get()).windows, 7);
            link.get_mut_unchecked().session = Rc::downgrade(&owner);
            winlink_stack_push(&raw mut (*owner.get()).lastw, link.clone());
            let mut context = format_tree::default();
            context.s = Rc::downgrade(&other);
            context.wl = link.clone();

            assert_eq!(
                crate::src::format::format_expand_cstring(
                    &mut context,
                    c"#{window_stack_index}".as_ptr(),
                )
                .as_c_str(),
                c"1"
            );
            assert!(context.s.ptr_eq(&Rc::downgrade(&other)));
            assert_eq!(context.wl, link);

            winlink_stack_remove(&raw mut (*owner.get()).lastw, link.clone());
            winlink_remove(&raw mut (*owner.get()).windows, link);
            drop((*other.get()).options.take().unwrap());
        }
    }
}
