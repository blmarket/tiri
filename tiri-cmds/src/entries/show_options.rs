use crate::cmd::invocation::{ExecOutcome, Invocation};
use std::future::Future;
use std::pin::Pin;
use crate::cmd::{cmd_get_args, cmd_get_entry};
use tiri_core::hooks::hooks_is_event;
use crate::shared::command::{CMD_AFTERHOOK, CMD_FIND_CANFAIL};
use crate::shared::command::{CMD_FIND_PANE, CMD_FIND_WINDOW};
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::{CStr, CString};
use std::time::{Duration, UNIX_EPOCH};
use tiri_core::arguments::{args_count, args_get, args_has, args_parse, args_string};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{format_add, format_add_time, format_expand_cstring};
use tiri_core::monitor::{
    MONITOR_ALL_PANES, MONITOR_ALL_WINDOWS, MONITOR_PANE, MONITOR_SESSION, MONITOR_WINDOW,
};
use tiri_core::monitor::{
    subscription_fire_count as hooks_monitor_get_fire_count,
    subscription_fire_time as hooks_monitor_get_fire_time,
    subscription_text as hooks_monitor_to_cstring,
};
use tiri_core::options::OPTIONS_TABLE_IS_HOOK;
use tiri_core::options::options_table;
use tiri_core::options::options_table_entry;
use tiri_core::options::{
    OptionMatchFailure, OptionsScope, options_get_fire_count, options_get_fire_time,
    options_get_monitor_data, options_is_array, options_is_string, options_iter,
    options_match_owned, options_scope_from_flags, options_scope_from_name, options_to_cstring,
};
use tiri_core::shared::abi::time_t;
use tiri_core::shared::format::format_tree;

pub const SHOW_OPTIONS_TEMPLATE: &std::ffi::CStr = c"#{?option_value_only,#{option_value},#{option_name}#{?option_has_array_key,[#{option_array_key}],}#{?option_is_parent,*,}#{?option_has_value, #{?option_is_string,#{q/a:option_value},#{option_value}},}}";
pub const SHOW_HOOKS_MONITOR_TEMPLATE: &std::ffi::CStr =
    c"#{option_name}:#{hook_monitor_target}:#{hook_monitor_format}";
pub static cmd_show_options_entry: cmd_entry = {
    cmd_entry {
        name: c"show-options",
        alias: Some(c"show"),
        args: args_parse {
            template: c"AgF:Hpqst:vw",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-AgHpqsvw] [-F format] [-t target-pane] [option]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_show_options_exec),
    }
};
pub static cmd_show_window_options_entry: cmd_entry = {
    cmd_entry {
        name: c"show-window-options",
        alias: Some(c"showw"),
        args: args_parse {
            template: c"F:gvt:",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-gv] [-F format] [-t target-window] [option]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_WINDOW,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_show_options_exec),
    }
};
pub static cmd_show_hooks_entry: cmd_entry = {
    cmd_entry {
        name: c"show-hooks",
        alias: None,
        args: args_parse {
            template: c"BF:gpt:w",
            lower: 0 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-Bgpw] [-F format] [-t target-pane] [hook]",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: b't',
            type_0: CMD_FIND_PANE,
            flags: CMD_FIND_CANFAIL,
        },
        flags: CMD_AFTERHOOK,
        exec: Some(cmd_show_options_exec),
    }
};
fn cmd_show_options_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
    let args = cmd_get_args(self_0);
    let mut target_state = inv.target().clone();
    let target = &mut target_state;
    let window = std::ptr::eq(cmd_get_entry(self_0), &cmd_show_window_options_entry) as i32;
    let show_hooks = std::ptr::eq(cmd_get_entry(self_0), &cmd_show_hooks_entry);
    let monitor = show_hooks && args_has(args, b'B') != 0;
    let quiet = args_has(args, b'q') != 0;
    if args_count(args) == 0 {
        let (scope, selected) = match options_scope_from_flags(args, window, &*target) {
            Ok(selected) => selected,
            Err(cause) => return show_options_scope_error(inv, quiet, &cause),
        };
        if monitor {
            let names = selected.with_local(|table| {
                options_iter(table)
                    .map(|(name, _)| name.to_owned())
                    .collect::<Vec<_>>()
            });
            for name in names {
                if !cmd_show_hooks_print_monitor(self_0, inv, &selected, &name) {
                    break;
                }
            }
            return ExecOutcome::ok();
        }
        return cmd_show_options_all(self_0, inv, scope, &selected);
    }

    let argument = inv.format_single( args_string(args, 0).unwrap_or(c""));
    let parsed = match options_match_owned(&argument) {
        Ok(parsed) => parsed,
        Err(failure) => {
            if quiet {
                return ExecOutcome::ok();
            }
            let ambiguous = matches!(
                failure,
                OptionMatchFailure::Parse | OptionMatchFailure::Ambiguous
            );
            inv.error( |out| {
                out.write_all(if ambiguous {
                    b"ambiguous option: "
                } else {
                    b"invalid option: "
                })?;
                write_cstr(out, &*argument)
            });
            return ExecOutcome::err();
        }
    };
    let selected = match options_scope_from_name(args, window, &parsed.name, &*target) {
        Ok((_, selected)) => selected,
        Err(cause) => return show_options_scope_error(inv, quiet, &cause),
    };
    let resolved = show_options_resolve(&selected, &parsed.name, args_has(args, b'A') != 0);
    let Some((resolved, parent)) = resolved else {
        if parsed.name.as_bytes().first() == Some(&b'@') && !quiet {
            inv.error( |out| {
                out.write_all(b"invalid option: ")?;
                write_cstr(out, &*argument)
            });
            return ExecOutcome::err();
        }
        return ExecOutcome::ok();
    };
    if monitor {
        cmd_show_hooks_print_monitor(self_0, inv, &resolved, &parsed.name);
    } else {
        cmd_show_options_print(
            self_0,
            inv,
            &resolved,
            &parsed.name,
            parsed.array_key.as_deref(),
            parent,
            true,
        );
    }
    ExecOutcome::ok()
})
}

