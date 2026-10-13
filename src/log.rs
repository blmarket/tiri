use crate::src::compat::vis::strvis;
use crate::src::ffi::libc::{__errno_location, exit, getpid, snprintf, strerror};
use crate::src::format::bytes::format_bytes;
use crate::src::format::bytes::try_format_message_with;
use crate::src::shared::abi::*;
use crate::src::shared::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB};
use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::time::{SystemTime, UNIX_EPOCH};

mod format;
pub use format::{log_byte, log_bytes, log_cstr, log_cstr_n, log_cstr_width, log_hex, log_pointer};

thread_local! {
    static LOG_FILE: RefCell<Option<File>> = const { RefCell::new(None) };
    static LOG_LEVEL: Cell<::core::ffi::c_int> = const { Cell::new(0) };
}

fn log_enabled() -> bool {
    LOG_FILE.with_borrow(Option::is_some)
}
fn log_write(message: &[u8], prefix: &[u8]) {
    LOG_FILE.with_borrow_mut(|file| {
        if let Some(file) = file {
            log_write_escaped(file, message, prefix);
        }
    });
}
pub unsafe fn log_add_level() {
    LOG_LEVEL.set(LOG_LEVEL.get() + 1);
}
pub unsafe fn log_get_level() -> ::core::ffi::c_int {
    LOG_LEVEL.get()
}
pub unsafe fn log_open(name: &CStr) {
    if LOG_LEVEL.get() == 0 as ::core::ffi::c_int {
        return;
    }
    log_close();
    let pid = (getpid() as ::core::ffi::c_long).to_string();
    let mut path = b"tmux-".to_vec();
    path.extend_from_slice(name.to_bytes());
    path.push(b'-');
    path.extend_from_slice(pid.as_bytes());
    path.extend_from_slice(b".log");
    let path = std::ffi::OsStr::from_bytes(&path);
    let Ok(file) = OpenOptions::new().append(true).create(true).open(path) else {
        return;
    };
    LOG_FILE.set(Some(file));
}
pub unsafe fn log_toggle(name: &CStr) {
    if LOG_LEVEL.get() == 0 as ::core::ffi::c_int {
        LOG_LEVEL.set(1 as ::core::ffi::c_int);
        log_open(name);
        log_debug(format_args!("log opened"));
    } else {
        log_debug(format_args!("log closed"));
        LOG_LEVEL.set(0 as ::core::ffi::c_int);
        log_close();
    };
}
pub unsafe fn log_close() {
    LOG_FILE.set(None);
}
unsafe fn log_message(
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    prefix: &CStr,
) {
    if !log_enabled() {
        return;
    }
    let Some(s) = try_format_message_with(write) else {
        return;
    };
    // strvis writes at most four bytes per input byte plus the terminator.
    let Some(capacity) = s
        .as_bytes()
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(4))
    else {
        return;
    };
    let mut out = Vec::<u8>::new();
    if out.try_reserve_exact(capacity).is_err() {
        return;
    }
    out.resize(capacity, 0);
    strvis(
        out.as_mut_ptr().cast(),
        s.as_ptr(),
        VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL,
    );
    drop(s);
    let message = CStr::from_bytes_until_nul(&out).unwrap();
    log_write(message.to_bytes(), prefix.to_bytes());
}

/// Write one complete log line with a single write, so the line reaches the
/// file immediately and is never interleaved with another process's output.
fn log_write_escaped(file: &mut File, message: &[u8], prefix: &[u8]) {
    let tv = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut line = format!("{}.{:06} ", tv.as_secs(), tv.subsec_micros()).into_bytes();
    line.extend_from_slice(prefix);
    line.extend_from_slice(message);
    line.push(b'\n');
    let _ = file.write_all(&line);
}

pub fn log_debug(args: fmt::Arguments<'_>) {
    if !log_enabled() {
        return;
    }
    let mut message = format_bytes(args);
    if let Some(end) = message.iter().position(|&byte| byte == 0) {
        message.truncate(end);
    }
    log_write(&message, b"");
}

