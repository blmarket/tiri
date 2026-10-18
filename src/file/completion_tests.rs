use super::*;
use crate::src::cmd::queue::{cmdq_append, cmdq_get_callback_owned, cmdq_next};
use crate::src::reactor::shutdown_runtime;
use crate::src::shared::command::CmdOutcome;
use std::cell::Cell;

#[test]
fn local_completion_can_be_cancelled_after_runtime_shutdown() {
    use tiri_rt::Runtime as _;
    let runtime = tiri_rt::mio::Runtime::new().unwrap();
    unsafe {
        let data = Rc::new(());
        let retained = data.clone();
        let file = Rc::new(Cell::new(Weak::new()));
        let observed = file.clone();
        let owner = crate::src::server_client::queue_test_clients::with_queue();
        let head = cmdq_append(
            Some(&owner),
            cmdq_get_callback_owned(
                c"local read",
                Some(Box::new(move |_| {
                    let wait = file_read_wait(None, c"/dev/null", move |file| {
                        observed.set(file);
                        Some(Box::new(move |_| {
                            let _ = &retained;
                            panic!("shutdown must not deliver progress");
                        }))
                    });
                    CmdOutcome::Suspend(Box::pin(async move {
                        let _ = wait.await;
                        panic!("shutdown must not deliver completion");
                    }))
                })),
            ),
        );
        assert_eq!(
            cmdq_next(Some(&owner)),
            0,
            "local completion must be deferred"
        );
        let file = file.take();
        assert!(file.upgrade().is_some());
        shutdown_runtime(runtime);
        drop(owner);
        assert!(head.upgrade().is_none());
        assert!(file.upgrade().is_none());
        assert_eq!(Rc::strong_count(&data), 1);
    }
}

#[test]
fn dropping_a_lost_client_cancels_its_local_write() {
    use tiri_rt::Runtime as _;
    let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
    unsafe {
        let owner = crate::src::server_client::queue_test_clients::with_queue();
        let observed = Rc::new(Cell::new(false));
        let body = observed.clone();
        let file = Rc::new(Cell::new(Weak::new()));
        let observed_file = file.clone();
        let head = cmdq_append(
            Some(&owner),
            cmdq_get_callback_owned(
                c"local write",
                Some(Box::new(move |_| {
                    let wait = file_write_wait(None, c"/dev/null", 0, b"data");
                    observed_file.set(wait.file.clone());
                    CmdOutcome::Suspend(Box::pin(async move {
                        let _ = wait.await;
                        body.set(true);
                    }))
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(&owner)), 0);
        let file = file.take();
        assert!(
            file.upgrade().is_some(),
            "the completion task retains the file"
        );

        let client = Rc::downgrade(&owner);
        drop(owner);
        assert!(client.upgrade().is_none());
        assert!(head.upgrade().is_none());
        assert!(file.upgrade().is_none());
        runtime.poll(Some(std::time::Duration::ZERO)).unwrap();
        assert!(!observed.get(), "the body does not run");
    }
    shutdown_runtime(runtime);
}

#[test]
fn a_dead_file_client_leaves_its_command_blocked() {
    use tiri_rt::Runtime as _;
    let mut runtime = tiri_rt::mio::Runtime::new().unwrap();
    unsafe {
        let owner = crate::src::server_client::queue_test_clients::with_queue();
        let client = crate::src::server_client::queue_test_clients::with_queue();
        client.update_flags(CLIENT_DEAD as uint64_t, 0);
        let file_client = client.clone();
        let observed = Rc::new(Cell::new(false));
        let body = observed.clone();
        let head = cmdq_append(
            Some(&owner),
            cmdq_get_callback_owned(
                c"dead file client",
                Some(Box::new(move |_| {
                    let file = client_file::new();
                    let (waiter, done) = tiri_rt::oneshot::channel();
                    (*file.get()).c = Some(file_client);
                    (*file.get()).wait = Some(waiter);
                    (*file.get()).completed = true;
                    let wait = file_wait(file, done);
                    CmdOutcome::Suspend(Box::pin(async move {
                        if wait.await.is_none() {
                            return std::future::pending().await;
                        }
                        body.set(true);
                    }))
                })),
            ),
        );
        assert_eq!(cmdq_next(Some(&owner)), 0);
        runtime.poll(Some(std::time::Duration::ZERO)).unwrap();
        assert!(!observed.get());
        assert_eq!(Rc::strong_count(&client), 1, "the file released its client");
        assert_eq!(
            cmdq_next(Some(&owner)),
            0,
            "the parked command blocks its queue"
        );
        assert!(head.upgrade().is_some());
        drop(owner);
        assert!(head.upgrade().is_none());
    }
    shutdown_runtime(runtime);
}
