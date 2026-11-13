//! Options: typed values in scoped tables, defined by the option table.

mod changes;
mod diagnostic;
mod from_string;
mod global;
mod mutation;
mod parse;
mod scope;
mod select;
mod store;
mod table;
mod types;

pub use changes::options_push_changes;
pub use from_string::options_find_choice;
pub use scope::OptionsScope;
pub use select::{options_scope_from_flags, options_scope_from_name};
pub(crate) use store::options_default_to_cstring;
pub use store::{
    options_create, options_default, options_empty, options_get_command, options_get_fire_count,
    options_get_fire_time, options_get_monitor_data, options_get_number, options_get_string,
    options_get_string_optional, options_hook_fired, options_is_array, options_is_string,
    options_iter, options_local_entry, options_match_owned, options_parse_owned,
    options_read_entry, options_search, options_set_number, options_set_string,
    options_table_entry, options_to_cstring, OptionMatchFailure, OwnedOptionName,
};
pub use table::options_table;
pub use types::{
    options, options_array, options_array_item, options_entry, options_table_entry,
    options_table_type, options_value, OptionCommand, OptionsArrayKey, OPTIONS_TABLE_CHOICE,
    OPTIONS_TABLE_COLOUR, OPTIONS_TABLE_COMMAND, OPTIONS_TABLE_FLAG, OPTIONS_TABLE_IS_ARRAY,
    OPTIONS_TABLE_IS_COLOUR, OPTIONS_TABLE_IS_HOOK, OPTIONS_TABLE_IS_STYLE, OPTIONS_TABLE_KEY,
    OPTIONS_TABLE_NONE, OPTIONS_TABLE_NUMBER, OPTIONS_TABLE_PANE, OPTIONS_TABLE_SERVER,
    OPTIONS_TABLE_SESSION, OPTIONS_TABLE_STRING, OPTIONS_TABLE_WINDOW,
};
