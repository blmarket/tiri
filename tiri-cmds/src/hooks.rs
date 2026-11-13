use crate::cmd::cmd_list_print_cstring;
use crate::cmd::parse::cmd_parse_from_string;
use crate::cmd::queue::cmdq_item;
use crate::cmd::queue::{
    cmdq_add_formats, cmdq_append, cmdq_error, cmdq_get_command, cmdq_get_flags, cmdq_insert_after,
    cmdq_running,
};
use crate::shared::command::CMDQ_STATE_NOHOOKS;
use crate::shared::command::cmd_list;
use std::ffi::{CStr, CString};
use tiri_core::actions::ExecutionState as cmdq_state;
use tiri_core::actions::queue::{event as cmdq_get_event, new_state as cmdq_new_state};
use tiri_core::events::{events_add_sink, events_fire};
use tiri_core::events_payload::{
    event_payload_add_formats, event_payload_create, event_payload_get_client,
    event_payload_get_identity, event_payload_get_target, event_payload_set_client,
    event_payload_set_identity, event_payload_set_int, event_payload_set_pane,
    event_payload_set_session, event_payload_set_string, event_payload_set_target,
    event_payload_set_window,
};
use tiri_core::format::bytes::write_cstr;
use tiri_core::format::{
    format_add, format_create, format_create_defaults, format_expand_cstring, format_log_debug,
    format_merge,
};
use tiri_core::log::{log_cstr, log_debug, log_get_level};
use tiri_core::monitor::{
    monitor_add, monitor_callback, monitor_change, monitor_create_session_owned, monitor_type,
};
use tiri_core::options::OPTIONS_TABLE_IS_HOOK;
use tiri_core::options::options_table;
use tiri_core::options::{OptionsScope, options_hook_fired, options_search, options_set_string};
use tiri_core::shared::client::ClientWeak;
use tiri_core::shared::events::{EventSinkId, event_payload, events_callback};
use tiri_core::shared::format::format_tree;
use tiri_core::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use tiri_core::shared::pane::window_pane;
use tiri_core::shared::session::SessionRef;
use tiri_core::shared::target::Target as cmd_find_state;
use tiri_core::shared::window::{WindowRef, WindowWeak};
use tiri_core::window::WindowPane;
use tiri_core::winlink::winlink_window;

#[derive(Default)]
struct HooksEvents {
    events: Vec<CString>,
}

impl HooksEvents {
    fn contains(&self, name: &CStr) -> bool {
        self.events
            .iter()
            .any(|event| event.as_bytes() == name.to_bytes())
    }

    fn insert(&mut self, name: CString) -> bool {
        if self.contains(name.as_c_str()) {
            return false;
        }
        self.events.push(name);
        true
    }
}
#[repr(C)]
pub struct hooks_data<'a> {
    pub name: &'a CStr,
    pub formats: Box<format_tree>,
    pub client: ClientWeak,
    pub expand: ::core::ffi::c_int,
}
use tiri_core::monitor::MonitorSubscription as hooks_monitor;

