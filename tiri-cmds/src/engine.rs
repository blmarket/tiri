use crate::cmd::queue::cmdq_item;
use crate::cmd::queue::{
    cmdq_append, cmdq_cancel_background, cmdq_error, cmdq_get_callback_owned, cmdq_get_command,
    cmdq_get_error, cmdq_guard, cmdq_insert_after, cmdq_new, cmdq_next, cmdq_print, cmdq_running,
    cmdq_wait,
};
use crate::shared::command::{cmd_list, cmd_parse_input};
use std::ffi::CStr;
use std::rc::Rc;
use tiri_core::actions::{CommandEngine, ContextRef, ContextWeak, ProgramRef};

pub fn native(item: &ContextRef) -> Rc<cmdq_item> {
    item.native::<cmdq_item>()
}
pub fn context(item: Rc<cmdq_item>) -> ContextRef {
    ContextRef::new(item)
}
pub fn weak(item: std::rc::Weak<cmdq_item>) -> ContextWeak {
    item.upgrade()
        .map_or_else(ContextWeak::new, |item| context(item).downgrade())
}
pub fn program(program: &ProgramRef) -> Rc<cmd_list> {
    let owner: Rc<dyn std::any::Any> = program.clone();
    owner
        .downcast::<cmd_list>()
        .unwrap_or_else(|_| panic!("command program"))
}

pub static ENGINE: CommandEngine = CommandEngine {
    queue_new: || cmdq_new(),
    queue_append: |client, item| weak(cmdq_append(client, native(&item))),
    queue_insert: |anchor, item| weak(cmdq_insert_after(&native(anchor), native(&item))),
    queue_program: |commands, state| context(cmdq_get_command(&program(commands), state)),
    queue_callback: |name, callback| {
        context(cmdq_get_callback_owned(
            name,
            callback.map(|callback| {
                Box::new(move |item: &Rc<cmdq_item>| callback(&context(item.clone()))) as _
            }),
        ))
    },
    queue_error: |error| context(cmdq_get_error(error)),
    queue_drive: cmdq_next,
    queue_running: || weak(cmdq_running()),
    queue_wait: |item| cmdq_wait(&native(item)),
    queue_cancel: cmdq_cancel_background,
    report: |item, write, error| {
        if error {
            cmdq_error(&native(item), write);
        } else {
            cmdq_print(&native(item), write);
        }
    },
    guard: |item, prefix, flags| cmdq_guard(&native(item), prefix, flags),
    parse: |bytes, origin| {
        let mut pi = cmd_parse_input {
            flags: origin.flags,
            file: origin.file.map(CStr::to_owned),
            line: origin.line,
            item: std::rc::Weak::new(),
            c: origin
                .client
                .map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade),
            fs: Default::default(),
        };
        let parsed = if origin.file.is_some() {
            crate::cmd::parse::cmd_parse_from_file(bytes, Some(&mut pi))
        } else {
            crate::cmd::parse::cmd_parse_from_buffer(bytes, Some(&mut pi))
        };
        parsed.map(|program| program as ProgramRef)
    },
    parse_argv: |argv| {
        crate::cmd::parse::cmd_parse_from_argv(argv).map(|program| program as ProgramRef)
    },
    command_names: || {
        crate::cmd::cmd_table()
            .iter()
            .map(|entry| entry.name)
            .collect()
    },
};
