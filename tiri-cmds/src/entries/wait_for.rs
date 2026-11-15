use crate::cmd::cmd_get_args;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::shared::command::CMD_FIND_PANE;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag};
use std::ffi::CStr;
use std::future::Future;
use std::pin::Pin;
use tiri_core::arguments::{args, args_get, args_has, args_parse, args_string};
use tiri_core::format::bytes::write_cstr;
use tiri_core::hooks::hooks_valid_event_name;
use tiri_core::shared::abi::{u_char, u_int};
use tiri_core::wait::{
    wait_channel_list, wait_channel_lock, wait_channel_signal, wait_channel_unlock,
    wait_channel_wait, wait_channel_wake, wait_event_list, wait_event_wait, wait_event_wake,
};

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

fn cmd_wait_for_exec<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
    Box::pin(async move {
        let args = cmd_get_args(self_0);
        let name: &CStr = args_string(args, 0 as u_int).expect("argument is present");
        if args_has(args, 'E' as i32 as u_char) != 0 {
            return cmd_wait_for_event(self_0, inv, name, args).await;
        }
        if args_has(args, 'l' as i32 as u_char) != 0 {
            for owner in wait_channel_list(name) {
                inv.print(|out| write_cstr(out, owner.as_c_str()));
            }
            return ExecOutcome::ok();
        }
        if args_has(args, 'w' as i32 as u_char) != 0 {
            let client_name: &CStr = args_get(args, 'w' as i32 as u_char).expect("-w has a value");
            wait_channel_wake(name, client_name);
            return ExecOutcome::ok();
        }
        if args_has(args, 'S' as i32 as u_char) != 0 {
            wait_channel_signal(name);
            return ExecOutcome::ok();
        }
        if args_has(args, 'L' as i32 as u_char) != 0 {
            let c = inv.client();
            let res = wait_channel_lock(c.as_ref(), inv.owning_client(), name);
            drop(c);
            return match res {
                Ok(Some(fut)) => {
                    inv.suspend(self_0);
                    fut.await;
                    ExecOutcome::ok()
                }
                Ok(None) => ExecOutcome::ok(),
                Err(err) => {
                    inv.error(|out| write_cstr(out, err));
                    ExecOutcome::err()
                }
            };
        }
        if args_has(args, 'U' as i32 as u_char) != 0 {
            return match wait_channel_unlock(name) {
                Ok(()) => ExecOutcome::ok(),
                Err(err) => {
                    inv.error(|out| write_cstr(out, &*err));
                    ExecOutcome::err()
                }
            };
        }
        let c = inv.client();
        let res = wait_channel_wait(c.as_ref(), inv.owning_client(), name);
        drop(c);
        match res {
            Ok(Some(fut)) => {
                inv.suspend(self_0);
                fut.await;
                ExecOutcome::ok()
            }
            Ok(None) => ExecOutcome::ok(),
            Err(err) => {
                inv.error(|out| write_cstr(out, err));
                ExecOutcome::err()
            }
        }
    })
}

async fn cmd_wait_for_event<'a, 'ctx>(
    self_0: &'a cmd,
    inv: &'a mut Invocation<'ctx>,
    name: &CStr,
    args: &args,
) -> ExecOutcome {
    let filter: Option<&CStr> = args_get(args, 'F' as i32 as u_char);
    if hooks_valid_event_name(name) == 0 {
        inv.error(|out| {
            out.write_all(b"invalid event: ")?;
            write_cstr(out, name)
        });
        return ExecOutcome::err();
    }
    if args_has(args, 'l' as i32 as u_char) != 0 {
        for owner in wait_event_list(name) {
            inv.print(|out| write_cstr(out, owner.as_c_str()));
        }
        return ExecOutcome::ok();
    }
    if args_has(args, 'w' as i32 as u_char) != 0 {
        let client_name: &CStr = args_get(args, 'w' as i32 as u_char).expect("-w has a value");
        return match wait_event_wake(name, client_name) {
            Ok(()) => ExecOutcome::ok(),
            Err(err) => {
                inv.error(|out| write_cstr(out, &*err));
                ExecOutcome::err()
            }
        };
    }
    let c = inv.client();
    let format = filter.map(|_| inv.format());
    let res = wait_event_wait(
        c.as_ref(),
        inv.owning_client(),
        format,
        name,
        filter,
        args_has(args, 'v' as i32 as u_char),
    );
    drop(c);
    match res {
        Ok(fut) => {
            inv.suspend(self_0);
            fut.await;
            ExecOutcome::ok()
        }
        Err(err) => {
            inv.error(|out| write_cstr(out, err));
            ExecOutcome::err()
        }
    }
}

