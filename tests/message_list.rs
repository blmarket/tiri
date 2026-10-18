use std::ffi::CString;
use std::time::{Duration, UNIX_EPOCH};
use tiri::shared::status::message_list;

#[test]
fn message_list_owns_entries_trims_oldest_and_reverses_for_display() {
    let mut messages = message_list::new();
    let time = UNIX_EPOCH + Duration::from_secs(123) + Duration::from_micros(456);

    messages.push_back(CString::new("one").unwrap(), 0, time);
    messages.push_back(CString::new("two").unwrap(), 1, time);
    messages.push_back(CString::new("three").unwrap(), 2, time);
    messages.trim(3, 2);

    let displayed: Vec<_> = messages
        .iter_rev()
        .map(|msg| {
            (
                msg.msg.to_bytes().to_vec(),
                msg.msg_num,
                msg.msg_time.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            )
        })
        .collect();
    assert_eq!(
        displayed,
        [(b"three".to_vec(), 2, 123), (b"two".to_vec(), 1, 123)]
    );

    messages.trim(3, 1);
    let displayed: Vec<_> = messages
        .iter_rev()
        .map(|msg| (msg.msg.to_bytes().to_vec(), msg.msg_num))
        .collect();
    assert_eq!(displayed, [(b"three".to_vec(), 2)]);

    messages.clear();
    assert_eq!(messages.iter_rev().count(), 0);
}
