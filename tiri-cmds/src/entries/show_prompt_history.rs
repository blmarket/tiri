use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use crate::shared::command::CMD_AFTERHOOK;
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::CStr;
use tiri_core::arguments::{args_get, args_parse};
use tiri_core::format::bytes::write_cstr;
use tiri_core::prompt::{prompt_type, prompt_type_string};
use tiri_core::prompt_history::{prompt_history_clear, prompt_history_get, prompt_history_size};
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::shared::prompt::PROMPT_NTYPES;
use tiri_core::shared::prompt::{PROMPT_TYPE_COMMAND, PROMPT_TYPE_INVALID, prompt_type};
pub static cmd_show_prompt_history_entry: cmd_entry = {
    cmd_entry {
        name: c"show-prompt-history",
        alias: Some(c"showphist"),
        args: args_parse {
            template: c"T:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-T prompt-type]",
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_show_prompt_history_exec),
    }
};
pub static cmd_clear_prompt_history_entry: cmd_entry = {
    cmd_entry {
        name: c"clear-prompt-history",
        alias: Some(c"clearphist"),
        args: args_parse {
            template: c"T:",
            lower: 0 as ::core::ffi::c_int,
            upper: 0 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-T prompt-type]",
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
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_show_prompt_history_exec),
    }
};
fn cmd_show_prompt_history_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let typestr: Option<&CStr> = args_get(args, 'T' as i32 as u_char);
    let mut type_0: prompt_type = PROMPT_TYPE_COMMAND;
    let mut t: u_int = 0;
    let mut h: u_int = 0;
    if std::ptr::eq(cmd_get_entry(self_0), &cmd_clear_prompt_history_entry) {
        if let Some(typestr) = typestr {
            type_0 = prompt_type(typestr);
            if type_0 as ::core::ffi::c_uint
                == PROMPT_TYPE_INVALID as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                inv.error( |out| {
                    out.write_all(b"invalid type: ")?;
                    write_cstr(out, typestr)
                });
                return ExecOutcome::err();
            }
            prompt_history_clear(type_0);
        } else {
            t = 0 as u_int;
            while t < PROMPT_NTYPES as u_int {
                prompt_history_clear(t as prompt_type);
                t = t.wrapping_add(1);
            }
        }
        return ExecOutcome::ok();
    }
    match typestr {
        None => {
            t = 0 as u_int;
            while t < PROMPT_NTYPES as u_int {
                let type_name = prompt_type_string(t as prompt_type);
                inv.print( |out| {
                    out.write_all(b"History for ")?;
                    out.write_all(type_name.to_bytes())?;
                    out.write_all(b":\n")
                });
                h = 0 as u_int;
                while h < prompt_history_size(t as prompt_type) {
                    let value =
                        prompt_history_get(t as prompt_type, h).expect("existing history entry");
                    inv.print( |out| {
                        write!(out, "{}: ", (h.wrapping_add(1 as u_int)) as i32)?;
                        out.write_all(value.to_bytes())
                    });
                    h = h.wrapping_add(1);
                }
                inv.print( |out| write_cstr(out, c""));
                t = t.wrapping_add(1);
            }
        }
        Some(typestr) => {
            type_0 = prompt_type(typestr);
            if type_0 as ::core::ffi::c_uint
                == PROMPT_TYPE_INVALID as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                inv.error( |out| {
                    out.write_all(b"invalid type: ")?;
                    write_cstr(out, typestr)
                });
                return ExecOutcome::err();
            }
            inv.print( |out| {
                out.write_all(b"History for ")?;
                out.write_all(prompt_type_string(type_0).to_bytes())?;
                out.write_all(b":\n")
            });
            h = 0 as u_int;
            while h < prompt_history_size(type_0) {
                let value = prompt_history_get(type_0, h).expect("existing history entry");
                inv.print( |out| {
                    write!(out, "{}: ", (h.wrapping_add(1 as u_int)) as i32)?;
                    out.write_all(value.to_bytes())
                });
                h = h.wrapping_add(1);
            }
            inv.print( |out| write_cstr(out, c""));
        }
    }
    ExecOutcome::ok()
})
}
