use std::ffi::{CStr, CString};
use tiri::src::key_string::{
    key_string_format, key_string_format_into, key_string_lookup_string, key_string_parse,
    key_string_parse_cstr,
};
use tiri::src::shared::key::*;

fn formatted(key: key_code, with_flags: bool) -> Vec<u8> {
    key_string_format(key, with_flags).into_bytes()
}

#[test]
fn parses_modifiers_aliases_named_keys_and_numeric_forms() {
    assert_eq!(
        key_string_parse(b"c-m-s-a"),
        Some(('a' as key_code) | KEYC_CTRL | KEYC_META | KEYC_SHIFT)
    );
    assert_eq!(
        key_string_parse(b"C-a"),
        Some(('a' as key_code) | KEYC_CTRL)
    );
    assert_eq!(key_string_parse(b"^A"), Some(('a' as key_code) | KEYC_CTRL));
    assert_eq!(
        key_string_parse(b"M-S-Up"),
        Some(KEYC_UP | KEYC_CURSOR | KEYC_META | KEYC_SHIFT | KEYC_IMPLIED_META)
    );
    assert_eq!(
        key_string_parse(b"C-Up"),
        Some(KEYC_UP | KEYC_CURSOR | KEYC_CTRL)
    );

    assert_eq!(key_string_parse(b"IC"), Some(KEYC_IC));
    assert_eq!(key_string_parse(b"Insert"), Some(KEYC_IC));
    assert_eq!(key_string_parse(b"PageDown"), Some(KEYC_NPAGE));
    assert_eq!(key_string_parse(b"PgDn"), Some(KEYC_NPAGE));
    assert_eq!(key_string_parse(b"f1"), Some(KEYC_F1));
    assert_eq!(key_string_parse(b"User17"), Some(KEYC_USER + 17));
    assert_eq!(key_string_parse(b"User17tail"), Some(KEYC_USER + 17));
    assert_eq!(key_string_parse(b"User1000"), Some(KEYC_USER + 1000));
    assert_eq!(key_string_parse(b"User1001"), None);

    assert_eq!(key_string_parse(b"None"), Some(KEYC_NONE));
    assert_eq!(key_string_parse(b"Any"), Some(KEYC_ANY));
    let numeric_ascii = key_string_parse(b"0x41").unwrap();
    assert_ne!(numeric_ascii, 0x41);
    assert_eq!(formatted(numeric_ascii, false), b"A");
    assert_eq!(key_string_parse(b"0x41tail"), Some(numeric_ascii));
    assert_eq!(key_string_parse(b"0x1"), Some(0x1));
    let numeric_del = key_string_parse(b"0x7f").unwrap();
    assert_ne!(numeric_del, 0x7f);
    assert_eq!(formatted(numeric_del, false), b"\x7f");
}

#[test]
fn rejects_invalid_bytes_and_sentinels_have_distinct_contracts() {
    for input in [
        &b""[..],
        b"C-",
        b"X-a",
        b"^aX",
        b"Unknown",
        b"0x",
        b"0xzz",
        b"0X41",
        b"\x01",
        b"\xff",
        b"\xc3",
        b"a\0b",
    ] {
        assert_eq!(key_string_parse(input), None, "{input:?}");
    }
    assert_eq!(key_string_parse(b"^"), Some(b'^' as key_code));

    assert_eq!(formatted(KEYC_NONE, false), b"None");
    assert_eq!(formatted(KEYC_UNKNOWN, false), b"Unknown");
    assert_eq!(formatted(KEYC_ANY, false), b"Any");
    assert_eq!(key_string_parse(b"Unknown"), None);
}

#[test]
fn empty_inputs_are_rejected_by_both_rust_parse_entry_points() {
    assert_eq!(key_string_parse(b""), None);
    let empty = c"";
    assert_eq!(key_string_parse_cstr(empty), None);
}

#[test]
fn safe_cstr_parser_preserves_prefix_and_suffix_boundaries() {
    for (input, expected) in [
        (b"C-F1\0".as_slice(), KEYC_CTRL | KEYC_F1),
        (
            b"M-F1\0".as_slice(),
            KEYC_META | KEYC_IMPLIED_META | KEYC_F1,
        ),
        (b"^F1\0".as_slice(), KEYC_CTRL | KEYC_F1),
        (b"c-Insert\0".as_slice(), KEYC_CTRL | KEYC_IC),
    ] {
        let cstr = CStr::from_bytes_with_nul(input).unwrap();
        assert_eq!(key_string_parse_cstr(cstr), Some(expected), "{input:?}");
        assert_eq!(key_string_parse(&input[..input.len() - 1]), Some(expected));
    }

    let numeric = c"0x41tail";
    let numeric_key = key_string_parse_cstr(numeric).expect("numeric key");
    assert_eq!(key_string_parse(b"0x41tail"), Some(numeric_key));
    assert_eq!(formatted(numeric_key, false), b"A");
}