thread_local! {
    static CStrings: std::cell::RefCell<HooksEvents> = const {
        std::cell::RefCell::new(HooksEvents { events: Vec::new() })
    };
}
fn hooks_insert_one(
    item_handle: Option<&std::rc::Rc<cmdq_item>>,
    hd: &hooks_data,
    commands: Option<&dyn tiri_core::actions::ProgramSource>,
    state: &std::rc::Rc<cmdq_state>,
) -> std::rc::Weak<cmdq_item> {
    let Some(commands) = commands else {
        return item_handle.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    };
    let commands = commands.program_ref();
    if log_get_level() != 0 as ::core::ffi::c_int {
        let s = cmd_list_print_cstring(&commands, 0 as ::core::ffi::c_int);
        log_debug(format_args!(
            "{}: hook {} is: {}",
            "hooks_insert_one",
            tiri_core::log::log_bytes(hd.name.to_bytes()),
            log_cstr(&s)
        ));
    }
    let new_item_allocation = cmdq_get_command(&commands, Some(state));
    if let Some(item) = item_handle {
        return cmdq_insert_after(item, new_item_allocation);
    }
    cmdq_append(None, new_item_allocation)
}
fn hooks_parse(
    hd: &mut hooks_data,
    fs: &cmd_find_state,
    value: &CStr,
) -> Result<std::rc::Rc<cmd_list>, CString> {
    if hd.expand == 0 {
        return cmd_parse_from_string(value, None);
    }
    let client_owner = hd.client.upgrade();
    let mut ft_owner = format_create_defaults(
        None,
        client_owner.as_ref(),
        fs.session_handle().as_ref(),
        (fs.winlink_handle()).clone(),
        fs.pane_handle().as_ref(),
    );
    let ft = &mut *ft_owner;
    format_merge(ft, &hd.formats);
    let expanded = format_expand_cstring(ft, value);
    drop(ft_owner);
    let pr = cmd_parse_from_string(expanded.as_c_str(), None);
    drop(client_owner);
    pr
}
fn hooks_find(
    oo: Option<&OptionsScope>,
    name: &CStr,
    target: &cmd_find_state,
) -> Option<(OptionsScope, cmd_find_state)> {
    let mut fs = cmd_find_state::default();
    log_debug(format_args!(
        "hooks_insert: inserting hook {}",
        tiri_core::log::log_bytes(name.to_bytes())
    ));
    if target.is_empty() || !target.is_valid() {
        fs = cmd_find_state::from_nothing(0).unwrap_or_else(|| cmd_find_state::clear_state(0));
    } else {
        fs.copy_from(target);
    }
    let source = if let Some(scope) = oo {
        scope.resolve(name, true)
    } else {
        let scope = if fs.s.strong_count() == 0 {
            OptionsScope::GlobalSession
        } else {
            OptionsScope::Session(fs.s.clone())
        };
        let mut source = scope.resolve(name, false);
        if source.is_none() && fs.wp.strong_count() != 0 {
            source = OptionsScope::Pane(fs.wp.clone()).resolve(name, false);
        }
        if source.is_none() && fs.wl.is_alive() {
            let link = fs.wl.try_borrow().expect("live winlink");
            let window = link.window_handle().expect("hook window");
            source = OptionsScope::Window(WindowRef::downgrade(window)).resolve(name, false);
        }
        source
    };
    let Some(source) = source else {
        log_debug(format_args!(
            "hooks_insert: hook {} not found",
            tiri_core::log::log_bytes(name.to_bytes())
        ));
        return None;
    };
    let runnable = source
        .with_entry(name, |entry| {
            options_hook_fired(entry);
            name.to_bytes().starts_with(b"@")
                || entry
                    .value
                    .array()
                    .is_some_and(|array| array.iter().next().is_some())
        })
        .expect("resolved hook");
    runnable.then_some((source, fs))
}
fn hooks_insert(
    item_handle: Option<&std::rc::Rc<cmdq_item>>,
    hd: &mut hooks_data,
    source: OptionsScope,
    fs: cmd_find_state,
) {
    let mut after = item_handle.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    let name = hd.name;
    let event = item_handle.map(|item| cmdq_get_event(&item.borrow()));
    let state = cmdq_new_state(Some(&fs), event.as_ref(), CMDQ_STATE_NOHOOKS);
    cmdq_add_formats(&state, &hd.formats);
    if name.to_bytes().starts_with(b"@") {
        let value = source
            .with_entry(name, |entry| {
                entry.value.string_ptr().expect("hook string").to_owned()
            })
            .expect("resolved hook");
        let pr = hooks_parse(hd, &fs, &value);
        match pr {
            Err(error) => log_debug(format_args!(
                "hooks_insert: can't parse hook {}: {}",
                tiri_core::log::log_bytes(name.to_bytes()),
                log_cstr(&error)
            )),
            Ok(commands) => {
                hooks_insert_one(after.upgrade().as_ref(), hd, Some(&commands), &state);
            }
        }
    } else {
        let keys = source
            .with_entry(name, |entry| {
                entry
                    .value
                    .array()
                    .into_iter()
                    .flat_map(|array| array.iter())
                    .map(|item| item.key.clone())
                    .collect::<Vec<_>>()
            })
            .expect("resolved hook");
        for key in keys {
            if hd.expand != 0 {
                let value = source
                    .with_entry(name, |entry| {
                        entry
                            .value
                            .array()
                            .and_then(|array| array.get(&key))
                            .map(|value| {
                                value.string_ptr().expect("expanded hook string").to_owned()
                            })
                    })
                    .flatten();
                let Some(value) = value else {
                    break;
                };
                let pr = hooks_parse(hd, &fs, &value);
                match pr {
                    Err(error) => {
                        cmdq_error(item_handle.expect("hook command item"), |out| {
                            write_cstr(out, error.as_c_str())
                        });
                    }
                    Ok(commands) => {
                        after =
                            hooks_insert_one(after.upgrade().as_ref(), hd, Some(&commands), &state);
                    }
                }
            } else {
                let commands = source
                    .with_entry(name, |entry| {
                        entry
                            .value
                            .array()
                            .and_then(|array| array.get(&key))
                            .map(|value| value.commands().cloned())
                    })
                    .flatten();
                let Some(commands) = commands else {
                    break;
                };
                after = hooks_insert_one(
                    after.upgrade().as_ref(),
                    hd,
                    commands
                        .as_ref()
                        .map(|commands| commands as &dyn tiri_core::actions::ProgramSource),
                    &state,
                );
            }
        }
    }
}
fn hooks_insert_event(
    item_handle: Option<&std::rc::Rc<cmdq_item>>,
    name: &CStr,
    ep: &event_payload,
    oo: Option<OptionsScope>,
    expand: ::core::ffi::c_int,
) {
    if item_handle.is_some_and(|item| cmdq_get_flags(&item.borrow()) & CMDQ_STATE_NOHOOKS != 0) {
        return;
    }
    let mut target = cmd_find_state::default();
    event_payload_get_target(ep, &mut target);
    let Some((source, fs)) = hooks_find(oo.as_ref(), name, &target) else {
        return;
    };
    let c = event_payload_get_client(ep);
    let mut formats = format_create(
        c,
        item_handle.map(|item| item as &dyn tiri_core::actions::ContextSource),
        FORMAT_NONE,
        FORMAT_NOJOBS,
    );
    let ft = &mut *formats;
    event_payload_add_formats(ep, ft, Some(c"hook_"));
    format_add(ft, c"hook", |out| write_cstr(out, &*name));
    format_log_debug(ft, c"hooks_insert_event");
    let mut hd = hooks_data {
        name,
        formats,
        client: c.map(std::rc::Rc::downgrade).unwrap_or_default(),
        expand,
    };
    hooks_insert(item_handle, &mut hd, source, fs);
    drop(hd.formats);
}
fn CString_cb(name: &CStr, payload: &mut event_payload) {
    let ep = &*payload;
    if matches!(
        event_payload_get_identity(ep, c"_hooks_monitor"),
        Some(tiri_core::shared::events::EventPayloadIdentity::HookMonitor(_))
    ) {
        return;
    }
    if let Some(tiri_core::shared::events::EventPayloadIdentity::Operation(observer)) =
        event_payload_get_identity(ep, c"_cmdq_item")
    {
        let Some(item_owner) = observer.upgrade() else {
            return;
        };
        hooks_insert_event(
            Some(&crate::runtime::native(&item_owner)),
            name,
            ep,
            None,
            0 as ::core::ffi::c_int,
        );
        return;
    }
    let running = cmdq_running().upgrade();
    if running
        .as_ref()
        .is_none_or(|item| cmdq_get_flags(&item.borrow()) & CMDQ_STATE_NOHOOKS == 0)
    {
        hooks_insert_event(None, name, ep, None, 0 as ::core::ffi::c_int);
    }
}
pub fn hooks_add_event(name: &CStr) {
    let event_name = name;
    if CStrings.with_borrow(|events| events.contains(event_name)) {
        return;
    }

    events_add_sink(event_name, events_callback(CString_cb));
    CStrings.with_borrow_mut(|events| events.insert(event_name.to_owned()));
}
pub fn hooks_is_event(name: &CStr) -> ::core::ffi::c_int {
    CStrings.with_borrow(|events| events.contains(name)) as ::core::ffi::c_int
}
pub fn hooks_valid_event_name(name: &CStr) -> ::core::ffi::c_int {
    if name.to_bytes().first() == Some(&b'@') {
        return 1 as ::core::ffi::c_int;
    }
    options_search(name).is_some_and(|oe| oe.flags & OPTIONS_TABLE_IS_HOOK != 0)
        as ::core::ffi::c_int
}
pub fn hooks_build_events() {
    for oe in options_table
        .iter()
        .take_while(|entry| entry.name.is_some())
    {
        if oe.flags & OPTIONS_TABLE_IS_HOOK != 0 {
            hooks_add_event(oe.name.expect("named option"));
        }
    }
}
pub fn hooks_run(item_handle: Option<&std::rc::Rc<cmdq_item>>, name: &CStr) {
    let (target_state, client) = {
        let item = item_handle.expect("command queue item").borrow();
        (
            tiri_core::actions::queue::target(&item).clone(),
            item.client.clone(),
        )
    };
    let Some((source, fs)) = hooks_find(None, name, &target_state) else {
        return;
    };
    let mut hd = hooks_data {
        name,
        formats: format_create(None, None, 0, FORMAT_NOJOBS),
        client,
        expand: 0,
    };
    format_add(&mut hd.formats, c"hook", |out| write_cstr(out, &*name));
    format_log_debug(&mut hd.formats, c"hooks_run");
    hooks_insert(item_handle, &mut hd, source, fs);
    drop(hd.formats);
}

