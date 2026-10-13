use tiri::src::json::{
    json_array_members, json_find, json_get_number, json_parse, json_to_string, NODE_ARRAY,
    NODE_OBJECT,
};
use std::ffi::CString;

#[test]
fn array_owned_members_preserve_order_and_iteration() {
    let input = CString::new(r#"{"items":[{"x":1},{"y":2},{"z":3}]}"#).unwrap();
    let mut cause = None;
    let root = json_parse(&input, Some(&mut cause)).expect("parse JSON array");
    assert!(cause.is_none());
    drop(input);
    let array = json_find(&root, c"items").unwrap();
    assert_eq!(array.type_0(), NODE_ARRAY);
    let members = json_array_members(array).unwrap();
    assert_eq!(members.len(), 3);
    for ((member, key), number) in members.iter().zip([c"x", c"y", c"z"]).zip(1..=3) {
        assert_eq!(member.type_0(), NODE_OBJECT);
        assert_eq!(
            json_get_number(json_find(member, key).unwrap()),
            Some(number)
        );
    }
    assert_eq!(
        json_to_string(&root).to_bytes(),
        br#"{"items":[{"x":1},{"y":2},{"z":3}]}"#
    );
}

#[test]
fn empty_array_has_no_members() {
    let root = json_parse(c"{\"items\":[]}", None).unwrap();
    assert!(json_array_members(json_find(&root, c"items").unwrap())
        .unwrap()
        .is_empty());
    assert!(json_array_members(&root).is_none());
}

#[test]
fn malformed_array_drops_already_parsed_members() {
    let input = c"{\"items\":[{\"x\":1},{\"y\":2},]}";
    let mut cause = None;
    assert!(json_parse(input, Some(&mut cause)).is_none());
    assert!(cause.is_some());
}
