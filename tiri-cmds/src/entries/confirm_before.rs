use crate::arguments::args_make_commands_now;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::{cmd_get_args, cmd_list_first};
use crate::shared::command::CMD_CLIENT_TFLAG;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_list};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use tiri_core::actions::Context;
use tiri_core::arguments::{
    ARGS_PARSE_COMMANDS_OR_STRING, ArgsParseError, args, args_get, args_has, args_parse,
    args_parse_type,
};
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{u_char, u_int, uint64_t};
use tiri_core::shared::client::CLIENT_DEAD;
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::prompt::PROMPT_TYPE_COMMAND;
use tiri_core::shared::prompt::{PROMPT_CLOSE, PROMPT_SINGLE, prompt_result};
use tiri_core::status::status_prompt_set;

pub struct cmd_confirm_before_data {
    pub reply: Option<cmd_confirm_before_reply>,
    pub confirm_key: u_char,
    pub default_yes: ::core::ffi::c_int,
}

pub enum cmd_confirm_before_reply {
    Wait(tiri_core::tiri_rt::oneshot::Sender<bool>),
    Background(Rc<cmd_list>),
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
    _args: &args,
    _idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    Ok(ARGS_PARSE_COMMANDS_OR_STRING)
}

fn cmd_confirm_before_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let args = cmd_get_args(self_0);
        let tc = inv.target_client();
        let wait: ::core::ffi::c_int =
            (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
        let Some(cmdlist) = args_make_commands_now(self_0, inv, 0, 1) else {
            return ExecOutcome::err();
        };
        let mut cdata = Box::new(cmd_confirm_before_data {
            reply: None,
            confirm_key: 0,
            default_yes: args_has(args, 'y' as i32 as u_char),
        });
        if let Some(confirm_key) = args_get(args, 'c' as i32 as u_char) {
            if let &[key @ 32..=126] = confirm_key.to_bytes() {
                cdata.confirm_key = key;
            } else {
                inv.error(|out| out.write_all(b"invalid confirm key"));
                return ExecOutcome::err();
            }
        } else {
            cdata.confirm_key = 'y' as i32 as u_char;
        }
        let new_prompt = if let Some(prompt) = args_get(args, 'p' as i32 as u_char) {
            let mut bytes = prompt.to_bytes().to_vec();
            bytes.push(b' ');
            CString::new(bytes).expect("C string prompt contains no interior NUL")
        } else {
            let cmd = cmd_list_first(&cmdlist)
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
        let target = inv.target().clone();
        if wait != 0 {
            let (answer, answered) = tiri_core::tiri_rt::oneshot::channel();
            cdata.reply = Some(cmd_confirm_before_reply::Wait(answer));
            let inputcb = cdata.into_callback();
            status_prompt_set(
                &tc.expect("live client"),
                Some(&target),
                &new_prompt,
                None,
                inputcb,
                None,
                PROMPT_SINGLE,
                PROMPT_TYPE_COMMAND,
            );
            drop(target);
            drop(new_prompt);
            cmd_confirm_before_wait(self_0, inv, answered, cmdlist).await
        } else {
            cdata.reply = Some(cmd_confirm_before_reply::Background(cmdlist));
            let inputcb = cdata.into_callback();
            status_prompt_set(
                &tc.expect("live client"),
                Some(&target),
                &new_prompt,
                None,
                inputcb,
                None,
                PROMPT_SINGLE,
                PROMPT_TYPE_COMMAND,
            );
            ExecOutcome::ok()
        }
    })
}

async fn cmd_confirm_before_wait<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
    answered: tiri_core::tiri_rt::oneshot::Receiver<bool>,
    cmdlist: Rc<cmd_list>,
) -> ExecOutcome {
    inv.suspend(self_0);
    let Some(confirmed) = answered.await else {
        drop(cmdlist);
        return std::future::pending().await;
    };
    if let Some(client) = inv.client() {
        if client.attached_session().upgrade().is_none() {
            client.set_return_value(if confirmed { 0 } else { 1 });
        }
    }
    if confirmed {
        ExecOutcome::commands(cmdlist)
    } else {
        drop(cmdlist);
        ExecOutcome::ok()
    }
}

fn cmd_confirm_before_callback(
    c_owner: &ClientRef,
    cdata: &mut cmd_confirm_before_data,
    s: Option<&CStr>,
) -> prompt_result {
    let c: Option<ClientRef> = Some(c_owner.clone());
    let mut confirmed = false;
    if !(c.as_ref().expect("live client").flags() & CLIENT_DEAD as uint64_t != 0) {
        if let Some(s) = s {
            let bytes = s.to_bytes();
            confirmed = bytes.len() == 1
                && (bytes[0] == cdata.confirm_key
                    || (bytes[0] == b'\r' && cdata.default_yes != 0));
        }
    }
    match cdata.reply.take() {
        Some(cmd_confirm_before_reply::Wait(answer)) => {
            let _ = answer.send(confirmed);
        }
        Some(cmd_confirm_before_reply::Background(cmdlist)) if confirmed => {
            let client_weak = c.as_ref().map_or_else(Weak::new, Rc::downgrade);
            crate::cmd::run::spawn(
                c.as_ref(),
                cmdlist,
                Context::new(client_weak, None, None, 0),
            );
        }
        _ => {}
    }
    PROMPT_CLOSE
}

