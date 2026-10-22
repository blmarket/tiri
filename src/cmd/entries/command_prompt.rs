use crate::arguments::{
    args, args_command_state, args_count, args_get, args_has, args_make_commands,
    args_make_commands_get_command_cstring, args_make_commands_prepare, args_parse,
    args_parse_type, ArgsParseError, ARGS_PARSE_COMMANDS_OR_STRING,
};
use crate::cmd::queue::{
    cmdq_append, cmdq_error, cmdq_get_command, cmdq_get_error, cmdq_get_target_client,
    cmdq_insert_after, cmdq_resumed,
};
use crate::cmd::{cmd_append_argv, cmd_get_args};
use crate::format::bytes::write_cstr;
use crate::prompt::prompt_type;
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::CMD_CLIENT_TFLAG;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_list, cmdq_item};
use crate::shared::pane::window_pane;
use crate::shared::prompt::*;
use crate::shared::prompt::{
    prompt_result, PROMPT_BSPACE_EXIT, PROMPT_CLOSE, PROMPT_CONTINUE, PROMPT_INCREMENTAL,
    PROMPT_ISPANE, PROMPT_KEY, PROMPT_NOFREEZE, PROMPT_NUMERIC, PROMPT_SINGLE,
};
use crate::status::{status_prompt_set, status_prompt_update};
use crate::window_pane::WindowPane as _;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};

pub struct cmd_command_prompt_cdata {
    /// Resumes the waiting item with the commands to insert after it; the
    /// prompt closing or being freed without any drops it, which resumes the
    /// item with nothing (tmux's cmd_command_prompt_free). None for -b and -i.
    pub resume: Option<tiri_rt::oneshot::Sender<Rc<RefCell<cmd_list>>>>,
    pub state: Option<Box<args_command_state>>,
    pub flags: ::core::ffi::c_int,
    pub prompt_type: prompt_type,
    pub wp: Weak<RefCell<window_pane>>,
    pub prompts: Vec<cmd_command_prompt_prompt>,
    pub current: u_int,
    pub argv: Vec<CString>,
}
pub struct cmd_command_prompt_prompt {
    pub input: Option<CString>,
    pub prompt: CString,
}

