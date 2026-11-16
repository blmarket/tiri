use crate::events::{events_add_sink, events_remove_sink};
use crate::events_payload::{
    event_payload_add_formats, event_payload_item_name, event_payload_item_print_owned,
    event_payload_items,
};
use crate::format::bytes::{format_message_with, write_cstr};
use crate::format::{format_create, format_expand_cstring, format_true};
use crate::log::{log_cstr, log_debug, log_pointer};
use crate::server_client::Client as _;
use crate::shared::client::{ClientRef, ClientWeak};
use crate::shared::events::{EventSinkId, event_payload, events_callback};
use crate::shared::format::{CommandFormat, FORMAT_NOJOBS, FORMAT_NONE};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::future::Future;
use tiri_buffer::SegmentedBuf;

pub struct Waiter {
    client: ClientWeak,
    done: crate::tiri_rt::oneshot::Sender<()>,
}

impl Waiter {
    fn pair(client: ClientWeak) -> (Self, impl Future<Output = ()> + use<>) {
        let (done, receiver) = crate::tiri_rt::oneshot::channel();
        let wait = async move {
            if receiver.await.is_none() {
                std::future::pending::<()>().await;
            }
        };
        (Self { client, done }, wait)
    }

    pub fn client(&self) -> Option<ClientRef> {
        self.client.upgrade()
    }

    pub fn is_cancelled(&self) -> bool {
        self.done.is_closed()
    }

    pub fn resume(self) {
        let _ = self.done.send(());
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn detached(client: ClientWeak) -> (Self, crate::tiri_rt::oneshot::Receiver<()>) {
        let (done, receiver) = crate::tiri_rt::oneshot::channel();
        (Self { client, done }, receiver)
    }
}

#[repr(C)]
pub struct wait_channel {
    pub locked: ::core::ffi::c_int,
    pub woken: ::core::ffi::c_int,
    pub waiters: Vec<Waiter>,
    pub lockers: Vec<Waiter>,
}

struct wait_channels {
    entries: BTreeMap<CString, wait_channel>,
}

struct wait_event_item {
    id: u64,
    waiter: Waiter,
    sink: wait_event_sink,
    name: CString,
    filter: Option<CString>,
    format: Option<CommandFormat>,
    verbose: ::core::ffi::c_int,
}

thread_local! {
    static NEXT_WAIT_EVENT_ID: Cell<u64> = const { Cell::new(0) };
    static WAIT_EVENT_ITEMS: RefCell<Vec<wait_event_item>> = const { RefCell::new(Vec::new()) };
    static WAIT_CHANNELS: RefCell<wait_channels> = const {
        RefCell::new(wait_channels {
            entries: BTreeMap::new(),
        })
    };
}

fn wait_items_continue_all(list: Vec<Waiter>) {
    for waiter in list {
        waiter.resume();
    }
}

fn wait_event_items_position(items: &[wait_event_item], id: u64) -> Option<usize> {
    items.iter().position(|owner| owner.id == id)
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

fn waiter_client_name(waiter: &Waiter) -> CString {
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
            let mut evb = SegmentedBuf::default();
            let _ = write_cstr(&mut evb, key);
            evb.extend_from_slice(b"=");
            let _ = write_cstr(
                &mut evb,
                CStr::from_bytes_until_nul(&value).expect("payload value is NUL terminated"),
            );
            ClientRef::print_to(client, true, &mut evb);
        }
    }
}

