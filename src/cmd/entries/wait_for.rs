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
    pub(crate) waiters: Vec<wait_item>,
    pub(crate) lockers: Vec<wait_item>,
}

pub struct wait_channels {
    entries: std::collections::BTreeMap<std::ffi::CString, Box<wait_channel>>,
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
static mut wait_event_items: Vec<wait_event_item> = Vec::new();
static mut wait_channels: wait_channels = wait_channels {
    entries: std::collections::BTreeMap::new(),
};

fn wait_channels_find(head: &mut wait_channels, name: &CStr) -> *mut wait_channel {
    head.entries
        .get_mut(name)
        .map(|owner| &raw mut **owner)
        .unwrap_or(::core::ptr::null_mut::<wait_channel>())
}

fn wait_channels_insert(
    head: &mut wait_channels,
    mut owner: Box<wait_channel>,
) -> *mut wait_channel {
    let key = owner.name.clone();
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
    // elm lives inside the map; copy its name before unlinking it.
    let key = elm.name.clone();
    head.entries.remove(&key)
}

// Each channel is the first field of its boxed owner, so pointers handed to
// the translated C routines can still find the Rust-owned queues.
unsafe fn wait_channel_waiters(wc: *mut wait_channel) -> *mut Vec<wait_item> {
    &raw mut (*wc).waiters
}

unsafe fn wait_channel_lockers(wc: *mut wait_channel) -> *mut Vec<wait_item> {
    &raw mut (*wc).lockers
}

// Continue and remove every queued item in order, front first.
unsafe fn wait_items_continue_all(list: *mut Vec<wait_item>) {
    while !(*list).is_empty() {
        cmd_wait_for_continue_waiter(&(&*list)[0]);
        drop((&mut *list).remove(0));
    }
}

// Each event waiter belongs to exactly one waiting command item, so the item
// identifies its entry without relying on the entry's address.
fn wait_event_items_position(
    items: &[wait_event_item],
    item: &Weak<UnsafeCell<cmdq_item>>,
) -> Option<usize> {
    items
        .iter()
        .position(|owner| Weak::ptr_eq(&owner.item, item))
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
unsafe fn cmd_wait_for_client_name(wei: &wait_event_item) -> CString {
    wei.item.upgrade().map_or_else(CString::default, |item| {
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
    key: &Weak<UnsafeCell<cmdq_item>>,
) {
    // The sink is removed together with its entry, so the entry is present.
    let Some(index) =
        wait_event_items_position((&raw const wait_event_items).as_ref().unwrap(), key)
    else {
        return;
    };
    let (verbose, filter) = {
        let wei = &(&raw const wait_event_items).as_ref().unwrap()[index];
        (wei.verbose, wei.filter.clone())
    };
    let Some(item_owner) = key.upgrade() else {
        drop((&raw mut wait_event_items).as_mut().unwrap().remove(index));
        return;
    };
    let item = item_owner.get();
    let queue_client = cmdq_get_client((item).as_ref());
    let ep = &*payload;
    let mut ft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut flag: ::core::ffi::c_int = 0;
    if verbose != 0 {
        cmd_wait_for_event_print(&item_owner, ep);
    }
    if let Some(filter) = &filter {
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
        let expanded = format_expand_cstring(ft, filter.as_ptr().cast_mut());
        flag = format_true(expanded.as_ptr());
        drop(ft_owner);
        if flag == 0 {
            return;
        }
    }
    let owner = wait_event_items_position((&raw const wait_event_items).as_ref().unwrap(), key)
        .map(|index| (&raw mut wait_event_items).as_mut().unwrap().remove(index));
    cmdq_continue(&item_owner);
    drop(owner);
}
impl Drop for wait_event_item {
    fn drop(&mut self) {
        unsafe { events_remove_sink(self.sink) };
    }
}

fn cmd_wait_for_event_prune_expired(items: &mut Vec<wait_event_item>) {
    items.retain(|owner| owner.item.upgrade().is_some());
}
unsafe fn cmd_wait_for_event(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
    mut args: *mut args,
) -> cmd_retval {
    let item = item_handle.get();
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
    let mut owner = wait_event_item {
        item: std::rc::Rc::downgrade(item_handle),
        sink: EventSinkId::default(),
        name: name.to_owned(),
        filter: if filter.is_null() {
            None
        } else {
            Some(CStr::from_ptr(filter).to_owned())
        },
        verbose: args_has(args, 'v' as i32 as u_char),
    };

    let key = owner.item.clone();
    owner.sink = events_add_sink(
        &owner.name,
        events_callback(move |name, payload| unsafe { cmd_wait_for_event_cb(name, payload, &key) }),
    );
    (&raw mut wait_event_items).as_mut().unwrap().push(owner);
    CMD_RETURN_WAIT
}
unsafe fn cmd_wait_for_event_list(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    name: &CStr,
) -> cmd_retval {
    cmd_wait_for_event_prune_expired(&mut wait_event_items);
    for wei in (&raw const wait_event_items).as_ref().unwrap() {
        if strcmp((wei.name).as_ptr().cast_mut(), name.as_ptr()) == 0 as ::core::ffi::c_int {
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
        let wei = &(&raw const wait_event_items).as_ref().unwrap()[index];
        if !(strcmp((wei.name).as_ptr().cast_mut(), name.as_ptr()) != 0 as ::core::ffi::c_int)
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
        for list in [wait_channel_waiters(wc), wait_channel_lockers(wc)] {
            let mut index = 0;
            while index < (*list).len() {
                let name = cmd_wait_for_waiter_client_name(&(&*list)[index]);
                if strcmp(name.as_ptr(), client_name) != 0 as ::core::ffi::c_int {
                    index += 1;
                } else {
                    cmd_wait_for_continue_waiter(&(&*list)[index]);
                    drop((&mut *list).remove(index));
                    cmd_wait_for_remove_empty(wc);
                    return CMD_RETURN_NORMAL;
                }
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
    wait_items_continue_all(wait_channel_waiters(wc));
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
    (*wait_channel_waiters(wc)).push(wait_item {
        item: std::rc::Rc::downgrade(item_handle),
    });
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
        (*wait_channel_lockers(wc)).push(wait_item {
            item: std::rc::Rc::downgrade(item_handle),
        });
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
    if let Some(wi) = (*lockers).first() {
        cmd_wait_for_continue_waiter(wi);
        drop((&mut *lockers).remove(0));
    } else {
        (*wc).locked = 0 as ::core::ffi::c_int;
        cmd_wait_for_remove(wc);
    }
    CMD_RETURN_NORMAL
}
pub unsafe fn cmd_wait_for_flush() {
    while !(&raw const wait_event_items).as_ref().unwrap().is_empty() {
        let owner = (&raw mut wait_event_items).as_mut().unwrap().remove(0);
        if let Some(item) = owner.item.upgrade() {
            cmdq_continue(&item);
        }
        drop(owner);
    }
    let channels = (&raw mut wait_channels)
        .as_mut()
        .unwrap()
        .entries
        .values_mut()
        .map(|owner| &raw mut **owner)
        .collect::<Vec<_>>();
    for wc in channels {
        wait_items_continue_all(wait_channel_waiters(wc));
        (*wc).woken = 1 as ::core::ffi::c_int;
        wait_items_continue_all(wait_channel_lockers(wc));
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

        Box::new(wait_channel {
            name,
            locked: 0,
            woken: 0,
            waiters: Vec::new(),
            lockers: Vec::new(),
        })
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
    fn continue_all_drains_channel_queue_in_order() {
        let name = CString::new("queue").unwrap();
        let mut owner = test_channel(&name);
        let channel = &raw mut *owner;
        owner
            .waiters
            .extend((0..128).map(|_| wait_item { item: Weak::new() }));
        owner
            .lockers
            .extend((0..128).map(|_| wait_item { item: Weak::new() }));

        unsafe {
            wait_items_continue_all(wait_channel_waiters(channel));
            assert!((*wait_channel_waiters(channel)).is_empty());
            assert_eq!((*wait_channel_lockers(channel)).len(), 128);
        }
    }
}
