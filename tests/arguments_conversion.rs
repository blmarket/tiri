use std::ffi::{CStr, CString};
use std::rc::Rc;
use tiri::src::shared::command::cmd_list;

use tiri::src::arguments::{
    args_copy, args_create, args_first_value, args_parse as parse_args, args_percentage_result,
    args_push_positional_commands, args_set_owned_commands, args_string,
    args_string_percentage_result, args_strtonum_result, parse_number, parse_percentage,
    ArgumentValueError,
};
use tiri::src::cmd::{cmd_list_new, cmd_list_print};
use tiri::src::shared::arguments::{args, args_parse, ArgumentValue, ARGS_PARSE_COMMANDS};

fn cstring(value: &str) -> CString {
    CString::new(value).expect("test input contains no NUL")
}

fn error_message(error: ArgumentValueError) -> &'static [u8] {
    error.message().to_bytes()
}

fn borrowed_string_value(value: &CStr) -> ArgumentValue<'_> {
    ArgumentValue::borrowed_string(value)
}

fn borrowed_commands_value(cmdlist: &Rc<std::cell::RefCell<cmd_list>>) -> ArgumentValue<'_> {
    ArgumentValue::borrowed_commands(cmdlist)
}

#[test]
fn number_helper_preserves_syntax_bounds_and_diagnostics() {
    let minimum = cstring("-9223372036854775808");
    let maximum = cstring("9223372036854775807");
    let below = cstring("-9223372036854775809");
    let above = cstring("9223372036854775808");
    let signed = cstring("  +17");
    let malformed = cstring("17x");
    let empty = cstring("");

    assert_eq!(
        parse_number(minimum.as_c_str(), i64::MIN, i64::MAX),
        Ok(i64::MIN)
    );
    assert_eq!(
        parse_number(maximum.as_c_str(), i64::MIN, i64::MAX),
        Ok(i64::MAX)
    );
    assert_eq!(
        parse_number(below.as_c_str(), i64::MIN, i64::MAX),
        Err(ArgumentValueError::TooSmall)
    );
    assert_eq!(
        parse_number(above.as_c_str(), i64::MIN, i64::MAX),
        Err(ArgumentValueError::TooLarge)
    );
    assert_eq!(parse_number(signed.as_c_str(), 0, 20), Ok(17));
    assert_eq!(
        parse_number(malformed.as_c_str(), 0, 20),
        Err(ArgumentValueError::Invalid)
    );
    assert_eq!(
        parse_number(empty.as_c_str(), 0, 20),
        Err(ArgumentValueError::Invalid)
    );

    assert_eq!(error_message(ArgumentValueError::TooSmall), b"too small");
    assert_eq!(error_message(ArgumentValueError::TooLarge), b"too large");
    assert_eq!(error_message(ArgumentValueError::Invalid), b"invalid");
}

#[test]
fn percentage_helper_preserves_literal_and_computed_bounds() {
    let zero = cstring("0%");
    let full = cstring("100%");
    let maximum = cstring("1000%");
    let percentage_too_large = cstring("1001%");
    let computed_too_large = cstring("101%");
    let computed_too_small = cstring("0%");
    let malformed = cstring("bad%");
    let empty = cstring("");

    assert_eq!(parse_percentage(zero.as_c_str(), 0, 100, 80), Ok(0));
    assert_eq!(parse_percentage(full.as_c_str(), 0, 100, 80), Ok(80));
    assert_eq!(parse_percentage(maximum.as_c_str(), 0, 100, 10), Ok(100));
    assert_eq!(
        parse_percentage(percentage_too_large.as_c_str(), 0, 100, 10),
        Err(ArgumentValueError::TooLarge)
    );
    assert_eq!(
        parse_percentage(computed_too_large.as_c_str(), 0, 100, 100),
        Err(ArgumentValueError::TooLarge)
    );
    assert_eq!(
        parse_percentage(computed_too_small.as_c_str(), 1, 100, 100),
        Err(ArgumentValueError::TooSmall)
    );
    assert_eq!(
        parse_percentage(malformed.as_c_str(), 0, 100, 100),
        Err(ArgumentValueError::Invalid)
    );
    assert_eq!(
        parse_percentage(empty.as_c_str(), 0, 100, 100),
        Err(ArgumentValueError::Empty)
    );
    assert_eq!(
        args_string_percentage_result(None, 100, 80),
        Err(ArgumentValueError::Missing)
    );
    assert_eq!(
        args_string_percentage_result(Some(zero.as_c_str()), 100, 80),
        Ok(0)
    );
    assert_eq!(error_message(ArgumentValueError::Empty), b"empty");
}

