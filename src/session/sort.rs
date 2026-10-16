//! Session registry ordering; comparison keys stay inside the Session owner.
use super::*;
use crate::src::shared::session::SessionRef;
use crate::src::sort::{sort_by_criteria, sort_ordering};
use std::cmp::Ordering;

unsafe fn sort_session_cmp(sa: &session, sb: &session, sort_crit: &sort_criteria) -> Ordering {
    let mut result: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    match sort_crit.order as ::core::ffi::c_uint {
        2 => {
            result = sa.id.cmp(&sb.id) as ::core::ffi::c_int;
        }
        1 => {
            result = sa.creation_time.cmp(&sb.creation_time) as i32;
        }
        0 => {
            result = sb.activity_time.cmp(&sa.activity_time) as i32;
        }
        4 => {
            result = strcmp((sa.name).as_ptr().cast_mut(), (sb.name).as_ptr().cast_mut());
        }
        _ => {}
    }
    if result == 0 as ::core::ffi::c_int {
        result = strcmp((sa.name).as_ptr().cast_mut(), (sb.name).as_ptr().cast_mut());
    }
    sort_ordering(result, sort_crit.reversed)
}
pub(super) unsafe fn sort_get_sessions(sort_crit: &sort_criteria) -> Vec<SessionRef> {
    let mut sessions_sorted = Vec::new();
    let mut current = sessions::first();
    while let Some(owner) = current {
        current = sessions::after((*owner.get()).name.as_c_str());
        sessions_sorted.push(owner);
    }
    sort_by_criteria(&mut sessions_sorted, sort_crit, |a, b, criteria| unsafe {
        sort_session_cmp(&*a.get(), &*b.get(), criteria)
    });
    sessions_sorted
}
