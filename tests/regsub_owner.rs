//! Exercise the public substitution API's owned result.
use std::ffi::CStr;
use tiri::src::regsub::regsub_cstring;

#[test]
fn substitution_preserves_matches_and_returns_freeable_bytes() {
    for (pattern, replacement, text, expected) in [
        (
            b"(a)(b)\0".as_slice(),
            b"\\2\\1\0".as_slice(),
            b"zabzab\0".as_slice(),
            b"zbazba".as_slice(),
        ),
        (b"^\0", b"<\0", b"abc\0", b"<abc"),
        (b"x\0", b"Y\0", b"abc\0", b"abc"),
        (b"a\0", b"\0", b"banana\0", b"bnn"),
        (b"a\0", b"b\0", b"\0", b""),
        (b"\0", b"ignored\0", b"plain\0", b"plain"),
        (b"(a)?b\0", b"\\1\0", b"b\0", b"1"),
        (b"x*\0", b"<\0", b"ab\0", b"a<b<"),
        (b"a\0", b"\\q\0", b"a\0", b"q"),
        (b"a\0", b"\\\0", b"a\0", b"\\"),
        (b"x\0", b"\xff\0", b"x\0", b"\xff"),
        (b"x\0", b"Y\0", b"\xffx\xfe\0", b"\xffY\xfe"),
    ] {
        let result = regsub_cstring(
            CStr::from_bytes_with_nul(pattern).unwrap(),
            CStr::from_bytes_with_nul(replacement).unwrap(),
            CStr::from_bytes_with_nul(text).unwrap(),
            libc::REG_EXTENDED,
        )
        .unwrap();
        assert_eq!(result.as_bytes(), expected);
    }

    let invalid = regsub_cstring(c"[", c"x", c"abc", libc::REG_EXTENDED);
    assert!(invalid.is_none());
}
