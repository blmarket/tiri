use crate::src::arguments::{
    args, args_flag_values, args_get, args_has, args_parse, args_to_vector,
};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_target_client};
use crate::src::environ::{environ_create, environ_put};
use crate::src::format::bytes::write_cstr;
use crate::src::server_fn::server_redraw_window;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::session::SessionRef;
use crate::src::shared::spawn::spawn_context;
use crate::src::shared::spawn::{SPAWN_EMPTY, SPAWN_KILL, SPAWN_RESPAWN};
use crate::src::shared::window::winlink;
use crate::src::spawn::spawn_window;
pub static cmd_respawn_window_entry: cmd_entry = {
    cmd_entry {
        name: c"respawn-window",
        alias: Some(c"respawnw"),
        args: args_parse {
            template: c"c:e:Ekt:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-Ek] [-c start-directory] [-e environment] [-t target-window] [shell-command [argument ...]]",
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
        exec: Some(cmd_respawn_window_exec),
    }
};
unsafe fn cmd_respawn_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut sc: spawn_context = spawn_context {
        item: std::rc::Weak::new(),
        s: std::rc::Weak::new(),
        wl: refbox::Weak::new(),
        tc: std::rc::Weak::new(),
        wp0: std::rc::Weak::new(),
        name: None,
        argv: Vec::new(),
        environ: None,
        idx: 0,
        cwd: None,
        flags: 0,
    };
    let mut argv_owner = Vec::new();
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut _tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let mut wl: refbox::Weak<winlink> = (*target).winlink_handle();
    let mut cause: Option<std::ffi::CString> = None;
    sc.item = std::rc::Rc::downgrade(item_handle);
    sc.s = std::rc::Rc::downgrade(s.as_ref().expect("live session"));
    sc.set_wl(wl.clone());
    sc.tc = tc_owner
        .as_ref()
        .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    argv_owner = args_to_vector(&*args);
    sc.argv = argv_owner;
    sc.environ = Some(environ_create());
    for av in args_flag_values(&*args, 'e' as i32 as u_char) {
        environ_put(
            sc.environ.as_deref_mut().expect("environment"),
            av.as_string().expect("string argument"),
            0 as ::core::ffi::c_int,
        );
    }
    sc.idx = -(1 as ::core::ffi::c_int);
    sc.cwd = args_get(&*(args), 'c' as i32 as u_char).map(|value| value.to_owned());
    sc.flags = SPAWN_RESPAWN;
    if args_has(&*args, 'E' as i32 as u_char) != 0 {
        sc.flags |= SPAWN_EMPTY;
    }
    if args_has(&*args, 'k' as i32 as u_char) != 0 {
        sc.flags |= SPAWN_KILL;
    }
    if !spawn_window(&mut sc, &mut cause).is_alive() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"respawn window failed: ")?;
            write_cstr(out, cause.as_deref())
        });
        drop(sc.environ.take());
        return CmdOutcome::Error;
    }
    server_redraw_window((wl.get_unchecked().window_handle().as_ref()).expect("live window"));
    drop(sc.environ.take());
    CmdOutcome::Normal
}
