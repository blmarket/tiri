//! macOS implementation of the operating-system boundary.

use std::ffi::{OsStr, OsString};
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;

use super::Platform;

/// The macOS platform implementation selected by the native server.
pub struct Darwin;

impl Platform for Darwin {
    fn pane_cwd(pty: BorrowedFd<'_>) -> Option<PathBuf> {
        let foreground_pgrp = crate::terminal_foreground_group(pty).unwrap_or(-1);
        if foreground_pgrp <= 0 {
            return None;
        }

        process_vnode_cwd(foreground_pgrp)
    }

    fn pane_name(pty: BorrowedFd<'_>) -> Option<OsString> {
        let foreground_pgrp = crate::terminal_foreground_group(pty).ok()?;
        let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
        let written = unsafe {
            libc::proc_pidinfo(
                foreground_pgrp,
                libc::PROC_PIDT_SHORTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdshortinfo).cast(),
                size,
            )
        };
        if written != size {
            return None;
        }
        let name = bytes_from_c_buffer(&info.pbsi_comm, info.pbsi_comm.len());
        (!name.is_empty()).then(|| name.to_owned())
    }

    fn process_arguments(pid: u32) -> Vec<OsString> {
        procargs2(pid)
            .map(|buffer| parse_procargs2(&buffer))
            .unwrap_or_default()
    }
}

/// Fetch the raw `KERN_PROCARGS2` buffer for `pid`: `argc`, the exec path, then
/// the argument strings, then the environment strings, all in one sysctl.
fn procargs2(pid: u32) -> Option<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut size = 0usize;
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            ptr::null_mut(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
        || size < std::mem::size_of::<libc::c_int>()
    {
        return None;
    }

    let mut buffer = vec![0u8; size];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            buffer.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    buffer.truncate(size);
    Some(buffer)
}

fn process_vnode_cwd(pid: libc::pid_t) -> Option<PathBuf> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&mut info as *mut libc::proc_vnodepathinfo).cast(),
            size,
        )
    };
    (written == size)
        .then(|| path_from_vnode_info(&info.pvi_cdir))
        .flatten()
}

fn path_from_vnode_info(info: &libc::vnode_info_path) -> Option<PathBuf> {
    let path = &info.vip_path;
    let bytes = unsafe {
        std::slice::from_raw_parts(path.as_ptr().cast::<u8>(), std::mem::size_of_val(path))
    };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    if end == 0 {
        return None;
    }
    Some(PathBuf::from(OsStr::from_bytes(&bytes[..end])))
}

fn bytes_from_c_buffer(buffer: &[libc::c_char], length: usize) -> &OsStr {
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), buffer.len()) };
    let end = bytes[..length.min(bytes.len())]
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(length.min(bytes.len()));
    OsStr::from_bytes(&bytes[..end])
}

fn parse_procargs2(buffer: &[u8]) -> Vec<OsString> {
    let Some(argc_bytes) = buffer.get(..std::mem::size_of::<libc::c_int>()) else {
        return Vec::new();
    };
    let argc = libc::c_int::from_ne_bytes(argc_bytes.try_into().unwrap());
    if argc <= 0 {
        return Vec::new();
    }

    let mut index = std::mem::size_of::<libc::c_int>();
    while index < buffer.len() && buffer[index] != 0 {
        index += 1;
    }
    while index < buffer.len() && buffer[index] == 0 {
        index += 1;
    }

    let mut args = Vec::new();
    while args.len() < argc as usize && index < buffer.len() {
        while index < buffer.len() && buffer[index] == 0 {
            index += 1;
        }
        let start = index;
        while index < buffer.len() && buffer[index] != 0 {
            index += 1;
        }
        if start < index {
            args.push(OsString::from(OsStr::from_bytes(&buffer[start..index])));
        }
    }

    args
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsFd;

    use super::{Darwin, Platform, parse_procargs2};

    #[test]
    fn procargs2_extracts_arguments_before_environment() {
        let mut buffer = 2i32.to_ne_bytes().to_vec();
        buffer.extend_from_slice(b"/bin/prog\0\0\0\0prog\0x=y\0A=1\0B=\0\0\0");
        let args = parse_procargs2(&buffer);
        assert_eq!(args, ["prog", "x=y"]);
    }

    #[test]
    fn pane_cwd_absent_for_non_tty() {
        let (read, _write) = std::io::pipe().unwrap();
        assert!(Darwin::pane_cwd(read.as_fd()).is_none());
    }
}
