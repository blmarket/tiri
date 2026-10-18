use std::ffi::CStr;
use tiri::tmux::clean_name_cstring;

unsafe fn cleaned(input: &[u8], untrusted: bool) -> Option<Vec<u8>> {
    let end = input.iter().position(|&byte| byte == 0).unwrap();
    let input = CStr::from_bytes_with_nul(&input[..=end]).unwrap();
    clean_name_cstring(input, i32::from(untrusted)).map(std::ffi::CString::into_bytes)
}

#[test]
fn clean_name_rewrites_only_untrusted_command_markers() {
    unsafe {
        let name = b"prefix#(command)# suffix\0";
        assert_eq!(
            cleaned(name, false),
            Some(b"prefix#(command)# suffix".to_vec())
        );
        assert_eq!(
            cleaned(name, true),
            Some(b"prefix_(command)# suffix".to_vec())
        );
        assert_eq!(cleaned(b"#(#(x)\0", true), Some(b"_(_(x)".to_vec()));
    }
}

#[test]
fn clean_name_uses_first_nul_and_rejects_invalid_utf8() {
    unsafe {
        assert_eq!(
            cleaned(b"before\0#(after)\0", true),
            Some(b"before".to_vec())
        );
        assert_eq!(
            cleaned("café#(x)\0".as_bytes(), true),
            Some("café_(x)".as_bytes().to_vec())
        );
        assert_eq!(cleaned(b"bad\xff#(x)\0", false), None);
        assert_eq!(cleaned(b"bad\xff#(x)\0", true), None);
    }
}
