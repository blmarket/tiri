use crate::src::shared::abi::*;
use crate::src::shared::abi::{gid_t, socklen_t, uid_t};
use crate::src::shared::socket::SOL_SOCKET;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct ucred {
    pub pid: pid_t,
    pub uid: uid_t,
    pub gid: gid_t,
}

pub const SO_PEERCRED: ::core::ffi::c_int = 17 as ::core::ffi::c_int;
pub unsafe fn getpeereid(
    mut s: ::core::ffi::c_int,
    mut uid: *mut uid_t,
    mut gid: *mut gid_t,
) -> ::core::ffi::c_int {
    if s < 0 {
        errno::set_errno(errno::Errno(libc::EBADF));
        return -1;
    }
    crate::src::reactor::io_status(
        tiri_rt::unix::peer_credentials(std::os::fd::BorrowedFd::borrow_raw(s)).map(
            |(user, group)| {
                *uid = user;
                *gid = group;
            },
        ),
    )
}
