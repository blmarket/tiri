#[test]
fn repeated_initialization_preserves_the_scripting_catalog() {
    tiri::initialize();
    let names = tiri_core::actions::command_names();
    assert_eq!(names.len(), 89);
    tiri::initialize();
    assert_eq!(names, tiri_core::actions::command_names());
}

#[test]
fn process_exit_reports_its_status_to_a_sessionless_waiter_and_resumes_it() {
    tiri::initialize();
    tiri_core::window_pane::wait_close_tests::process_exit_reports_its_status_to_a_sessionless_waiter_and_resumes_it();
}

#[test]
fn destroying_a_waited_pane_before_its_exit_reports_sighup() {
    tiri::initialize();
    tiri_core::window_pane::wait_close_tests::destroying_a_waited_pane_before_its_exit_reports_sighup();
}

#[test]
fn a_cancelled_waiter_keeps_an_exited_pane_until_its_status_is_ready() {
    tiri::initialize();
    tiri_core::window_pane::wait_close_tests::a_cancelled_waiter_keeps_an_exited_pane_until_its_status_is_ready();
}

#[test]
fn input_ending_with_its_pane_resumes_the_command() {
    tiri::initialize();
    tiri_core::window_pane::input_wait_tests::input_ending_with_its_pane_resumes_the_command();
}

#[test]
fn input_ending_after_its_pane_is_gone_exits_the_client_and_parks() {
    tiri::initialize();
    tiri_core::window_pane::input_wait_tests::input_ending_after_its_pane_is_gone_exits_the_client_and_parks();
}

#[test]
fn a_dead_file_client_parks_without_exiting() {
    tiri::initialize();
    tiri_core::window_pane::input_wait_tests::a_dead_file_client_parks_without_exiting();
}

#[test]
fn dropping_a_lost_client_drops_its_suspended_future() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::dropping_a_lost_client_drops_its_suspended_future();
}

#[test]
fn a_completed_future_may_release_its_own_client() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::a_completed_future_may_release_its_own_client();
}

#[test]
fn a_body_may_release_its_own_client_while_polled() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::a_body_may_release_its_own_client_while_polled();
}

#[test]
fn dropping_a_lost_client_cancels_a_file_backed_wait() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::dropping_a_lost_client_cancels_a_file_backed_wait();
}

#[test]
fn a_client_retired_inside_a_callback_is_freed_from_dispatch() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::a_client_retired_inside_a_callback_is_freed_from_dispatch();
}

#[test]
fn retiring_a_lost_client_cancels_its_suspended_item_from_dispatch() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::retiring_a_lost_client_cancels_its_suspended_item_from_dispatch();
}

#[test]
fn runtime_shutdown_releases_a_pending_retirement() {
    tiri::initialize();
    tiri_core::server_client::lost_queue_tests::runtime_shutdown_releases_a_pending_retirement();
}

#[test]
fn local_completion_can_be_cancelled_after_runtime_shutdown() {
    tiri::initialize();
    tiri_core::file::completion_tests::local_completion_can_be_cancelled_after_runtime_shutdown();
}

#[test]
fn dropping_a_lost_client_cancels_its_local_write() {
    tiri::initialize();
    tiri_core::file::completion_tests::dropping_a_lost_client_cancels_its_local_write();
}

#[test]
fn a_dead_file_client_leaves_its_command_blocked() {
    tiri::initialize();
    tiri_core::file::completion_tests::a_dead_file_client_leaves_its_command_blocked();
}

#[test]
fn cfg_done_resumes_the_waiting_startup_client() {
    tiri::initialize();
    tiri_core::cfg::cfg_wait_tests::cfg_done_resumes_the_waiting_startup_client();
}

#[test]
fn the_startup_client_does_not_wait_for_a_finished_config() {
    tiri::initialize();
    tiri_core::cfg::cfg_wait_tests::the_startup_client_does_not_wait_for_a_finished_config();
}

#[test]
fn a_freed_startup_client_is_not_retained_and_cfg_done_skips_it() {
    tiri::initialize();
    tiri_core::cfg::cfg_wait_tests::a_freed_startup_client_is_not_retained_and_cfg_done_skips_it();
}

#[test]
fn a_wake_from_another_queues_dispatch_runs_the_body_before_that_exec_returns() {
    tiri::initialize();
    tiri_cmds::test_cases::client_queue_tests::a_wake_from_another_queues_dispatch_runs_the_body_before_that_exec_returns();
}

#[test]
fn a_clientless_item_records_no_client_while_exec_sees_the_lent_one() {
    tiri::initialize();
    tiri_cmds::test_cases::client_queue_tests::a_clientless_item_records_no_client_while_exec_sees_the_lent_one();
}

#[test]
fn a_client_queue_item_records_its_client() {
    tiri::initialize();
    tiri_cmds::test_cases::client_queue_tests::a_client_queue_item_records_its_client();
}

#[test]
fn a_deferred_item_from_a_lent_item_records_the_lent_client() {
    tiri::initialize();
    tiri_cmds::test_cases::client_queue_tests::a_deferred_item_from_a_lent_item_records_the_lent_client();
}

#[test]
fn finish_reports_the_status_only_to_a_client_sessionless_at_finish() {
    tiri::initialize();
    tiri_cmds::test_cases::client_queue_tests::finish_reports_the_status_only_to_a_client_sessionless_at_finish();
}

#[test]
fn a_waiter_is_cancelled_once_its_item_drops() {
    tiri::initialize();
    tiri_cmds::test_cases::client_queue_tests::a_waiter_is_cancelled_once_its_item_drops();
}

