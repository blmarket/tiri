use crate::actions::queue::{client as context_client, report_error, wait as context_wait};
use crate::actions::{Completion, ContextSource, ContextWeak, TaskOutcome};
use crate::events::{events_add_sink, events_remove_sink};
use crate::events_payload::{
    event_payload_add_formats, event_payload_item_name, event_payload_item_print_owned,
    event_payload_items,
};
use crate::format::bytes::write_cstr;
use crate::format::{format_create, format_expand_cstring, format_true};
use crate::log::{log_cstr, log_debug, log_pointer};
use crate::reactor::{evbuffer_add_formatted, evbuffer_new};
use crate::server_client::Client as _;
use crate::shared::client::ClientRef;
use crate::shared::events::{EventSinkId, event_payload, events_callback};
use crate::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};

#[repr(C)]
pub struct wait_channel {
    pub locked: ::core::ffi::c_int,
    pub woken: ::core::ffi::c_int,
    pub waiters: Vec<Completion>,
    pub lockers: Vec<Completion>,
}

struct wait_channels {
    entries: BTreeMap<CString, wait_channel>,
}

#[repr(C)]
struct wait_event_item {
    item: ContextWeak,
    waiter: Completion,
    sink: wait_event_sink,
    name: CString,
    filter: Option<CString>,
    verbose: ::core::ffi::c_int,
}

thread_local! {
    static WAIT_EVENT_ITEMS: RefCell<Vec<wait_event_item>> = const { RefCell::new(Vec::new()) };
    static WAIT_CHANNELS: RefCell<wait_channels> = const {
        RefCell::new(wait_channels {
            entries: BTreeMap::new(),
        })
    };
}

fn wait_items_continue_all(list: Vec<Completion>) {
    for waiter in list {
        waiter.resume();
    }
}

fn wait_event_items_position(items: &[wait_event_item], item: &ContextWeak) -> Option<usize> {
    items.iter().position(|owner| owner.item.ptr_eq(item))
}

fn wait_channel_add<'a>(channels: &'a mut wait_channels, name: &CStr) -> &'a mut wait_channel {
    channels.entries.entry(name.to_owned()).or_insert_with(|| {
        log_debug(format_args!("add wait channel {}", log_cstr(name)));
        wait_channel {
            locked: 0,
            woken: 0,
            waiters: Vec::new(),
            lockers: Vec::new(),
        }
    })
}

fn wait_channel_remove(channels: &mut wait_channels, name: &CStr) {
    let Some(wc) = channels.entries.get(name) else {
        return;
    };
    if wc.locked != 0 {
        return;
    }
    if !wc.waiters.is_empty() || wc.woken == 0 {
        return;
    }
    log_debug(format_args!("remove wait channel {}", log_cstr(name)));
    channels.entries.remove(name);
}

fn wait_channel_remove_empty(channels: &mut wait_channels, name: &CStr) {
    let Some(wc) = channels.entries.get(name) else {
        return;
    };
    if wc.locked != 0 || wc.woken != 0 {
        return;
    }
    if !wc.waiters.is_empty() || !wc.lockers.is_empty() {
        return;
    }
    log_debug(format_args!("remove empty wait channel {}", log_cstr(name)));
    channels.entries.remove(name);
}

fn waiter_client_name(waiter: &Completion) -> CString {
    waiter
        .client()
        .and_then(|owner| owner.name())
        .unwrap_or_default()
}

fn wait_channel_prune_expired(wc: &mut wait_channel) {
    wc.waiters.retain(|waiter| !waiter.is_cancelled());
    wc.lockers.retain(|waiter| !waiter.is_cancelled());
}

fn wait_event_print(client: Option<&ClientRef>, ep: &event_payload) {
    for epi in event_payload_items(ep) {
        let key = event_payload_item_name(epi);
        if key.to_bytes().first() != Some(&b'_') {
            let value = event_payload_item_print_owned(epi);
            let mut evb = evbuffer_new();
            evbuffer_add_formatted(&mut evb, |out| {
                write_cstr(out, key)?;
                out.write_all(b"=")?;
                write_cstr(
                    out,
                    CStr::from_bytes_until_nul(&value).expect("payload value is NUL terminated"),
                )
            });
            ClientRef::print_to(client, true, &mut evb);
        }
    }
}