#[cfg(feature = "test-support")]
pub mod wait_for_tests {
    use super::{CStr, Future, Invocation, Pin, cmd, cmd_wait_for_entry, cmd_wait_for_event};
    use crate::cmd::invocation::ExecOutcome;
    use crate::cmd::run::test_harness::spawn_invocation;
    use std::cell::RefCell;
    use std::rc::{Rc, Weak};
    use tiri_core::reactor::Lane;
    use tiri_core::server_client::Client as _;
    use tiri_core::server_client::queue_test_clients as clients;
    use tiri_core::shared::client::{CLIENT_DEAD, ClientRef};
    use tiri_core::wait::test_support::{
        Waiter, channel, event_sinks, event_waiter_cancelled, with_channel_mut,
    };
    use tiri_core::wait::{
        wait_channel_lock, wait_channel_signal, wait_channel_unlock, wait_channel_wait,
        wait_for_client_lost, wait_for_flush,
    };

    struct OnWake(std::cell::Cell<Option<Box<dyn FnOnce()>>>);
    impl std::task::LocalWake for OnWake {
        fn wake(self: Rc<Self>) {
            if let Some(on_wake) = self.0.take() {
                on_wake();
            }
        }
    }

    fn waiter_on_wake(
        on_wake: impl FnOnce() + 'static,
    ) -> (Waiter, tiri_core::tiri_rt::oneshot::Receiver<()>) {
        let (waiter, mut resumed) = Waiter::detached(Weak::new());
        let waker = std::task::LocalWaker::from(Rc::new(OnWake(std::cell::Cell::new(Some(
            Box::new(on_wake),
        )))));
        let mut context = std::task::ContextBuilder::from_waker(std::task::Waker::noop())
            .local_waker(&waker)
            .build();
        assert!(
            std::pin::Pin::new(&mut resumed)
                .poll(&mut context)
                .is_pending()
        );
        (waiter, resumed)
    }

    fn recorded_waiters(
        count: usize,
        trace: &Rc<RefCell<Vec<usize>>>,
    ) -> (
        Vec<Waiter>,
        Vec<tiri_core::tiri_rt::oneshot::Receiver<()>>,
    ) {
        (0..count)
            .map(|index| {
                let trace = trace.clone();
                waiter_on_wake(move || trace.borrow_mut().push(index))
            })
            .unzip()
    }