fn show_options_scope_error(
    inv: &Invocation<'_>,
    quiet: bool,
    cause: &CStr,
) -> ExecOutcome {
    if quiet {
        return ExecOutcome::ok();
    }
    inv.error( |out| write_cstr(out, cause));
    ExecOutcome::err()
}

/// Capture the defining scope once. A later local override must not redirect an
/// inherited array scan to a different owner after a formatting callback.
fn show_options_resolve(
    scope: &OptionsScope,
    name: &CStr,
    include_parent: bool,
) -> Option<(OptionsScope, i32)> {
    if let Some(local) = scope.resolve(name, true) {
        return Some((local, 0));
    }
    include_parent
        .then(|| scope.resolve(name, false))
        .flatten()
        .map(|parent| (parent, 1))
}

fn cmd_show_options_print(
    self_0: &cmd,
    inv: &Invocation<'_>,
    scope: &OptionsScope,
    name: &CStr,
    array_key: Option<&CStr>,
    parent: i32,
    suppress_empty_parent: bool,
) {
    if array_key.is_none() {
        let Some(array) = scope.with_entry(name, |entry| {
            (options_is_array(entry) != 0).then(|| {
                (
                    entry.id(),
                    entry
                        .value
                        .array()
                        .into_iter()
                        .flat_map(|array| array.iter())
                        .map(|item| item.key.clone())
                        .collect::<Vec<_>>(),
                )
            })
        }) else {
            return;
        };
        if let Some((identity, keys)) = array {
            if !keys.is_empty() {
                for key in keys {
                    let present = scope.with_entry(name, |entry| {
                        entry.id() == identity
                            && options_is_array(entry) != 0
                            && entry
                                .value
                                .array()
                                .and_then(|array| array.get(&key))
                                .is_some()
                    });
                    if present != Some(true) {
                        break;
                    }
                    cmd_show_options_print(
                        self_0,
                        inv,
                        scope,
                        name,
                        Some(&key),
                        parent,
                        false,
                    );
                }
                return;
            }
        }
    }

    let args = cmd_get_args(self_0);
    let template = args_get(args, b'F').map(CStr::to_owned);
    let value_only = args_has(args, b'v');
    let show_hooks = std::ptr::eq(cmd_get_entry(self_0), &cmd_show_hooks_entry);
    let mut ft_owner = inv.format_from_target();
    let ft = &mut *ft_owner;
    let print = scope.with_entry(name, |entry| {
        let is_array = options_is_array(entry);
        let has_value = array_key.is_some() || is_array == 0;
        if !has_value && template.is_none() && value_only != 0 {
            return false;
        }
        let value = if has_value {
            options_to_cstring(entry, array_key, 0)
        } else {
            CString::default()
        };
        let table = options_table_entry(entry);
        format_add(ft, c"option_name", |out| write_cstr(out, name));
        format_add(ft, c"option_value", |out| write_cstr(out, &*value));
        for (name, value) in [
            (c"option_value_only", value_only as i32),
            (
                c"option_is_parent",
                if !has_value && suppress_empty_parent {
                    0
                } else {
                    parent
                },
            ),
            (c"option_is_array", is_array),
            (c"option_is_string", options_is_string(entry)),
            (
                c"option_is_hook",
                table.is_some_and(|table| table.flags & OPTIONS_TABLE_IS_HOOK != 0) as i32,
            ),
            (c"option_is_user", table.is_none() as i32),
            (c"option_has_value", has_value as i32),
            (c"option_has_array_key", array_key.is_some() as i32),
        ] {
            format_add(ft, name, |out| write!(out, "{value}"));
        }
        format_add(ft, c"option_array_key", |out| {
            write_cstr(out, array_key.unwrap_or(c""))
        });
        if show_hooks {
            show_hook_add_fire_formats(
                ft,
                options_get_fire_count(entry),
                options_get_fire_time(entry),
            );
        }
        true
    });
    if print != Some(true) {
        drop(ft_owner);
        return;
    }
    let template = template.as_deref().unwrap_or(SHOW_OPTIONS_TEMPLATE);
    let line = format_expand_cstring(ft, template);
    drop(ft_owner);
    inv.print( |out| write_cstr(out, &*line));
}

