use std::ffi::CString;
use tiri::src::json::{json_find, json_parse};

#[test]
fn object_keys_survive_scratch_and_input_destruction() {
    let input = CString::new(
        b"{\"\xff\":2,\"a\\u0000b\":3,\"nested\":{\"child\":true},\"array\":[]}".as_slice(),
    )
    .unwrap();
    let mut cause: Option<CString> = None;
    let object = json_parse(&input, Some(&mut cause));
    let object = object.expect("document parses");
    assert!(cause.is_none());
    drop(input);
    for key in [b"\xff".as_slice(), b"a\\u0000b", b"nested", b"array"] {
        let key = CString::new(key).unwrap();
        let node = json_find(&object, &key).expect("key survives input destruction");
        assert_eq!(node.key.as_deref(), Some(key.as_c_str()));
    }
    drop(object);
}

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

#[test]
fn lookups_borrow_owned_values_and_preserve_error_bytes() {
    use tiri::src::json::{
        json_find_array, json_find_boolean, json_find_number, json_find_object, json_find_string,
    };
    let input = c"{\"text\":\"\xff\",\"number\":42,\"boolean\":false,\"object\":{},\"array\":[{}]}";
    let root = json_parse(input, None).unwrap();
    assert_eq!(json_find_string(&root, c"text").unwrap(), c"\xff");
    assert_eq!(json_find_number(&root, c"number").unwrap(), 42);
    assert_eq!(json_find_boolean(&root, c"boolean").unwrap(), 0);
    assert_eq!(json_find_object(&root, c"object").unwrap().type_0(), 3);
    assert_eq!(json_find_array(&root, c"array").unwrap().len(), 1);
    assert_eq!(
        json_find_string(&root, c"number").unwrap_err(),
        c"key \"number\" expected a string"
    );
    assert_eq!(
        json_find_number(&root, c"text").unwrap_err(),
        c"key \"text\" expected a number"
    );
    assert_eq!(
        json_find_boolean(&root, c"text").unwrap_err(),
        c"key \"text\" expected a boolean"
    );
    assert_eq!(
        json_find_object(&root, c"array").unwrap_err(),
        c"key \"array\" expected an object"
    );
    assert_eq!(
        json_find_array(&root, c"object").unwrap_err(),
        c"key \"object\" expected an array"
    );
    assert_eq!(
        json_find_number(&root, c"\xfe").unwrap_err(),
        c"key \"\xfe\" not found"
    );
}
