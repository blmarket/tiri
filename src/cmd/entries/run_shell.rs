use crate::src::arguments::{
    args_count, args_get, args_has, args_make_commands, args_make_commands_prepare, args_string,
};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::parse::cmd_parse_error_uppercase_first;
use crate::src::cmd::queue::{
    cmdq_append, cmdq_continue, cmdq_error, cmdq_get_client, cmdq_get_command,
    cmdq_get_target_client, cmdq_insert_after, cmdq_print,
};
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::bytes::xformat;
use crate::src::format::{format_add, format_create_from_target, format_expand_cstring};
use crate::src::job::job_run;
use crate::src::reactor::{
    evbuffer_add, evbuffer_get_length, evbuffer_new, evbuffer_pullup, evbuffer_readln,
};
use libc::strtod;

use crate::src::server_client::Client as _;

use crate::src::session::Session;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::args_command_state;
use crate::src::shared::arguments::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::CMD_FIND_CANFAIL;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::shared::event::*;
use crate::src::shared::format::format_tree;
use crate::src::shared::job::{JobCompletion, JOB_NOWAIT, JOB_SHOWSTDERR};
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionRef;
use crate::src::status::status_message_set;
use crate::src::window::Window as _;
use crate::src::window_copy::{window_copy_add, window_view_mode};
use crate::src::window_pane::WindowPane as _;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::os::unix::process::ExitStatusExt;
use std::rc::Weak;
use std::time::Duration;

pub struct cmd_run_shell_data {
    pub client: Option<ClientRef>,
    pub cmd: Option<CString>,
    pub state: Option<Box<args_command_state>>,
    pub cwd: CString,
    pub item: Weak<UnsafeCell<cmdq_item>>,
    pub wait: bool,
    pub s: Option<SessionRef>,
    pub wp_id: ::core::ffi::c_int,
    pub flags: ::core::ffi::c_int,
}
pub static cmd_run_shell_entry: cmd_entry = {
    cmd_entry {
        name: c"run-shell",
        alias: Some(c"run"),
        args: args_parse {
            template: c"bd:Ct:Es:c:",
            lower: 0 as ::core::ffi::c_int,
            upper: -(1 as ::core::ffi::c_int),
            cb: Some(
                cmd_run_shell_args_parse
            ),
        },
        usage: c"[-bCE] [-c start-directory] [-d delay] [-t target-pane] [shell-command [argument ...]]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_run_shell_exec),
    }
};
fn cmd_run_shell_args_parse(
    args: &mut args,
    _idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    if unsafe { args_has(args as *mut args, 'C' as i32 as u_char) } != 0 {
        return Ok(ARGS_PARSE_COMMANDS_OR_STRING);
    }
    Ok(ARGS_PARSE_STRING)
}
unsafe fn cmd_run_shell_print(cdata: &cmd_run_shell_data, msg: &CStr) {
    let item_owner = cdata.item.upgrade();
    if cdata.wait && item_owner.is_none() {
        return;
    }
    let mut pane = if cdata.wp_id != -1 {
        std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::find_by_id(cdata.wp_id as u32)
    } else {
        None
    };
    if pane.is_none() {
        if let Some(item) = item_owner.as_ref() {
            cmdq_print(item, |out| write_cstr(out, nullable_cstr(msg.as_ptr())));
            return;
        }
        pane = cdata
            .client
            .as_ref()
            .and_then(|client| client.attached_session().upgrade())
            .and_then(|session| {
                session
                    .current_winlink()
                    .get_unchecked()
                    .window_handle()
                    .expect("live window")
                    .active_pane()
                    .upgrade()
            });
        if pane.is_none() {
            if let Some(find) = cmd_find_state::find_from_nothing(0) {
                pane = find.pane_handle();
            }
        }
    }
    let Some(pane) = pane else {
        return;
    };
    if !pane.is_mode(&window_view_mode) {
        pane.set_mode(None, &window_view_mode, None, None, None);
    }
    window_copy_add(&pane, 1, |out| write_cstr(out, nullable_cstr(msg.as_ptr())));
}
fn cmd_run_shell_status_message(cmd: &CStr, suffix: &[u8], code: ::core::ffi::c_int) -> CString {
    let cmd = cmd.to_bytes();
    let code = code.to_string();
    let mut message = Vec::with_capacity(1 + cmd.len() + suffix.len() + code.len());
    message.push(b'\'');
    message.extend_from_slice(cmd);
    message.extend_from_slice(suffix);
    message.extend_from_slice(code.as_bytes());
    CString::new(message).expect("run-shell command and status text contain no NUL")
}

