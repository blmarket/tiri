use super::{CLIENT_DEAD, Rc, Weak, client_file, file_read_wait, file_wait, file_write_wait, uint64_t};
use crate::reactor::Lane;
use crate::reactor::shutdown_runtime;
use crate::server_client::Client as _;
use std::cell::Cell;

pub fn local_completion_can_be_cancelled_after_runtime_shutdown() {
    let runtime = tiri_rt::Runtime::new().unwrap();
    {
        let data = Rc::new(());
        let retained = data.clone();
        let file = Rc::new(Cell::new(Weak::new()));
        let observed = file.clone();
        let owner = crate::server_client::queue_test_clients::with_queue();
        let head_guard = Rc::new(());
        let head = Rc::downgrade(&head_guard);
        Lane::push(Some(&owner), async move {
            let _head = head_guard;
            let wait = file_read_wait(None, c"/dev/null", move |file| {
                observed.set(file);
                Some(Box::new(move |_| {
                    let _ = &retained;
                    panic!("shutdown must not deliver progress");
                }))
            });
            let _ = wait.await;
            panic!("shutdown must not deliver completion");
        });
        assert_eq!(Lane::drive(Some(&owner)), 0, "local completion must be deferred");
        let file = file.take();
        assert!(file.upgrade().is_some());
        shutdown_runtime(runtime);
        drop(owner);
        assert!(head.upgrade().is_none());
        assert!(file.upgrade().is_none());
        assert_eq!(Rc::strong_count(&data), 1);
    }
}

pub fn dropping_a_lost_client_cancels_its_local_write() {
    let mut runtime = tiri_rt::Runtime::new().unwrap();
    {
        let owner = crate::server_client::queue_test_clients::with_queue();
        let observed = Rc::new(Cell::new(false));
        let body = observed.clone();
        let file = Rc::new(Cell::new(Weak::new()));
        let observed_file = file.clone();
        let head_guard = Rc::new(());
        let head = Rc::downgrade(&head_guard);
        Lane::push(Some(&owner), async move {
            let _head = head_guard;
            let wait = file_write_wait(None, c"/dev/null", 0, b"data");
            observed_file.set(wait.file.clone());
            let _ = wait.await;
            body.set(true);
        });
        assert_eq!(Lane::drive(Some(&owner)), 0);
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

pub fn a_dead_file_client_leaves_its_command_blocked() {
    let mut runtime = tiri_rt::Runtime::new().unwrap();
    {
        let owner = crate::server_client::queue_test_clients::with_queue();
        let client = crate::server_client::queue_test_clients::with_queue();
        client.update_flags(CLIENT_DEAD as uint64_t, 0);
        let file_client = client.clone();
        let observed = Rc::new(Cell::new(false));
        let body = observed.clone();
        let head_guard = Rc::new(());
        let head = Rc::downgrade(&head_guard);
        Lane::push(Some(&owner), async move {
            let _head = head_guard;
            let file = client_file::new();
            let (waiter, done) = tiri_rt::oneshot::channel();
            {
                let mut cf = file.borrow_mut();
                cf.c = Some(file_client);
                cf.wait = Some(waiter);
                cf.completed = true;
            }
            let wait = file_wait(file, done);
            if wait.await.is_none() {
                return std::future::pending().await;
            }
            body.set(true);
        });
        assert_eq!(Lane::drive(Some(&owner)), 0);
        runtime.poll(Some(std::time::Duration::ZERO)).unwrap();
        assert!(!observed.get());
        assert_eq!(Rc::strong_count(&client), 1, "the file released its client");
        assert_eq!(
            Lane::drive(Some(&owner)),
            0,
            "the parked command blocks its queue"
        );
        assert!(head.upgrade().is_some());
        drop(owner);
        assert!(head.upgrade().is_none());
    }
    shutdown_runtime(runtime);
}
