use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_READONLY};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::future::Future;
use std::pin::Pin;
use tiri_core::arguments::{
    args, args_count, args_has, args_parse, args_string, args_strtonum_and_expand_result,
};
use tiri_core::bindings::{key_bindings_dispatch, key_bindings_get, key_bindings_get_table};
use tiri_core::format::bytes::write_cstr;
use tiri_core::key_string::key_string_parse_cstr;
use tiri_core::options::options_get_number;
use tiri_core::server_client::Client;
use tiri_core::session::Session;
use tiri_core::shared::abi::{u_char, u_int, uint64_t};
use tiri_core::shared::client::CLIENT_READONLY;
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::grid::utf8_char;
use tiri_core::shared::key::key_event;
use tiri_core::shared::key::{
    KEYC_LITERAL, KEYC_MASK_FLAGS, KEYC_NONE, KEYC_SENT, KEYC_UNKNOWN, key_code,
};
use tiri_core::shared::limits::UINT_MAX;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::utf8::UTF8_DONE;
use tiri_core::shared::window::{window_mode_entry, winlink};
use tiri_core::text::action_helpers::mouse_pane as cmd_mouse_pane;
use tiri_core::text::numbers::strtol_prefix;
use tiri_core::text::utf8::{utf8_from_data, utf8_fromcstr_vec};
use tiri_core::window::WindowPane;

pub static cmd_send_keys_entry: cmd_entry = {
    cmd_entry {
        name: c"send-keys",
        alias: Some(c"send"),
        args: args_parse {
            template: c"c:FHKlMN:Rt:X",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-FHKlMRX] [-c target-client] [-N repeat-count] [-t target-pane] [key ...]",
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
        flags: CMD_AFTERHOOK | CMD_CLIENT_CFLAG | CMD_CLIENT_CANFAIL | CMD_READONLY,
        exec: Some(cmd_send_keys_exec),
    }
};

pub static cmd_send_prefix_entry: cmd_entry = {
    cmd_entry {
        name: c"send-prefix",
        alias: None,
        args: args_parse {
            template: c"2t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-2] [-t target-pane]",
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_send_keys_exec),
    }
};

fn into_tail(tasks: Vec<Pin<Box<dyn Future<Output = ()>>>>) -> ExecOutcome {
    if tasks.is_empty() {
        ExecOutcome::ok()
    } else {
        ExecOutcome::tail(move |_| {
            Box::pin(async move {
                for task in tasks {
                    task.await;
                }
            })
        })
    }
}

fn cmd_send_keys_inject_key(
    inv: &Invocation<'_>,
    tasks: &mut Vec<Pin<Box<dyn Future<Output = ()>>>>,
    args: &args,
    key: key_code,
) -> bool {
    let target = inv.target();
    let tc_owner = inv.target_client();
    let wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let pane = target.pane_handle().expect("key target pane");
    if args_has(args, 'K' as i32 as u_char) != 0 {
        let Some(tc) = tc_owner.as_ref() else {
            return true;
        };
        let event = key_event::new(
            (key as ::core::ffi::c_ulonglong | KEYC_SENT) as key_code,
            Default::default(),
            None,
        );
        if let Some(task) = tc.prepare_sent_key(event, inv.client_weak()) {
            tasks.push(task);
        }
        return true;
    }
    let wme: tiri_core::refbox::Weak<window_mode_entry> = pane.mode_entry();
    if !wme.is_alive()
        || wme
            .try_borrow()
            .expect("live mode entry")
            .mode
            .key_table
            .is_none()
    {
        return pane.key(tc_owner.as_ref(), wl, key, None) == 0 as ::core::ffi::c_int;
    }
    let key_table = wme
        .try_borrow()
        .expect("live mode entry")
        .mode
        .key_table
        .expect("non-null function pointer");
    let table =
        key_bindings_get_table(key_table(wme), 1 as ::core::ffi::c_int).expect("created key table");
    let command = key_bindings_get(&table.borrow(), key & !KEYC_MASK_FLAGS).map(|bd| bd.command());
    if let Some(command) = command {
        let task = key_bindings_dispatch(command, tc_owner.as_ref(), inv.client_weak(), None, target);
        tasks.push(task);
    }
    true
}

fn cmd_send_keys_inject_string(
    inv: &Invocation<'_>,
    tasks: &mut Vec<Pin<Box<dyn Future<Output = ()>>>>,
    args: &args,
    i: ::core::ffi::c_int,
) {
    let s: &std::ffi::CStr = args_string(args, i as u_int).expect("argument is present");
    let mut uc: utf8_char = 0;
    let mut key: key_code;
    if args_has(args, 'H' as i32 as u_char) != 0 {
        let (n, consumed) = strtol_prefix(s.to_bytes(), 16);
        if s.is_empty()
            || n < 0 as ::core::ffi::c_long
            || n > 0xff as ::core::ffi::c_long
            || consumed != s.to_bytes().len()
        {
            return;
        }
        cmd_send_keys_inject_key(inv, tasks, args, KEYC_LITERAL | n as key_code);
        return;
    }
    let mut literal = args_has(args, 'l' as i32 as u_char);
    if literal == 0 {
        key = key_string_parse_cstr(s).unwrap_or(KEYC_UNKNOWN);
        if key != KEYC_NONE as ::core::ffi::c_ulong as key_code
            && key != KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
            && cmd_send_keys_inject_key(inv, tasks, args, key)
        {
            return;
        }
        literal = 1 as ::core::ffi::c_int;
    }
    if literal != 0 {
        let cells = utf8_fromcstr_vec(s);
        for cell in cells.iter().take_while(|cell| cell.size != 0) {
            let resolved = if cell.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                && cell.data[0] as ::core::ffi::c_int <= 0x7f as ::core::ffi::c_int
            {
                Some(cell.data[0] as key_code)
            } else if utf8_from_data(cell, &mut uc) as ::core::ffi::c_uint
                != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                None
            } else {
                Some(uc as key_code)
            };
            if let Some(key) = resolved {
                cmd_send_keys_inject_key(inv, tasks, args, key);
            }
        }
    }
}

