//! Bounded interfaces to the platform's character escaping and classification.

use std::ffi::c_char;

/// vis writes at most four encoded bytes plus its trailing NUL.
pub(crate) fn vis_into(output: &mut [u8], byte: u8, flags: i32, next: u8) -> usize {
    assert!(output.len() >= 5, "vis needs four bytes plus a terminator");
    unsafe {
        let start = output.as_mut_ptr().cast::<c_char>();
        let end =
            crate::compat::vis::vis(start, byte as c_char as i32, flags, next as c_char as i32);
        end.offset_from(start) as usize
    }
}

pub(crate) fn is_alpha(byte: u8) -> bool {
    unsafe { libc::isalpha(i32::from(byte)) != 0 }
}
