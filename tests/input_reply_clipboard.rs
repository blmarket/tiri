use tiri::src::input::input_reply_clipboard;
use tiri::src::reactor::{bufferevent_free, bufferevent_new, evbuffer_pullup};
use std::ffi::c_char;

fn reply(buf: *const c_char, len: usize, end: &'static [u8], clip: c_char) -> Vec<u8> {
    unsafe {
        let bev = bufferevent_new(-1, None, None, None);
        assert!(!bev.is_null());
        input_reply_clipboard(bev, buf, len, end.as_ptr().cast(), clip);
        let output = &mut *(*bev).output;
        let bytes = evbuffer_pullup(output, -1).unwrap_or_default().to_vec();
        bufferevent_free(bev);
        bytes
    }
}

#[test]
fn encodes_binary_clipboard_data_and_preserves_bel_and_st() {
    let binary = b"A\0B";
    assert_eq!(
        reply(
            binary.as_ptr().cast(),
            binary.len(),
            b"\x07\0",
            b'c' as c_char
        ),
        b"\x1b]52;c;QQBC\x07"
    );
    let text = b"hi";
    assert_eq!(
        reply(text.as_ptr().cast(), text.len(), b"\x1b\\\0", 0),
        b"\x1b]52;;aGk=\x1b\\"
    );
}

#[test]
fn null_or_empty_data_yields_empty_payload_and_oversize_is_rejected() {
    let byte = b'X';
    assert_eq!(reply(std::ptr::null(), 1, b"\x07\0", 0), b"\x1b]52;;\x07");
    assert_eq!(
        reply((&byte as *const u8).cast(), 0, b"\x1b\\\0", 0),
        b"\x1b]52;;\x1b\\"
    );
    assert!(reply((&byte as *const u8).cast(), usize::MAX, b"\x07\0", 0).is_empty());
}