unsafe fn cmd_run_shell_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let _tc: Option<ClientRef> = tc_owner.clone();
    let mut s: Option<SessionRef> = (*target).session_handle();
    let pane_owner = (*target).pane_handle();
    let mut delay: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cmd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut d: ::core::ffi::c_double = 0.;
    let mut tv = Duration::ZERO;
    let mut end: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut key: [u8; 16] = [0; 16];
    let mut i: u_int = 0;
    let mut wait: ::core::ffi::c_int =
        (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    delay =
        args_get(&*(args), 'd' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !delay.is_null() {
        d = strtod(delay, &raw mut end);
        if *end as ::core::ffi::c_int != '\0' as i32 {
            cmdq_error(item_handle, |out| {
                out.write_all(b"invalid delay time: ")?;
                write_cstr(out, nullable_cstr(delay))
            });
            return CMD_RETURN_ERROR;
        }
    } else if args_count(args) == 0 as u_int {
        return CMD_RETURN_NORMAL;
    }
    let cwd = if args_has(args, 'c' as i32 as u_char) != 0 {
        args_get(&*(args), 'c' as i32 as u_char).map(CStr::to_owned)
    } else {
        ClientRef::working_directory(c.as_ref(), s.as_ref())
    };
    let mut cdata = Box::new(cmd_run_shell_data {
        client: None,
        cmd: None,
        state: None,
        cwd: cwd.expect("shell working directory"),
        item: Weak::new(),
        wait: wait != 0,
        s: None,
        wp_id: 0,
        flags: 0,
    });
    if args_has(args, 'C' as i32 as u_char) == 0 {
        cmd =
            args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
        if !cmd.is_null() {
            let mut ft_owner = format_create_from_target(item_handle);
            ft = &raw mut *ft_owner;
            i = 1 as u_int;
            while i < args_count(args) {
                xformat(&mut key, format_args!("{}", i as u32));
                format_add(
                    ft,
                    CStr::from_bytes_until_nul(&key).expect("xformat output is terminated"),
                    |out| {
                        write_cstr(
                            out,
                            nullable_cstr(
                                args_string(&mut *(args), i)
                                    .map_or(std::ptr::null(), |value| value.as_ptr()),
                            ),
                        )
                    },
                );
                i = i.wrapping_add(1);
            }
            cdata.cmd = Some(format_expand_cstring(ft, cmd));
            drop(ft_owner);
        }
    } else {
        cdata.state = Some(args_make_commands_prepare(
            self_0.clone(),
            item_handle,
            0 as u_int,
            None,
            wait,
            1 as ::core::ffi::c_int,
        ));
    }
    if let Some(pane) = pane_owner
        .as_ref()
        .filter(|_| args_has(args, 't' as i32 as u_char) != 0)
    {
        cdata.wp_id = pane.id() as ::core::ffi::c_int;
    } else {
        cdata.wp_id = -(1 as ::core::ffi::c_int);
    }
    if wait != 0 {
        cdata.client = c_owner.clone();
        cdata.item = std::rc::Rc::downgrade(item_handle);
    } else {
        cdata.client = tc_owner.clone();
        cdata.flags |= JOB_NOWAIT;
    }
    if args_has(args, 'E' as i32 as u_char) != 0 {
        cdata.flags |= JOB_SHOWSTDERR;
    }
    cdata.s = if s.is_none() { None } else { s.clone() };
    if !delay.is_null() {
        // The pinned tmux build treats negative, nonfinite and out-of-range
        // delays as expired. Rust's saturating float casts would instead turn
        // positive infinity into an enormous timer. The upper bound is
        // exclusive because time_t::MAX rounds up when converted to f64.
        if (0.0..time_t::MAX as f64).contains(&d) {
            tv = Duration::from_secs(d as u64)
                + Duration::from_micros((d.fract() * 1_000_000.0) as u64);
        }
    }
    if wait == 0 {
        crate::src::cmd::queue::cmdq_background(tv, move || unsafe { cmd_run_shell_timer(cdata) })
            .expect("arm background run-shell delay");
        return CMD_RETURN_NORMAL;
    }
    let timer =
        Timer::new(tv, move || unsafe { cmd_run_shell_timer(cdata) }).expect("arm run-shell delay");
    crate::src::cmd::queue::cmdq_set_wait_timer(&mut *item, timer);
    CMD_RETURN_WAIT
}
unsafe fn cmd_run_shell_timer(mut cdata: Box<cmd_run_shell_data>) {
    let item_owner = cdata.item.upgrade();
    if cdata.wait && item_owner.is_none() {
        return;
    }
    if let Some(item) = item_owner.as_ref() {
        crate::src::cmd::queue::cmdq_clear_wait_timer(&mut *item.get());
    }
    let c = cdata.client.as_ref();
    let cmd = cdata.cmd.as_deref();
    let item = item_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let new_item_allocation;
    if cdata.state.is_none() {
        if cmd.is_none() {
            if cdata.wait {
                cmdq_continue(item_owner.as_ref().expect("live command queue item"));
            }
            return;
        }
        let job = job_run(
            cmd,
            &Vec::new(),
            None,
            cdata.s.as_ref(),
            Some(cdata.cwd.as_c_str()),
            None,
            None,
            None,
            cdata.flags,
            -(1 as ::core::ffi::c_int),
            -(1 as ::core::ffi::c_int),
        );
        if job.is_empty() {
            if !cdata.wait {
                status_message_set(
                    c,
                    -(1 as ::core::ffi::c_int),
                    1 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    |out| {
                        out.write_all(b"failed to run command: ")?;
                        write_cstr(out, cmd.unwrap())
                    },
                );
            } else {
                cmdq_error(
                    item_owner.as_ref().expect("live command queue item"),
                    |out| {
                        out.write_all(b"failed to run command: ")?;
                        write_cstr(out, cmd.unwrap())
                    },
                );
                cmdq_continue(item_owner.as_ref().expect("live command queue item"));
            }
        } else {
            // job_run does not dispatch callbacks before returning. Transfer the
            // record only after startup succeeds; dropping the callback also
            // releases it if the job is cancelled before completion.
            job.try_borrow_mut()
                .expect("newly registered job")
                .completecb = Some(Box::new(move |completion| unsafe {
                cmd_run_shell_callback(completion, &cdata);
            }));
        }
        return;
    }
    match args_make_commands(
        cdata.state.as_deref_mut().expect("prepared command state"),
        &Vec::new(),
    ) {
        Err(mut error) => {
            if !cdata.wait {
                cmd_parse_error_uppercase_first(&mut error);
            }
            let error_ptr = error
                .as_ref()
                .map_or(::core::ptr::null(), |cause| cause.as_ptr());
            if !cdata.wait {
                status_message_set(
                    c,
                    -(1 as ::core::ffi::c_int),
                    1 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    0 as ::core::ffi::c_int,
                    |out| write_cstr(out, nullable_cstr(error_ptr)),
                );
            } else {
                cmdq_error(
                    item_owner.as_ref().expect("live command queue item"),
                    |out| write_cstr(out, nullable_cstr(error_ptr)),
                );
            }
        }
        Ok(commands) if !cdata.wait => {
            new_item_allocation = cmdq_get_command(&commands, None);
            cmdq_append(c, new_item_allocation);
            drop(commands);
        }
        Ok(commands) => {
            new_item_allocation = cmdq_get_command(&commands, (*item).state.as_ref());
            cmdq_insert_after(
                item_owner.as_ref().expect("live command queue item"),
                new_item_allocation,
            );
            drop(commands);
        }
    }
    if cdata.wait {
        cmdq_continue(item_owner.as_ref().expect("live command queue item"));
    }
}
unsafe fn cmd_run_shell_callback(completion: JobCompletion, cdata: &cmd_run_shell_data) {
    let item_owner = cdata.item.upgrade();
    if cdata.wait && item_owner.is_none() {
        return;
    }
    let mut event = evbuffer_new();
    if !completion.output.is_empty() {
        evbuffer_add(&mut event, &completion.output);
    }
    let item = item_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let cmd = cdata
        .cmd
        .as_ref()
        .expect("run-shell callback requires a command")
        .as_c_str();
    let mut msg: Option<CString> = None;
    let mut size: size_t = 0;
    let mut retcode: ::core::ffi::c_int = 0;
    while let Some(line) = evbuffer_readln(&mut event) {
        cmd_run_shell_print(cdata, CStr::from_ptr(line.as_ptr().cast()));
    }
    size = evbuffer_get_length(&event);
    if size != 0 as size_t {
        let mut partial_line = evbuffer_pullup(&mut event, -1).unwrap_or_default().to_vec();
        partial_line.push(0);
        cmd_run_shell_print(cdata, CStr::from_ptr(partial_line.as_ptr().cast()));
    }
    if let Some(code) = completion.status.code() {
        retcode = code;
        if retcode != 0 as ::core::ffi::c_int {
            msg = Some(cmd_run_shell_status_message(cmd, b"' returned ", retcode));
        }
    } else if let Some(signal) = completion.status.signal() {
        retcode = signal;
        msg = Some(cmd_run_shell_status_message(
            cmd,
            b"' terminated by signal ",
            retcode,
        ));
        retcode += 128 as ::core::ffi::c_int;
    }
    if let Some(msg) = msg.as_ref() {
        cmd_run_shell_print(cdata, msg);
    }
    if cdata.wait {
        if let Some(client) = cmdq_get_client((item).as_ref()) {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(retcode);
            }
        }
        cmdq_continue(item_owner.as_ref().expect("live command queue item"));
    }
}
impl Drop for cmd_run_shell_data {
    fn drop(&mut self) {
        unsafe {
            if let Some(session) = self.s.take() {
                (session).release(c"cmd_run_shell_data::drop");
            }
            if let Some(client) = self.client.take() {
                (client).release();
            }
            drop(self.state.take());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_wait_skips_job_completion() {
        let data = cmd_run_shell_data {
            client: None,
            cmd: None,
            state: None,
            cwd: c"/".to_owned(),
            item: Weak::new(),
            wait: true,
            s: None,
            wp_id: -1,
            flags: 0,
        };
        unsafe {
            cmd_run_shell_callback(
                JobCompletion {
                    status: Default::default(),
                    output: Vec::new(),
                },
                &data,
            );
        }
    }
}
