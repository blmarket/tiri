//! Platform ABI declarations shared by all translated modules.
//!
//! These aliases were emitted identically by every C2Rust
//! translation unit.  Keeping one definition prevents otherwise identical C
//! function signatures from acquiring distinct Rust type identities.

pub type __u_char = ::core::ffi::c_uchar;
pub type __u_short = ::core::ffi::c_ushort;
pub type __u_int = ::core::ffi::c_uint;
pub type __uint8_t = u8;
pub type __uint64_t = u64;
pub type __pid_t = ::core::ffi::c_int;
pub type __time_t = ::core::ffi::c_long;

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

pub type cc_t = ::core::ffi::c_uchar;
pub type speed_t = ::core::ffi::c_uint;
pub type tcflag_t = ::core::ffi::c_uint;

pub type __off_t = ::core::ffi::c_long;
pub type __off64_t = ::core::ffi::c_long;
pub type __uint16_t = u16;
pub type __uint32_t = u32;
pub type __uid_t = ::core::ffi::c_uint;
pub type __clock_t = ::core::ffi::c_long;
pub type __socklen_t = ::core::ffi::c_uint;
pub type ssize_t = isize;
pub type socklen_t = __socklen_t;
pub type uint32_t = __uint32_t;
pub type uint16_t = __uint16_t;
pub type uid_t = __uid_t;
pub type __int32_t = i32;
pub type __gid_t = ::core::ffi::c_uint;
pub type __id_t = ::core::ffi::c_uint;
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
pub type __dev_t = ::core::ffi::c_ulong;
pub type __ino_t = ::core::ffi::c_ulong;
pub type __mode_t = ::core::ffi::c_uint;
pub type __nlink_t = ::core::ffi::c_ulong;
pub type __blksize_t = ::core::ffi::c_long;
pub type __blkcnt_t = ::core::ffi::c_long;
pub type __syscall_slong_t = ::core::ffi::c_long;
pub type __clockid_t = ::core::ffi::c_int;
pub type clockid_t = __clockid_t;

pub const NULL_0: *mut ::core::ffi::c_void = ::core::ptr::null_mut::<::core::ffi::c_void>();

#[cfg(test)]
mod tests {
    use super::NULL;

    #[test]
    fn null_matches_the_c_null_pointer_constant() {
        assert!(NULL.is_null());
    }
}
