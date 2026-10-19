use crate::arguments::{args_has, args_parse};
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_get_state_owned, cmdq_insert_hook};
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::resize::recalculate_sizes;
use crate::server_client::Client as _;
use crate::server_fn::server_redraw_session;
use crate::session::Session;

use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_TARGET_SESSION_USAGE;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::session::SessionRef;
use crate::shared::window::winlink;
use crate::window::Window as _;
pub static cmd_select_window_entry: cmd_entry = {
    cmd_entry {
        name: c"select-window",
        alias: Some(c"selectw"),
        args: args_parse {
            template: c"lnpTt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-lnpT] [-t target-window]",
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
        exec: Some(cmd_select_window_exec),
    }
};
pub static cmd_next_window_entry: cmd_entry = {
    cmd_entry {
        name: c"next-window",
        alias: Some(c"next"),
        args: args_parse {
            template: c"at:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-a] [-t target-session]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_select_window_exec),
    }
};
pub static cmd_previous_window_entry: cmd_entry = {
    cmd_entry {
        name: c"previous-window",
        alias: Some(c"prev"),
        args: args_parse {
            template: c"at:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-a] [-t target-session]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_select_window_exec),
    }
};
pub static cmd_last_window_entry: cmd_entry = {
    cmd_entry {
        name: c"last-window",
        alias: Some(c"last"),
        args: args_parse {
            template: c"t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: CMD_TARGET_SESSION_USAGE,
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_select_window_exec),
    }
};
unsafe fn cmd_select_window_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let current = cmdq_get_state_owned(&*(item));
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let mut target: *mut cmd_find_state = &mut target_state;
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut next: ::core::ffi::c_int = 0;
    let mut previous: ::core::ffi::c_int = 0;
    let mut last: ::core::ffi::c_int = 0;
    let mut activity: ::core::ffi::c_int = 0;
    next = (std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_next_window_entry,
    )) as ::core::ffi::c_int;
    if args_has(args, 'n' as i32 as u_char) != 0 {
        next = 1 as ::core::ffi::c_int;
    }
    previous = (std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_previous_window_entry,
    )) as ::core::ffi::c_int;
    if args_has(args, 'p' as i32 as u_char) != 0 {
        previous = 1 as ::core::ffi::c_int;
    }
    last = (std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_last_window_entry,
    )) as ::core::ffi::c_int;
    if args_has(args, 'l' as i32 as u_char) != 0 {
        last = 1 as ::core::ffi::c_int;
    }
    if next != 0 || previous != 0 || last != 0 {
        activity = args_has(args, 'a' as i32 as u_char);
        if next != 0 {
            if (s.as_ref().expect("live session")).select_adjacent_window(false, (activity) != 0)
                != 0 as ::core::ffi::c_int
            {
                cmdq_error(item_handle, |out| out.write_all(b"no next window"));
                return CmdOutcome::Error;
            }
        } else if previous != 0 {
            if (s.as_ref().expect("live session")).select_adjacent_window(true, (activity) != 0)
                != 0 as ::core::ffi::c_int
            {
                cmdq_error(item_handle, |out| out.write_all(b"no previous window"));
                return CmdOutcome::Error;
            }
        } else if (s.as_ref().expect("live session")).select_last_window()
            != 0 as ::core::ffi::c_int
        {
            cmdq_error(item_handle, |out| out.write_all(b"no last window"));
            return CmdOutcome::Error;
        }
        *current.current.borrow_mut() = cmd_find_state::find_from_session(
            s.as_ref().expect("live session"),
            0 as ::core::ffi::c_int,
        );
        server_redraw_session(s.as_ref().expect("live session"));
        cmdq_insert_hook(
            s.as_ref(),
            item_handle,
            Some(&current.current_snapshot()),
            |out| out.write_all(b"after-select-window"),
        );
    } else {
        if args_has(args, 'T' as i32 as u_char) != 0
            && wl == s.as_ref().expect("live session").current_winlink()
        {
            if (s.as_ref().expect("live session")).select_last_window() != 0 as ::core::ffi::c_int {
                cmdq_error(item_handle, |out| out.write_all(b"no last window"));
                return CmdOutcome::Error;
            }
            if crate::shared::rc::same(
                current.current.borrow().session_handle().as_ref(),
                s.as_ref(),
            ) {
                *current.current.borrow_mut() = cmd_find_state::find_from_session(
                    s.as_ref().expect("live session"),
                    0 as ::core::ffi::c_int,
                );
            }
            server_redraw_session(s.as_ref().expect("live session"));
        } else if (s.as_ref().expect("live session")).select_index(wl.get_unchecked().idx)
            == 0 as ::core::ffi::c_int
        {
            *current.current.borrow_mut() = cmd_find_state::find_from_session(
                s.as_ref().expect("live session"),
                0 as ::core::ffi::c_int,
            );
            server_redraw_session(s.as_ref().expect("live session"));
        }
        cmdq_insert_hook(
            s.as_ref(),
            item_handle,
            Some(&current.current_snapshot()),
            |out| out.write_all(b"after-select-window"),
        );
    }
    if !c.is_none()
        && !c
            .as_ref()
            .expect("live client")
            .attached_session()
            .upgrade()
            .is_none()
    {
        s.as_ref()
            .expect("live session")
            .current_winlink()
            .get_unchecked()
            .window_handle()
            .expect("live window")
            .set_latest_client(c.as_ref());
    }
    recalculate_sizes();
    CmdOutcome::Normal
}
