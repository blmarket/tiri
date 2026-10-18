use crate::src::arguments::{
    args, args_count, args_flag_values, args_get, args_has, args_parse, args_to_vector,
};
use crate::src::cfg::{cfg_finished, cfg_show_causes};
use crate::src::cmd::entries::attach_session::cmd_attach_session;
use crate::src::cmd::queue::{
    cmdq_error, cmdq_get_client, cmdq_get_flags, cmdq_get_state_owned, cmdq_insert_hook, cmdq_print,
};
use crate::src::cmd::{cmd_get_args_mut, cmd_get_entry};
use crate::src::compat::strtonum::strtonum;
use crate::src::environ::{environ_create, environ_put, environ_update};
use crate::src::events::events_fire_session;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_cstring;
use crate::src::options::{options_get_number, options_get_string, options_set_string};
use crate::src::server_client::Client as _;
use libc::sscanf;

use crate::src::options::options;
use crate::src::session::Session;
use crate::src::session::{session_group_add, session_group_synchronize_to, session_groups};
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::{CLIENT_ATTACHED, CLIENT_CONTROL};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::command::{
    CMDQ_STATE_REPEAT, CMD_FIND_CANFAIL, CMD_STARTSERVER, CMD_TARGET_SESSION_USAGE,
};
use crate::src::shared::environment::environ;
use crate::src::shared::limits::USHRT_MAX;
use crate::src::shared::session::SessionRef;
use crate::src::shared::spawn::spawn_context;
use crate::src::shared::terminal::*;
use crate::src::shared::tree::RB_NEGINF;
use crate::src::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::src::spawn::spawn_window;
use crate::src::text::utf8::utf8_stravis_cstring;
use crate::src::tmux::{check_name, clean_name_cstring, with_global_s_options};
use crate::src::window::winlinks_minmax;
use std::ffi::{CStr, CString};