fn wait_event_cb(_name: &CStr, payload: &mut event_payload, key: &ContextWeak) {
    let found = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        let index = wait_event_items_position(items, key)?;
        let wei = &items[index];
        if wei.waiter.is_cancelled() {
            drop(items.remove(index));
            return None;
        }
        Some((wei.verbose, wei.filter.clone(), wei.waiter.client()))
    });
    let Some((verbose, filter, queue_client)) = found else {
        return;
    };
    let ep = &*payload;
    if verbose != 0 {
        wait_event_print(queue_client.as_ref(), ep);
        if WAIT_EVENT_ITEMS.with_borrow(|items| wait_event_items_position(items, key).is_none()) {
            return;
        }
    }
    if let Some(filter) = &filter {
        let Some(item_owner) = key.upgrade() else {
            return;
        };
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            Some(&item_owner),
            FORMAT_NONE,
            FORMAT_NOJOBS,
        );
        drop(item_owner);
        let ft = &mut *ft_owner;
        event_payload_add_formats(ep, ft, None);
        let expanded = format_expand_cstring(ft, filter);
        let flag = format_true(&expanded);
        drop(ft_owner);
        if !flag {
            return;
        }
    }
    drop(queue_client);
    let woken = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        wait_event_items_position(items, key).map(|index| items.remove(index))
    });
    if let Some(wei) = woken {
        wei.resume();
    }
}

impl wait_event_item {
    fn resume(self) {
        self.waiter.resume();
    }
}

struct wait_event_sink(EventSinkId);
impl Drop for wait_event_sink {
    fn drop(&mut self) {
        events_remove_sink(self.0);
    }
}

fn wait_event_prune_expired(items: &mut Vec<wait_event_item>) {
    items.retain(|owner| !owner.waiter.is_cancelled());
}

pub fn wait_event_wait(
    item: &dyn ContextSource,
    name: &CStr,
    filter: Option<&CStr>,
    verbose: ::core::ffi::c_int,
) -> TaskOutcome {
    if context_client(Some(&*item.borrow())).is_none() {
        report_error(item, |out| out.write_all(b"not able to wait"));
        return TaskOutcome::Error;
    }
    let key = item.downgrade();
    let sink = wait_event_sink(events_add_sink(name, {
        let key = key.clone();
        events_callback(move |name, payload| wait_event_cb(name, payload, &key))
    }));
    let (waiter, retval) = context_wait(item);
    WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        items.push(wait_event_item {
            item: key,
            waiter,
            sink,
            name: name.to_owned(),
            filter: filter.map(CStr::to_owned),
            verbose,
        })
    });
    retval
}

pub fn wait_event_list(name: &CStr) -> Vec<CString> {
    WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        wait_event_prune_expired(items);
        items
            .iter()
            .filter(|wei| wei.name.as_c_str() == name)
            .map(|wei| waiter_client_name(&wei.waiter))
            .collect()
    })
}

pub fn wait_event_wake(item: &dyn ContextSource, name: &CStr, client_name: &CStr) -> TaskOutcome {
    let woken = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        wait_event_prune_expired(items);
        let index = items.iter().position(|wei| {
            wei.name.as_c_str() == name
                && waiter_client_name(&wei.waiter).as_c_str() == client_name
        })?;
        Some(items.remove(index))
    });
    if let Some(wei) = woken {
        wei.resume();
        return TaskOutcome::Normal;
    }
    report_error(item, |out| {
        out.write_all(b"waiter ")?;
        write_cstr(out, client_name)?;
        out.write_all(b" not found")
    });
    TaskOutcome::Error
}