    type WaitOp = for<'a, 'ctx> fn(
        &'a cmd,
        &'a mut Invocation<'ctx>,
        &'a CStr,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>>;

    fn run_on(owner: &ClientRef, name: &'static CStr, exec: WaitOp) -> Weak<()> {
        let item = spawn_invocation(Some(owner), move |cmd_ref, inv| exec(cmd_ref, inv, name));
        Lane::drive(Some(owner));
        item
    }

    fn wait_fn<'a, 'ctx>(
        cmd_ref: &'a cmd,
        inv: &'a mut Invocation<'ctx>,
        name: &'a CStr,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
        Box::pin(async move {
            let c = inv.client();
            let fut = wait_channel_wait(c.as_ref(), inv.owning_client(), name).unwrap();
            drop(c);
            if let Some(fut) = fut {
                inv.suspend(cmd_ref);
                fut.await;
            }
            ExecOutcome::ok()
        })
    }

    fn lock_fn<'a, 'ctx>(
        cmd_ref: &'a cmd,
        inv: &'a mut Invocation<'ctx>,
        name: &'a CStr,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
        Box::pin(async move {
            let c = inv.client();
            let fut = wait_channel_lock(c.as_ref(), inv.owning_client(), name).unwrap();
            drop(c);
            if let Some(fut) = fut {
                inv.suspend(cmd_ref);
                fut.await;
            }
            ExecOutcome::ok()
        })
    }

    fn unlock_fn<'a, 'ctx>(
        _cmd_ref: &'a cmd,
        _inv: &'a mut Invocation<'ctx>,
        name: &'a CStr,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
        Box::pin(async move {
            wait_channel_unlock(name).unwrap();
            ExecOutcome::ok()
        })
    }

    fn signal_fn<'a, 'ctx>(
        _cmd_ref: &'a cmd,
        _inv: &'a mut Invocation<'ctx>,
        name: &'a CStr,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
        Box::pin(async move {
            wait_channel_signal(name);
            ExecOutcome::ok()
        })
    }

    pub fn signal_resumes_waiters_in_fifo_order() {
        let (trace, locked) = Default::default();
        let (waiters, _resumed) = recorded_waiters(128, &trace);
        let (lockers, _locking) = recorded_waiters(4, &locked);
        with_channel_mut(c"fifo", |wc| {
            wc.waiters.extend(waiters);
            wc.lockers.extend(lockers);
        });
        wait_channel_signal(c"fifo");
        assert_eq!(*trace.borrow(), (0..128).collect::<Vec<_>>());
        assert_eq!(
            channel(c"fifo", |wc| (wc.waiters.len(), wc.woken)),
            Some((0, 0))
        );
        assert!(locked.borrow().is_empty());
        assert_eq!(channel(c"fifo", |wc| wc.lockers.len()), Some(4));
    }

    pub fn unlock_hands_the_lock_to_the_first_locker_only() {
        let [holder, first, second] = [(); 3].map(|_| clients::with_queue());
        assert!(run_on(&holder, c"l", lock_fn).upgrade().is_none());
        let first_item = run_on(&first, c"l", lock_fn);
        let second_item = run_on(&second, c"l", lock_fn);
        assert!(first_item.upgrade().is_some() && second_item.upgrade().is_some());

        assert!(run_on(&holder, c"l", unlock_fn).upgrade().is_none());
        Lane::drive(Some(&first));
        Lane::drive(Some(&second));
        assert!(first_item.upgrade().is_none());
        assert!(second_item.upgrade().is_some());
        assert_eq!(channel(c"l", |wc| wc.locked), Some(1));

        assert!(run_on(&first, c"l", unlock_fn).upgrade().is_none());
        Lane::drive(Some(&second));
        assert!(second_item.upgrade().is_none());
        assert_eq!(channel(c"l", |wc| wc.locked), Some(1));

        assert!(run_on(&second, c"l", unlock_fn).upgrade().is_none());
        assert_eq!(channel(c"l", |wc| wc.locked), Some(0));
    }

    fn reenter_on_wake(
        owner: &ClientRef,
        name: &'static CStr,
        exec: WaitOp,
    ) -> (
        Waiter,
        tiri_core::tiri_rt::oneshot::Receiver<()>,
        Rc<std::cell::OnceCell<Weak<()>>>,
    ) {
        let reentered = Rc::new(std::cell::OnceCell::new());
        let (owner, cell) = (owner.clone(), reentered.clone());
        let (waiter, resumed) = waiter_on_wake(move || {
            assert!(cell.set(run_on(&owner, name, exec)).is_ok());
        });
        (waiter, resumed, reentered)
    }

    pub fn a_waiter_woken_by_signal_can_wait_again_on_the_same_channel() {
        let owner = clients::with_queue();
        let (waiter, _resumed, rewaited) = reenter_on_wake(&owner, c"rewait", wait_fn);
        with_channel_mut(c"rewait", |wc| wc.waiters.push(waiter));

        wait_channel_signal(c"rewait");
        let item = rewaited.get().expect("signal woke the waiter");
        assert!(item.upgrade().is_some());
        assert_eq!(
            channel(c"rewait", |wc| (wc.waiters.len(), wc.woken)),
            Some((1, 0))
        );

        wait_channel_signal(c"rewait");
        Lane::drive(Some(&owner));
        assert!(item.upgrade().is_none());
    }

    pub fn a_waiter_woken_by_signal_can_signal_the_same_channel_again() {
        let trace = Default::default();
        let (resignaller, _resignalled) = waiter_on_wake(|| {
            wait_channel_signal(c"resignal");
        });
        let (waiters, _resumed) = recorded_waiters(1, &trace);
        with_channel_mut(c"resignal", |wc| {
            wc.waiters.push(resignaller);
            wc.waiters.extend(waiters);
        });

        wait_channel_signal(c"resignal");
        assert_eq!(*trace.borrow(), [0]);
        assert_eq!(
            channel(c"resignal", |wc| (wc.waiters.len(), wc.woken)),
            Some((0, 1))
        );
    }

    pub fn unlock_hands_the_lock_to_a_locker_that_unlocks_on_wake() {
        let [holder, relay] = [(); 2].map(|_| clients::with_queue());
        assert!(run_on(&holder, c"relay", lock_fn).upgrade().is_none());
        let (unlocker, _unlocking, unlocked) = reenter_on_wake(&relay, c"relay", unlock_fn);
        let trace = Default::default();
        let (next, _locking) = recorded_waiters(1, &trace);
        with_channel_mut(c"relay", |wc| {
            wc.lockers.push(unlocker);
            wc.lockers.extend(next);
        });

        assert!(run_on(&holder, c"relay", unlock_fn).upgrade().is_none());
        let item = unlocked.get().expect("unlock handed over the lock");
        assert!(item.upgrade().is_none());
        assert_eq!(*trace.borrow(), [0]);
        assert_eq!(
            channel(c"relay", |wc| (wc.locked, wc.lockers.len())),
            Some((1, 0))
        );
    }

    pub fn unlock_hands_the_lock_to_a_locker_that_relocks_on_wake() {
        let [holder, relocker] = [(); 2].map(|_| clients::with_queue());
        assert!(run_on(&holder, c"relock", lock_fn).upgrade().is_none());
        let (locker, _locking, relocked) = reenter_on_wake(&relocker, c"relock", lock_fn);
        with_channel_mut(c"relock", |wc| wc.lockers.push(locker));

        assert!(run_on(&holder, c"relock", unlock_fn).upgrade().is_none());
        let item = relocked.get().expect("unlock handed over the lock");
        assert!(item.upgrade().is_some());
        assert_eq!(
            channel(c"relock", |wc| (wc.locked, wc.lockers.len())),
            Some((1, 1))
        );

        assert!(run_on(&holder, c"relock", unlock_fn).upgrade().is_none());
        Lane::drive(Some(&relocker));
        assert!(item.upgrade().is_none());
        assert_eq!(
            channel(c"relock", |wc| (wc.locked, wc.lockers.len())),
            Some((1, 0))
        );
    }

    pub fn flush_survives_a_locker_that_relocks_on_wake() {
        let relocker = clients::with_queue();
        let (locker, _locking, relocked) = reenter_on_wake(&relocker, c"flushed", lock_fn);
        with_channel_mut(c"flushed", |wc| {
            wc.locked = 1;
            wc.lockers.push(locker);
        });

        wait_for_flush();
        let item = relocked.get().expect("flush woke the locker");
        assert!(item.upgrade().is_none());
        assert_eq!(
            channel(c"flushed", |wc| (wc.locked, wc.woken, wc.lockers.len())),
            Some((1, 0, 0))
        );
    }

    pub fn signal_skips_a_cancelled_waiter_and_stays_sticky() {
        let gone = clients::with_queue();
        let gone_item = run_on(&gone, c"c", wait_fn);
        drop(gone);
        assert!(gone_item.upgrade().is_none());
        assert_eq!(channel(c"c", |wc| wc.waiters[0].is_cancelled()), Some(true));

        let other = clients::with_queue();
        assert!(run_on(&other, c"c", signal_fn).upgrade().is_none());
        assert_eq!(channel(c"c", |wc| wc.waiters.len()), Some(0));
        assert_eq!(channel(c"c", |wc| wc.woken), Some(1));
        assert!(run_on(&other, c"c", wait_fn).upgrade().is_none());
        assert!(channel(c"c", |_| ()).is_none());
    }

    pub fn client_lost_resumes_only_the_lost_clients_waiters() {
        let [lost, kept, holder] = [(); 3].map(|_| clients::with_queue());
        let lost_waiter = run_on(&lost, c"c", wait_fn);
        let kept_waiter = run_on(&kept, c"c", wait_fn);
        assert!(run_on(&holder, c"l", lock_fn).upgrade().is_none());

        wait_for_client_lost(&lost);
        Lane::drive(Some(&lost));
        assert!(lost_waiter.upgrade().is_none());
        assert!(kept_waiter.upgrade().is_some());
        assert_eq!(channel(c"c", |wc| wc.waiters.len()), Some(1));

        let lost_locker = run_on(&lost, c"l", lock_fn);
        let kept_locker = run_on(&holder, c"l", lock_fn);
        wait_for_client_lost(&lost);
        Lane::drive(Some(&lost));
        assert!(lost_locker.upgrade().is_none());
        assert!(kept_locker.upgrade().is_some());
        assert!(kept_waiter.upgrade().is_some());
        assert_eq!(channel(c"l", |wc| wc.lockers.len()), Some(1));
        assert_eq!(channel(c"l", |wc| wc.locked), Some(1));
    }

    pub fn a_lost_client_waiting_in_wait_for_is_freed() {
        let owner = clients::with_queue();
        let head = run_on(&owner, c"c", wait_fn);
        owner.update_flags(CLIENT_DEAD as u64, 0);
        wait_for_client_lost(&owner);
        Lane::drive(Some(&owner));
        assert!(head.upgrade().is_none());
        assert!(channel(c"c", |_| ()).is_none());
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_none());
    }

