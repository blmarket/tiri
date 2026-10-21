use crate::arguments::{args_flag_values, args_get, args_has, args_parse, args_to_vector};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::cmdq_error;
use crate::environ::{environ_create, environ_put};
use crate::format::bytes::write_cstr;
use crate::server_fn::{server_redraw_window_borders, server_status_window};
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::pane::window_pane;
use crate::shared::session::SessionRef;
use crate::shared::spawn::spawn_context;
use crate::shared::spawn::{SPAWN_EMPTY, SPAWN_KILL, SPAWN_RESPAWN};
use crate::shared::window::winlink;
use crate::window_pane::WindowPane;
pub static cmd_respawn_pane_entry: cmd_entry = {
    cmd_entry {
        name: c"respawn-pane",
        alias: Some(c"respawnp"),
        args: args_parse {
            template: c"c:e:Ekt:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-Ek] [-c start-directory] [-e environment] [-t target-pane] [shell-command [argument ...]]",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_respawn_pane_exec),
    }
};
unsafe fn cmd_respawn_pane_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
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
    let mut s: Option<SessionRef> = target.session_handle();
    let mut wl: refbox::Weak<winlink> = target.winlink_handle();
    let wp = target.pane_handle();
    let mut cause: Option<std::ffi::CString> = None;
    sc.item = std::rc::Rc::downgrade(item_handle);
    sc.s = std::rc::Rc::downgrade(s.as_ref().expect("live session"));
    sc.set_wl(wl.clone());
    sc.wp0 = std::rc::Rc::downgrade(wp.as_ref().expect("respawn pane"));
    argv_owner = args_to_vector(args);
    sc.argv = argv_owner;
    sc.environ = Some(environ_create());
    for av in args_flag_values(args, 'e' as i32 as u_char) {
        environ_put(
            sc.environ.as_deref_mut().expect("environment"),
            av.as_string().expect("string argument"),
            0 as ::core::ffi::c_int,
        );
    }
    sc.idx = -(1 as ::core::ffi::c_int);
    sc.cwd = args_get(args, 'c' as i32 as u_char).map(|value| value.to_owned());
    sc.flags = SPAWN_RESPAWN;
    if args_has(args, 'E' as i32 as u_char) != 0 {
        sc.flags |= SPAWN_EMPTY;
    }
    if args_has(args, 'k' as i32 as u_char) != 0 {
        sc.flags |= SPAWN_KILL;
    }
    if <std::rc::Rc<std::cell::RefCell<window_pane>> as WindowPane>::spawn_process(
        &mut sc, &mut cause,
    )
    .is_none()
    {
        cmdq_error(item_handle, |out| {
            out.write_all(b"respawn pane failed: ")?;
            write_cstr(out, cause.as_deref())
        });
        drop(sc.environ.take());
        return CmdOutcome::Error;
    }
    wp.as_ref().expect("respawn pane").request_redraw(false);
    let window = wp
        .as_ref()
        .expect("respawn pane")
        .window_observer()
        .upgrade()
        .expect("respawn pane window");
    server_redraw_window_borders(&window);
    server_status_window(&window);
    drop(window);
    CmdOutcome::Normal
}