#[test]
fn command_values_and_cached_strings_keep_their_storage_ownership() {
    unsafe {
        let mut args = args_create();
        let cmdlist = cmd_list_new();
        args_set_owned_commands(&mut args, b'c', cmdlist, 0);

        assert_eq!(
            args_strtonum_result(&mut *args, b'c', 0, 100),
            Err(ArgumentValueError::Missing)
        );
        assert_eq!(
            args_percentage_result(&mut *args, b'c', 0, 100, 100),
            Err(ArgumentValueError::Missing)
        );

        let value = args_first_value(&args, b'c').unwrap();
        let rendered = cmd_list_print(&value.as_commands().unwrap().borrow(), 0);
        assert_eq!(rendered.as_bytes(), b"");
        drop(args);

        let mut args = args_create();
        args_push_positional_commands(&mut args, cmd_list_new());
        let first = args_string(&mut args, 0).map_or(std::ptr::null(), |value| value.as_ptr());
        let second = args_string(&mut args, 0).map_or(std::ptr::null(), |value| value.as_ptr());
        assert_eq!(first, second);
        assert_eq!(CStr::from_ptr(first).to_bytes(), b"");
        drop(args);
    }
}

#[test]
fn borrowed_parser_command_retains_only_while_stored() {
    fn commands(
        _: &mut args,
        _: core::ffi::c_uint,
    ) -> Result<core::ffi::c_uint, tiri::src::shared::arguments::ArgsParseError> {
        Ok(ARGS_PARSE_COMMANDS)
    }

    unsafe {
        let cmdlist = cmd_list_new();
        let values = [
            ArgumentValue::borrowed_string(c"command"),
            ArgumentValue::borrowed_commands(&cmdlist),
        ];
        let mut spec = args_parse {
            template: c"",
            lower: 1,
            upper: -1,
            cb: Some(commands),
        };
        let stored = parse_args(&spec, &values).expect("valid command argument");
        assert_eq!(Rc::strong_count(&cmdlist), 2);
        drop(stored);
        assert_eq!(Rc::strong_count(&cmdlist), 1);

        spec.lower = 2;
        assert!(parse_args(&spec, &values).is_err());
        assert_eq!(Rc::strong_count(&cmdlist), 1);
        drop(values);
        drop(cmdlist);
    }
}

#[test]
fn positional_command_cache_survives_array_growth_and_copy() {
    fn command_argument(
        args: &mut args,
        count: core::ffi::c_uint,
    ) -> Result<core::ffi::c_uint, tiri::src::shared::arguments::ArgsParseError> {
        if count == 1 {
            // Populate the positional cache while the value array still grows.
            args_string(args, 0);
        }
        Ok(ARGS_PARSE_COMMANDS)
    }

    unsafe {
        let command = cstring("command");
        let command_lists = [cmd_list_new(), cmd_list_new()];
        let values = [
            borrowed_string_value(command.as_c_str()),
            borrowed_commands_value(&command_lists[0]),
            borrowed_commands_value(&command_lists[1]),
        ];
        let spec = args_parse {
            template: c"",
            lower: 0,
            upper: -1,
            cb: Some(command_argument),
        };
        let mut args = parse_args(&spec, &values).expect("valid command arguments");
        drop(values);
        drop(command_lists);

        let first = (&args.values)[0].cached.as_ref().unwrap().as_ptr();
        assert!(!first.is_null());
        assert_eq!(CStr::from_ptr(first).to_bytes(), b"");
        assert_eq!(
            args_string(&mut args, 0).map_or(std::ptr::null(), |value| value.as_ptr()),
            first
        );
        let second = args_string(&mut args, 1).map_or(std::ptr::null(), |value| value.as_ptr());
        assert_ne!(first, second);

        let mut copied = args_copy(&args, &Vec::new());
        let copied_first =
            args_string(&mut copied, 0).map_or(std::ptr::null(), |value| value.as_ptr());
        let copied_second =
            args_string(&mut copied, 1).map_or(std::ptr::null(), |value| value.as_ptr());
        assert_ne!(copied_first, first);
        assert_ne!(copied_second, second);
        assert_eq!(CStr::from_ptr(copied_first).to_bytes(), b"");
        assert_eq!(CStr::from_ptr(copied_second).to_bytes(), b"");
        drop(args);
        assert_eq!(CStr::from_ptr(copied_first).to_bytes(), b"");
        drop(copied);
    }
}

#[test]
fn rejected_command_argument_keeps_source_value_ownership() {
    unsafe {
        let command = cstring("command");
        let command_list = cmd_list_new();
        let values = [
            borrowed_string_value(command.as_c_str()),
            borrowed_commands_value(&command_list),
        ];
        let spec = args_parse {
            template: c"",
            lower: 0,
            upper: -1,
            cb: None,
        };
        let error = parse_args(&spec, &values)
            .err()
            .expect("command value must be rejected");
        let error = match error {
            tiri::src::arguments::ArgsParseError::Message(error) => error,
            tiri::src::arguments::ArgsParseError::Usage => panic!("expected a diagnostic"),
        };
        assert_eq!(error.to_bytes(), b"argument 1 must be \"string\"");
        drop(values);
        drop(command_list);
    }
}
