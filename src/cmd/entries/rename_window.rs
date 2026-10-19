use crate::arguments::{args_parse, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::format::bytes::write_cstr;
use crate::format::format_single_from_target_cstring;
use crate::options::options_set_number;
use crate::server_fn::{server_redraw_window_borders, server_status_window};
use crate::shared::abi::*;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::window::winlink;
use crate::tmux::check_name;
use crate::window::Window;
pub static cmd_rename_window_entry: cmd_entry = {
    cmd_entry {
        name: c"rename-window",
        alias: Some(c"renamew"),
        args: args_parse {
            template: c"t:",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-t target-window] new-name",
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
        exec: Some(cmd_rename_window_exec),
    }
};
unsafe fn cmd_rename_window_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let mut wl: refbox::Weak<winlink> = target.winlink_handle();
    let name = format_single_from_target_cstring(
        item_handle,
        args_string(args, 0 as u_int).unwrap_or(c""),
    );
    if !check_name(&name) {
        cmdq_error(item_handle, |out| {
            out.write_all(b"invalid window name: ")?;
            write_cstr(out, &*name)
        });
        return CmdOutcome::Error;
    }
    let window = wl
        .get_unchecked()
        .window_handle()
        .expect("target window")
        .clone();
    window.rename(&name, false);
    options_set_number(&mut window.borrow_options_mut(), c"automatic-rename", 0);
    server_redraw_window_borders(
        (wl.get_unchecked().window_handle().as_ref()).expect("live window"),
    );
    server_status_window((wl.get_unchecked().window_handle().as_ref()).expect("live window"));
    drop(window);
    CmdOutcome::Normal
}
