use crate::format::bytes::{format_message_with, write_cstr};
use crate::server::server_create_socket;
use crate::shared::abi::uint64_t;
use crate::shared::errno::ErrorText;
use crate::tmux::set_socket_path;
use ::systemd::daemon::{self, Listening, SocketType};
use ::systemd::id128::Id128;
use ::systemd::login::{self, UnitType};
use libc::E2BIG;
use libc::{getpid, getppid};
use std::cell::{Cell, OnceCell};
use std::ffi::{CStr, CString};
use std::io;
use std::time::{Duration, Instant};
use tiri_sdbus::{Bus, Variant};

pub const EPFNOSUPPORT: ::core::ffi::c_int = 96 as ::core::ffi::c_int;
pub const SD_LISTEN_FDS_START: ::core::ffi::c_int = 3 as ::core::ffi::c_int;

const SYSTEMD_TIMEOUT: Duration = Duration::from_secs(1);

macro_rules! systemd_error {
    ($($arg:tt)*) => {
        format_message_with(|out| write!(out, $($arg)*))
    };
}

pub fn systemd_activated() -> bool {
    daemon::listen_fds(false).is_ok_and(|fds| !fds.is_empty())
}
pub unsafe fn systemd_create_socket(
    flags: ::core::ffi::c_int,
) -> Result<::core::ffi::c_int, CString> {
    let fds = daemon::listen_fds(false).map_or(0, |fds| fds.len());
    let error = if fds > 1 {
        E2BIG
    } else if fds == 1 {
        let fd = SD_LISTEN_FDS_START;
        if matches!(
            daemon::is_socket_unix(
                fd,
                Some(SocketType::Stream),
                Listening::IsListening,
                None::<&CStr>,
            ),
            Ok(false)
        ) {
            EPFNOSUPPORT
        } else {
            match tiri_rt::unix::socket_path(std::os::fd::BorrowedFd::borrow_raw(fd)) {
                Ok(path) => {
                    set_socket_path(path.unwrap_or_default());
                    return Ok(fd);
                }
                Err(error) => error.raw_os_error().unwrap_or(libc::EIO),
            }
        }
    } else {
        return server_create_socket(flags as uint64_t);
    };
    Err(systemd_error!(
        "systemd socket error ({})",
        ErrorText(&io::Error::from_raw_os_error(error))
    ))
}

fn scope_name(uuid: &[u8; 16]) -> CString {
    let hex = b"0123456789abcdef";
    let mut scope = Vec::with_capacity(b"tmux-spawn-".len() + 36 + b".scope".len());
    scope.extend_from_slice(b"tmux-spawn-");
    for (i, byte) in uuid.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            scope.push(b'-');
        }
        scope.push(hex[(byte >> 4) as usize]);
        scope.push(hex[(byte & 0x0f) as usize]);
    }
    scope.extend_from_slice(b".scope");
    CString::new(scope).expect("systemd scope name contains no NUL")
}

fn owned_cstring(value: String) -> CString {
    CString::new(value).expect("libsystemd strings contain no NUL")
}

const JOB_REMOVED_MATCH: &CStr = c"type='signal',sender='org.freedesktop.systemd1',path='/org/freedesktop/systemd1',interface='org.freedesktop.systemd1.Manager',member='JobRemoved'";

