//! Side effects of option changes.
use super::store::*;
use crate::src::alerts::alerts_reset_all;
use crate::src::input::input_set_buffer_size;
use crate::src::log::{log_cstr, log_debug};
use crate::src::resize::recalculate_sizes;
use crate::src::screen_redraw::redraw_invalidate_all_scenes;
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use libc::{strcmp, strncmp};

use crate::src::server_fn::server_redraw_client;
use crate::src::session::sessions;

use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::status::status_timer_start_all;
use crate::src::text::utf8::utf8_update_width_cache;
use crate::src::tmux::global_options;
use crate::src::tty::tty_invalidate;
use crate::src::tty_keys::tty_keys_build;

use crate::src::shared::abi::*;
use crate::src::shared::tty::TTY_OPENED;
use crate::src::window::{windows, Window, WindowPane};
use std::ffi::CStr;

pub unsafe fn options_push_changes(name: &CStr) {
    let name: *const ::core::ffi::c_char = name.as_ptr();
    let mut loop_0: Option<ClientRef> = None;
    let mut s: Option<SessionRef> = None;
    log_debug(format_args!(
        "{}: {}",
        "options_push_changes",
        log_cstr(CStr::from_ptr(name))
    ));
    if strcmp(name, c"theme".as_ptr()) == 0 as ::core::ffi::c_int
        || strncmp(name, c"dark-theme-".as_ptr(), 11 as size_t) == 0 as ::core::ffi::c_int
        || strncmp(name, c"light-theme-".as_ptr(), 12 as size_t) == 0 as ::core::ffi::c_int
    {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            if let Some(client) = loop_0.as_ref() {
                client.update_theme_colours();
            };
            {
                let terminal = &(loop_0.as_ref().expect("live client"));
                if {
                    let tty_state = terminal.borrow_terminal();
                    tty_state.flags
                } & TTY_OPENED
                    != 0
                {
                    tty_invalidate(terminal);
                }
            };
            server_redraw_client(loop_0.as_ref().expect("live client"));
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if strcmp(name, c"automatic-rename".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            if let Some(active) = window_owner.active_pane().upgrade() {
                if options_get_number(window_owner.borrow_options_mut(), c"automatic-rename") != 0 {
                    active.mark_changed();
                }
            }
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"cursor-colour".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.reset_default_cursor();
        }
    }
    if strcmp(name, c"cursor-style".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.reset_default_cursor();
        }
    }
    if strcmp(name, c"fill-character".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            window_owner.refresh_fill_cells();
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"key-table".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            loop_0.clone().expect("live client").set_key_table(None);
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if strcmp(name, c"utmp".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.refresh_utmp();
        }
    }
    if strcmp(name, c"user-keys".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut registry_loop_0_owner = ClientRegistry::first();
        loop_0 = registry_loop_0_owner.clone();
        while !loop_0.is_none() {
            {
                let mut terminal = loop_0.as_ref().expect("live client").borrow_terminal_mut();
                if terminal.flags & TTY_OPENED != 0 {
                    tty_keys_build(&mut *terminal);
                }
            }
            registry_loop_0_owner = ClientRegistry::next(
                registry_loop_0_owner
                    .as_ref()
                    .expect("current registry client"),
            );
            loop_0 = registry_loop_0_owner.clone();
        }
    }
    if strcmp(name, c"status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"status-interval".as_ptr()) == 0 as ::core::ffi::c_int
    {
        status_timer_start_all();
    }
    if strcmp(name, c"status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"status-position".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-border-indicators".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-border-lines".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-border-status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-timeout".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-position".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-style".as_ptr()) == 0 as ::core::ffi::c_int
    {
        redraw_invalidate_all_scenes();
    }
    if strcmp(name, c"monitor-silence".as_ptr()) == 0 as ::core::ffi::c_int {
        alerts_reset_all();
    }
    if strcmp(name, c"window-style".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"window-active-style".as_ptr()) == 0 as ::core::ffi::c_int
    {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.mark_style_changed(true);
        }
    }
    if *name as ::core::ffi::c_int == '@' as i32 {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.mark_style_changed(false);
        }
    }
    if strcmp(name, c"pane-colours".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.refresh_palette();
        }
    }
    if strcmp(name, c"pane-border-status".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars".as_ptr()) == 0 as ::core::ffi::c_int
        || strcmp(name, c"pane-scrollbars-position".as_ptr()) == 0 as ::core::ffi::c_int
    {
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            window_owner.refresh_scrollbars();
            window_owner.refit();
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"pane-scrollbars".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.hide_scrollbar();
        }
    }
    if strcmp(name, c"pane-scrollbars-style".as_ptr()) == 0 as ::core::ffi::c_int {
        for pane_owner in crate::src::shared::pane::window_pane_tree::snapshot() {
            pane_owner.refresh_scrollbar_style();
        }
        let mut window_cursor = windows::first();
        while let Some(window_owner) = window_cursor.take() {
            window_owner.refit();
            window_cursor = window_owner.next_window().upgrade();
            drop(window_owner);
        }
    }
    if strcmp(name, c"codepoint-widths".as_ptr()) == 0 as ::core::ffi::c_int {
        utf8_update_width_cache();
    }
    if strcmp(name, c"input-buffer-size".as_ptr()) == 0 as ::core::ffi::c_int {
        input_set_buffer_size(options_get_number(global_options(), c"input-buffer-size") as size_t);
    }
    if strcmp(name, c"history-limit".as_ptr()) == 0 as ::core::ffi::c_int {
        let mut s_owner = sessions::first();
        s = s_owner.clone();
        while !s.is_none() {
            s.as_ref().expect("live session").update_history();
            s_owner = sessions::after(s.as_ref().expect("live session").name().as_c_str());
            s = s_owner.clone();
        }
    }
    recalculate_sizes();
    let mut registry_loop_0_owner = ClientRegistry::first();
    loop_0 = registry_loop_0_owner.clone();
    while !loop_0.is_none() {
        if !loop_0
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
        {
            server_redraw_client(loop_0.as_ref().expect("live client"));
        }
        registry_loop_0_owner = ClientRegistry::next(
            registry_loop_0_owner
                .as_ref()
                .expect("current registry client"),
        );
        loop_0 = registry_loop_0_owner.clone();
    }
}
