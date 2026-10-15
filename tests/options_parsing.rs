//! Isolated option-name regressions for set-option/show-options parsing.

use std::ffi::CString;
use tiri::src::options::{options_match_owned, options_parse_owned, OptionMatchFailure};

#[derive(Debug, PartialEq, Eq)]
struct MatchOutcome {
    name: Option<Vec<u8>>,
    key: Option<Vec<u8>>,
    ambiguous: i32,
}

fn cstring_until_nul(input: &[u8]) -> CString {
    let end = input
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(input.len());
    CString::new(&input[..end]).expect("test input prefix has no NUL")
}

unsafe fn run_match(input: &[u8]) -> MatchOutcome {
    let input = CString::new(input).expect("test input has no embedded NUL");
    match options_match_owned(input.as_c_str()) {
        Ok(parsed) => MatchOutcome {
            name: Some(parsed.name.into_bytes()),
            key: parsed.array_key.map(CString::into_bytes),
            ambiguous: 0,
        },
        Err(OptionMatchFailure::Ambiguous) => MatchOutcome {
            name: None,
            key: None,
            ambiguous: 1,
        },
        Err(OptionMatchFailure::Parse | OptionMatchFailure::Invalid) => MatchOutcome {
            name: None,
            key: None,
            ambiguous: 0,
        },
    }
}

fn run_parse(input: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let input = cstring_until_nul(input);
    options_parse_owned(input.as_c_str()).map_or((None, None), |parsed| {
        (
            Some(parsed.name.into_bytes()),
            parsed.array_key.map(CString::into_bytes),
        )
    })
}

/// Input bytes, then the expected option name and array key.
type ParseCase = (&'static [u8], Option<&'static [u8]>, Option<&'static [u8]>);
/// Input bytes, expected name and array key, then the expected ambiguity flag.
type MatchCase = (
    &'static [u8],
    Option<&'static [u8]>,
    Option<&'static [u8]>,
    i32,
);

#[test]
fn options_parse_array_key_preserves_bytes_and_normalizes_numbers() {
    let cases: &[ParseCase] = &[
        (b"status-format[0007]", Some(b"status-format"), Some(b"7")),
        (b"@test[\xff]", Some(b"@test"), Some(b"\xff")),
        (
            b"status-format[0007]\0ignored",
            Some(b"status-format"),
            Some(b"7"),
        ),
        (b"status-format[]", None, None),
        (b"status-format[4294967296]", None, None),
    ];
    for &(input, name, key) in cases {
        assert_eq!(
            run_parse(input),
            (name.map(<[u8]>::to_vec), key.map(<[u8]>::to_vec)),
            "input={input:?}"
        );
    }
}

const CASES: &[MatchCase] = &[
    (b"status", Some(b"status"), None, 0),
    (b"mous", Some(b"mouse"), None, 0),
    (b"pane-colors", Some(b"pane-colours"), None, 0),
    (
        b"status-format[0007]",
        Some(b"status-format"),
        Some(b"7"),
        0,
    ),
    (b"@test[hook]", Some(b"@test"), Some(b"hook"), 0),
    (b"[key]", None, None, 1),
    (b"status-", None, None, 1),
    (b"status[]", None, None, 0),
    (b"status-format[4294967296]", None, None, 0),
    (b"status-format[0]tail", None, None, 0),
    (b"not-an-option", None, None, 0),
];

#[test]
fn options_match_owned_cases() {
    for &(input, name, key, ambiguous) in CASES {
        let actual = unsafe { run_match(input) };
        assert_eq!(
            actual,
            MatchOutcome {
                name: name.map(<[u8]>::to_vec),
                key: key.map(<[u8]>::to_vec),
                ambiguous,
            },
            "input={input:?}"
        );
    }
}

#[test]
fn options_match_owned_reports_parse_ambiguity_and_invalidity() {
    unsafe {
        assert!(matches!(
            options_match_owned(c""),
            Err(OptionMatchFailure::Parse)
        ));
        assert!(matches!(
            options_match_owned(c"status-"),
            Err(OptionMatchFailure::Ambiguous)
        ));
        assert!(matches!(
            options_match_owned(c"not-an-option"),
            Err(OptionMatchFailure::Invalid)
        ));
    }
    assert!(options_parse_owned(c"status[]").is_none());
}
