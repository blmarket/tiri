use tiri::key_string::key_string_format;
use tiri::shared::key::*;

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