pub unsafe fn fatal(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) -> ! {
    let mut tmp: [::core::ffi::c_char; 256] = [0; 256];
    if snprintf(
        &raw mut tmp as *mut ::core::ffi::c_char,
        ::core::mem::size_of::<[::core::ffi::c_char; 256]>() as size_t,
        c"fatal: %s: ".as_ptr(),
        strerror(*__errno_location()),
    ) < 0 as ::core::ffi::c_int
    {
        exit(1 as ::core::ffi::c_int);
    }
    log_message(
        write,
        CStr::from_ptr(&raw mut tmp as *mut ::core::ffi::c_char),
    );
    exit(1 as ::core::ffi::c_int);
}
pub unsafe fn fatalx(write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>) -> ! {
    log_message(write, c"fatal: ");
    exit(1 as ::core::ffi::c_int);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::fd::FromRawFd;

    // Keep the comparison against libc without defining a variadic Rust function.
    macro_rules! legacy_message {
        ($format:expr $(, $arg:expr)* $(,)?) => {{
            let count = snprintf(std::ptr::null_mut(), 0, $format $(, $arg)*);
            assert!(count >= 0);
            let mut raw = vec![0u8; count as usize + 1];
            assert_eq!(snprintf(raw.as_mut_ptr().cast(), raw.len(), $format $(, $arg)*), count);
            let raw = CStr::from_ptr(raw.as_ptr().cast());
            let mut escaped = vec![0u8; raw.to_bytes().len() * 4 + 1];
            strvis(escaped.as_mut_ptr().cast(), raw.as_ptr(), VIS_CSTYLE | VIS_NL | VIS_OCTAL | VIS_TAB);
            escaped.truncate(escaped.iter().position(|&byte| byte == 0).unwrap());
            escaped
        }};
    }

    fn message(args: fmt::Arguments<'_>) -> Vec<u8> {
        let mut bytes = format_bytes(args);
        if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
            bytes.truncate(end);
        }
        bytes
    }

    #[test]
    fn debug_arguments_match_legacy_escaping_for_every_byte() {
        unsafe {
            for byte in 0..=255u8 {
                let bytes = [byte, b'7', b'\\', b'\n', 0];
                assert_eq!(
                    message(format_args!(
                        "before {} after",
                        log_cstr(CStr::from_bytes_until_nul(&bytes).unwrap())
                    )),
                    legacy_message!(c"before %s after".as_ptr(), bytes.as_ptr()),
                    "C string byte {byte}"
                );
                assert_eq!(
                    message(format_args!("before {}7 after", log_byte(byte))),
                    legacy_message!(c"before %c7 after".as_ptr(), byte as i32),
                    "character byte {byte}"
                );
                assert_eq!(
                    message(format_args!("before {} after", log_bytes(&bytes))),
                    legacy_message!(c"before %s after".as_ptr(), bytes.as_ptr()),
                    "slice byte {byte}"
                );
            }
        }
    }

    #[test]
    fn bounded_strings_and_padding_match_printf_before_escaping() {
        unsafe {
            // No NUL terminator: the slice bounds the read like a precision.
            let bytes = [0xffu8, b'\\', b'\n', 0xc3];
            for precision in 0..=bytes.len() as i32 {
                assert_eq!(
                    message(format_args!(
                        "[{}]",
                        log_cstr_n(&bytes[..precision as usize])
                    )),
                    legacy_message!(c"[%.*s]".as_ptr(), precision, bytes.as_ptr())
                );
            }
            let bytes = c"\xff\\\n";
            let with_nul = bytes.to_bytes_with_nul();
            for precision in [-1, 0, 1, 2, 3, 20] {
                let limit = if precision < 0 {
                    with_nul.len()
                } else {
                    (precision as usize).min(with_nul.len())
                };
                assert_eq!(
                    message(format_args!("[{}]", log_cstr_n(&with_nul[..limit]))),
                    legacy_message!(c"[%.*s]".as_ptr(), precision, bytes.as_ptr())
                );
            }
            for width in [-8, -1, 0, 1, 8] {
                assert_eq!(
                    message(format_args!("[{}]", log_cstr_width(bytes, width))),
                    legacy_message!(c"[%*s]".as_ptr(), width, bytes.as_ptr())
                );
            }
        }
    }

    #[test]
    fn pointer_and_hex_spelling_match_printf() {
        unsafe {
            let byte = 0u8;
            let ptr = (&raw const byte).cast::<::core::ffi::c_void>();
            assert_eq!(
                message(format_args!("{}", log_pointer(ptr))),
                legacy_message!(c"%p".as_ptr(), ptr)
            );
            // Logs keep glibc's null spelling; other libcs print it differently.
            assert_eq!(
                message(format_args!(
                    "{}",
                    log_pointer(std::ptr::null::<::core::ffi::c_void>())
                )),
                b"(nil)"
            );
            for value in [0u64, 1, 0xff, u64::MAX] {
                assert_eq!(
                    message(format_args!("{}", log_hex(value))),
                    legacy_message!(c"%#llx".as_ptr(), value)
                );
            }
        }
    }

    struct RestoreLog(Option<File>);
    impl Drop for RestoreLog {
        fn drop(&mut self) {
            LOG_FILE.set(self.0.take());
        }
    }

    #[test]
    fn disabled_debug_does_not_format_arguments() {
        struct MustNotFormat;
        impl fmt::Display for MustNotFormat {
            fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
                panic!("disabled logger formatted an argument")
            }
        }
        unsafe {
            let _restore = RestoreLog(LOG_FILE.replace(None));
            log_debug(format_args!("{}", MustNotFormat));
            log_message(
                |_| panic!("disabled logger invoked the message writer"),
                c"fatal: ",
            );
        }
    }

    #[test]
    fn debug_sink_writes_one_escaped_line_and_preserves_nul_truncation() {
        unsafe {
            let file = libc::tmpfile();
            assert!(!file.is_null());
            let fd = libc::dup(libc::fileno(file));
            assert!(fd >= 0);
            libc::fclose(file);
            let mut reader = File::from_raw_fd(fd);
            let writer = reader.try_clone().unwrap();
            let _restore = RestoreLog(LOG_FILE.replace(Some(writer)));
            log_debug(format_args!("packet: {}", log_bytes(b"\x80\n\\")));
            log_debug(format_args!("stop{}ignored", log_byte(0)));
            reader.seek(SeekFrom::Start(0)).unwrap();
            let mut output = Vec::new();
            reader.read_to_end(&mut output).unwrap();
            let lines: Vec<_> = output.split(|&byte| byte == b'\n').collect();
            assert_eq!(lines.len(), 3);
            assert!(lines[0].ends_with(b" packet: \\200\\n\\\\"));
            assert!(lines[1].ends_with(b" stop"));
            assert!(lines[2].is_empty());
        }
    }
}
