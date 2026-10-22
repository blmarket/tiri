#![cfg(unix)]
//! Scratch: drive paths whose RefCell borrows changed and report the first
//! step after which the server is gone.
mod common;
use common::Server;
use std::time::Duration;

const ALL: &str = "active_window_index=#{active_window_index} alternate_on=#{alternate_on} alternate_saved_x=#{alternate_saved_x} alternate_saved_y=#{alternate_saved_y} bracket_paste_flag=#{bracket_paste_flag} buffer_created=#{buffer_created} buffer_full=#{buffer_full} buffer_mode_format=#{buffer_mode_format} buffer_name=#{buffer_name} buffer_sample=#{buffer_sample} buffer_size=#{buffer_size} client_activity=#{client_activity} client_cell_height=#{client_cell_height} client_cell_width=#{client_cell_width} client_colours=#{client_colours} client_control_mode=#{client_control_mode} client_created=#{client_created} client_discarded=#{client_discarded} client_flags=#{client_flags} client_height=#{client_height} client_key_table=#{client_key_table} client_last_session=#{client_last_session} client_mode_format=#{client_mode_format} client_name=#{client_name} client_pid=#{client_pid} client_prefix=#{client_prefix} client_readonly=#{client_readonly} client_session=#{client_session} client_termfeatures=#{client_termfeatures} client_termname=#{client_termname} client_termtype=#{client_termtype} client_theme=#{client_theme} client_tty=#{client_tty} client_uid=#{client_uid} client_user=#{client_user} client_utf8=#{client_utf8} client_width=#{client_width} client_written=#{client_written} config_files=#{config_files} cursor_blinking=#{cursor_blinking} cursor_character=#{cursor_character} cursor_colour=#{cursor_colour} cursor_flag=#{cursor_flag} cursor_shape=#{cursor_shape} cursor_very_visible=#{cursor_very_visible} cursor_x=#{cursor_x} cursor_y=#{cursor_y} history_added=#{history_added} history_all_bytes=#{history_all_bytes} history_bytes=#{history_bytes} history_collected=#{history_collected} history_generation=#{history_generation} history_limit=#{history_limit} history_size=#{history_size} host=#{host} host_short=#{host_short} insert_flag=#{insert_flag} keypad_cursor_flag=#{keypad_cursor_flag} keypad_flag=#{keypad_flag} last_window_index=#{last_window_index} mouse_all_flag=#{mouse_all_flag} mouse_any_flag=#{mouse_any_flag} mouse_button_flag=#{mouse_button_flag} mouse_hyperlink=#{mouse_hyperlink} mouse_line=#{mouse_line} mouse_pane=#{mouse_pane} mouse_sgr_flag=#{mouse_sgr_flag} mouse_standard_flag=#{mouse_standard_flag} mouse_status_line=#{mouse_status_line} mouse_status_range=#{mouse_status_range} mouse_utf8_flag=#{mouse_utf8_flag} mouse_word=#{mouse_word} mouse_x=#{mouse_x} mouse_y=#{mouse_y} next_session_id=#{next_session_id} origin_flag=#{origin_flag} pane_active=#{pane_active} pane_at_bottom=#{pane_at_bottom} pane_at_left=#{pane_at_left} pane_at_right=#{pane_at_right} pane_at_top=#{pane_at_top} pane_bg=#{pane_bg} pane_bottom=#{pane_bottom} pane_command_duration=#{pane_command_duration} pane_command_end_time=#{pane_command_end_time} pane_command_running=#{pane_command_running} pane_command_start_time=#{pane_command_start_time} pane_command_status=#{pane_command_status} pane_current_command=#{pane_current_command} pane_current_path=#{pane_current_path} pane_dead=#{pane_dead} pane_dead_signal=#{pane_dead_signal} pane_dead_status=#{pane_dead_status} pane_dead_time=#{pane_dead_time} pane_fg=#{pane_fg} pane_flags=#{pane_flags} pane_format=#{pane_format} pane_height=#{pane_height} pane_id=#{pane_id} pane_index=#{pane_index} pane_in_mode=#{pane_in_mode} pane_input_off=#{pane_input_off} pane_key_mode=#{pane_key_mode} pane_last=#{pane_last} pane_last_output_time=#{pane_last_output_time} pane_last_prompt_time=#{pane_last_prompt_time} pane_left=#{pane_left} pane_marked=#{pane_marked} pane_marked_set=#{pane_marked_set} pane_mode=#{pane_mode} pane_output_generation=#{pane_output_generation} pane_path=#{pane_path} pane_pb_progress=#{pane_pb_progress} pane_pb_state=#{pane_pb_state} pane_pid=#{pane_pid} pane_pipe=#{pane_pipe} pane_pipe_pid=#{pane_pipe_pid} pane_private_modes=#{pane_private_modes} pane_right=#{pane_right} pane_search_string=#{pane_search_string} pane_start_command=#{pane_start_command} pane_start_command_list=#{pane_start_command_list} pane_start_path=#{pane_start_path} pane_synchronized=#{pane_synchronized} pane_tabs=#{pane_tabs} pane_title=#{pane_title} pane_top=#{pane_top} pane_tty=#{pane_tty} pane_unseen_changes=#{pane_unseen_changes} pane_unzoomed_height=#{pane_unzoomed_height} pane_unzoomed_width=#{pane_unzoomed_width} pane_width=#{pane_width} pane_x=#{pane_x} pane_y=#{pane_y} pane_zoomed_flag=#{pane_zoomed_flag} pid=#{pid} scroll_region_lower=#{scroll_region_lower} scroll_region_upper=#{scroll_region_upper} server_sessions=#{server_sessions} session_active=#{session_active} session_activity=#{session_activity} session_activity_flag=#{session_activity_flag} session_alert=#{session_alert} session_alerts=#{session_alerts} session_attached=#{session_attached} session_attached_list=#{session_attached_list} session_bell_flag=#{session_bell_flag} session_created=#{session_created} session_format=#{session_format} session_group=#{session_group} session_group_attached=#{session_group_attached} session_group_attached_list=#{session_group_attached_list} session_grouped=#{session_grouped} session_group_list=#{session_group_list} session_group_many_attached=#{session_group_many_attached} session_group_size=#{session_group_size} session_id=#{session_id} session_last_attached=#{session_last_attached} session_many_attached=#{session_many_attached} session_marked=#{session_marked} session_name=#{session_name} session_path=#{session_path} session_silence_flag=#{session_silence_flag} session_stack=#{session_stack} session_windows=#{session_windows} sixel_support=#{sixel_support} socket_path=#{socket_path} start_time=#{start_time} synchronized_output_flag=#{synchronized_output_flag} tree_mode_format=#{tree_mode_format} uid=#{uid} user=#{user} version=#{version} window_active=#{window_active} window_active_clients=#{window_active_clients} window_active_clients_list=#{window_active_clients_list} window_active_sessions=#{window_active_sessions} window_active_sessions_list=#{window_active_sessions_list} window_activity=#{window_activity} window_activity_flag=#{window_activity_flag} window_bell_flag=#{window_bell_flag} window_bigger=#{window_bigger} window_cell_height=#{window_cell_height} window_cell_width=#{window_cell_width} window_end_flag=#{window_end_flag} window_flags=#{window_flags} window_format=#{window_format} window_height=#{window_height} window_id=#{window_id} window_index=#{window_index} window_last_flag=#{window_last_flag} window_layout=#{window_layout} window_layout_name=#{window_layout_name} window_linked=#{window_linked} window_linked_sessions=#{window_linked_sessions} window_linked_sessions_list=#{window_linked_sessions_list} window_manual_height=#{window_manual_height} window_manual_width=#{window_manual_width} window_marked_flag=#{window_marked_flag} window_name=#{window_name} window_offset_x=#{window_offset_x} window_offset_y=#{window_offset_y} window_panes=#{window_panes} window_raw_flags=#{window_raw_flags} window_silence_flag=#{window_silence_flag} window_stack_index=#{window_stack_index} window_start_flag=#{window_start_flag} window_visible_layout=#{window_visible_layout} window_width=#{window_width} window_zoomed_flag=#{window_zoomed_flag} wrap_flag=#{wrap_flag}";

