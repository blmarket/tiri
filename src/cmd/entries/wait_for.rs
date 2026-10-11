use crate::src::arguments::{args_get, args_has, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::cmd::queue::{cmdq_continue, cmdq_error, cmdq_get_client, cmdq_print};
use crate::src::events::{events_add_sink, events_remove_sink};
use crate::src::events_payload::{
    event_payload_add_formats, event_payload_item_name, event_payload_item_print_owned,
    event_payload_items,
};
use crate::src::ffi::libc::strcmp;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::{format_create_with_client, format_expand_cstring, format_true};
use crate::src::hooks::hooks_valid_event_name;
use crate::src::log::{log_cstr, log_debug, log_pointer};
use crate::src::server_client::Client as _;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item, wait_item};
use crate::src::shared::events::{event_payload, events_callback, EventSinkId};
use crate::src::shared::format::format_tree;
use crate::src::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::Weak;

#[repr(C)]
pub struct wait_channel {
    pub name: std::ffi::CString,
    pub locked: ::core::ffi::c_int,
    pub woken: ::core::ffi::c_int,
    pub(crate) waiters: Vec<Box<wait_item>>,
    pub(crate) lockers: Vec<Box<wait_item>>,
}

pub struct wait_channels {
    entries: std::collections::BTreeMap<Vec<u8>, Box<wait_channel>>,
}
// The map owns each channel and its queues through this stable C-shaped
// prefix. The name stays alive until the channel is removed.

#[repr(C)]
pub struct wait_event_item {
    pub item: Weak<UnsafeCell<cmdq_item>>,
    pub sink: EventSinkId,
    pub name: std::ffi::CString,
    pub filter: Option<std::ffi::CString>,
    pub verbose: ::core::ffi::c_int,
}

pub static cmd_wait_for_entry: cmd_entry = {
    cmd_entry {
        name: c"wait-for",
        alias: Some(c"wait"),
        args: args_parse {
            template: c"EF:LSUlvw:",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-ELSUlv] [-F format] [-w waiter] name",
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
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_wait_for_exec),
    }
};
static mut wait_event_items: Vec<Box<wait_event_item>> = Vec::new();
static mut wait_channels: wait_channels = wait_channels {
    entries: std::collections::BTreeMap::new(),
};

fn wait_channel_key(name: &CStr) -> Vec<u8> {
    name.to_bytes().to_vec()
}

fn wait_channels_find(head: &mut wait_channels, name: &CStr) -> *mut wait_channel {
    head.entries
        .get_mut(&wait_channel_key(name))
        .map(|owner| &raw mut **owner)
        .unwrap_or(::core::ptr::null_mut::<wait_channel>())
}

fn wait_channels_insert(
    head: &mut wait_channels,
    mut owner: Box<wait_channel>,
) -> *mut wait_channel {
    let key = wait_channel_key(owner.name.as_c_str());
    match head.entries.entry(key) {
        std::collections::btree_map::Entry::Occupied(mut entry) => &raw mut **entry.get_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => {
            let channel = &raw mut *owner;
            entry.insert(owner);
            channel
        }
    }
}

fn wait_channels_remove(head: &mut wait_channels, elm: &wait_channel) -> Option<Box<wait_channel>> {
    head.entries.remove(&wait_channel_key(elm.name.as_c_str()))
}

// Each channel is the first field of its boxed owner, so pointers handed to
// the translated C routines can still find the Rust-owned queues.
unsafe fn wait_channel_waiters(wc: *mut wait_channel) -> *mut Vec<Box<wait_item>> {
    &raw mut (*wc).waiters
}

unsafe fn wait_channel_lockers(wc: *mut wait_channel) -> *mut Vec<Box<wait_item>> {
    &raw mut (*wc).lockers
}

unsafe fn wait_item_ptr(list: *mut Vec<Box<wait_item>>, index: usize) -> *mut wait_item {
    (&mut *list)
        .get_mut(index)
        .map(|item| &raw mut **item)
        .unwrap_or(::core::ptr::null_mut())
}