pub const NEW_SESSION_TEMPLATE: &std::ffi::CStr = c"#{session_name}:";
pub static cmd_new_session_entry: cmd_entry = {
    cmd_entry {
        name: c"new-session",
        alias: Some(c"new"),
        args: args_parse {
            template: c"Ac:dDe:EF:f:n:Ps:t:x:Xy:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: None,
        },
        usage: c"[-AdDEPX] [-c start-directory] [-e environment] [-F format] [-f flags] [-n window-name] [-s session-name] [-t target-session] [-x width] [-y height] [shell-command [argument ...]]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_SESSION,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_STARTSERVER,
        exec: Some(cmd_new_session_exec),
    }
};
pub static cmd_has_session_entry: cmd_entry = {
    cmd_entry {
        name: c"has-session",
        alias: Some(c"has"),
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
        exec: Some(cmd_new_session_exec),
    }
};
unsafe fn cmd_new_session_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let mut queue_client_ptr: Option<ClientRef> = queue_client.clone();
    let mut current_block: u64;
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let current = cmdq_get_state_owned(&*(item));
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let mut s: Option<SessionRef> = None;
    let mut as_0: Option<SessionRef> = None;
    let mut groupwith: Option<SessionRef> = None;
    let mut groupwith_owner = None;
    let mut env: Option<Box<environ>> = None;
    let mut oo: Option<Box<options>> = None;
    let mut tio: termios = unsafe { ::core::mem::zeroed() };
    let mut tiop: *mut termios = ::core::ptr::null_mut::<termios>();
    let mut sg = crate::src::shared::session::SessionGroupWeak::new();
    let mut group: Option<&CStr> = None;
    let mut cause: Option<std::ffi::CString> = None;
    let mut cwd: Option<CString> = None;
    let mut wname_owned: Option<CString> = None;
    let mut sname_owned: Option<CString> = None;
    let mut prefix: Option<CString> = None;
    let mut detached: ::core::ffi::c_int = 0;
    let mut already_attached: ::core::ffi::c_int = 0;
    let mut is_control: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut sx: u_int = 0;
    let mut sy: u_int = 0;
    let mut dsx: u_int = 0;
    let mut dsy: u_int = 0;
    let mut count: u_int = args_count(&*args);
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
    let mut retval: CmdOutcome = CmdOutcome::Normal;
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    if std::ptr::eq(
        cmd_get_entry(self_0.get_unchecked()),
        &cmd_has_session_entry,
    ) {
        return CmdOutcome::Normal;
    }
    if args_has(&*args, 't' as i32 as u_char) != 0
        && (count != 0 as u_int || args_has(&*args, 'n' as i32 as u_char) != 0)
    {
        cmdq_error(item_handle, |out| {
            out.write_all(b"command or window name given with target")
        });
        return CmdOutcome::Error;
    }
    if let Some(tmp) = args_get(&*(args), 'n' as i32 as u_char) {
        let ename = format_single_cstring(
            Some(item_handle),
            tmp.as_ptr(),
            c.as_ref(),
            None,
            (refbox::Weak::new()).clone(),
            None,
        );
        if !check_name(&ename) {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid window name: ")?;
                out.write_all(ename.as_bytes())
            });
            return CmdOutcome::Error;
        }
        wname_owned = Some(
            clean_name_cstring(ename.as_c_str(), 0).expect("check_name validated the window name"),
        );
    }
    if let Some(tmp) = args_get(&*(args), 's' as i32 as u_char) {
        let ename = format_single_cstring(
            Some(item_handle),
            tmp.as_ptr(),
            c.as_ref(),
            None,
            (refbox::Weak::new()).clone(),
            None,
        );
        if !check_name(&ename) {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid session name: ")?;
                out.write_all(ename.as_bytes())
            });
            current_block = 5193972633326621385;
        } else {
            sname_owned = Some(
                clean_name_cstring(ename.as_c_str(), 0)
                    .expect("check_name validated the session name"),
            );
            current_block = 10043043949733653460;
        }
    } else {
        current_block = 10043043949733653460;
    }
    if current_block == 10043043949733653460 {
        if args_has(&*args, 'A' as i32 as u_char) != 0 {
            if let Some(sname) = sname_owned.as_deref() {
                as_0 = crate::src::session::sessions::find(sname);
            } else {
                as_0 = (*target).session_handle();
            }
            if let Some(as_0_value) = as_0.as_ref() {
                retval = cmd_attach_session(
                    item_handle,
                    Some(&*(as_0_value.name())),
                    args_has(&*args, 'D' as i32 as u_char),
                    args_has(&*args, 'X' as i32 as u_char),
                    0 as ::core::ffi::c_int,
                    args_get(&*(args), 'c' as i32 as u_char),
                    args_has(&*args, 'E' as i32 as u_char),
                    args_get(&*(args), 'f' as i32 as u_char),
                );
                return retval;
            }
        }
        if sname_owned
            .as_deref()
            .is_some_and(|sname| crate::src::session::sessions::find(sname).is_some())
        {
            cmdq_error(item_handle, |out| {
                out.write_all(b"duplicate session: ")?;
                write_cstr(out, sname_owned.as_deref())
            });
        } else {
            group = args_get(&*(args), 't' as i32 as u_char);
            if let Some(group) = group {
                groupwith_owner = (*target).s.upgrade();
                groupwith = groupwith_owner.clone();
                if groupwith.is_none() {
                    sg = session_groups::find(group);
                } else {
                    sg = crate::src::session::session_groups::for_session(
                        &groupwith
                            .as_ref()
                            .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
                    );
                }
                if sg.is_alive() {
                    prefix = Some(crate::src::session::session_group_name(&sg));
                    current_block = 6717214610478484138;
                } else if let Some(groupwith_value) = groupwith.as_ref() {
                    prefix = Some(groupwith_value.name().clone());
                    current_block = 6717214610478484138;
                } else if !check_name(group) {
                    cmdq_error(item_handle, |out| {
                        out.write_all(b"invalid session group name: ")?;
                        write_cstr(out, group)
                    });
                    current_block = 5193972633326621385;
                } else {
                    prefix = Some(utf8_stravis_cstring(
                        group,
                        VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL,
                    ));
                    current_block = 6717214610478484138;
                }
            } else {
                current_block = 6717214610478484138;
            }
            match current_block {
                5193972633326621385 => {}
                _ => {
                    detached = args_has(&*args, 'd' as i32 as u_char);
                    match &c {
                        None => detached = 1 as ::core::ffi::c_int,
                        Some(client) if client.flags() & CLIENT_CONTROL as uint64_t != 0 => {
                            is_control = 1 as ::core::ffi::c_int;
                        }
                        Some(_) => {}
                    }
                    already_attached = 0 as ::core::ffi::c_int;
                    if c.as_ref()
                        .is_some_and(|client| client.attached_session().upgrade().is_some())
                    {
                        already_attached = 1 as ::core::ffi::c_int;
                    }
                    if let Some(tmp) = args_get(&*(args), 'c' as i32 as u_char) {
                        cwd = Some(format_single_cstring(
                            Some(item_handle),
                            tmp.as_ptr(),
                            c.as_ref(),
                            None,
                            (refbox::Weak::new()).clone(),
                            None,
                        ));
                    } else {
                        cwd = ClientRef::working_directory(c.as_ref(), None);
                    }
                    if detached == 0
                        && already_attached == 0
                        && c.as_ref().expect("live client").has_input_fd()
                        && !c.as_ref().expect("live client").flags() & CLIENT_CONTROL as uint64_t
                            != 0
                    {
                        if queue_client_ptr.as_ref().expect("live client").is_nested() {
                            cmdq_error(item_handle, |out| {
                                out.write_all(
                                    b"sessions should be nested with care, unset $TMUX to force",
                                )
                            });
                            current_block = 5193972633326621385;
                        } else {
                            tio = c.as_ref().expect("live client").capture_termios();
                            tiop = &raw mut tio;
                            current_block = 6545907279487748450;
                        }
                    } else {
                        tiop = ::core::ptr::null_mut::<termios>();
                        current_block = 6545907279487748450;
                    }
                    match current_block {
                        5193972633326621385 => {}
                        _ => {
                            if detached == 0 && already_attached == 0 {
                                if let Err(open_error) =
                                    (c_owner.as_ref().expect("terminal client")).open_terminal()
                                {
                                    cmdq_error(item_handle, |out| {
                                        out.write_all(b"open terminal failed: ")?;
                                        out.write_all(open_error.as_bytes())
                                    });
                                    current_block = 5193972633326621385;
                                } else {
                                    current_block = 5181772461570869434;
                                }
                            } else {
                                current_block = 5181772461570869434;
                            }
                            match current_block {
                                5193972633326621385 => {}
                                _ => {
                                    if args_has(&*args, 'x' as i32 as u_char) != 0 {
                                        let tmp = args_get(&*(args), 'x' as i32 as u_char)
                                            .expect("-x has a value");
                                        if tmp == c"-" {
                                            if let Some(c_value) = c.as_ref() {
                                                dsx = c_value.terminal_size().0;
                                            } else {
                                                dsx = 80 as u_int;
                                            }
                                            current_block = 5873035170358615968;
                                        } else {
                                            let parsed = strtonum(
                                                tmp,
                                                1 as ::core::ffi::c_longlong,
                                                USHRT_MAX as ::core::ffi::c_longlong,
                                            );
                                            dsx = parsed.unwrap_or(0) as u_int;
                                            if let Err(errstr) = parsed {
                                                cmdq_error(item_handle, |out| {
                                                    out.write_all(b"width ")?;
                                                    write_cstr(out, errstr)
                                                });
                                                current_block = 5193972633326621385;
                                            } else {
                                                current_block = 5873035170358615968;
                                            }
                                        }
                                    } else {
                                        dsx = 80 as u_int;
                                        current_block = 5873035170358615968;
                                    }
                                    match current_block {
                                        5193972633326621385 => {}
                                        _ => {
                                            if args_has(&*args, 'y' as i32 as u_char) != 0 {
                                                let tmp = args_get(&*(args), 'y' as i32 as u_char)
                                                    .expect("-y has a value");
                                                if tmp == c"-" {
                                                    if let Some(c_value) = c.as_ref() {
                                                        dsy = c_value.terminal_size().1;
                                                    } else {
                                                        dsy = 24 as u_int;
                                                    }
                                                    current_block = 15855550149339537395;
                                                } else {
                                                    let parsed = strtonum(
                                                        tmp,
                                                        1 as ::core::ffi::c_longlong,
                                                        USHRT_MAX as ::core::ffi::c_longlong,
                                                    );
                                                    dsy = parsed.unwrap_or(0) as u_int;
                                                    if let Err(errstr) = parsed {
                                                        cmdq_error(item_handle, |out| {
                                                            out.write_all(b"height ")?;
                                                            write_cstr(out, errstr)
                                                        });
                                                        current_block = 5193972633326621385;
                                                    } else {
                                                        current_block = 15855550149339537395;
                                                    }
                                                }
                                            } else {
                                                dsy = 24 as u_int;
                                                current_block = 15855550149339537395;
                                            }
                                            match current_block {
                                                5193972633326621385 => {}
                                                _ => {
                                                    if detached == 0 && is_control == 0 {
                                                        sx = c
                                                            .as_ref()
                                                            .expect("live client")
                                                            .terminal_size()
                                                            .0;
                                                        sy = c
                                                            .as_ref()
                                                            .expect("live client")
                                                            .terminal_size()
                                                            .1;
                                                        if sy > 0 as u_int
                                                            && with_global_s_options(|oo| {
                                                                options_get_number(oo, c"status")
                                                            }) != 0
                                                        {
                                                            sy = sy.wrapping_sub(1);
                                                        }
                                                    } else {
                                                        let default_size =
                                                            with_global_s_options(|oo| {
                                                                options_get_string(
                                                                    oo,
                                                                    c"default-size",
                                                                )
                                                            });
                                                        if sscanf(
                                                            default_size.as_ptr(),
                                                            c"%ux%u".as_ptr(),
                                                            &raw mut sx,
                                                            &raw mut sy,
                                                        ) != 2 as ::core::ffi::c_int
                                                        {
                                                            sx = dsx;
                                                            sy = dsy;
                                                        } else {
                                                            if args_has(
                                                                &*args,
                                                                'x' as i32 as u_char,
                                                            ) != 0
                                                            {
                                                                sx = dsx;
                                                            }
                                                            if args_has(
                                                                &*args,
                                                                'y' as i32 as u_char,
                                                            ) != 0
                                                            {
                                                                sy = dsy;
                                                            }
                                                        }
                                                    }
                                                    if sx == 0 as u_int {
                                                        sx = 1 as u_int;
                                                    }
                                                    if sy == 0 as u_int {
                                                        sy = 1 as u_int;
                                                    }
                                                    oo = Some(crate::src::options::options_create_owned(Some(crate::src::options::OptionsScope::GlobalSession)));
                                                    if args_has(&*args, 'x' as i32 as u_char) != 0
                                                        || args_has(&*args, 'y' as i32 as u_char)
                                                            != 0
                                                    {
                                                        if args_has(&*args, 'x' as i32 as u_char)
                                                            == 0
                                                        {
                                                            dsx = sx;
                                                        }
                                                        if args_has(&*args, 'y' as i32 as u_char)
                                                            == 0
                                                        {
                                                            dsy = sy;
                                                        }
                                                        options_set_string(
                                                            &mut **oo
                                                                .as_mut()
                                                                .expect("new session options"),
                                                            c"default-size",
                                                            0 as ::core::ffi::c_int,
                                                            |out| {
                                                                write!(
                                                                    out,
                                                                    "{}x{}",
                                                                    (dsx) as u32,
                                                                    (dsy) as u32
                                                                )
                                                            },
                                                        );
                                                    }
                                                    env = Some(environ_create());
                                                    if let Some(client) = c.as_ref().filter(|_| {
                                                        args_has(&*args, 'E' as i32 as u_char) == 0
                                                    }) {
                                                        let client_environment = client
                                                            .with_environment(|env| {
                                                                env.expect("client environment")
                                                                    .clone()
                                                            });
                                                        with_global_s_options(|oo| {
                                                            environ_update(
                                                                oo,
                                                                &client_environment,
                                                                env.as_deref_mut()
                                                                    .expect("new environment"),
                                                            );
                                                        });
                                                    }
                                                    for av in args_flag_values(
                                                        &*args,
                                                        'e' as i32 as u_char,
                                                    ) {
                                                        environ_put(
                                                            env.as_deref_mut()
                                                                .expect("environment"),
                                                            av.as_string()
                                                                .expect("string argument"),
                                                            0 as ::core::ffi::c_int,
                                                        );
                                                    }
                                                    let session_owner = crate::src::shared::session::SessionRef::create(prefix.as_deref(), sname_owned.as_deref(), cwd.as_deref().expect("client working directory is set at identify"), env.take()
                                                            .expect("new session environment"), oo.take(), tiop.as_ref());
                                                    s = Some(session_owner.clone());
                                                    sc.item = std::rc::Rc::downgrade(item_handle);
                                                    sc.s = std::rc::Rc::downgrade(&session_owner);
                                                    if detached == 0 {
                                                        sc.tc = c_owner.as_ref().map_or_else(
                                                            std::rc::Weak::new,
                                                            std::rc::Rc::downgrade,
                                                        );
                                                    }
                                                    sc.name = wname_owned.clone();
                                                    argv_owner = args_to_vector(&*args);
                                                    sc.argv = argv_owner;
                                                    sc.idx = -(1 as ::core::ffi::c_int);
                                                    sc.cwd =
                                                        args_get(&*(args), 'c' as i32 as u_char)
                                                            .map(CStr::to_owned);
                                                    sc.flags = 0 as ::core::ffi::c_int;
                                                    if !spawn_window(&mut sc, &mut cause).is_alive()
                                                    {
                                                        s.clone().expect("live session").destroy(
                                                            (0 as ::core::ffi::c_int) != 0,
                                                            c"cmd_new_session_exec",
                                                        );
                                                        cmdq_error(item_handle, |out| {
                                                            out.write_all(
                                                                b"create window failed: ",
                                                            )?;
                                                            write_cstr(out, cause.as_deref())
                                                        });
                                                    } else {
                                                        if let Some(group) = group {
                                                            if !sg.is_alive() {
                                                                if let Some(groupwith_value) =
                                                                    groupwith.as_ref()
                                                                {
                                                                    sg = session_groups::find_or_create(
                                                                        groupwith_value.name().as_c_str(),
                                                                    );
                                                                    session_group_add(
                                                                        &sg, groupwith_owner.as_ref().expect("retained group session"),
                                                                    );
                                                                } else {
                                                                    sg = session_groups::find_or_create(
                                                                        group,
                                                                    );
                                                                }
                                                            }
                                                            session_group_add(&sg, &session_owner);
                                                            session_group_synchronize_to(
                                                                &s.clone().expect("live session"),
                                                            );
                                                            s.clone()
                                                                .expect("live session")
                                                                .select_index(
                                                                    (s.as_ref()
                                                                        .expect("live session")
                                                                        .with_winlinks(|links| {
                                                                            winlinks_minmax(
                                                                                links, RB_NEGINF,
                                                                            )
                                                                        }))
                                                                    .get_unchecked()
                                                                    .idx,
                                                                );
                                                        }
                                                        events_fire_session(
                                                            c"session-created",
                                                            s.clone().expect("live session"),
                                                        );
                                                        if detached == 0 {
                                                            if args_has(
                                                                &*args,
                                                                'f' as i32 as u_char,
                                                            ) != 0
                                                            {
                                                                c.clone()
                                                                    .expect("live client")
                                                                    .parse_flags(
                                                                        args_get(
                                                                            &*(args),
                                                                            'f' as i32 as u_char,
                                                                        )
                                                                        .expect("-f has a value"),
                                                                    );
                                                            }
                                                            if already_attached == 0 {
                                                                if !c
                                                                    .as_ref()
                                                                    .expect("live client")
                                                                    .flags()
                                                                    & CLIENT_CONTROL as uint64_t
                                                                    != 0
                                                                {
                                                                    c.as_ref()
                                                                        .expect("live client")
                                                                        .send_ready();
                                                                }
                                                            } else if !c
                                                                .as_ref()
                                                                .expect("live client")
                                                                .attached_session()
                                                                .upgrade()
                                                                .is_none()
                                                            {
                                                                c.as_ref()
                                                                    .expect("live client")
                                                                    .remember_session();
                                                            }
                                                            c.clone()
                                                                .expect("live client")
                                                                .set_session(s.as_ref());
                                                            if !cmdq_get_flags(&*(item))
                                                                & CMDQ_STATE_REPEAT
                                                                != 0
                                                            {
                                                                c.clone()
                                                                    .expect("live client")
                                                                    .set_key_table(None);
                                                            }
                                                        }
                                                        if args_has(&*args, 'P' as i32 as u_char)
                                                            != 0
                                                        {
                                                            let template = args_get(
                                                                &*(args),
                                                                'F' as i32 as u_char,
                                                            )
                                                            .unwrap_or(NEW_SESSION_TEMPLATE);
                                                            let cp = format_single_cstring(
                                                                Some(item_handle),
                                                                template.as_ptr(),
                                                                c.as_ref(),
                                                                s.as_ref(),
                                                                (s.as_ref()
                                                                    .expect("live session")
                                                                    .current_winlink())
                                                                .clone(),
                                                                None,
                                                            );
                                                            cmdq_print(item_handle, |out| {
                                                                out.write_all(cp.as_bytes())
                                                            });
                                                        }
                                                        if detached == 0 {
                                                            c.as_ref()
                                                                .expect("live client")
                                                                .update_flags(
                                                                    CLIENT_ATTACHED as uint64_t,
                                                                    0,
                                                                );
                                                        }
                                                        if args_has(&*args, 'd' as i32 as u_char)
                                                            == 0
                                                        {
                                                            *current.current.borrow_mut() =
                                                                cmd_find_state::find_from_session(
                                                                    &s.clone()
                                                                        .expect("live session"),
                                                                    0 as ::core::ffi::c_int,
                                                                );
                                                        }
                                                        fs = cmd_find_state::find_from_session(
                                                            &s.clone().expect("live session"),
                                                            0 as ::core::ffi::c_int,
                                                        );
                                                        cmdq_insert_hook(
                                                            s.as_ref(),
                                                            item_handle,
                                                            &raw mut fs,
                                                            |out| {
                                                                out.write_all(b"after-new-session")
                                                            },
                                                        );
                                                        if cfg_finished != 0 {
                                                            cfg_show_causes(s.as_ref());
                                                        }
                                                        return CmdOutcome::Normal;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    CmdOutcome::Error
}