    fn wait_event(owner: &ClientRef, name: &'static CStr, flags: &[&CStr]) -> Weak<()> {
        let values = [c"wait-for", c"-E"]
            .into_iter()
            .chain(flags.iter().copied())
            .chain([name])
            .map(tiri_core::arguments::ParseValue::String)
            .collect::<Vec<_>>();
        let args = tiri_core::arguments::args_parse(&cmd_wait_for_entry.args, &values).unwrap();
        let item = spawn_invocation(
            Some(owner),
            move |cmd_ref: &cmd, inv: &mut Invocation<'_>| -> Pin<Box<dyn Future<Output = ExecOutcome> + '_>> {
                Box::pin(async move { cmd_wait_for_event(cmd_ref, inv, name, &args).await })
            },
        );
        Lane::drive(Some(owner));
        item
    }

    fn fire(name: &CStr, value: &str) {
        let mut ep = tiri_core::events_payload::event_payload_create();
        tiri_core::events_payload::event_payload_set_string(&mut ep, c"value", |out| {
            out.write_all(value.as_bytes())
        });
        tiri_core::events::events_fire(name, ep);
    }

    fn append_flag(owner: &ClientRef) -> Rc<std::cell::Cell<bool>> {
        let ran = Rc::new(std::cell::Cell::new(false));
        let flag = ran.clone();
        spawn_invocation(Some(owner), move |_, _| {
            Box::pin(async move {
                flag.set(true);
                ExecOutcome::ok()
            })
        });
        ran
    }

