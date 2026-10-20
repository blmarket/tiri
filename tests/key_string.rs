use tiri::key_string::{key_string_format, key_string_parse_cstr};
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

#[test]
fn numeric_key_names_accept_scalar_values_only() {
    assert_eq!(key_string_parse_cstr(c"0xD800"), None);
    assert_eq!(key_string_parse_cstr(c"0x110000"), None);
    let emoji = key_string_parse_cstr(c"0x1F600").expect("a four-byte codepoint");
    assert_eq!(key_string_parse_cstr(c"\u{1F600}"), Some(emoji));
}
