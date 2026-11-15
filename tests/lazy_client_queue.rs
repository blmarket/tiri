use std::cell::Cell;
use std::rc::Rc;
use tiri::actions::sources::compile;
use tiri::actions::{Context, run};
use tiri::reactor::Lane;

#[test]
fn scripting_attaches_to_a_client_created_by_core_before_initialization() {
    let client = tiri_core::server_client::queue_test_clients::with_queue();
    tiri::initialize();
    let program = compile(c"wait-for -S lazy-client").expect("parse");
    let context = Context::new(Rc::downgrade(&client), None, None, 0);
    let ran = Rc::new(Cell::new(false));
    let observed = ran.clone();
    Lane::push(Some(&client), async move {
        assert!(run(program, context).await.is_ok());
        observed.set(true);
    });
    assert_eq!(Lane::drive(Some(&client)), 1);
    assert!(ran.get());
}
