use super::*;
use crate::src::cmd::queue::{
    cmdq_append, cmdq_continue, cmdq_get_callback_owned, cmdq_is_waiting, cmdq_next,
};
use crate::src::reactor::shutdown_runtime;
use crate::src::shared::command::CMD_RETURN_WAIT;
use std::cell::Cell;

/// Run a real local read from a queue item, retaining only observers afterward.
unsafe fn start_read(
    path: &'static CStr,
    callback: impl FnOnce(Weak<UnsafeCell<cmdq_item>>) -> client_file_cb + 'static,
    cancel: Option<Box<dyn FnOnce()>>,
) -> (Weak<UnsafeCell<cmdq_item>>, Weak<UnsafeCell<client_file>>) {
    let file = Rc::new(Cell::new(Weak::new()));
    let observed = file.clone();
    let item = cmdq_get_callback_owned(
        c"local read",
        Some(Box::new(move |item| {
            let callback = callback(Rc::downgrade(item));
            file_read_with_cmdq_wait_init(
                None,
                &*path,
                move |file| {
                    observed.set(file);
                    callback
                },
                item,
                cancel,
            );
            CMD_RETURN_WAIT
        })),
    );
    let item = cmdq_append(None, item);
    assert_eq!(cmdq_next(None), 0, "local completion must be deferred");
    assert!(cmdq_is_waiting(&*item.upgrade().unwrap().get()));
    (item, file.take())
}

#[test]
fn local_completion_can_be_cancelled_after_runtime_shutdown() {
    use tiri_rt::Runtime as _;
    let runtime = tiri_rt::mio::Runtime::new().unwrap();
    unsafe {
        let data = Rc::new(());
        let retained = data.clone();
        let cancelled = Rc::new(Cell::new(0));
        let observed = cancelled.clone();
        let (item, file) = start_read(
            c"/dev/null",
            move |_| {
                Some(Box::new(move |_| {
                    let _ = &retained;
                    panic!("shutdown must not deliver completion");
                }))
            },
            Some(Box::new(move || observed.set(observed.get() + 1))),
        );
        shutdown_runtime(runtime);
        // Runtime shutdown does not own command cleanup. The command's file
        // wait remains cancellable after its runtime stops.
        let owner = file.upgrade().unwrap();
        file_cancel_cmdq_wait(&owner);
        drop(owner);
        assert!(file.upgrade().is_none());
        assert_eq!(Rc::strong_count(&data), 1);
        assert_eq!(cancelled.get(), 1);
        cmdq_continue(&item.upgrade().unwrap());
        assert_eq!(cmdq_next(None), 0);
        assert!(item.upgrade().is_none());
    }
}
