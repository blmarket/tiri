use crate::actions::sources::compile;
use crate::actions::{Context, NO_HOOKS, ProgramRef, ProgramSource, Target};
use crate::reactor::Lane;
use crate::events::{events_add_sink, events_fire};
use crate::events_payload::{
    event_payload_add_formats, event_payload_create, event_payload_get_client,
    event_payload_get_identity, event_payload_get_target, event_payload_set_client,
    event_payload_set_identity, event_payload_set_int, event_payload_set_pane,
    event_payload_set_session, event_payload_set_string, event_payload_set_target,
    event_payload_set_window,
};
use crate::format::bytes::write_cstr;
use crate::format::{
    format_add, format_create, format_create_defaults, format_expand_cstring, format_log_debug,
    format_merge,
};
use crate::log::{log_bytes, log_cstr, log_debug, log_get_level};
use crate::monitor::{
    MonitorSubscription, monitor_add, monitor_callback, monitor_change,
    monitor_create_session_owned, monitor_type,
};
use crate::options::{
    OPTIONS_TABLE_IS_HOOK, OptionsScope, options_hook_fired, options_search, options_set_string,
    options_table,
};
use crate::server_client::Client as _;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::events::{
    EventPayloadIdentity, EventSinkId, event_payload, events_callback,
};
use crate::shared::format::{CommandFormat, FORMAT_NOJOBS, FORMAT_NONE, format_tree};
use crate::shared::key::key_event;
use crate::shared::pane::window_pane;
use crate::shared::session::SessionRef;
use crate::shared::window::{WindowRef, WindowWeak};
use crate::window::WindowPane;
use crate::winlink::winlink_window;
use std::ffi::{CStr, CString};
use std::rc::Rc;

pub struct HookCommandContext {
    pub flags: i32,
    pub event: key_event,
    pub format: CommandFormat,
    pub source: Option<(CString, u32)>,
}

impl Clone for HookCommandContext {
    fn clone(&self) -> Self {
        Self {
            flags: self.flags,
            event: self.event.metadata_snapshot(),
            format: self.format.clone(),
            source: self.source.clone(),
        }
    }
}

pub fn set_running_hook_context(ctx: Option<HookCommandContext>) -> Option<HookCommandContext> {
    RUNNING_HOOK_CONTEXT.with_borrow_mut(|slot| std::mem::replace(slot, ctx))
}

pub fn running_hook_context() -> Option<HookCommandContext> {
    RUNNING_HOOK_CONTEXT.with_borrow(Clone::clone)
}

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

thread_local! {
    static HOOK_EVENTS: std::cell::RefCell<HooksEvents> = const {
        std::cell::RefCell::new(HooksEvents { events: Vec::new() })
    };
    static RUNNING_HOOK_CONTEXT: std::cell::RefCell<Option<HookCommandContext>> = const {
        std::cell::RefCell::new(None)
    };
}

fn hooks_build_one(
    hd: &hooks_data,
    commands: Option<&dyn ProgramSource>,
    exec_client: ClientWeak,
    fs: &Target,
    event: Option<&key_event>,
) -> Option<(ProgramRef, Context)> {
    let commands = commands?.program_ref();
    if log_get_level() != 0 {
        let s = commands.text(0);
        log_debug(format_args!(
            "{}: hook {} is: {}",
            "hooks_insert_one",
            log_bytes(hd.name.to_bytes()),
            log_cstr(&s)
        ));
    }
    let mut context = Context::new(exec_client, Some(fs), event, NO_HOOKS);
    context.add_formats(&hd.formats);
    Some((commands, context))
}

