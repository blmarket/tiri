use std::ffi::CString;
use tiri::src::json::{json_parse, json_to_string, PARSE_DEPTH_MAX};

#[test]
fn truncated_tokens_report_errors_with_or_without_diagnostics() {
    for document in [
        br#"{"key":"value","nested":{"number":123},"items":[{"ok":true}]}"#.as_slice(),
        br#"{"escaped":"a\"b\u0000\n","false":false}"#,
        b"{\"\xff\":\"\xfe\"}",
    ] {
        for len in 0..document.len() {
            let input = CString::new(&document[..len]).unwrap();
            let mut cause = None;
            assert!(json_parse(&input, Some(&mut cause)).is_none(), "{input:?}");
            assert!(cause.is_some(), "{input:?}");
            assert!(json_parse(&input, None).is_none(), "{input:?}");
        }
        let input = CString::new(document).unwrap();
        let root = json_parse(&input, None);
        let root = root.expect("complete document parses");
        drop(input);
        assert!(!json_to_string(&root).to_bytes().is_empty());
    }
}

#[test]
fn borrowed_input_preserves_values_and_success_leaves_cause_untouched() {
    let expected = b"{\"a\":-9223372036854775808,\"b\":9223372036854775807,\"c\":0,\"d\":false,\"e\":true,\"f\":\"\xff\\u0000\\n\"}";
    let input = CString::new(expected.as_slice()).unwrap();
    let mut cause = Some(CString::new("previous diagnostic").unwrap());
    let root = json_parse(&input, Some(&mut cause));
    let root = root.expect("integer limits parse");
    drop(input);
    assert_eq!(cause.unwrap().as_bytes(), b"previous diagnostic");
    assert_eq!(json_to_string(&root).as_bytes(), expected);
    drop(root);

    let root = json_parse(c"{\"zero\":-0}", None);
    let root = root.unwrap();
    assert_eq!(json_to_string(&root).as_bytes(), br#"{"zero":0}"#);
    drop(root);
}

#[test]
fn parser_preserves_tmux_json_subset_and_error_context() {
    for (input, expected) in [
        ("", "empty input"),
        (" ", "expected object:  "),
        ("[]", "expected object: []"),
        ("{} {}", "unexpected trailing data: {}"),
        (r#"{"a":1,"a":2}"#, "duplicate key: :2}"),
        (r#"{"a":""}"#, "invalid string: \"\"}"),
        (r#"{"a":null}"#, "invalid boolean: null}"),
        (r#"{"a":truex}"#, "invalid boolean: truex}"),
        (r#"{"a":+1}"#, "invalid boolean: +1}"),
        (r#"{"a":01}"#, "invalid number: 01}"),
        (r#"{"a":-01}"#, "invalid number: -01}"),
        (r#"{"a":1.5}"#, "invalid number: 1.5}"),
        (r#"{"a":1e2}"#, "invalid number: 1e2}"),
        (
            r#"{"a":9223372036854775808}"#,
            "invalid number: 92233720...",
        ),
        (
            r#"{"a":-9223372036854775809}"#,
            "invalid number: -9223372...",
        ),
        (r#"{"a":[[]]}"#, "invalid array member: []]}"),
        (r#"{"a":[{},]}"#, "invalid array: ,]}"),
    ] {
        let input = CString::new(input).unwrap();
        let mut cause = None;
        assert!(json_parse(&input, Some(&mut cause)).is_none(), "{input:?}");
        assert_eq!(cause.unwrap().as_bytes(), expected.as_bytes(), "{input:?}");
        assert!(json_parse(&input, None).is_none(), "{input:?}");
    }
}

#[test]
fn recursive_borrows_preserve_depth_limit_and_restore_depth_for_siblings() {
    for (open, close) in [(r#"{"a":"#, "}"), (r#"{"a":["#, "]}")] {
        for depth in [PARSE_DEPTH_MAX - 1, PARSE_DEPTH_MAX] {
            let nested = format!("{}{{}}{}", open.repeat(depth - 1), close.repeat(depth - 1));
            let input = CString::new(format!(r#"{{"a":{nested},"b":{nested}}}"#)).unwrap();
            let mut cause = None;
            let root = json_parse(&input, Some(&mut cause));
            if depth < PARSE_DEPTH_MAX {
                let root = root.expect("nesting within limit parses");
                assert!(cause.is_none());
                assert_eq!(json_to_string(&root), input);
                drop(root);
            } else {
                assert!(root.is_none());
                assert!(cause
                    .unwrap()
                    .as_bytes()
                    .starts_with(b"parse depth exceeded"));
                assert!(json_parse(&input, None).is_none());
            }
        }
    }
}
