use super::number::parse_number;
use super::parse::{ARGS_PARSE_COMMANDS, ARGS_PARSE_INVALID};
use super::store::args_last_string;
use super::value::{args_type, ARGS_NONE};
use super::*;
use crate::src::cmd::cmd_list_new;
use crate::src::shared::command::cmd_list;
use ::core::mem::{align_of, size_of};
use std::ffi::{CStr, CString};
use std::rc::Rc;

#[test]
fn parser_slices_preserve_optional_values_and_flag_boundaries() {
    let spec = args_parse {
        template: c"ao::r:",
        lower: 0,
        upper: -1,
        cb: None,
    };
    /// (argv, -o value, whether -a is set, remaining positional arguments)
    type ParseCase<'a> = (&'a [&'a [u8]], Option<&'a [u8]>, bool, &'a [&'a [u8]]);
    let cases: &[ParseCase] = &[
        (&[b"-o"], None, false, &[]),
        (&[b"-o", b"-a"], None, true, &[]),
        (&[b"-o", b"--", b"-a"], None, false, &[b"-a"]),
        (&[b"-o", b"-7"], Some(b"-7"), false, &[]),
        (&[b"-o", b"-"], Some(b"-"), false, &[]),
        (&[b"-o", b""], Some(b""), false, &[]),
        (
            &[b"-aoRAW\xff", b"tail"],
            Some(b"RAW\xff"),
            true,
            &[b"tail"],
        ),
        (&[b"", b"-a"], None, false, &[b"", b"-a"]),
        (&[b"-", b"-a"], None, false, &[b"-", b"-a"]),
    ];
    for &(input, optional, has_a, positional) in cases {
        let values: Vec<_> = std::iter::once(b"test".as_slice())
            .chain(input.iter().copied())
            .map(|bytes| args_value::string(CString::new(bytes).unwrap()))
            .collect();
        let parsed = args_parse(&spec, &values).unwrap();
        assert_eq!(
            args_last_string(&parsed, b'o').map(CStr::to_bytes),
            optional,
            "{input:?}"
        );
        assert_eq!(
            args_flags(&parsed).any(|flag| flag == b'a'),
            has_a,
            "{input:?}"
        );
        let actual: Vec<_> = parsed
            .values
            .iter()
            .map(|value| value.as_string().expect("string argument").to_bytes())
            .collect();
        assert_eq!(actual, positional, "{input:?}");
    }
    let values = [
        args_value::string(c"test".to_owned()),
        args_value::string(c"-a".to_owned()),
        args_value::string(c"-?".to_owned()),
    ];
    assert!(args_parse(&spec, &values[..2]).is_ok());
    assert!(matches!(
        args_parse(&spec, &values),
        Err(ArgsParseError::Usage)
    ));
    let required = args_parse { lower: 1, ..spec };
    assert!(args_parse(&required, &[]).is_ok());
    assert!(args_parse(&required, &values[..1]).is_err());
}

#[test]
fn borrowed_payloads_copy_to_owners_without_retaining_input_storage() {
    let text = CString::new(b"borrowed\xff".as_slice()).unwrap();
    let commands = Rc::new(std::cell::RefCell::new(cmd_list {
        group: 0,
        list: Vec::new(),
    }));
    let observer = Rc::downgrade(&commands);
    let inputs = [
        ArgumentValue::borrowed_string(&text),
        ArgumentValue::borrowed_commands(&commands),
    ];
    assert_eq!(inputs[0].as_string().unwrap().as_ptr(), text.as_ptr());
    assert_eq!(Rc::strong_count(&commands), 1);
    let stored: Vec<args_value> = inputs.iter().map(ArgumentValue::to_owned).collect();
    assert_ne!(stored[0].as_string().unwrap().as_ptr(), text.as_ptr());
    assert!(Rc::ptr_eq(stored[1].as_commands().unwrap(), &commands));
    assert_eq!(Rc::strong_count(&commands), 2);
    drop(inputs);
    drop(text);
    drop(commands);
    assert_eq!(stored[0].as_string().unwrap().to_bytes(), b"borrowed\xff");
    assert_eq!(observer.strong_count(), 1);
    drop(stored);
    assert!(observer.upgrade().is_none());
}

