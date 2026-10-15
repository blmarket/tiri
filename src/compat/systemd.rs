use crate::src::ffi::libc::__errno_location;
use crate::src::ffi::systemd::{
    sd_bus, sd_bus_error, sd_bus_message, sd_bus_slot, sd_id128, sd_id128_t,
};
use crate::src::ffi::systemd::{
    sd_bus_call, sd_bus_default_user, sd_bus_error_free, sd_bus_match_signal,
    sd_bus_message_append, sd_bus_message_close_container, sd_bus_message_new_method_call,
    sd_bus_message_open_container, sd_bus_message_read, sd_bus_message_unref, sd_bus_process,
    sd_bus_slot_unref, sd_bus_unref, sd_bus_wait, sd_id128_randomize, sd_is_socket_unix,
    sd_listen_fds, sd_pid_get_unit, sd_pid_get_user_slice, sd_pid_get_user_unit,
};
use crate::src::server::server_create_socket;
use crate::src::shared::abi::uint32_t;
use crate::src::shared::abi::*;
use crate::src::shared::errno::E2BIG;
use crate::src::shared::socket::SOCK_STREAM;
use crate::src::tmux::socket_path;
use libc::{free, getpid, getppid, strcmp, strerror};
use std::ffi::{CStr, CString};
use std::time::Instant;

struct ForeignCString(*mut ::core::ffi::c_char);

impl ForeignCString {
    fn as_ptr(&self) -> *const ::core::ffi::c_char {
        self.0
    }
}

impl Drop for ForeignCString {
    fn drop(&mut self) {
        unsafe { free(self.0 as *mut ::core::ffi::c_void) }
    }
}

fn systemd_message(
    format: *const ::core::ffi::c_char,
    reason: Option<*const ::core::ffi::c_char>,
) -> CString {
    let template = unsafe { CStr::from_ptr(format) }.to_bytes();
    let mut message = Vec::with_capacity(template.len());
    if let Some(reason) = reason {
        let marker = template
            .windows(2)
            .position(|part| part == b"%s")
            .expect("systemd diagnostic must have a %s marker");
        message.extend_from_slice(&template[..marker]);
        if reason.is_null() {
            message.extend_from_slice(b"(null)");
        } else {
            message.extend_from_slice(unsafe { CStr::from_ptr(reason) }.to_bytes());
        }
        message.extend_from_slice(&template[marker + 2..]);
    } else {
        message.extend_from_slice(template);
    }
    CString::new(message).expect("systemd diagnostics contain no NUL")
}

macro_rules! set_systemd_error {
    ($destination:ident, $format:expr, $reason:expr $(,)?) => {
        $destination = Some(systemd_message($format, Some($reason)))
    };
    ($destination:ident, $format:expr $(,)?) => {
        $destination = Some(systemd_message($format, None))
    };
}

#[cfg(test)]
mod systemd_message_tests {
    use super::systemd_message;
    use std::ffi::CString;

    #[test]
    fn preserves_reason_bytes_and_null_rendering() {
        let format = c"systemd: %s";
        let reason = CString::new(b"unit-\xff".to_vec()).unwrap();
        assert_eq!(
            systemd_message(format.as_ptr(), Some(reason.as_ptr())).as_bytes(),
            b"systemd: unit-\xff"
        );
        assert_eq!(
            systemd_message(format.as_ptr(), Some(::core::ptr::null())).as_bytes(),
            b"systemd: (null)"
        );
    }
}

#[derive(Clone)]
#[repr(C)]
pub struct systemd_job_watch {
    pub path: Option<::std::ffi::CString>,
    pub done: ::core::ffi::c_int,
}
pub const EPFNOSUPPORT: ::core::ffi::c_int = 96 as ::core::ffi::c_int;

pub const SD_BUS_ERROR_NULL: sd_bus_error = sd_bus_error {
    name: ::core::ptr::null::<::core::ffi::c_char>(),
    message: ::core::ptr::null::<::core::ffi::c_char>(),
    _need_free: 0 as ::core::ffi::c_int,
};
pub const SD_LISTEN_FDS_START: ::core::ffi::c_int = 3 as ::core::ffi::c_int;

struct SystemdBusResources {
    error: *mut sd_bus_error,
    message: *mut *mut sd_bus_message,
    reply: *mut *mut sd_bus_message,
    slot: *mut *mut sd_bus_slot,
    bus: *mut *mut sd_bus,
}