/// Move the calling (forked pane) process into a new transient scope unit and
/// wait for systemd to finish creating it.
pub fn systemd_move_to_new_cgroup() -> Result<(), CString> {
    let start = Instant::now();
    let job: OnceCell<CString> = OnceCell::new();
    let done = Cell::new(false);

    let bus = Bus::default_user()
        .map_err(|e| systemd_error!("failed to connect to session bus: {}", ErrorText(&e)))?;
    bus.with_match(
        JOB_REMOVED_MATCH,
        |m| {
            let Some(job) = job.get() else {
                return Ok(());
            };
            m.read_u32()?;
            if m.read_object_path()? == job.as_c_str() {
                done.set(true);
            }
            Ok(())
        },
        || {
            let mut m = bus
                .new_method_call(
                    c"org.freedesktop.systemd1",
                    c"/org/freedesktop/systemd1",
                    c"org.freedesktop.systemd1.Manager",
                    c"StartTransientUnit",
                )
                .map_err(|e| systemd_error!("failed to create bus message: {}", ErrorText(&e)))?;
            let uuid = Id128::from_random()
                .map_err(|e| systemd_error!("failed to generate uuid: {}", ErrorText(&e)))?;
            let append_error =
                |e: io::Error| systemd_error!("failed to append to bus message: {}", ErrorText(&e));
            m.append_str(&scope_name(uuid.as_bytes()))
                .map_err(append_error)?;
            m.append_str(c"fail").map_err(append_error)?;
            m.open_container(b'a', c"(sv)").map_err(|e| {
                systemd_error!("failed to start properties array: {}", ErrorText(&e))
            })?;

            let pid = unsafe { getpid() };
            let parent_pid = unsafe { getppid() };
            let description = owned_cstring(format!(
                "tmux child pane {} launched by process {}",
                pid, parent_pid
            ));
            let slice = login::get_slice(UnitType::UserUnit, Some(parent_pid))
                .map_or_else(|_| c"app-tmux.slice".to_owned(), owned_cstring);
            let unit = login::get_unit(UnitType::UserUnit, Some(parent_pid))
                .or_else(|_| login::get_unit(UnitType::SystemUnit, Some(parent_pid)))
                .ok()
                .map(owned_cstring);
            let units = unit.as_deref().map(|unit| [unit]);

            let property_error =
                |e: io::Error| systemd_error!("failed to append to properties: {}", ErrorText(&e));
            m.append_property(c"Description", Variant::Str(&description))
                .map_err(property_error)?;
            m.append_property(c"SendSIGHUP", Variant::Bool(true))
                .map_err(property_error)?;
            m.append_property(c"Slice", Variant::Str(&slice))
                .map_err(property_error)?;
            m.append_property(c"PIDs", Variant::U32Array(&[pid as u32]))
                .map_err(property_error)?;
            m.append_property(c"CollectMode", Variant::Str(c"inactive-or-failed"))
                .map_err(property_error)?;
            if let Some(units) = &units {
                m.append_property(c"Before", Variant::StrArray(units))
                    .map_err(property_error)?;
                m.append_property(c"PartOf", Variant::StrArray(units))
                    .map_err(property_error)?;
            }
            m.close_container()
                .map_err(|e| systemd_error!("failed to end properties array: {}", ErrorText(&e)))?;
            m.open_container(b'a', c"(sa(sv))")
                .and_then(|()| m.close_container())
                .map_err(append_error)?;

            let mut reply = bus.call(&m, SYSTEMD_TIMEOUT).map_err(|e| match e.message {
                Some(message) => format_message_with(|out| {
                    out.write_all(b"StartTransientUnit call failed: ")?;
                    write_cstr(out, message.as_c_str())
                }),
                None => systemd_error!("StartTransientUnit call failed: {}", ErrorText(&e.error)),
            })?;
            let job_path = reply
                .read_object_path()
                .map_err(|e| systemd_error!("failed to parse method reply: {}", ErrorText(&e)))?
                .to_owned();
            job.set(job_path).expect("job path is set once");

            let wait_error = |e: io::Error| {
                systemd_error!("failed waiting for cgroup allocation: {}", ErrorText(&e))
            };
            while !done.get() {
                if bus.process().map_err(wait_error)? {
                    continue;
                }
                let elapsed = start.elapsed();
                if elapsed >= SYSTEMD_TIMEOUT {
                    return Err(systemd_error!("timeout waiting for cgroup allocation"));
                }
                bus.wait(SYSTEMD_TIMEOUT - elapsed).map_err(wait_error)?;
            }
            Ok(())
        },
    )
    .map_err(|e| systemd_error!("failed to create match signal: {}", ErrorText(&e)))?
}
