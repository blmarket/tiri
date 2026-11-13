use crate::arguments::{args_command_state, args_make_commands, args_make_commands_prepare};
use crate::cmd::find::TargetLookup;
use crate::cmd::queue::{
    cmdq_append, cmdq_cancel_background, cmdq_error, cmdq_get_callback_owned, cmdq_get_cmd,
    cmdq_get_command, cmdq_get_error, cmdq_guard, cmdq_insert_after, cmdq_new, cmdq_next,
    cmdq_print, cmdq_running, cmdq_wait,
};
use crate::shared::command::{cmd_list, cmdq_item};
use std::rc::Rc;
use tiri_core::actions::{ContextRef, ContextWeak, PreparedAction, ProgramRef, Runtime};

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
impl PreparedAction for args_command_state {
    fn materialize(
        &mut self,
        arguments: &Vec<std::ffi::CString>,
    ) -> Result<ProgramRef, Option<std::ffi::CString>> {
        args_make_commands(self, arguments).map(|program| program as ProgramRef)
    }
}

pub static RUNTIME: Runtime = Runtime {
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
    parse_source: |source| {
        crate::cmd::parse::cmd_parse_from_string(source, None).map(|program| program as ProgramRef)
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
    has_command: |item| cmdq_get_cmd(&native(item).borrow()).is_some(),
    prepare: |item, index, default, wait, expand| {
        let owner = native(item);
        let command = owner.borrow();
        args_make_commands_prepare(
            cmdq_get_cmd(&command).expect("live command"),
            &owner,
            index,
            default,
            wait,
            expand,
        )
    },
    config_finished: crate::cfg::cfg_finished,
    config_client: crate::cfg::cfg_client,
    config_forget: crate::cfg::cfg_forget_client,
    config_start: crate::cfg::start_cfg,
    config_files: crate::cfg::cfg_files,
    config_set_files: crate::cfg::cfg_set_files,
    config_report_missing: crate::cfg::cfg_report_missing,
    hooks_build: crate::hooks::hooks_build_events,
    hook_add: crate::hooks::hooks_add_event,
    hook_is_event: crate::hooks::hooks_is_event,
    bindings_init: crate::key_bindings::key_bindings_init,
    binding_dispatch: |binding, client, item, event, target| {
        let binding = crate::shared::key::KeyBindingCommand {
            key: binding.key,
            commands: binding.commands.clone(),
            flags: binding.flags,
        };
        weak(crate::key_bindings::key_bindings_dispatch(
            binding,
            item.map(native).as_ref(),
            client,
            event,
            target,
        ))
    },
    client_lost: crate::entries::wait_for::cmd_wait_for_client_lost,
    flush: crate::entries::wait_for::cmd_wait_for_flush,
    target_resolve: |target, item, text, kind, flags| {
        target.find_target(item.map(native).as_ref(), text, kind, flags)
    },
};