#[test]
fn a_zero_delay_runs_the_timer_body_one_runtime_turn_later() {
    tiri::initialize();
    tiri_cmds::test_cases::run_shell_tests::a_zero_delay_runs_the_timer_body_one_runtime_turn_later();
}

#[test]
fn a_delay_resumes_at_its_deadline() {
    tiri::initialize();
    tiri_cmds::test_cases::run_shell_tests::a_delay_resumes_at_its_deadline();
}

#[test]
fn dropping_the_item_during_the_delay_cancels_the_timer() {
    tiri::initialize();
    tiri_cmds::test_cases::run_shell_tests::dropping_the_item_during_the_delay_cancels_the_timer();
}

#[test]
fn job_completion_runs_the_callback_body_inside_the_send() {
    tiri::initialize();
    tiri_cmds::test_cases::run_shell_tests::job_completion_runs_the_callback_body_inside_the_send();
}

#[test]
fn a_lost_client_is_freed_once_its_job_completes() {
    tiri::initialize();
    tiri_cmds::test_cases::run_shell_tests::a_lost_client_is_freed_once_its_job_completes();
}

#[test]
fn a_job_freed_unfinished_releases_the_data_and_parks() {
    tiri::initialize();
    tiri_cmds::test_cases::run_shell_tests::a_job_freed_unfinished_releases_the_data_and_parks();
}

#[test]
fn the_answer_acts_inside_the_send() {
    tiri::initialize();
    tiri_cmds::test_cases::confirm_before_tests::the_answer_acts_inside_the_send();
}

#[test]
fn a_prompt_freed_unanswered_releases_the_data_and_parks() {
    tiri::initialize();
    tiri_cmds::test_cases::confirm_before_tests::a_prompt_freed_unanswered_releases_the_data_and_parks();
}

#[test]
fn an_answer_after_the_waiting_client_is_freed_queues_nothing() {
    tiri::initialize();
    tiri_cmds::test_cases::confirm_before_tests::an_answer_after_the_waiting_client_is_freed_queues_nothing();
}

#[test]
fn a_background_confirmation_is_appended_to_the_answering_client() {
    tiri::initialize();
    tiri_cmds::test_cases::confirm_before_tests::a_background_confirmation_is_appended_to_the_answering_client();
}

#[test]
fn a_reported_file_is_stored_inside_the_send() {
    tiri::initialize();
    tiri_cmds::test_cases::load_buffer_tests::a_reported_file_is_stored_inside_the_send();
}

#[test]
fn a_local_file_is_stored_one_runtime_turn_later() {
    tiri::initialize();
    tiri_cmds::test_cases::load_buffer_tests::a_local_file_is_stored_one_runtime_turn_later();
}

#[test]
fn a_dead_file_client_releases_the_target_client_and_parks() {
    tiri::initialize();
    tiri_cmds::test_cases::load_buffer_tests::a_dead_file_client_releases_the_target_client_and_parks();
}

#[test]
fn signal_resumes_waiters_in_fifo_order() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::signal_resumes_waiters_in_fifo_order();
}

#[test]
fn unlock_hands_the_lock_to_the_first_locker_only() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::unlock_hands_the_lock_to_the_first_locker_only();
}

#[test]
fn a_waiter_woken_by_signal_can_wait_again_on_the_same_channel() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::a_waiter_woken_by_signal_can_wait_again_on_the_same_channel();
}

#[test]
fn a_waiter_woken_by_signal_can_signal_the_same_channel_again() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::a_waiter_woken_by_signal_can_signal_the_same_channel_again();
}

#[test]
fn unlock_hands_the_lock_to_a_locker_that_unlocks_on_wake() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::unlock_hands_the_lock_to_a_locker_that_unlocks_on_wake();
}

#[test]
fn unlock_hands_the_lock_to_a_locker_that_relocks_on_wake() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::unlock_hands_the_lock_to_a_locker_that_relocks_on_wake();
}

#[test]
fn flush_survives_a_locker_that_relocks_on_wake() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::flush_survives_a_locker_that_relocks_on_wake();
}

#[test]
fn signal_skips_a_cancelled_waiter_and_stays_sticky() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::signal_skips_a_cancelled_waiter_and_stays_sticky();
}

#[test]
fn client_lost_resumes_only_the_lost_clients_waiters() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::client_lost_resumes_only_the_lost_clients_waiters();
}

#[test]
fn a_lost_client_waiting_in_wait_for_is_freed() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::a_lost_client_waiting_in_wait_for_is_freed();
}

#[test]
fn an_accepted_event_resumes_the_event_waiter() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::an_accepted_event_resumes_the_event_waiter();
}

#[test]
fn a_rejected_event_keeps_the_event_waiter_suspended() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::a_rejected_event_keeps_the_event_waiter_suspended();
}

#[test]
fn a_dropped_event_waiter_is_pruned_by_the_next_event() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::a_dropped_event_waiter_is_pruned_by_the_next_event();
}

#[test]
fn wake_resumes_the_first_matching_event_waiter_only() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::wake_resumes_the_first_matching_event_waiter_only();
}

#[test]
fn client_lost_resumes_only_the_lost_clients_event_waiters() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::client_lost_resumes_only_the_lost_clients_event_waiters();
}

#[test]
fn flush_resumes_every_event_waiter() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::flush_resumes_every_event_waiter();
}

#[test]
fn a_lost_client_waiting_for_an_event_is_freed() {
    tiri::initialize();
    tiri_cmds::test_cases::wait_for_tests::a_lost_client_waiting_for_an_event_is_freed();
}
