use crate::arguments::{args, args_get, args_has, args_parse, args_string};
use crate::cmd::cmd_get_args;
use crate::cmd::queue::{cmdq_error, cmdq_get_client, cmdq_print, cmdq_wait, CmdqWaiter};
use crate::events::{events_add_sink, events_remove_sink};
use crate::events_payload::{
    event_payload_add_formats, event_payload_item_name, event_payload_item_print_owned,
    event_payload_items,
};
use crate::format::bytes::write_cstr;
use crate::format::{format_create, format_expand_cstring, format_true};
use crate::hooks::hooks_valid_event_name;
use crate::log::{log_cstr, log_debug, log_pointer};
use crate::reactor::{evbuffer_add_formatted, evbuffer_new};
use crate::server_client::Client as _;
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::shared::events::{event_payload, events_callback, EventSinkId};
use crate::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::rc::Weak;

#[repr(C)]
pub struct wait_channel {
    pub locked: ::core::ffi::c_int,
    pub woken: ::core::ffi::c_int,
    pub(crate) waiters: Vec<CmdqWaiter>,
    pub(crate) lockers: Vec<CmdqWaiter>,
}

/// Channels by name, ordered as tmux's strcmp tree.
pub struct wait_channels {
    entries: std::collections::BTreeMap<std::ffi::CString, wait_channel>,
}

