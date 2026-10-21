use crate::arguments::{args_has, args_parse};
use crate::cmd::queue::{cmdq_get_client, cmdq_get_event};
use crate::cmd::{cmd_get_args, cmd_get_entry, cmd_mouse_pane};
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_READONLY, CMD_TARGET_PANE_USAGE};
use crate::shared::key::*;
use crate::shared::session::SessionRef;
use crate::window::WindowPane;
use crate::window_clock::window_clock_mode;
use crate::window_copy::{
    window_copy_mode, window_copy_pagedown, window_copy_pageup, window_copy_scroll,
    window_copy_set_line_numbers, window_copy_start_drag,
};
pub static cmd_copy_mode_entry: cmd_entry = {
    cmd_entry {
        name: c"copy-mode",
        alias: None,
        args: args_parse {
            template: c"dekHMqSs:t:u",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-dekHMqSu] [-s src-pane] [-t target-pane]",
        source: cmd_entry_flag {
            flag: b's',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK | CMD_READONLY,
        exec: Some(cmd_copy_mode_exec),
    }
};
pub static cmd_clock_mode_entry: cmd_entry = {
    cmd_entry {
        name: c"clock-mode",
        alias: None,
        args: args_parse {
            template: c"t:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: CMD_TARGET_PANE_USAGE,
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
        exec: Some(cmd_copy_mode_exec),
    }
};
unsafe fn cmd_copy_mode_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let mut event = cmdq_get_event(&(item));
    let source = crate::cmd::queue::cmdq_get_source(&item);
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut s: Option<SessionRef> = None;
    let mut pane_owner = target.wp.upgrade().expect("copy-mode target pane");
    let mut line_numbers: ::core::ffi::c_int = 0;
    if args_has(args, 'q' as i32 as u_char) != 0 {
        pane_owner.reset_all_modes();
        return CmdOutcome::Normal;
    }
    if args_has(args, 'M' as i32 as u_char) != 0 {
        let mut mouse_session_owner = None;
        let mouse_pane_owner = cmd_mouse_pane(&event.m, Some(&mut mouse_session_owner), None);
        s = mouse_session_owner.clone();
        let Some(mouse_pane) = mouse_pane_owner else {
            return CmdOutcome::Normal;
        };
        pane_owner = mouse_pane;
        if c.is_none()
            || !crate::shared::rc::same(
                c.as_ref()
                    .expect("live client")
                    .attached_session()
                    .upgrade()
                    .as_ref(),
                s.as_ref(),
            )
        {
            return CmdOutcome::Normal;
        }
    }
    if std::ptr::eq(cmd_get_entry(&cmd_ref), &cmd_clock_mode_entry) {
        pane_owner.set_mode(None, &window_clock_mode, Some(item_handle), None, None);
        return CmdOutcome::Normal;
    }
    let source_owner = if args_has(args, 's' as i32 as u_char) != 0 {
        source.wp.upgrade()
    } else {
        Some(pane_owner.clone())
    };
    line_numbers = 1 as ::core::ffi::c_int;
    if event.key as ::core::ffi::c_ulonglong & KEYC_MASK_KEY
        == KEYC_MOUSE as ::core::ffi::c_ulong as ::core::ffi::c_ulonglong
        || event.key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
            >= (KEYC_TYPE_MOUSEMOVE as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                << 32 as ::core::ffi::c_int
            && event.key as ::core::ffi::c_ulonglong & KEYC_MASK_TYPE
                <= (KEYC_TYPE_TRIPLECLICK as ::core::ffi::c_int as ::core::ffi::c_ulonglong)
                    << 32 as ::core::ffi::c_int
    {
        line_numbers = 0 as ::core::ffi::c_int;
    }
    if pane_owner.set_mode(
        source_owner.as_ref(),
        &window_copy_mode,
        Some(item_handle),
        None,
        Some(args),
    ) == 0
    {
        window_copy_set_line_numbers(&pane_owner, line_numbers);
        if args_has(args, 'M' as i32 as u_char) != 0 {
            window_copy_start_drag(c_owner.as_ref(), &mut event.m);
        }
    } else {
        window_copy_set_line_numbers(&pane_owner, line_numbers);
    }
    if args_has(args, 'u' as i32 as u_char) != 0 {
        window_copy_pageup(&pane_owner);
    }
    if args_has(args, 'd' as i32 as u_char) != 0 {
        window_copy_pagedown(&pane_owner, args_has(args, 'e' as i32 as u_char));
    }
    if args_has(args, 'S' as i32 as u_char) != 0 {
        let Some(c) = c.as_ref() else {
            return CmdOutcome::Normal;
        };
        let tty_oy = c.terminal_view().oy;
        let slider = c.mouse_slider_position();
        window_copy_scroll(
            &pane_owner,
            slider,
            event.m.y,
            tty_oy,
            args_has(args, 'e' as i32 as u_char),
        );
        return CmdOutcome::Normal;
    }
    CmdOutcome::Normal
}
