//! Building command lists from command arguments.

use super::store::args;
use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_target_client};
use crate::src::cmd::{
    cmd_get_args_mut, cmd_get_source, cmd_list_copy, cmd_list_first, cmd_log_argv,
    cmd_template_replace_cstring,
};
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::log::{fatalx, log_bytes, log_cstr, log_debug};
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::{cmd, cmd_find_state, cmd_list, cmd_parse_input, cmdq_item};
use std::ffi::{CStr, CString};
use std::rc::Rc;

/// Box-owned prepared command; retained model references have typed owners.
pub struct args_command_state {
    cmdlist: Option<Rc<std::cell::RefCell<cmd_list>>>,
    client: Option<ClientRef>,
    cmd: Option<std::ffi::CString>,
    pi: cmd_parse_input,
    file: Option<std::ffi::CString>,
}

impl args_command_state {
    fn empty() -> Self {
        Self {
            cmdlist: None,
            client: None,
            cmd: Default::default(),
            pi: Default::default(),
            file: Default::default(),
        }
    }
}

pub unsafe fn args_make_commands_now(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    idx: u_int,
    expand: ::core::ffi::c_int,
) -> Option<Rc<std::cell::RefCell<cmd_list>>> {
    let mut state = args_make_commands_prepare(self_0.clone(), item_handle, idx, None, 0, expand);
    match args_make_commands(&mut state, &Vec::new()) {
        Ok(commands) => Some(commands),
        Err(error) => {
            cmdq_error(item_handle, |out| write_cstr(out, error.as_deref()));
            None
        }
    }
}
pub unsafe fn args_make_commands_prepare(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut idx: u_int,
    default_command: Option<&'static CStr>,
    mut wait: ::core::ffi::c_int,
    mut expand: ::core::ffi::c_int,
) -> Box<args_command_state> {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut state = Box::new(args_command_state::empty());
    let values = &(*args).values;
    let command = if let Some(value) = values.get(idx as usize) {
        if let Some(commands) = value.as_commands() {
            state.cmdlist = Some(commands.clone());
            return state;
        }
        value.as_string().expect("command string argument")
    } else {
        default_command.unwrap_or_else(|| fatalx(|out| out.write_all(b"argument out of range")))
    };
    if expand != 0 {
        state.cmd = Some(format_single_from_target_cstring(
            item_handle,
            command.as_ptr(),
        ));
    } else {
        state.cmd = Some(command.to_owned());
    }

    log_debug(format_args!(
        "{}: {}",
        "args_make_commands_prepare",
        log_cstr(state.cmd.as_deref().unwrap_or(c"(null)"))
    ));
    if wait != 0 {
        state.pi.set_item(Some(item_handle));
    }
    let (source, line) = cmd_get_source(self_0.get_unchecked());
    state.pi.line = line;
    if let Some(file) = source {
        state.file = Some(file.to_owned());
        state.pi.file = state.file.clone();
    }
    state.pi.c = tc_owner
        .as_ref()
        .map_or_else(std::rc::Weak::new, Rc::downgrade);
    state.client = tc_owner;
    state.pi.fs.copy_from(&*target);
    state
}
pub unsafe fn args_make_commands(
    state: &mut args_command_state,
    argv: &Vec<CString>,
) -> Result<Rc<std::cell::RefCell<cmd_list>>, Option<CString>> {
    let mut i: ::core::ffi::c_int = 0;
    if let Some(commands) = state.cmdlist.as_ref() {
        if argv.is_empty() {
            return Ok(Rc::clone(commands));
        }
        return Ok(cmd_list_copy(&commands.borrow(), argv));
    }
    let mut cmd = state.cmd.as_ref().expect("prepared command text").clone();
    log_debug(format_args!(
        "{}: {}",
        "args_make_commands",
        log_bytes(cmd.as_bytes())
    ));
    cmd_log_argv(argv, c"args_make_commands");
    i = 0 as ::core::ffi::c_int;
    while (i as usize) < argv.len() {
        let next = cmd_template_replace_cstring(
            cmd.as_c_str(),
            argv[i as usize].as_c_str(),
            i + 1 as ::core::ffi::c_int,
        );
        log_debug(format_args!(
            "{}: %{} {}: {}",
            "args_make_commands",
            (i + 1 as ::core::ffi::c_int) as u32,
            log_bytes(argv[i as usize].as_bytes()),
            log_bytes(next.as_bytes())
        ));
        cmd = next;
        i += 1;
    }
    log_debug(format_args!(
        "{}: {}",
        "args_make_commands",
        log_bytes(cmd.as_bytes())
    ));
    let mut pr = cmd_parse_from_string(cmd.as_c_str(), &raw mut state.pi);
    drop(cmd);
    match pr.status as ::core::ffi::c_uint {
        0 => Err(pr.error),
        1 => Ok(pr.cmdlist.take().expect("successful command parse")),
        _ => fatalx(|out| out.write_all(b"invalid parse return state")),
    }
}
impl Drop for args_command_state {
    fn drop(&mut self) {
        drop(self.cmdlist.take());
        if let Some(client) = self.client.take() {
            drop(client);
        }
    }
}

pub(crate) fn args_make_commands_get_command_cstring(state: &args_command_state) -> CString {
    if let Some(commands) = state.cmdlist.as_ref() {
        let commands = commands.borrow();
        let Some(first) = cmd_list_first(&commands) else {
            return CString::new(Vec::new()).expect("empty command name has no NUL");
        };
        return first.entry.name.to_owned();
    }
    let command = state
        .cmd
        .as_ref()
        .expect("prepared command text")
        .as_bytes();
    let n = command
        .iter()
        .position(|byte| b" ,".contains(byte))
        .unwrap_or(command.len()) as ::core::ffi::c_int;
    let prefix = if n < 0 {
        command
    } else {
        &command[..n as usize]
    };
    CString::new(prefix).expect("command prefix has no NUL")
}
