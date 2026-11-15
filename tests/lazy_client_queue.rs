use std::cell::Cell;
use std::rc::Rc;
use tiri::actions::TaskOutcome;
use tiri::actions::queue::{callback, drive, enqueue};

#[test]
fn scripting_attaches_to_a_client_created_by_core_before_initialization() {
    let client = tiri_core::server_client::queue_test_clients::with_queue();
    tiri::initialize();
    let ran = Rc::new(Cell::new(false));
    let observed = ran.clone();
    enqueue(
        Some(&client),
        callback(
            c"existing core client",
            Some(Box::new(move |_| {
                observed.set(true);
                TaskOutcome::Normal
            })),
        ),
    );
    assert_eq!(drive(Some(&client)), 1);
    assert!(ran.get());
}