fn show_hook_add_fire_formats(ft: &mut format_tree, count: u32, time: time_t) {
    format_add(ft, c"hook_fire_count", |out| write!(out, "{count}"));
    if time != 0 {
        let offset = Duration::from_secs(time.unsigned_abs());
        let timestamp = if time > 0 {
            UNIX_EPOCH + offset
        } else {
            UNIX_EPOCH - offset
        };
        format_add_time(ft, c"hook_fire_time", timestamp);
    }
}

fn cmd_show_hooks_print_monitor(
    self_0: &cmd,
    inv: &Invocation<'_>,
    scope: &OptionsScope,
    name: &CStr,
) -> bool {
    let args = cmd_get_args(self_0);
    let template = args_get(args, b'F')
        .map(CStr::to_owned)
        .unwrap_or_else(|| SHOW_HOOKS_MONITOR_TEMPLATE.to_owned());
    let mut ft_owner = inv.format_from_target();
    let ft = &mut *ft_owner;
    let print = scope.with_entry(name, |entry| {
        let Some(value) = hooks_monitor_to_cstring(name, entry) else {
            return false;
        };
        let Some(monitor) = options_get_monitor_data(entry) else {
            return false;
        };
        let target = match monitor.type_0 {
            MONITOR_SESSION => CString::default(),
            MONITOR_PANE => CString::new(format!("%{}", monitor.id)).unwrap(),
            MONITOR_ALL_PANES => c"%*".to_owned(),
            MONITOR_WINDOW => CString::new(format!("@{}", monitor.id)).unwrap(),
            MONITOR_ALL_WINDOWS => c"@*".to_owned(),
            _ => return false,
        };
        format_add(ft, c"hook_monitor_target", |out| write_cstr(out, &*target));
        format_add(ft, c"hook_monitor_format", |out| {
            write_cstr(out, &*monitor.format)
        });
        format_add(ft, c"option_name", |out| write_cstr(out, name));
        format_add(ft, c"option_value", |out| write_cstr(out, &*value));
        for (name, value) in [
            (c"option_value_only", 0),
            (c"option_is_parent", 0),
            (c"option_is_array", 0),
            (c"option_is_string", 1),
            (c"option_is_hook", 1),
            (c"option_is_user", 1),
            (c"option_has_value", 1),
            (c"option_has_array_key", 0),
        ] {
            format_add(ft, name, |out| write!(out, "{value}"));
        }
        format_add(ft, c"option_array_key", |_| Ok(()));
        show_hook_add_fire_formats(
            ft,
            hooks_monitor_get_fire_count(name, entry),
            hooks_monitor_get_fire_time(name, entry),
        );
        true
    });
    if print == Some(true) {
        let line = format_expand_cstring(ft, &template);
        drop(ft_owner);
        inv.print( |out| write_cstr(out, &*line));
    } else {
        drop(ft_owner);
    }
    print.is_some()
}

fn cmd_show_options_all(
    self_0: &cmd,
    inv: &Invocation<'_>,
    scope: i32,
    selected: &OptionsScope,
) -> ExecOutcome {
    let show_hooks = std::ptr::eq(cmd_get_entry(self_0), &cmd_show_hooks_entry);
    let args = cmd_get_args(self_0);
    let include_hooks = args_has(args, b'H') != 0;
    let include_parent = args_has(args, b'A') != 0;
    let names = selected.with_local(|table| {
        options_iter(table)
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>()
    });
    for name in names {
        let Some((user, monitor)) = selected.with_entry(&name, |entry| {
            (
                options_table_entry(entry).is_none(),
                options_get_monitor_data(entry).is_some(),
            )
        }) else {
            break;
        };
        if !user {
            continue;
        }
        let user_hook =
            name.as_bytes().first() == Some(&b'@') && (hooks_is_event(&name) != 0 || monitor);
        if (show_hooks && user_hook) || (!show_hooks && (!user_hook || include_hooks)) {
            cmd_show_options_print(self_0, inv, selected, &name, None, 0, false);
        }
    }
    for definition in options_table
        .iter()
        .take_while(|entry| entry.name.is_some())
    {
        if !definition.scope & scope != 0 {
            continue;
        }
        let hook = definition.flags & OPTIONS_TABLE_IS_HOOK != 0;
        if (!show_hooks && !include_hooks && hook) || (show_hooks && !hook) {
            continue;
        }
        let name = definition.name.unwrap();
        if let Some((resolved, parent)) = show_options_resolve(selected, name, include_parent) {
            cmd_show_options_print(self_0, inv, &resolved, name, None, parent, false);
        }
    }
    ExecOutcome::ok()
}
