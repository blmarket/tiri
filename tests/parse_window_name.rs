use tiri::src::names::parse_window_name_cstring;
use std::ffi::CStr;

unsafe fn parsed(input: &[u8]) -> Vec<u8> {
    let end = input.iter().position(|&byte| byte == 0).unwrap();
    let input = CStr::from_bytes_with_nul(&input[..=end]).unwrap();
    parse_window_name_cstring(input).into_bytes()
}

#[test]
fn parse_window_name_handles_shell_command_forms() {
    unsafe {
        assert_eq!(parsed(b"\"exec -/usr/bin/vim -u NONE\"\0"), b"vim");
        assert_eq!(parsed(b"  --/bin/zsh -l\0"), b"zsh");
        assert_eq!(parsed(b"exec /bin/bash.sh  --login\0"), b"bash.sh");
        assert_eq!(parsed(b"command!\t\n\0"), b"command!");
    }
}

#[test]
fn parse_window_name_stops_at_first_nul_and_returns_owned_result() {
    unsafe {
        assert_eq!(parsed(b"before\0after\0"), b"before");
        assert_eq!(parsed(b"ba\xffd\0"), b"");
        assert_eq!(parsed(b"\0"), b"");
    }
}
