use crate::entries::attach_session::cmd_attach_session_entry;
use crate::entries::bind_key::cmd_bind_key_entry;
use crate::entries::break_pane::cmd_break_pane_entry;
use crate::entries::capture_pane::{cmd_capture_pane_entry, cmd_clear_history_entry};
use crate::entries::choose_tree::{
    cmd_choose_buffer_entry, cmd_choose_client_entry, cmd_choose_tree_entry,
    cmd_customize_mode_entry, cmd_display_panes_entry, cmd_switch_mode_entry,
};
use crate::entries::command_prompt::cmd_command_prompt_entry;
use crate::entries::confirm_before::cmd_confirm_before_entry;
use crate::entries::copy_mode::{cmd_clock_mode_entry, cmd_copy_mode_entry};
use crate::entries::detach_client::{cmd_detach_client_entry, cmd_suspend_client_entry};
use crate::entries::display_menu::{cmd_display_menu_entry, cmd_display_popup_entry};
use crate::entries::display_message::cmd_display_message_entry;
use crate::entries::find_window::cmd_find_window_entry;
use crate::entries::if_shell::cmd_if_shell_entry;
use crate::entries::join_pane::cmd_join_pane_entry;
use crate::entries::kill_pane::cmd_kill_pane_entry;
use crate::entries::kill_server::{cmd_kill_server_entry, cmd_start_server_entry};
use crate::entries::kill_session::cmd_kill_session_entry;
use crate::entries::kill_window::{cmd_kill_window_entry, cmd_unlink_window_entry};
use crate::entries::list_buffers::cmd_list_buffers_entry;
use crate::entries::list_clients::cmd_list_clients_entry;
use crate::entries::list_commands::cmd_list_commands_entry;
use crate::entries::list_keys::cmd_list_keys_entry;
use crate::entries::list_panes::cmd_list_panes_entry;
use crate::entries::list_sessions::cmd_list_sessions_entry;
use crate::entries::list_windows::cmd_list_windows_entry;
use crate::entries::load_buffer::cmd_load_buffer_entry;
use crate::entries::lock_server::{
    cmd_lock_client_entry, cmd_lock_server_entry, cmd_lock_session_entry,
};
use crate::entries::move_window::{cmd_link_window_entry, cmd_move_window_entry};
use crate::entries::new_session::{cmd_has_session_entry, cmd_new_session_entry};
use crate::entries::new_window::cmd_new_window_entry;
use crate::entries::paste_buffer::cmd_paste_buffer_entry;
use crate::entries::pipe_pane::cmd_pipe_pane_entry;
use crate::entries::refresh_client::cmd_refresh_client_entry;
use crate::entries::rename_session::cmd_rename_session_entry;
use crate::entries::rename_window::cmd_rename_window_entry;
use crate::entries::resize_pane::cmd_resize_pane_entry;
use crate::entries::resize_window::cmd_resize_window_entry;
use crate::entries::respawn_pane::cmd_respawn_pane_entry;
use crate::entries::respawn_window::cmd_respawn_window_entry;
use crate::entries::rotate_window::cmd_rotate_window_entry;
use crate::entries::run_shell::cmd_run_shell_entry;
use crate::entries::save_buffer::{cmd_save_buffer_entry, cmd_show_buffer_entry};
use crate::entries::select_layout::cmd_select_layout_entry;
use crate::entries::select_pane::{cmd_last_pane_entry, cmd_select_pane_entry};
use crate::entries::select_window::{
    cmd_last_window_entry, cmd_next_window_entry, cmd_previous_window_entry,
    cmd_select_window_entry,
};
use crate::entries::send_keys::{cmd_send_keys_entry, cmd_send_prefix_entry};
use crate::entries::server_access::cmd_server_access_entry;
use crate::entries::set_buffer::{cmd_delete_buffer_entry, cmd_set_buffer_entry};
use crate::entries::set_environment::cmd_set_environment_entry;
use crate::entries::set_option::{
    cmd_set_hook_entry, cmd_set_option_entry, cmd_set_window_option_entry,
};
use crate::entries::show_environment::cmd_show_environment_entry;
use crate::entries::show_messages::cmd_show_messages_entry;
use crate::entries::show_options::{
    cmd_show_hooks_entry, cmd_show_options_entry, cmd_show_window_options_entry,
};
use crate::entries::show_prompt_history::{
    cmd_clear_prompt_history_entry, cmd_show_prompt_history_entry,
};
use crate::entries::source_file::cmd_source_file_entry;
use crate::entries::split_window::{cmd_new_pane_entry, cmd_split_window_entry};
use crate::entries::swap_pane::cmd_swap_pane_entry;
use crate::entries::swap_window::cmd_swap_window_entry;
use crate::entries::switch_client::cmd_switch_client_entry;
use crate::entries::unbind_key::cmd_unbind_key_entry;
use crate::entries::wait_for::cmd_wait_for_entry;
use crate::shared::command::cmd_entry;
pub static CMD_TABLE: [&cmd_entry; 89] = {
    [
        &cmd_attach_session_entry,
        &cmd_bind_key_entry,
        &cmd_break_pane_entry,
        &cmd_capture_pane_entry,
        &cmd_choose_buffer_entry,
        &cmd_choose_client_entry,
        &cmd_choose_tree_entry,
        &cmd_clear_history_entry,
        &cmd_clear_prompt_history_entry,
        &cmd_clock_mode_entry,
        &cmd_command_prompt_entry,
        &cmd_confirm_before_entry,
        &cmd_copy_mode_entry,
        &cmd_customize_mode_entry,
        &cmd_delete_buffer_entry,
        &cmd_detach_client_entry,
        &cmd_display_menu_entry,
        &cmd_display_message_entry,
        &cmd_display_popup_entry,
        &cmd_display_panes_entry,
        &cmd_find_window_entry,
        &cmd_has_session_entry,
        &cmd_if_shell_entry,
        &cmd_join_pane_entry,
        &cmd_kill_pane_entry,
        &cmd_kill_server_entry,
        &cmd_kill_session_entry,
        &cmd_kill_window_entry,
        &cmd_last_pane_entry,
        &cmd_last_window_entry,
        &cmd_link_window_entry,
        &cmd_list_buffers_entry,
        &cmd_list_clients_entry,
        &cmd_list_commands_entry,
        &cmd_list_keys_entry,
        &cmd_list_panes_entry,
        &cmd_list_sessions_entry,
        &cmd_list_windows_entry,
        &cmd_load_buffer_entry,
        &cmd_lock_client_entry,
        &cmd_lock_server_entry,
        &cmd_lock_session_entry,
        &cmd_move_window_entry,
        &cmd_new_pane_entry,
        &cmd_new_session_entry,
        &cmd_new_window_entry,
        &cmd_next_window_entry,
        &cmd_paste_buffer_entry,
        &cmd_pipe_pane_entry,
        &cmd_previous_window_entry,
        &cmd_refresh_client_entry,
        &cmd_rename_session_entry,
        &cmd_rename_window_entry,
        &cmd_resize_pane_entry,
        &cmd_resize_window_entry,
        &cmd_respawn_pane_entry,
        &cmd_respawn_window_entry,
        &cmd_rotate_window_entry,
        &cmd_run_shell_entry,
        &cmd_save_buffer_entry,
        &cmd_select_layout_entry,
        &cmd_select_pane_entry,
        &cmd_select_window_entry,
        &cmd_send_keys_entry,
        &cmd_send_prefix_entry,
        &cmd_server_access_entry,
        &cmd_set_buffer_entry,
        &cmd_set_environment_entry,
        &cmd_set_hook_entry,
        &cmd_set_option_entry,
        &cmd_set_window_option_entry,
        &cmd_show_buffer_entry,
        &cmd_show_environment_entry,
        &cmd_show_hooks_entry,
        &cmd_show_messages_entry,
        &cmd_show_options_entry,
        &cmd_show_prompt_history_entry,
        &cmd_show_window_options_entry,
        &cmd_source_file_entry,
        &cmd_split_window_entry,
        &cmd_start_server_entry,
        &cmd_suspend_client_entry,
        &cmd_swap_pane_entry,
        &cmd_swap_window_entry,
        &cmd_switch_client_entry,
        &cmd_switch_mode_entry,
        &cmd_unbind_key_entry,
        &cmd_unlink_window_entry,
        &cmd_wait_for_entry,
    ]
};
