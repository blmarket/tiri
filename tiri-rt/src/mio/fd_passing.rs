use std::io;
use std::mem::{size_of, zeroed};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use crate::Received;

use super::readiness::syscall_result;

const CONTROL_SIZE: usize = unsafe { libc::CMSG_SPACE(size_of::<libc::c_int>() as _) as usize };

// macOS installs every passed descriptor even when it truncates the control
// data, leaking those it cannot report. Receive the largest control message
// sendmsg accepts (one 2KiB cluster) so excess descriptors are adopted and closed.
#[cfg(target_vendor = "apple")]
const RECEIVE_CONTROL_SIZE: usize = 2048;
#[cfg(not(target_vendor = "apple"))]
const RECEIVE_CONTROL_SIZE: usize = CONTROL_SIZE;

// Align ancillary storage for cmsghdr; CMSG_DATA is accessed unaligned.
#[repr(C)]
union Control<const SIZE: usize> {
    _align: libc::cmsghdr,
    bytes: [u8; SIZE],
}

pub(super) fn supported(raw: RawFd) -> io::Result<bool> {
    let mut kind: libc::c_int = 0;
    let mut len = size_of_val(&kind) as libc::socklen_t;
    // SAFETY: the caller owns the socket; kind and len are valid outputs.
    if unsafe {
        libc::getsockopt(
            raw,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut kind as *mut libc::c_int).cast(),
            &mut len,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: zero is a valid initialization for sockaddr_storage.
    let mut address: libc::sockaddr_storage = unsafe { zeroed() };
    let mut len = size_of_val(&address) as libc::socklen_t;
    // SAFETY: address has room for any socket address; len describes it.
    if unsafe {
        libc::getsockname(
            raw,
            (&mut address as *mut libc::sockaddr_storage).cast(),
            &mut len,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(kind == libc::SOCK_STREAM && address.ss_family as libc::c_int == libc::AF_UNIX)
}

pub(super) fn read(raw: RawFd, buffer: &mut [u8]) -> io::Result<Received> {
    let mut control = Control {
        bytes: [0; RECEIVE_CONTROL_SIZE],
    };
    let mut iov = libc::iovec {
        iov_base: buffer.as_mut_ptr().cast(),
        iov_len: buffer.len().min(isize::MAX as usize),
    };
    // SAFETY: zero is valid for msghdr's optional pointers and lengths.
    let mut msg: libc::msghdr = unsafe { zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = (&mut control as *mut Control<RECEIVE_CONTROL_SIZE>).cast();
    msg.msg_controllen = RECEIVE_CONTROL_SIZE as _;
    // SAFETY: readiness validation ensures the socket is live; all output
    // pointers describe exclusive, correctly aligned storage. Every retry
    // constructs fresh storage and resets the msghdr output lengths.
    #[cfg(target_vendor = "apple")]
    let flags = 0;
    #[cfg(not(target_vendor = "apple"))]
    let flags = libc::MSG_CMSG_CLOEXEC;
    let bytes = syscall_result(unsafe { libc::recvmsg(raw, &mut msg, flags) })?;
    let mut fd = None;
    let mut excess = false;
    // SAFETY: the kernel supplied valid control headers in our buffer. Adopt
    // every delivered FD before inspecting truncation, so errors close them.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&msg);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(header).cast::<libc::c_int>();
                let len = (*header).cmsg_len as usize - libc::CMSG_LEN(0) as usize;
                for index in 0..len / size_of::<libc::c_int>() {
                    let received = OwnedFd::from_raw_fd(data.add(index).read_unaligned());
                    if fd.is_none() {
                        fd = Some(received);
                    } else {
                        excess = true;
                        drop(received);
                    }
                }
            }
            header = libc::CMSG_NXTHDR(&msg, header);
        }
    }
    // Alignment padding may fit a second FD without setting MSG_CTRUNC.
    if excess || msg.msg_flags & libc::MSG_CTRUNC != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ancillary data exceeds single-FD receive capacity",
        ));
    }
    #[cfg(target_vendor = "apple")]
    if let Some(fd) = &fd {
        // macOS has no MSG_CMSG_CLOEXEC. Adopt every descriptor before changing
        // flags so all delivered FDs close even if this operation fails.
        // SAFETY: fd is owned and live.
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(Received { bytes, fd })
}

pub(super) fn write(
    raw: RawFd,
    iov: &mut [libc::iovec],
    fd: Option<BorrowedFd<'_>>,
) -> io::Result<usize> {
    let mut control = Control {
        bytes: [0; CONTROL_SIZE],
    };
    // SAFETY: zero initializes unused msghdr fields.
    let mut msg: libc::msghdr = unsafe { zeroed() };
    msg.msg_iov = iov.as_mut_ptr();
    msg.msg_iovlen = iov.len() as _;
    if let Some(fd) = fd {
        msg.msg_control = (&mut control as *mut Control<CONTROL_SIZE>).cast();
        // SAFETY: Control has space for one FD. Header and data lie within
        // that aligned allocation.
        unsafe {
            let bytes = size_of::<libc::c_int>() as _;
            msg.msg_controllen = libc::CMSG_SPACE(bytes) as _;
            let header = libc::CMSG_FIRSTHDR(&msg);
            (*header).cmsg_len = libc::CMSG_LEN(bytes) as _;
            (*header).cmsg_level = libc::SOL_SOCKET;
            (*header).cmsg_type = libc::SCM_RIGHTS;
            let data = libc::CMSG_DATA(header).cast::<libc::c_int>();
            data.write_unaligned(fd.as_raw_fd());
        }
    }
    // SAFETY: readiness validation ensures the socket is live; the caller
    // keeps all byte buffers and the borrowed descriptor live through sendmsg.
    syscall_result(unsafe { libc::sendmsg(raw, &msg, libc::MSG_NOSIGNAL) })
}
