use crate::format::bytes::cstr_basename;
use crate::format::{
    format_create, format_defaults_pane, format_defaults_window, format_expand_cstring,
};
use crate::log::log_debug;
use crate::options::options_get_number;
use crate::server_fn::{server_redraw_window_borders, server_status_window};
use crate::shared::abi::*;
use crate::shared::format::FORMAT_WINDOW;
use crate::shared::window::WindowRef;
use crate::tmux::clean_name_cstring;
use crate::window::Window as _;
use crate::window::WindowPane as _;
use libc::{strchr, strcspn, strlen, strncmp};
use std::ffi::{CStr, CString};
use std::time::{Duration, Instant};

pub const NAME_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) fn name_time_left(previous: Option<Instant>, now: Instant) -> Duration {
    let Some(previous) = previous else {
        return Duration::ZERO;
    };
    NAME_INTERVAL.saturating_sub(now.saturating_duration_since(previous))
}

pub unsafe fn check_window_name(owner: &WindowRef) {
    if owner.active_pane().upgrade().is_none() {
        return;
    }
    if options_get_number(&owner.borrow_options_mut(), c"automatic-rename") == 0 {
        return;
    }
    let pane = owner.active_pane().upgrade().expect("active pane");
    if !pane.has_pending_change() {
        log_debug(format_args!("@{} active pane not changed", owner.id()));
        return;
    }
    log_debug(format_args!("@{} active pane changed", owner.id()));
    let now = Instant::now();
    if !owner.begin_name_check(now) {
        return;
    }
    pane.acknowledge_change();
    drop(pane);
    let name = format_window_name(owner);
    let previous = owner.name();
    if name != previous {
        log_debug(format_args!(
            "@{} new name {} (was {})",
            owner.id(),
            crate::log::log_bytes(name.as_bytes()),
            crate::log::log_bytes(previous.as_bytes())
        ));
        owner.rename(&name, true);
        server_redraw_window_borders(owner);
        server_status_window(owner);
    } else {
        log_debug(format_args!(
            "@{} name not changed (still {})",
            owner.id(),
            crate::log::log_bytes(previous.as_bytes())
        ));
    }
}

pub(crate) unsafe fn default_window_name_cstring(owner: &WindowRef) -> CString {
    owner
        .active_pane()
        .upgrade()
        .map_or_else(|| c"".to_owned(), |pane| pane.default_window_name())
}
unsafe fn format_window_name(owner: &WindowRef) -> CString {
    let mut context = format_create(None, None, (FORMAT_WINDOW | owner.id()) as i32, 0);
    format_defaults_window(&mut *context, Some(owner));
    format_defaults_pane(
        &mut *context,
        &owner.active_pane().upgrade().expect("active pane"),
    );
    let format = crate::options::options_get_string_optional(
        &owner.borrow_options_mut(),
        c"automatic-rename-format",
    );
    let name = format_expand_cstring(&mut *context, format.as_deref().unwrap_or(c""));
    drop(context);
    name
}

pub unsafe fn parse_window_name_cstring(in_0: &CStr) -> CString {
    let mut name: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut ptr: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut copy = in_0.to_bytes_with_nul().to_vec();
    name = copy.as_mut_ptr().cast();
    if *name as ::core::ffi::c_int == '"' as i32 {
        name = name.offset(1);
    }
    *name.offset(strcspn(name, c"\"".as_ptr()) as isize) = '\0' as i32 as ::core::ffi::c_char;
    if strncmp(
        name,
        c"exec ".as_ptr(),
        (::core::mem::size_of::<[::core::ffi::c_char; 6]>() as size_t).wrapping_sub(1 as size_t),
    ) == 0 as ::core::ffi::c_int
    {
        name = name
            .add(::core::mem::size_of::<[::core::ffi::c_char; 6]>() as usize)
            .offset(-(1 as ::core::ffi::c_int as isize));
    }
    while *name as ::core::ffi::c_int == ' ' as i32 || *name as ::core::ffi::c_int == '-' as i32 {
        name = name.offset(1);
    }
    ptr = strchr(name, ' ' as i32);
    if !ptr.is_null() {
        *ptr = '\0' as i32 as ::core::ffi::c_char;
    }
    if *name as ::core::ffi::c_int != '\0' as i32 {
        ptr = name
            .add(strlen(name))
            .offset(-(1 as ::core::ffi::c_int as isize));
        while ptr > name
            && !(*ptr as u_char).is_ascii_alphanumeric()
            && !(*ptr as u_char).is_ascii_punctuation()
        {
            let fresh0 = ptr;
            ptr = ptr.offset(-1);
            *fresh0 = '\0' as i32 as ::core::ffi::c_char;
        }
    }
    let name = CStr::from_ptr(name);
    let name = if name.to_bytes().first() == Some(&b'/') {
        cstr_basename(name)
    } else {
        name.to_owned()
    };
    clean_name_cstring(&name, 0).unwrap_or_else(|| c"".to_owned())
}

#[cfg(test)]
mod owned_name_tests {
    use super::*;

    #[test]
    fn automatic_name_delay_handles_first_check_and_exact_deadline() {
        let previous = Instant::now();
        assert_eq!(name_time_left(None, previous), Duration::ZERO);
        for (elapsed, expected) in [
            (0, 500_000),
            (100_000, 400_000),
            (200_000, 300_000),
            (499_999, 1),
            (500_000, 0),
            (500_001, 0),
            (1_200_000, 0),
        ] {
            assert_eq!(
                name_time_left(Some(previous), previous + Duration::from_micros(elapsed)),
                Duration::from_micros(expected)
            );
        }
    }
}
