use crate::src::arguments::{
    args, args_get, args_has, args_make_commands_now, args_parse, args_parse_type, ArgsParseError,
    ARGS_PARSE_COMMANDS_OR_STRING,
};
use crate::src::cmd::queue::{
    cmdq_append, cmdq_error, cmdq_get_client, cmdq_get_command, cmdq_get_target_client,
    cmdq_insert_after, cmdq_resumed, cmdq_suspend,
};
use crate::src::cmd::{cmd_get_args_mut, cmd_list_first};
use crate::src::format::bytes::nullable_cstr;
use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
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
use std::cell::{RefCell, UnsafeCell};
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

pub struct cmd_confirm_before_data {
    /// Taken by the answer.
    pub reply: Option<cmd_confirm_before_reply>,
    pub confirm_key: u_char,
    pub default_yes: ::core::ffi::c_int,
}
/// Who acts on the answer.
pub enum cmd_confirm_before_reply {
    /// The waiting item's continuation, which owns the command list.
    Wait(tiri_rt::oneshot::Sender<bool>),
    /// -b: a confirmed command is appended to the answering client's queue.
    Background(Rc<RefCell<cmd_list>>),
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
    let mut wait: ::core::ffi::c_int =
        (args_has(&*args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let Some(cmdlist) = args_make_commands_now(self_0.clone(), item_handle, 0, 1) else {
        return CMD_RETURN_ERROR;
    };
    let mut cdata = Box::new(cmd_confirm_before_data {
        reply: None,
        confirm_key: 0,
        default_yes: 0,
    });
    cdata.default_yes = args_has(&*args, 'y' as i32 as u_char);
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
    let new_prompt = if let Some(prompt) = args_get(&*(args), 'p' as i32 as u_char) {
        let mut bytes = prompt.to_bytes().to_vec();
        bytes.push(b' ');
        CString::new(bytes).expect("C string prompt contains no interior NUL")
    } else {
        let cmd = cmd_list_first(&cmdlist.borrow())
            .expect("confirmation command")
            .entry
            .name;
        let mut bytes = b"Confirm '".to_vec();
        bytes.extend_from_slice(cmd.to_bytes());
        bytes.extend_from_slice(b"'? (");
        bytes.push(cdata.confirm_key);
        bytes.extend_from_slice(b"/n) ");
        CString::new(bytes).expect("C string command and validated key contain no interior NUL")
    };
    let continuation = if wait != 0 {
        let (answer, answered) = tiri_rt::oneshot::channel();
        cdata.reply = Some(cmd_confirm_before_reply::Wait(answer));
        Some(cmd_confirm_before_wait(
            answered,
            cmdlist,
            Rc::downgrade(item_handle),
        ))
    } else {
        cdata.reply = Some(cmd_confirm_before_reply::Background(cmdlist));
        None
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
    let Some(continuation) = continuation else {
        return CMD_RETURN_NORMAL;
    };
    cmdq_suspend(item_handle, continuation)
}
/// Act on the answer inside its send, at answer time as tmux does: queue a
/// confirmed command after the item with the item's state, and report the
/// answer to a waiting client without a session. A prompt freed unanswered
/// never resumes the item (tmux's cmd_confirm_before_free): release the
/// command list and park.
async fn cmd_confirm_before_wait(
    answered: tiri_rt::oneshot::Receiver<bool>,
    cmdlist: Rc<RefCell<cmd_list>>,
    item: Weak<UnsafeCell<cmdq_item>>,
) {
    let Some(confirmed) = answered.await else {
        drop(cmdlist);
        return std::future::pending().await;
    };
    unsafe {
        let item = cmdq_resumed(&item);
        if confirmed {
            let new_item_allocation = cmdq_get_command(&cmdlist, (*item.get()).state.as_ref());
            cmdq_insert_after(&item, new_item_allocation);
        }
        if let Some(client) = cmdq_get_client(Some(&*item.get())) {
            if client.attached_session().upgrade().is_none() {
                client.set_return_value(if confirmed { 0 } else { 1 });
            }
        }
    }
}
unsafe fn cmd_confirm_before_callback(
    c_owner: &ClientRef,
    cdata: &mut cmd_confirm_before_data,
    s: Option<&CStr>,
) -> prompt_result {
    let mut c: Option<ClientRef> = Some(c_owner.clone());
    let new_item_allocation;
    let mut confirmed = false;
    if !(c.as_ref().expect("live client").flags() & CLIENT_DEAD as uint64_t != 0) {
        if let Some(s) = s {
            let bytes = s.to_bytes();
            confirmed = bytes.len() == 1
                && (bytes[0] == cdata.confirm_key || (bytes[0] == b'\r' && cdata.default_yes != 0));
        }
    }
    match cdata.reply.take() {
        Some(cmd_confirm_before_reply::Wait(answer)) => {
            let _ = answer.send(confirmed);
        }
        Some(cmd_confirm_before_reply::Background(cmdlist)) if confirmed => {
            new_item_allocation = cmdq_get_command(&cmdlist, None);
            cmdq_append(c.as_ref(), new_item_allocation);
        }
        _ => {}
    }
    PROMPT_CLOSE
}
impl cmd_confirm_before_data {
    fn into_callback(mut self: Box<Self>) -> crate::src::shared::status::status_prompt_input_cb {
        Some(Box::new(move |client, text, _key| unsafe {
            cmd_confirm_before_callback(client.expect("live client"), &mut self, text)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::cmd::core::cmd_list_new;
    use crate::src::cmd::queue::{cmdq_get_callback_owned, cmdq_next};
    use crate::src::server_client::queue_test_clients as clients;
    use std::cell::Cell;

    /// Queue an item on `owner` that waits for the answer to a prompt, as
    /// cmd_confirm_before_exec does; return the item, the prompt's data and
    /// the command list the continuation owns.
    unsafe fn append_wait(
        owner: &ClientRef,
        default_yes: bool,
    ) -> (
        Weak<UnsafeCell<cmdq_item>>,
        Box<cmd_confirm_before_data>,
        Weak<RefCell<cmd_list>>,
    ) {
        let (answer, answered) = tiri_rt::oneshot::channel();
        let cmdlist = cmd_list_new();
        let list = Rc::downgrade(&cmdlist);
        let item = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"confirm-before",
                Some(Box::new(move |item| {
                    cmdq_suspend(
                        item,
                        cmd_confirm_before_wait(answered, cmdlist, Rc::downgrade(item)),
                    )
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(owner)), 0, "the item waits");
        let cdata = Box::new(cmd_confirm_before_data {
            reply: Some(cmd_confirm_before_reply::Wait(answer)),
            confirm_key: b'y',
            default_yes: default_yes.into(),
        });
        (item, cdata, list)
    }

    /// Queue a callback that records whether it ran.
    unsafe fn append_flag(owner: &ClientRef) -> Rc<Cell<bool>> {
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"flag",
                Some(Box::new(move |_| {
                    flag.set(true);
                    CMD_RETURN_NORMAL
                })),
            ),
        );
        ran
    }

    #[test]
    fn the_answer_acts_inside_the_send() {
        unsafe {
            for (default_yes, answer, confirmed) in [
                (false, Some(c"y"), true),
                (false, Some(c"n"), false),
                (false, Some(c"\r"), false),
                (true, Some(c"\r"), true),
                (true, None, false),
            ] {
                let owner = clients::with_queue();
                let (head, mut cdata, list) = append_wait(&owner, default_yes);
                let ran = append_flag(&owner);
                owner.set_return_value(-1);
                assert_eq!(
                    cmd_confirm_before_callback(&owner, &mut cdata, answer),
                    PROMPT_CLOSE
                );
                assert_eq!(clients::return_value(&owner), (!confirmed).into());
                assert!(list.upgrade().is_none(), "the continuation finished");
                assert!(head.upgrade().is_some() && !ran.get());
                assert_eq!(cmdq_next(Some(&owner)), u_int::from(confirmed) + 1);
                assert!(head.upgrade().is_none() && ran.get());
                cmd_confirm_before_callback(&owner, &mut cdata, Some(c"y"));
                assert_eq!(cmdq_next(Some(&owner)), 0);
            }
        }
    }

    #[test]
    fn a_prompt_freed_unanswered_releases_the_data_and_parks() {
        unsafe {
            let owner = clients::with_queue();
            let (head, cdata, list) = append_wait(&owner, false);
            let ran = append_flag(&owner);
            owner.set_return_value(-1);
            drop(cdata);
            assert!(list.upgrade().is_none(), "the command list is released");
            assert_eq!(cmdq_next(Some(&owner)), 0);
            assert!(head.upgrade().is_some() && !ran.get());
            assert_eq!(clients::return_value(&owner), -1);
            owner.update_flags(CLIENT_DEAD as u64, 0);
            drop(owner);
            assert!(head.upgrade().is_none());
        }
    }

    #[test]
    fn an_answer_after_the_waiting_client_is_freed_queues_nothing() {
        unsafe {
            let owner = clients::with_queue();
            let answering = clients::with_queue();
            let (head, mut cdata, list) = append_wait(&owner, false);
            owner.update_flags(CLIENT_DEAD as u64, 0);
            let client = Rc::downgrade(&owner);
            drop(owner);
            assert!(client.upgrade().is_none());
            assert!(head.upgrade().is_none() && list.upgrade().is_none());
            assert_eq!(
                cmd_confirm_before_callback(&answering, &mut cdata, Some(c"y")),
                PROMPT_CLOSE
            );
            assert_eq!(cmdq_next(Some(&answering)), 0);
        }
    }

    #[test]
    fn a_background_confirmation_is_appended_to_the_answering_client() {
        unsafe {
            for (answer, queued) in [(c"n", 0), (c"y", 1)] {
                let answering = clients::with_queue();
                let mut cdata = Box::new(cmd_confirm_before_data {
                    reply: Some(cmd_confirm_before_reply::Background(cmd_list_new())),
                    confirm_key: b'y',
                    default_yes: 0,
                });
                cmd_confirm_before_callback(&answering, &mut cdata, Some(answer));
                assert_eq!(cmdq_next(Some(&answering)), queued);
            }
        }
    }
}