fn alive(server: &Server, step: &str) {
    std::thread::sleep(Duration::from_millis(60));
    let out = server.run(&["has-session"]);
    assert!(out.status.success(), "server died after step: {step}");
}

fn step(server: &Server, step: &str, args: &[&str]) {
    let _ = server.run(args);
    alive(server, step);
}

#[test]
fn borrow_stress() {
    let server = Server::new();
    server.start(100, 30, 2);
    let mut term = server.attach_terminal(60, 20);
    term.drain();
    alive(&server, "attach");
    let tty = term.tty.clone();

    step(&server, "all formats", &["display-message", "-p", ALL]);
    step(&server, "all formats list-panes", &["list-panes", "-a", "-F", ALL]);
    step(&server, "set-environment", &["set-environment", "FOO", "bar"]);
    step(&server, "show-environment", &["show-environment"]);
    step(&server, "show-environment name", &["show-environment", "FOO"]);
    step(&server, "env format", &["display-message", "-p", "#{FOO} #{HOME}"]);
    step(&server, "env loop", &["display-message", "-p", "#{V:#{environ_name}=#{environ_value}#{FOO} }"]);
    step(&server, "env loop g", &["display-message", "-p", "#{V/g:#{environ_name} }"]);
    step(&server, "env loop c", &["display-message", "-p", "-c", &tty, "#{V/c:#{environ_name} }"]);
    step(&server, "status env", &["set", "-g", "status-right", "#{FOO} #{pane_current_command} #{cursor_shape} #{V:#{environ_name}}"]);
    step(&server, "status msg", &["display-message", "-c", &tty, "-d", "200", "hello #{FOO} #{pane_title}"]);
    std::thread::sleep(Duration::from_millis(300));
    alive(&server, "status msg timeout");
    step(&server, "status msg 0", &["display-message", "-c", &tty, "-d", "0", "x"]);
    term.send(b"q");
    alive(&server, "status msg key");

    step(&server, "copy-mode", &["copy-mode"]);
    step(&server, "copy-mode formats", &["display-message", "-p", ALL]);
    step(&server, "copy-mode search", &["send-keys", "-X", "search-backward", "x"]);
    step(&server, "copy-mode formats 2", &["list-panes", "-F", ALL]);
    step(&server, "pane-border-format", &["set", "-g", "pane-border-status", "top"]);
    step(&server, "pane-border-format 2", &["set", "-g", "pane-border-format", "#{pane_current_command} #{cursor_shape} #{pane_in_mode} #{pane_mode} #{FOO}"]);
    term.drain();
    step(&server, "copy cancel", &["send-keys", "-X", "cancel"]);

    for mode in [["choose-tree", ""], ["choose-client", ""], ["choose-buffer", ""], ["customize-mode", ""], ["clock-mode", ""]] {
        step(&server, mode[0], &[mode[0]]);
        step(&server, &format!("{} formats", mode[0]), &["display-message", "-p", ALL]);
        term.drain();
        term.send(b"q");
        alive(&server, &format!("{} quit", mode[0]));
    }

    step(&server, "command-prompt", &["command-prompt", "-b", "-t", &tty]);
    term.drain();
    term.send(b"\x1b");
    alive(&server, "prompt escape");
    step(&server, "command-prompt 2", &["command-prompt", "-b", "-t", &tty, "-p", "#{FOO}"]);
    term.send(b"display-message hi\r");
    alive(&server, "prompt enter");

    step(&server, "osc 4 query", &["send-keys", "printf '\\033]4;1;?\\033\\\\'", "Enter"]);
    step(&server, "osc 52", &["send-keys", "printf '\\033]52;c;aGVsbG8=\\007'", "Enter"]);
    step(&server, "osc 11 query", &["send-keys", "printf '\\033]11;?\\033\\\\'", "Enter"]);
    term.drain();

    step(&server, "pan", &["refresh-client", "-t", &tty, "-R", "5"]);
    step(&server, "pan l", &["refresh-client", "-t", &tty, "-L", "3"]);
    step(&server, "pan d", &["refresh-client", "-t", &tty, "-D", "3"]);
    step(&server, "pan c", &["refresh-client", "-t", &tty, "-c"]);
    term.drain();

    let control = server.attach_control(80, 24);
    alive(&server, "control attach");
    step(&server, "control output", &["send-keys", "seq 1 2000", "Enter"]);
    step(&server, "control formats", &["display-message", "-p", ALL]);
    drop(control);
    alive(&server, "control detach");

    step(&server, "select-pane -P", &["select-pane", "-P", "bg=red"]);
    step(&server, "pipe-pane", &["pipe-pane", "cat > /dev/null"]);
    step(&server, "pipe formats", &["display-message", "-p", ALL]);
    step(&server, "pipe-pane off", &["pipe-pane"]);
    step(&server, "respawn", &["respawn-pane", "-k", "sleep 60"]);
    step(&server, "mark", &["select-pane", "-m"]);
    step(&server, "marked formats", &["display-message", "-p", ALL]);
    step(&server, "dead", &["set", "-g", "remain-on-exit", "on"]);
    step(&server, "dead 2", &["respawn-pane", "-k", "true"]);
    std::thread::sleep(Duration::from_millis(200));
    step(&server, "dead formats", &["display-message", "-p", ALL]);
    term.drain();
    step(&server, "detach", &["detach-client", "-t", &tty]);
}
