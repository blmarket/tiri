use crate::src::arguments::{args_get, args_has};
use crate::src::cmd::queue::{cmdq_error, cmdq_get_state_owned, cmdq_insert_hook, cmdq_print};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::events::events_fire;
use crate::src::events_payload::{
    event_payload_create, event_payload_set_int, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_window,
};
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::options::{options_get_string, options_set_string};
use crate::src::server::{
    marked_pane, server_check_marked, server_clear_marked, server_is_marked, server_set_marked,
};
use crate::src::server_client::Client as _;
use crate::src::server_client::ClientRegistry;
use crate::src::server_fn::{
    server_redraw_client, server_redraw_window_borders, server_status_window,
};
use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::layout::Direction;
use crate::src::shared::window::WindowRef;
use crate::src::tty::tty_window_bigger;
use crate::src::window::Window as _;

use crate::src::window_pane::WindowPane as _;

use crate::src::shared::abi::*;
use crate::src::shared::arguments::args_parse;
use crate::src::shared::client::{CLIENT_CONTROL, CLIENT_REDRAWBORDERS, CLIENT_REDRAWSTATUS};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
pub static cmd_select_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"select-pane",
        alias: Some(c"selectp"),
        args: args_parse {
            template: c"DdegLlMmP:RT:t:U",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-DdeLlMmRU] [-T title] [-t target-pane]",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_select_pane_exec),
    }
};
pub static cmd_last_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"last-pane",
        alias: Some(c"lastp"),
        args: args_parse {
            template: c"det:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-de] [-t target-window]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_select_pane_exec),
    }
};
unsafe fn cmd_select_pane_redraw(w_owner: &WindowRef) {
    let mut c: Option<ClientRef> = None;
    let mut registry_c_owner = ClientRegistry::first();
    c = registry_c_owner.clone();
    while !c.is_none() {
        if !(c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
            || c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t != 0)
        {
            if (c
                .as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .expect("live session")
                .current_winlink())
            .get_unchecked()
            .window_handle()
            .is_some_and(|owner| WindowRef::ptr_eq(owner, w_owner))
                && tty_window_bigger(c.as_ref().expect("live client")) != 0
            {
                server_redraw_client(c.as_ref().expect("live client"));
            } else {
                if (c
                    .as_ref()
                    .expect("live client")
                    .attached_session()
                    .upgrade()
                    .expect("live session")
                    .current_winlink())
                .get_unchecked()
                .window_handle()
                .is_some_and(|owner| WindowRef::ptr_eq(owner, w_owner))
                {
                    c.as_ref()
                        .expect("live client")
                        .update_flags(CLIENT_REDRAWBORDERS as uint64_t, 0);
                }
                if (c
                    .as_ref()
                    .expect("live client")
                    .attached_session()
                    .upgrade()
                    .expect("live session")
                    .contains_window(w_owner) as i32)
                    != 0
                {
                    c.as_ref()
                        .expect("live client")
                        .update_flags(CLIENT_REDRAWSTATUS as uint64_t, 0);
                }
            }
        }
        registry_c_owner =
            ClientRegistry::next(registry_c_owner.as_ref().expect("current registry client"));
        c = registry_c_owner.clone();
    }
}
unsafe fn cmd_select_pane_marked_pane(
    mut command: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let arguments = cmd_get_args_mut(command.get_mut_unchecked()).expect("select pane arguments");
    let target = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let pane = target.pane_handle().expect("marked target pane");
    let link = target.winlink_handle();
    let session = target.session_handle();
    let previous = if server_check_marked() != 0 {
        marked_pane.pane_handle()
    } else {
        None
    };
    if args_has(arguments, b'M') != 0
        || server_is_marked(session.as_ref(), link.clone(), Some(&pane)) != 0
    {
        server_clear_marked();
    } else {
        server_set_marked(session.as_ref(), link, Some(&pane));
    }
    let marked = marked_pane.pane_handle();
    let mut find = cmd_find_state::default();
    let payload_pane = marked.as_ref().or(previous.as_ref()).unwrap_or(&pane);
    find = cmd_find_state::find_from_pane(payload_pane, 0)
        .unwrap_or_else(|| cmd_find_state::clear_state(0));
    let mut payload = event_payload_create();
    event_payload_set_target(&mut payload, &find);
    event_payload_set_pane(&mut payload, c"pane", payload_pane.clone());
    if let Some(marked) = marked.as_ref() {
        event_payload_set_pane(&mut payload, c"new_pane", marked.clone());
    }
    event_payload_set_window(
        &mut payload,
        c"window",
        payload_pane
            .window_observer()
            .upgrade()
            .expect("marked pane window"),
    );
    if let Some(previous) = previous.as_ref() {
        event_payload_set_pane(&mut payload, c"old_pane", previous.clone());
    }
    event_payload_set_int(&mut payload, c"marked", marked.is_some() as i32);
    events_fire(c"marked-pane-changed", payload);
    for changed in [previous.as_ref(), marked.as_ref()].into_iter().flatten() {
        changed.invalidate_style();
        let window = changed
            .window_observer()
            .upgrade()
            .expect("marked pane window");
        server_redraw_window_borders(&window);
        server_status_window(&window);
        drop(window);
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_select_pane_exec(
    mut command: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let entry = cmd_get_entry(command.get_unchecked());
    let marked_command = command.clone();
    let arguments = cmd_get_args_mut(command.get_mut_unchecked()).expect("select pane arguments");
    let current = cmdq_get_state_owned(&*item);
    let target = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let link = target.winlink_handle();
    let session = target.session_handle();
    let original_pane = target.pane_handle().expect("select target pane");
    let window = target.w.upgrade().expect("select target window");
    let result = (|| {
        if std::ptr::eq(entry, &cmd_last_pane_entry) || args_has(arguments, b'l') != 0 {
            let mut last = window.last_active_pane().upgrade();
            if last.is_none() && window.pane_snapshot().len() == 2 {
                let active = window.active_pane();
                last = window
                    .step_pane(Some(&active), true)
                    .upgrade()
                    .or_else(|| window.step_pane(Some(&active), false).upgrade());
            }
            let Some(last) = last else {
                cmdq_error(item_handle, |out| out.write_all(b"no last pane"));
                return CMD_RETURN_ERROR;
            };
            if args_has(arguments, b'e') != 0 || args_has(arguments, b'd') != 0 {
                last.set_input_enabled(args_has(arguments, b'e') != 0);
                server_redraw_window_borders(&window);
                server_status_window(&window);
            } else {
                window.redraw_active_switch(Some(&last));
                if window.select_pane(&last, true) != 0 {
                    *current.current.borrow_mut() =
                        cmd_find_state::find_from_winlink(link.clone(), 0);
                    cmd_select_pane_redraw(&window);
                }
            }
            return CMD_RETURN_NORMAL;
        }
        if args_has(arguments, b'm') != 0 || args_has(arguments, b'M') != 0 {
            return cmd_select_pane_marked_pane(marked_command.clone(), item_handle);
        }
        if let Some(style) = args_get(arguments, b'P') {
            let invalid = original_pane.with_options_mut(|options| {
                options_set_string(options, c"window-style", 0, |out| write_cstr(out, &*style))
                    .is_null()
            });
            if invalid {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"bad style: ")?;
                    write_cstr(out, &*style)
                });
                return CMD_RETURN_ERROR;
            }
            original_pane.with_options_mut(|options| {
                options_set_string(options, c"window-active-style", 0, |out| {
                    write_cstr(out, &*style)
                })
                .is_null()
            });
            original_pane.invalidate_style();
        }
        if args_has(arguments, b'g') != 0 {
            let style = original_pane
                .with_options_mut(|options| options_get_string(options, c"window-style"));
            cmdq_print(item_handle, |out| write_cstr(out, &*style));
            return CMD_RETURN_NORMAL;
        }
        let direction = [
            (b'L', Direction::Left),
            (b'R', Direction::Right),
            (b'U', Direction::Up),
            (b'D', Direction::Down),
        ]
        .into_iter()
        .find(|&(flag, _)| args_has(arguments, flag) != 0)
        .map(|(_, direction)| direction);
        let pane = if let Some(direction) = direction {
            let Some(selected) = window
                .pane_in_direction(&original_pane, direction)
                .upgrade()
            else {
                return CMD_RETURN_NORMAL;
            };
            selected
        } else {
            original_pane.clone()
        };
        if args_has(arguments, b'e') != 0 || args_has(arguments, b'd') != 0 {
            pane.set_input_enabled(args_has(arguments, b'e') != 0);
            server_redraw_window_borders(&window);
            server_status_window(&window);
            return CMD_RETURN_NORMAL;
        }
        if args_has(arguments, b'T') != 0 {
            let title = format_single_from_target_cstring(
                item_handle,
                args_get(arguments, b'T').expect("pane title").as_ptr(),
            );
            if pane.set_title(&title) {
                let mut find = cmd_find_state::default();
                find = cmd_find_state::find_from_pane(&pane, 0)
                    .unwrap_or_else(|| cmd_find_state::clear_state(0));
                let mut payload = event_payload_create();
                event_payload_set_target(&mut payload, &find);
                event_payload_set_pane(&mut payload, c"pane", pane.clone());
                event_payload_set_window(&mut payload, c"window", window.clone());
                event_payload_set_string(&mut payload, c"new_title", |out| {
                    write_cstr(out, &*title)
                });
                events_fire(c"pane-title-changed", payload);
                server_redraw_window_borders(&window);
                server_status_window(&window);
            }
            return CMD_RETURN_NORMAL;
        }
        if window.active_pane().ptr_eq(&std::rc::Rc::downgrade(&pane)) {
            return CMD_RETURN_NORMAL;
        }
        window.redraw_active_switch(Some(&pane));
        if window.select_pane(&pane, true) != 0 {
            *current.current.borrow_mut() =
                cmd_find_state::find_from_winlink_pane(link.clone(), &pane, 0);
        }
        cmdq_insert_hook(
            session.as_ref(),
            item_handle,
            &mut current.current_snapshot(),
            |out| out.write_all(b"after-select-pane"),
        );
        cmd_select_pane_redraw(&window);
        CMD_RETURN_NORMAL
    })();
    drop(window);
    result
}
