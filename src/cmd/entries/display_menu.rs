use crate::arguments::{
    args, args_count, args_flag_values, args_get, args_has, args_parse, args_parse_type,
    args_percentage_result, args_string, args_strtonum_result, args_to_vector, ArgsParseError,
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING,
};
use crate::cmd::queue::{cmdq_error, cmdq_get_event, cmdq_get_target_client, cmdq_wait};
use crate::cmd::{cmd_append_argv, cmd_get_args_mut};
use crate::environ::{environ_create, environ_put};
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create_from_target, format_expand_cstring, format_single_from_target_cstring,
};
use crate::key_string::key_string_parse_cstr;
use crate::log::{log_cstr, log_debug};
use crate::menu::{menu_add_item, menu_create, menu_display};
use crate::options::{options_find_choice, options_get_number, options_get_string};
use crate::popup::{popup_display, popup_modify, popup_present};
use libc::strtol;
use std::ffi::CStr;

use crate::server_client::Client as _;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_CONTROL;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CFLAG};
use crate::shared::environment::environ;
use crate::shared::key::key_event;
use crate::shared::key::*;
use crate::shared::layout::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::menu::{menu_item, MENU_NOMOUSE, MENU_STAYOPEN};
use crate::shared::popup::{POPUP_CLOSEANYKEY, POPUP_CLOSEEXIT, POPUP_CLOSEEXITZERO};
use crate::shared::posix_io::_PATH_BSHELL;
use crate::shared::session::SessionRef;
use crate::shared::style::*;
use crate::shared::tty::tty_window_view;
use crate::shared::window::winlink;
use crate::status::{status_at_line, status_line_size};
use crate::tmux::checkshell;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
pub static cmd_display_menu_entry: cmd_entry = {
    cmd_entry {
        name: c"display-menu",
        alias: Some(c"menu"),
        args: args_parse {
            template: c"b:c:C:H:s:S:MOt:T:x:y:",
            lower: 1 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: Some(
                cmd_display_menu_args_parse
            ),
        },
        usage: c"[-MO] [-b border-lines] [-c target-client] [-C starting-choice] [-H selected-style] [-s style] [-S border-style] [-t target-pane] [-T title] [-x position] [-y position] name [key] [command] ...",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_CFLAG,
        exec: Some(cmd_display_menu_exec),
    }
};
pub static cmd_display_popup_entry: cmd_entry = {
    cmd_entry {
        name: c"display-popup",
        alias: Some(c"popup"),
        args: args_parse {
            template: c"Bb:Cc:d:e:Eh:kNs:S:t:T:w:x:y:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-BCEkN] [-b border-lines] [-c target-client] [-d start-directory] [-e environment] [-h height] [-s style] [-S border-style] [-t target-pane] [-T title] [-w width] [-x position] [-y position] [shell-command [argument ...]]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_CFLAG,
        exec: Some(cmd_display_popup_exec),
    }
};
fn cmd_display_menu_args_parse(
    args: &mut args,
    idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    let mut i: u_int = 0 as u_int;
    let mut type_0: args_parse_type = ARGS_PARSE_STRING;
    loop {
        type_0 = ARGS_PARSE_STRING;
        if i == idx {
            break;
        }
        let fresh0 = i;
        i = i.wrapping_add(1);
        if args_string(args, fresh0)
            .expect("argument is present")
            .is_empty()
        {
            continue;
        }
        type_0 = ARGS_PARSE_STRING;
        let fresh1 = i;
        i = i.wrapping_add(1);
        if fresh1 == idx {
            break;
        }
        type_0 = ARGS_PARSE_COMMANDS_OR_STRING;
        let fresh2 = i;
        i = i.wrapping_add(1);
        if fresh2 == idx {
            break;
        }
    }
    Ok(type_0)
}
unsafe fn cmd_display_menu_status_position(
    client: &ClientRef,
    lines: u_int,
    window_index: u_int,
) -> Option<(u_int, u_int)> {
    let status = client.borrow_status();
    status.entries[..lines as usize]
        .iter()
        .enumerate()
        .find_map(|(line, entry)| {
            entry.ranges.as_slice().iter().find_map(|range| {
                (range.type_0 == STYLE_RANGE_WINDOW && range.argument == window_index)
                    .then_some((line as u_int, range.start))
            })
        })
}

unsafe fn cmd_display_menu_get_popup_pos(
    tc_owner: &ClientRef,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    args: &args,
    px: &mut u_int,
    py: &mut u_int,
    mut w: u_int,
    mut h: u_int,
) -> ::core::ffi::c_int {
    let item = item_handle.borrow();
    let mut tc: Option<ClientRef> = Some(tc_owner.clone());
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut event_snapshot = cmdq_get_event(&*(item));
    let event: *mut key_event = &mut event_snapshot;
    let mut s: Option<SessionRef> = tc
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let pane = (*target).pane_handle().expect("menu target pane");
    let mut xp: &CStr = c"";
    let mut yp: &CStr = c"";
    let mut top: ::core::ffi::c_int = 0;
    let _line: u_int = 0;
    let mut lines: u_int = 0;
    let mut position: u_int = 0;
    let mut n: ::core::ffi::c_long = 0;
    if w > tc.as_ref().expect("live client").terminal_size().0
        || h > tc.as_ref().expect("live client").terminal_size().1
    {
        return 0 as ::core::ffi::c_int;
    }
    let mut ft_owner = format_create_from_target(item_handle);
    let ft = &mut *ft_owner;
    if (*event).m.valid != 0 {
        format_add(ft, c"popup_mouse_x", |out| {
            write!(out, "{}", { (*event).m.x })
        });
        format_add(ft, c"popup_mouse_y", |out| {
            write!(out, "{}", { (*event).m.y })
        });
    }
    let (last_x, last_y) = (*target)
        .window_handle()
        .expect("popup target window")
        .last_menu_position();
    format_add(ft, c"popup_last_x", |out| write!(out, "{}", last_x));
    format_add(ft, c"popup_last_y", |out| {
        write!(out, "{}", last_y.wrapping_add(h))
    });
    top = status_at_line(tc.as_ref().expect("live client"));
    if top != -(1 as ::core::ffi::c_int) {
        lines = status_line_size(tc.as_ref().expect("live client"));
        if top == 0 as ::core::ffi::c_int {
            top = lines as ::core::ffi::c_int;
        } else {
            top = 0 as ::core::ffi::c_int;
        }
        position = options_get_number(
            &s.as_ref().expect("live session").borrow_options_mut(),
            c"status-position",
        ) as u_int;
        let index = wl.get_unchecked().idx as u_int;
        if let Some((line, start)) =
            cmd_display_menu_status_position(tc.as_ref().expect("live client"), lines, index)
        {
            format_add(ft, c"popup_window_status_line_x", |out| {
                write!(out, "{}", { start })
            });
            if position == 0 as u_int {
                format_add(ft, c"popup_window_status_line_y", |out| {
                    write!(out, "{}", { line.wrapping_add(1 as u_int).wrapping_add(h) })
                });
            } else {
                format_add(ft, c"popup_window_status_line_y", |out| {
                    write!(
                        out,
                        "{}",
                        (tc.as_ref()
                            .expect("live client")
                            .terminal_size()
                            .1
                            .wrapping_sub(lines)
                            .wrapping_add(line)) as u32
                    )
                });
            }
        }
        if position == 0 as u_int {
            format_add(ft, c"popup_status_line_y", |out| {
                write!(out, "{}", (lines.wrapping_add(h)) as u32)
            });
        } else {
            format_add(ft, c"popup_status_line_y", |out| {
                write!(
                    out,
                    "{}",
                    (tc.as_ref()
                        .expect("live client")
                        .terminal_size()
                        .1
                        .wrapping_sub(lines)) as u32
                )
            });
        }
    } else {
        top = 0 as ::core::ffi::c_int;
    }
    format_add(ft, c"popup_width", |out| write!(out, "{}", { w }));
    format_add(ft, c"popup_height", |out| write!(out, "{}", { h }));
    n = tc
        .as_ref()
        .expect("live client")
        .terminal_size()
        .0
        .wrapping_sub(1 as u_int) as ::core::ffi::c_long
        / 2 as ::core::ffi::c_long
        - w.wrapping_div(2 as u_int) as ::core::ffi::c_long;
    if n < 0 as ::core::ffi::c_long {
        format_add(ft, c"popup_centre_x", |out| {
            write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
        });
    } else {
        format_add(ft, c"popup_centre_x", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    n = tc
        .as_ref()
        .expect("live client")
        .terminal_size()
        .1
        .wrapping_sub(1 as u_int)
        .wrapping_div(2 as u_int)
        .wrapping_add(h.wrapping_div(2 as u_int)) as ::core::ffi::c_long;
    if n >= tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_long {
        format_add(ft, c"popup_centre_y", |out| {
            write!(out, "{}", {
                tc.as_ref()
                    .expect("live client")
                    .terminal_size()
                    .1
                    .wrapping_sub(h)
            })
        });
    } else {
        format_add(ft, c"popup_centre_y", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    if (*event).m.valid != 0 {
        n = (*event).m.x as ::core::ffi::c_long - w.wrapping_div(2 as u_int) as ::core::ffi::c_long;
        if n < 0 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_centre_x", |out| {
                write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
            });
        } else {
            format_add(ft, c"popup_mouse_centre_x", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
        n = (*event).m.y.wrapping_sub(h.wrapping_div(2 as u_int)) as ::core::ffi::c_long;
        if n + h as ::core::ffi::c_long
            >= tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_long
        {
            format_add(ft, c"popup_mouse_centre_y", |out| {
                write!(out, "{}", {
                    tc.as_ref()
                        .expect("live client")
                        .terminal_size()
                        .1
                        .wrapping_sub(h)
                })
            });
        } else {
            format_add(ft, c"popup_mouse_centre_y", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
        n = (*event).m.y as ::core::ffi::c_long + h as ::core::ffi::c_long;
        if n >= tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_top", |out| {
                write!(out, "{}", {
                    tc.as_ref()
                        .expect("live client")
                        .terminal_size()
                        .1
                        .wrapping_sub(1 as u_int)
                })
            });
        } else {
            format_add(ft, c"popup_mouse_top", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
        n = (*event).m.y.wrapping_sub(h) as ::core::ffi::c_long;
        if n < 0 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_bottom", |out| {
                write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
            });
        } else {
            format_add(ft, c"popup_mouse_bottom", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
    }
    let tty_window_view { ox, oy, .. } = tc.as_ref().expect("live client").terminal_view();
    let (pane_width, pane_height, pane_x, pane_y) = pane.geometry();
    n = ((top + pane_y) as u_int).wrapping_sub(oy).wrapping_add(h) as ::core::ffi::c_long;
    if n >= tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_long {
        format_add(ft, c"popup_pane_top", |out| {
            write!(out, "{}", {
                tc.as_ref()
                    .expect("live client")
                    .terminal_size()
                    .1
                    .wrapping_sub(h)
            })
        });
    } else {
        format_add(ft, c"popup_pane_top", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    format_add(ft, c"popup_pane_bottom", |out| {
        write!(
            out,
            "{}",
            (((top + pane_y) as u_int)
                .wrapping_add(pane_height)
                .wrapping_sub(oy)) as u32
        )
    });
    format_add(ft, c"popup_pane_left", |out| {
        write!(out, "{}", { (pane_x as u_int).wrapping_sub(ox) })
    });
    n = pane_x as ::core::ffi::c_long + pane_width as ::core::ffi::c_long
        - ox as ::core::ffi::c_long
        - w as ::core::ffi::c_long;
    if n < 0 as ::core::ffi::c_long {
        format_add(ft, c"popup_pane_right", |out| {
            write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
        });
    } else {
        format_add(ft, c"popup_pane_right", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    xp = args_get(&*(args), 'x' as i32 as u_char).unwrap_or(c"C");
    xp = match xp.to_bytes() {
        b"C" => c"#{popup_centre_x}",
        b"R" => c"#{popup_pane_right}",
        b"P" => c"#{popup_pane_left}",
        b"M" => c"#{popup_mouse_centre_x}",
        b"L" => c"#{popup_last_x}",
        b"W" => c"#{popup_window_status_line_x}",
        _ => xp,
    };
    let p = format_expand_cstring(ft, xp.as_ptr());
    n = strtol(
        p.as_ptr(),
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>(),
        10 as ::core::ffi::c_int,
    );
    if n + w as ::core::ffi::c_long
        >= tc.as_ref().expect("live client").terminal_size().0 as ::core::ffi::c_long
    {
        n = tc
            .as_ref()
            .expect("live client")
            .terminal_size()
            .0
            .wrapping_sub(w) as ::core::ffi::c_long;
    } else if n < 0 as ::core::ffi::c_long {
        n = 0 as ::core::ffi::c_long;
    }
    *px = n as u_int;
    log_debug(format_args!(
        "{}: -x: {} = {} = {} (-w {})",
        "cmd_display_menu_get_popup_pos",
        log_cstr(xp),
        log_cstr(&p),
        { *px },
        { w }
    ));
    yp = args_get(&*(args), 'y' as i32 as u_char).unwrap_or(c"C");
    yp = match yp.to_bytes() {
        b"C" => c"#{popup_centre_y}",
        b"P" => c"#{popup_pane_bottom}",
        b"M" => c"#{popup_mouse_top}",
        b"L" => c"#{popup_last_y}",
        b"S" => c"#{popup_status_line_y}",
        b"W" => c"#{popup_window_status_line_y}",
        _ => yp,
    };
    let p = format_expand_cstring(ft, yp.as_ptr());
    n = strtol(
        p.as_ptr(),
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>(),
        10 as ::core::ffi::c_int,
    );
    if n < h as ::core::ffi::c_long {
        n = 0 as ::core::ffi::c_long;
    } else {
        n -= h as ::core::ffi::c_long;
    }
    if n + h as ::core::ffi::c_long
        >= tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_long
    {
        n = tc
            .as_ref()
            .expect("live client")
            .terminal_size()
            .1
            .wrapping_sub(h) as ::core::ffi::c_long;
    } else if n < 0 as ::core::ffi::c_long {
        n = 0 as ::core::ffi::c_long;
    }
    *py = n as u_int;
    log_debug(format_args!(
        "{}: -y: {} = {} = {} (-h {})",
        "cmd_display_menu_get_popup_pos",
        log_cstr(yp),
        log_cstr(&p),
        { *py },
        { h }
    ));
    drop(ft_owner);
    1 as ::core::ffi::c_int
}
unsafe fn cmd_display_menu_get_menu_pos(
    tc_owner: &ClientRef,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    args: &args,
    px: &mut u_int,
    py: &mut u_int,
    mut w: u_int,
    mut h: u_int,
) -> ::core::ffi::c_int {
    let item = item_handle.borrow();
    let mut tc: Option<ClientRef> = Some(tc_owner.clone());
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut event_snapshot = cmdq_get_event(&*(item));
    let event: *mut key_event = &mut event_snapshot;
    let mut s: Option<SessionRef> = tc
        .as_ref()
        .expect("live client")
        .attached_session()
        .upgrade();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let window = (*target).window_handle().expect("menu target window");
    let pane = (*target).pane_handle().expect("menu target pane");
    let mut xp: &CStr = c"";
    let mut yp: &CStr = c"";
    let _line: u_int = 0;
    let mut lines: u_int = 0;
    let mut position: u_int = 0;
    let mut n: ::core::ffi::c_long = 0;
    let mut max_x: ::core::ffi::c_long = 0;
    let mut max_y: ::core::ffi::c_long = 0;
    let mut mouse_x: ::core::ffi::c_long = 0 as ::core::ffi::c_long;
    let mut mouse_y: ::core::ffi::c_long = 0 as ::core::ffi::c_long;
    let tty_window_view { ox, oy, sx, sy, .. } = tc.as_ref().expect("live client").terminal_view();
    let logical = (ox.wrapping_add(sx), oy.wrapping_add(sy));
    max_x = ox.wrapping_add(sx.saturating_sub(w)) as ::core::ffi::c_long;
    max_y = oy.wrapping_add(sy.saturating_sub(h)) as ::core::ffi::c_long;
    let mut ft_owner = format_create_from_target(item_handle);
    let ft = &mut *ft_owner;
    if (*event).m.valid != 0 {
        mouse_x = (*event).m.x.wrapping_add(ox) as ::core::ffi::c_long;
        if (*event).m.statusat == 0 as ::core::ffi::c_int {
            if (*event).m.y >= (*event).m.statuslines {
                mouse_y = (*event)
                    .m
                    .y
                    .wrapping_sub((*event).m.statuslines)
                    .wrapping_add(oy) as ::core::ffi::c_long;
            } else {
                mouse_y = oy as ::core::ffi::c_long;
            }
        } else if (*event).m.statusat > 0 as ::core::ffi::c_int
            && (*event).m.y >= (*event).m.statusat as u_int
        {
            mouse_y = oy.wrapping_add(sy).wrapping_sub(1 as u_int) as ::core::ffi::c_long;
        } else {
            mouse_y = (*event).m.y.wrapping_add(oy) as ::core::ffi::c_long;
        }
        format_add(ft, c"popup_mouse_x", |out| {
            write!(out, "{}", (mouse_x) as ::core::ffi::c_long)
        });
        format_add(ft, c"popup_mouse_y", |out| {
            write!(out, "{}", (mouse_y) as ::core::ffi::c_long)
        });
    }
    format_add(ft, c"popup_last_x", |out| {
        write!(out, "{}", { window.last_menu_position().0 })
    });
    format_add(ft, c"popup_last_y", |out| {
        write!(out, "{}", { window.last_menu_position().1.wrapping_add(h) })
    });
    lines = status_line_size(tc.as_ref().expect("live client"));
    position = options_get_number(
        &s.as_ref().expect("live session").borrow_options_mut(),
        c"status-position",
    ) as u_int;
    if status_at_line(tc.as_ref().expect("live client")) != -(1 as ::core::ffi::c_int)
        && lines != 0 as u_int
    {
        let index = wl.get_unchecked().idx as u_int;
        if let Some((_line, start)) =
            cmd_display_menu_status_position(tc.as_ref().expect("live client"), lines, index)
        {
            format_add(ft, c"popup_window_status_line_x", |out| {
                write!(out, "{}", { start.wrapping_add(ox) })
            });
            if position == 0 as u_int {
                format_add(ft, c"popup_window_status_line_y", |out| {
                    write!(out, "{}", { h })
                });
            } else {
                format_add(ft, c"popup_window_status_line_y", |out| {
                    write!(out, "{}", { logical.1 })
                });
            }
        }
        if position == 0 as u_int {
            format_add(ft, c"popup_status_line_y", |out| write!(out, "{}", { h }));
        } else {
            format_add(ft, c"popup_status_line_y", |out| {
                write!(out, "{}", { logical.1 })
            });
        }
    }
    format_add(ft, c"popup_width", |out| write!(out, "{}", { w }));
    format_add(ft, c"popup_height", |out| write!(out, "{}", { h }));
    n = (sx as ::core::ffi::c_long - 1 as ::core::ffi::c_long) / 2 as ::core::ffi::c_long
        - w.wrapping_div(2 as u_int) as ::core::ffi::c_long;
    if n < 0 as ::core::ffi::c_long {
        format_add(ft, c"popup_centre_x", |out| write!(out, "{}", ox));
    } else {
        format_add(ft, c"popup_centre_x", |out| {
            write!(out, "{}", n + ox as ::core::ffi::c_long)
        });
    }
    n = oy as ::core::ffi::c_long
        + (sy as ::core::ffi::c_long - 1 as ::core::ffi::c_long) / 2 as ::core::ffi::c_long
        + h.wrapping_div(2 as u_int) as ::core::ffi::c_long;
    if n >= logical.1 as ::core::ffi::c_long {
        format_add(ft, c"popup_centre_y", |out| {
            write!(out, "{}", (max_y) as ::core::ffi::c_long)
        });
    } else {
        format_add(ft, c"popup_centre_y", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    if (*event).m.valid != 0 {
        n = mouse_x - w.wrapping_div(2 as u_int) as ::core::ffi::c_long;
        if n < 0 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_centre_x", |out| {
                write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
            });
        } else {
            format_add(ft, c"popup_mouse_centre_x", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
        n = mouse_y - h.wrapping_div(2 as u_int) as ::core::ffi::c_long;
        if n + h as ::core::ffi::c_long >= logical.1 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_centre_y", |out| {
                write!(out, "{}", (max_y) as ::core::ffi::c_long)
            });
        } else {
            format_add(ft, c"popup_mouse_centre_y", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
        n = mouse_y + h as ::core::ffi::c_long;
        if n >= logical.1 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_top", |out| {
                write!(out, "{}", { logical.1.wrapping_sub(1 as u_int) })
            });
        } else {
            format_add(ft, c"popup_mouse_top", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
        n = mouse_y - h as ::core::ffi::c_long;
        if n < 0 as ::core::ffi::c_long {
            format_add(ft, c"popup_mouse_bottom", |out| {
                write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
            });
        } else {
            format_add(ft, c"popup_mouse_bottom", |out| {
                write!(out, "{}", (n) as ::core::ffi::c_long)
            });
        }
    }
    let (pane_width, pane_height, pane_x, pane_y) = pane.geometry();
    n = (pane_y as u_int).wrapping_add(h) as ::core::ffi::c_long;
    if n >= logical.1 as ::core::ffi::c_long {
        format_add(ft, c"popup_pane_top", |out| {
            write!(out, "{}", (max_y) as ::core::ffi::c_long)
        });
    } else {
        format_add(ft, c"popup_pane_top", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    format_add(ft, c"popup_pane_bottom", |out| {
        write!(out, "{}", { (pane_y as u_int).wrapping_add(pane_height) })
    });
    format_add(ft, c"popup_pane_left", |out| {
        write!(out, "{}", (pane_x) as u32)
    });
    n = pane_x as ::core::ffi::c_long + pane_width as ::core::ffi::c_long
        - w as ::core::ffi::c_long;
    if n < 0 as ::core::ffi::c_long {
        format_add(ft, c"popup_pane_right", |out| {
            write!(out, "{}", (0 as ::core::ffi::c_int) as u32)
        });
    } else {
        format_add(ft, c"popup_pane_right", |out| {
            write!(out, "{}", (n) as ::core::ffi::c_long)
        });
    }
    xp = args_get(&*(args), 'x' as i32 as u_char).unwrap_or(c"C");
    xp = match xp.to_bytes() {
        b"C" => c"#{popup_centre_x}",
        b"R" => c"#{popup_pane_right}",
        b"P" => c"#{popup_pane_left}",
        b"M" => c"#{popup_mouse_centre_x}",
        b"L" => c"#{popup_last_x}",
        b"W" => c"#{popup_window_status_line_x}",
        _ => xp,
    };
    let p = format_expand_cstring(ft, xp.as_ptr());
    n = strtol(
        p.as_ptr(),
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>(),
        10 as ::core::ffi::c_int,
    );
    if n < ox as ::core::ffi::c_long {
        n = ox as ::core::ffi::c_long;
    }
    if n > max_x {
        n = max_x;
    }
    *px = n as u_int;
    log_debug(format_args!(
        "{}: -x: {} = {} = {} (-w {})",
        "cmd_display_menu_get_menu_pos",
        log_cstr(xp),
        log_cstr(&p),
        { *px },
        { w }
    ));
    yp = args_get(&*(args), 'y' as i32 as u_char).unwrap_or(c"C");
    yp = match yp.to_bytes() {
        b"C" => c"#{popup_centre_y}",
        b"P" => c"#{popup_pane_bottom}",
        b"M" => c"#{popup_mouse_top}",
        b"L" => c"#{popup_last_y}",
        b"S" => c"#{popup_status_line_y}",
        b"W" => c"#{popup_window_status_line_y}",
        _ => yp,
    };
    let p = format_expand_cstring(ft, yp.as_ptr());
    n = strtol(
        p.as_ptr(),
        ::core::ptr::null_mut::<*mut ::core::ffi::c_char>(),
        10 as ::core::ffi::c_int,
    );
    if n < h as ::core::ffi::c_long {
        n = 0 as ::core::ffi::c_long;
    } else {
        n -= h as ::core::ffi::c_long;
    }
    if n < oy as ::core::ffi::c_long {
        n = oy as ::core::ffi::c_long;
    }
    if n > max_y {
        n = max_y;
    }
    *py = n as u_int;
    log_debug(format_args!(
        "{}: -y: {} = {} = {} (-h {})",
        "cmd_display_menu_get_menu_pos",
        log_cstr(yp),
        log_cstr(&p),
        { *py },
        { h }
    ));
    drop(ft_owner);
    drop(window);
    1 as ::core::ffi::c_int
}
unsafe fn cmd_display_menu_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let target = &target_state;
    let mut event_snapshot = cmdq_get_event(&*(item));
    let event: *mut key_event = &mut event_snapshot;
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let style = args_get(&*(args), b's');
    let border_style = args_get(&*(args), b'S');
    let selected_style = args_get(&*(args), b'H');
    let mut starting_choice = 0;
    if args_has(&*args, b'C') != 0 {
        if args_get(&*(args), b'C').expect("argument is present") == c"-" {
            starting_choice = -1;
        } else {
            match args_strtonum_result(&*args, b'C', 0, UINT_MAX as i64) {
                Ok(value) => starting_choice = value as i32,
                Err(error) => {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"starting choice ")?;
                        write_cstr(out, error.message())
                    });
                    return CmdOutcome::Error;
                }
            }
        }
    }
    let title = if args_has(&*args, b'T') != 0 {
        Some(format_single_from_target_cstring(
            item_handle,
            args_get(&*(args), b'T').map_or(std::ptr::null(), |value| value.as_ptr()),
        ))
    } else {
        None
    };
    let mut menu = menu_create(title.as_deref().unwrap_or(c""));
    drop(title);
    let count = args_count(&*args);
    let mut i = 0;
    while i != count {
        let name = args_string(&mut *(args), i)
            .expect("argument is present")
            .to_owned();
        i += 1;
        if name.is_empty() {
            menu_add_item(
                &mut menu,
                None,
                Some(item_handle),
                tc_owner.as_ref(),
                Some(target),
            );
            continue;
        }
        if count - i < 2 {
            cmdq_error(item_handle, |out| out.write_all(b"not enough arguments"));
            return CmdOutcome::Error;
        }
        let key = key_string_parse_cstr(args_string(&mut *(args), i).expect("argument is present"))
            .unwrap_or(KEYC_UNKNOWN);
        let command = args_string(&mut *(args), i + 1).expect("argument is present");
        i += 2;
        let definition = menu_item {
            name: &name,
            key,
            command: Some(command),
        };
        menu_add_item(
            &mut menu,
            Some(&definition),
            Some(item_handle),
            tc_owner.as_ref(),
            Some(target),
        );
    }
    let mut px = 0;
    let mut py = 0;
    if menu.count() == 0
        || cmd_display_menu_get_menu_pos(
            &tc.clone().expect("live client"),
            item_handle,
            &*args,
            &mut px,
            &mut py,
            menu.width.wrapping_add(4),
            menu.count().wrapping_add(2),
        ) == 0
    {
        return CmdOutcome::Normal;
    }
    let mut lines = BOX_LINES_DEFAULT;
    if let Some(value) = args_get(&*(args), b'b') {
        match options_find_choice(
            crate::options::options_table
                .iter()
                .find(|entry| entry.name == Some(c"menu-border-lines"))
                .expect("built-in border choice"),
            value,
        ) {
            Ok(choice) => lines = choice as box_lines,
            Err(cause) => {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"menu-border-lines ")?;
                    write_cstr(out, cause.as_c_str())
                });
                return CmdOutcome::Error;
            }
        }
    }
    let mut flags = 0;
    if args_has(&*args, b'O') != 0 {
        flags |= MENU_STAYOPEN;
    }
    if (*event).m.valid == 0 && args_has(&*args, b'M') == 0 {
        flags |= MENU_NOMOUSE;
    }
    menu_display(
        menu,
        flags,
        starting_choice,
        Some(&*event),
        px,
        py,
        tc_owner.as_ref(),
        lines,
        style,
        selected_style,
        border_style,
        Some(target),
        None,
    );
    CmdOutcome::Normal
}
unsafe fn cmd_display_popup_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let mut shellcmd_session_value: Option<std::ffi::CString> = None;
    let mut shell_session_value: Option<std::ffi::CString> = None;

    let item = item_handle.borrow();
    let mut current_block: u64;
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut s: Option<SessionRef> = (*target).session_handle();
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut value: Option<&CStr> = None;
    let mut shellcmd: Option<&CStr> = None;
    let style = args_get(&*(args), 's' as i32 as u_char);
    let border_style = args_get(&*(args), 'S' as i32 as u_char);
    let mut cwd: Option<&CStr> = None;
    let mut formatted_cwd: Option<std::ffi::CString> = None;
    let mut default_cwd: Option<std::ffi::CString> = None;
    let mut argv_owner = Vec::new();
    let title: &CStr;
    let mut formatted_title: Option<std::ffi::CString> = None;
    let mut modify: ::core::ffi::c_int = popup_present(&tc.clone().expect("live client"));
    let mut flags: ::core::ffi::c_int = -(1 as ::core::ffi::c_int);
    let mut lines: box_lines = BOX_LINES_DEFAULT;
    let mut px: u_int = 0;
    let mut py: u_int = 0;
    let mut w: u_int = 0;
    let mut h: u_int = 0;
    let mut count: u_int = args_count(&*args);
    let mut env: Option<Box<environ>> = None;
    if args_has(&*args, 'C' as i32 as u_char) != 0 {
        tc.as_ref().expect("live client").clear_overlay();
        return CmdOutcome::Normal;
    }
    if tc.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0 {
        return CmdOutcome::Normal;
    }
    if modify == 0 && tc.as_ref().expect("live client").has_overlay() {
        return CmdOutcome::Normal;
    }
    if modify == 0 {
        h = tc
            .as_ref()
            .expect("live client")
            .terminal_size()
            .1
            .wrapping_div(2 as u_int);
        if args_has(&*args, 'h' as i32 as u_char) != 0 {
            match args_percentage_result(
                &*args,
                'h' as i32 as u_char,
                1 as ::core::ffi::c_longlong,
                tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_longlong,
                tc.as_ref().expect("live client").terminal_size().1 as ::core::ffi::c_longlong,
            ) {
                Ok(value) => {
                    h = value as u_int;
                    current_block = 17833034027772472439;
                }
                Err(error) => {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"height ")?;
                        write_cstr(out, error.message())
                    });
                    current_block = 1988999557336856620;
                }
            }
        } else {
            current_block = 17833034027772472439;
        }
        match current_block {
            1988999557336856620 => {}
            _ => {
                w = tc
                    .as_ref()
                    .expect("live client")
                    .terminal_size()
                    .0
                    .wrapping_div(2 as u_int);
                if args_has(&*args, 'w' as i32 as u_char) != 0 {
                    match args_percentage_result(
                        &*args,
                        'w' as i32 as u_char,
                        1 as ::core::ffi::c_longlong,
                        tc.as_ref().expect("live client").terminal_size().0
                            as ::core::ffi::c_longlong,
                        tc.as_ref().expect("live client").terminal_size().0
                            as ::core::ffi::c_longlong,
                    ) {
                        Ok(value) => {
                            w = value as u_int;
                            current_block = 11042950489265723346;
                        }
                        Err(error) => {
                            cmdq_error(item_handle, |out| {
                                out.write_all(b"width ")?;
                                write_cstr(out, error.message())
                            });
                            current_block = 1988999557336856620;
                        }
                    }
                } else {
                    current_block = 11042950489265723346;
                }
                match current_block {
                    1988999557336856620 => {}
                    _ => {
                        if w > tc.as_ref().expect("live client").terminal_size().0 {
                            w = tc.as_ref().expect("live client").terminal_size().0;
                        }
                        if h > tc.as_ref().expect("live client").terminal_size().1 {
                            h = tc.as_ref().expect("live client").terminal_size().1;
                        }
                        if cmd_display_menu_get_popup_pos(
                            &tc.clone().expect("live client"),
                            item_handle,
                            &*args,
                            &mut px,
                            &mut py,
                            w,
                            h,
                        ) == 0
                        {
                            current_block = 6589043366517631393;
                        } else {
                            value = args_get(&*(args), 'd' as i32 as u_char);
                            if let Some(value) = value {
                                formatted_cwd = Some(format_single_from_target_cstring(
                                    item_handle,
                                    value.as_ptr(),
                                ));
                                cwd = formatted_cwd.as_deref();
                            } else {
                                default_cwd = ClientRef::working_directory(tc.as_ref(), s.as_ref());
                                cwd = Some(default_cwd.as_deref().expect("default cwd was copied"));
                            }
                            if count == 0 as u_int {
                                shellcmd_session_value = Some(options_get_string(
                                    &s.as_ref().expect("live session").borrow_options_mut(),
                                    c"default-command",
                                ));
                                shellcmd = shellcmd_session_value.as_deref();
                            } else if count == 1 as u_int {
                                shellcmd = args_string(&mut *(args), 0 as u_int);
                            }
                            if count <= 1 as u_int && shellcmd.is_none_or(CStr::is_empty) {
                                shellcmd = None;
                                shell_session_value = Some(options_get_string(
                                    &s.as_ref().expect("live session").borrow_options_mut(),
                                    c"default-shell",
                                ));
                                let mut shell: &CStr =
                                    shell_session_value.as_deref().expect("option snapshot");
                                if checkshell(Some(shell)) == 0 {
                                    shell = _PATH_BSHELL;
                                }
                                cmd_append_argv(&mut argv_owner, shell);
                            } else {
                                argv_owner = args_to_vector(&*args);
                            }
                            if args_has(&*args, 'e' as i32 as u_char) >= 1 as ::core::ffi::c_int {
                                env = Some(environ_create());
                                for av in args_flag_values(&*args, 'e' as i32 as u_char) {
                                    environ_put(
                                        env.as_deref_mut().expect("environment"),
                                        av.as_string().expect("string argument"),
                                        0 as ::core::ffi::c_int,
                                    );
                                }
                            }
                            current_block = 1345366029464561491;
                        }
                    }
                }
            }
        }
    } else {
        current_block = 1345366029464561491;
    }
    if current_block == 1345366029464561491 {
        value = args_get(&*(args), 'b' as i32 as u_char);
        if args_has(&*args, 'B' as i32 as u_char) != 0 {
            lines = BOX_LINES_NONE;
            current_block = 8151474771948790331;
        } else if let Some(value) = value {
            match options_find_choice(
                crate::options::options_table
                    .iter()
                    .find(|entry| entry.name == Some(c"popup-border-lines"))
                    .expect("built-in border choice"),
                value,
            ) {
                Ok(choice) => {
                    lines = choice as box_lines;
                    current_block = 8151474771948790331;
                }
                Err(cause) => {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"popup-border-lines ")?;
                        write_cstr(out, cause.as_c_str())
                    });
                    current_block = 1988999557336856620;
                }
            }
        } else {
            current_block = 8151474771948790331;
        }
        match current_block {
            1988999557336856620 => {}
            _ => {
                if args_has(&*args, 'T' as i32 as u_char) != 0 {
                    formatted_title = Some(format_single_from_target_cstring(
                        item_handle,
                        args_get(&*(args), 'T' as i32 as u_char)
                            .map_or(std::ptr::null(), |value| value.as_ptr()),
                    ));
                    title = formatted_title.as_deref().expect("formatted title was set");
                } else {
                    title = c"";
                }
                if args_has(&*args, 'N' as i32 as u_char) != 0 || modify == 0 {
                    flags = 0 as ::core::ffi::c_int;
                }
                if args_has(&*args, 'E' as i32 as u_char) > 1 as ::core::ffi::c_int {
                    if flags == -(1 as ::core::ffi::c_int) {
                        flags = 0 as ::core::ffi::c_int;
                    }
                    flags |= POPUP_CLOSEEXITZERO;
                } else if args_has(&*args, 'E' as i32 as u_char) != 0 {
                    if flags == -(1 as ::core::ffi::c_int) {
                        flags = 0 as ::core::ffi::c_int;
                    }
                    flags |= POPUP_CLOSEEXIT;
                }
                if args_has(&*args, 'k' as i32 as u_char) != 0 {
                    if flags == -(1 as ::core::ffi::c_int) {
                        flags = 0 as ::core::ffi::c_int;
                    }
                    flags |= POPUP_CLOSEANYKEY;
                }
                if modify != 0 {
                    popup_modify(
                        &tc.clone().expect("live client"),
                        Some(title),
                        style,
                        border_style,
                        lines,
                        flags,
                    );
                } else {
                    let mut retval = CmdOutcome::Normal;
                    let wait = || {
                        let (waiter, wait_retval) = cmdq_wait(item_handle);
                        retval = wait_retval;
                        waiter
                    };
                    if popup_display(
                        flags,
                        lines,
                        wait,
                        px,
                        py,
                        w,
                        h,
                        env.as_deref(),
                        shellcmd,
                        &argv_owner,
                        cwd,
                        Some(title),
                        &tc.clone().expect("live client"),
                        s.as_ref(),
                        style,
                        border_style,
                    ) == 0 as ::core::ffi::c_int
                    {
                        drop(env.take());
                        return retval;
                    }
                }
                current_block = 6589043366517631393;
            }
        }
    }
    match current_block {
        1988999557336856620 => {
            drop(env.take());
            CmdOutcome::Error
        }
        _ => {
            drop(env.take());
            CmdOutcome::Normal
        }
    }
}
