//! Owned, lazy builtin values. Cross-model lookups never retain a Client borrow.
use super::*;
use crate::format::FormatValue;
use crate::session::sessions;
use crate::shared::client::ClientRef;
use std::rc::Rc;

pub(super) fn value(
    owner: &ClientRef,
    key: &CStr,
    context: &mut format_tree,
) -> Option<FormatValue> {
    if !context.c.ptr_eq(&Rc::downgrade(owner)) {
        return None;
    }
    let text = match key.to_bytes() {
        b"client_activity" => {
            return Some(FormatValue::Time(crate::shared::time::unix_seconds(
                owner.activity_time(),
            )))
        }
        b"client_created" => {
            return Some(FormatValue::Time(crate::shared::time::unix_seconds(
                owner.creation_time(),
            )))
        }
        b"client_name" => owner.name()?,
        b"client_tty" => owner.tty_name()?,
        b"client_session" => owner.attached_session().upgrade()?.name(),
        b"client_last_session" => {
            let previous = owner.previous_session();
            sessions::resolve(&previous)?.name()
        }
        b"client_control_mode" => boolean(owner.is_control()),
        b"client_readonly" => boolean(owner.is_read_only()),
        b"client_utf8" => boolean(owner.flags() & CLIENT_UTF8 as u64 != 0),
        b"client_key_table" => {
            let table = owner.borrow().keytable.clone().expect("key table");
            let name = table.borrow().name.clone();
            name
        }
        b"client_prefix" => {
            let session = owner.attached_session().upgrade();
            let default = session
                .map(|session| options_get_string(&session.borrow_options_mut(), c"key-table"))
                .filter(|name| !name.as_bytes().is_empty())
                .unwrap_or_else(|| c"root".to_owned());
            let table = owner.borrow().keytable.clone().expect("key table");
            let differs = table.borrow().name != default;
            boolean(differs)
        }
        b"client_uid" => {
            let uid = owner.peer_uid();
            if uid == !0 {
                return None;
            }
            number(uid)
        }
        b"client_user" => {
            if let Some(name) = owner.borrow().user.clone() {
                return Some(FormatValue::String(name));
            }
            let uid = owner.peer_uid();
            if uid == !0 {
                return None;
            }
            // SAFETY: getpwuid's result is either null or a record in static
            // storage that stays valid until the next lookup on this thread;
            // the name is copied out before anything else can run.
            let name = unsafe {
                let passwd = libc::getpwuid(uid);
                if passwd.is_null() {
                    return None;
                }
                CStr::from_ptr((*passwd).pw_name).to_owned()
            };
            owner.borrow_mut().user = Some(name.clone());
            name
        }
        b"client_flags" => server_client_get_flags(&owner.borrow()),
        b"client_pid" => number(owner.borrow().pid),
        b"client_discarded" => number(owner.borrow().discarded),
        b"client_written" => number(owner.borrow().written),
        b"client_termfeatures" => tty_get_features(owner.borrow().term_features),
        b"client_termname" => owner.borrow().term_name.clone()?,
        b"client_termtype" => owner.borrow().term_type.clone().unwrap_or_default(),
        b"client_theme" => match owner.borrow().theme {
            1 => c"light".to_owned(),
            2 => c"dark".to_owned(),
            _ => return None,
        },
        b"client_width" => number(owner.terminal_size().0),
        b"client_height" | b"client_cell_width" | b"client_cell_height" | b"client_colours" => {
            let state = owner.borrow();
            let tty = &state.tty;
            if !tty.is_started() {
                return None;
            }
            match key.to_bytes() {
                b"client_height" => number(tty.size().1),
                b"client_cell_width" => number(tty.pixel_size().0),
                b"client_cell_height" => number(tty.pixel_size().1),
                _ => number(tty.colour_count()),
            }
        }
        _ => return None,
    };
    Some(FormatValue::String(text))
}

fn boolean(value: bool) -> CString {
    if value {
        c"1".to_owned()
    } else {
        c"0".to_owned()
    }
}

fn number(value: impl std::fmt::Display) -> CString {
    CString::new(value.to_string()).expect("formatted number contains no NUL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_own_bytes_and_preserve_unstarted_terminal_behavior() {
        unsafe {
            let owner = client::new();
            owner.borrow_mut().name = Some(CString::new(b"client-\xff".to_vec()).unwrap());
            crate::tty::tty_set_size(&mut owner.borrow_mut().tty, 91, 27, 0, 0);
            let mut context = format_create(None, None, 0, 0);
            context.c = Rc::downgrade(&owner);
            let Some(FormatValue::String(name)) = owner.format_value(c"client_name", &mut context)
            else {
                panic!("client name builtin");
            };
            let Some(FormatValue::String(width)) =
                owner.format_value(c"client_width", &mut context)
            else {
                panic!("width is available before terminal startup");
            };
            assert_eq!(width.as_c_str(), c"91");
            assert!(owner.format_value(c"client_height", &mut context).is_none());
            owner.borrow_mut().tty.set_started_for_test(true);
            let Some(FormatValue::String(height)) =
                owner.format_value(c"client_height", &mut context)
            else {
                panic!("started terminal height");
            };
            assert_eq!(height.as_c_str(), c"27");
            owner.borrow_mut().tty.set_started_for_test(false);
            owner.borrow_mut().name = Some(c"renamed".to_owned());
            assert_eq!(name.as_bytes(), b"client-\xff");
            context.c = std::rc::Weak::new();
            assert!(owner.format_value(c"client_name", &mut context).is_none());
            drop(context);
        }
    }
}
