use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_WINDOW};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_parse, args_string};
use tiri_core::format::bytes::write_cstr;
use tiri_core::options::options_set_number;
use tiri_core::server_fn::{server_redraw_window_borders, server_status_window};
use tiri_core::shared::abi::u_int;
use tiri_core::shared::window::winlink;
use tiri_core::tmux::check_name;
use tiri_core::window::Window;
use tiri_core::winlink::winlink_window;
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
            flag: b't',
            type_0: CMD_FIND_WINDOW,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_rename_window_exec),
    }
};
fn cmd_rename_window_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let target = inv.target().clone();
    let mut wl: tiri_core::refbox::Weak<winlink> = target.winlink_handle();
    let name = inv.format_single( args_string(args, 0 as u_int).unwrap_or(c""));
    if !check_name(&name) {
        inv.error( |out| {
            out.write_all(b"invalid window name: ")?;
            write_cstr(out, &*name)
        });
        return ExecOutcome::err();
    }
    let window = winlink_window(&wl).expect("target window").clone();
    window.rename(&name, false);
    options_set_number(&mut window.borrow_options_mut(), c"automatic-rename", 0);
    server_redraw_window_borders((winlink_window(&wl).as_ref()).expect("live window"));
    server_status_window((winlink_window(&wl).as_ref()).expect("live window"));
    drop(window);
    ExecOutcome::ok()
})
}
