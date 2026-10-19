use crate::arguments::{args_has, args_parse};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_get_target_client, cmdq_print};
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_add_time, format_create_from_target, format_expand_cstring,
};
use crate::job::job_print_summary;
use crate::server::message_log;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_TFLAG};
use crate::tty::tty_term_descriptions;

pub const SHOW_MESSAGES_TEMPLATE: &std::ffi::CStr = c"#{t/p:message_time}: #{message_text}";
pub static cmd_show_messages_entry: cmd_entry = {
    cmd_entry {
        name: c"show-messages",
        alias: Some(c"showmsgs"),
        args: args_parse {
            template: c"JTt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-JT] [-t target-client]",
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
        flags: CMD_AFTERHOOK | CMD_CLIENT_TFLAG | CMD_CLIENT_CANFAIL,
        exec: Some(cmd_show_messages_exec),
    }
};
unsafe fn cmd_show_messages_terminals(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    mut blank: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let tc_owner = cmdq_get_target_client(Some(&*item));
    let target = if args_has(args, b't') != 0 {
        tc_owner.as_ref()
    } else {
        None
    };
    let mut n = 0_u32;
    for term in tty_term_descriptions(target) {
        if blank != 0 {
            cmdq_print(item_handle, |_| Ok(()));
            blank = 0;
        }
        cmdq_print(item_handle, |out| {
            write!(out, "Terminal {}: ", n)?;
            out.write_all(term.name.to_bytes())?;
            out.write_all(b" for ")?;
            write_cstr(out, term.client_name.as_deref())?;
            write!(out, ", flags=0x{:x}:", term.flags as u32)
        });
        n = n.wrapping_add(1);
        for code in term.codes {
            cmdq_print(item_handle, |out| out.write_all(code.as_bytes()));
        }
    }
    i32::from(n != 0)
}

unsafe fn cmd_show_messages_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let args = cmd_get_args(self_0.get_unchecked());
    let mut done: ::core::ffi::c_int = 0;
    let mut blank: ::core::ffi::c_int = 0;
    blank = 0 as ::core::ffi::c_int;
    done = blank;
    if args_has(args, 'T' as i32 as u_char) != 0 {
        blank = cmd_show_messages_terminals(self_0.clone(), item_handle, blank);
        done = 1 as ::core::ffi::c_int;
    }
    if args_has(args, 'J' as i32 as u_char) != 0 {
        job_print_summary(item_handle, blank);
        done = 1 as ::core::ffi::c_int;
    }
    if done != 0 {
        return CmdOutcome::Normal;
    }
    let mut ft_owner = format_create_from_target(item_handle);
    let ft = &mut *ft_owner;
    for msg in message_log.iter_rev() {
        format_add(ft, c"message_text", |out| write_cstr(out, &*msg.msg));
        format_add(ft, c"message_number", |out| {
            write!(out, "{}", { msg.msg_num })
        });
        let msg_time = msg.msg_time;
        format_add_time(ft, c"message_time", msg_time);
        let s = format_expand_cstring(ft, SHOW_MESSAGES_TEMPLATE.as_ptr());
        cmdq_print(item_handle, |out| write_cstr(out, &*s));
    }
    drop(ft_owner);
    CmdOutcome::Normal
}
