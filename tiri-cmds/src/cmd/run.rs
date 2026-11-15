use crate::cmd::find::cmd_find_client;
use crate::cmd::invocation::{ExecOutcome, Invocation};
use crate::cmd::{cmd_get_args, cmd_get_entry, cmd_print_cstring};
use crate::shared::command::{
    CMD_AFTERHOOK, CMD_CLIENT_CANFAIL, CMD_CLIENT_CFLAG, CMD_CLIENT_TFLAG, CMDQ_STATE_CONTROL,
    cmd, cmd_list,
};
use std::ffi::CString;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use tiri_core::actions::{CmdFailed, CmdResult, Context, ProgramRef};
use tiri_core::arguments::args_get;
use tiri_core::cfg::cfg_finished;
use tiri_core::control::next_control_guard_stamp;
use tiri_core::format::bytes::{format_message_with, write_cstr};
use tiri_core::hooks::set_running_hook_context;
use tiri_core::key_string::key_string_format;
use tiri_core::log::{log_bytes, log_debug, log_get_level};
use tiri_core::reactor::LaneTarget;
use tiri_core::server::server_add_message;
use tiri_core::server_client::Client as _;
use tiri_core::shared::abi::{__uid_t, u_char, uid_t};
use tiri_core::shared::client::ClientRef;
use tiri_core::shared::key::{KEYC_NONE, key_code, key_event};
use tiri_core::tiri_sys::{user_id, user_name};

pub fn run(program: ProgramRef, mut context: Context) -> Pin<Box<dyn Future<Output = CmdResult>>> {
    let any: Rc<dyn std::any::Any> = program;
    let list = any
        .downcast::<cmd_list>()
        .unwrap_or_else(|_| panic!("command program"));
    Box::pin(async move { run_list(&list, &mut context).await })
}

pub(crate) fn spawn(owner: Option<&ClientRef>, list: Rc<cmd_list>, mut context: Context) {
    tiri_core::reactor::Lane::push(owner, async move {
        let _ = run_list(&list, &mut context).await;
    });
}

pub(crate) async fn run_list(list: &cmd_list, context: &mut Context) -> CmdResult {
    let lane = LaneTarget::for_client(context.client.upgrade().as_ref());
    if list.groups.iter().all(Vec::is_empty) {
        if lane.try_with(|_| ()).is_none() {
            return std::future::pending().await;
        }
        lane.step();
        return Ok(());
    }
    let mut last: CmdResult = Ok(());
    for group in &list.groups {
        for cmd_ref in group {
            last = run_one(cmd_ref, context, &lane).await;
            if last.is_err() {
                break;
            }
        }
    }
    last
}

fn lane_client_name(c: Option<&ClientRef>) -> CString {
    let Some(c) = c else {
        return c"<global>".to_owned();
    };
    let name = c.name();
    format_message_with(|out| {
        if let Some(name) = name.as_ref() {
            out.write_all(b"<")?;
            out.write_all(name.as_bytes())?;
            out.write_all(b">")
        } else {
            write!(out, "<{:p}>", std::ptr::from_ref(&**c))
        }
    })
}

fn add_command_message(c_owner: Option<&ClientRef>, event: Option<&key_event>, cmd_ref: &cmd) {
    let tmp = cmd_print_cstring(cmd_ref);
    if let Some(c_value) = c_owner {
        let uid: uid_t = c_value.peer_uid();
        let user: CString = if uid != (-1_i32) as uid_t && uid != user_id() {
            if let Some(name) = user_name(uid as __uid_t) {
                let name = name.to_bytes();
                let mut bytes = Vec::with_capacity(name.len() + 2);
                bytes.push(b'[');
                bytes.extend_from_slice(name);
                bytes.push(b']');
                CString::new(bytes).expect("passwd name is a C string")
            } else {
                c"[unknown]".to_owned()
            }
        } else {
            c"".to_owned()
        };
        let event_key = event.map_or(KEYC_NONE as key_code, |ev| ev.key);
        if c_value.attached_session().upgrade().is_some()
            && event_key != KEYC_NONE as ::core::ffi::c_ulong as key_code
        {
            let key = key_string_format(event_key, false);
            server_add_message(|out| {
                write_cstr(out, c_value.name().as_deref())?;
                write_cstr(out, &*user)?;
                out.write_all(b" key ")?;
                write_cstr(out, &*key)?;
                out.write_all(b": ")?;
                write_cstr(out, &*tmp)
            });
        } else {
            server_add_message(|out| {
                write_cstr(out, c_value.name().as_deref())?;
                write_cstr(out, &*user)?;
                out.write_all(b" command: ")?;
                write_cstr(out, &*tmp)
            });
        }
    } else {
        server_add_message(|out| {
            out.write_all(b"command: ")?;
            write_cstr(out, &*tmp)
        });
    }
}

