use std::ffi::CString;
use tiri::src::hyperlinks::*;

#[test]
fn uri_limit_applies_after_escaping_without_consuming_ids() {
    let table = hyperlinks_init();
    let limit = MAX_HYPERLINK_URI as usize;
    let boundary = CString::new(vec![b'x'; limit]).unwrap();
    let first = hyperlinks_put(&table, &boundary, None);
    assert_ne!(first, 0);
    let too_long = CString::new(vec![b'x'; limit + 1]).unwrap();
    assert_eq!(hyperlinks_put(&table, &too_long, None), 0);
    let escaped_boundary = CString::new(vec![0xff; limit / 4]).unwrap();
    let second = hyperlinks_put(&table, &escaped_boundary, None);
    assert_eq!(second, first + 1);
    let entry = hyperlinks_get(&table, second).unwrap();
    assert_eq!(entry.uri.as_bytes(), b"\\377".repeat(limit / 4));
    drop(entry);
    let escaped_overflow = CString::new(vec![0xff; limit / 4 + 1]).unwrap();
    assert_eq!(hyperlinks_put(&table, &escaped_overflow, None), 0);
    assert_eq!(hyperlinks_put(&table, c"next", None), second + 1);
}
