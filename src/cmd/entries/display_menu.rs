use crate::arguments::{
    args, args_count, args_flag_values, args_get, args_has, args_parse, args_parse_type,
    args_percentage_result, args_string, args_strtonum_result, args_to_vector, ArgsParseError,
    ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING,
};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_event, cmdq_get_target_client};
use crate::environ::{environ_create, environ_put};
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create_from_target, format_expand_cstring, format_single_from_target_cstring,
};
use crate::key_string::key_string_parse_cstr;
use crate::menu::{menu_add_item, menu_create, menu_display};
use crate::options::{options_find_choice, options_get_number};
use crate::text::numbers::strtol_prefix;
use std::ffi::CStr;

use crate::server_client::Client as _;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_CONTROL;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CFLAG};
use crate::shared::key::*;
use crate::shared::layout::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::menu::{menu_item, MENU_NOMOUSE, MENU_STAYOPEN};
use crate::shared::style::*;
use crate::status::{status_at_line, status_line_size};
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
            flag: b't',
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
            template: c"Bb:Cc:d:e:Eh:ks:S:t:T:w:x:y:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-BCEk] [-b border-lines] [-c target-client] [-d start-directory] [-e environment] [-h height] [-s style] [-S border-style] [-t target-pane] [-T title] [-w width] [-x position] [-y position] [shell-command [argument ...]]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK | CMD_CLIENT_CFLAG,
        exec: Some(cmd_display_popup_exec),
    }
};
fn cmd_display_menu_args_parse(args: &args, idx: u_int) -> Result<args_parse_type, ArgsParseError> {
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
fn cmd_display_menu_status_position(
    client: &ClientRef,
    lines: u_int,
    window_index: u_int,
) -> Option<(u_int, u_int)> {
    let status = client.borrow_status();
    status.entries[..lines as usize]
        .iter()
        .enumerate()
        .find_map(|(line, entry)| {
            entry.ranges.iter().find_map(|range| {
                (range.type_0 == STYLE_RANGE_WINDOW && range.argument == window_index)
                    .then_some((line as u_int, range.start))
            })
        })
}

fn cmd_display_menu_get_menu_pos(
    client: &ClientRef,
    item: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    args: &args,
    px: &mut u_int,
    py: &mut u_int,
    width: u_int,
    height: u_int,
) -> i32 {
    let target = crate::cmd::queue::cmdq_get_target(&item.borrow()).clone();
    let event = cmdq_get_event(&item.borrow()).metadata_snapshot();
    let window = if event.m.valid != 0 {
        client
            .attached_session()
            .upgrade()
            .and_then(|session| crate::winlink::winlink_window(&session.current_winlink()))
    } else {
        target.window_handle()
    }
    .expect("menu window");
    let (sx, sy) = window.size();
    let pane = target.pane_handle().expect("menu target pane");
    let (pane_width, pane_height, pane_x, pane_y) = pane.geometry();
    let view = client.terminal_view();
    let ox = view.ox as i64;
    let oy = view.oy as i64;
    let mouse_x = event.m.x as i64 + ox;
    let status_at = status_at_line(client);
    let lines = status_line_size(client);
    let mouse_y = if status_at == 0 {
        (event.m.y as i64 - lines as i64).max(0) + oy
    } else if status_at > 0 && event.m.y >= status_at as u32 {
        oy + view.sy.saturating_sub(1) as i64
    } else {
        event.m.y as i64 + oy
    };
    let mut format = format_create_from_target(item);
    let (last_x, last_y) = window.last_menu_position();
    for (name, value) in [
        (c"popup_mouse_x", mouse_x),
        (c"popup_mouse_y", mouse_y),
        (c"popup_last_x", last_x as i64),
        (c"popup_last_y", last_y as i64 + height as i64),
        (c"popup_width", width as i64),
        (c"popup_height", height as i64),
        (
            c"popup_centre_x",
            ((sx.saturating_sub(1) / 2) as i64 - (width / 2) as i64).max(0),
        ),
        (
            c"popup_centre_y",
            (sy.saturating_sub(1) / 2 + height / 2) as i64,
        ),
        (
            c"popup_mouse_centre_x",
            (mouse_x - (width / 2) as i64).max(0),
        ),
        (
            c"popup_mouse_centre_y",
            (mouse_y + (height / 2) as i64).min(sy.saturating_sub(height) as i64),
        ),
        (
            c"popup_mouse_top",
            (mouse_y + height as i64).min(sy.saturating_sub(height) as i64),
        ),
        (c"popup_mouse_bottom", (mouse_y - height as i64).max(0)),
        (
            c"popup_pane_top",
            (pane_y as i64 + height as i64).min(sy.saturating_sub(height) as i64),
        ),
        (c"popup_pane_bottom", pane_y as i64 + pane_height as i64),
        (c"popup_pane_left", pane_x as i64),
        (
            c"popup_pane_right",
            (pane_x as i64 + pane_width as i64 - width as i64).max(0),
        ),
    ] {
        format_add(&mut format, name, |out| write!(out, "{value}"));
    }
    if status_at != -1 && lines != 0 {
        let session = client.attached_session().upgrade().expect("menu session");
        let link = target.winlink_handle();
        let index = link.try_borrow().expect("menu winlink").idx as u32;
        let top = options_get_number(&session.borrow_options_mut(), c"status-position") == 0;
        let y = if top { height } else { sy };
        if let Some((_, start)) = cmd_display_menu_status_position(client, lines, index) {
            format_add(&mut format, c"popup_window_status_line_x", |out| {
                write!(out, "{}", start as i64 + ox)
            });
            format_add(&mut format, c"popup_window_status_line_y", |out| {
                write!(out, "{y}")
            });
        }
        format_add(&mut format, c"popup_status_line_y", |out| {
            write!(out, "{y}")
        });
    }
    let x = args_get(args, b'x').unwrap_or(c"C");
    let x = match x.to_bytes() {
        b"C" => c"#{popup_centre_x}",
        b"R" => c"#{popup_pane_right}",
        b"P" => c"#{popup_pane_left}",
        b"M" => c"#{popup_mouse_centre_x}",
        b"L" => c"#{popup_last_x}",
        b"W" => c"#{popup_window_status_line_x}",
        _ => x,
    };
    let x = strtol_prefix(format_expand_cstring(&mut format, x).as_bytes(), 10).0;
    *px = x.max(0).min(sx.saturating_sub(width) as _) as u32;
    let y = args_get(args, b'y').unwrap_or(c"C");
    let y = match y.to_bytes() {
        b"C" => c"#{popup_centre_y}",
        b"P" => c"#{popup_pane_bottom}",
        b"M" => c"#{popup_mouse_top}",
        b"L" => c"#{popup_last_y}",
        b"S" => c"#{popup_status_line_y}",
        b"W" => c"#{popup_window_status_line_y}",
        _ => y,
    };
    let y = strtol_prefix(format_expand_cstring(&mut format, y).as_bytes(), 10).0;
    *py = y
        .saturating_sub(height as _)
        .max(0)
        .min(sy.saturating_sub(height) as _) as u32;
    1
}
fn cmd_display_menu_exec(
    self_0: &cmd,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let target = &target_state;
    let event = cmdq_get_event(&(item));
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let style = args_get(args, b's');
    let border_style = args_get(args, b'S');
    let selected_style = args_get(args, b'H');
    let mut starting_choice = 0;
    if args_has(args, b'C') != 0 {
        if args_get(args, b'C').expect("argument is present") == c"-" {
            starting_choice = -1;
        } else {
            match args_strtonum_result(args, b'C', 0, UINT_MAX as i64) {
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
    let title = if args_has(args, b'T') != 0 {
        Some(format_single_from_target_cstring(
            item_handle,
            args_get(args, b'T').unwrap_or(c""),
        ))
    } else {
        None
    };
    let mut menu = menu_create(title.as_deref().unwrap_or(c""));
    drop(title);
    let count = args_count(args);
    let mut i = 0;
    while i != count {
        let name = args_string(args, i)
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
        let key = key_string_parse_cstr(args_string(args, i).expect("argument is present"))
            .unwrap_or(KEYC_UNKNOWN);
        let command = args_string(args, i + 1).expect("argument is present");
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
            args,
            &mut px,
            &mut py,
            menu.width.wrapping_add(4),
            menu.count().wrapping_add(2),
        ) == 0
    {
        return CmdOutcome::Normal;
    }
    let mut lines = BOX_LINES_DEFAULT;
    if let Some(value) = args_get(args, b'b') {
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
    if args_has(args, b'O') != 0 {
        flags |= MENU_STAYOPEN;
    }
    if event.m.valid == 0 && args_has(args, b'M') == 0 {
        flags |= MENU_NOMOUSE;
    }
    menu_display(
        menu,
        flags,
        starting_choice,
        Some(&event),
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
fn cmd_display_popup_exec(
    command: &cmd,
    item: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    use crate::cmd::queue::cmdq_wait;
    use crate::options::{options_set_number, options_set_string};
    use crate::shared::pane::{window_pane, PaneWaitFor};
    use crate::shared::spawn::{spawn_context, SPAWN_MODAL};
    use std::rc::Rc;

    let args = cmd_get_args(command);
    let target = crate::cmd::queue::cmdq_get_target(&item.borrow()).clone();
    let client = cmdq_get_target_client(Some(&item.borrow())).expect("popup target client");
    let session = target.session_handle().expect("popup session");
    let window = target.window_handle().expect("popup window");
    if args_has(args, b'C') != 0 {
        if let Some(pane) = window.modal_pane().upgrade() {
            pane.kill_process();
        }
        return CmdOutcome::Normal;
    }
    if client.flags() & CLIENT_CONTROL as u64 != 0 || window.modal_pane().upgrade().is_some() {
        return CmdOutcome::Normal;
    }
    for flag in [b's', b'S'] {
        if let Some(value) = args_get(args, flag) {
            let mut style = crate::shared::style::style::default();
            crate::style::style_set(&mut style, &crate::grid::grid_default_cell);
            if crate::style::style_parse(&mut style, &crate::grid::grid_default_cell, value) != 0 {
                cmdq_error(item, |out| {
                    out.write_all(b"bad style: ")?;
                    write_cstr(out, value)
                });
                return CmdOutcome::Error;
            }
        }
    }
    let lines = if args_has(args, b'B') != 0 {
        PANE_LINES_NONE
    } else {
        let value = match args_get(args, b'b').unwrap_or(c"single").to_bytes() {
            b"rounded" => c"single",
            b"padded" => c"spaces",
            _ => args_get(args, b'b').unwrap_or(c"single"),
        };
        let entry = crate::options::options_table
            .iter()
            .find(|entry| entry.name == Some(c"pane-border-lines"))
            .expect("pane border option");
        match options_find_choice(entry, value) {
            Ok(value) => value as pane_lines,
            Err(error) => {
                cmdq_error(item, |out| {
                    out.write_all(b"pane-border-lines ")?;
                    write_cstr(out, error.as_c_str())
                });
                return CmdOutcome::Error;
            }
        }
    };
    let (sx, sy) = window.size();
    let mut dimensions = [sx / 2, sy / 2];
    for (index, flag, limit, label) in [
        (0, b'w', sx, b"width ".as_slice()),
        (1, b'h', sy, b"height ".as_slice()),
    ] {
        if args_has(args, flag) != 0 {
            match args_percentage_result(args, flag, 1, limit as i64, limit as i64) {
                Ok(value) => dimensions[index] = value as u32,
                Err(error) => {
                    cmdq_error(item, |out| {
                        out.write_all(label)?;
                        write_cstr(out, error.message())
                    });
                    return CmdOutcome::Error;
                }
            }
        }
    }
    let [width, height] = dimensions;
    if width < 1 || height < 1 || lines != PANE_LINES_NONE && (width < 3 || height < 3) {
        return CmdOutcome::Normal;
    }
    let mut x = 0;
    let mut y = 0;
    cmd_display_menu_get_menu_pos(&client, item, args, &mut x, &mut y, width, height);
    let mut environment = environ_create();
    for value in args_flag_values(args, b'e') {
        environ_put(
            &mut environment,
            value.as_string().expect("environment argument"),
            0,
        );
    }
    let source = target.pane_handle().expect("popup source pane");
    let mut context = spawn_context {
        geometry: Some(layout_geometry {
            sx: width,
            sy: height,
            xoff: x as i32,
            yoff: y as i32,
        }),
        item: Rc::downgrade(item),
        s: Rc::downgrade(&session),
        wl: target.winlink_handle(),
        tc: Rc::downgrade(&client),
        wp0: Rc::downgrade(&source),
        name: None,
        argv: if args_count(args) == 1 && args_string(args, 0).is_some_and(CStr::is_empty) {
            Vec::new()
        } else {
            args_to_vector(args)
        },
        environ: Some(environment),
        idx: -1,
        cwd: args_get(args, b'd').map(CStr::to_owned),
        flags: SPAWN_MODAL,
    };
    let pane =
        match <Rc<std::cell::RefCell<window_pane>> as crate::window_pane::WindowPane>::spawn_process(
            &mut context,
        ) {
            Ok(pane) => pane,
            Err(error) => {
                cmdq_error(item, |out| {
                    out.write_all(b"create pane failed: ")?;
                    write_cstr(out, error.as_c_str())
                });
                return CmdOutcome::Error;
            }
        };
    pane.set_modal(args_has(args, b'E') == 0);
    {
        let mut options = pane.borrow_options_mut();
        options_set_number(&mut options, c"pane-border-lines", lines as i64);
        let remain = match (args_has(args, b'E'), args_has(args, b'k') != 0) {
            (2.., true) => 4,
            (2.., false) => 2,
            (1, _) => 0,
            (_, true) => 3,
            _ => 1,
        };
        options_set_number(&mut options, c"remain-on-exit", remain);
        options_set_string(&mut options, c"remain-on-exit-format", 0, |_| Ok(()));
        for (flag, names) in [
            (b's', [c"window-style", c"window-active-style"]),
            (b'S', [c"pane-border-style", c"pane-active-border-style"]),
        ] {
            if let Some(style) = args_get(args, flag) {
                for name in names {
                    options_set_string(&mut options, name, 0, |out| write_cstr(out, style));
                }
            }
        }
        if args_has(args, b'T') != 0 {
            options_set_number(
                &mut options,
                c"pane-border-status",
                crate::shared::pane::PANE_STATUS_TOP as i64,
            );
            options_set_string(&mut options, c"pane-border-format", 0, |out| {
                out.write_all(b"#{pane_title}")
            });
        }
    }
    if let Some(title) = args_get(args, b'T') {
        use crate::events_payload::{
            event_payload_create, event_payload_set_pane, event_payload_set_string,
            event_payload_set_target, event_payload_set_window,
        };
        let title = format_single_from_target_cstring(item, title);
        pane.set_title(&title);
        let mut event = event_payload_create();
        let target = cmd_find_state::find_from_winlink_pane(context.winlink_handle(), &pane, 0);
        event_payload_set_target(&mut event, &target);
        event_payload_set_pane(&mut event, c"pane", pane.clone());
        event_payload_set_window(&mut event, c"window", window.clone());
        event_payload_set_string(&mut event, c"new_title", |out| {
            write_cstr(out, title.as_c_str())
        });
        crate::events::events_fire(c"pane-title-changed", event);
    }
    pane.invalidate_style();
    window.refit();
    crate::server_fn::server_redraw_session(&session);
    let (waiter, outcome) = cmdq_wait(item);
    pane.wait_for(PaneWaitFor::Closed, waiter);
    outcome
}