pub fn hooks_monitor_remove(scope: &OptionsScope, name: &CStr) {
    let old = scope
        .with_entry(name, |entry| entry.monitor_data.take())
        .flatten();
    drop(old);
}
fn hooks_monitor_hook_cb(
    name: &CStr,
    payload: &mut event_payload,
    scope: &OptionsScope,
    generation: usize,
) {
    if !matches!(event_payload_get_identity(payload, c"_hooks_monitor"),
        Some(tiri_core::shared::events::EventPayloadIdentity::HookMonitor(id)) if *id == generation)
    {
        return;
    }
    if scope.with_entry(name, |entry| {
        entry
            .monitor_data
            .as_ref()
            .is_some_and(|monitor| monitor.generation == generation)
    }) != Some(true)
    {
        return;
    }
    hooks_insert_event(
        cmdq_running().upgrade().as_ref(),
        name,
        payload,
        Some(scope.clone()),
        1,
    );
}
fn hooks_monitor_cb(change: &monitor_change, fallback: &cmd_find_state, generation: usize) {
    let wl = change.wl.clone();
    let client_owner = change.c.upgrade();
    let session_owner = change.s.upgrade();
    let s = session_owner.clone();
    let pane_owner = std::rc::Rc::<std::cell::RefCell<window_pane>>::from_observer(&change.wp);
    let mut fs: cmd_find_state = cmd_find_state {
        flags: 0,
        s: Default::default(),
        wl: Default::default(),
        w: Default::default(),
        wp: Default::default(),
        idx: 0,
    };
    let mut ep = event_payload_create();
    event_payload_set_identity(
        &mut ep,
        c"_hooks_monitor",
        tiri_core::shared::events::EventPayloadIdentity::HookMonitor(generation),
    );
    fs = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
    if let Some(pane) = pane_owner.as_ref().filter(|pane| {
        wl.is_alive()
            && pane.window_observer().ptr_eq(
                &winlink_window(&wl)
                    .map_or_else(WindowWeak::new, |window| WindowRef::downgrade(&window)),
            )
    }) {
        fs = cmd_find_state::from_winlink_pane(wl.clone(), pane, 0 as ::core::ffi::c_int);
    } else if wl.is_alive() {
        fs = cmd_find_state::from_winlink(wl.clone(), 0 as ::core::ffi::c_int);
    } else if let Some(pane_owner_value) = pane_owner.as_ref() {
        fs = cmd_find_state::from_pane(pane_owner_value, 0 as ::core::ffi::c_int)
            .unwrap_or_else(|| cmd_find_state::clear_state(0 as ::core::ffi::c_int));
    } else if !s.is_none() {
        fs = cmd_find_state::from_session(
            &s.clone().expect("live session"),
            0 as ::core::ffi::c_int,
        );
    } else {
        fs.copy_from(fallback);
    }
    event_payload_set_string(&mut ep, c"value", |out| {
        out.write_all(change.value.to_bytes())
    });
    if let Some(last) = change.last {
        event_payload_set_string(&mut ep, c"last", |out| out.write_all(last.to_bytes()));
    } else {
        event_payload_set_string(&mut ep, c"last", |out| write_cstr(out, c""));
    }
    if let Some(client) = client_owner.as_ref() {
        event_payload_set_client(&mut ep, client.clone());
    }
    if !s.is_none() {
        event_payload_set_session(&mut ep, c"session", s.clone().expect("live session"));
    }
    if wl.is_alive() {
        if s.is_none() {
            let session_owner = wl.try_borrow().expect("live winlink").session.upgrade();
            if let Some(session_owner) = session_owner {
                event_payload_set_session(&mut ep, c"session", session_owner.clone());
            }
        }
        event_payload_set_window(
            &mut ep,
            c"window",
            winlink_window(&wl).expect("live window"),
        );
        event_payload_set_int(
            &mut ep,
            c"window_index",
            wl.try_borrow().expect("live winlink").idx,
        );
    }
    if let Some(pane_owner_value) = pane_owner.as_ref() {
        event_payload_set_pane(&mut ep, c"pane", pane_owner_value.clone());
        if !wl.is_alive() {
            event_payload_set_window(
                &mut ep,
                c"window",
                pane_owner_value
                    .window_observer()
                    .upgrade()
                    .expect("monitor pane window"),
            );
        }
    }
    event_payload_set_target(&mut ep, &fs);
    events_fire(change.name, ep);
    drop(session_owner);
    drop(client_owner);
}
pub fn hooks_monitor_add(
    scope: &OptionsScope,
    name: &CStr,
    type_0: monitor_type,
    id: ::core::ffi::c_int,
    format: &CStr,
    flags: ::core::ffi::c_int,
    fs: &cmd_find_state,
    s_owner: Option<&SessionRef>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_MONITOR: AtomicUsize = AtomicUsize::new(1);
    hooks_monitor_remove(scope, name);
    if scope.with_entry(name, |_| ()).is_none() {
        scope.with_local(|table| {
            options_set_string(table, name, 0, |out| out.write_all(b""));
        });
    }
    let generation = NEXT_MONITOR
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("hook monitor identity exhausted");
    let mut fallback = cmd_find_state::default();
    fallback.copy_from(fs);
    let callback_scope = scope.clone();
    let mut owner = Box::new(hooks_monitor {
        generation,
        set: Some(monitor_create_session_owned(
            s_owner,
            monitor_callback(move |change| hooks_monitor_cb(change, &fallback, generation)),
        )),
        sink: EventSinkId::default(),
        type_0,
        id,
        format: format.to_owned(),
    });
    owner.sink = events_add_sink(
        name,
        events_callback(move |name, payload| {
            hooks_monitor_hook_cb(name, payload, &callback_scope, generation)
        }),
    );
    let previous = scope
        .with_entry(name, |entry| entry.monitor_data.replace(owner))
        .expect("monitor option");
    drop(previous);
    let set = scope
        .with_entry(name, |entry| {
            entry
                .monitor_data
                .as_ref()
                .expect("installed monitor")
                .set
                .as_ref()
                .expect("monitor set")
                .downgrade()
        })
        .expect("monitor option");
    monitor_add(&set, name, type_0, id, format, flags);
}
