use crate::arguments::{args_has, args_parse, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::format::bytes::write_cstr;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::window::{LayoutKind, Window as _};
pub static cmd_select_layout_entry: cmd_entry = {
    cmd_entry {
        name: c"select-layout",
        alias: Some(c"selectl"),
        args: args_parse {
            template: c"nt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-n] [-t target-window] [layout-name]",
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_select_layout_exec),
    }
};

/// Arrange the window in the named layout, the next one with -n, or afresh
/// in its current one.
unsafe fn cmd_select_layout_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let mut target_state = crate::cmd::queue::cmdq_get_target(&item).clone();
    let target: *mut cmd_find_state = &mut target_state;
    let window_owner = (*target).w.upgrade().expect("live layout window");
    let current = window_owner.layout();
    let kind = if args_has(args, b'n') != 0 {
        Some(current.next())
    } else if let Some(name) = args_string(args, 0 as u_int) {
        LayoutKind::from_name(name)
    } else {
        Some(current)
    };
    let result = match kind {
        Some(kind) => {
            window_owner.set_layout(kind);
            CmdOutcome::Normal
        }
        None => {
            let name = args_string(args, 0 as u_int).expect("named layout");
            cmdq_error(item_handle, |out| {
                out.write_all(b"unknown layout: ")?;
                write_cstr(out, &*name)
            });
            CmdOutcome::Error
        }
    };
    drop(window_owner);
    result
}
