//! Authoritative UTF-8 state values.

pub type utf8_state = ::core::ffi::c_uint;
pub const UTF8_ERROR: utf8_state = 2;
pub const UTF8_DONE: utf8_state = 1;
pub const UTF8_MORE: utf8_state = 0;

pub type hanguljamo_state = ::core::ffi::c_uint;
pub const HANGULJAMO_STATE_NOT_COMPOSABLE: hanguljamo_state = 3;
pub const HANGULJAMO_STATE_COMPOSABLE: hanguljamo_state = 2;
pub const HANGULJAMO_STATE_CHOSEONG: hanguljamo_state = 1;
pub const HANGULJAMO_STATE_NOT_HANGULJAMO: hanguljamo_state = 0;

pub type wchar_t = ::libc::wchar_t;

pub const UTF8_SIZE: ::core::ffi::c_int = 32 as ::core::ffi::c_int;

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, size_of};

    #[test]
    fn utf8_state_matches_translated_c_baseline() {
        assert_eq!(size_of::<utf8_state>(), 4);
        assert_eq!(align_of::<utf8_state>(), 4);
        assert_eq!(UTF8_MORE, 0);
        assert_eq!(UTF8_DONE, 1);
        assert_eq!(UTF8_ERROR, 2);
    }
}