struct GlobalHookClear(bool);

impl Drop for GlobalHookClear {
    fn drop(&mut self) {
        if self.0 {
            set_running_hook_context(None);
        }
    }
}

async fn run_one(cmd_ref: &cmd, context: &mut Context, lane: &LaneTarget) -> CmdResult {
    let exec = cmd_get_entry(cmd_ref)
        .exec
        .expect("non-null function pointer");
    run_with_exec(cmd_ref, context, lane, move |c, inv| exec(c, inv)).await
}

pub(crate) async fn run_with_exec<F>(
    cmd_ref: &cmd,
    context: &mut Context,
    lane: &LaneTarget,
    exec: F,
) -> CmdResult
where
    F: for<'a, 'ctx> FnOnce(
        &'a cmd,
        &'a mut Invocation<'ctx>,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>>,
{
    if lane.try_with(|_| ()).is_none() {
        return std::future::pending().await;
    }
    let saved_client = context.client.upgrade();
    let client_freed =
        saved_client.is_none() && !std::rc::Weak::ptr_eq(&context.client, &std::rc::Weak::new());
    if client_freed || saved_client.as_ref().is_some_and(|c| c.is_dead()) {
        lane.step();
        return Err(CmdFailed);
    }
    if cfg_finished() {
        add_command_message(saved_client.as_ref(), context.event.as_ref(), cmd_ref);
    }
    if log_get_level() > 1 {
        let name = lane_client_name(saved_client.as_ref());
        let tmp = cmd_print_cstring(cmd_ref);
        log_debug(format_args!(
            "cmdq_fire_command {}: {}",
            log_bytes(name.as_bytes()),
            log_bytes(tmp.as_bytes())
        ));
    }
    drop(saved_client);

    let is_global = matches!(lane, LaneTarget::Global);
    let mut clear_global = GlobalHookClear(is_global);
    let guard_stamp = next_control_guard_stamp();
    let control_flags = i32::from(context.flags & CMDQ_STATE_CONTROL != 0);
    let suspended = std::cell::Cell::new(false);
    let mut was_parked = false;
    let mut inv = Invocation::new(
        context,
        lane.clone(),
        cmd_ref,
        guard_stamp,
        control_flags,
        &suspended,
    );
    if is_global {
        set_running_hook_context(Some(inv.hook_context()));
    }
    inv.guard(c"begin");

    if inv.context().client.upgrade().is_none() {
        let lent = cmd_find_client(Some(&inv), None, 1);
        inv.lend_client(lent.as_ref().map_or_else(std::rc::Weak::new, Rc::downgrade));
        if is_global {
            set_running_hook_context(Some(inv.hook_context()));
        }
    }
    let execution_client = inv.client_weak();

    let entry = cmd_get_entry(cmd_ref);
    let args = cmd_get_args(cmd_ref);
    let quiet = i32::from(entry.flags & CMD_CLIENT_CANFAIL != 0);
    let tc_result = if entry.flags & CMD_CLIENT_CFLAG != 0 {
        let tc = cmd_find_client(Some(&inv), args_get(args, b'c' as u_char), quiet);
        if tc.is_none() && quiet == 0 {
            Err(CmdFailed)
        } else {
            Ok(tc)
        }
    } else if entry.flags & CMD_CLIENT_TFLAG != 0 {
        let tc = cmd_find_client(Some(&inv), args_get(args, b't' as u_char), quiet);
        if tc.is_none() && quiet == 0 {
            Err(CmdFailed)
        } else {
            Ok(tc)
        }
    } else {
        Ok(cmd_find_client(Some(&inv), None, 1))
    };

    let outcome = match tc_result {
        Err(CmdFailed) => ExecOutcome::err(),
        Ok(tc) => {
            inv.set_target_client(tc.as_ref().map_or_else(std::rc::Weak::new, Rc::downgrade));
            drop(tc);
            if inv.resolve_source(cmd_ref, &entry.source).is_err()
                || inv.resolve_target(cmd_ref, &entry.target).is_err()
            {
                ExecOutcome::err()
            } else {
                let mut exec_fut = exec(cmd_ref, &mut inv);
                std::future::poll_fn(|cx| {
                    let res = exec_fut.as_mut().poll(cx);
                    if lane.try_with(|_| ()).is_none() {
                        return Poll::Pending;
                    }
                    if res.is_pending() && suspended.get() {
                        was_parked = true;
                    }
                    res
                })
                .await
            }
        }
    };

    let tail_client = inv.client_weak();

    if !inv.is_suspended() {
        if outcome.result.is_ok() && entry.flags & CMD_AFTERHOOK != 0 {
            let exec_cl = execution_client.upgrade();
            if let Some(fs) = inv.resolve_hook_target(exec_cl.as_ref()) {
                inv.insert_hook(Some(&fs), cmd_ref, |out| {
                    out.write_all(b"after-")?;
                    write_cstr(out, entry.name)
                });
            }
        }
        inv.unlend_client();
        if outcome.result.is_err() {
            let fallback = inv.context().client.upgrade();
            let fs = inv.resolve_hook_target(fallback.as_ref());
            inv.insert_hook(fs.as_ref(), cmd_ref, |out| out.write_all(b"command-error"));
            inv.guard(c"error");
        } else {
            inv.guard(c"end");
        }
        if lane.try_with(|_| ()).is_none() {
            return std::future::pending().await;
        }
        let hooks = inv.take_hooks();
        drop(inv);
        if is_global {
            clear_global.0 = false;
            set_running_hook_context(None);
        }
        lane.step();
        for (program, hook_ctx) in hooks {
            let _ = run(program, hook_ctx).await;
        }
        if let Some(tail) = outcome.tail {
            let prev = std::mem::replace(&mut context.client, tail_client);
            tail(context).await;
            context.client = prev;
        }
        return outcome.result;
    }

    let hooks = inv.take_hooks();
    let resumed_hook_ctx = inv.hook_context();
    drop(inv);

    if was_parked {
        if is_global {
            set_running_hook_context(Some(resumed_hook_ctx));
        }
        lane.yield_to_drive().await;
    }
    if lane.try_with(|_| ()).is_none() {
        return std::future::pending().await;
    }
    if is_global {
        clear_global.0 = false;
        set_running_hook_context(None);
    }
    lane.step();
    if was_parked {
        if let Some(tail) = outcome.tail {
            let prev = std::mem::replace(&mut context.client, tail_client);
            tail(context).await;
            context.client = prev;
        }
        for (program, hook_ctx) in hooks {
            let _ = run(program, hook_ctx).await;
        }
    } else {
        for (program, hook_ctx) in hooks {
            let _ = run(program, hook_ctx).await;
        }
        if let Some(tail) = outcome.tail {
            let prev = std::mem::replace(&mut context.client, tail_client);
            tail(context).await;
            context.client = prev;
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod test_harness {
    use super::{ExecOutcome, Future, Invocation, Pin, Rc, run_with_exec};
    use crate::shared::command::{CMD_FIND_PANE, cmd, cmd_entry, cmd_entry_flag};
    use tiri_core::actions::Context;
    use tiri_core::arguments::{args_create, args_parse};
    use tiri_core::reactor::{Lane, LaneTarget};
    use tiri_core::shared::client::ClientRef;

    fn dummy_exec<'a, 'ctx>(
        _cmd: &'a cmd,
        _inv: &'a mut Invocation<'ctx>,
    ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>> {
        Box::pin(async { ExecOutcome::ok() })
    }

    static TEST_ENTRY: cmd_entry = cmd_entry {
        name: c"test-cmd",
        alias: None,
        args: args_parse {
            template: c"",
            lower: 0,
            upper: 0,
            cb: None,
        },
        usage: c"",
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
        flags: 0,
        exec: Some(dummy_exec),
    };

    pub(crate) fn dummy_cmd() -> cmd {
        cmd {
            entry: &TEST_ENTRY,
            args: args_create(),
            file: None,
            line: 0,
            parse_flags: 0,
        }
    }

    pub(crate) fn spawn_invocation<F>(owner: Option<&ClientRef>, exec: F) -> std::rc::Weak<()>
    where
        F: for<'a, 'ctx> FnOnce(
                &'a cmd,
                &'a mut Invocation<'ctx>,
            ) -> Pin<Box<dyn Future<Output = ExecOutcome> + 'a>>
            + 'static,
    {
        let head_guard = Rc::new(());
        let head = Rc::downgrade(&head_guard);
        let client = owner.map_or_else(std::rc::Weak::new, Rc::downgrade);
        let lane = LaneTarget::for_client(owner);
        let cmd_val = dummy_cmd();
        let mut ctx = Context::new(client, None, None, 0);
        Lane::push(owner, async move {
            let _head = head_guard;
            let _ = run_with_exec(&cmd_val, &mut ctx, &lane, exec).await;
        });
        head
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::test_harness::spawn_invocation;
    use crate::cmd::invocation::ExecOutcome;
    use std::cell::{Cell, RefCell};
    use std::rc::{Rc, Weak};
    use tiri_core::hooks::running_hook_context;
    use tiri_core::reactor::Lane;

    pub(super) struct SetOnDrop(pub(super) Rc<Cell<bool>>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    pub(super) fn append_recording(
        trace: &Rc<RefCell<Vec<&'static str>>>,
        tag: &'static str,
    ) -> Weak<()> {
        let trace = trace.clone();
        spawn_invocation(None, move |_, _| {
            Box::pin(async move {
                trace.borrow_mut().push(tag);
                ExecOutcome::ok()
            })
        })
    }

    #[test]
    fn running_global_invocation_is_visible() {
        let ran = Rc::new(Cell::new(false));
        let observed = ran.clone();
        let item = spawn_invocation(None, move |_, _| {
            Box::pin(async move {
                assert!(running_hook_context().is_some());
                observed.set(true);
                ExecOutcome::ok()
            })
        });
        assert_eq!(Lane::drive(None), 1);
        assert!(ran.get());
        assert!(item.upgrade().is_none());
        assert!(running_hook_context().is_none());
    }

    #[test]
    fn reentrant_next_during_exec_returns_without_removing() {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let reentered = Rc::new(Cell::new(None));
        let observed = reentered.clone();
        let inner_trace = trace.clone();
        let head = spawn_invocation(None, move |_, _| {
            Box::pin(async move {
                observed.set(Some(Lane::drive(None)));
                assert!(inner_trace.borrow().is_empty());
                assert!(running_hook_context().is_some());
                ExecOutcome::ok()
            })
        });
        let next = append_recording(&trace, "next");
        assert_eq!(Lane::drive(None), 2);
        assert_eq!(reentered.get(), Some(0));
        assert!(head.upgrade().is_none());
        assert!(next.upgrade().is_none());
        assert_eq!(*trace.borrow(), ["next"]);
    }
}

#[cfg(test)]
mod suspend_tests {
    use super::lifecycle_tests::{SetOnDrop, append_recording};
    use super::test_harness::spawn_invocation;
    use crate::cmd::invocation::ExecOutcome;
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::rc::{Rc, Weak};
    use tiri_core::hooks::running_hook_context;
    use tiri_core::reactor::Lane;
    use tiri_core::tiri_rt::oneshot::channel;

    type Trace = Rc<RefCell<Vec<&'static str>>>;

    fn append_suspended(future: impl Future<Output = ()> + 'static) -> Weak<()> {
        spawn_invocation(None, move |cmd_ref, inv| {
            Box::pin(async move {
                inv.suspend(cmd_ref);
                future.await;
                ExecOutcome::ok()
            })
        })
    }

    #[test]
    fn send_runs_the_body_and_defers_following_commands_to_dispatch() {
        let trace: Trace = Default::default();
        let (tx, rx) = channel::<()>();
        let body = trace.clone();
        let head = append_suspended(async move {
            assert_eq!(rx.await, Some(()));
            body.borrow_mut().push("body");
        });
        let next = append_recording(&trace, "next");
        assert_eq!(Lane::drive(None), 0, "the first poll is pending");
        assert_eq!(Lane::drive(None), 0, "a suspended head blocks its queue");
        assert!(trace.borrow().is_empty());
        tx.send(()).unwrap();
        assert_eq!(*trace.borrow(), ["body"]);
        assert!(head.upgrade().is_some());
        assert!(running_hook_context().is_some());
        assert_eq!(Lane::drive(None), 2);
        assert!(head.upgrade().is_none());
        assert!(next.upgrade().is_none());
        assert_eq!(*trace.borrow(), ["body", "next"]);
    }

    #[test]
    fn send_during_exec_completes_in_the_same_pass() {
        let trace: Trace = Default::default();
        let body = trace.clone();
        let head = spawn_invocation(None, move |cmd_ref, inv| {
            Box::pin(async move {
                let (tx, rx) = channel::<()>();
                tx.send(()).unwrap();
                assert!(body.borrow().is_empty());
                inv.suspend(cmd_ref);
                assert_eq!(rx.await, Some(()));
                body.borrow_mut().push("body");
                ExecOutcome::ok()
            })
        });
        let next = append_recording(&trace, "next");
        assert_eq!(Lane::drive(None), 2);
        assert!(head.upgrade().is_none());
        assert!(next.upgrade().is_none());
        assert_eq!(*trace.borrow(), ["body", "next"]);
    }

    #[test]
    fn running_hides_the_suspended_body_and_exposes_the_resumed_item() {
        let (tx, rx) = channel::<()>();
        let ran = Rc::new(Cell::new(false));
        let body = ran.clone();
        let head = spawn_invocation(None, move |cmd_ref, inv| {
            Box::pin(async move {
                inv.suspend(cmd_ref);
                assert!(running_hook_context().is_none(), "first poll");
                assert_eq!(rx.await, Some(()));
                assert!(running_hook_context().is_none(), "body");
                let _ = inv.flags();
                body.set(true);
                ExecOutcome::ok()
            })
        });
        assert_eq!(Lane::drive(None), 0);
        assert!(running_hook_context().is_none());
        tx.send(()).unwrap();
        assert!(ran.get());
        assert!(head.upgrade().is_some());
        assert!(running_hook_context().is_some());
        assert_eq!(Lane::drive(None), 1);
        assert!(head.upgrade().is_none());
        assert!(running_hook_context().is_none());
    }

    #[test]
    fn reentrant_next_from_inside_a_body_returns_zero() {
        let trace: Trace = Default::default();
        let reentered = Rc::new(RefCell::new(Vec::new()));
        let observed = reentered.clone();
        let (tx, rx) = channel::<()>();
        let head = append_suspended(async move {
            let items = Lane::drive(None);
            observed.borrow_mut().push(items);
            assert_eq!(rx.await, Some(()));
            let items = Lane::drive(None);
            observed.borrow_mut().push(items);
        });
        let next = append_recording(&trace, "next");
        assert_eq!(Lane::drive(None), 0);
        tx.send(()).unwrap();
        assert_eq!(*reentered.borrow(), [0, 0]);
        assert!(trace.borrow().is_empty());
        assert!(head.upgrade().is_some());
        assert_eq!(Lane::drive(None), 2);
        assert!(head.upgrade().is_none());
        assert!(next.upgrade().is_none());
        assert_eq!(*trace.borrow(), ["next"]);
    }

    #[test]
    fn send_after_runtime_shutdown_runs_the_body_and_leaves_dispatch_pending() {
        use tiri_core::tiri_rt::Runtime as _;
        let runtime = tiri_core::tiri_rt::mio::Runtime::new().unwrap();
        {
            let trace: Trace = Default::default();
            let (tx, rx) = channel::<()>();
            let body = trace.clone();
            let released = Rc::new(Cell::new(false));
            let guard = SetOnDrop(released.clone());
            let head = append_suspended(async move {
                let _guard = guard;
                assert_eq!(rx.await, Some(()));
                body.borrow_mut().push("body");
            });
            let next = append_recording(&trace, "next");
            assert_eq!(Lane::drive(None), 0);
            tiri_core::reactor::shutdown_runtime(runtime);
            tx.send(()).unwrap();
            assert_eq!(*trace.borrow(), ["body"]);
            assert!(released.get());
            assert_eq!(Lane::drive(None), 2);
            assert!(head.upgrade().is_none());
            assert!(next.upgrade().is_none());
            assert_eq!(*trace.borrow(), ["body", "next"]);
        }
    }
}

#[cfg(feature = "test-support")]
pub mod client_queue_tests {
    use super::run_with_exec;
    use super::test_harness::{dummy_cmd, spawn_invocation};
    use crate::cmd::invocation::ExecOutcome;
    use std::cell::RefCell;
    use std::rc::{Rc, Weak};
    use tiri_core::reactor::{Lane, LaneTarget};
    use tiri_core::server_client::Client as _;
    use tiri_core::server_client::queue_test_clients as clients;
    use tiri_core::server_client::queue_test_clients::{attach, return_value};
    use tiri_core::session::session_new_for_test;
    use tiri_core::shared::client::{ClientRef, ClientWeak};

    struct TestWaiter {
        client: ClientWeak,
        done: tiri_core::tiri_rt::oneshot::Sender<Option<i32>>,
    }

    impl TestWaiter {
        fn pair(client: ClientWeak) -> (Self, impl std::future::Future<Output = Option<i32>>) {
            let (done, rx) = tiri_core::tiri_rt::oneshot::channel();
            let fut = async move {
                match rx.await {
                    Some(status) => status,
                    None => std::future::pending().await,
                }
            };
            (Self { client, done }, fut)
        }

        fn client(&self) -> Option<ClientRef> {
            self.client.upgrade()
        }

        fn is_cancelled(&self) -> bool {
            self.done.is_closed()
        }

        fn resume(self) {
            let _ = self.done.send(None);
        }

        fn finish(self, status: i32) {
            let _ = self.done.send(Some(status));
        }
    }

    type Slot = Rc<RefCell<Option<TestWaiter>>>;

    fn waiting(owner: Option<&ClientRef>, slot: &Slot) -> Weak<()> {
        let slot = slot.clone();
        let head_guard = Rc::new(());
        let head = Rc::downgrade(&head_guard);
        let client_weak = owner.map_or_else(Weak::new, Rc::downgrade);
        let lane = LaneTarget::for_client(owner);
        Lane::push(owner, async move {
            let _head = head_guard;
            let (waiter, fut) = TestWaiter::pair(client_weak.clone());
            *slot.borrow_mut() = Some(waiter);
            if let Some(status) = fut.await {
                if let Some(c) = client_weak.upgrade() {
                    if c.attached_session().upgrade().is_none() {
                        c.set_return_value(status);
                    }
                }
            }
            lane.yield_to_drive().await;
            lane.step();
        });
        head
    }

    pub fn a_wake_from_another_queues_dispatch_runs_the_body_before_that_exec_returns() {
        let trace: Rc<RefCell<Vec<&'static str>>> = Default::default();
        let (tx, rx) = tiri_core::tiri_rt::oneshot::channel::<()>();
        let body = trace.clone();
        let head = spawn_invocation(None, move |cmd_ref, inv| {
            Box::pin(async move {
                inv.suspend(cmd_ref);
                assert_eq!(rx.await, Some(()));
                body.borrow_mut().push("body");
                ExecOutcome::ok()
            })
        });
        let next_trace = trace.clone();
        let next = spawn_invocation(None, move |_, _| {
            Box::pin(async move {
                next_trace.borrow_mut().push("next");
                ExecOutcome::ok()
            })
        });
        assert_eq!(Lane::drive(None), 0);
        let owner = clients::with_queue();
        let exec = trace.clone();
        let sender = spawn_invocation(Some(&owner), move |_, _| {
            Box::pin(async move {
                tx.send(()).unwrap();
                exec.borrow_mut().push("sent");
                ExecOutcome::ok()
            })
        });
        assert_eq!(Lane::drive(Some(&owner)), 1);
        assert!(sender.upgrade().is_none());
        assert_eq!(*trace.borrow(), ["body", "sent"]);
        assert!(head.upgrade().is_some());
        assert_eq!(Lane::drive(None), 2);
        assert!(head.upgrade().is_none());
        assert!(next.upgrade().is_none());
        assert_eq!(*trace.borrow(), ["body", "sent", "next"]);
    }

    pub fn a_clientless_item_records_no_client_while_exec_sees_the_lent_one() {
        let lent = clients::with_queue();
        let slot: Slot = Default::default();
        let (stored, substitute) = (slot.clone(), lent.clone());
        let head = spawn_invocation(None, move |cmd_ref, inv| {
            Box::pin(async move {
                inv.lend_client(Rc::downgrade(&substitute));
                let c = inv.client();
                assert!(c.is_some_and(|c| Rc::ptr_eq(&c, &substitute)));
                let (waiter, fut) = TestWaiter::pair(inv.owning_client());
                *stored.borrow_mut() = Some(waiter);
                inv.suspend(cmd_ref);
                if let Some(status) = fut.await {
                    if let Some(c) = inv.client() {
                        if c.attached_session().upgrade().is_none() {
                            c.set_return_value(status);
                        }
                    }
                }
                ExecOutcome::ok()
            })
        });
        assert_eq!(Lane::drive(None), 0);
        let waiter = slot.take().unwrap();
        assert!(waiter.client().is_none());
        waiter.finish(5);
        assert_eq!(return_value(&lent), 0);
        assert_eq!(Lane::drive(None), 1);
        assert!(head.upgrade().is_none());
    }

    pub fn a_client_queue_item_records_its_client() {
        let owner = clients::with_queue();
        let slot: Slot = Default::default();
        let head = waiting(Some(&owner), &slot);
        assert_eq!(Lane::drive(Some(&owner)), 0);
        let waiter = slot.take().unwrap();
        assert!(waiter.client().is_some_and(|c| Rc::ptr_eq(&c, &owner)));
        waiter.resume();
        assert_eq!(Lane::drive(Some(&owner)), 1);
        assert!(head.upgrade().is_none());
    }

    pub fn a_deferred_item_from_a_lent_item_records_the_lent_client() {
        let lent = clients::with_queue();
        let slot: Slot = Default::default();
        let (inserted, substitute) = (slot.clone(), lent.clone());
        let head = spawn_invocation(None, move |_, inv| {
            Box::pin(async move {
                inv.lend_client(Rc::downgrade(&substitute));
                ExecOutcome::tail(move |ctx| {
                    Box::pin(async move {
                        let cmd_val = dummy_cmd();
                        let lane = LaneTarget::Global;
                        let _ = run_with_exec(&cmd_val, ctx, &lane, move |c, child_inv| {
                            Box::pin(async move {
                                let (waiter, fut) = TestWaiter::pair(child_inv.owning_client());
                                *inserted.borrow_mut() = Some(waiter);
                                child_inv.suspend(c);
                                if let Some(status) = fut.await {
                                    if let Some(cl) = child_inv.client() {
                                        if cl.attached_session().upgrade().is_none() {
                                            cl.set_return_value(status);
                                        }
                                    }
                                }
                                ExecOutcome::ok()
                            })
                        })
                        .await;
                    })
                })
            })
        });
        assert_eq!(Lane::drive(None), 1);
        assert!(head.upgrade().is_some());
        let waiter = slot.take().unwrap();
        assert!(waiter.client().is_some_and(|c| Rc::ptr_eq(&c, &lent)));
        waiter.finish(3);
        assert_eq!(return_value(&lent), 3);
        assert_eq!(Lane::drive(None), 1);
        assert!(head.upgrade().is_none());
    }

    pub fn finish_reports_the_status_only_to_a_client_sessionless_at_finish() {
        let owner = clients::with_queue();
        let session = session_new_for_test();
        attach(&owner, Some(&session));
        let (first, second): (Slot, Slot) = Default::default();
        let first_item = waiting(Some(&owner), &first);
        let second_item = waiting(Some(&owner), &second);
        assert_eq!(Lane::drive(Some(&owner)), 0);
        attach(&owner, None);
        first.take().unwrap().finish(7);
        assert_eq!(return_value(&owner), 7);
        assert_eq!(Lane::drive(Some(&owner)), 1);
        assert!(first_item.upgrade().is_none());
        attach(&owner, Some(&session));
        second.take().unwrap().finish(9);
        assert_eq!(return_value(&owner), 7);
        assert_eq!(Lane::drive(Some(&owner)), 1);
        assert!(second_item.upgrade().is_none());
        attach(&owner, None);
    }

    pub fn a_waiter_is_cancelled_once_its_item_drops() {
        let owner = clients::with_queue();
        let slot: Slot = Default::default();
        let head = waiting(Some(&owner), &slot);
        assert_eq!(Lane::drive(Some(&owner)), 0);
        let waiter = slot.take().unwrap();
        assert!(!waiter.is_cancelled());
        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(
            client.upgrade().is_none(),
            "a waiter does not retain its client"
        );
        assert!(head.upgrade().is_none());
        assert!(waiter.is_cancelled());
        assert!(waiter.client().is_none());
        waiter.resume();
    }
}
