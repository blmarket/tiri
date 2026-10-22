use crate::format::bytes::cstr_basename;
use crate::format::{
    format_create, format_defaults_pane, format_defaults_window, format_expand_cstring,
};
use crate::log::log_debug;
use crate::options::options_get_number;
use crate::server_fn::{server_redraw_window_borders, server_status_window};
use crate::shared::format::FORMAT_WINDOW;
use crate::shared::window::WindowRef;
use crate::tmux::clean_name_cstring;
use crate::window::Window as _;
use crate::window::WindowPane as _;
use std::ffi::{CStr, CString};
use std::time::{Duration, Instant};

pub const NAME_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) fn name_time_left(previous: Option<Instant>, now: Instant) -> Duration {
    let Some(previous) = previous else {
        return Duration::ZERO;
    };
    NAME_INTERVAL.saturating_sub(now.saturating_duration_since(previous))
}

pub fn check_window_name(owner: &WindowRef) {
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

pub(crate) fn default_window_name_cstring(owner: &WindowRef) -> CString {
    owner
        .active_pane()
        .upgrade()
        .map_or_else(|| c"".to_owned(), |pane| pane.default_window_name())
}
fn format_window_name(owner: &WindowRef) -> CString {
    let mut context = format_create(None, None, (FORMAT_WINDOW | owner.id()) as i32, 0);
    format_defaults_window(&mut context, Some(owner));
    format_defaults_pane(
        &mut context,
        &owner.active_pane().upgrade().expect("active pane"),
    );
    let format = crate::options::options_get_string_optional(
        &owner.borrow_options_mut(),
        c"automatic-rename-format",
    );
    let name = format_expand_cstring(&mut context, format.as_deref().unwrap_or(c""));
    drop(context);
    name
}

pub fn parse_window_name_cstring(in_0: &CStr) -> CString {
    let mut name = in_0.to_bytes();
    if let Some(rest) = name.strip_prefix(b"\"") {
        name = rest;
    }
    if let Some(end) = name.iter().position(|&byte| byte == b'"') {
        name = &name[..end];
    }
    if let Some(rest) = name.strip_prefix(b"exec ") {
        name = rest;
    }
    while let [b' ' | b'-', rest @ ..] = name {
        name = rest;
    }
    if let Some(end) = name.iter().position(|&byte| byte == b' ') {
        name = &name[..end];
    }
    while let [_, .., last] = name {
        if last.is_ascii_alphanumeric() || last.is_ascii_punctuation() {
            break;
        }
        name = &name[..name.len() - 1];
    }
    let name = CString::new(name).expect("window name has no NUL");
    let name = if name.to_bytes().first() == Some(&b'/') {
        cstr_basename(&name)
    } else {
        name
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