fn cmd_send_keys_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let args = cmd_get_args(self_0);
        let tc_owner = inv.target_client();
        let tc: Option<ClientRef> = tc_owner.clone();
        let s: Option<SessionRef> = inv.target().session_handle();
        let wl: tiri_core::refbox::Weak<winlink> = inv.target().winlink_handle();
        let pane = inv.target().pane_handle().expect("key target pane");
        let mut event = inv.event();
        let wme: tiri_core::refbox::Weak<window_mode_entry> = pane.mode_entry();
        let mut tasks = Vec::new();
        let mut np: u_int = 1 as u_int;
        let count: u_int = args_count(args);
        if tc
            .as_ref()
            .is_some_and(|tc_ref| tc_ref.flags() & CLIENT_READONLY as uint64_t != 0)
            && args_has(args, 'X' as i32 as u_char) == 0
        {
            inv.error(|out| out.write_all(b"client is read-only"));
            return ExecOutcome::err();
        }
        if args_has(args, 'N' as i32 as u_char) != 0 {
            let cf = inv.format();
            np = match args_strtonum_and_expand_result(
                args,
                'N' as i32 as u_char,
                1 as ::core::ffi::c_longlong,
                UINT_MAX as ::core::ffi::c_longlong,
                Some(&cf),
                tc.as_ref(),
                inv.target(),
            ) {
                Ok(value) => value as u_int,
                Err(error) => {
                    inv.error(|out| {
                        out.write_all(b"repeat count ")?;
                        write_cstr(out, error.message())
                    });
                    return ExecOutcome::err();
                }
            };
            if wme.is_alive() && (args_has(args, 'X' as i32 as u_char) != 0 || count == 0 as u_int)
            {
                if wme
                    .try_borrow()
                    .expect("live mode entry")
                    .mode
                    .command
                    .is_none()
                {
                    inv.error(|out| out.write_all(b"not in a mode"));
                    return ExecOutcome::err();
                }
                wme.try_borrow_mut().expect("live mode entry").prefix = np;
            }
        }
        if args_has(args, 'X' as i32 as u_char) != 0 {
            if !wme.is_alive()
                || wme
                    .try_borrow()
                    .expect("live mode entry")
                    .mode
                    .command
                    .is_none()
            {
                inv.error(|out| out.write_all(b"not in a mode"));
                return ExecOutcome::err();
            }
            let m = if event.m.valid == 0 {
                None
            } else {
                Some(&mut event.m)
            };
            let command = wme
                .try_borrow()
                .expect("live mode entry")
                .mode
                .command
                .expect("non-null function pointer");
            command(
                wme,
                tc_owner.as_ref(),
                inv.target().s.upgrade().as_ref(),
                wl,
                args,
                m,
            );
            return ExecOutcome::ok();
        }
        if args_has(args, 'M' as i32 as u_char) != 0 {
            let mut mouse_session_owner = None;
            let mouse_pane_owner = cmd_mouse_pane(&event.m, Some(&mut mouse_session_owner), None);
            let Some(mouse_pane) = mouse_pane_owner else {
                inv.error(|out| out.write_all(b"no mouse target"));
                return ExecOutcome::err();
            };
            mouse_pane.key(tc_owner.as_ref(), wl, event.m.key, Some(&mut event.m));
            return ExecOutcome::ok();
        }
        if std::ptr::eq(cmd_get_entry(self_0), &cmd_send_prefix_entry) {
            let key = if args_has(args, '2' as i32 as u_char) != 0 {
                options_get_number(
                    &s.as_ref().expect("live session").borrow_options_mut(),
                    c"prefix2",
                ) as key_code
            } else {
                options_get_number(
                    &s.as_ref().expect("live session").borrow_options_mut(),
                    c"prefix",
                ) as key_code
            };
            cmd_send_keys_inject_key(inv, &mut tasks, args, key);
            return into_tail(tasks);
        }
        if args_has(args, 'R' as i32 as u_char) != 0 {
            pane.reset_input();
        }
        if count == 0 as u_int {
            if args_has(args, 'N' as i32 as u_char) != 0 || args_has(args, 'R' as i32 as u_char) != 0
            {
                return ExecOutcome::ok();
            }
            while np != 0 as u_int {
                cmd_send_keys_inject_key(inv, &mut tasks, args, event.key);
                np = np.wrapping_sub(1);
            }
            return into_tail(tasks);
        }
        while np != 0 as u_int {
            let mut i: u_int = 0;
            while i < count {
                cmd_send_keys_inject_string(inv, &mut tasks, args, i as ::core::ffi::c_int);
                i = i.wrapping_add(1);
            }
            np = np.wrapping_sub(1);
        }
        into_tail(tasks)
    })
}
