use crate::src::cmd::cmd_list_print_cstring;
use crate::src::cmd::find::{
    cmd_find_copy_state, cmd_find_empty_state, cmd_find_from_nothing,
    cmd_find_from_pane, cmd_find_from_session, cmd_find_from_winlink, cmd_find_from_winlink_pane,
    cmd_find_valid_state,
};
use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::cmd::queue::{
    cmdq_add_formats, cmdq_append, cmdq_error, cmdq_get_command, cmdq_get_event, cmdq_get_flags,
    cmdq_insert_after, cmdq_new_state, cmdq_running,
};
use crate::src::events::{events_add_sink, events_fire, events_remove_sink};
use crate::src::events_payload::{
    event_payload_add_formats, event_payload_create, event_payload_get_client,
    event_payload_get_identity, event_payload_get_target, event_payload_set_client,
    event_payload_set_identity, event_payload_set_int, event_payload_set_pane,
    event_payload_set_session, event_payload_set_string, event_payload_set_target,
    event_payload_set_window,
};
use crate::src::format::bytes::write_cstr;
use crate::src::format::{
    format_add, format_create_defaults, format_create_owned, format_expand_cstring, format_free,
    format_log_debug, format_merge,
};
use crate::src::log::{log_cstr, log_debug, log_get_level};
use crate::src::monitor::{
    monitor_add, monitor_create_session_owned, monitor_get_fire_count, monitor_get_fire_time,
};
use crate::src::options::{
    options_get_monitor_data, options_hook_fired, options_name, options_search, options_set_string,
    OptionsScope,
};
use crate::src::options_table::options_table;
use crate::src::shared::abi::*;
use crate::src::shared::client::ClientWeak;
use crate::src::shared::command::CMDQ_STATE_NOHOOKS;
use crate::src::shared::command::{cmd_find_state, cmd_list, cmdq_item, cmdq_state};
use crate::src::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::src::shared::events::{event_payload, events_callback, EventSinkId};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use crate::src::shared::monitor::monitor_type;
use crate::src::shared::monitor::{monitor_callback, monitor_change, monitor_set};
use crate::src::shared::options::options_entry;
use crate::src::shared::options::OPTIONS_TABLE_IS_HOOK;
use crate::src::shared::pane::window_pane;
use crate::src::shared::session::SessionRef;
use crate::src::shared::window::{WindowRef, WindowWeak};
use crate::src::window::WindowPane;
use std::ffi::{CStr, CString};

#[derive(Default)]
struct HooksEvents {
    // Boxes keep each event record stable when the owning vector grows.
    events: Vec<Box<CString>>,
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
        self.events.push(Box::new(name));
        true
    }
}
#[repr(C)]
pub struct hooks_data<'a> {
    pub name: &'a CStr,
    pub fs: cmd_find_state,
    pub formats: Box<format_tree>,
    pub oo: Option<OptionsScope>,
    pub client: ClientWeak,
    pub expand: ::core::ffi::c_int,
}
#[repr(C)]
pub struct hooks_monitor {
    // Callbacks identify this record by owner, option name and generation.
    pub generation: usize,
    pub set: Option<refbox::RefBox<monitor_set>>,
    pub sink: EventSinkId,
    pub type_0: monitor_type,
    pub id: ::core::ffi::c_int,
    pub format: CString,
}