unsafe fn wait_item_next(list: *mut Vec<Box<wait_item>>, item: *mut wait_item) -> *mut wait_item {
    let index = (&*list)
        .iter()
        .position(|entry| ::core::ptr::eq(entry.as_ref(), item));
    index.map_or(::core::ptr::null_mut(), |index| {
        wait_item_ptr(list, index + 1)
    })
}

unsafe fn wait_item_remove(
    list: *mut Vec<Box<wait_item>>,
    item: *mut wait_item,
) -> Option<Box<wait_item>> {
    let index = (&*list)
        .iter()
        .position(|entry| ::core::ptr::eq(entry.as_ref(), item))?;
    Some((&mut *list).remove(index))
}

fn wait_event_item_ptr(owner: &mut wait_event_item) -> *mut wait_event_item {
    &raw mut *owner
}

unsafe fn wait_event_items_remove(
    items: *mut Vec<Box<wait_event_item>>,
    item: *mut wait_event_item,
) -> Option<Box<wait_event_item>> {
    let index = (&*items)
        .iter()
        .position(|owner| ::core::ptr::eq(&**owner, item))?;
    Some((&mut *items).remove(index))
}

unsafe fn wait_event_item_next(
    items: *mut Vec<Box<wait_event_item>>,
    item: *mut wait_event_item,
) -> *mut wait_event_item {
    let index = (&*items)
        .iter()
        .position(|owner| ::core::ptr::eq(&**owner, item));
    index.map_or(::core::ptr::null_mut(), |index| {
        (&mut *items)
            .get_mut(index + 1)
            .map(|owner| wait_event_item_ptr(owner))
            .unwrap_or(::core::ptr::null_mut())
    })
}

unsafe fn wait_event_item_at(
    items: *mut Vec<Box<wait_event_item>>,
    index: usize,
) -> *mut wait_event_item {
    (&mut *items)
        .get_mut(index)
        .map(|owner| wait_event_item_ptr(owner))
        .unwrap_or(::core::ptr::null_mut())
}

unsafe fn cmd_wait_for_add(name: &CStr) -> *mut wait_channel {
    let mut name: *const ::core::ffi::c_char = name.as_ptr();
    let name = CStr::from_ptr(name).to_owned();
    let mut owner = Box::new(wait_channel {
        name,
        locked: 0,
        woken: 0,
        waiters: Vec::new(),
        lockers: Vec::new(),
    });
    let wc = wait_channels_insert(&mut wait_channels, owner);
    log_debug(format_args!("add wait channel {}", log_cstr(&(*wc).name)));
    wc
}
unsafe fn cmd_wait_for_remove(mut wc: *mut wait_channel) {
    if (*wc).locked != 0 {
        return;
    }
    if !(*wait_channel_waiters(wc)).is_empty() || (*wc).woken == 0 {
        return;
    }
    log_debug(format_args!(
        "remove wait channel {}",
        log_cstr(&(*wc).name)
    ));
    drop(wait_channels_remove(&mut wait_channels, &*wc));
}
unsafe fn cmd_wait_for_remove_empty(mut wc: *mut wait_channel) {
    if (*wc).locked != 0 || (*wc).woken != 0 {
        return;
    }
    if !(*wait_channel_waiters(wc)).is_empty() || !(*wait_channel_lockers(wc)).is_empty() {
        return;
    }
    log_debug(format_args!(
        "remove empty wait channel {}",
        log_cstr(&(*wc).name)
    ));
    drop(wait_channels_remove(&mut wait_channels, &*wc));
}

