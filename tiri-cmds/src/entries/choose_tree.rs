use crate::cmd::queue::cmdq_error;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use tiri_core::actions::TaskOutcome as CmdOutcome;
use tiri_core::arguments::{
    ARGS_PARSE_COMMANDS_OR_STRING, ArgsParseError, args, args_get, args_has, args_parse,
    args_parse_type,
};
use tiri_core::paste::paste_is_empty;
use tiri_core::server_client::Client as _;
use tiri_core::shared::client::ClientRef;

use crate::cmd::queue::cmdq_item;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_WINDOW};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::sort::{SORT_ACTIVITY, SORT_END, sort_order};
use tiri_core::shared::window::window_mode;
use tiri_core::sort::sort_order_from_string;
use tiri_core::window_buffer::window_buffer_mode;
use tiri_core::window_client::window_client_mode;
use tiri_core::window_customize::window_customize_mode;
use tiri_core::window_pane::WindowPane as _;
use tiri_core::window_panes::window_panes_mode;
use tiri_core::window_switch::window_switch_mode;
use tiri_core::window_tree::window_tree_mode;
pub static cmd_choose_tree_entry: cmd_entry = {
    cmd_entry {
        name: c"choose-tree",
        alias: None,
        args: args_parse {
            template: c"F:f:GhK:kNO:rst:wyZ",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(
                cmd_choose_tree_args_parse
            ),
        },
        usage: c"[-GhkNrswZ] [-F format] [-f filter] [-K key-format] [-O sort-order] [-t target-pane] [template]",
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
        exec: Some(cmd_choose_tree_exec),
    }
};
pub static cmd_choose_client_entry: cmd_entry = {
    cmd_entry {
        name: c"choose-client",
        alias: None,
        args: args_parse {
            template: c"F:f:hiK:kNO:rt:yZ",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(
                cmd_choose_tree_args_parse
            ),
        },
        usage: c"[-hikNrZ] [-F format] [-f filter] [-K key-format] [-O sort-order] [-t target-pane] [template]",
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
        exec: Some(cmd_choose_tree_exec),
    }
};
pub static cmd_choose_buffer_entry: cmd_entry = {
    cmd_entry {
        name: c"choose-buffer",
        alias: None,
        args: args_parse {
            template: c"F:f:K:kNO:rt:yZ",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(
                cmd_choose_tree_args_parse
            ),
        },
        usage: c"[-kNrZ] [-F format] [-f filter] [-K key-format] [-O sort-order] [-t target-pane] [template]",
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
        exec: Some(cmd_choose_tree_exec),
    }
};
pub static cmd_customize_mode_entry: cmd_entry = {
    cmd_entry {
        name: c"customize-mode",
        alias: None,
        args: args_parse {
            template: c"F:f:kNt:yZ",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-kNZ] [-F format] [-f filter] [-t target-pane]",
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
        exec: Some(cmd_choose_tree_exec),
    }
};
pub static cmd_switch_mode_entry: cmd_entry = {
    cmd_entry {
        name: c"switch-mode",
        alias: None,
        args: args_parse {
            template: c"F:kst:wZ",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(cmd_choose_tree_args_parse),
        },
        usage: c"[-ksZw] [-F format] [-t target-pane] [command]",
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
        exec: Some(cmd_choose_tree_exec),
    }
};
pub static cmd_display_panes_entry: cmd_entry = {
    cmd_entry {
        name: c"display-panes",
        alias: Some(c"displayp"),
        args: args_parse {
            template: c"d:kNs:t:Z",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(cmd_choose_tree_args_parse),
        },
        usage: c"[-kNZ] [-d duration] [-s source-window] [-t target-pane] [template]",
        source: cmd_entry_flag {
            flag: b's',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_choose_tree_exec),
    }
};
fn cmd_choose_tree_args_parse(
    _args: &args,
    _idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    Ok(ARGS_PARSE_COMMANDS_OR_STRING)
}
fn cmd_choose_tree_exec(self_0: &cmd, item_handle: &std::rc::Rc<cmdq_item>) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0);
    let target = tiri_core::actions::queue::target(&item);
    let wp = target.pane_handle();

    let mut order: sort_order = SORT_ACTIVITY;
    order = sort_order_from_string(args_get(args, 'O' as i32 as u_char));
    if order as ::core::ffi::c_uint == SORT_END as ::core::ffi::c_int as ::core::ffi::c_uint
        && args_has(args, 'O' as i32 as u_char) != 0
    {
        cmdq_error(item_handle, |out| out.write_all(b"invalid sort order"));
        return CmdOutcome::Error;
    }
    let mode: &'static window_mode =
        if std::ptr::eq(cmd_get_entry(self_0), &cmd_choose_buffer_entry) {
            if paste_is_empty() != 0 {
                return CmdOutcome::Normal;
            }
            &window_buffer_mode
        } else if std::ptr::eq(cmd_get_entry(self_0), &cmd_choose_client_entry) {
            if ClientRef::attached_count() == 0 as u_int {
                return CmdOutcome::Normal;
            }
            &window_client_mode
        } else if std::ptr::eq(cmd_get_entry(self_0), &cmd_customize_mode_entry) {
            &window_customize_mode
        } else if std::ptr::eq(cmd_get_entry(self_0), &cmd_switch_mode_entry) {
            &window_switch_mode
        } else if std::ptr::eq(cmd_get_entry(self_0), &cmd_display_panes_entry) {
            &window_panes_mode
        } else {
            &window_tree_mode
        };
    wp.as_ref().expect("mode target pane").set_mode(
        None,
        mode,
        Some(item_handle),
        Some(target),
        Some(args),
    );
    CmdOutcome::Normal
}