fn wait_event_cb(_name: &CStr, payload: &mut event_payload, id: u64) {
    let found = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        let index = wait_event_items_position(items, id)?;
        let wei = &items[index];
        if wei.waiter.is_cancelled() {
            drop(items.remove(index));
            return None;
        }
        Some((
            wei.verbose,
            wei.filter.clone(),
            wei.format.clone(),
            wei.waiter.client(),
        ))
    });
    let Some((verbose, filter, cmd_format, queue_client)) = found else {
        return;
    };
    let ep = &*payload;
    if verbose != 0 {
        wait_event_print(queue_client.as_ref(), ep);
        if WAIT_EVENT_ITEMS.with_borrow(|items| wait_event_items_position(items, id).is_none()) {
            return;
        }
    }
    if let Some(filter) = &filter {
        let mut ft_owner = format_create(
            queue_client.as_ref(),
            cmd_format.as_ref(),
            FORMAT_NONE,
            FORMAT_NOJOBS,
        );
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
        wait_event_items_position(items, id).map(|index| items.remove(index))
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
    client: Option<&ClientRef>,
    owning_client: ClientWeak,
    format: Option<CommandFormat>,
    name: &CStr,
    filter: Option<&CStr>,
    verbose: ::core::ffi::c_int,
) -> Result<impl Future<Output = ()> + use<>, &'static CStr> {
    if client.is_none() {
        return Err(c"not able to wait");
    }
    let id = NEXT_WAIT_EVENT_ID.with(|cell| {
        let next = cell.get().wrapping_add(1);
        cell.set(next);
        next
    });
    let sink = wait_event_sink(events_add_sink(
        name,
        events_callback(move |name, payload| wait_event_cb(name, payload, id)),
    ));
    let (waiter, wait) = Waiter::pair(owning_client);
    WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        items.push(wait_event_item {
            id,
            waiter,
            sink,
            name: name.to_owned(),
            filter: filter.map(CStr::to_owned),
            format,
            verbose,
        })
    });
    Ok(wait)
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

pub fn wait_event_wake(name: &CStr, client_name: &CStr) -> Result<(), CString> {
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
        return Ok(());
    }
    Err(format_message_with(|out| {
        out.write_all(b"waiter ")?;
        write_cstr(out, client_name)?;
        out.write_all(b" not found")
    }))
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

pub fn wait_channel_signal(name: &CStr) {
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
}

pub fn wait_channel_wait(
    client: Option<&ClientRef>,
    owning_client: ClientWeak,
    name: &CStr,
) -> Result<Option<impl Future<Output = ()> + use<>>, &'static CStr> {
    let Some(c) = client else {
        return Err(c"not able to wait");
    };
    Ok(WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = wait_channel_add(channels, name);
        if wc.woken != 0 {
            log_debug(format_args!(
                "wait channel {} already woken ({})",
                log_cstr(name),
                log_pointer(std::rc::Rc::as_ptr(c))
            ));
            wait_channel_remove(channels, name);
            return None;
        }
        log_debug(format_args!(
            "wait channel {} not woken ({})",
            log_cstr(name),
            log_pointer(std::rc::Rc::as_ptr(c))
        ));
        let (waiter, wait) = Waiter::pair(owning_client);
        wc.waiters.push(waiter);
        Some(wait)
    }))
}

pub fn wait_channel_lock(
    client: Option<&ClientRef>,
    owning_client: ClientWeak,
    name: &CStr,
) -> Result<Option<impl Future<Output = ()> + use<>>, &'static CStr> {
    if client.is_none() {
        return Err(c"not able to lock");
    }
    Ok(WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = wait_channel_add(channels, name);
        if wc.locked != 0 {
            let (waiter, wait) = Waiter::pair(owning_client);
            wc.lockers.push(waiter);
            return Some(wait);
        }
        wc.locked = 1 as ::core::ffi::c_int;
        None
    }))
}

pub fn wait_channel_unlock(name: &CStr) -> Result<(), CString> {
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
        return Err(format_message_with(|out| {
            out.write_all(b"channel ")?;
            write_cstr(out, name)?;
            out.write_all(b" not locked")
        }));
    };
    if let Some(locker) = next {
        locker.resume();
    }
    Ok(())
}

pub fn wait_for_client_lost(c: &ClientRef) {
    let lost_waiter = |waiter: &Waiter| {
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
    pub use super::Waiter;
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
