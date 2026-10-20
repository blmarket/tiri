//! Command argument storage, parsing, printing and value conversion.

mod commands;
mod number;
mod parse;
mod print;
mod store;
mod value;

pub(crate) use commands::args_make_commands_get_command_cstring;
pub use commands::{
    args_command_state, args_make_commands, args_make_commands_now, args_make_commands_prepare,
};
pub use number::{
    args_percentage_result, args_strtonum_and_expand_result, args_strtonum_result,
    ArgumentValueError,
};
pub use parse::{
    args_parse, args_parse_type, ArgsParseError, ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING,
};
pub(crate) use print::{args_escape_cstring, args_print_cstring};
pub use store::{
    args, args_copy, args_count, args_create, args_flag_values, args_flags, args_get, args_has,
    args_set_flag, args_set_string, args_string, args_to_vector,
};
pub use value::{args_value, ParseValue};

#[cfg(test)]
mod tests;