    pub fn an_accepted_event_resumes_the_event_waiter() {
        let owner = clients::with_queue();
        let head = wait_event(&owner, c"@accepted", &[c"-F", c"#{==:#{value},y}"]);
        let ran = append_flag(&owner);
        let [sink] = event_sinks()[..] else {
            panic!("one event waiter")
        };
        assert!(head.upgrade().is_some());

        fire(c"@accepted", "y");
        assert!(event_sinks().is_empty());
        assert!(!tiri_core::events::events_has_sink(sink));
        Lane::drive(Some(&owner));
        assert!(head.upgrade().is_none());
        assert!(ran.get());
    }

    pub fn a_rejected_event_keeps_the_event_waiter_suspended() {
        let owner = clients::with_queue();
        let head = wait_event(&owner, c"@rejected", &[c"-F", c"#{==:#{value},y}"]);
        let ran = append_flag(&owner);
        let [sink] = event_sinks()[..] else {
            panic!("one event waiter")
        };

        fire(c"@rejected", "x");
        Lane::drive(Some(&owner));
        assert!(head.upgrade().is_some());
        assert!(!ran.get());
        assert_eq!(event_sinks(), [sink]);
        assert!(tiri_core::events::events_has_sink(sink));

        fire(c"@rejected", "y");
        Lane::drive(Some(&owner));
        assert!(head.upgrade().is_none());
        assert!(ran.get());
        assert!(!tiri_core::events::events_has_sink(sink));
    }