fn cmd_command_prompt_rows(
    prompts: &[u8],
    inputs: Option<&[u8]>,
    literal: bool,
    space: bool,
) -> Vec<cmd_command_prompt_prompt> {
    if literal {
        return vec![cmd_command_prompt_prompt {
            prompt: CString::new(prompts).expect("prompt has no embedded NUL"),
            input: inputs.map(|s| CString::new(s).expect("input has no embedded NUL")),
        }];
    }

    let mut input_parts = inputs.map(|s| s.split(|&byte| byte == b','));
    prompts
        .split(|&byte| byte == b',')
        .map(|part| {
            let mut prompt = part.to_vec();
            if space {
                prompt.push(b' ');
            }
            let input = input_parts.as_mut().and_then(Iterator::next).unwrap_or(b"");
            cmd_command_prompt_prompt {
                prompt: CString::new(prompt).expect("prompt has no embedded NUL"),
                input: Some(CString::new(input).expect("input has no embedded NUL")),
            }
        })
        .collect()
}
pub static cmd_command_prompt_entry: cmd_entry = {
    cmd_entry {
        name: c"command-prompt",
        alias: None,
        args: args_parse {
            template: c"1CbeFiklI:NPp:t:T:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: Some(cmd_command_prompt_args_parse),
        },
        usage:
            c"[-1CbeFiklNP] [-I inputs] [-p prompts] [-t target-client] [-T prompt-type] [template]",
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
        exec: Some(cmd_command_prompt_exec),
    }
};
fn cmd_command_prompt_args_parse(
    _args: &args,
    _idx: u_int,
) -> Result<args_parse_type, ArgsParseError> {
    Ok(ARGS_PARSE_COMMANDS_OR_STRING)
}
unsafe fn cmd_command_prompt_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let cmd_ref = self_0.try_borrow().expect("live command");
    let args = cmd_get_args(&cmd_ref);
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let mut tc: Option<ClientRef> = tc_owner.clone();
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let mut prompt_bytes = Vec::<u8>::new();
    let wp = target.pane_handle();
    let mut count: u_int = args_count(args);
    let mut wait: ::core::ffi::c_int =
        (args_has(args, 'b' as i32 as u_char) == 0) as ::core::ffi::c_int;
    let mut space: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
    let mut pane: ::core::ffi::c_int = args_has(args, 'P' as i32 as u_char);
    if pane != 0 {
        if wp.is_none() || wp.as_ref().is_some_and(|pane| pane.has_prompt()) {
            return CmdOutcome::Normal;
        }
    } else if tc
        .as_ref()
        .expect("live client")
        .prompt_observer()
        .is_alive()
    {
        return CmdOutcome::Normal;
    }
    if args_has(args, 'i' as i32 as u_char) != 0 {
        wait = 0 as ::core::ffi::c_int;
    }
    let mut cdata = Box::new(cmd_command_prompt_cdata {
        resume: None,
        state: None,
        flags: 0,
        prompt_type: PROMPT_TYPE_COMMAND,
        wp: Weak::new(),
        prompts: Vec::new(),
        current: 0,
        argv: Vec::new(),
    });
    if pane != 0 {
        cdata.wp = std::rc::Rc::downgrade(wp.as_ref().expect("prompt target pane"));
    }
    cdata.state = Some(args_make_commands_prepare(
        self_0.clone(),
        item_handle,
        0 as u_int,
        Some(c"%1"),
        wait,
        args_has(args, 'F' as i32 as u_char),
    ));
    let literal = args_has(args, 'l' as i32 as u_char) != 0;
    if let Some(s) = args_get(args, 'p' as i32 as u_char) {
        prompt_bytes.extend_from_slice(s.to_bytes());
    } else {
        if count != 0 as u_int {
            let command = args_make_commands_get_command_cstring(
                cdata.state.as_deref().expect("prepared command state"),
            );
            prompt_bytes.push(b'(');
            prompt_bytes.extend_from_slice(command.as_bytes());
            prompt_bytes.push(b')');
        } else {
            prompt_bytes.push(b':');
            space = 0;
        }
    }
    let input_bytes = args_get(args, 'I' as i32 as u_char).map(CStr::to_bytes);
    cdata.prompts = cmd_command_prompt_rows(&prompt_bytes, input_bytes, literal, space != 0);
    if let Some(type_0) = args_get(args, 'T' as i32 as u_char) {
        cdata.prompt_type = prompt_type(type_0);
        if cdata.prompt_type as ::core::ffi::c_uint
            == PROMPT_TYPE_INVALID as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            cmdq_error(item_handle, |out| {
                out.write_all(b"unknown type: ")?;
                write_cstr(out, type_0)
            });
            return CmdOutcome::Error;
        }
    } else {
        cdata.prompt_type = PROMPT_TYPE_COMMAND;
    }
    if args_has(args, '1' as i32 as u_char) != 0 {
        cdata.flags |= PROMPT_SINGLE;
    } else if args_has(args, 'N' as i32 as u_char) != 0 {
        cdata.flags |= PROMPT_NUMERIC;
    } else if args_has(args, 'i' as i32 as u_char) != 0 {
        cdata.flags |= PROMPT_INCREMENTAL;
    } else if args_has(args, 'k' as i32 as u_char) != 0 {
        cdata.flags |= PROMPT_KEY;
    } else if args_has(args, 'e' as i32 as u_char) != 0 {
        cdata.flags |= PROMPT_BSPACE_EXIT;
    }
    if args_has(args, 'C' as i32 as u_char) != 0 {
        cdata.flags |= PROMPT_NOFREEZE;
    }
    if pane != 0 {
        cdata.flags |= PROMPT_ISPANE;
    }
    let (prompt, input) = (
        cdata.prompts[0].prompt.clone(),
        cdata.prompts[0].input.clone(),
    );
    let flags = cdata.flags;
    let prompt_type = cdata.prompt_type;
    let continuation = if wait != 0 {
        let (resume, answered) = tiri_rt::oneshot::channel();
        cdata.resume = Some(resume);
        Some(cmd_command_prompt_wait(
            answered,
            Rc::downgrade(item_handle),
        ))
    } else {
        None
    };
    let inputcb = cdata.into_callback();
    if pane != 0 {
        wp.as_ref().expect("prompt target pane").set_prompt(
            tc_owner.as_ref(),
            Some(target),
            &prompt,
            input.as_deref(),
            inputcb,
            None,
            flags,
            prompt_type,
        );
    } else {
        status_prompt_set(
            &tc.clone().expect("live client"),
            Some(target),
            &prompt,
            input.as_deref(),
            inputcb,
            None,
            flags,
            prompt_type,
        );
    }
    let Some(continuation) = continuation else {
        return CmdOutcome::Normal;
    };
    CmdOutcome::Suspend(Box::pin(continuation))
}
/// Insert the answer's commands after the item with the item's state, inside
/// the send as tmux's callback does. A prompt closed or freed without any
/// resumes the item with nothing queued.
async fn cmd_command_prompt_wait(
    answered: tiri_rt::oneshot::Receiver<Rc<RefCell<cmd_list>>>,
    item: Weak<std::cell::RefCell<cmdq_item>>,
) {
    let Some(cmdlist) = answered.await else {
        return;
    };
    {
        let item = cmdq_resumed(&item);
        let new_item_allocation = cmdq_get_command(&cmdlist, item.borrow().state.as_ref());
        cmdq_insert_after(&item, new_item_allocation);
    }
}
fn cmd_command_prompt_callback(
    c_owner: Option<&ClientRef>,
    cdata: &mut cmd_command_prompt_cdata,
    mut s: Option<&CStr>,
    mut key: prompt_key_result,
) -> prompt_result {
    let mut c: Option<ClientRef> = c_owner.cloned();
    let mut current_block: u64;
    let new_item_allocation;
    if let Some(s) = s.filter(|_| {
        key as ::core::ffi::c_uint != PROMPT_KEY_MOVE as ::core::ffi::c_int as ::core::ffi::c_uint
    }) {
        if key as ::core::ffi::c_uint
            == PROMPT_KEY_CLOSE as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            if cdata.flags & PROMPT_INCREMENTAL != 0 {
                current_block = 11745758271394821990;
            } else {
                cmd_append_argv(&mut cdata.argv, s);
                cdata.current = cdata.current.wrapping_add(1);
                if (cdata.current as usize) != cdata.prompts.len() {
                    let row = &cdata.prompts[cdata.current as usize];
                    let (prompt, input) = (row.prompt.clone(), row.input.clone());
                    if !Weak::ptr_eq(&cdata.wp, &Weak::new()) {
                        let Some(pane) =
                            std::rc::Rc::<std::cell::RefCell<window_pane>>::from_observer(
                                &cdata.wp,
                            )
                        else {
                            return PROMPT_CLOSE;
                        };
                        pane.update_prompt(&prompt, input.as_deref());
                    } else {
                        status_prompt_update(
                            &c.clone().expect("live client"),
                            &prompt,
                            input.as_deref(),
                        );
                    }
                    return PROMPT_CONTINUE;
                }
                current_block = 1841672684692190573;
            }
        } else {
            current_block = 1841672684692190573;
        }
        match current_block {
            11745758271394821990 => {}
            _ => {
                let mut argv_owner = cdata.argv.clone();
                if key as ::core::ffi::c_uint
                    != PROMPT_KEY_CLOSE as ::core::ffi::c_int as ::core::ffi::c_uint
                {
                    cmd_append_argv(&mut argv_owner, s);
                } else {
                    cdata.argv = argv_owner.clone();
                }
                match args_make_commands(
                    cdata.state.as_deref_mut().expect("prepared command state"),
                    &argv_owner,
                ) {
                    Err(error) => {
                        cmdq_append(
                            c_owner,
                            cmdq_get_error(error.as_deref().expect("command error")),
                        );
                    }
                    Ok(cmdlist) => match cdata.resume.take() {
                        Some(resume) => {
                            let _ = resume.send(cmdlist);
                        }
                        None => {
                            new_item_allocation = cmdq_get_command(&cmdlist, None);
                            cmdq_append(c_owner, new_item_allocation);
                            drop(cmdlist);
                        }
                    },
                }
                if cdata.flags & PROMPT_INCREMENTAL != 0 {
                    return PROMPT_CONTINUE;
                }
            }
        }
    }
    drop(cdata.resume.take());
    PROMPT_CLOSE
}
impl cmd_command_prompt_cdata {
    fn into_callback(mut self: Box<Self>) -> crate::shared::status::status_prompt_input_cb {
        Some(Box::new(move |client, text, key| {
            cmd_command_prompt_callback(client, &mut self, text, key)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_prompts_keep_empty_fields_and_fill_missing_inputs() {
        let rows = cmd_command_prompt_rows(b"first,,last,", Some(b"one,,three"), false, true);
        let prompts: Vec<_> = rows.iter().map(|row| row.prompt.to_bytes()).collect();
        let inputs: Vec<_> = rows
            .iter()
            .map(|row| row.input.as_ref().unwrap().to_bytes())
            .collect();
        assert_eq!(prompts, [b"first ".as_slice(), b" ", b"last ", b" "]);
        assert_eq!(inputs, [b"one".as_slice(), b"", b"three", b""]);
    }

    #[test]
    fn literal_prompt_keeps_commas_and_null_input() {
        let rows = cmd_command_prompt_rows(b"first,second", None, true, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt.to_bytes(), b"first,second");
        assert!(rows[0].input.is_none());

        let rows = cmd_command_prompt_rows(b":", Some(b""), true, false);
        assert_eq!(rows[0].prompt.to_bytes(), b":");
        assert!(rows[0].input.is_some());
        assert_eq!(rows[0].input.as_ref().unwrap().to_bytes(), b"");
    }
}
