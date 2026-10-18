use tiri::json::json_parse;

#[test]
fn malformed_array_drops_already_parsed_members() {
    let input = c"{\"items\":[{\"x\":1},{\"y\":2},]}";
    let mut cause = None;
    assert!(json_parse(input, Some(&mut cause)).is_none());
    assert!(cause.is_some());
}
