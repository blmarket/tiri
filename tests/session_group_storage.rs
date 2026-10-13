use hmux::src::session::*;
use std::ffi::CString;

#[test]
fn named_group_owns_borrowed_name_and_reuses_stable_allocation() {
    unsafe {
        let head = &raw mut session_groups;
        assert!((*head).storage.is_empty());
        let borrowed_name = CString::new(b"gr\xffup".to_vec()).unwrap();
        let first = session_group_new(&borrowed_name);
        drop(borrowed_name);
        assert_eq!((*first).name.as_c_str().to_bytes(), b"gr\xffup");
        assert_eq!(session_group_new(c"gr\xffup"), first);
        assert_eq!(session_group_find(c"gr\xffup"), first);
        assert!(session_group_find(c"missing").is_null());
        assert!(session_group_members(first).is_empty());
        assert!(session_groups_remove(first));
        assert!((*head).storage.is_empty());
    }
}