fn hooks_parse(hd: &mut hooks_data, fs: &Target, value: &CStr) -> Result<ProgramRef, CString> {
    if hd.expand == 0 {
        return compile(value);
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
    let pr = compile(expanded.as_c_str());
    drop(client_owner);
    pr
}

fn hooks_find(
    oo: Option<&OptionsScope>,
    name: &CStr,
    target: &Target,
) -> Option<(OptionsScope, Target)> {
    let mut fs = Target::default();
    log_debug(format_args!(
        "hooks_insert: inserting hook {}",
        log_bytes(name.to_bytes())
    ));
    if target.is_empty() || !target.is_valid() {
        fs = Target::from_nothing(0).unwrap_or_else(|| Target::clear_state(0));
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
            log_bytes(name.to_bytes())
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

fn hooks_collect(
    exec_client: ClientWeak,
    event: Option<&key_event>,
    ctx: Option<&HookCommandContext>,
    hd: &mut hooks_data,
    source: OptionsScope,
    fs: Target,
) -> Vec<(ProgramRef, Context)> {
    let mut tasks = Vec::new();
    let name = hd.name;
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
                log_bytes(name.to_bytes()),
                log_cstr(&error)
            )),
            Ok(commands) => {
                if let Some(task) = hooks_build_one(hd, Some(&commands), exec_client, &fs, event) {
                    tasks.push(task);
                }
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
                        let client = ctx.and_then(|ctx| ctx.format.client.upgrade());
                        let source_ref = ctx
                            .and_then(|ctx| ctx.source.as_ref().map(|(f, l)| (f.as_c_str(), *l)));
                        ClientRef::report_error(client.as_ref(), source_ref, |out| {
                            write_cstr(out, error.as_c_str())
                        });
                    }
                    Ok(commands) => {
                        if let Some(task) =
                            hooks_build_one(hd, Some(&commands), exec_client.clone(), &fs, event)
                        {
                            tasks.push(task);
                        }
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
                if let Some(task) = hooks_build_one(
                    hd,
                    commands
                        .as_ref()
                        .map(|commands| commands as &dyn ProgramSource),
                    exec_client.clone(),
                    &fs,
                    event,
                ) {
                    tasks.push(task);
                }
            }
        }
    }
    tasks
}

fn hooks_collect_event(
    exec_client: ClientWeak,
    ctx: Option<&HookCommandContext>,
    name: &CStr,
    ep: &event_payload,
    oo: Option<OptionsScope>,
    expand: ::core::ffi::c_int,
) -> Vec<(ProgramRef, Context)> {
    if ctx.is_some_and(|ctx| ctx.flags & NO_HOOKS != 0) {
        return Vec::new();
    }
    let mut target = Target::default();
    event_payload_get_target(ep, &mut target);
    let Some((source, fs)) = hooks_find(oo.as_ref(), name, &target) else {
        return Vec::new();
    };
    let c = event_payload_get_client(ep);
    let mut formats = format_create(c, ctx.map(|ctx| &ctx.format), FORMAT_NONE, FORMAT_NOJOBS);
    let ft = &mut *formats;
    event_payload_add_formats(ep, ft, Some(c"hook_"));
    format_add(ft, c"hook", |out| write_cstr(out, &*name));
    format_log_debug(ft, c"hooks_insert_event");
    let mut hd = hooks_data {
        name,
        formats,
        client: c.map(Rc::downgrade).unwrap_or_default(),
        expand,
    };
    let tasks = hooks_collect(
        exec_client,
        ctx.map(|ctx| &ctx.event),
        ctx,
        &mut hd,
        source,
        fs,
    );
    drop(hd.formats);
    tasks
}

pub fn hooks_fire_command(
    identity: usize,
    ctx: &HookCommandContext,
    name: &CStr,
    mut ep: Box<event_payload>,
) -> Vec<(ProgramRef, Context)> {
    event_payload_set_identity(
        &mut ep,
        c"_hooks_command",
        EventPayloadIdentity::CommandHook(identity),
    );
    let tasks = hooks_collect_event(ctx.format.client.clone(), Some(ctx), name, &ep, None, 0);
    events_fire(name, ep);
    tasks
}

