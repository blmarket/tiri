use std::ffi::{CStr, CString};
/// The argv wire representation is malformed: the count is out of range or
/// the buffer holds fewer NUL-terminated arguments than declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidArgv;

/// Decode the length-delimited argv wire representation. The final byte is
/// forced to NUL before token scans, matching the old C decoder.
pub fn unpack_argv(
    buffer: &mut [u8],
    argc: ::core::ffi::c_int,
) -> Result<Vec<CString>, InvalidArgv> {
    if argc == 0 {
        return Ok(Vec::new());
    }
    if !(0..=1000).contains(&argc) || buffer.is_empty() {
        return Err(InvalidArgv);
    }
    *buffer.last_mut().expect("nonempty buffer") = 0;
    let mut result = Vec::with_capacity(argc as usize);
    let mut start = 0;
    for _ in 0..argc {
        if start >= buffer.len() {
            return Err(InvalidArgv);
        }
        let tail = &buffer[start..];
        let end = tail.iter().position(|byte| *byte == 0).ok_or(InvalidArgv)?;
        result.push(
            CStr::from_bytes_with_nul(&tail[..=end])
                .map_err(|_| InvalidArgv)?
                .to_owned(),
        );
        start += end + 1;
    }
    Ok(result)
}

#[cfg(test)]
mod owned_argv_tests {
    use super::unpack_argv;

    #[test]
    fn unpack_preserves_empty_and_non_utf8_arguments_and_terminates_view() {
        let mut bytes = [b'a', 0, 0, 0xff, 0, b'x'];
        let argv = unpack_argv(&mut bytes, 3).expect("three arguments fit");
        assert_eq!(bytes[5], 0);
        assert_eq!(argv.len(), 3);
        assert_eq!(argv[0].as_bytes(), b"a");
        assert_eq!(argv[1].as_bytes(), b"");
        assert_eq!(argv[2].as_bytes(), &[0xff]);
        assert!(unpack_argv(&mut [], 0)
            .expect("empty argv is valid")
            .is_empty());
    }

    #[test]
    fn unpack_rejects_more_arguments_than_the_wire_buffer_contains() {
        let mut bytes = [b'x', 0];
        assert!(unpack_argv(&mut bytes, 2).is_err());
    }
}
