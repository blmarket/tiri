use crate::arguments::{
    args_count, args_flag_values, args_get, args_has, args_parse, args_string, args_to_vector,
};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{
    cmdq_error, cmdq_get_client, cmdq_get_state_owned, cmdq_get_target, cmdq_get_target_client,
    cmdq_insert_hook, cmdq_print,
};
use crate::environ::{environ_create, environ_put};
use crate::format::bytes::write_cstr;
use crate::format::format_single_cstring;
use crate::resize::recalculate_sizes;
use crate::server_client::Client as _;
use crate::server_fn::{
    server_redraw_session, server_redraw_session_group, server_status_session_group,
};
use crate::session::Session;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_FIND_WINDOW_INDEX;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::shared::spawn::spawn_context;
use crate::shared::spawn::{SPAWN_DETACHED, SPAWN_EMPTY, SPAWN_KILL};
use crate::shared::tree::RB_NEGINF;
use crate::shared::window::winlink;
use crate::spawn::spawn_window;
use crate::tmux::{check_name, clean_name_cstring};
use crate::window::Window as _;
use crate::window::{winlink_find_by_index, winlink_next, winlink_shuffle_up, winlinks_minmax};
use crate::winlink::winlink_window;
use std::ffi::{CStr, CString};

pub const NEW_WINDOW_TEMPLATE: &std::ffi::CStr = c"#{session_name}:#{window_index}.#{pane_index}";
pub static cmd_new_window_entry: cmd_entry = {
    cmd_entry {
        name: c"new-window",
        alias: Some(c"neww"),
        args: args_parse {
            template: c"abc:de:EF:kn:PSt:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-abdEkPS] [-c start-directory] [-e environment] [-F format] [-n window-name] [-t target-window] [shell-command [argument ...]]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_WINDOW,
            flags: CMD_FIND_WINDOW_INDEX,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_new_window_exec),
    }
};
unsafe fn cmd_new_window_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let c_owner = cmdq_get_client(Some(&*item));
    let mut c: Option<ClientRef> = c_owner.clone();
    let current = cmdq_get_state_owned(&(item));
    let target = cmdq_get_target(&item).clone();
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
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut _tc: Option<ClientRef> = tc_owner.clone();
    let session_owner = target.session_handle().expect("new-window target session");
    let s = Some(session_owner.clone());
    let mut wl: refbox::Weak<winlink> = target.winlink_handle();
    let mut new_wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut idx: ::core::ffi::c_int = target.idx;
    let mut before: ::core::ffi::c_int = 0;
    let mut count: ::core::ffi::c_int = args_count(args) as ::core::ffi::c_int;
    let mut cause: Option<std::ffi::CString> = None;
    let mut wname_owned: Option<CString> = None;
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    if args_has(args, 'E' as i32 as u_char) != 0
        && count != 0 as ::core::ffi::c_int
        && (count != 1 as ::core::ffi::c_int
            || *args_string(args, 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr())
                as ::core::ffi::c_int
                != '\0' as i32)
    {
        cmdq_error(item_handle, |out| {
            out.write_all(b"command cannot be given for empty pane")
        });
        return CmdOutcome::Error;
    }
    if let Some(name) = args_get(args, 'n' as i32 as u_char) {
        let expanded = format_single_cstring(
            Some(item_handle),
            name,
            c_owner.as_ref(),
            Some(&session_owner),
            (refbox::Weak::new()).clone(),
            None,
        );
        if !check_name(&expanded) {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid window name: ")?;
                out.write_all(expanded.as_bytes())
            });
            return CmdOutcome::Error;
        }
        wname_owned = Some(
            clean_name_cstring(expanded.as_c_str(), 0)
                .expect("check_name validated the window name"),
        );
    }
    if args_has(args, 'S' as i32 as u_char) != 0 {
        if idx != -(1 as ::core::ffi::c_int) {
            new_wl = s
                .as_ref()
                .expect("live session")
                .with_winlinks(|links| winlink_find_by_index(links, idx));
        } else if let Some(wname) = wname_owned.as_deref() {
            let expanded = format_single_cstring(
                Some(item_handle),
                wname,
                c_owner.as_ref(),
                Some(&session_owner),
                (refbox::Weak::new()).clone(),
                None,
            );
            wl = s
                .as_ref()
                .expect("live session")
                .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
            while wl.is_alive() {
                if winlink_window(&wl).expect("live window").name() == expanded {
                    if !new_wl.is_alive() {
                        new_wl = wl.clone();
                    } else {
                        cmdq_error(item_handle, |out| {
                            out.write_all(b"multiple windows named ")?;
                            write_cstr(out, wname)
                        });
                        return CmdOutcome::Error;
                    }
                }
                wl = winlink_next(&wl);
            }
        }
    }
    if new_wl.is_alive() {
        if args_has(args, 'd' as i32 as u_char) != 0 {
            return CmdOutcome::Normal;
        }
        if (s.as_ref().expect("live session")).select_winlink((new_wl).clone())
            == 0 as ::core::ffi::c_int
        {
            server_redraw_session(s.as_ref().expect("live session"));
        }
        if !c.is_none()
            && !c
                .as_ref()
                .expect("live client")
                .attached_session()
                .upgrade()
                .is_none()
        {
            winlink_window(&s.as_ref().expect("live session").current_winlink())
                .expect("live window")
                .set_latest_client(c.as_ref());
        }
        recalculate_sizes();
        return CmdOutcome::Normal;
    }
    before = args_has(args, 'b' as i32 as u_char);
    if args_has(args, 'a' as i32 as u_char) != 0 || before != 0 {
        idx = winlink_shuffle_up(&session_owner, wl.clone(), before);
        if idx == -(1 as ::core::ffi::c_int) {
            idx = target.idx;
        }
    }
    sc.item = std::rc::Rc::downgrade(item_handle);
    sc.s = std::rc::Rc::downgrade(s.as_ref().expect("live session"));
    sc.tc = tc_owner
        .as_ref()
        .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    sc.name = wname_owned.clone();
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
    sc.idx = idx;
    sc.cwd = args_get(args, 'c' as i32 as u_char).map(CStr::to_owned);
    sc.flags = 0 as ::core::ffi::c_int;
    if args_has(args, 'E' as i32 as u_char) != 0
        || count == 1 as ::core::ffi::c_int
            && *args_string(args, 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr())
                as ::core::ffi::c_int
                == '\0' as i32
    {
        sc.flags |= SPAWN_EMPTY;
    }
    if args_has(args, 'd' as i32 as u_char) != 0 {
        sc.flags |= SPAWN_DETACHED;
    }
    if args_has(args, 'k' as i32 as u_char) != 0 {
        sc.flags |= SPAWN_KILL;
    }
    new_wl = spawn_window(&mut sc, &mut cause);
    if !new_wl.is_alive() {
        cmdq_error(item_handle, |out| {
            out.write_all(b"create window failed: ")?;
            write_cstr(out, cause.as_deref())
        });
        drop(sc.environ.take());
        CmdOutcome::Error
    } else {
        if args_has(args, 'd' as i32 as u_char) == 0
            || new_wl == s.as_ref().expect("live session").current_winlink()
        {
            *current.current.borrow_mut() =
                cmd_find_state::find_from_winlink((new_wl).clone(), 0 as ::core::ffi::c_int);
            server_redraw_session_group(s.as_ref().expect("live session"));
        } else {
            server_status_session_group(s.as_ref().expect("live session"));
        }
        if args_has(args, 'P' as i32 as u_char) != 0 {
            let template = args_get(args, 'F' as i32 as u_char).unwrap_or(NEW_WINDOW_TEMPLATE);
            let cp = format_single_cstring(
                Some(item_handle),
                template,
                tc_owner.as_ref(),
                Some(&session_owner),
                (new_wl).clone(),
                (((winlink_window(&new_wl).as_ref()).expect("live window"))
                    .active_pane()
                    .upgrade())
                .as_ref(),
            );
            cmdq_print(item_handle, |out| out.write_all(cp.as_bytes()));
        }
        fs = cmd_find_state::find_from_winlink((new_wl).clone(), 0 as ::core::ffi::c_int);
        cmdq_insert_hook(Some(&session_owner), item_handle, Some(&fs), |out| {
            out.write_all(b"after-new-window")
        });
        drop(sc.environ.take());
        CmdOutcome::Normal
    }
}
