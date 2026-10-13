//! Presentation defaults shared with tiri, applied before user configuration.
use crate::src::format::bytes::nullable_cstr;
use crate::src::options::{options_get_only, options_set_string, options_to_cstring};
use crate::src::tmux::global_w_options;
use std::ffi::CStr;

const WINDOW_STATUS_FORMAT: &CStr = c"#I:#{P/i:#{?#{m:*fable*,#{pane_agent_model}},#[bg=red],#{?#{m:*luna*,#{pane_agent_model}},#[bg=brightblue],}}#{pane_state_emoji}#[default],#{?window_active,#[nounderscore],#[underscore]}#{?#{m:*fable*,#{pane_agent_model}},#[bg=red],#{?#{m:*luna*,#{pane_agent_model}},#[bg=brightblue],}}#{pane_state_emoji}#[default]} #{?git_worktree,#{git_worktree}#{?git_subdir,/,},#{?pane_current_path,#{b:pane_current_path},#{b:session_path}}}#{?git_action, [#{git_action}#{?git_action_total, #{git_action_step}/#{git_action_total},}],}#{?window_flags,#{window_flags}, }";

pub(super) unsafe fn apply() {
    let Some(options) = global_w_options.as_mut() else {
        return;
    };
    for name in [c"window-status-format", c"window-status-current-format"] {
        let unchanged = options_get_only(options, name).is_some_and(|entry| {
            entry.tableentry.is_some_and(|definition| {
                options_to_cstring(entry, nullable_cstr(std::ptr::null()), 0)
                    == crate::src::options::options_default_to_cstring(definition)
            })
        });
        if unchanged {
            options_set_string(options, name, 0, |out| {
                out.write_all(WINDOW_STATUS_FORMAT.to_bytes())
            });
        }
    }
}
