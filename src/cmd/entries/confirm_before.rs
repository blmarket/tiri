use crate::src::arguments::{args_get, args_has, args_make_commands_now};
use crate::src::cmd::queue::{
    cmdq_append, cmdq_continue, cmdq_error, cmdq_get_client, cmdq_get_command,
    cmdq_get_target_client, cmdq_insert_after,
};
use crate::src::cmd::{cmd_get_args_mut, cmd_list_first};
use crate::src::format::bytes::nullable_cstr;
use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::client::CLIENT_DEAD;
use crate::src::shared::command::CMD_CLIENT_TFLAG;
use crate::src::shared::command::*;
use crate::src::shared::command::{
    cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmd_list, cmdq_item,
};
use crate::src::shared::prompt::*;
use crate::src::shared::prompt::{prompt_result, PROMPT_CLOSE, PROMPT_SINGLE};
use crate::src::status::status_prompt_set;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::Rc;

pub struct cmd_confirm_before_data {
    pub item: std::rc::Weak<UnsafeCell<cmdq_item>>,
    pub cmdlist: Rc<std::cell::RefCell<cmd_list>>,
    pub confirm_key: u_char,
    pub default_yes: ::core::ffi::c_int,
}
pub static cmd_confirm_before_entry: cmd_entry = {
    cmd_entry {
        name: c"confirm-before",
        alias: Some(c"confirm"),
        args: args_parse {
            template: c"bc:p:t:y",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(cmd_confirm_before_args_parse),
        },
        usage: c"[-by] [-c confirm-key] [-p prompt] [-t target-client] command",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        flags: CMD_CLIENT_TFLAG,
        exec: Some(cmd_confirm_before_exec),
    }
};
fn cmd_confirm_before_args_parse(
    _args: &mut args,
    _idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    Ok(ARGS_PARSE_COMMANDS_OR_STRING)
}
unsafe fn cmd_confirm_before_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut confirm_key: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut prompt: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cmd: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut wait: ::core::ffi::c_int =
        (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let Some(cmdlist) = args_make_commands_now(self_0.clone(), item_handle, 0, 1) else {
        return CMD_RETURN_ERROR;
    };
    let mut cdata = Box::new(cmd_confirm_before_data {
        item: std::rc::Weak::new(),
        cmdlist,
        confirm_key: 0,
        default_yes: 0,
    });
    if wait != 0 {
        cdata.item = std::rc::Rc::downgrade(item_handle);
    }
    cdata.default_yes = args_has(args, 'y' as i32 as u_char);
    confirm_key =
        args_get(&*(args), 'c' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !confirm_key.is_null() {
        if *confirm_key.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
            == '\0' as i32
            && *confirm_key.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
                > 31 as ::core::ffi::c_int
            && (*confirm_key.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int)
                < 127 as ::core::ffi::c_int
        {
            cdata.confirm_key = *confirm_key.offset(0 as ::core::ffi::c_int as isize) as u_char;
        } else {
            cmdq_error(item_handle, |out| out.write_all(b"invalid confirm key"));
            return CMD_RETURN_ERROR;
        }
    } else {
        cdata.confirm_key = 'y' as i32 as u_char;
    }
    prompt =
        args_get(&*(args), 'p' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    let new_prompt = if !prompt.is_null() {
        let mut bytes = CStr::from_ptr(prompt).to_bytes().to_vec();
        bytes.push(b' ');
        CString::new(bytes).expect("C string prompt contains no interior NUL")
    } else {
        cmd = cmd_list_first(&cdata.cmdlist.borrow())
            .expect("confirmation command")
            .entry
            .name
            .as_ptr();
        let mut bytes = b"Confirm '".to_vec();
        bytes.extend_from_slice(CStr::from_ptr(cmd).to_bytes());
        bytes.extend_from_slice(b"'? (");
        bytes.push(cdata.confirm_key);
        bytes.extend_from_slice(b"/n) ");
        CString::new(bytes).expect("C string command and validated key contain no interior NUL")
    };
    let inputcb = cdata.into_callback();
    status_prompt_set(
        &tc.clone().expect("live client"),
        target.as_ref(),
        &new_prompt,
        nullable_cstr(::core::ptr::null::<::core::ffi::c_char>()),
        inputcb,
        None,
        PROMPT_SINGLE,
        PROMPT_TYPE_COMMAND,
    );
    drop(new_prompt);
    if wait == 0 {
        return CMD_RETURN_NORMAL;
    }
    CMD_RETURN_WAIT
}
unsafe fn cmd_confirm_before_callback(
    c_owner: &ClientRef,
    cdata: &cmd_confirm_before_data,
    s: Option<&CStr>,
) -> prompt_result {
    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let item_owner = cdata.item.upgrade();
    let item: *mut cmdq_item = item_owner
        .as_ref()
        .map_or(std::ptr::null_mut(), |owner| owner.get());
    let new_item_allocation;
    let mut retcode: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
    if !(c.as_ref().expect("live client").flags() & CLIENT_DEAD as uint64_t != 0) {
        if let Some(s) = s {
            let bytes = s.to_bytes();
            let confirmed = bytes.len() == 1
                && (bytes[0] == cdata.confirm_key || (bytes[0] == b'\r' && cdata.default_yes != 0));
            if confirmed {
                retcode = 0 as ::core::ffi::c_int;
                if item.is_null() {
                    new_item_allocation = cmdq_get_command(&cdata.cmdlist, None);
                    cmdq_append(c.as_ref(), new_item_allocation);
                } else {
                    new_item_allocation = cmdq_get_command(&cdata.cmdlist, (*item).state.as_ref());
                    cmdq_insert_after(
                        item_owner.as_ref().expect("live command queue item"),
                        new_item_allocation,
                    );
                }
            }
        }
    }
    if !item.is_null() {
        if let Some(client) = cmdq_get_client((item).as_ref()) {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(retcode);
            }
        }
        cmdq_continue(item_owner.as_ref().expect("live command queue item"));
    }
    PROMPT_CLOSE
}
impl cmd_confirm_before_data {
    fn into_callback(self: Box<Self>) -> crate::src::shared::status::status_prompt_input_cb {
        Some(Box::new(move |client, text, _key| unsafe {
            cmd_confirm_before_callback(client.expect("live client"), &self, text)
        }))
    }
}