fn hooks_event_cb(name: &CStr, payload: &mut event_payload) {
    let ep = &*payload;
    if matches!(
        event_payload_get_identity(ep, c"_hooks_monitor"),
        Some(EventPayloadIdentity::HookMonitor(_))
    ) || matches!(
        event_payload_get_identity(ep, c"_hooks_command"),
        Some(EventPayloadIdentity::CommandHook(_))
    ) {
        return;
    }
    let suppress = RUNNING_HOOK_CONTEXT
        .with_borrow(|slot| slot.as_ref().is_some_and(|ctx| ctx.flags & NO_HOOKS != 0));
    if !suppress {
        for (program, context) in
            hooks_collect_event(std::rc::Weak::new(), None, name, ep, None, 0)
        {
            Lane::push(None, async move {
                let _ = crate::actions::run(program, context).await;
            });
        }
    }
}

pub fn hooks_add_event(name: &CStr) {
    if HOOK_EVENTS.with_borrow(|events| events.contains(name)) {
        return;
    }
    events_add_sink(name, events_callback(hooks_event_cb));
    HOOK_EVENTS.with_borrow_mut(|events| events.insert(name.to_owned()));
}

pub fn hooks_is_event(name: &CStr) -> ::core::ffi::c_int {
    HOOK_EVENTS.with_borrow(|events| events.contains(name)) as ::core::ffi::c_int
}

pub fn hooks_valid_event_name(name: &CStr) -> ::core::ffi::c_int {
    if name.to_bytes().first() == Some(&b'@') {
        return 1;
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

pub fn hooks_run(
    target_state: &Target,
    client: ClientWeak,
    event: &key_event,
    name: &CStr,
) -> Vec<(ProgramRef, Context)> {
    let Some((source, fs)) = hooks_find(None, name, target_state) else {
        return Vec::new();
    };
    let mut hd = hooks_data {
        name,
        formats: format_create(None, None, 0, FORMAT_NOJOBS),
        client: client.clone(),
        expand: 0,
    };
    format_add(&mut hd.formats, c"hook", |out| write_cstr(out, &*name));
    format_log_debug(&mut hd.formats, c"hooks_run");
    let tasks = hooks_collect(client, Some(event), None, &mut hd, source, fs);
    drop(hd.formats);
    tasks
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
        Some(EventPayloadIdentity::HookMonitor(id)) if *id == generation)
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
    let active = RUNNING_HOOK_CONTEXT.with_borrow(Clone::clone);
    for (program, context) in hooks_collect_event(
        std::rc::Weak::new(),
        active.as_ref(),
        name,
        payload,
        Some(scope.clone()),
        1,
    ) {
        Lane::push(None, async move {
            let _ = crate::actions::run(program, context).await;
        });
    }
}

fn hooks_monitor_cb(change: &monitor_change, fallback: &Target, generation: usize) {
    let wl = change.wl.clone();
    let client_owner = change.c.upgrade();
    let session_owner = change.s.upgrade();
    let s = session_owner.clone();
    let pane_owner = Rc::<std::cell::RefCell<window_pane>>::from_observer(&change.wp);
    let mut ep = event_payload_create();
    event_payload_set_identity(
        &mut ep,
        c"_hooks_monitor",
        EventPayloadIdentity::HookMonitor(generation),
    );
    let mut fs = Target::clear_state(0);
    if let Some(pane) = pane_owner.as_ref().filter(|pane| {
        wl.is_alive()
            && pane.window_observer().ptr_eq(
                &winlink_window(&wl)
                    .map_or_else(WindowWeak::new, |window| WindowRef::downgrade(&window)),
            )
    }) {
        fs = Target::from_winlink_pane(wl.clone(), pane, 0);
    } else if wl.is_alive() {
        fs = Target::from_winlink(wl.clone(), 0);
    } else if let Some(pane_owner_value) = pane_owner.as_ref() {
        fs = Target::from_pane(pane_owner_value, 0).unwrap_or_else(|| Target::clear_state(0));
    } else if let Some(session) = s.as_ref() {
        fs = Target::from_session(session, 0);
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
    if let Some(session) = s.as_ref() {
        event_payload_set_session(&mut ep, c"session", session.clone());
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
    fs: &Target,
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
    let mut fallback = Target::default();
    fallback.copy_from(fs);
    let callback_scope = scope.clone();
    let mut owner = Box::new(MonitorSubscription {
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
