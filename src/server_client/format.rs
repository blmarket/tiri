//! Owned, lazy builtin values. Cross-model lookups never retain a Client borrow.
use super::*;
use crate::src::format::FormatValue;
use crate::src::session::sessions;
use crate::src::shared::client::ClientRef;
use crate::src::shared::tty::{TERM_256COLOURS, TERM_RGBCOLOURS};
use crate::src::tty_term::{tty_term_number, tty_term_owner_ptr};
use std::rc::Rc;

pub(super) unsafe fn value(
    owner: &ClientRef,
    key: &CStr,
    context: &mut format_tree,
) -> Option<FormatValue> {
    if !context.c.ptr_eq(&Rc::downgrade(owner)) {
        return None;
    }
    let text = match key.to_bytes() {
        b"client_activity" => {
            return Some(FormatValue::Time(crate::src::shared::time::unix_seconds(
                owner.activity_time(),
            )))
        }
        b"client_created" => {
            return Some(FormatValue::Time(crate::src::shared::time::unix_seconds(
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
            let table = (*owner.get()).keytable.clone().expect("key table");
            let name = table.borrow().name.clone();
            name
        }
        b"client_prefix" => {
            let session = owner.attached_session().upgrade();
            let default = session
                .map(|session| {
                    session.with_options_mut(|options| options_get_string(options, c"key-table"))
                })
                .filter(|name| !name.as_bytes().is_empty())
                .unwrap_or_else(|| c"root".to_owned());
            let table = (*owner.get()).keytable.clone().expect("key table");
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
            if let Some(name) = (*owner.get()).user.clone() {
                return Some(FormatValue::String(name));
            }
            let uid = owner.peer_uid();
            if uid == !0 {
                return None;
            }
            let passwd = libc::getpwuid(uid);
            if passwd.is_null() {
                return None;
            }
            let name = CStr::from_ptr((*passwd).pw_name).to_owned();
            (*owner.get()).user = Some(name.clone());
            name
        }
        b"client_flags" => server_client_get_flags(&*owner.get()),
        b"client_pid" => number((*owner.get()).pid),
        b"client_discarded" => number((*owner.get()).discarded),
        b"client_written" => number((*owner.get()).written),
        b"client_termfeatures" => tty_get_features((*owner.get()).term_features),
        b"client_termname" => (*owner.get()).term_name.clone()?,
        b"client_termtype" => (*owner.get()).term_type.clone().unwrap_or_default(),
        b"client_theme" => match (*owner.get()).theme {
            1 => c"light".to_owned(),
            2 => c"dark".to_owned(),
            _ => return None,
        },
        b"client_width" => number(owner.terminal_size().0),
        b"client_height" | b"client_cell_width" | b"client_cell_height" | b"client_colours" => {
            let tty = &(*owner.get()).tty;
            if tty.flags & TTY_STARTED == 0 {
                return None;
            }
            match key.to_bytes() {
                b"client_height" => number(tty.sy),
                b"client_cell_width" => number(tty.xpixel),
                b"client_cell_height" => number(tty.ypixel),
                _ => {
                    let term = tty_term_owner_ptr(&tty.term).expect("terminal description");
                    let colours = if term.flags & TERM_RGBCOLOURS != 0 {
                        16777216
                    } else if term.flags & TERM_256COLOURS != 0 {
                        256
                    } else {
                        match tty_term_number(term, TTYC_COLORS) as u32 {
                            0..=7 => 2,
                            8..=15 => 8,
                            _ => 16,
                        }
                    };
                    number(colours)
                }
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
            (*owner.get()).name = Some(CString::new(b"client-\xff".to_vec()).unwrap());
            (*owner.get()).tty.sx = 91;
            (*owner.get()).tty.sy = 27;
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
            (*owner.get()).tty.flags |= TTY_STARTED;
            let Some(FormatValue::String(height)) =
                owner.format_value(c"client_height", &mut context)
            else {
                panic!("started terminal height");
            };
            assert_eq!(height.as_c_str(), c"27");
            (*owner.get()).tty.flags &= !TTY_STARTED;
            (*owner.get()).name = Some(c"renamed".to_owned());
            assert_eq!(name.as_bytes(), b"client-\xff");
            context.c = std::rc::Weak::new();
            assert!(owner.format_value(c"client_name", &mut context).is_none());
            drop(context);
        }
    }
}