static mut CStrings: HooksEvents = HooksEvents { events: Vec::new() };
unsafe fn hooks_insert_one(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    mut hd: *mut hooks_data,
    commands: Option<&std::rc::Rc<std::cell::RefCell<cmd_list>>>,
    state: &std::rc::Rc<cmdq_state>,
) -> std::rc::Weak<std::cell::UnsafeCell<cmdq_item>> {
    let Some(commands) = commands else {
        return item_handle.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    };
    if log_get_level() != 0 as ::core::ffi::c_int {
        let s = cmd_list_print_cstring(&commands.borrow(), 0 as ::core::ffi::c_int);
        log_debug(format_args!(
            "{}: hook {} is: {}",
            "hooks_insert_one",
            crate::src::log::log_bytes((*hd).name.to_bytes()),
            log_cstr(&s)
        ));
    }
    let new_item_allocation = cmdq_get_command(commands, Some(state));
    if let Some(item) = item_handle {
        return cmdq_insert_after(item, new_item_allocation);
    }
    cmdq_append(None, new_item_allocation)
}
unsafe fn hooks_parse(hd: *mut hooks_data, fs: &cmd_find_state, value: &CStr) -> cmd_parse_result {
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    if (*hd).expand == 0 {
        return cmd_parse_from_string(value, ::core::ptr::null_mut::<cmd_parse_input>());
    }
    let client_owner = (*hd).client.upgrade();
    let mut ft_owner = format_create_defaults(
        None,
        client_owner.as_ref(),
        fs.session_handle().as_ref(),
        (fs.winlink_handle()).clone(),
        fs.pane_handle().as_ref(),
    );
    ft = &raw mut *ft_owner;
    format_merge(ft, &raw mut *(*hd).formats);
    let expanded = format_expand_cstring(ft, value.as_ptr());
    format_free(ft_owner);
    let pr = cmd_parse_from_string(
        expanded.as_c_str(),
        ::core::ptr::null_mut::<cmd_parse_input>(),
    );
    drop(client_owner);
    pr
}
unsafe fn hooks_insert(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    hd: *mut hooks_data,
) {
    let mut after = item_handle.map_or_else(std::rc::Weak::new, std::rc::Rc::downgrade);
    let mut fs = cmd_find_state::default();
    log_debug(format_args!(
        "hooks_insert: inserting hook {}",
        crate::src::log::log_bytes((*hd).name.to_bytes())
    ));
    if cmd_find_empty_state(&(*hd).fs) != 0 || cmd_find_valid_state(&(*hd).fs) == 0 {
        cmd_find_from_nothing(&mut fs, 0);
    } else {
        cmd_find_copy_state(&mut fs, &(*hd).fs);
    }
    let name = (*hd).name;
    let source = if let Some(scope) = &(*hd).oo {
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
            let link = fs.wl.get_unchecked();
            let window = link.window_handle().expect("hook window");
            source = OptionsScope::Window(WindowRef::downgrade(window)).resolve(name, false);
        }
        source
    };
    let Some(source) = source else {
        log_debug(format_args!(
            "hooks_insert: hook {} not found",
            crate::src::log::log_bytes(name.to_bytes())
        ));
        return;
    };
    source
        .with_entry(name, |entry| options_hook_fired(entry))
        .expect("resolved hook");
    let mut event = item_handle.map(|item| cmdq_get_event(&*item.get()));
    let state = cmdq_new_state(
        &mut fs,
        event.as_mut().map_or(std::ptr::null_mut(), |event| event),
        CMDQ_STATE_NOHOOKS,
    );
    cmdq_add_formats(&state, &mut *(*hd).formats);
    if name.to_bytes().starts_with(b"@") {
        let value = source
            .with_entry(name, |entry| {
                entry.value.string_ptr().expect("hook string").to_owned()
            })
            .expect("resolved hook");
        let pr = hooks_parse(hd, &fs, &value);
        match pr.status {
            0 => log_debug(format_args!(
                "hooks_insert: can't parse hook {}: {}",
                crate::src::log::log_bytes(name.to_bytes()),
                log_cstr(pr.error.as_deref().unwrap_or(c"(null)"))
            )),
            1 => {
                hooks_insert_one(after.upgrade().as_ref(), hd, pr.cmdlist.as_ref(), &state);
            }
            _ => {}
        }
    } else {
        let keys = source
            .with_entry(name, |entry| {
                crate::src::options::options_array_iter(entry)
                    .map(|item| item.key.clone())
                    .collect::<Vec<_>>()
            })
            .expect("resolved hook");
        for key in keys {
            if (*hd).expand != 0 {
                let value = source
                    .with_entry(name, |entry| {
                        crate::src::options::options_array_get(entry, &key).map(|value| {
                            value.string_ptr().expect("expanded hook string").to_owned()
                        })
                    })
                    .flatten();
                let Some(value) = value else {
                    break;
                };
                let pr = hooks_parse(hd, &fs, &value);
                match pr.status {
                    0 => {
                        if let Some(error) = pr.error.as_ref() {
                            cmdq_error(item_handle.expect("hook command item"), |out| {
                                write_cstr(out, (&*error).as_c_str())
                            });
                        }
                    }
                    1 => {
                        after = hooks_insert_one(
                            after.upgrade().as_ref(),
                            hd,
                            pr.cmdlist.as_ref(),
                            &state,
                        );
                    }
                    _ => {}
                }
            } else {
                let commands = source
                    .with_entry(name, |entry| {
                        crate::src::options::options_array_get(entry, &key)
                            .map(|value| value.commands().cloned())
                    })
                    .flatten();
                let Some(commands) = commands else {
                    break;
                };
                after = hooks_insert_one(after.upgrade().as_ref(), hd, commands.as_ref(), &state);
            }
        }
    }
}
unsafe fn hooks_insert_event(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    name: &CStr,
    ep: &event_payload,
    oo: Option<OptionsScope>,
    expand: ::core::ffi::c_int,
) {
    let item = item_handle.map_or(std::ptr::null_mut(), |item| item.get());
    if !item.is_null() && cmdq_get_flags(&*(item)) & CMDQ_STATE_NOHOOKS != 0 {
        return;
    }
    let c = event_payload_get_client(ep);
    let mut formats = format_create_owned(c, item_handle, FORMAT_NONE, FORMAT_NOJOBS);
    let ft = &raw mut *formats;
    event_payload_add_formats(ep, ft, Some(c"hook_"));
    format_add(ft, c"hook", |out| write_cstr(out, &*name));
    format_log_debug(ft, c"hooks_insert_event");
    let mut hd = hooks_data {
        name: name,
        fs: cmd_find_state::default(),
        formats,
        oo,
        client: c.map(std::rc::Rc::downgrade).unwrap_or_default(),
        expand,
    };
    event_payload_get_target(ep, &mut hd.fs);
    hooks_insert(item_handle, &raw mut hd);
    format_free(hd.formats);
}
unsafe fn CString_cb(name: &CStr, payload: &mut event_payload) {
    let name = name.as_ptr();
    let ep = &*payload;
    if matches!(
        event_payload_get_identity(ep, c"_hooks_monitor"),
        Some(crate::src::shared::events::EventPayloadIdentity::HookMonitor(_))
    ) {
        return;
    }
    if let Some(crate::src::shared::events::EventPayloadIdentity::QueueItem(observer)) =
        event_payload_get_identity(ep, c"_cmdq_item")
    {
        let Some(item_owner) = observer.upgrade() else {
            return;
        };
        hooks_insert_event(
            Some(&item_owner),
            CStr::from_ptr(name),
            ep,
            None,
            0 as ::core::ffi::c_int,
        );
        return;
    }
    let running = cmdq_running().upgrade();
    if running
        .as_ref()
        .is_none_or(|item| cmdq_get_flags(&*item.get()) & CMDQ_STATE_NOHOOKS == 0)
    {
        hooks_insert_event(
            None,
            CStr::from_ptr(name),
            ep,
            None,
            0 as ::core::ffi::c_int,
        );
    }
}
pub unsafe fn hooks_add_event(name: &CStr) {
    let event_name = name;
    let events = &raw const CStrings;
    if (*events).contains(event_name) {
        return;
    }

    events_add_sink(
        event_name,
        events_callback(|name, payload| unsafe { CString_cb(name, payload) }),
    );
    let events = &raw mut CStrings;
    (*events).insert(event_name.to_owned());
}
pub unsafe fn hooks_is_event(name: &CStr) -> ::core::ffi::c_int {
    let events = &raw const CStrings;
    (*events).contains(name) as ::core::ffi::c_int
}
pub fn hooks_valid_event_name(name: &CStr) -> ::core::ffi::c_int {
    if name.to_bytes().first() == Some(&b'@') {
        return 1 as ::core::ffi::c_int;
    }
    options_search(name).is_some_and(|oe| oe.flags & OPTIONS_TABLE_IS_HOOK != 0)
        as ::core::ffi::c_int
}
pub unsafe fn hooks_build_events() {
    for oe in options_table
        .iter()
        .take_while(|entry| entry.name.is_some())
    {
        if oe.flags & OPTIONS_TABLE_IS_HOOK != 0 {
            hooks_add_event(oe.name.expect("named option"));
        }
    }
}
pub unsafe fn hooks_run(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    name: &CStr,
) {
    let item = item_handle.map_or(std::ptr::null_mut(), |item| item.get());
    let target = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let mut hd = hooks_data {
        name: name,
        fs: cmd_find_state::default(),
        formats: format_create_owned(None, None, 0, FORMAT_NOJOBS),
        oo: None,
        client: (*item).client.clone(),
        expand: 0,
    };
    cmd_find_copy_state(&raw mut hd.fs, target);
    format_add(&raw mut *hd.formats, c"hook", |out| write_cstr(out, &*name));
    format_log_debug(&raw mut *hd.formats, c"hooks_run");
    hooks_insert(item_handle, &raw mut hd);
    format_free(hd.formats);
}
impl Drop for hooks_monitor {
    fn drop(&mut self) {
        unsafe {
            events_remove_sink(self.sink);
            if let Some(set) = self.set.take() {
                crate::src::monitor::monitor_destroy(set);
            }
        }
    }
}
pub unsafe fn hooks_monitor_remove(scope: &OptionsScope, name: &CStr) {
    let old = scope
        .with_entry(name, |entry| entry.monitor_data.take())
        .flatten();
    // Monitor destruction can release a Session; no model borrow remains here.
    drop(old);
}
unsafe fn hooks_monitor_hook_cb(
    name: &CStr,
    payload: &mut event_payload,
    scope: &OptionsScope,
    generation: usize,
) {
    if !matches!(event_payload_get_identity(payload, c"_hooks_monitor"),
        Some(crate::src::shared::events::EventPayloadIdentity::HookMonitor(id)) if *id == generation)
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
        &*name,
        payload,
        Some(scope.clone()),
        1,
    );
}
unsafe fn hooks_monitor_cb(change: &monitor_change, fallback: &cmd_find_state, generation: usize) {
    let wl = change.wl.clone();
    let client_owner = change.c.upgrade();
    let session_owner = change.s.upgrade();
    let s = session_owner.clone();
    let pane_owner = std::rc::Rc::<std::cell::UnsafeCell<window_pane>>::from_observer(&change.wp);
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
        crate::src::shared::events::EventPayloadIdentity::HookMonitor(generation),
    );
    fs = cmd_find_state::clear_state(0 as ::core::ffi::c_int);
    if wl.is_alive()
        && pane_owner.is_some()
        && pane_owner
            .as_ref()
            .expect("live pane")
            .window_observer()
            .ptr_eq(
                &wl.get_unchecked()
                    .window_handle()
                    .map_or_else(WindowWeak::new, WindowRef::downgrade),
            )
    {
        cmd_find_from_winlink_pane(
            &raw mut fs,
            wl.clone(),
            pane_owner.as_ref().expect("monitor pane"),
            0 as ::core::ffi::c_int,
        );
    } else if wl.is_alive() {
        cmd_find_from_winlink(&raw mut fs, wl.clone(), 0 as ::core::ffi::c_int);
    } else if let Some(pane_owner_value) = pane_owner.as_ref() {
        cmd_find_from_pane(&raw mut fs, pane_owner_value, 0 as ::core::ffi::c_int);
    } else if !s.is_none() {
        cmd_find_from_session(
            &raw mut fs,
            &s.clone().expect("live session"),
            0 as ::core::ffi::c_int,
        );
    } else {
        cmd_find_copy_state(&raw mut fs, fallback);
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
            if let Some(session_owner) = wl.get_unchecked().session.upgrade() {
                event_payload_set_session(&mut ep, c"session", session_owner.clone());
            }
        }
        event_payload_set_window(
            &mut ep,
            c"window",
            WindowRef::clone((wl.get_unchecked().window_handle().as_ref()).expect("live window")),
        );
        event_payload_set_int(&mut ep, c"window_index", wl.get_unchecked().idx);
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
    // Payload construction has finished reading the link. Dispatch may unlink it.
    event_payload_set_target(&mut ep, &fs);
    events_fire(&*change.name, ep);
    drop(session_owner);
    drop(client_owner);
}
pub unsafe fn hooks_monitor_add(
    scope: &OptionsScope,
    name: &CStr,
    type_0: monitor_type,
    id: ::core::ffi::c_int,
    format: &CStr,
    flags: ::core::ffi::c_int,
    fs: *mut cmd_find_state,
    s_owner: Option<&SessionRef>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_MONITOR: AtomicUsize = AtomicUsize::new(1);
    hooks_monitor_remove(scope, name);
    scope.with_local(|table| {
        if crate::src::options::options_get_only(table, name).is_none() {
            options_set_string(table, name, 0, |out| out.write_all(b""));
        }
    });
    let generation = NEXT_MONITOR
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("hook monitor identity exhausted");
    let mut fallback = cmd_find_state::default();
    cmd_find_copy_state(&mut fallback, &*fs);
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
    // The option keeps sole ownership; only a weak observer leaves its borrow.
    monitor_add(&set, name, type_0, id, format, flags);
}
pub(crate) unsafe fn hooks_monitor_to_cstring(o: *mut options_entry) -> Option<CString> {
    let mut bytes = options_name(&*o).to_bytes().to_vec();
    let hm = options_get_monitor_data(&mut *o)?;
    let target = match hm.type_0 {
        0 => b"::".as_slice(),
        1 => b":%".as_slice(),
        2 => b":%*:".as_slice(),
        3 => b":@".as_slice(),
        4 => b":@*:".as_slice(),
        _ => return None,
    };
    bytes.extend_from_slice(target);
    if matches!(hm.type_0, 1 | 3) {
        bytes.extend_from_slice(hm.id.to_string().as_bytes());
        bytes.push(b':');
    }
    bytes.extend_from_slice(hm.format.as_bytes());
    Some(CString::new(bytes).expect("C string parts contain no NUL"))
}
pub unsafe fn hooks_monitor_get_fire_count(mut o: *mut options_entry) -> u_int {
    let name = (*o).name.as_ptr();
    let Some(hm) = options_get_monitor_data(&mut *o) else {
        return 0 as u_int;
    };
    monitor_get_fire_count(
        &hm.set.as_ref().expect("hook monitor").downgrade(),
        CStr::from_ptr(name),
    )
}
pub unsafe fn hooks_monitor_get_fire_time(mut o: *mut options_entry) -> time_t {
    let name = (*o).name.as_ptr();
    let Some(hm) = options_get_monitor_data(&mut *o) else {
        return 0 as time_t;
    };
    monitor_get_fire_time(
        &hm.set.as_ref().expect("hook monitor").downgrade(),
        CStr::from_ptr(name),
    )
}

#[cfg(test)]
mod CStrings_tests {
    use super::*;

    #[test]
    fn registry_owns_stable_c_names_and_deduplicates_by_bytes() {
        let mut registry = HooksEvents::default();
        let name = CString::new(vec![b'@', 0xff]).unwrap();
        assert!(registry.insert(name));

        let event_address = &*registry.events[0] as *const CString;
        let name_address = registry.events[0].as_ptr();

        for i in 0..128 {
            let other = CString::new(format!("event-{i}")).unwrap();
            assert!(registry.insert(other));
        }

        // The first owner and its C string stay at stable addresses as the
        // collection grows, and arbitrary non-UTF-8 event names are preserved.
        assert_eq!(&*registry.events[0] as *const CString, event_address);
        assert_eq!(registry.events[0].as_ptr(), name_address);
        assert_eq!(
            unsafe { CStr::from_ptr(name_address) }.to_bytes(),
            [b'@', 0xff]
        );
        assert!(registry.contains(CStr::from_bytes_with_nul(b"@\xff\0").unwrap()));
        assert!(!registry.insert(CString::new(vec![b'@', 0xff]).unwrap()));
    }
}
