//! Platform ABI declarations shared by all translated modules.
//!
//! These aliases were emitted identically by every C2Rust
//! translation unit.  Keeping one definition prevents otherwise identical C
//! function signatures from acquiring distinct Rust type identities.
//! Platform-dependent widths come from the libc crate.

pub type __u_char = ::core::ffi::c_uchar;
pub type __u_short = ::core::ffi::c_ushort;
pub type __u_int = ::core::ffi::c_uint;
pub type __uint8_t = u8;
pub type __uint64_t = u64;
pub type __pid_t = ::libc::pid_t;
pub type __time_t = ::libc::time_t;

pub type u_char = __u_char;
pub type u_short = __u_short;
pub type u_int = __u_int;
pub type pid_t = __pid_t;
pub type time_t = __time_t;
pub type size_t = usize;
pub type uint8_t = __uint8_t;
pub type uint64_t = __uint64_t;
pub type bitstr_t = ::core::ffi::c_uchar;

pub const NULL: *mut ::core::ffi::c_void = ::core::ptr::null_mut::<::core::ffi::c_void>();

pub type cc_t = ::libc::cc_t;
pub type tcflag_t = ::libc::tcflag_t;

pub type __uint16_t = u16;
pub type __uint32_t = u32;
pub type __uid_t = ::libc::uid_t;
pub type __socklen_t = ::libc::socklen_t;
pub type ssize_t = isize;
pub type socklen_t = __socklen_t;
pub type uint32_t = __uint32_t;
pub type uid_t = __uid_t;
pub type __int32_t = i32;
pub type __gid_t = ::libc::gid_t;
pub type __id_t = ::libc::id_t;
pub type id_t = __id_t;
pub type __size_t = usize;
pub type gid_t = __gid_t;
pub type __compar_fn_t = Option<
    unsafe extern "C" fn(
        *const ::core::ffi::c_void,
        *const ::core::ffi::c_void,
    ) -> ::core::ffi::c_int,
>;
pub type __int64_t = i64;
pub type int64_t = __int64_t;
pub type __mode_t = ::libc::mode_t;

#[cfg(test)]
mod tests {
    use super::NULL;

    #[test]
    fn null_matches_the_c_null_pointer_constant() {
        assert!(NULL.is_null());
    }
}
