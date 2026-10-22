//! Session-owned lazy formatting. Returned values own their bytes or timestamp.
use super::*;
use crate::format::bytes::format_bytes;
use crate::format::FORMAT_TYPE_SESSION;
use crate::format::{format_tree, FormatValue};
use crate::server::server_check_marked;
use crate::server_client::Client as _;
use crate::server_client::ClientRegistry;
use crate::shared::client::ClientRef;
use crate::shared::session::SessionRef;
use crate::shared::window::{WINLINK_ACTIVITY, WINLINK_BELL, WINLINK_SILENCE};
use crate::window::winlink_count;
use std::io::Write as _;

pub(super) fn format_value(
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

/// Whether `client` is attached to `owner`.
fn client_attached_to(client: &ClientRef, owner: &SessionRef) -> bool {
    client
        .attached_session()
        .upgrade()
        .is_some_and(|attached| Rc::ptr_eq(&attached, owner))
}

fn format_cb_session_attached_list(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let mut names = Vec::<u8>::new();
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        if client_attached_to(&client, &owner) {
            if !names.is_empty() {
                names.push(b',');
            }
            names.extend_from_slice(client.name().expect("string is present").as_bytes());
        }
        cursor = ClientRegistry::next(&client);
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

fn format_cb_session_alert(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let mut alerts = Vec::<u8>::new();
    let mut alerted: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut wl = winlinks_minmax(&owner.borrow().windows, RB_NEGINF);
    while wl.is_alive() {
        if !(wl.try_borrow().expect("live winlink").flags & WINLINK_ALERTFLAGS
            == 0 as ::core::ffi::c_int)
        {
            if !alerted & wl.try_borrow().expect("live winlink").flags & WINLINK_ACTIVITY != 0 {
                alerts.push(b'#');
                alerted |= WINLINK_ACTIVITY;
            }
            if !alerted & wl.try_borrow().expect("live winlink").flags & WINLINK_BELL != 0 {
                alerts.push(b'!');
                alerted |= WINLINK_BELL;
            }
            if !alerted & wl.try_borrow().expect("live winlink").flags & WINLINK_SILENCE != 0 {
                alerts.push(b'~');
                alerted |= WINLINK_SILENCE;
            }
        }
        wl = winlink_next(&wl);
    }
    Some(CString::new(alerts).expect("callback bytes contain no NUL"))
}

fn format_cb_session_alerts(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let mut alerts = Vec::<u8>::new();
    let mut wl = winlinks_minmax(&owner.borrow().windows, RB_NEGINF);
    while wl.is_alive() {
        if !(wl.try_borrow().expect("live winlink").flags & WINLINK_ALERTFLAGS
            == 0 as ::core::ffi::c_int)
        {
            if !alerts.is_empty() {
                alerts.push(b',');
            }
            write!(
                alerts,
                "{}",
                (wl.try_borrow().expect("live winlink").idx) as u32
            )
            .expect("write to Vec");
            if wl.try_borrow().expect("live winlink").flags & WINLINK_ACTIVITY != 0 {
                alerts.push(b'#');
            }
            if wl.try_borrow().expect("live winlink").flags & WINLINK_BELL != 0 {
                alerts.push(b'!');
            }
            if wl.try_borrow().expect("live winlink").flags & WINLINK_SILENCE != 0 {
                alerts.push(b'~');
            }
        }
        wl = winlink_next(&wl);
    }
    Some(CString::new(alerts).expect("callback bytes contain no NUL"))
}

fn format_cb_session_stack(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let s = owner.borrow();
    let mut result = format_bytes(format_args!(
        "{}",
        (s.current_winlink().try_borrow().expect("live winlink").idx) as u32
    ));
    let mut wl = crate::window::winlink_stack_first(&s.lastw);
    while wl.is_alive() {
        write!(
            result,
            ",{}",
            (wl.try_borrow().expect("live winlink").idx) as u32
        )
        .expect("write to Vec");
        wl = crate::window::winlink_stack_next(&s.lastw, wl.clone());
    }
    Some(CString::new(result).expect("callback bytes contain no NUL"))
}

fn format_cb_session_group_list(ft: &mut format_tree) -> Option<CString> {
    ft.s.upgrade()?;
    let sg = session_groups::for_session(&ft.s);
    if !sg.is_alive() {
        return None;
    }
    let mut names = Vec::<u8>::new();
    for member in crate::session::session_group_members(&sg) {
        if !names.is_empty() {
            names.push(b',');
        }
        names.extend_from_slice(member.borrow().name.as_bytes());
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

fn format_cb_session_group_attached_list(ft: &mut format_tree) -> Option<CString> {
    ft.s.upgrade()?;
    let sg = session_groups::for_session(&ft.s);
    if !sg.is_alive() {
        return None;
    }
    let mut names = Vec::<u8>::new();
    let mut cursor = ClientRegistry::first();
    while let Some(client) = cursor {
        for member in crate::session::session_group_members(&sg) {
            if client_attached_to(&client, &member) {
                if !names.is_empty() {
                    names.push(b',');
                }
                names.extend_from_slice(client.name().expect("string is present").as_bytes());
            }
        }
        cursor = ClientRegistry::next(&client);
    }
    if names.is_empty() {
        return None;
    }
    Some(CString::new(names).expect("callback bytes contain no NUL"))
}

fn format_cb_session_active(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let client = ft.c.upgrade()?;
    if client_attached_to(&client, &owner) {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

/// Whether any winlink in the format's session carries `flag`, as tmux 3.8's
/// session flag callbacks walk the whole session rather than its current
/// winlink.
fn format_session_winlink_flag(ft: &mut format_tree, flag: i32) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let mut wl = winlinks_minmax(&owner.borrow().windows, RB_NEGINF);
    while wl.is_alive() {
        if wl.try_borrow().expect("live winlink").flags & flag != 0 {
            return Some(c"1".to_owned());
        }
        wl = winlink_next(&wl);
    }
    Some(c"0".to_owned())
}

fn format_cb_session_activity_flag(ft: &mut format_tree) -> Option<CString> {
    format_session_winlink_flag(ft, WINLINK_ACTIVITY)
}

fn format_cb_session_bell_flag(ft: &mut format_tree) -> Option<CString> {
    format_session_winlink_flag(ft, WINLINK_BELL)
}

fn format_cb_session_silence_flag(ft: &mut format_tree) -> Option<CString> {
    format_session_winlink_flag(ft, WINLINK_SILENCE)
}

fn format_cb_session_attached(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    Some(
        CString::new(format!("{}", (owner.borrow().attached) as u32))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_session_format(ft: &mut format_tree) -> Option<CString> {
    if ft.type_0 as ::core::ffi::c_uint
        == FORMAT_TYPE_SESSION as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

/// The format's session group, when the session is live and grouped.
fn format_session_group(ft: &format_tree) -> Option<crate::shared::session::SessionGroupWeak> {
    ft.s.upgrade()?;
    let sg = session_groups::for_session(&ft.s);
    sg.is_alive().then_some(sg)
}

fn format_cb_session_group(ft: &mut format_tree) -> Option<CString> {
    let sg = format_session_group(ft)?;
    Some(crate::session::session_group_name(&sg))
}

fn format_cb_session_group_attached(ft: &mut format_tree) -> Option<CString> {
    let sg = format_session_group(ft)?;
    Some(
        CString::new(format!("{}", (session_group_attached_count(&sg)) as u32))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_session_group_many_attached(ft: &mut format_tree) -> Option<CString> {
    let sg = format_session_group(ft)?;
    if session_group_attached_count(&sg) > 1 as u_int {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_session_group_size(ft: &mut format_tree) -> Option<CString> {
    let sg = format_session_group(ft)?;
    Some(
        CString::new(format!("{}", (session_group_count(&sg)) as u32))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_session_grouped(ft: &mut format_tree) -> Option<CString> {
    ft.s.upgrade()?;
    if session_groups::for_session(&ft.s).is_alive() {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_session_id(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    Some(
        CString::new(format!("${}", (owner.borrow().id) as u32))
            .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_session_many_attached(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    if owner.borrow().attached > 1 as u_int {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_session_marked(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let marked = unsafe {
        server_check_marked() != 0
            && marked_pane
                .with_borrow(|marked| marked.session_handle())
                .is_some_and(|marked| Rc::ptr_eq(&marked, &owner))
    };
    if marked {
        return Some(c"1".to_owned());
    }
    Some(c"0".to_owned())
}

fn format_cb_session_name(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let name = owner.borrow().name.clone();
    Some(name)
}

fn format_cb_session_path(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let path = owner
        .borrow()
        .cwd
        .as_deref()
        .expect("string is present")
        .to_owned();
    Some(path)
}

fn format_cb_session_windows(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let count = winlink_count(&owner.borrow().windows);
    Some(CString::new(format!("{}", count as u32)).expect("formatted numbers contain no NUL"))
}

fn format_cb_session_activity(ft: &mut format_tree) -> Option<time_t> {
    let owner = ft.s.upgrade()?;
    let time = owner.borrow().activity_time;
    Some(crate::shared::time::unix_seconds(time))
}

fn format_cb_session_created(ft: &mut format_tree) -> Option<time_t> {
    let owner = ft.s.upgrade()?;
    let time = owner.borrow().creation_time;
    Some(crate::shared::time::unix_seconds(time))
}

fn format_cb_session_last_attached(ft: &mut format_tree) -> Option<time_t> {
    let owner = ft.s.upgrade()?;
    let time = owner.borrow().last_attached_time;
    Some(crate::shared::time::unix_seconds(time))
}

fn format_cb_active_window_index(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let wl = owner.borrow().current_winlink();
    Some(
        CString::new(format!(
            "{}",
            (wl.try_borrow().expect("live winlink").idx) as u32
        ))
        .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_last_window_index(ft: &mut format_tree) -> Option<CString> {
    let owner = ft.s.upgrade()?;
    let wl = winlinks_minmax(&owner.borrow().windows, RB_INF);
    Some(
        CString::new(format!(
            "{}",
            (wl.try_borrow().expect("live winlink").idx) as u32
        ))
        .expect("formatted numbers contain no NUL"),
    )
}

fn format_cb_window_stack_index(ft: &mut format_tree) -> Option<CString> {
    if !ft.winlink_handle().is_alive() {
        return None;
    }
    let owner = ft.s.upgrade()?;
    let s = owner.borrow();
    let mut idx: u_int = 0;
    let mut wl = crate::window::winlink_stack_first(&s.lastw);
    while wl.is_alive() {
        idx = idx.wrapping_add(1);
        if wl == ft.winlink_handle() {
            break;
        }
        wl = crate::window::winlink_stack_next(&s.lastw, wl.clone());
    }
    if !wl.is_alive() {
        return Some(c"0".to_owned());
    }
    Some(CString::new(format!("{}", idx as u32)).expect("formatted numbers contain no NUL"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::format_create;

    #[test]
    fn session_formats_observe_context_session_without_retaining_it() {
        unsafe {
            let owner = session::new();
            owner.borrow_mut().name = c"observed-session".to_owned();
            let observer = std::rc::Rc::downgrade(&owner);
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &mut *ft_owner;
            ft.s = Rc::downgrade(&owner);
            assert!(observer.ptr_eq(&ft.s));
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
            let link = crate::window::winlink_add(&owner.borrow().windows, 7);
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &mut *ft_owner;
            ft.wl = link.clone();
            assert_eq!(ft.winlink_handle(), link);
            assert_eq!(
                crate::format::format_expand_cstring(ft, c"#{window_index}").as_c_str(),
                c"7"
            );
            crate::window::winlink_remove(&owner.borrow().windows, link.clone());
            assert!(!ft.winlink_handle().is_alive());
            assert_eq!(
                crate::format::format_expand_cstring(ft, c"#{window_index}").as_c_str(),
                c""
            );
            drop(ft_owner);
        }
    }

    #[test]
    fn model_format_dispatch_checks_target_identity_and_preserves_typed_times() {
        use crate::session::Session;
        unsafe {
            let session = session::new();
            let other = session::new();
            session.borrow_mut().options = Some(crate::options::options_create(None));
            session.borrow_mut().creation_time = UNIX_EPOCH + Duration::from_secs(123);
            let mut context = format_tree::default();
            context.s = Rc::downgrade(&session);
            assert!(matches!(
                session.format_value(c"session_created", &mut context),
                Some(FormatValue::Time(123))
            ));
            assert_eq!(
                crate::format::format_expand_cstring(&mut context, c"#{session_created}")
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
            let options = session.borrow_mut().options.take().unwrap();
            drop(options);
        }
    }

    #[test]
    fn window_history_format_uses_link_session_without_retargeting_the_context() {
        unsafe {
            let owner = session::new();
            let other = session::new();
            other.borrow_mut().options = Some(crate::options::options_create(None));
            let mut link = winlink_add(&owner.borrow().windows, 7);
            link.try_borrow_mut().expect("live winlink").session = Rc::downgrade(&owner);
            winlink_stack_push(&mut owner.borrow_mut().lastw, link.clone());
            let mut context = format_tree::default();
            context.s = Rc::downgrade(&other);
            context.wl = link.clone();

            assert_eq!(
                crate::format::format_expand_cstring(&mut context, c"#{window_stack_index}",)
                    .as_c_str(),
                c"1"
            );
            assert!(context.s.ptr_eq(&Rc::downgrade(&other)));
            assert_eq!(context.wl, link);

            winlink_stack_remove(&mut owner.borrow_mut().lastw, link.clone());
            session_winlink_remove(&owner, link);
            let options = other.borrow_mut().options.take().unwrap();
            drop(options);
        }
    }
}