pub fn wait_channel_list(name: &CStr) -> Vec<CString> {
    WAIT_CHANNELS.with_borrow_mut(|channels| {
        let Some(wc) = channels.entries.get_mut(name) else {
            return Vec::new();
        };
        wait_channel_prune_expired(wc);
        wc.waiters
            .iter()
            .chain(&wc.lockers)
            .map(waiter_client_name)
            .collect()
    })
}

pub fn wait_channel_wake(name: &CStr, client_name: &CStr) {
    let woken = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = channels.entries.get_mut(name)?;
        wait_channel_prune_expired(wc);
        let waiter = [&mut wc.waiters, &mut wc.lockers]
            .into_iter()
            .find_map(|list| {
                let index = list
                    .iter()
                    .position(|wi| waiter_client_name(wi).as_c_str() == client_name)?;
                Some(list.remove(index))
            })?;
        wait_channel_remove_empty(channels, name);
        Some(waiter)
    });
    if let Some(waiter) = woken {
        waiter.resume();
    }
}

pub fn wait_channel_signal(name: &CStr) -> TaskOutcome {
    let waiters = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = wait_channel_add(channels, name);
        wait_channel_prune_expired(wc);
        if wc.waiters.is_empty() && wc.woken == 0 {
            log_debug(format_args!(
                "signal wait channel {}, no waiters",
                log_cstr(name)
            ));
            wc.woken = 1 as ::core::ffi::c_int;
            return Vec::new();
        }
        log_debug(format_args!(
            "signal wait channel {}, with waiters",
            log_cstr(name)
        ));
        let waiters = std::mem::take(&mut wc.waiters);
        wait_channel_remove(channels, name);
        waiters
    });
    wait_items_continue_all(waiters);
    TaskOutcome::Normal
}

pub fn wait_channel_wait(item: &dyn ContextSource, name: &CStr) -> TaskOutcome {
    let Some(c) = context_client(Some(&*item.borrow())) else {
        report_error(item, |out| out.write_all(b"not able to wait"));
        return TaskOutcome::Error;
    };
    WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = wait_channel_add(channels, name);
        if wc.woken != 0 {
            log_debug(format_args!(
                "wait channel {} already woken ({})",
                log_cstr(name),
                log_pointer(std::rc::Rc::as_ptr(&c))
            ));
            wait_channel_remove(channels, name);
            return TaskOutcome::Normal;
        }
        log_debug(format_args!(
            "wait channel {} not woken ({})",
            log_cstr(name),
            log_pointer(std::rc::Rc::as_ptr(&c))
        ));
        let (waiter, retval) = context_wait(item);
        wc.waiters.push(waiter);
        retval
    })
}

pub fn wait_channel_lock(item: &dyn ContextSource, name: &CStr) -> TaskOutcome {
    if context_client(Some(&*item.borrow())).is_none() {
        report_error(item, |out| out.write_all(b"not able to lock"));
        return TaskOutcome::Error;
    }
    WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = wait_channel_add(channels, name);
        if wc.locked != 0 {
            let (waiter, retval) = context_wait(item);
            wc.lockers.push(waiter);
            return retval;
        }
        wc.locked = 1 as ::core::ffi::c_int;
        TaskOutcome::Normal
    })
}

pub fn wait_channel_unlock(item: &dyn ContextSource, name: &CStr) -> TaskOutcome {
    let unlocked = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = channels.entries.get_mut(name).filter(|wc| wc.locked != 0)?;
        wait_channel_prune_expired(wc);
        if !wc.lockers.is_empty() {
            return Some(Some(wc.lockers.remove(0)));
        }
        wc.locked = 0 as ::core::ffi::c_int;
        wait_channel_remove(channels, name);
        Some(None)
    });
    let Some(next) = unlocked else {
        report_error(item, |out| {
            out.write_all(b"channel ")?;
            write_cstr(out, name)?;
            out.write_all(b" not locked")
        });
        return TaskOutcome::Error;
    };
    if let Some(locker) = next {
        locker.resume();
    }
    TaskOutcome::Normal
}

