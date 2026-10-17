use std::ffi::CStr;
use tiri::src::shared::grid::utf8_data;
use tiri::src::shared::utf8::{UTF8_DONE, UTF8_ERROR, UTF8_MORE};
use tiri::src::text::utf8::{utf8_append, utf8_cstrhas, utf8_open, utf8_towc};
use tiri::src::text::utf8_decode::{decode_utf8, DecodeResult};

fn empty_data() -> utf8_data {
    utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    }
}

#[test]
fn legacy_stream_adapter_consumes_an_invalid_candidate_before_recovery() {
    let mut data = empty_data();
    unsafe {
        assert_eq!(utf8_open(&mut data, 0xe2), UTF8_MORE);
        assert_eq!(data.size, 3);
        assert_eq!(data.have, 1);

        assert_eq!(utf8_append(&mut data, b'('), UTF8_MORE);
        assert_eq!(data.have, 2);
        assert_eq!(utf8_append(&mut data, 0xa1), UTF8_ERROR);
        assert_eq!(data.have, 3);
        assert_eq!(&data.data[..3], &[0xe2, b'(', 0xa1]);
    }
}

#[test]
fn cstrhas_respects_utf8_cells_and_the_first_nul() {
    let query = |bytes: &[u8]| {
        let mut cell = empty_data();
        cell.size = bytes.len() as u8;
        cell.data[..bytes.len()].copy_from_slice(bytes);
        cell
    };
    for (source, cell, expected) in [
        (&b"a\0b\0"[..], query(b"b"), false),
        (&b"a\xc3\xa9z\0"[..], query(&[0xa9]), false),
        (&b"\xe2(\xa1\0"[..], query(&[0xe2, b'(', 0xa1]), false),
        (&b"\xe2(\xa1\0"[..], query(b"("), true),
        (&b"\xf0\x9f\x92\0"[..], query(&[0x9f]), true),
        (&b"\xff\0"[..], query(&[0xff]), true),
        (&b"a\xe2(\xa1\0"[..], query(b"a"), true),
    ] {
        assert_eq!(
            utf8_cstrhas(CStr::from_bytes_until_nul(source).unwrap(), &cell),
            expected
        );
    }
}

#[test]
fn legacy_codepoint_conversion_accepts_an_embedded_nul_cell() {
    let mut data = empty_data();
    data.data[0] = 0;
    data.have = 1;
    data.size = 1;
    let mut codepoint = -1i32 as libc::wchar_t;

    unsafe {
        assert_eq!(utf8_towc(&data, &mut codepoint), UTF8_DONE);
    }
    assert_eq!(codepoint, 0);
}

#[test]
fn legacy_codepoint_conversion_keeps_combined_cell_first_codepoint_behavior() {
    let mut data = empty_data();
    data.data[..2].copy_from_slice(b"ab");
    data.have = 2;
    data.size = 2;
    let mut codepoint = -1i32 as libc::wchar_t;

    unsafe {
        assert_eq!(utf8_towc(&data, &mut codepoint), UTF8_DONE);
    }
    assert_eq!(codepoint, b'a' as libc::wchar_t);
}

#[test]
fn helper_keeps_trailing_bytes_for_the_next_decode() {
    assert_eq!(
        decode_utf8("λx".as_bytes()),
        DecodeResult::Complete {
            codepoint: 0x03bb,
            len: 2,
        }
    );
}

#[test]
fn copied_cells_clear_unused_bytes_and_allow_self_assignment() {
    use tiri::src::text::utf8::{utf8_copy, utf8_set};
    let mut cell = utf8_data {
        data: [0xab; 32],
        have: 2,
        size: 2,
        width: 1,
    };
    cell.data[..2].copy_from_slice("é".as_bytes());
    let original = cell;
    cell = utf8_copy(&cell);
    assert_eq!(&cell.data[..2], "é".as_bytes());
    assert!(cell.data[2..].iter().all(|&byte| byte == 0));
    assert_eq!((cell.have, cell.size, cell.width), (2, 2, 1));
    assert!(original.data[2..].iter().all(|&byte| byte == 0xab));

    let oversized = utf8_data {
        data: [0xcd; 32],
        size: 33,
        ..empty_data()
    };
    assert_eq!(utf8_copy(&oversized).data, [0xcd; 32]);
    utf8_set(&mut cell, 0xff);
    assert_eq!((cell.have, cell.size, cell.width), (1, 1, 1));
    assert_eq!(cell.data[0], 0xff);
    assert!(cell.data[1..].iter().all(|&byte| byte == 0));
}

#[test]
fn codepoint_encoding_preserves_failure_outputs_and_unused_bytes() {
    use tiri::src::text::utf8::utf8_fromwc;
    unsafe {
        for codepoint in [-1i32 as libc::wchar_t, 0xd800, 0x110000] {
            let mut cell = utf8_data {
                data: [0xaa; 32],
                have: 7,
                size: 8,
                width: 9,
            };
            assert_eq!(utf8_fromwc(codepoint, &mut cell), UTF8_ERROR);
            assert_eq!(cell.data, [0xaa; 32]);
            assert_eq!((cell.have, cell.size, cell.width), (7, 8, 9));
        }
        for (codepoint, bytes, width) in [
            (0, &b"\0"[..], 0),
            (0xe9, "é".as_bytes(), 1),
            (0x6f22, "漢".as_bytes(), 2),
            (0x1f600, "😀".as_bytes(), 2),
        ] {
            let mut cell = utf8_data {
                data: [0xaa; 32],
                ..empty_data()
            };
            assert_eq!(utf8_fromwc(codepoint, &mut cell), UTF8_DONE);
            assert_eq!(&cell.data[..bytes.len()], bytes);
            assert!(cell.data[bytes.len()..].iter().all(|&byte| byte == 0xaa));
            assert_eq!(
                (cell.have, cell.size, cell.width),
                (bytes.len() as u8, bytes.len() as u8, width)
            );
        }
        for bytes in [&b"\xe2\x82"[..], &b"\xff"[..]] {
            let mut cell = empty_data();
            cell.data[..bytes.len()].copy_from_slice(bytes);
            cell.size = bytes.len() as u8;
            let mut codepoint = 123;
            assert_eq!(utf8_towc(&cell, &mut codepoint), UTF8_ERROR);
            assert_eq!(codepoint, 123);
        }
    }
}

#[test]
fn whitespace_scanning_respects_decode_order_and_cell_bounds() {
    use tiri::src::text::utf8::utf8_has_whitespace;
    for (bytes, expected) in [
        (&b"a\xc2\xa0"[..], 1),
        (&b"a\xe3\x80\x80"[..], 1),
        (&b" \xff"[..], 1),
        (&b"\xff "[..], 0),
        (&b"a\xe2\x82"[..], 0),
        ("é".as_bytes(), 0),
    ] {
        let mut cell = empty_data();
        cell.data[..bytes.len()].copy_from_slice(bytes);
        cell.size = bytes.len() as u8;
        assert_eq!(utf8_has_whitespace(&cell), expected);
    }
    assert_eq!(
        utf8_has_whitespace(&utf8_data {
            size: 33,
            ..empty_data()
        }),
        0
    );
}