#[test]
fn argument_domains_match_translated_c_baseline() {
    assert_eq!(size_of::<args_type>(), 4);
    assert_eq!(align_of::<args_type>(), 4);
    assert_eq!(size_of::<args_parse_type>(), 4);
    assert_eq!(align_of::<args_parse_type>(), 4);
    assert_eq!(ARGS_NONE, 0);
    assert_eq!(ARGS_COMMANDS, 2);
    assert_eq!(ARGS_PARSE_INVALID, 0);
    assert_eq!(ARGS_PARSE_COMMANDS, 3);
}

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
    assert_eq!(parse_number(c"\x0b\x0c-0", -1, 1), Ok(0));
    for invalid in [c"-", c" +", c"+-5", c"5 ", c"99999999999999999999x"] {
        assert_eq!(
            parse_number(invalid, i64::MIN, i64::MAX),
            Err(ArgumentValueError::Invalid)
        );
    }
    assert_eq!(parse_number(c"0", 1, 0), Err(ArgumentValueError::Invalid));

    assert_eq!(error_message(ArgumentValueError::TooSmall), b"too small");
    assert_eq!(error_message(ArgumentValueError::TooLarge), b"too large");
    assert_eq!(error_message(ArgumentValueError::Invalid), b"invalid");
}

#[test]
fn borrowed_parser_command_retains_only_while_stored() {
    fn commands(_: &mut args, _: core::ffi::c_uint) -> Result<core::ffi::c_uint, ArgsParseError> {
        Ok(ARGS_PARSE_COMMANDS)
    }

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
    let stored = args_parse(&spec, &values).expect("valid command argument");
    assert_eq!(Rc::strong_count(&cmdlist), 2);
    drop(stored);
    assert_eq!(Rc::strong_count(&cmdlist), 1);

    spec.lower = 2;
    assert!(args_parse(&spec, &values).is_err());
    assert_eq!(Rc::strong_count(&cmdlist), 1);
    drop(values);
    drop(cmdlist);
}

#[test]
fn positional_command_cache_survives_array_growth_and_copy() {
    fn command_argument(
        args: &mut args,
        count: core::ffi::c_uint,
    ) -> Result<core::ffi::c_uint, ArgsParseError> {
        if count == 1 {
            args_string(args, 0);
        }
        Ok(ARGS_PARSE_COMMANDS)
    }

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
    let mut args = args_parse(&spec, &values).expect("valid command arguments");
    drop(values);
    drop(command_lists);

    let cached = args.values[0]
        .cached
        .as_deref()
        .expect("cached command text");
    assert_eq!(cached.to_bytes(), b"");
    let first = cached.as_ptr();
    assert_eq!(args_string(&mut args, 0).map(CStr::as_ptr), Some(first));
    let second = args_string(&mut args, 1).map(CStr::as_ptr);
    assert_ne!(Some(first), second);

    let mut copied = args_copy(&args, &Vec::new());
    let copied_first = args_string(&mut copied, 0).map(CStr::as_ptr);
    let copied_second = args_string(&mut copied, 1).map(CStr::as_ptr);
    assert_ne!(copied_first, Some(first));
    assert_ne!(copied_second, second);
    drop(args);
    assert_eq!(
        args_string(&mut copied, 0).map(CStr::to_bytes),
        Some(&b""[..])
    );
    assert_eq!(
        args_string(&mut copied, 1).map(CStr::to_bytes),
        Some(&b""[..])
    );
    drop(copied);
}

#[test]
fn rejected_command_argument_keeps_source_value_ownership() {
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
    let error = args_parse(&spec, &values)
        .err()
        .expect("command value must be rejected");
    let error = match error {
        ArgsParseError::Message(error) => error,
        ArgsParseError::Usage => panic!("expected a diagnostic"),
    };
    assert_eq!(error.to_bytes(), b"argument 1 must be \"string\"");
    drop(values);
    drop(command_list);
}