unsafe fn cmd_wait_for_item_client_name(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> CString {
    let item = item_handle.get();
    let owner = cmdq_get_client((item).as_ref());
    owner
        .as_ref()
        .and_then(|owner| owner.name())
        .unwrap_or_default()
}
unsafe fn cmd_wait_for_waiter_client_name(waiter: &wait_item) -> CString {
    waiter.item.upgrade().map_or_else(CString::default, |item| {
        cmd_wait_for_item_client_name(&item)
    })
}

unsafe fn cmd_wait_for_continue_waiter(waiter: &wait_item) {
    if let Some(item) = waiter.item.upgrade() {
        cmdq_continue(&item);
    }
}

unsafe fn cmd_wait_for_prune_expired(wc: *mut wait_channel) {
    (&mut *wait_channel_waiters(wc)).retain(|waiter| waiter.item.upgrade().is_some());
    (&mut *wait_channel_lockers(wc)).retain(|waiter| waiter.item.upgrade().is_some());
}
unsafe fn cmd_wait_for_client_name(wei: *mut wait_event_item) -> CString {
    (*wei).item.upgrade().map_or_else(CString::default, |item| {
        cmd_wait_for_item_client_name(&item)
    })
}
unsafe fn cmd_wait_for_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut name: *const ::core::ffi::c_char =
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut wc: *mut wait_channel = ::core::ptr::null_mut::<wait_channel>();
    if args_has(args, 'E' as i32 as u_char) != 0 {
        return cmd_wait_for_event(item_handle, CStr::from_ptr(name), args);
    }
    wc = wait_channels_find(&mut wait_channels, CStr::from_ptr(name));
    if args_has(args, 'l' as i32 as u_char) != 0 {
        return cmd_wait_for_list(item_handle, wc);
    }
    if args_has(args, 'w' as i32 as u_char) != 0 {
        return cmd_wait_for_wake(args, wc);
    }
    if args_has(args, 'S' as i32 as u_char) != 0 {
        return cmd_wait_for_signal(CStr::from_ptr(name), wc);
    }
    if args_has(args, 'L' as i32 as u_char) != 0 {
        return cmd_wait_for_lock(item_handle, CStr::from_ptr(name), wc);
    }
    if args_has(args, 'U' as i32 as u_char) != 0 {
        return cmd_wait_for_unlock(item_handle, CStr::from_ptr(name), wc);
    }
    cmd_wait_for_wait(item_handle, CStr::from_ptr(name), wc)
}
unsafe fn cmd_wait_for_event_print(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    ep: &event_payload,
) {
    let mut key: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    for epi in event_payload_items(ep) {
        key = event_payload_item_name(epi).as_ptr();
        if *key as ::core::ffi::c_int != '_' as i32 {
            let value = event_payload_item_print_owned(epi);
            cmdq_print(item_handle, |out| {
                write_cstr(out, nullable_cstr(key))?;
                out.write_all(b"=")?;
                write_cstr(
                    out,
                    nullable_cstr(value.as_ptr().cast::<::core::ffi::c_char>()),
                )
            });
        }
    }
}
unsafe fn cmd_wait_for_event_cb(
    _name: &CStr,
    payload: &mut event_payload,
    wei: *mut wait_event_item,
) {
    let Some(item_owner) = (*wei).item.upgrade() else {
        drop(wait_event_items_remove(&raw mut wait_event_items, wei));
        return;
    };
    let item = item_owner.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let ep = &*payload;
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut flag: ::core::ffi::c_int = 0;
    if (*wei).verbose != 0 {
        cmd_wait_for_event_print(&item_owner, ep);
    }
    if !(*wei).filter.is_none() {
        let mut ft_owner = format_create_with_client(
            queue_client.as_ref(),
            Some(&item_owner),
            FORMAT_NONE,
            FORMAT_NOJOBS,
        );
        ft = &raw mut *ft_owner;
        event_payload_add_formats(
            ep,
            ft,
            nullable_cstr(::core::ptr::null::<::core::ffi::c_char>()),
        );
        let expanded = format_expand_cstring(
            ft,
            ((*wei).filter)
                .as_ref()
                .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
        );
        flag = format_true(expanded.as_ptr());
        drop(ft_owner);
        if flag == 0 {
            return;
        }
    }
    let owner = wait_event_items_remove(&raw mut wait_event_items, wei);
    cmdq_continue(&item_owner);
    drop(owner);
}
impl Drop for wait_event_item {
    fn drop(&mut self) {
        unsafe { events_remove_sink(self.sink) };
    }
}

