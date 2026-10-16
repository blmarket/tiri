use std::ffi::CString;
use tiri::src::json::{json_find, json_parse};

#[test]
fn object_key_errors_destroy_partial_objects() {
    for (input, diagnostic) in [
        (r#"{"":1}"#, "invalid key"),
        (r#"{"a":1,"a":2}"#, "duplicate key"),
        (r#"{"a" "b"}"#, "missing colon"),
        (r#"{"a":}"#, "unexpected value"),
        (r#"{"a":false,}"#, "invalid object"),
        (r#"{"a":1 "b":2}"#, "invalid object"),
        (r#"{"a":{ "b":true, }}"#, "invalid object"),
        (r#"{"a":1, false:2}"#, "tokenization error"),
    ] {
        let input = CString::new(input).unwrap();
        let mut cause: Option<CString> = None;
        let object = json_parse(&input, Some(&mut cause));
        assert!(object.is_none(), "accepted {input:?}");
        assert!(cause.is_some());
        let message = cause.as_ref().unwrap().as_bytes();
        assert!(
            message
                .windows(diagnostic.len())
                .any(|w| w == diagnostic.as_bytes()),
            "{input:?}: {message:?}"
        );
    }
}

#[test]
fn token_growth_keeps_late_keys_and_cleans_up_on_error() {
    let fields = (0..350)
        .map(|i| format!("\"key{i}\":{i}"))
        .collect::<Vec<_>>()
        .join(",");
    for (input, valid) in [
        (format!("{{{fields}}}"), true),
        (format!("{{{fields},\"bad\":}}"), false),
    ] {
        let input = CString::new(input).unwrap();
        let mut cause: Option<CString> = None;
        let object = json_parse(&input, Some(&mut cause));
        if valid {
            let object = object.unwrap();
            assert!(cause.is_none());
            drop(input);
            for key in ["key0", "key255", "key349"] {
                let key = CString::new(key).unwrap();
                assert!(json_find(&object, &key).is_some(), "missing {key:?}");
            }
            drop(object);
        } else {
            assert!(object.is_none());
            assert!(cause.is_some());
            assert!(cause
                .as_ref()
                .unwrap()
                .to_bytes()
                .starts_with(b"unexpected value"));
        }
    }
}
