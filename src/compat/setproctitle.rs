#[cfg(target_os = "linux")]
use crate::compat::getprogname::getprogname;

/// The kernel's 15-byte task name for `progname: title`, built as the C
/// code built it: the title is cut to 15 bytes, `snprintf` cut the joined
/// name to 15, and a name that did not fit is then cut at its last space.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn process_name(progname: &[u8], formatted: &[u8]) -> Vec<u8> {
    let title = &formatted[..formatted.len().min(15)];
    let mut name = [progname, b": ", title].concat();
    if name.len() >= 16 {
        name.truncate(15);
        if let Some(space) = name.iter().rposition(|&byte| byte == b' ') {
            name.truncate(space);
        }
    }
    name
}

#[cfg(target_os = "linux")]
pub fn setproctitle(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {
    let formatted = crate::format::bytes::format_message_with(write);
    let name = process_name(getprogname().to_bytes(), formatted.as_bytes());
    let mut comm = [0u8; 16];
    comm[..name.len()].copy_from_slice(&name);
    // SAFETY: PR_SET_NAME reads a NUL-terminated name of at most 16 bytes.
    unsafe {
        ::libc::prctl(::libc::PR_SET_NAME, comm.as_ptr());
    }
}
// Like tmux's compat fallback, platforms without PR_SET_NAME keep their title.
#[cfg(not(target_os = "linux"))]
pub fn setproctitle(_write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) {}

#[cfg(test)]
mod tests {
    use super::process_name;

    #[test]
    fn a_short_name_is_kept_whole() {
        assert_eq!(process_name(b"tiri", b"server"), b"tiri: server");
        assert_eq!(process_name(b"tiri", b"abcdefghi"), b"tiri: abcdefghi");
    }

    #[test]
    fn a_long_name_is_cut_at_fifteen_bytes_then_at_the_last_space() {
        assert_eq!(
            process_name(b"tiri", b"server (/tmp/tmux-1000/default)"),
            b"tiri: server"
        );
        assert_eq!(process_name(b"tiri", b"abcdefghij"), b"tiri:");
    }

    #[test]
    fn a_long_name_without_a_space_keeps_fifteen_bytes() {
        assert_eq!(
            process_name(b"a-very-long-program", b"client"),
            b"a-very-long-pro"
        );
    }
}
