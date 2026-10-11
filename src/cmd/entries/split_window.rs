use crate::src::arguments::{
    args_count, args_flag_values, args_get, args_has, args_string, args_to_vector,
};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{
    cmdq_error, cmdq_get_state_owned, cmdq_get_target_client, cmdq_insert_hook, cmdq_print,
};
use crate::src::environ::{environ_create, environ_put};
use crate::src::events::events_fire;
use crate::src::events_payload::{
    event_payload_create, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_window,
};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{format_single_cstring, format_single_from_target_cstring};
use crate::src::options::{options_set_number, options_set_string};

use crate::src::server_client::Client as _;
use crate::src::server_fn::{server_redraw_session, server_redraw_window};
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::session::SessionRef;
use crate::src::shared::spawn::spawn_context;
use crate::src::shared::spawn::{SPAWN_BEFORE, SPAWN_DETACHED, SPAWN_EMPTY};
use crate::src::shared::window::winlink;
use crate::src::window::Window;

use crate::src::window_pane::WindowPane as _;

pub const SPLIT_WINDOW_TEMPLATE: &std::ffi::CStr = c"#{session_name}:#{window_index}.#{pane_index}";
pub static cmd_new_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"new-pane",
        alias: Some(c"newp"),
        args: args_parse {
            template: c"bc:de:EF:Ikm:PR:s:S:t:T:W",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-bdEIkPW] [-c start-directory] [-e environment] [-F format] [-m message] [-s style] [-S active-border-style] [-R inactive-border-style] [-T title] [-t target-pane] [shell-command [argument ...]]",
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
        exec: Some(cmd_split_window_exec),
    }
};
/// Historical alias for new-pane: the strip has no split direction or size,
/// so -f, -h, -l, -p and -v are accepted and ignored.
pub static cmd_split_window_entry: cmd_entry = {
    cmd_entry {
        name: c"split-window",
        alias: Some(c"splitw"),
        args: args_parse {
            template: c"bc:de:EfF:hIkl:m:p:PR:s:S:t:T:vW",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-bdEfhIkPvW] [-c start-directory] [-e environment] [-F format] [-l size] [-m message] [-p percentage] [-s style] [-S active-border-style] [-R inactive-border-style] [-T title] [-t target-pane] [shell-command [argument ...]]",
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
        exec: Some(cmd_split_window_exec),
    }
};
unsafe fn cmd_split_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let current = cmdq_get_state_owned(&*(item));
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut sc: spawn_context = spawn_context {
        item: std::rc::Weak::new(),
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        tc: std::rc::Weak::new(),
        wp0: std::rc::Weak::new(),
        name: None,
        argv: Vec::new(),
        environ: None,
        idx: 0,
        cwd: None,
        flags: 0,
    };
    let mut argv_owner = Vec::new();
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let original_window = wl
        .get_unchecked()
        .window_handle()
        .cloned()
        .expect("split target window");
    let result = (|| {
        let mut current_block: u64;
        let original_pane = (*target).pane_handle().expect("split target pane");
        let mut fs: cmd_find_state = cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        };
        let mut input: ::core::ffi::c_int = 0;
        let mut empty: ::core::ffi::c_int = 0;
        let mut flags: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
        let mut template: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut style: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
        let mut cause: Option<std::ffi::CString> = None;
        let mut count: u_int = args_count(args);
        if args_has(args, 'b' as i32 as u_char) != 0 {
            flags |= SPAWN_BEFORE;
        }
        if args_has(args, 'd' as i32 as u_char) != 0 {
            flags |= SPAWN_DETACHED;
        }
        input = args_has(args, 'I' as i32 as u_char);
        if input != 0
            || count == 1 as u_int
                && *args_string(&mut *(args), 0 as u_int)
                    .map_or(std::ptr::null(), |value| value.as_ptr())
                    as ::core::ffi::c_int
                    == '\0' as i32
        {
            empty = 1 as ::core::ffi::c_int;
        } else {
            empty = args_has(args, 'E' as i32 as u_char);
        }
        if empty != 0
            && count != 0 as u_int
            && (count != 1 as u_int
                || *args_string(&mut *(args), 0 as u_int)
                    .map_or(std::ptr::null(), |value| value.as_ptr())
                    as ::core::ffi::c_int
                    != '\0' as i32)
        {
            cmdq_error(item_handle, |out| {
                out.write_all(b"command cannot be given for empty pane")
            });
            return CMD_RETURN_ERROR;
        }
        if empty != 0 {
            flags |= SPAWN_EMPTY;
        }
        sc.item = std::rc::Rc::downgrade(item_handle);
        sc.s = std::rc::Rc::downgrade(s.as_ref().expect("live session"));
        sc.set_wl(wl.clone());
        sc.wp0 = std::rc::Rc::downgrade(&original_pane);
        argv_owner = args_to_vector(&*args);
        sc.argv = argv_owner;
        sc.environ = Some(environ_create());
        for av in args_flag_values(&*args, 'e' as i32 as u_char) {
            environ_put(
                sc.environ.as_deref_mut().expect("environment"),
                av.as_string().expect("string argument"),
                0 as ::core::ffi::c_int,
            );
        }
        sc.idx = -(1 as ::core::ffi::c_int);
        sc.cwd = args_get(&*(args), 'c' as i32 as u_char).map(|value| value.to_owned());
        sc.flags = flags;
        let window_owner = wl
            .get_unchecked()
            .window_handle()
            .expect("target window")
            .clone();
        let spawned_pane = match <std::rc::Rc<
            std::cell::UnsafeCell<crate::src::shared::pane::window_pane>,
        > as crate::src::window_pane::WindowPane>::spawn_process(
            &mut sc, &mut cause
        ) {
            Some(pane) => Some(pane),
            None => {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"create pane failed: ")?;
                    write_cstr(out, cause.as_deref())
                });
                drop(window_owner);
                return CMD_RETURN_ERROR;
            }
        };
        if spawned_pane.is_none() {
            cmdq_error(item_handle, |out| write_cstr(out, cause.as_deref()));
        } else {
            let new_pane = spawned_pane.as_ref().expect("spawned pane");
            style = args_get(&*(args), 's' as i32 as u_char)
                .map_or(std::ptr::null(), |value| value.as_ptr());
            if !style.is_null() {
                if new_pane.with_options_mut(|options| {
                    options_set_string(options, c"window-style", 0 as ::core::ffi::c_int, |out| {
                        write_cstr(out, nullable_cstr(style))
                    })
                    .is_null()
                }) {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"bad style: ")?;
                        write_cstr(out, nullable_cstr(style))
                    });
                    current_block = 9814746494299271243;
                } else {
                    new_pane.with_options_mut(|options| {
                        options_set_string(
                            options,
                            c"window-active-style",
                            0 as ::core::ffi::c_int,
                            |out| write_cstr(out, nullable_cstr(style)),
                        )
                        .is_null()
                    });
                    new_pane.invalidate_style();
                    current_block = 14329534724295951598;
                }
            } else {
                current_block = 14329534724295951598;
            }
            match current_block {
                9814746494299271243 => {}
                _ => {
                    style = args_get(&*(args), 'S' as i32 as u_char)
                        .map_or(std::ptr::null(), |value| value.as_ptr());
                    if !style.is_null() {
                        if new_pane.with_options_mut(|options| {
                            options_set_string(
                                options,
                                c"pane-active-border-style",
                                0 as ::core::ffi::c_int,
                                |out| write_cstr(out, nullable_cstr(style)),
                            )
                            .is_null()
                        }) {
                            cmdq_error(item_handle, |out| {
                                out.write_all(b"bad active border style: ")?;
                                write_cstr(out, nullable_cstr(style))
                            });
                            current_block = 9814746494299271243;
                        } else {
                            current_block = 12070711452894729854;
                        }
                    } else {
                        current_block = 12070711452894729854;
                    }
                    match current_block {
                        9814746494299271243 => {}
                        _ => {
                            style = args_get(&*(args), 'R' as i32 as u_char)
                                .map_or(std::ptr::null(), |value| value.as_ptr());
                            if !style.is_null() {
                                if new_pane.with_options_mut(|options| {
                                    options_set_string(
                                        options,
                                        c"pane-border-style",
                                        0 as ::core::ffi::c_int,
                                        |out| write_cstr(out, nullable_cstr(style)),
                                    )
                                    .is_null()
                                }) {
                                    cmdq_error(item_handle, |out| {
                                        out.write_all(b"bad inactive border style: ")?;
                                        write_cstr(out, nullable_cstr(style))
                                    });
                                    current_block = 9814746494299271243;
                                } else {
                                    current_block = 16313536926714486912;
                                }
                            } else {
                                current_block = 16313536926714486912;
                            }
                            match current_block {
                                9814746494299271243 => {}
                                _ => {
                                    if args_has(args, 'k' as i32 as u_char) != 0
                                        || args_has(args, 'm' as i32 as u_char) != 0
                                    {
                                        new_pane.with_options_mut(|options| {
                                            options_set_number(
                                                options,
                                                c"remain-on-exit",
                                                3 as ::core::ffi::c_longlong,
                                            )
                                        });
                                        if args_has(args, 'm' as i32 as u_char) != 0 {
                                            new_pane.with_options_mut(|options| {
                                                options_set_string(
                                                    options,
                                                    c"remain-on-exit-format",
                                                    0 as ::core::ffi::c_int,
                                                    |out| {
                                                        write_cstr(
                                                            out,
                                                            nullable_cstr(
                                                                args_get(
                                                                    &*(args),
                                                                    'm' as i32 as u_char,
                                                                )
                                                                .map_or(std::ptr::null(), |value| {
                                                                    value.as_ptr()
                                                                }),
                                                            ),
                                                        )
                                                    },
                                                )
                                                .is_null()
                                            });
                                        }
                                    }
                                    if args_has(args, 'T' as i32 as u_char) != 0 {
                                        let title = format_single_from_target_cstring(
                                            item_handle,
                                            args_get(&*(args), 'T' as i32 as u_char)
                                                .map_or(std::ptr::null(), |value| value.as_ptr()),
                                        );
                                        new_pane.set_title(&title);
                                        let mut ep = event_payload_create();
                                        fs = cmd_find_state::find_from_pane(
                                            new_pane,
                                            0 as ::core::ffi::c_int,
                                        )
                                        .unwrap_or_else(|| {
                                            cmd_find_state::clear_state(0 as ::core::ffi::c_int)
                                        });
                                        event_payload_set_target(&mut ep, &fs);
                                        event_payload_set_pane(
                                            &mut ep,
                                            c"pane",
                                            std::rc::Rc::clone(new_pane),
                                        );
                                        event_payload_set_window(
                                            &mut ep,
                                            c"window",
                                            new_pane
                                                .window_observer()
                                                .upgrade()
                                                .expect("spawned pane window"),
                                        );
                                        event_payload_set_string(&mut ep, c"new_title", |out| {
                                            write_cstr(out, &*title)
                                        });
                                        events_fire(c"pane-title-changed", ep);
                                    }
                                    if input != 0 {
                                        match new_pane.start_input(item_handle) {
                                            Err(error) => {
                                                cmdq_error(item_handle, |out| {
                                                    write_cstr(out, &*error)
                                                });
                                                current_block = 9814746494299271243;
                                            }
                                            Ok(1) => {
                                                input = 0;
                                                current_block = 12543410360505780601;
                                            }
                                            Ok(_) => current_block = 12543410360505780601,
                                        }
                                    } else {
                                        current_block = 12543410360505780601;
                                    }
                                    match current_block {
                                        9814746494299271243 => {}
                                        _ => {
                                            if !flags & SPAWN_DETACHED != 0 {
                                                *current.current.borrow_mut() =
                                                    cmd_find_state::find_from_winlink_pane(
                                                        wl.clone(),
                                                        new_pane,
                                                        0 as ::core::ffi::c_int,
                                                    );
                                            }
                                            server_redraw_window(&original_window);
                                            server_redraw_session(
                                                s.as_ref().expect("live session"),
                                            );
                                            if args_has(args, 'P' as i32 as u_char) != 0 {
                                                template = args_get(&*(args), 'F' as i32 as u_char)
                                                    .map_or(std::ptr::null(), |value| {
                                                        value.as_ptr()
                                                    });
                                                if template.is_null() {
                                                    template = SPLIT_WINDOW_TEMPLATE.as_ptr();
                                                }
                                                let cp = format_single_cstring(
                                                    Some(item_handle),
                                                    template,
                                                    tc.as_ref(),
                                                    s.as_ref(),
                                                    wl.clone(),
                                                    Some(new_pane),
                                                );
                                                cmdq_print(item_handle, |out| {
                                                    write_cstr(out, &*cp)
                                                });
                                            }
                                            fs = cmd_find_state::find_from_winlink_pane(
                                                wl.clone(),
                                                new_pane,
                                                0 as ::core::ffi::c_int,
                                            );
                                            cmdq_insert_hook(
                                                s.as_ref(),
                                                item_handle,
                                                &raw mut fs,
                                                |out| out.write_all(b"after-split-window"),
                                            );
                                            drop(sc.environ.take());
                                            if input != 0 {
                                                drop(window_owner);
                                                return CMD_RETURN_WAIT;
                                            }
                                            if args_has(args, 'W' as i32 as u_char) != 0 {
                                                new_pane.wait_until_close(item_handle);
                                                drop(window_owner);
                                                return CMD_RETURN_WAIT;
                                            }
                                            drop(window_owner);
                                            return CMD_RETURN_NORMAL;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(new_pane) = spawned_pane.as_ref() {
            ClientRef::forget_pane(new_pane);
            original_window.remove_pane(new_pane);
        }
        drop(sc.environ.take());
        drop(window_owner);
        CMD_RETURN_ERROR
    })();
    drop(original_window);
    result
}