#[test]
fn safe_cstr_parser_keeps_invalid_bytes_and_sentinels_at_the_boundary() {
    for input in [
        b"\xff\0".as_slice(),
        b"C-\xff\0".as_slice(),
        b"\xc3\0".as_slice(),
    ] {
        let cstr = CStr::from_bytes_with_nul(input).unwrap();
        assert_eq!(key_string_parse_cstr(cstr), None, "{input:?}");
        unsafe {
            assert_eq!(key_string_lookup_string(cstr), KEYC_UNKNOWN);
        }
    }

    let unknown = c"Unknown";
    assert_eq!(key_string_parse_cstr(unknown), None);
    unsafe {
        assert_eq!(key_string_lookup_string(unknown), KEYC_UNKNOWN);
    }
}

#[test]
fn formats_canonically_with_flags_unicode_and_invalid_values() {
    assert_eq!(
        formatted(KEYC_CTRL | KEYC_META | KEYC_SHIFT | b'a' as key_code, false),
        b"C-M-S-a"
    );
    assert_eq!(formatted(KEYC_IC, false), b"IC");
    assert_eq!(formatted(KEYC_NPAGE, false), b"NPage");
    assert_eq!(
        formatted(KEYC_UP | KEYC_CURSOR | KEYC_IMPLIED_META, true),
        b"Up[CI]"
    );
    assert_eq!(formatted(KEYC_LITERAL | b'a' as key_code, true), b"a[L]");
    assert_eq!(formatted(KEYC_USER + 17, false), b"User17");
    assert_eq!(formatted(KEYC_USER + 1000, false), b"User1000");
    let invalid = (14 as key_code) << 32 | 0x1234;
    assert_eq!(formatted(invalid, false), b"Invalid#e00001234");
    assert_eq!(
        formatted(KEYC_CTRL | invalid, false),
        b"Invalid#200e00001234"
    );

    let unicode = key_string_parse("λ".as_bytes()).expect("valid UTF-8 key");
    assert_eq!(formatted(unicode, false), "λ".as_bytes());
    let unicode_four = key_string_parse("😀".as_bytes()).expect("valid UTF-8 key");
    assert_eq!(formatted(unicode_four, false), "😀".as_bytes());
}

#[test]
fn invalid_packed_unicode_preserves_c_string_termination() {
    let invalid_packed_unicode = 0x01000000 as key_code;
    assert_eq!(
        key_string_format(invalid_packed_unicode, false).as_bytes(),
        b""
    );
    assert_eq!(
        key_string_format(KEYC_CTRL | KEYC_CURSOR | invalid_packed_unicode, true).as_bytes(),
        b"C-[C]"
    );
}

#[test]
fn owned_results_survive_later_calls_and_caller_buffers_are_bounded() {
    let first = key_string_format(KEYC_F1, false);
    let first_bytes = first.as_bytes().to_vec();
    for n in 0..512 {
        let _ = key_string_format(KEYC_USER + (n % 1001), n % 2 == 0);
    }
    assert_eq!(first.as_bytes(), first_bytes.as_slice());

    let expected = key_string_format(KEYC_UP | KEYC_CURSOR, true);
    let mut output = [0xa5; 64];
    let length = key_string_format_into(KEYC_UP | KEYC_CURSOR, true, &mut output).unwrap();
    assert_eq!(&output[..length], expected.as_bytes());
    assert_eq!(output[length], 0);

    let mut too_small = [0xa5; 2];
    assert_eq!(key_string_format_into(KEYC_UP, false, &mut too_small), None);
    assert!(too_small.iter().all(|&byte| byte == 0xa5));
}

#[test]
fn canonical_parse_format_round_trips() {
    for input in [
        &b"C-a"[..],
        b"M-S-Up",
        b"IC",
        b"NPage",
        b"F1",
        b"None",
        b"Any",
        b"User17",
        b"User1000",
        "λ".as_bytes(),
    ] {
        let key = key_string_parse(input).expect("canonical key name");
        let output = key_string_format(key, false);
        assert_eq!(key_string_parse(output.as_bytes()), Some(key), "{input:?}");
    }
}

#[test]
fn c_exports_remain_narrow_compatibility_adapters() {
    unsafe {
        let input = CString::new("C-a").unwrap();
        assert_eq!(
            key_string_lookup_string(&input),
            key_string_parse_cstr(&input).unwrap()
        );

        assert_eq!(key_string_format(KEYC_F1, false).as_bytes(), b"F1");
    }
}
