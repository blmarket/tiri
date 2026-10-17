use crate::src::arguments::args_string;
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::cmdq_error;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::options::options_set_number;
use crate::src::server_fn::{server_redraw_window_borders, server_status_window};
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::CMD_AFTERHOOK;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::window::winlink;
use crate::src::tmux::check_name;
use crate::src::window::Window;
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
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let name = format_single_from_target_cstring(
        item_handle,
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr()),
    );
    if !check_name(&name) {
        cmdq_error(item_handle, |out| {
            out.write_all(b"invalid window name: ")?;
            write_cstr(out, &*name)
        });
        return CMD_RETURN_ERROR;
    }
    let window = wl
        .get_unchecked()
        .window_handle()
        .expect("target window")
        .clone();
    window.rename(&name, false);
    options_set_number(window.borrow_options_mut(), c"automatic-rename", 0);
    server_redraw_window_borders(
        (wl.get_unchecked().window_handle().as_ref()).expect("live window"),
    );
    server_status_window((wl.get_unchecked().window_handle().as_ref()).expect("live window"));
    drop(window);
    CMD_RETURN_NORMAL
}