impl Drop for SystemdBusResources {
    fn drop(&mut self) {
        unsafe {
            sd_bus_error_free(self.error);
            sd_bus_message_unref(*self.message);
            sd_bus_message_unref(*self.reply);
            sd_bus_slot_unref(*self.slot);
            sd_bus_unref(*self.bus);
        }
    }
}
pub unsafe fn systemd_activated() -> ::core::ffi::c_int {
    (sd_listen_fds(0 as ::core::ffi::c_int) >= 1 as ::core::ffi::c_int) as ::core::ffi::c_int
}
pub unsafe fn systemd_create_socket(
    mut flags: ::core::ffi::c_int,
) -> Result<::core::ffi::c_int, CString> {
    let mut fds: ::core::ffi::c_int = 0;
    let mut fd: ::core::ffi::c_int = 0;
    fds = sd_listen_fds(0 as ::core::ffi::c_int);
    if fds > 1 as ::core::ffi::c_int {
        *__errno_location() = E2BIG;
    } else if fds == 1 as ::core::ffi::c_int {
        fd = SD_LISTEN_FDS_START;
        if sd_is_socket_unix(
            fd,
            SOCK_STREAM as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            ::core::ptr::null::<::core::ffi::c_char>(),
            0 as size_t,
        ) == 0
        {
            *__errno_location() = EPFNOSUPPORT;
        } else {
            match tiri_rt::unix::socket_path(std::os::fd::BorrowedFd::borrow_raw(fd)) {
                Ok(path) => {
                    socket_path = Some(path.unwrap_or_default());
                    return Ok(fd);
                }
                Err(error) => *__errno_location() = error.raw_os_error().unwrap_or(libc::EIO),
            }
        }
    } else {
        return server_create_socket(flags as uint64_t);
    }
    let saved_errno = *__errno_location();
    let reason = CStr::from_ptr(strerror(saved_errno)).to_bytes();
    let mut message = b"systemd socket error (".to_vec();
    message.extend_from_slice(reason);
    message.push(b')');
    Err(CString::new(message).expect("strerror returns a C string"))
}
unsafe extern "C" fn job_removed_handler(
    mut m: *mut sd_bus_message,
    mut userdata: *mut ::core::ffi::c_void,
    _ret_error: *mut sd_bus_error,
) -> ::core::ffi::c_int {
    let mut watch: *mut systemd_job_watch = userdata as *mut systemd_job_watch;
    let mut path: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut id: uint32_t = 0;
    let mut r: ::core::ffi::c_int = 0;
    let Some(watch_path) = (*watch).path.as_ref() else {
        return 0 as ::core::ffi::c_int;
    };
    r = sd_bus_message_read(m, c"uo".as_ptr(), &raw mut id, &raw mut path);
    if r < 0 as ::core::ffi::c_int {
        return r;
    }
    if strcmp(path, watch_path.as_ptr()) == 0 as ::core::ffi::c_int {
        (*watch).done = 1 as ::core::ffi::c_int;
    }
    0 as ::core::ffi::c_int
}
pub unsafe fn systemd_move_to_new_cgroup() -> (::core::ffi::c_int, Option<CString>) {
    let mut current_block: u64;
    let mut error: sd_bus_error = SD_BUS_ERROR_NULL;
    let mut m: *mut sd_bus_message = ::core::ptr::null_mut::<sd_bus_message>();
    let mut reply: *mut sd_bus_message = ::core::ptr::null_mut::<sd_bus_message>();
    let mut bus: *mut sd_bus = ::core::ptr::null_mut::<sd_bus>();
    let mut slot: *mut sd_bus_slot = ::core::ptr::null_mut::<sd_bus_slot>();
    let mut cause: Option<CString> = None;
    let mut slice: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut job_path: *const ::core::ffi::c_char = ::core::ptr::null();
    let mut uuid: sd_id128_t = sd_id128 { bytes: [0; 16] };
    let mut r: ::core::ffi::c_int = 0;
    let mut elapsed_usec: uint64_t = 0;
    let mut pid: pid_t = 0;
    let mut parent_pid: pid_t = 0;
    let mut watch: systemd_job_watch = systemd_job_watch {
        path: None,
        done: 0,
    };
    let resources = SystemdBusResources {
        error: &raw mut error,
        message: &raw mut m,
        reply: &raw mut reply,
        slot: &raw mut slot,
        bus: &raw mut bus,
    };
    let start = Instant::now();
    r = sd_bus_default_user(&raw mut bus);
    if r < 0 as ::core::ffi::c_int {
        set_systemd_error!(
            cause,
            c"failed to connect to session bus: %s".as_ptr(),
            strerror(-r),
        );
    } else {
        r = sd_bus_match_signal(
            bus,
            &raw mut slot,
            c"org.freedesktop.systemd1".as_ptr(),
            c"/org/freedesktop/systemd1".as_ptr(),
            c"org.freedesktop.systemd1.Manager".as_ptr(),
            c"JobRemoved".as_ptr(),
            Some(
                job_removed_handler
                    as unsafe extern "C" fn(
                        *mut sd_bus_message,
                        *mut ::core::ffi::c_void,
                        *mut sd_bus_error,
                    ) -> ::core::ffi::c_int,
            ),
            &raw mut watch as *mut ::core::ffi::c_void,
        );
        if r < 0 as ::core::ffi::c_int {
            set_systemd_error!(
                cause,
                c"failed to create match signal: %s".as_ptr(),
                strerror(-r),
            );
        } else {
            r = sd_bus_message_new_method_call(
                bus,
                &raw mut m,
                c"org.freedesktop.systemd1".as_ptr(),
                c"/org/freedesktop/systemd1".as_ptr(),
                c"org.freedesktop.systemd1.Manager".as_ptr(),
                c"StartTransientUnit".as_ptr(),
            );
            if r < 0 as ::core::ffi::c_int {
                set_systemd_error!(
                    cause,
                    c"failed to create bus message: %s".as_ptr(),
                    strerror(-r),
                );
            } else {
                r = sd_id128_randomize(&raw mut uuid);
                if r < 0 as ::core::ffi::c_int {
                    set_systemd_error!(
                        cause,
                        c"failed to generate uuid: %s".as_ptr(),
                        strerror(-r),
                    );
                } else {
                    let hex = b"0123456789abcdef";
                    let mut scope = Vec::with_capacity(b"tmux-spawn-".len() + 36 + b".scope".len());
                    scope.extend_from_slice(b"tmux-spawn-");
                    for (i, byte) in uuid.bytes.iter().enumerate() {
                        if matches!(i, 4 | 6 | 8 | 10) {
                            scope.push(b'-');
                        }
                        scope.push(hex[(byte >> 4) as usize]);
                        scope.push(hex[(byte & 0x0f) as usize]);
                    }
                    scope.extend_from_slice(b".scope");
                    let name = CString::new(scope).expect("systemd scope name contains no NUL");
                    r = sd_bus_message_append(m, c"s".as_ptr(), name.as_ptr());
                    if r < 0 as ::core::ffi::c_int {
                        set_systemd_error!(
                            cause,
                            c"failed to append to bus message: %s".as_ptr(),
                            strerror(-r),
                        );
                    } else {
                        r = sd_bus_message_append(m, c"s".as_ptr(), c"fail".as_ptr());
                        if r < 0 as ::core::ffi::c_int {
                            set_systemd_error!(
                                cause,
                                c"failed to append to bus message: %s".as_ptr(),
                                strerror(-r),
                            );
                        } else {
                            r = sd_bus_message_open_container(
                                m,
                                'a' as i32 as ::core::ffi::c_char,
                                c"(sv)".as_ptr(),
                            );
                            if r < 0 as ::core::ffi::c_int {
                                set_systemd_error!(
                                    cause,
                                    c"failed to start properties array: %s".as_ptr(),
                                    strerror(-r),
                                );
                            } else {
                                pid = getpid() as pid_t;
                                parent_pid = getppid() as pid_t;
                                let desc = CString::new(format!(
                                    "tmux child pane {} launched by process {}",
                                    pid as ::core::ffi::c_long, parent_pid as ::core::ffi::c_long
                                ))
                                .expect("systemd pane description contains no NUL");
                                r = sd_bus_message_append(
                                    m,
                                    c"(sv)".as_ptr(),
                                    c"Description".as_ptr(),
                                    c"s".as_ptr(),
                                    desc.as_ptr(),
                                );
                                if r < 0 as ::core::ffi::c_int {
                                    set_systemd_error!(
                                        cause,
                                        c"failed to append to properties: %s".as_ptr(),
                                        strerror(-r),
                                    );
                                } else {
                                    r = sd_bus_message_append(
                                        m,
                                        c"(sv)".as_ptr(),
                                        c"SendSIGHUP".as_ptr(),
                                        c"b".as_ptr(),
                                        1 as ::core::ffi::c_int,
                                    );
                                    if r < 0 as ::core::ffi::c_int {
                                        set_systemd_error!(
                                            cause,
                                            c"failed to append to properties: %s".as_ptr(),
                                            strerror(-r),
                                        );
                                    } else {
                                        r = sd_pid_get_user_slice(parent_pid, &raw mut slice);
                                        let slice_owner = ForeignCString(slice);
                                        let fallback_slice = if r < 0 as ::core::ffi::c_int {
                                            Some(CString::new("app-tmux.slice").unwrap())
                                        } else {
                                            None
                                        };
                                        let slice_ptr = fallback_slice
                                            .as_ref()
                                            .map_or(slice_owner.as_ptr(), |value| value.as_ptr());
                                        r = sd_bus_message_append(
                                            m,
                                            c"(sv)".as_ptr(),
                                            c"Slice".as_ptr(),
                                            c"s".as_ptr(),
                                            slice_ptr,
                                        );
                                        drop(slice_owner);
                                        drop(fallback_slice);
                                        if r < 0 as ::core::ffi::c_int {
                                            set_systemd_error!(
                                                cause,
                                                c"failed to append to properties: %s".as_ptr(),
                                                strerror(-r),
                                            );
                                        } else {
                                            r = sd_bus_message_append(
                                                m,
                                                c"(sv)".as_ptr(),
                                                c"PIDs".as_ptr(),
                                                c"au".as_ptr(),
                                                1 as ::core::ffi::c_int,
                                                pid,
                                            );
                                            if r < 0 as ::core::ffi::c_int {
                                                set_systemd_error!(
                                                    cause,
                                                    c"failed to append to properties: %s".as_ptr(),
                                                    strerror(-r),
                                                );
                                            } else {
                                                r = sd_bus_message_append(
                                                    m,
                                                    c"(sv)".as_ptr(),
                                                    c"CollectMode".as_ptr(),
                                                    c"s".as_ptr(),
                                                    c"inactive-or-failed".as_ptr(),
                                                );
                                                if r < 0 as ::core::ffi::c_int {
                                                    set_systemd_error!(
                                                        cause,
                                                        c"failed to append to properties: %s"
                                                            .as_ptr(),
                                                        strerror(-r),
                                                    );
                                                } else {
                                                    let mut unit = ::core::ptr::null_mut::<
                                                        ::core::ffi::c_char,
                                                    >(
                                                    );
                                                    let mut have_unit = sd_pid_get_user_unit(
                                                        parent_pid,
                                                        &raw mut unit,
                                                    ) == 0
                                                        as ::core::ffi::c_int;
                                                    if !have_unit {
                                                        free(unit as *mut ::core::ffi::c_void);
                                                        unit = ::core::ptr::null_mut();
                                                        have_unit = sd_pid_get_unit(
                                                            parent_pid,
                                                            &raw mut unit,
                                                        ) == 0 as ::core::ffi::c_int;
                                                    }
                                                    let unit_owner = ForeignCString(unit);
                                                    if have_unit {
                                                        r = sd_bus_message_append(
                                                            m,
                                                            c"(sv)".as_ptr(),
                                                            c"Before".as_ptr(),
                                                            c"as".as_ptr(),
                                                            1 as ::core::ffi::c_int,
                                                            unit_owner.as_ptr(),
                                                        );
                                                        if r >= 0 as ::core::ffi::c_int {
                                                            r = sd_bus_message_append(
                                                                m,
                                                                c"(sv)".as_ptr(),
                                                                c"PartOf".as_ptr(),
                                                                c"as".as_ptr(),
                                                                1 as ::core::ffi::c_int,
                                                                unit_owner.as_ptr(),
                                                            );
                                                        }
                                                        drop(unit_owner);
                                                        if r < 0 as ::core::ffi::c_int {
                                                            set_systemd_error!(
                                                                cause,
                                                                c"failed to append to properties: %s".as_ptr(),
                                                                strerror(-r),
                                                            );
                                                            current_block = 3315597219737674933;
                                                        } else {
                                                            current_block = 1423531122933789233;
                                                        }
                                                    } else {
                                                        current_block = 1423531122933789233;
                                                    }
                                                    match current_block {
                                                        3315597219737674933 => {}
                                                        _ => {
                                                            r = sd_bus_message_close_container(m);
                                                            if r < 0 as ::core::ffi::c_int {
                                                                set_systemd_error!(
                                                                    cause,
                                                                    c"failed to end properties array: %s".as_ptr(),
                                                                    strerror(-r),
                                                                );
                                                            } else {
                                                                r = sd_bus_message_append(
                                                                    m,
                                                                    c"a(sa(sv))".as_ptr(),
                                                                    0 as ::core::ffi::c_int,
                                                                );
                                                                if r < 0 as ::core::ffi::c_int {
                                                                    set_systemd_error!(
                                                                        cause,
                                                                        c"failed to append to bus message: %s".as_ptr(),
                                                                        strerror(-r),
                                                                    );
                                                                } else {
                                                                    r = sd_bus_call(
                                                                        bus,
                                                                        m,
                                                                        1000000 as uint64_t,
                                                                        &raw mut error,
                                                                        &raw mut reply,
                                                                    );
                                                                    if r < 0 as ::core::ffi::c_int {
                                                                        if !error.message.is_null()
                                                                        {
                                                                            set_systemd_error!(
                                                                                cause,
                                                                                c"StartTransientUnit call failed: %s".as_ptr(),
                                                                                error.message,
                                                                            );
                                                                        } else {
                                                                            set_systemd_error!(
                                                                                cause,
                                                                                c"StartTransientUnit call failed: %s".as_ptr(),
                                                                                strerror(-r),
                                                                            );
                                                                        }
                                                                    } else {
                                                                        r = sd_bus_message_read(
                                                                            reply,
                                                                            c"o".as_ptr(),
                                                                            &raw mut job_path,
                                                                        );
                                                                        if r < 0
                                                                            as ::core::ffi::c_int
                                                                        {
                                                                            set_systemd_error!(
                                                                                cause,
                                                                                c"failed to parse method reply: %s".as_ptr(),
                                                                                strerror(-r),
                                                                            );
                                                                        } else {
                                                                            if !job_path.is_null() {
                                                                                watch.path = Some(
                                                                                    CStr::from_ptr(
                                                                                        job_path,
                                                                                    )
                                                                                    .to_owned(),
                                                                                );
                                                                            }
                                                                            while watch.done == 0 {
                                                                                r = sd_bus_process(
                                                                                    bus,
                                                                                    ::core::ptr::null_mut::<*mut sd_bus_message>(),
                                                                                );
                                                                                if r < 0 as ::core::ffi::c_int {
                                                                                    set_systemd_error!(
                                                                                        cause,
                                                                                        c"failed waiting for cgroup allocation: %s".as_ptr(),
                                                                                        strerror(-r),
                                                                                    );
                                                                                    break;
                                                                                } else {
                                                                                    if r > 0 as ::core::ffi::c_int {
                                                                                        continue;
                                                                                    }
                                                                                    elapsed_usec = start.elapsed().as_micros().min(u64::MAX as u128) as u64;
                                                                                    if elapsed_usec >= 1000000 as uint64_t {
                                                                                        set_systemd_error!(
                                                                                            cause,
                                                                                            c"timeout waiting for cgroup allocation".as_ptr(),
                                                                                        );
                                                                                        break;
                                                                                    } else {
                                                                                        r = sd_bus_wait(
                                                                                            bus,
                                                                                            (1000000 as uint64_t).wrapping_sub(elapsed_usec),
                                                                                        );
                                                                                        if !(r < 0 as ::core::ffi::c_int) {
                                                                                            continue;
                                                                                        }
                                                                                        set_systemd_error!(
                                                                                            cause,
                                                                                            c"failed waiting for cgroup allocation: %s".as_ptr(),
                                                                                            strerror(-r),
                                                                                        );
                                                                                        break;
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    drop(resources);
    (r, cause)
}
