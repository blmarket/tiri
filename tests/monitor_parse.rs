use std::ffi::CString;
use tiri::monitor::{
    monitor_parse_owned, MONITOR_ALL_PANES, MONITOR_ALL_WINDOWS, MONITOR_PANE, MONITOR_SESSION,
    MONITOR_WINDOW,
};

fn parse(input: &[u8]) -> (i32, Option<Vec<u8>>, u32, i32, Option<Vec<u8>>) {
    let input = CString::new(input).unwrap();
    let Some(parsed) = monitor_parse_owned(&input) else {
        return (-1, None, MONITOR_SESSION, -1, None);
    };
    (
        0,
        Some(parsed.name.into_bytes()),
        parsed.type_0,
        parsed.id,
        Some(parsed.format.into_bytes()),
    )
}

#[test]
fn parses_monitor_targets_without_mutating_input() {
    let input = CString::new(b"\xffhook:%+12:format\xfe:tail".as_slice()).unwrap();
    let original = input.as_bytes_with_nul().to_vec();
    let parsed = monitor_parse_owned(&input).unwrap();
    assert_eq!(parsed.name.as_bytes(), b"\xffhook");
    assert_eq!(parsed.type_0, MONITOR_PANE);
    assert_eq!(parsed.id, 12);
    assert_eq!(parsed.format.as_bytes(), b"format\xfe:tail");
    assert_eq!(input.as_bytes_with_nul(), original.as_slice());
}

#[test]
fn preserves_monitor_target_variants_and_rejects_invalid_input() {
    for (input, expected_type, expected_id) in [
        (b"hook:%*:fmt".as_slice(), MONITOR_ALL_PANES, -1),
        (b"hook:@*:fmt".as_slice(), MONITOR_ALL_WINDOWS, -1),
        (b"hook::fmt".as_slice(), MONITOR_SESSION, -1),
        (b"hook:@7junk:fmt".as_slice(), MONITOR_WINDOW, 7),
    ] {
        let (status, name, type_0, id, format) = parse(input);
        assert_eq!(status, 0, "{input:?}");
        assert_eq!(name.as_deref(), Some(&b"hook"[..]), "{input:?}");
        assert_eq!(type_0, expected_type, "{input:?}");
        assert_eq!(id, expected_id, "{input:?}");
        assert_eq!(format.as_deref(), Some(&b"fmt"[..]), "{input:?}");
    }

    for input in [b"hook".as_slice(), b"hook:bogus:fmt".as_slice()] {
        let (status, name, _, _, format) = parse(input);
        assert_eq!(status, -1, "{input:?}");
        assert!(name.is_none(), "{input:?}");
        assert!(format.is_none(), "{input:?}");
    }
}