#[repr(C)]
pub struct wait_event_item {
    /// Read at event time only, to expand the filter; also the entry's key.
    pub item: Weak<std::cell::RefCell<cmdq_item>>,
    waiter: CmdqWaiter,
    sink: wait_event_sink,
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
// Each access borrows a registry only for its own duration; waiters are
// resumed, and printing and format expansion run, after the borrow ends.
thread_local! {
    static WAIT_EVENT_ITEMS: RefCell<Vec<wait_event_item>> = const { RefCell::new(Vec::new()) };
    static WAIT_CHANNELS: RefCell<wait_channels> = const {
        RefCell::new(wait_channels {
            entries: std::collections::BTreeMap::new(),
        })
    };
}

fn wait_items_continue_all(list: Vec<CmdqWaiter>) {
    for waiter in list {
        waiter.resume();
    }
}

fn wait_event_items_position(
    items: &[wait_event_item],
    item: &Weak<std::cell::RefCell<cmdq_item>>,
) -> Option<usize> {
    items
        .iter()
        .position(|owner| Weak::ptr_eq(&owner.item, item))
}

/// Find the channel `name`, adding it if missing.
fn cmd_wait_for_add<'a>(channels: &'a mut wait_channels, name: &CStr) -> &'a mut wait_channel {
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
fn cmd_wait_for_remove(channels: &mut wait_channels, name: &CStr) {
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
fn cmd_wait_for_remove_empty(channels: &mut wait_channels, name: &CStr) {
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

unsafe fn cmd_wait_for_waiter_client_name(waiter: &CmdqWaiter) -> CString {
    waiter
        .client()
        .and_then(|owner| owner.name())
        .unwrap_or_default()
}

fn cmd_wait_for_prune_expired(wc: &mut wait_channel) {
    wc.waiters.retain(|waiter| !waiter.is_cancelled());
    wc.lockers.retain(|waiter| !waiter.is_cancelled());
}
unsafe fn cmd_wait_for_exec(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let args = cmd_get_args(self_0.get_unchecked());
    let mut name: &CStr = args_string(args, 0 as u_int).expect("argument is present");
    if args_has(args, 'E' as i32 as u_char) != 0 {
        return cmd_wait_for_event(item_handle, name, args);
    }
    if args_has(args, 'l' as i32 as u_char) != 0 {
        return cmd_wait_for_list(item_handle, name);
    }
    if args_has(args, 'w' as i32 as u_char) != 0 {
        return cmd_wait_for_wake(args, name);
    }
    if args_has(args, 'S' as i32 as u_char) != 0 {
        return cmd_wait_for_signal(name);
    }
    if args_has(args, 'L' as i32 as u_char) != 0 {
        return cmd_wait_for_lock(item_handle, name);
    }
    if args_has(args, 'U' as i32 as u_char) != 0 {
        return cmd_wait_for_unlock(item_handle, name);
    }
    cmd_wait_for_wait(item_handle, name)
}
/// Print the payload to the waiting item's client, as cmdq_print would.
unsafe fn cmd_wait_for_event_print(client: Option<&ClientRef>, ep: &event_payload) {
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
unsafe fn cmd_wait_for_event_cb(
    _name: &CStr,
    payload: &mut event_payload,
    key: &Weak<std::cell::RefCell<cmdq_item>>,
) {
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
        cmd_wait_for_event_print(queue_client.as_ref(), ep);
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
/// The entry's event sink, removed when the entry is dropped.
struct wait_event_sink(EventSinkId);
impl Drop for wait_event_sink {
    fn drop(&mut self) {
        events_remove_sink(self.0);
    }
}

fn cmd_wait_for_event_prune_expired(items: &mut Vec<wait_event_item>) {
    items.retain(|owner| !owner.waiter.is_cancelled());
}
unsafe fn cmd_wait_for_event(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
    args: &args,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let mut filter: Option<&CStr> = args_get(args, 'F' as i32 as u_char);
    if hooks_valid_event_name(name) == 0 {
        cmdq_error(item_handle, |out| {
            out.write_all(b"invalid event: ")?;
            write_cstr(out, name)
        });
        return CmdOutcome::Error;
    }
    if args_has(args, 'l' as i32 as u_char) != 0 {
        return cmd_wait_for_event_list(item_handle, name);
    }
    if args_has(args, 'w' as i32 as u_char) != 0 {
        return cmd_wait_for_event_wake(item_handle, name, args);
    }
    if cmdq_get_client(Some(&*item)).is_none() {
        cmdq_error(item_handle, |out| out.write_all(b"not able to wait"));
        return CmdOutcome::Error;
    }
    let key = std::rc::Rc::downgrade(item_handle);
    let sink = wait_event_sink(events_add_sink(name, {
        let key = key.clone();
        events_callback(move |name, payload| unsafe { cmd_wait_for_event_cb(name, payload, &key) })
    }));
    let (waiter, retval) = cmdq_wait(item_handle);
    WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        items.push(wait_event_item {
            item: key,
            waiter,
            sink,
            name: name.to_owned(),
            filter: filter.map(CStr::to_owned),
            verbose: args_has(args, 'v' as i32 as u_char),
        })
    });
    retval
}
unsafe fn cmd_wait_for_event_list(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
) -> CmdOutcome {
    let names = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        cmd_wait_for_event_prune_expired(items);
        items
            .iter()
            .filter(|wei| wei.name.as_c_str() == name)
            .map(|wei| cmd_wait_for_waiter_client_name(&wei.waiter))
            .collect::<Vec<_>>()
    });
    for owner in names {
        cmdq_print(item_handle, |out| write_cstr(out, owner.as_c_str()));
    }
    CmdOutcome::Normal
}
unsafe fn cmd_wait_for_event_wake(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
    args: &args,
) -> CmdOutcome {
    let mut client_name: &CStr = args_get(args, 'w' as i32 as u_char).expect("-w has a value");
    let woken = WAIT_EVENT_ITEMS.with_borrow_mut(|items| {
        cmd_wait_for_event_prune_expired(items);
        let index = items.iter().position(|wei| {
            wei.name.as_c_str() == name
                && cmd_wait_for_waiter_client_name(&wei.waiter).as_c_str() == client_name
        })?;
        Some(items.remove(index))
    });
    if let Some(wei) = woken {
        wei.resume();
        return CmdOutcome::Normal;
    }
    cmdq_error(item_handle, |out| {
        out.write_all(b"waiter ")?;
        write_cstr(out, client_name)?;
        out.write_all(b" not found")
    });
    CmdOutcome::Error
}
unsafe fn cmd_wait_for_list(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
) -> CmdOutcome {
    let names = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let Some(wc) = channels.entries.get_mut(name) else {
            return Vec::new();
        };
        cmd_wait_for_prune_expired(wc);
        wc.waiters
            .iter()
            .chain(&wc.lockers)
            .map(|wi| cmd_wait_for_waiter_client_name(wi))
            .collect::<Vec<_>>()
    });
    for owner in names {
        cmdq_print(item_handle, |out| write_cstr(out, owner.as_c_str()));
    }
    CmdOutcome::Normal
}
unsafe fn cmd_wait_for_wake(args: &args, name: &CStr) -> CmdOutcome {
    let mut client_name: &CStr = args_get(args, 'w' as i32 as u_char).expect("-w has a value");
    let woken = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = channels.entries.get_mut(name)?;
        cmd_wait_for_prune_expired(wc);
        let waiter = [&mut wc.waiters, &mut wc.lockers]
            .into_iter()
            .find_map(|list| {
                let index = list
                    .iter()
                    .position(|wi| cmd_wait_for_waiter_client_name(wi).as_c_str() == client_name)?;
                Some(list.remove(index))
            })?;
        cmd_wait_for_remove_empty(channels, name);
        Some(waiter)
    });
    if let Some(waiter) = woken {
        waiter.resume();
    }
    CmdOutcome::Normal
}
fn cmd_wait_for_signal(name: &CStr) -> CmdOutcome {
    let waiters = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = cmd_wait_for_add(channels, name);
        cmd_wait_for_prune_expired(wc);
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
        cmd_wait_for_remove(channels, name);
        waiters
    });
    wait_items_continue_all(waiters);
    CmdOutcome::Normal
}
unsafe fn cmd_wait_for_wait(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let Some(c) = cmdq_get_client(Some(&*item)) else {
        cmdq_error(item_handle, |out| out.write_all(b"not able to wait"));
        return CmdOutcome::Error;
    };
    WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = cmd_wait_for_add(channels, name);
        if wc.woken != 0 {
            log_debug(format_args!(
                "wait channel {} already woken ({})",
                log_cstr(name),
                log_pointer(std::rc::Rc::as_ptr(&c))
            ));
            cmd_wait_for_remove(channels, name);
            return CmdOutcome::Normal;
        }
        log_debug(format_args!(
            "wait channel {} not woken ({})",
            log_cstr(name),
            log_pointer(std::rc::Rc::as_ptr(&c))
        ));
        let (waiter, retval) = cmdq_wait(item_handle);
        wc.waiters.push(waiter);
        retval
    })
}
unsafe fn cmd_wait_for_lock(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
) -> CmdOutcome {
    let item = item_handle.borrow();
    if cmdq_get_client(Some(&*item)).is_none() {
        cmdq_error(item_handle, |out| out.write_all(b"not able to lock"));
        return CmdOutcome::Error;
    }
    WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = cmd_wait_for_add(channels, name);
        if wc.locked != 0 {
            let (waiter, retval) = cmdq_wait(item_handle);
            wc.lockers.push(waiter);
            return retval;
        }
        wc.locked = 1 as ::core::ffi::c_int;
        CmdOutcome::Normal
    })
}
unsafe fn cmd_wait_for_unlock(
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
    name: &CStr,
) -> CmdOutcome {
    // None if the channel is not locked, else the locker the lock passes to.
    let unlocked = WAIT_CHANNELS.with_borrow_mut(|channels| {
        let wc = channels.entries.get_mut(name).filter(|wc| wc.locked != 0)?;
        cmd_wait_for_prune_expired(wc);
        if !wc.lockers.is_empty() {
            return Some(Some(wc.lockers.remove(0)));
        }
        wc.locked = 0 as ::core::ffi::c_int;
        cmd_wait_for_remove(channels, name);
        Some(None)
    });
    let Some(next) = unlocked else {
        cmdq_error(item_handle, |out| {
            out.write_all(b"channel ")?;
            write_cstr(out, name)?;
            out.write_all(b" not locked")
        });
        return CmdOutcome::Error;
    };
    if let Some(locker) = next {
        locker.resume();
    }
    CmdOutcome::Normal
}
/// Continue and forget every waiter, locker and event waiter whose item
/// belongs to a lost client, wherever that item is queued.
pub fn cmd_wait_for_client_lost(c: &ClientRef) {
    let lost_waiter = |waiter: &CmdqWaiter| {
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
            cmd_wait_for_remove_empty(channels, &name);
        }
        resumed
    });
    wait_items_continue_all(waiters);
}
pub fn cmd_wait_for_flush() {
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
            cmd_wait_for_remove(channels, &name);
        }
        resumed
    });
    wait_items_continue_all(waiters);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::queue::{cmdq_append, cmdq_get_callback_owned, cmdq_next};
    use crate::server_client::queue_test_clients as clients;
    use crate::shared::client::CLIENT_DEAD;
    use std::cell::RefCell;
    use std::ffi::CString;
    use std::future::Future;
    use std::rc::Rc;

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
        for name in [&name_z, &name_a_high, &name_a0, &name_a] {
            cmd_wait_for_add(&mut channels, name);
        }
        cmd_wait_for_add(&mut channels, &name_a).locked = 1;
        assert_eq!(cmd_wait_for_add(&mut channels, &lookup_a).locked, 1);
        assert_eq!(
            channels.entries.keys().collect::<Vec<_>>(),
            [&name_a, &name_a0, &name_a_high, &name_z]
        );

        cmd_wait_for_add(&mut channels, &name_a_high).woken = 1;
        cmd_wait_for_remove(&mut channels, &name_a_high);
        assert!(!channels.entries.contains_key(name_a_high.as_c_str()));
        assert_eq!(channels.entries.len(), 3);
    }

    /// Appends its index to the trace when woken.
    struct Record(usize, Rc<RefCell<Vec<usize>>>);
    impl std::task::LocalWake for Record {
        fn wake(self: Rc<Self>) {
            self.1.borrow_mut().push(self.0);
        }
    }

    /// Detached waiters whose resumes append their index to `trace`; the
    /// receivers must outlive the resumes.
    fn recorded_waiters(
        count: usize,
        trace: &Rc<RefCell<Vec<usize>>>,
    ) -> (Vec<CmdqWaiter>, Vec<tiri_rt::oneshot::Receiver<()>>) {
        (0..count)
            .map(|index| {
                let (waiter, mut resumed) = CmdqWaiter::detached(Weak::new());
                let waker = std::task::LocalWaker::from(Rc::new(Record(index, trace.clone())));
                let mut context = std::task::ContextBuilder::from_waker(std::task::Waker::noop())
                    .local_waker(&waker)
                    .build();
                assert!(std::pin::Pin::new(&mut resumed)
                    .poll(&mut context)
                    .is_pending());
                (waiter, resumed)
            })
            .unzip()
    }

    /// Read the channel `name`, if it exists.
    fn channel<R>(name: &CStr, f: impl FnOnce(&wait_channel) -> R) -> Option<R> {
        WAIT_CHANNELS.with_borrow(|channels| channels.entries.get(name).map(f))
    }

    /// Run `exec` as the next item of `owner`'s queue on the channel `name`.
    /// The returned item stays queued while it waits.
    unsafe fn run_on(
        owner: &ClientRef,
        name: &'static CStr,
        exec: unsafe fn(&Rc<std::cell::RefCell<cmdq_item>>, &CStr) -> CmdOutcome,
    ) -> Weak<std::cell::RefCell<cmdq_item>> {
        let item = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(c"wait-for", Some(Box::new(move |item| exec(item, name)))),
        );
        cmdq_next(Some(owner));
        item
    }

    fn signal(_: &Rc<std::cell::RefCell<cmdq_item>>, name: &CStr) -> CmdOutcome {
        cmd_wait_for_signal(name)
    }

    #[test]
    fn signal_resumes_waiters_in_fifo_order() {
        let (trace, locked) = Default::default();
        let (waiters, _resumed) = recorded_waiters(128, &trace);
        let (lockers, _locking) = recorded_waiters(4, &locked);
        WAIT_CHANNELS.with_borrow_mut(|channels| {
            let wc = cmd_wait_for_add(channels, c"fifo");
            wc.waiters.extend(waiters);
            wc.lockers.extend(lockers);
        });
        assert!(matches!(cmd_wait_for_signal(c"fifo"), CmdOutcome::Normal));
        assert_eq!(*trace.borrow(), (0..128).collect::<Vec<_>>());
        assert_eq!(
            channel(c"fifo", |wc| (wc.waiters.len(), wc.woken)),
            Some((0, 0))
        );
        assert!(locked.borrow().is_empty());
        assert_eq!(channel(c"fifo", |wc| wc.lockers.len()), Some(4));
    }

    #[test]
    fn unlock_hands_the_lock_to_the_first_locker_only() {
        unsafe {
            let [holder, first, second] = [(); 3].map(|_| clients::with_queue());
            assert!(run_on(&holder, c"l", cmd_wait_for_lock).upgrade().is_none());
            let first_item = run_on(&first, c"l", cmd_wait_for_lock);
            let second_item = run_on(&second, c"l", cmd_wait_for_lock);
            assert!(first_item.upgrade().is_some() && second_item.upgrade().is_some());

            assert!(run_on(&holder, c"l", cmd_wait_for_unlock)
                .upgrade()
                .is_none());
            cmdq_next(Some(&first));
            cmdq_next(Some(&second));
            assert!(first_item.upgrade().is_none());
            assert!(second_item.upgrade().is_some());
            assert_eq!(channel(c"l", |wc| wc.locked), Some(1));

            assert!(run_on(&first, c"l", cmd_wait_for_unlock)
                .upgrade()
                .is_none());
            cmdq_next(Some(&second));
            assert!(second_item.upgrade().is_none());
            assert_eq!(channel(c"l", |wc| wc.locked), Some(1));

            assert!(run_on(&second, c"l", cmd_wait_for_unlock)
                .upgrade()
                .is_none());
            assert_eq!(channel(c"l", |wc| wc.locked), Some(0));
        }
    }

    #[test]
    fn signal_skips_a_cancelled_waiter_and_stays_sticky() {
        unsafe {
            let gone = clients::with_queue();
            let gone_item = run_on(&gone, c"c", cmd_wait_for_wait);
            drop(gone);
            assert!(gone_item.upgrade().is_none());
            assert_eq!(channel(c"c", |wc| wc.waiters[0].is_cancelled()), Some(true));

            let other = clients::with_queue();
            assert!(run_on(&other, c"c", signal).upgrade().is_none());
            assert_eq!(channel(c"c", |wc| wc.waiters.len()), Some(0));
            assert_eq!(channel(c"c", |wc| wc.woken), Some(1));
            assert!(run_on(&other, c"c", cmd_wait_for_wait).upgrade().is_none());
            assert!(channel(c"c", |_| ()).is_none());
        }
    }

    #[test]
    fn client_lost_resumes_only_the_lost_clients_waiters() {
        unsafe {
            let [lost, kept, holder] = [(); 3].map(|_| clients::with_queue());
            let lost_waiter = run_on(&lost, c"c", cmd_wait_for_wait);
            let kept_waiter = run_on(&kept, c"c", cmd_wait_for_wait);
            assert!(run_on(&holder, c"l", cmd_wait_for_lock).upgrade().is_none());

            cmd_wait_for_client_lost(&lost);
            cmdq_next(Some(&lost));
            assert!(lost_waiter.upgrade().is_none());
            assert!(kept_waiter.upgrade().is_some());
            assert_eq!(channel(c"c", |wc| wc.waiters.len()), Some(1));

            let lost_locker = run_on(&lost, c"l", cmd_wait_for_lock);
            let kept_locker = run_on(&holder, c"l", cmd_wait_for_lock);
            cmd_wait_for_client_lost(&lost);
            cmdq_next(Some(&lost));
            assert!(lost_locker.upgrade().is_none());
            assert!(kept_locker.upgrade().is_some());
            assert!(kept_waiter.upgrade().is_some());
            assert_eq!(channel(c"l", |wc| wc.lockers.len()), Some(1));
            assert_eq!(channel(c"l", |wc| wc.locked), Some(1));
        }
    }

    #[test]
    fn a_lost_client_waiting_in_wait_for_is_freed() {
        unsafe {
            let owner = clients::with_queue();
            let head = run_on(&owner, c"c", cmd_wait_for_wait);
            owner.update_flags(CLIENT_DEAD as u64, 0);
            cmd_wait_for_client_lost(&owner);
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_none());
            assert!(channel(c"c", |_| ()).is_none());
            let client = Rc::downgrade(&owner);
            drop(owner);
            assert!(client.upgrade().is_none());
        }
    }

    /// Run `wait-for -E <flags> <name>` as the next item of `owner`'s queue.
    /// The returned item stays queued while it waits.
    unsafe fn wait_event(
        owner: &ClientRef,
        name: &'static CStr,
        flags: &[&CStr],
    ) -> Weak<std::cell::RefCell<cmdq_item>> {
        let values = [c"wait-for", c"-E"]
            .into_iter()
            .chain(flags.iter().copied())
            .chain([name])
            .map(crate::arguments::ParseValue::String)
            .collect::<Vec<_>>();
        let args = crate::arguments::args_parse(&cmd_wait_for_entry.args, &values).unwrap();
        let item = cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"wait-for",
                Some(Box::new(move |item| cmd_wait_for_event(item, name, &args))),
            ),
        );
        cmdq_next(Some(owner));
        item
    }

    unsafe fn fire(name: &CStr, value: &str) {
        let mut ep = crate::events_payload::event_payload_create();
        crate::events_payload::event_payload_set_string(&mut ep, c"value", |out| {
            out.write_all(value.as_bytes())
        });
        crate::events::events_fire(name, ep);
    }

    fn event_sinks() -> Vec<EventSinkId> {
        WAIT_EVENT_ITEMS.with_borrow(|items| items.iter().map(|wei| wei.sink.0).collect())
    }

    /// Queue a callback behind the waiter that records whether it ran.
    unsafe fn append_flag(owner: &ClientRef) -> Rc<std::cell::Cell<bool>> {
        let ran = Rc::new(std::cell::Cell::new(false));
        let flag = ran.clone();
        cmdq_append(
            Some(owner),
            cmdq_get_callback_owned(
                c"after",
                Some(Box::new(move |_| {
                    flag.set(true);
                    CmdOutcome::Normal
                })),
            ),
        );
        ran
    }

    #[test]
    fn an_accepted_event_resumes_the_event_waiter() {
        unsafe {
            let owner = clients::with_queue();
            let head = wait_event(&owner, c"@accepted", &[c"-F", c"#{==:#{value},y}"]);
            let ran = append_flag(&owner);
            let [sink] = event_sinks()[..] else {
                panic!("one event waiter")
            };
            assert!(head.upgrade().is_some());

            fire(c"@accepted", "y");
            assert!(event_sinks().is_empty());
            assert!(!crate::events::events_has_sink(sink));
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_none());
            assert!(ran.get());
        }
    }

    #[test]
    fn a_rejected_event_keeps_the_event_waiter_suspended() {
        unsafe {
            let owner = clients::with_queue();
            let head = wait_event(&owner, c"@rejected", &[c"-F", c"#{==:#{value},y}"]);
            let ran = append_flag(&owner);
            let [sink] = event_sinks()[..] else {
                panic!("one event waiter")
            };

            fire(c"@rejected", "x");
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_some());
            assert!(!ran.get());
            assert_eq!(event_sinks(), [sink]);
            assert!(crate::events::events_has_sink(sink));

            fire(c"@rejected", "y");
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_none());
            assert!(ran.get());
            assert!(!crate::events::events_has_sink(sink));
        }
    }

    #[test]
    fn a_dropped_event_waiter_is_pruned_by_the_next_event() {
        unsafe {
            let owner = clients::with_queue();
            let head = wait_event(&owner, c"@dropped", &[]);
            let [sink] = event_sinks()[..] else {
                panic!("one event waiter")
            };
            drop(owner);
            assert!(head.upgrade().is_none());
            assert!(WAIT_EVENT_ITEMS.with_borrow(|items| items[0].waiter.is_cancelled()));

            fire(c"@dropped", "");
            assert!(event_sinks().is_empty());
            assert!(!crate::events::events_has_sink(sink));
        }
    }

    #[test]
    fn wake_resumes_the_first_matching_event_waiter_only() {
        unsafe {
            let [first, second, waker] = [(); 3].map(|_| clients::with_queue());
            let first_item = wait_event(&first, c"@wake", &[]);
            let second_item = wait_event(&second, c"@wake", &[]);
            let [first_sink, second_sink] = event_sinks()[..] else {
                panic!("two event waiters")
            };
            assert!(wait_event(&waker, c"@wake", &[c"-w", c""])
                .upgrade()
                .is_none());
            cmdq_next(Some(&first));
            cmdq_next(Some(&second));
            assert!(first_item.upgrade().is_none());
            assert!(second_item.upgrade().is_some());
            assert!(!crate::events::events_has_sink(first_sink));
            assert_eq!(event_sinks(), [second_sink]);
        }
    }

    #[test]
    fn client_lost_resumes_only_the_lost_clients_event_waiters() {
        unsafe {
            let [lost, kept] = [(); 2].map(|_| clients::with_queue());
            let lost_item = wait_event(&lost, c"@lost", &[]);
            let kept_item = wait_event(&kept, c"@lost", &[]);
            let [lost_sink, kept_sink] = event_sinks()[..] else {
                panic!("two event waiters")
            };

            cmd_wait_for_client_lost(&lost);
            cmdq_next(Some(&lost));
            cmdq_next(Some(&kept));
            assert!(lost_item.upgrade().is_none());
            assert!(kept_item.upgrade().is_some());
            assert!(!crate::events::events_has_sink(lost_sink));
            assert_eq!(event_sinks(), [kept_sink]);
        }
    }

    #[test]
    fn flush_resumes_every_event_waiter() {
        unsafe {
            let [first, second] = [(); 2].map(|_| clients::with_queue());
            let items = [
                wait_event(&first, c"@flush", &[]),
                wait_event(&second, c"@flush", &[c"-F", c"0"]),
            ];
            let sinks = event_sinks();
            assert_eq!(sinks.len(), 2);

            cmd_wait_for_flush();
            assert!(event_sinks().is_empty());
            for sink in sinks {
                assert!(!crate::events::events_has_sink(sink));
            }
            cmdq_next(Some(&first));
            cmdq_next(Some(&second));
            assert!(items.iter().all(|item| item.upgrade().is_none()));
        }
    }

    #[test]
    fn a_lost_client_waiting_for_an_event_is_freed() {
        unsafe {
            let owner = clients::with_queue();
            let head = wait_event(&owner, c"@freed", &[]);
            let [sink] = event_sinks()[..] else {
                panic!("one event waiter")
            };
            owner.update_flags(CLIENT_DEAD as u64, 0);
            cmd_wait_for_client_lost(&owner);
            cmdq_next(Some(&owner));
            assert!(head.upgrade().is_none());
            assert!(event_sinks().is_empty());
            assert!(!crate::events::events_has_sink(sink));
            let client = Rc::downgrade(&owner);
            drop(owner);
            assert!(client.upgrade().is_none());
        }
    }
}
