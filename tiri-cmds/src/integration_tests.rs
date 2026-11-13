#[test]
fn process_exit_reports_its_status_to_a_sessionless_waiter_and_resumes_it() {
    crate::initialize();
    tiri_core::window_pane::wait_close_tests::process_exit_reports_its_status_to_a_sessionless_waiter_and_resumes_it();
}

#[test]
fn destroying_a_waited_pane_before_its_exit_reports_sighup() {
    crate::initialize();
    tiri_core::window_pane::wait_close_tests::destroying_a_waited_pane_before_its_exit_reports_sighup();
}

#[test]
fn a_cancelled_waiter_keeps_an_exited_pane_until_its_status_is_ready() {
    crate::initialize();
    tiri_core::window_pane::wait_close_tests::a_cancelled_waiter_keeps_an_exited_pane_until_its_status_is_ready();
}

#[test]
fn input_ending_with_its_pane_resumes_the_command() {
    crate::initialize();
    tiri_core::window_pane::input_wait_tests::input_ending_with_its_pane_resumes_the_command();
}

#[test]
fn input_ending_after_its_pane_is_gone_exits_the_client_and_parks() {
    crate::initialize();
    tiri_core::window_pane::input_wait_tests::input_ending_after_its_pane_is_gone_exits_the_client_and_parks();
}

#[test]
fn a_dead_file_client_parks_without_exiting() {
    crate::initialize();
    tiri_core::window_pane::input_wait_tests::a_dead_file_client_parks_without_exiting();
}

#[test]
fn dropping_a_lost_client_drops_its_suspended_future() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::dropping_a_lost_client_drops_its_suspended_future();
}

#[test]
fn a_completed_future_may_release_its_own_client() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::a_completed_future_may_release_its_own_client();
}

#[test]
fn a_body_may_release_its_own_client_while_polled() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::a_body_may_release_its_own_client_while_polled();
}

#[test]
fn dropping_a_lost_client_cancels_a_file_backed_wait() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::dropping_a_lost_client_cancels_a_file_backed_wait();
}

#[test]
fn a_client_retired_inside_a_callback_is_freed_from_dispatch() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::a_client_retired_inside_a_callback_is_freed_from_dispatch();
}

#[test]
fn retiring_a_lost_client_cancels_its_suspended_item_from_dispatch() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::retiring_a_lost_client_cancels_its_suspended_item_from_dispatch();
}

#[test]
fn runtime_shutdown_releases_a_pending_retirement() {
    crate::initialize();
    tiri_core::server_client::lost_queue_tests::runtime_shutdown_releases_a_pending_retirement();
}

#[test]
fn local_completion_can_be_cancelled_after_runtime_shutdown() {
    crate::initialize();
    tiri_core::file::completion_tests::local_completion_can_be_cancelled_after_runtime_shutdown();
}

#[test]
fn dropping_a_lost_client_cancels_its_local_write() {
    crate::initialize();
    tiri_core::file::completion_tests::dropping_a_lost_client_cancels_its_local_write();
}

#[test]
fn a_dead_file_client_leaves_its_command_blocked() {
    crate::initialize();
    tiri_core::file::completion_tests::a_dead_file_client_leaves_its_command_blocked();
}
#[test]
fn scripting_attaches_to_a_client_created_by_core_before_initialization() {
    use crate::cmd::queue::{cmdq_append, cmdq_get_callback_owned, cmdq_next};
    use std::cell::Cell;
    use std::rc::Rc;
    use tiri_core::actions::TaskOutcome as CmdOutcome;

    let client = tiri_core::server_client::queue_test_clients::with_queue();
    crate::initialize();
    let ran = Rc::new(Cell::new(false));
    let observed = ran.clone();
    cmdq_append(
        Some(&client),
        cmdq_get_callback_owned(
            c"existing core client",
            Some(Box::new(move |_| {
                observed.set(true);
                CmdOutcome::Normal
            })),
        ),
    );
    assert_eq!(cmdq_next(Some(&client)), 1);
    assert!(ran.get());
}
