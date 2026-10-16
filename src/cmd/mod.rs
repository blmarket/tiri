//! Command construction, target lookup, parsing, queue execution, and command entries.

mod core;
pub(crate) use self::core::cmd_append_argv;
pub(crate) use self::core::{
    cmd_list_print_cstring, cmd_print_cstring, cmd_stringify_argv_cstring,
    cmd_template_replace_cstring,
};
pub mod entries;
pub mod find;
pub mod parse;
pub mod queue;

// Preserve the established family API without glob exports.
pub use self::core::{
    cmd_find, cmd_get_alias, cmd_get_args_mut, cmd_get_entry, cmd_get_group, cmd_get_parse_flags,
    cmd_get_source, cmd_list_all_have, cmd_list_any_have, cmd_list_append, cmd_list_append_all,
    cmd_list_copy, cmd_list_first, cmd_list_move, cmd_list_new, cmd_log_argv, cmd_mouse_at,
    cmd_mouse_pane, cmd_mouse_window, cmd_pack_argv, cmd_parse, cmd_table,
};