    pub fn a_dropped_event_waiter_is_pruned_by_the_next_event() {
        let owner = clients::with_queue();
        let head = wait_event(&owner, c"@dropped", &[]);
        let [sink] = event_sinks()[..] else {
            panic!("one event waiter")
        };
        drop(owner);
        assert!(head.upgrade().is_none());
        assert!(event_waiter_cancelled(0));

        fire(c"@dropped", "");
        assert!(event_sinks().is_empty());
        assert!(!tiri_core::events::events_has_sink(sink));
    }

    pub fn wake_resumes_the_first_matching_event_waiter_only() {
        let [first, second, waker] = [(); 3].map(|_| clients::with_queue());
        let first_item = wait_event(&first, c"@wake", &[]);
        let second_item = wait_event(&second, c"@wake", &[]);
        let [first_sink, second_sink] = event_sinks()[..] else {
            panic!("two event waiters")
        };
        assert!(
            wait_event(&waker, c"@wake", &[c"-w", c""])
                .upgrade()
                .is_none()
        );
        Lane::drive(Some(&first));
        Lane::drive(Some(&second));
        assert!(first_item.upgrade().is_none());
        assert!(second_item.upgrade().is_some());
        assert!(!tiri_core::events::events_has_sink(first_sink));
        assert_eq!(event_sinks(), [second_sink]);
    }

    pub fn client_lost_resumes_only_the_lost_clients_event_waiters() {
        let [lost, kept] = [(); 2].map(|_| clients::with_queue());
        let lost_item = wait_event(&lost, c"@lost", &[]);
        let kept_item = wait_event(&kept, c"@lost", &[]);
        let [lost_sink, kept_sink] = event_sinks()[..] else {
            panic!("two event waiters")
        };

        wait_for_client_lost(&lost);
        Lane::drive(Some(&lost));
        Lane::drive(Some(&kept));
        assert!(lost_item.upgrade().is_none());
        assert!(kept_item.upgrade().is_some());
        assert!(!tiri_core::events::events_has_sink(lost_sink));
        assert_eq!(event_sinks(), [kept_sink]);
    }

    pub fn flush_resumes_every_event_waiter() {
        let [first, second] = [(); 2].map(|_| clients::with_queue());
        let items = [
            wait_event(&first, c"@flush", &[]),
            wait_event(&second, c"@flush", &[c"-F", c"0"]),
        ];
        let sinks = event_sinks();
        assert_eq!(sinks.len(), 2);

        wait_for_flush();
        assert!(event_sinks().is_empty());
        for sink in sinks {
            assert!(!tiri_core::events::events_has_sink(sink));
        }
        Lane::drive(Some(&first));
        Lane::drive(Some(&second));
        assert!(items.iter().all(|item| item.upgrade().is_none()));
    }

    pub fn a_lost_client_waiting_for_an_event_is_freed() {
        let owner = clients::with_queue();
        let head = wait_event(&owner, c"@freed", &[]);
        let [sink] = event_sinks()[..] else {
            panic!("one event waiter")
        };
        owner.update_flags(CLIENT_DEAD as u64, 0);
        wait_for_client_lost(&owner);
        Lane::drive(Some(&owner));
        assert!(head.upgrade().is_none());
        assert!(event_sinks().is_empty());
        assert!(!tiri_core::events::events_has_sink(sink));
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_none());
    }
}
