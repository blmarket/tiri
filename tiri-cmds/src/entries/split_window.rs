use crate::cmd::cmd_get_args;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::future::Future;
use std::pin::Pin;
use tiri_core::arguments::{
    args_count, args_flag_values, args_get, args_has, args_parse, args_string, args_to_vector,
};
use tiri_core::environ::{environ_create, environ_put};
use tiri_core::events::events_fire;
use tiri_core::events_payload::{
    event_payload_create, event_payload_set_pane, event_payload_set_string,
    event_payload_set_target, event_payload_set_window,
};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::format_single_cstring;
use tiri_core::options::{options_set_number, options_set_string};
use tiri_core::server_client::Client as _;
use tiri_core::server_fn::{server_redraw_session, server_redraw_window};
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::pane::PaneWaitFor;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::spawn::spawn_context;
use tiri_core::shared::spawn::{SPAWN_BEFORE, SPAWN_DETACHED, SPAWN_EMPTY};
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::shared::window::winlink;
use tiri_core::window::Window;
use tiri_core::window_pane::WindowPane as _;
use tiri_core::winlink::winlink_window;

pub const SPLIT_WINDOW_TEMPLATE: &std::ffi::CStr = c"#{session_name}:#{window_index}.#{pane_index}";

pub static cmd_new_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"new-pane",
        alias: Some(c"newp"),
        args: args_parse {
            template: c"bc:de:EF:Ikm:PR:s:S:t:T:WZ",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-bdEIkPWZ] [-c start-directory] [-e environment] [-F format] [-m message] [-s style] [-S active-border-style] [-R inactive-border-style] [-T title] [-t target-pane] [shell-command [argument ...]]",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_split_window_exec),
    }
};

pub static cmd_split_window_entry: cmd_entry = {
    cmd_entry {
        name: c"split-window",
        alias: Some(c"splitw"),
        args: args_parse {
            template: c"bc:de:EfF:hIkl:m:p:PR:s:S:t:T:vWZ",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-bdEfhIkPvWZ] [-c start-directory] [-e environment] [-F format] [-l size] [-m message] [-p percentage] [-s style] [-S active-border-style] [-R inactive-border-style] [-T title] [-t target-pane] [shell-command [argument ...]]",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_split_window_exec),
    }
};