impl cmd_confirm_before_data {
    fn into_callback(mut self: Box<Self>) -> tiri_core::shared::status::status_prompt_input_cb {
        Some(Box::new(move |client, text, _key| {
            cmd_confirm_before_callback(client.expect("live client"), &mut self, text)
        }))
    }
}

#[cfg(feature = "test-support")]
pub mod confirm_before_tests {
    use super::{
        CLIENT_DEAD, ClientRef, PROMPT_CLOSE, Rc, Weak, cmd_confirm_before_callback,
        cmd_confirm_before_data, cmd_confirm_before_reply, cmd_confirm_before_wait, cmd_list,
    };
    use crate::cmd::cmd_list_new;
    use crate::cmd::invocation::ExecOutcome;
    use crate::cmd::run::test_harness::spawn_invocation;
    use std::cell::Cell;
    use tiri_core::reactor::Lane;
    use tiri_core::server_client::Client as _;
    use tiri_core::server_client::queue_test_clients as clients;
    use tiri_core::server_client::queue_test_clients::return_value;

    fn append_wait(
        owner: &ClientRef,
        default_yes: bool,
    ) -> (Weak<()>, Box<cmd_confirm_before_data>, Weak<cmd_list>) {
        let (answer, answered) = tiri_core::tiri_rt::oneshot::channel();
        let cmdlist = Rc::new(cmd_list_new());
        let list = Rc::downgrade(&cmdlist);
        let item = spawn_invocation(Some(owner), move |cmd_ref, inv| {
            Box::pin(cmd_confirm_before_wait(cmd_ref, inv, answered, cmdlist))
        });
        assert_eq!(Lane::drive(Some(owner)), 0, "the item waits");
        let cdata = Box::new(cmd_confirm_before_data {
            reply: Some(cmd_confirm_before_reply::Wait(answer)),
            confirm_key: b'y',
            default_yes: default_yes.into(),
        });
        (item, cdata, list)
    }

    fn append_flag(owner: &ClientRef) -> Rc<Cell<bool>> {
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        spawn_invocation(Some(owner), move |_, _| {
            Box::pin(async move {
                flag.set(true);
                ExecOutcome::ok()
            })
        });
        ran
    }

    pub fn the_answer_acts_inside_the_send() {
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
            assert_eq!(return_value(&owner), (!confirmed).into());
            assert!(list.upgrade().is_none(), "the continuation finished");
            assert!(head.upgrade().is_some() && !ran.get());
            assert_eq!(Lane::drive(Some(&owner)), if confirmed { 3 } else { 2 });
            assert!(head.upgrade().is_none() && ran.get());
            cmd_confirm_before_callback(&owner, &mut cdata, Some(c"y"));
            assert_eq!(Lane::drive(Some(&owner)), 0);
        }
    }

    pub fn a_prompt_freed_unanswered_releases_the_data_and_parks() {
        let owner = clients::with_queue();
        let (head, cdata, list) = append_wait(&owner, false);
        let ran = append_flag(&owner);
        owner.set_return_value(-1);
        drop(cdata);
        assert!(list.upgrade().is_none(), "the command list is released");
        assert_eq!(Lane::drive(Some(&owner)), 0);
        assert!(head.upgrade().is_some() && !ran.get());
        assert_eq!(return_value(&owner), -1);
        owner.update_flags(CLIENT_DEAD as u64, 0);
        drop(owner);
        assert!(head.upgrade().is_none());
    }

    pub fn an_answer_after_the_waiting_client_is_freed_queues_nothing() {
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
        assert_eq!(Lane::drive(Some(&answering)), 0);
    }

    pub fn a_background_confirmation_is_appended_to_the_answering_client() {
        for (answer, queued) in [(c"n", 0), (c"y", 1)] {
            let answering = clients::with_queue();
            let mut cdata = Box::new(cmd_confirm_before_data {
                reply: Some(cmd_confirm_before_reply::Background(
                    Rc::new(cmd_list_new()),
                )),
                confirm_key: b'y',
                default_yes: 0,
            });
            cmd_confirm_before_callback(&answering, &mut cdata, Some(answer));
            assert_eq!(Lane::drive(Some(&answering)), queued);
        }
    }
}
