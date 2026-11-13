//! Command argument storage, parsing, printing and value conversion.

mod number;
mod parse;
mod print;
mod store;
mod value;

pub use number::{
    args_percentage_result, args_strtonum_and_expand_result, args_strtonum_result,
    ArgumentValueError,
};
pub use parse::{
    args_parse, args_parse_type, ArgsParseError, ARGS_PARSE_COMMANDS_OR_STRING, ARGS_PARSE_STRING,
};
pub use print::{args_escape_cstring, args_print_cstring};
pub use store::{
    args, args_copy, args_count, args_create, args_flag_values, args_flags, args_get, args_has,
    args_set_flag, args_set_string, args_string, args_to_vector,
};
pub use value::{args_value, ParseValue};

#[cfg(test)]
mod tests;
