use crate::arguments::{
    args, args_count, args_has, args_parse, args_string, args_strtonum_and_expand_result,
};
use crate::cmd::queue::{cmdq_error, cmdq_get_event, cmdq_get_target_client};
use crate::cmd::{cmd_get_args_mut, cmd_get_entry, cmd_mouse_pane};
use crate::format::bytes::write_cstr;
use crate::key_bindings::{key_bindings_dispatch, key_bindings_get, key_bindings_get_table};
use crate::key_string::key_string_parse_cstr;
use crate::options::options_get_number;
use crate::server_client::Client;
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::client::CLIENT_READONLY;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_READONLY};
use crate::shared::grid::*;
use crate::shared::key::key_event;
use crate::shared::key::*;
use crate::shared::limits::UINT_MAX;
use crate::shared::mouse::mouse_event;
use crate::shared::session::SessionRef;
use crate::shared::utf8::*;
use crate::shared::window::{window_mode_entry, winlink};
use crate::text::utf8::{utf8_from_data, utf8_fromcstr_vec};
use crate::window::WindowPane;
use libc::strtol;
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
            flag: 't' as i32 as ::core::ffi::c_char,
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
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_send_keys_exec),
    }
};
unsafe fn cmd_send_keys_inject_key(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    after_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    args: &args,
    mut key: key_code,
) -> std::rc::Weak<std::cell::RefCell<cmdq_item>> {
    let mut after = after_handle.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    let item = item_handle.borrow();
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut _s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let pane = (*target).pane_handle().expect("key target pane");
    let mut wme: refbox::Weak<window_mode_entry> = refbox::Weak::new();
    let mut new_after = after.clone();
    if args_has(&*args, 'K' as i32 as u_char) != 0 {
        if tc.is_none() {
            return std::rc::Rc::downgrade(item_handle);
        }
        let event = key_event::new(
            (key as ::core::ffi::c_ulonglong | KEYC_SENT) as key_code,
            Default::default(),
            None,
        );
        if after.strong_count() == 0 {
            if tc_owner
                .as_ref()
                .expect("key target client")
                .handle_key_after(event, None, None)
                != 0 as ::core::ffi::c_int
            {
                return std::rc::Rc::downgrade(item_handle);
            }
        } else if tc_owner
            .as_ref()
            .expect("key target client")
            .handle_key_after(event, after.upgrade().as_ref(), Some(&mut new_after))
            != 0 as ::core::ffi::c_int
        {
            return new_after;
        }
        return std::rc::Rc::downgrade(item_handle);
    }
    wme = pane.mode_entry();
    if !wme.is_alive() || wme.get_unchecked().mode.key_table.is_none() {
        if (*target).pane_handle().expect("key target pane").key(
            tc_owner.as_ref(),
            wl.clone(),
            key,
            None,
        ) != 0 as ::core::ffi::c_int
        {
            return std::rc::Weak::new();
        }
        return std::rc::Rc::downgrade(item_handle);
    }
    let table = key_bindings_get_table(
        wme.get_unchecked()
            .mode
            .key_table
            .expect("non-null function pointer")(wme),
        1 as ::core::ffi::c_int,
    )
    .expect("created key table");
    let command = key_bindings_get(&table.borrow(), key & !KEYC_MASK_FLAGS).map(|bd| bd.command());
    if let Some(command) = command {
        after = key_bindings_dispatch(
            command,
            after.upgrade().as_ref(),
            tc.as_ref(),
            None,
            &*target,
        );
    }
    after
}
unsafe fn cmd_send_keys_inject_string(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    after_handle: Option<&std::rc::Rc<std::cell::RefCell<cmdq_item>>>,
    mut args: *mut args,
    mut i: ::core::ffi::c_int,
) -> std::rc::Weak<std::cell::RefCell<cmdq_item>> {
    let mut after = after_handle.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    let mut s: &std::ffi::CStr =
        args_string(&mut *(args), i as u_int).expect("argument is present");
    let mut uc: utf8_char = 0;
    let mut key: key_code = 0;
    let mut endptr: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut n: ::core::ffi::c_long = 0;
    let mut literal: ::core::ffi::c_int = 0;
    if args_has(&*args, 'H' as i32 as u_char) != 0 {
        n = strtol(s.as_ptr(), &raw mut endptr, 16 as ::core::ffi::c_int);
        if s.is_empty()
            || n < 0 as ::core::ffi::c_long
            || n > 0xff as ::core::ffi::c_long
            || *endptr as ::core::ffi::c_int != '\0' as i32
        {
            return std::rc::Rc::downgrade(item_handle);
        }
        return cmd_send_keys_inject_key(
            item_handle,
            after.upgrade().as_ref(),
            &*args,
            KEYC_LITERAL | n as key_code,
        );
    }
    literal = args_has(&*args, 'l' as i32 as u_char);
    if literal == 0 {
        key = key_string_parse_cstr(s).unwrap_or(KEYC_UNKNOWN);
        if key != KEYC_NONE as ::core::ffi::c_ulong as key_code
            && key != KEYC_UNKNOWN as ::core::ffi::c_ulong as key_code
        {
            after = cmd_send_keys_inject_key(item_handle, after.upgrade().as_ref(), &*args, key);
            if after.strong_count() != 0 {
                return after;
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
                after =
                    cmd_send_keys_inject_key(item_handle, after.upgrade().as_ref(), &*args, key);
            }
        }
    }
    after
}
unsafe fn cmd_send_keys_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mouse_pane_owner;
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let pane = (*target).pane_handle().expect("key target pane");
    let mut event_snapshot = cmdq_get_event(&*(item));
    let event: *mut key_event = &mut event_snapshot;
    let mut m: *mut mouse_event = &raw mut (*event).m;
    let mut wme: refbox::Weak<window_mode_entry> = pane.mode_entry();
    let mut after = std::rc::Rc::downgrade(item_handle);
    let mut key: key_code = 0;
    let mut i: u_int = 0;
    let mut np: u_int = 1 as u_int;
    let mut count: u_int = args_count(&*args);
    if !tc.is_none()
        && tc.as_ref().expect("live client").flags() & CLIENT_READONLY as uint64_t != 0
        && args_has(&*args, 'X' as i32 as u_char) == 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"client is read-only"));
        return CmdOutcome::Error;
    }
    if args_has(&*args, 'N' as i32 as u_char) != 0 {
        np = match args_strtonum_and_expand_result(
            &*args,
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
        if !!wme.is_alive() && (args_has(&*args, 'X' as i32 as u_char) != 0 || count == 0 as u_int)
        {
            if wme.get_unchecked().mode.command.is_none() {
                cmdq_error(item_handle, |out| out.write_all(b"not in a mode"));
                return CmdOutcome::Error;
            }
            wme.get_mut_unchecked().prefix = np;
        }
    }
    if args_has(&*args, 'X' as i32 as u_char) != 0 {
        if !wme.is_alive() || wme.get_unchecked().mode.command.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"not in a mode"));
            return CmdOutcome::Error;
        }
        let m = if (*m).valid == 0 { None } else { Some(&mut *m) };
        wme.get_unchecked()
            .mode
            .command
            .expect("non-null function pointer")(
            wme,
            tc_owner.as_ref(),
            (*target).s.upgrade().as_ref(),
            wl,
            &mut *args,
            m,
        );
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'M' as i32 as u_char) != 0 {
        let mut mouse_session_owner = None;
        mouse_pane_owner = cmd_mouse_pane(&*m, Some(&mut mouse_session_owner), None);
        s = mouse_session_owner.clone();
        if mouse_pane_owner.is_none() {
            cmdq_error(item_handle, |out| out.write_all(b"no mouse target"));
            return CmdOutcome::Error;
        }
        mouse_pane_owner.as_ref().expect("mouse target pane").key(
            tc_owner.as_ref(),
            wl.clone(),
            (*m).key,
            m.as_mut(),
        );
        return CmdOutcome::Normal;
    }
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_send_prefix_entry,
    ) {
        if args_has(&*args, '2' as i32 as u_char) != 0 {
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
        cmd_send_keys_inject_key(item_handle, Some(item_handle), &*args, key);
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 'R' as i32 as u_char) != 0 {
        pane.reset_input();
    }
    if count == 0 as u_int {
        if args_has(&*args, 'N' as i32 as u_char) != 0
            || args_has(&*args, 'R' as i32 as u_char) != 0
        {
            return CmdOutcome::Normal;
        }
        after = if args_has(&*args, 'K' as i32 as u_char) != 0 {
            std::rc::Rc::downgrade(item_handle)
        } else {
            std::rc::Weak::new()
        };
        while np != 0 as u_int {
            after = cmd_send_keys_inject_key(
                item_handle,
                after.upgrade().as_ref(),
                &*args,
                (*event).key,
            );
            np = np.wrapping_sub(1);
        }
        return CmdOutcome::Normal;
    }
    while np != 0 as u_int {
        i = 0 as u_int;
        while i < count {
            after = cmd_send_keys_inject_string(
                item_handle,
                after.upgrade().as_ref(),
                args,
                i as ::core::ffi::c_int,
            );
            i = i.wrapping_add(1);
        }
        np = np.wrapping_sub(1);
    }
    CmdOutcome::Normal
}
