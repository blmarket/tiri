use hmux::src::hyperlinks::*;
use std::ffi::CString;

#[test]
fn deduplication_reference_lifetime_reset_and_global_eviction() {
    let a = hyperlinks_init();
    let b = hyperlinks_init();
    let first = hyperlinks_put(&a, c"https://example.com", Some(c"id"));
    assert_eq!(
        first,
        hyperlinks_put(&a, c"https://example.com", Some(c"id"))
    );
    assert_ne!(
        first,
        hyperlinks_put(&a, c"https://other.example", Some(c"id"))
    );
    let anonymous = hyperlinks_put(&a, c"https://example.com", Some(c""));
    assert_ne!(anonymous, hyperlinks_put(&a, c"https://example.com", None));
    assert!(hyperlinks_get(&a, 0).is_none());
    let link = hyperlinks_get(&a, first).unwrap();
    assert_eq!(link.uri.as_c_str(), c"https://example.com");
    assert_eq!(link.internal_id.as_c_str(), c"id");
    assert_eq!(link.external_id.as_c_str(), c"tmux1");
    // Sharing and eviction do not move the entry's string allocations.
    let first_external_address = link.external_id.as_ptr() as usize;
    drop(link);
    let shared = hyperlinks_copy(&a);
    drop(a);
    let link = hyperlinks_get(&shared, first).unwrap();
    assert_eq!(link.external_id.as_ptr() as usize, first_external_address);
    assert_eq!(link.external_id.as_c_str(), c"tmux1");
    let snapshot = link.clone();
    drop(link);
    let mut retained_inner = 0;
    let mut retained_external_address = 0;
    for index in 0..MAX_HYPERLINKS {
        let inserted = hyperlinks_put(&b, c"new", Some(c""));
        if index == 10 {
            retained_inner = inserted;
            retained_external_address =
                hyperlinks_get(&b, inserted).unwrap().external_id.as_ptr() as usize;
        }
    }
    assert_eq!(
        hyperlinks_get(&b, retained_inner)
            .unwrap()
            .external_id
            .as_ptr() as usize,
        retained_external_address
    );
    // Eviction applies across owners, removing both indexes in the victim.
    assert!(hyperlinks_get(&shared, first).is_none());
    assert!(shared.is_empty());
    assert_eq!(b.len(), MAX_HYPERLINKS as usize - 1);
    assert_eq!(snapshot.uri.as_c_str(), c"https://example.com");
    hyperlinks_reset(&b);
    assert!(b.is_empty());
    let again = hyperlinks_put(&b, c"again", Some(c"id"));
    assert_eq!(
        hyperlinks_get(&b, again).unwrap().external_id.as_c_str(),
        c"tmux138D"
    );

    // Both retained fields must survive the caller's input going away.
    let escaped_uri = CString::new(b"https://example.com/a\n\xff".to_vec()).unwrap();
    let escaped_id = CString::new(b"id\n\xff".to_vec()).unwrap();
    let escaped = hyperlinks_put(&b, &escaped_uri, Some(&escaped_id));
    drop(escaped_uri);
    drop(escaped_id);
    let link = hyperlinks_get(&b, escaped).unwrap();
    assert_eq!(link.uri.as_bytes(), b"https://example.com/a\n\\377");
    assert_eq!(link.internal_id.as_bytes(), b"id\n\\377");
    drop(link);
    assert_eq!(
        hyperlinks_put(
            &b,
            &CString::new(b"https://example.com/a\n\xff".to_vec()).unwrap(),
            Some(&CString::new(b"id\n\xff".to_vec()).unwrap()),
        ),
        escaped
    );

    // Resetting one table removes its interleaved records without
    // disturbing another table's entry in global insertion order.
    let c = hyperlinks_init();
    let d = hyperlinks_init();
    let c_first = hyperlinks_put(&c, c"c-first", Some(c""));
    let d_only = hyperlinks_put(&d, c"d-only", Some(c""));
    let c_second = hyperlinks_put(&c, c"c-second", Some(c""));
    hyperlinks_reset(&c);
    assert!(hyperlinks_get(&c, c_first).is_none());
    assert!(hyperlinks_get(&c, c_second).is_none());
    assert_eq!(
        hyperlinks_get(&d, d_only).unwrap().uri.as_c_str(),
        c"d-only"
    );
    drop(c);
    drop(d);
    drop(shared);
    drop(b);
}

#[test]
fn reset_removes_both_indexes_and_does_not_reuse_inner_ids() {
    let table = hyperlinks_init();
    let first = hyperlinks_put(&table, c"https://example.test", Some(c"id"));
    let entry = hyperlinks_get(&table, first).unwrap().clone();
    let shared = table.clone();
    hyperlinks_reset(&shared);
    assert!(table.is_empty());
    assert!(hyperlinks_get(&table, first).is_none());
    let second = hyperlinks_put(&table, &entry.uri, Some(&entry.internal_id));
    assert_eq!(second, first + 1);
    assert_eq!(table.len(), 1);
    assert_eq!(
        hyperlinks_put(&shared, c"https://example.test", Some(c"id")),
        second
    );
}

#[test]
fn transferring_an_entry_can_evict_its_source_table() {
    let source = hyperlinks_init();
    let destination = hyperlinks_init();
    let first = hyperlinks_put(&source, c"https://example.test", Some(c"style-link"));
    for _ in 0..MAX_HYPERLINKS - 2 {
        hyperlinks_put(&destination, c"filler", None);
    }
    let entry = hyperlinks_get(&source, first).unwrap().clone();
    let copied = hyperlinks_put(&destination, &entry.uri, Some(&entry.internal_id));
    assert!(source.is_empty());
    assert!(hyperlinks_get(&source, first).is_none());
    let copied = hyperlinks_get(&destination, copied).unwrap();
    assert_eq!(copied.uri, entry.uri);
    assert_eq!(copied.internal_id, entry.internal_id);
}

#[test]
fn uri_limit_applies_after_escaping_without_consuming_ids() {
    let table = hyperlinks_init();
    let limit = MAX_HYPERLINK_URI as usize;
    let boundary = CString::new(vec![b'x'; limit]).unwrap();
    let first = hyperlinks_put(&table, &boundary, None);
    assert_ne!(first, 0);
    let too_long = CString::new(vec![b'x'; limit + 1]).unwrap();
    assert_eq!(hyperlinks_put(&table, &too_long, None), 0);
    // Invalid UTF-8 is escaped as four octal bytes per input byte.
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
