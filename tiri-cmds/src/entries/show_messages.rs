use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::cmd_get_args;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_TFLAG};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use tiri_core::arguments::{args_has, args_parse};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{format_add, format_add_time, format_expand_cstring};
use tiri_core::job::job_print_summary;
use tiri_core::server::message_log;
use tiri_core::shared::abi::u_char;
use tiri_core::tty::tty_term_descriptions;

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
fn cmd_show_messages_terminals(
    self_0: &cmd,
    inv: &Invocation<'_>,
    mut blank: ::core::ffi::c_int,
) -> ::core::ffi::c_int {
    let args = cmd_get_args(self_0);
    let tc_owner = inv.target_client();
    let target = if args_has(args, b't') != 0 {
        tc_owner.as_ref()
    } else {
        None
    };
    let mut n = 0_u32;
    for term in tty_term_descriptions(target) {
        if blank != 0 {
            inv.print( |_| Ok(()));
            blank = 0;
        }
        inv.print( |out| {
            write!(out, "Terminal {}: ", n)?;
            out.write_all(term.name.to_bytes())?;
            out.write_all(b" for ")?;
            write_cstr(out, term.client_name.as_deref())?;
            write!(out, ", flags=0x{:x}:", term.flags as u32)
        });
        n = n.wrapping_add(1);
        for code in term.codes {
            inv.print( |out| out.write_all(code.as_bytes()));
        }
    }
    i32::from(n != 0)
}

fn cmd_show_messages_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let mut done: ::core::ffi::c_int = 0;
    let mut blank: ::core::ffi::c_int = 0;
    blank = 0 as ::core::ffi::c_int;
    done = blank;
    if args_has(args, 'T' as i32 as u_char) != 0 {
        blank = cmd_show_messages_terminals(self_0, inv, blank);
        done = 1 as ::core::ffi::c_int;
    }
    if args_has(args, 'J' as i32 as u_char) != 0 {
        let queue_client = inv.client();
        job_print_summary(queue_client.as_ref(), blank);
        done = 1 as ::core::ffi::c_int;
    }
    if done != 0 {
        return ExecOutcome::ok();
    }
    let mut ft_owner = inv.format_from_target();
    let ft = &mut *ft_owner;
    let messages: Vec<_> = message_log.with_borrow(|log| {
        log.iter_rev()
            .map(|msg| (msg.msg.clone(), msg.msg_num, msg.msg_time))
            .collect()
    });
    for (text, number, msg_time) in messages {
        format_add(ft, c"message_text", |out| write_cstr(out, &*text));
        format_add(ft, c"message_number", |out| write!(out, "{}", number));
        format_add_time(ft, c"message_time", msg_time);
        let s = format_expand_cstring(ft, SHOW_MESSAGES_TEMPLATE);
        inv.print( |out| write_cstr(out, &*s));
    }
    drop(ft_owner);
    ExecOutcome::ok()
})
}