fn cmd_wait_for_event_prune_expired(items: &mut Vec<Box<wait_event_item>>) {
    items.retain(|owner| owner.item.upgrade().is_some());
}
unsafe fn cmd_wait_for_event(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
    mut args: *mut args,
) -> cmd_retval {
    let item = item_handle.get();
    let mut wei: *mut wait_event_item = ::core::ptr::null_mut::<wait_event_item>();
    let mut filter: *const ::core::ffi::c_char =
        args_get(&*(args), 'F' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if hooks_valid_event_name(name) == 0 {
        cmdq_error(item_handle, |out| {
            out.write_all(b"invalid event: ")?;
            write_cstr(out, nullable_cstr(name.as_ptr()))
        });
        return CMD_RETURN_ERROR;
    }
    if args_has(args, 'l' as i32 as u_char) != 0 {
        return cmd_wait_for_event_list(item_handle, name);
    }
    if args_has(args, 'w' as i32 as u_char) != 0 {
        return cmd_wait_for_event_wake(item_handle, name, args);
    }
    if cmdq_get_client((item).as_ref()).is_none() {
        cmdq_error(item_handle, |out| out.write_all(b"not able to wait"));
        return CMD_RETURN_ERROR;
    }
    let mut owner = Box::new(wait_event_item {
        item: std::rc::Rc::downgrade(item_handle),
        sink: EventSinkId::default(),
        name: name.to_owned(),
        filter: if filter.is_null() {
            None
        } else {
            Some(CStr::from_ptr(filter).to_owned())
        },
        verbose: args_has(args, 'v' as i32 as u_char),
    });

    wei = &raw mut *owner;
    (*wei).sink = events_add_sink(
        &(*wei).name,
        events_callback(move |name, payload| unsafe { cmd_wait_for_event_cb(name, payload, wei) }),
    );
    (&raw mut wait_event_items).as_mut().unwrap().push(owner);
    CMD_RETURN_WAIT
}
unsafe fn cmd_wait_for_event_list(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
) -> cmd_retval {
    cmd_wait_for_event_prune_expired(&mut wait_event_items);
    for owner in (&raw const wait_event_items).as_ref().unwrap() {
        let wei = std::ptr::from_ref(owner.as_ref()).cast_mut();
        if strcmp(((*wei).name).as_ptr().cast_mut(), name.as_ptr()) == 0 as ::core::ffi::c_int {
            cmdq_print(item_handle, |out| {
                write_cstr(out, &*(cmd_wait_for_client_name(wei)))
            });
        }
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_wait_for_event_wake(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
    mut args: *mut args,
) -> cmd_retval {
    cmd_wait_for_event_prune_expired(&mut wait_event_items);
    let mut client_name: *const ::core::ffi::c_char =
        args_get(&*(args), 'w' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut index = 0;
    while index < (&raw const wait_event_items).as_ref().unwrap().len() {
        let wei = wait_event_item_at(&raw mut wait_event_items, index);
        if !(strcmp(((*wei).name).as_ptr().cast_mut(), name.as_ptr()) != 0 as ::core::ffi::c_int)
            && !(strcmp(cmd_wait_for_client_name(wei).as_ptr(), client_name)
                != 0 as ::core::ffi::c_int)
        {
            let owner = (&raw mut wait_event_items).as_mut().unwrap().remove(index);
            if let Some(item) = owner.item.upgrade() {
                cmdq_continue(&item);
            }
            drop(owner);
            return CMD_RETURN_NORMAL;
        }
        index += 1;
    }
    cmdq_error(item_handle, |out| {
        out.write_all(b"waiter ")?;
        write_cstr(out, nullable_cstr(client_name))?;
        out.write_all(b" not found")
    });
    CMD_RETURN_ERROR
}
unsafe fn cmd_wait_for_list(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut wc: *mut wait_channel,
) -> cmd_retval {
    if wc.is_null() {
        return CMD_RETURN_NORMAL;
    }
    cmd_wait_for_prune_expired(wc);
    for wi in &*wait_channel_waiters(wc) {
        cmdq_print(item_handle, |out| {
            write_cstr(out, &*(cmd_wait_for_waiter_client_name(wi)))
        });
    }
    for wi in &*wait_channel_lockers(wc) {
        cmdq_print(item_handle, |out| {
            write_cstr(out, &*(cmd_wait_for_waiter_client_name(wi)))
        });
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_wait_for_wake(mut args: *mut args, mut wc: *mut wait_channel) -> cmd_retval {
    let mut client_name: *const ::core::ffi::c_char =
        args_get(&*(args), 'w' as i32 as u_char).map_or(std::ptr::null(), |value| value.as_ptr());
    if !wc.is_null() {
        cmd_wait_for_prune_expired(wc);
        let waiters = wait_channel_waiters(wc);
        let mut wi = wait_item_ptr(waiters, 0);
        while !wi.is_null() {
            let wi1 = wait_item_next(waiters, wi);
            let name = cmd_wait_for_waiter_client_name(&*wi);
            if strcmp(name.as_ptr(), client_name) != 0 as ::core::ffi::c_int {
                wi = wi1;
            } else {
                cmd_wait_for_continue_waiter(&*wi);
                drop(wait_item_remove(waiters, wi));
                cmd_wait_for_remove_empty(wc);
                return CMD_RETURN_NORMAL;
            }
        }
        let lockers = wait_channel_lockers(wc);
        let mut wi = wait_item_ptr(lockers, 0);
        while !wi.is_null() {
            let wi1 = wait_item_next(lockers, wi);
            let name = cmd_wait_for_waiter_client_name(&*wi);
            if strcmp(name.as_ptr(), client_name) != 0 as ::core::ffi::c_int {
                wi = wi1;
            } else {
                cmd_wait_for_continue_waiter(&*wi);
                drop(wait_item_remove(lockers, wi));
                cmd_wait_for_remove_empty(wc);
                return CMD_RETURN_NORMAL;
            }
        }
    }
    CMD_RETURN_NORMAL
}
unsafe fn cmd_wait_for_signal(name: &CStr, mut wc: *mut wait_channel) -> cmd_retval {
    if wc.is_null() {
        wc = cmd_wait_for_add(name);
    }
    cmd_wait_for_prune_expired(wc);
    if (*wait_channel_waiters(wc)).is_empty() && (*wc).woken == 0 {
        log_debug(format_args!(
            "signal wait channel {}, no waiters",
            log_cstr(&(*wc).name)
        ));
        (*wc).woken = 1 as ::core::ffi::c_int;
        return CMD_RETURN_NORMAL;
    }
    log_debug(format_args!(
        "signal wait channel {}, with waiters",
        log_cstr(&(*wc).name)
    ));
    let waiters = wait_channel_waiters(wc);
    let mut wi = wait_item_ptr(waiters, 0);
    while !wi.is_null() {
        let wi1 = wait_item_next(waiters, wi);
        cmd_wait_for_continue_waiter(&*wi);
        drop(wait_item_remove(waiters, wi));
        wi = wi1;
    }
    cmd_wait_for_remove(wc);
    CMD_RETURN_NORMAL
}
unsafe fn cmd_wait_for_wait(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
    mut wc: *mut wait_channel,
) -> cmd_retval {
    let item = item_handle.get();
    let c_owner = cmdq_get_client((item).as_ref());
    let mut c: Option<ClientRef> = c_owner.clone();
    if c.is_none() {
        cmdq_error(item_handle, |out| out.write_all(b"not able to wait"));
        return CMD_RETURN_ERROR;
    }
    if wc.is_null() {
        wc = cmd_wait_for_add(name);
    }
    if (*wc).woken != 0 {
        log_debug(format_args!(
            "wait channel {} already woken ({})",
            log_cstr(&(*wc).name),
            log_pointer(std::rc::Rc::as_ptr(c.as_ref().expect("waiting client")))
        ));
        cmd_wait_for_remove(wc);
        return CMD_RETURN_NORMAL;
    }
    log_debug(format_args!(
        "wait channel {} not woken ({})",
        log_cstr(&(*wc).name),
        log_pointer(std::rc::Rc::as_ptr(c.as_ref().expect("waiting client")))
    ));
    (*wait_channel_waiters(wc)).push(Box::new(wait_item {
        item: std::rc::Rc::downgrade(item_handle),
    }));
    CMD_RETURN_WAIT
}
unsafe fn cmd_wait_for_lock(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
    mut wc: *mut wait_channel,
) -> cmd_retval {
    let item = item_handle.get();
    if cmdq_get_client((item).as_ref()).is_none() {
        cmdq_error(item_handle, |out| out.write_all(b"not able to lock"));
        return CMD_RETURN_ERROR;
    }
    if wc.is_null() {
        wc = cmd_wait_for_add(name);
    }
    if (*wc).locked != 0 {
        (*wait_channel_lockers(wc)).push(Box::new(wait_item {
            item: std::rc::Rc::downgrade(item_handle),
        }));
        return CMD_RETURN_WAIT;
    }
    (*wc).locked = 1 as ::core::ffi::c_int;
    CMD_RETURN_NORMAL
}
unsafe fn cmd_wait_for_unlock(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
    mut wc: *mut wait_channel,
) -> cmd_retval {
    if wc.is_null() || (*wc).locked == 0 {
        cmdq_error(item_handle, |out| {
            out.write_all(b"channel ")?;
            write_cstr(out, nullable_cstr(name.as_ptr()))?;
            out.write_all(b" not locked")
        });
        return CMD_RETURN_ERROR;
    }
    let lockers = wait_channel_lockers(wc);
    cmd_wait_for_prune_expired(wc);
    let wi = wait_item_ptr(lockers, 0);
    if !wi.is_null() {
        cmd_wait_for_continue_waiter(&*wi);
        drop(wait_item_remove(lockers, wi));
    } else {
        (*wc).locked = 0 as ::core::ffi::c_int;
        cmd_wait_for_remove(wc);
    }
    CMD_RETURN_NORMAL
}
pub unsafe fn cmd_wait_for_flush() {
    let mut wei = (&raw mut wait_event_items)
        .as_mut()
        .unwrap()
        .first_mut()
        .map(|owner| wait_event_item_ptr(owner))
        .unwrap_or(::core::ptr::null_mut());
    while !wei.is_null() {
        let wei1 = wait_event_item_next(&raw mut wait_event_items, wei);
        let Some(owner) = wait_event_items_remove(&raw mut wait_event_items, wei) else {
            break;
        };
        if let Some(item) = owner.item.upgrade() {
            cmdq_continue(&item);
        }
        drop(owner);
        wei = wei1;
    }
    let channels = (&raw mut wait_channels)
        .as_mut()
        .unwrap()
        .entries
        .values_mut()
        .map(|owner| &raw mut **owner)
        .collect::<Vec<_>>();
    for wc in channels {
        let waiters = wait_channel_waiters(wc);
        let mut wi = wait_item_ptr(waiters, 0);
        while !wi.is_null() {
            let wi1 = wait_item_next(waiters, wi);
            cmd_wait_for_continue_waiter(&*wi);
            drop(wait_item_remove(waiters, wi));
            wi = wi1;
        }
        (*wc).woken = 1 as ::core::ffi::c_int;
        let lockers = wait_channel_lockers(wc);
        let mut wi = wait_item_ptr(lockers, 0);
        while !wi.is_null() {
            let wi1 = wait_item_next(lockers, wi);
            cmd_wait_for_continue_waiter(&*wi);
            drop(wait_item_remove(lockers, wi));
            wi = wi1;
        }
        (*wc).locked = 0 as ::core::ffi::c_int;
        cmd_wait_for_remove(wc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn test_channel(name: &CStr) -> Box<wait_channel> {
        let name = name.to_owned();
        let owner = Box::new(wait_channel {
            name: name,
            locked: 0,
            woken: 0,
            waiters: Vec::new(),
            lockers: Vec::new(),
        });
        owner
    }

    #[test]
    fn wait_channels_match_c_string_tree_semantics() {
        let name_a = CString::new("a").unwrap();
        let name_a0 = CString::new("a0").unwrap();
        let name_a_high = CString::new(vec![b'a', 0xff]).unwrap();
        let name_z = CString::new("z").unwrap();
        let lookup_a = CString::new("a").unwrap();

        let mut channels = wait_channels {
            entries: std::collections::BTreeMap::new(),
        };
        unsafe {
            let a = wait_channels_insert(&mut channels, test_channel(&name_a));
            let a0 = wait_channels_insert(&mut channels, test_channel(&name_a0));
            let a_high = wait_channels_insert(&mut channels, test_channel(&name_a_high));
            let z = wait_channels_insert(&mut channels, test_channel(&name_z));
            assert!(!a.is_null() && !a0.is_null() && !a_high.is_null() && !z.is_null());
            assert_eq!(
                wait_channels_insert(&mut channels, test_channel(&name_a)),
                a
            );
            assert_eq!(wait_channels_find(&mut channels, lookup_a.as_c_str()), a);

            let ordered = channels
                .entries
                .values_mut()
                .map(|owner| &raw mut **owner)
                .collect::<Vec<_>>();
            assert_eq!(ordered, vec![a, a0, a_high, z]);

            let mut removed = wait_channels_remove(&mut channels, &*a_high).unwrap();
            assert_eq!(&raw mut *removed, a_high);
            assert!(wait_channels_find(&mut channels, name_a_high.as_c_str()).is_null());
        }
    }

    #[test]
    fn channel_queues_keep_order_and_stable_element_addresses() {
        let name = CString::new("queue").unwrap();
        let mut owner = test_channel(&name);
        let channel = &raw mut *owner;
        let original = (0..128)
            .map(|_| Box::new(wait_item { item: Weak::new() }))
            .collect::<Vec<_>>();
        let addresses = original
            .iter()
            .map(|item| &**item as *const wait_item as *mut wait_item)
            .collect::<Vec<_>>();
        owner.waiters.extend(original);
        let lockers = (0..128)
            .map(|_| Box::new(wait_item { item: Weak::new() }))
            .collect::<Vec<_>>();
        let first_locker = &*lockers[0] as *const wait_item as *mut wait_item;
        owner.lockers.extend(lockers);

        unsafe {
            assert_eq!(
                wait_item_ptr(wait_channel_waiters(channel), 0),
                addresses[0]
            );
            assert_eq!(
                wait_item_ptr(wait_channel_waiters(channel), 127),
                addresses[127]
            );
            assert_eq!(
                wait_item_ptr(wait_channel_lockers(channel), 0),
                first_locker,
            );

            drop(wait_item_remove(
                wait_channel_waiters(channel),
                addresses[37],
            ));
            assert!(wait_item_remove(wait_channel_waiters(channel), addresses[37]).is_none());
            let remaining = (0..127)
                .map(|index| wait_item_ptr(wait_channel_waiters(channel), index))
                .collect::<Vec<_>>();
            assert_eq!(remaining[36], addresses[36]);
            assert_eq!(remaining[37], addresses[38]);
            assert_eq!(remaining[126], addresses[127]);
        }
    }

    #[test]
    fn event_waiter_addresses_survive_owner_vector_growth_and_removal() {
        let name = CString::new("event").unwrap();
        let mut items = Vec::<Box<wait_event_item>>::new();
        let mut addresses = Vec::new();
        for _ in 0..128 {
            let mut owner = Box::new(wait_event_item {
                item: Weak::new(),
                sink: EventSinkId::default(),
                name: name.clone(),
                filter: None,
                verbose: 0,
            });

            addresses.push(wait_event_item_ptr(&mut owner));
            items.push(owner);
        }

        unsafe {
            let first = wait_event_item_at(&raw mut items, 0);
            let last = wait_event_item_at(&raw mut items, 127);
            assert_eq!(first, addresses[0]);
            assert_eq!(last, addresses[127]);
            assert_eq!(
                wait_event_item_next(&raw mut items, addresses[36]),
                addresses[37]
            );
            drop(wait_event_items_remove(&raw mut items, addresses[36]));
            assert!(wait_event_items_remove(&raw mut items, addresses[36]).is_none());
            assert_eq!(wait_event_item_at(&raw mut items, 36), addresses[37]);
            assert_eq!(wait_event_item_at(&raw mut items, 126), addresses[127]);
        }
    }
}