fn cmd_split_window_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let args = cmd_get_args(self_0);
        let mut sc: spawn_context = spawn_context {
            geometry: None,
            format: None,
            target_session: std::rc::Weak::new(),
            s: std::rc::Weak::new(),
            wl: tiri_core::refbox::Weak::new(),
            tc: std::rc::Weak::new(),
            wp0: std::rc::Weak::new(),
            name: None,
            argv: Vec::new(),
            environ: None,
            idx: 0,
            cwd: None,
            flags: 0,
        };
        let tc_owner = inv.target_client();
        let tc: Option<ClientRef> = tc_owner.clone();
        let s: Option<SessionRef> = inv.target().session_handle();
        let wl: tiri_core::refbox::Weak<winlink> = inv.target().winlink_handle();
        let original_window = winlink_window(&wl).expect("split target window");
        let original_pane = inv.target().pane_handle().expect("split target pane");
        original_window.unzoom(true);
        let mut fs: cmd_find_state;
        let input: ::core::ffi::c_int = args_has(args, 'I' as i32 as u_char);
        let count: u_int = args_count(args);
        let mut flags: ::core::ffi::c_int = 0;
        if args_has(args, 'b' as i32 as u_char) != 0 {
            flags |= SPAWN_BEFORE;
        }
        if args_has(args, 'd' as i32 as u_char) != 0 {
            flags |= SPAWN_DETACHED;
        }
        let empty = if input != 0
            || count == 1 as u_int
                && args_string(args, 0 as u_int)
                    .expect("argument is present")
                    .is_empty()
        {
            1 as ::core::ffi::c_int
        } else {
            args_has(args, 'E' as i32 as u_char)
        };
        if empty != 0
            && count != 0 as u_int
            && (count != 1 as u_int
                || !args_string(args, 0 as u_int)
                    .expect("argument is present")
                    .is_empty())
        {
            inv.error(|out| out.write_all(b"command cannot be given for empty pane"));
            return ExecOutcome::err();
        }
        if empty != 0 {
            flags |= SPAWN_EMPTY;
        }
        let cf = inv.format();
        sc.format = Some(cf.clone());
        sc.target_session = inv.target().s.clone();
        sc.s = std::rc::Rc::downgrade(s.as_ref().expect("live session"));
        sc.set_wl(wl.clone());
        sc.wp0 = std::rc::Rc::downgrade(&original_pane);
        drop(original_pane);
        sc.argv = args_to_vector(args);
        sc.environ = Some(environ_create());
        for av in args_flag_values(args, 'e' as i32 as u_char) {
            environ_put(
                sc.environ.as_deref_mut().expect("environment"),
                av.as_string().expect("string argument"),
                0 as ::core::ffi::c_int,
            );
        }
        sc.idx = -(1 as ::core::ffi::c_int);
        sc.cwd = args_get(args, 'c' as i32 as u_char).map(|value| value.to_owned());
        sc.flags = flags;
        let window_owner = winlink_window(&wl).expect("target window").clone();
        window_owner.push_zoom(true, args_has(args, 'Z' as i32 as u_char) != 0);
        let new_pane = match <std::rc::Rc<
            std::cell::RefCell<tiri_core::shared::pane::window_pane>,
        > as tiri_core::window_pane::WindowPane>::spawn_process(
            &mut sc
        ) {
            Ok(pane) => pane,
            Err(cause) => {
                inv.error(|out| {
                    out.write_all(b"create pane failed: ")?;
                    write_cstr(out, cause.as_c_str())
                });
                window_owner.pop_zoom();
                drop(sc.environ.take());
                return ExecOutcome::err();
            }
        };
        if let Some(style) = args_get(args, b's') {
            {
                let mut options = new_pane.borrow_options_mut();
                options_set_string(&mut options, c"window-style", 0, |out| {
                    write_cstr(out, style)
                });
                options_set_string(&mut options, c"window-active-style", 0, |out| {
                    write_cstr(out, style)
                });
            }
            new_pane.invalidate_style();
        }
        if let Some(style) = args_get(args, b'S') {
            options_set_string(
                &mut new_pane.borrow_options_mut(),
                c"pane-active-border-style",
                0,
                |out| write_cstr(out, style),
            );
        }
        if let Some(style) = args_get(args, b'R') {
            options_set_string(
                &mut new_pane.borrow_options_mut(),
                c"pane-border-style",
                0,
                |out| write_cstr(out, style),
            );
        }
        if args_has(args, 'k' as i32 as u_char) != 0
            || args_has(args, 'm' as i32 as u_char) != 0
        {
            options_set_number(
                &mut new_pane.borrow_options_mut(),
                c"remain-on-exit",
                3 as ::core::ffi::c_longlong,
            );
            if args_has(args, 'm' as i32 as u_char) != 0 {
                options_set_string(
                    &mut new_pane.borrow_options_mut(),
                    c"remain-on-exit-format",
                    0 as ::core::ffi::c_int,
                    |out| write_cstr(out, args_get(args, 'm' as i32 as u_char)),
                );
            }
        }
        if args_has(args, 'T' as i32 as u_char) != 0 {
            let title = inv.format_single(args_get(args, 'T' as i32 as u_char).unwrap_or(c""));
            new_pane.set_title(&title);
            let mut ep = event_payload_create();
            fs = cmd_find_state::from_pane(&new_pane, 0 as ::core::ffi::c_int)
                .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
            event_payload_set_target(&mut ep, &fs);
            event_payload_set_pane(&mut ep, c"pane", std::rc::Rc::clone(&new_pane));
            event_payload_set_window(
                &mut ep,
                c"window",
                new_pane
                    .window_observer()
                    .upgrade()
                    .expect("spawned pane window"),
            );
            event_payload_set_string(&mut ep, c"new_title", |out| write_cstr(out, &*title));
            events_fire(c"pane-title-changed", ep);
        }
        let mut pane_input = None;
        if input != 0 {
            let queue_client = inv.client();
            match new_pane.start_input(queue_client.as_ref()) {
                Err(error) => {
                    inv.error(|out| write_cstr(out, &*error));
                    ClientRef::forget_pane(&new_pane);
                    original_window.remove_pane(&new_pane);
                    window_owner.pop_zoom();
                    drop(sc.environ.take());
                    return ExecOutcome::err();
                }
                Ok(None) => {}
                Ok(Some(wait)) => {
                    pane_input = Some(wait);
                }
            }
        }
        if !flags & SPAWN_DETACHED != 0 {
            inv.context_mut().current =
                cmd_find_state::from_winlink_pane(wl.clone(), &new_pane, 0 as ::core::ffi::c_int);
        }
        window_owner.pop_zoom();
        server_redraw_window(&original_window);
        server_redraw_session(s.as_ref().expect("live session"));
        if args_has(args, 'P' as i32 as u_char) != 0 {
            let template = args_get(args, 'F' as i32 as u_char).unwrap_or(SPLIT_WINDOW_TEMPLATE);
            let cp = format_single_cstring(
                Some(&cf),
                template,
                tc.as_ref(),
                s.as_ref(),
                wl.clone(),
                Some(&new_pane),
            );
            inv.print(|out| write_cstr(out, &*cp));
        }
        fs = cmd_find_state::from_winlink_pane(wl, &new_pane, 0 as ::core::ffi::c_int);
        inv.insert_hook(Some(&fs), self_0, |out| out.write_all(b"after-split-window"));
        drop(sc.environ.take());
        drop(window_owner);
        drop(original_window);
        drop(s);
        drop(tc);
        drop(tc_owner);
        if let Some(wait) = pane_input {
            drop(new_pane);
            inv.suspend(self_0);
            wait.await;
            return ExecOutcome::ok();
        }
        if args_has(args, 'W' as i32 as u_char) != 0 {
            let wait = new_pane.wait_for(PaneWaitFor::ProcessExit, inv.owning_client());
            drop(new_pane);
            inv.suspend(self_0);
            wait.await;
            return ExecOutcome::ok();
        }
        ExecOutcome::ok()
    })
}
