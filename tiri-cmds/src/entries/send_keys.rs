use crate::cmd::queue::{CmdqFollowup, cmdq_error, cmdq_followup, cmdq_item};
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_READONLY};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::actions::TaskOutcome as CmdOutcome;
use tiri_core::actions::queue::{event as cmdq_get_event, target_client as cmdq_get_target_client};
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
fn cmd_send_keys_inject_key(
    item_handle: &std::rc::Rc<cmdq_item>,
    followup: &mut CmdqFollowup,
    args: &args,
    mut key: key_code,
) -> bool {
    let item = item_handle.borrow();
    let target = tiri_core::actions::queue::target(&item);
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut _s: Option<SessionRef> = target.session_handle();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let pane = target.pane_handle().expect("key target pane");
    let mut wme: tiri_core::refbox::Weak<window_mode_entry> = tiri_core::refbox::Weak::new();
    if args_has(args, 'K' as i32 as u_char) != 0 {
        let Some(tc) = tc_owner.as_ref() else {
            return true;
        };
        let event = key_event::new(
            (key as ::core::ffi::c_ulonglong | KEYC_SENT) as key_code,
            Default::default(),
            None,
        );
        if let Some(task) = tc.prepare_sent_key(event) {
            followup.push(crate::engine::native(&task));
        }
        return true;
    }
    wme = pane.mode_entry();
    if !wme.is_alive()
        || wme
            .try_borrow()
            .expect("live mode entry")
            .mode
            .key_table
            .is_none()
    {
        return target.pane_handle().expect("key target pane").key(
            tc_owner.as_ref(),
            wl.clone(),
            key,
            None,
        ) == 0 as ::core::ffi::c_int;
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
        let task = key_bindings_dispatch(command, tc.as_ref(), None, target);
        followup.push(crate::engine::native(&task));
    }
    true
}
fn cmd_send_keys_inject_string(
    item_handle: &std::rc::Rc<cmdq_item>,
    followup: &mut CmdqFollowup,
    args: &args,
    mut i: ::core::ffi::c_int,
) {
    let mut s: &std::ffi::CStr = args_string(args, i as u_int).expect("argument is present");
    let mut uc: utf8_char = 0;
    let mut key: key_code = 0;
    let mut n: ::core::ffi::c_long = 0;
    let mut literal: ::core::ffi::c_int = 0;
    if args_has(args, 'H' as i32 as u_char) != 0 {
        let consumed;
        (n, consumed) = strtol_prefix(s.to_bytes(), 16);
        if s.is_empty()
            || n < 0 as ::core::ffi::c_long
            || n > 0xff as ::core::ffi::c_long
            || consumed != s.to_bytes().len()
        {
            return;
        }
        cmd_send_keys_inject_key(item_handle, followup, args, KEYC_LITERAL | n as key_code);
        return;
    }
    literal = args_has(args, 'l' as i32 as u_char);
    if literal == 0 {
        key = key_string_parse_cstr(s).unwrap_or(KEYC_UNKNOWN);
        if key != KEYC_NONE as ::core::ffi::c_ulong as key_code
            && key != KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
        {
            if cmd_send_keys_inject_key(item_handle, followup, args, key) {
                return;
            }
        }
        literal = 1 as ::core::ffi::c_int;
    }
    if literal != 0 {
        let cells = utf8_fromcstr_vec(s);
        let mut current_block_20: u64;
        for cell in cells.iter().take_while(|cell| cell.size != 0) {
            if cell.size as ::core::ffi::c_int == 1 as ::core::ffi::c_int
                && cell.data[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int
                    <= 0x7f as ::core::ffi::c_int
            {
                key = cell.data[0 as ::core::ffi::c_int as usize] as key_code;
                current_block_20 = 12147880666119273379;
            } else if utf8_from_data(cell, &mut uc) as ::core::ffi::c_uint
                != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                current_block_20 = 1054647088692577877;
            } else {
                key = uc as key_code;
                current_block_20 = 12147880666119273379;
            }
            if current_block_20 == 12147880666119273379 {
                cmd_send_keys_inject_key(item_handle, followup, args, key);
            }
        }
    }
}
fn cmd_send_keys_exec(self_0: &cmd, item_handle: &std::rc::Rc<cmdq_item>) -> CmdOutcome {
    let item = item_handle.borrow();
    let mouse_pane_owner;
    let args = cmd_get_args(self_0);
    let target = tiri_core::actions::queue::target(&item);
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = target.session_handle();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let pane = target.pane_handle().expect("key target pane");
    let mut event = cmdq_get_event(&(item));
    let mut wme: tiri_core::refbox::Weak<window_mode_entry> = pane.mode_entry();
    let mut followup = cmdq_followup(item_handle);
    let mut key: key_code = 0;
    let mut i: u_int = 0;
    let mut np: u_int = 1 as u_int;
    let mut count: u_int = args_count(args);
    if !tc.is_none()
        && tc.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0
        && args_has(args, 'X' as i32 as u_char) == 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
        return CmdOutcome::Error;
    }
    if args_has(args, 'N' as i32 as u_char) != 0 {
        np = match args_strtonum_and_expand_result(
            args,
            'N' as i32 as u_char,
            1 as ::core::ffi::c_longlong,
            UINT_MAX as ::core::ffi::c_longlong,
            Some(item_handle),
        ) {
            Ok(value) => value as u_int,
            Err(error) => {
                cmdq_error(item_handle, |out| {
                    out.write_all(b"repeat count ")?;
                    write_cstr(out, error.message())
                });
                return CmdOutcome::Error;
            }
        };
        if !!wme.is_alive() && (args_has(args, 'X' as i32 as u_char) != 0 || count == 0 as u_int) {
            if wme
                .try_borrow()
                .expect("live mode entry")
                .mode
                .command
                .is_none()
            {
                cmdq_error(item_handle, |out| out.write_all(b"not in a mode"));
                return CmdOutcome::Error;
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
            cmdq_error(item_handle, |out| out.write_all(b"not in a mode"));
            return CmdOutcome::Error;
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
            target.s.upgrade().as_ref(),
            wl,
            args,
            m,
        );
        return CmdOutcome::Normal;
    }
    if args_has(args, 'M' as i32 as u_char) != 0 {
        let mut mouse_session_owner = None;
        mouse_pane_owner = cmd_mouse_pane(&event.m, Some(&mut mouse_session_owner), None);
        s = mouse_session_owner.clone();
        if mouse_pane_owner.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"no mouse target"));
            return CmdOutcome::Error;
        }
        mouse_pane_owner.as_ref().expect("mouse target pane").key(
            tc_owner.as_ref(),
            wl.clone(),
            event.m.key,
            Some(&mut event.m),
        );
        return CmdOutcome::Normal;
    }
    if std::ptr::eq(cmd_get_entry(self_0), &cmd_send_prefix_entry) {
        if args_has(args, '2' as i32 as u_char) != 0 {
            key = options_get_number(
                &s.as_ref().expect("live session").borrow_options_mut(),
                c"prefix2",
            ) as key_code;
        } else {
            key = options_get_number(
                &s.as_ref().expect("live session").borrow_options_mut(),
                c"prefix",
            ) as key_code;
        }
        cmd_send_keys_inject_key(item_handle, &mut followup, args, key);
        drop(item);
        return followup.into_outcome();
    }
    if args_has(args, 'R' as i32 as u_char) != 0 {
        pane.reset_input();
    }
    if count == 0 as u_int {
        if args_has(args, 'N' as i32 as u_char) != 0 || args_has(args, 'R' as i32 as u_char) != 0 {
            return CmdOutcome::Normal;
        }
        while np != 0 as u_int {
            cmd_send_keys_inject_key(item_handle, &mut followup, args, event.key);
            np = np.wrapping_sub(1);
        }
        drop(item);
        return followup.into_outcome();
    }
    while np != 0 as u_int {
        i = 0 as u_int;
        while i < count {
            cmd_send_keys_inject_string(item_handle, &mut followup, args, i as ::core::ffi::c_int);
            i = i.wrapping_add(1);
        }
        np = np.wrapping_sub(1);
    }
    drop(item);
    followup.into_outcome()
}
