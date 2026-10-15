use std::ffi::CString;
use tiri::src::session::*;

#[test]
fn named_group_owns_borrowed_name_and_reuses_stable_allocation() {
    assert!(session_groups::is_empty());
    let borrowed_name = CString::new(b"gr\xffup".to_vec()).unwrap();
    let first = session_groups::find_or_create(&borrowed_name);
    drop(borrowed_name);
    assert_eq!(session_group_name(&first).as_bytes(), b"gr\xffup");
    assert_eq!(session_groups::find_or_create(c"gr\xffup"), first);
    assert_eq!(session_groups::find(c"gr\xffup"), first);
    assert!(!session_groups::find(c"missing").is_alive());
    assert!(session_group_members(&first).is_empty());
    assert!(session_groups::remove(&first));
    assert!(!first.is_alive());
    assert!(session_groups::is_empty());
}
