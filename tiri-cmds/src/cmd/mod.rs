//! Command construction, target lookup, parsing, queue execution, and the command catalog.

mod core;
pub(crate) use self::core::cmd_list_print_cstring;
pub(crate) use self::core::cmd_print_cstring;
pub mod find;
pub mod parse;
pub mod queue;

pub(crate) use self::core::{
    cmd_find, cmd_get_alias, cmd_get_args, cmd_get_entry, cmd_get_parse_flags, cmd_get_source,
    cmd_list_all_have, cmd_list_copy, cmd_list_first, cmd_list_new, cmd_parse,
};

pub fn cmd_table() -> &'static [&'static crate::shared::command::cmd_entry] {
    &crate::catalog::CMD_TABLE
}
