//! Presentation defaults shared with tiri, applied before user configuration.
use crate::options::OptionsScope;
use crate::options::{options_local_entry, options_set_string, options_to_cstring};
use std::collections::btree_map::Entry;
use std::ffi::CStr;

const WINDOW_STATUS_FORMAT: &CStr = c"#I:#{P/i:#{?#{m:*fable*,#{pane_agent_model}},#[bg=red],#{?#{m:*luna*,#{pane_agent_model}},#[bg=brightblue],}}#{pane_state_emoji}#[default],#{?window_active,#[nounderscore],#[underscore]}#{?#{m:*fable*,#{pane_agent_model}},#[bg=red],#{?#{m:*luna*,#{pane_agent_model}},#[bg=brightblue],}}#{pane_state_emoji}#[default]} #{?git_worktree,#{git_worktree}#{?git_subdir,/,},#{?pane_current_path,#{b:pane_current_path},#{b:session_path}}}#{?git_action, [#{git_action}#{?git_action_total, #{git_action_step}/#{git_action_total},}],}#{?window_flags,#{window_flags}, }";

pub(super) fn apply() {
    OptionsScope::GlobalWindow.with_local(|options| {
        for name in [c"window-status-format", c"window-status-current-format"] {
            if let Entry::Occupied(e) = options_local_entry(options, name) {
                if e.get().tableentry.is_some_and(|definition| {
                    options_to_cstring(e.get(), None, 0)
                        == crate::options::options_default_to_cstring(definition)
                }) {
                    options_set_string(options, name, 0, |out| {
                        out.write_all(WINDOW_STATUS_FORMAT.to_bytes())
                    });
                }
            }
        }
    });
}