pub fn wait_for_client_lost(c: &ClientRef) {
    let lost_waiter = |waiter: &Completion| {
        waiter
            .client()
            .is_some_and(|owner| std::rc::Rc::ptr_eq(&owner, c))
    };
    let events = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        let (resumed, kept) = std::mem::take(items)
            .into_iter()
            .partition::<Vec<_>, _>(|wei| lost_waiter(&wei.waiter));
        *items = kept;
        resumed
    });
    for wei in events {
        wei.resume();
    }
    let waiters = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let mut resumed = Vec::new();
        let names = channels.entries.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let wc = channels.entries.get_mut(&name).expect("listed channel");
            for list in [&mut wc.waiters, &mut wc.lockers] {
                let (lost, kept) = std::mem::take(list)
                    .into_iter()
                    .partition::<Vec<_>, _>(lost_waiter);
                *list = kept;
                resumed.extend(lost);
            }
            wait_channel_remove_empty(channels, &name);
        }
        resumed
    });
    wait_items_continue_all(waiters);
}

pub fn wait_for_flush() {
    for wei in WAIT_EVENT_ITEMS.with_borrow_mut(std::mem::take) {
        wei.resume();
    }
    let waiters = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let mut resumed = Vec::new();
        let names = channels.entries.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let wc = channels.entries.get_mut(&name).expect("listed channel");
            resumed.append(&mut wc.waiters);
            wc.woken = 1 as ::core::ffi::c_int;
            resumed.append(&mut wc.lockers);
            wc.locked = 0 as ::core::ffi::c_int;
            wait_channel_remove(channels, &name);
        }
        resumed
    });
    wait_items_continue_all(waiters);
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{
        CStr, EventSinkId, WAIT_CHANNELS, WAIT_EVENT_ITEMS, wait_channel, wait_channel_add,
    };

    pub fn channel<R>(name: &CStr, f: impl FnOnce(&wait_channel) -> R) -> Option<R> {
        WAIT_CHANNELS.with_borrow(|channels| channels.entries.get(name).map(f))
    }

    pub fn with_channel_mut<R>(name: &CStr, f: impl FnOnce(&mut wait_channel) -> R) -> R {
        WAIT_CHANNELS.with_borrow_mut(|channels| f(wait_channel_add(channels, name)))
    }

    pub fn event_sinks() -> Vec<EventSinkId> {
        WAIT_EVENT_ITEMS.with_borrow(|items| items.iter().map(|wei| wei.sink.0).collect())
    }

    pub fn event_waiter_cancelled(index: usize) -> bool {
        WAIT_EVENT_ITEMS.with_borrow(|items| items[index].waiter.is_cancelled())
    }
}

#[cfg(test)]
mod tests {
    use super::{BTreeMap, CString, wait_channel_add, wait_channel_remove, wait_channels};

    #[test]
    fn wait_channels_match_c_string_tree_semantics() {
        let name_a = CString::new("a").unwrap();
        let name_a0 = CString::new("a0").unwrap();
        let name_a_high = CString::new(vec![b'a', 0xff]).unwrap();
        let name_z = CString::new("z").unwrap();
        let lookup_a = CString::new("a").unwrap();

        let mut channels = wait_channels {
            entries: BTreeMap::new(),
        };
        for name in [&name_z, &name_a_high, &name_a0, &name_a] {
            wait_channel_add(&mut channels, name);
        }
        wait_channel_add(&mut channels, &name_a).locked = 1;
        assert_eq!(wait_channel_add(&mut channels, &lookup_a).locked, 1);
        assert_eq!(
            channels.entries.keys().collect::<Vec<_>>(),
            [&name_a, &name_a0, &name_a_high, &name_z]
        );

        wait_channel_add(&mut channels, &name_a_high).woken = 1;
        wait_channel_remove(&mut channels, &name_a_high);
        assert!(!channels.entries.contains_key(name_a_high.as_c_str()));
        assert_eq!(channels.entries.len(), 3);
    }
}
