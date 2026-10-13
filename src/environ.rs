use crate::src::ffi::libc::{environ, fnmatch, free, getpid, setenv};
use crate::src::format::bytes::format_message_with;
use crate::src::format::bytes::write_cstr;
use crate::src::log::{log_cstr, log_debug};
use crate::src::options::options_get_string;
use crate::src::session::Session;
use crate::src::shared::session::SessionRef;
use crate::src::tmux::{getversion, global_environ, global_options, socket_path_cstr};
use crate::src::xmalloc::xcalloc;
use std::ffi::{CStr, CString, NulError};

struct ProcessEnvironmentSeed(*mut *mut ::core::ffi::c_char);

impl Drop for ProcessEnvironmentSeed {
    fn drop(&mut self) {
        unsafe {
            // setenv may replace the initially allocated pointer array. If it
            // did, the seed is no longer the active process environment and
            // remains ours to free. Otherwise ownership has moved to environ.
            if environ != self.0 {
                free(self.0 as *mut ::core::ffi::c_void);
            }
        }
    }
}

impl environ {
    pub fn find(&self, name: &CStr) -> Option<&environ_entry> {
        self.entries.get(name).map(Box::as_ref)
    }

    pub fn find_bytes(&self, name: &[u8]) -> Result<Option<&environ_entry>, NulError> {
        let name = CString::new(name)?;
        Ok(self.find(&name))
    }

    pub fn entries(&self) -> impl Iterator<Item = &environ_entry> {
        self.entries.values().map(Box::as_ref)
    }

    pub fn set_cstr(&mut self, name: &CStr, flags: ::core::ffi::c_int, value: &CStr) {
        let value = value.to_owned();
        if let Some(entry) = self.entries.get_mut(name) {
            entry.flags = flags;
            entry.value = Some(value);
        } else {
            self.entries.insert(
                name.to_owned(),
                Box::new(environ_entry::new(name.to_owned(), flags, Some(value))),
            );
        }
    }

    pub fn set(
        &mut self,
        name: &[u8],
        flags: ::core::ffi::c_int,
        value: &[u8],
    ) -> Result<(), NulError> {
        self.set_cstr(&CString::new(name)?, flags, &CString::new(value)?);
        Ok(())
    }

    pub fn clear_cstr(&mut self, name: &CStr) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.value = None;
        } else {
            self.entries.insert(
                name.to_owned(),
                Box::new(environ_entry::new(name.to_owned(), 0, None)),
            );
        }
    }

    pub fn clear(&mut self, name: &[u8]) -> Result<(), NulError> {
        self.clear_cstr(&CString::new(name)?);
        Ok(())
    }

    pub fn unset_cstr(&mut self, name: &CStr) {
        self.entries.remove(name);
    }

    pub fn copy_from(&mut self, source: &environ) {
        for entry in source.entries() {
            if let Some(value) = entry.value.as_deref() {
                self.set_cstr(&entry.name, entry.flags, value);
            } else {
                self.clear_cstr(&entry.name);
            }
        }
    }
}

impl environ_entry {
    pub fn name(&self) -> &CStr {
        &self.name
    }
    pub fn name_bytes(&self) -> &[u8] {
        self.name.as_bytes()
    }
    pub fn value(&self) -> Option<&CStr> {
        self.value.as_deref()
    }
    pub fn value_bytes(&self) -> Option<&[u8]> {
        self.value().map(CStr::to_bytes)
    }
    pub fn flags(&self) -> ::core::ffi::c_int {
        self.flags
    }
}

pub fn environ_iter(env: &environ) -> impl Iterator<Item = &environ_entry> {
    env.entries()
}

use crate::src::shared::abi::*;
pub use crate::src::shared::environment::ENVIRON_HIDDEN;
pub use crate::src::shared::environment::{environ, environ_entry};
use crate::src::shared::options::options;

pub fn environ_create() -> Box<environ> {
    Box::default()
}

pub fn environ_copy(src: &environ, dst: &mut environ) {
    dst.copy_from(src);
}

pub fn environ_find<'a>(env: &'a environ, name: &CStr) -> Option<&'a environ_entry> {
    env.find(name)
}

pub unsafe fn environ_set(
    env: &mut environ,
    name: &CStr,
    flags: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let value = format_message_with(write);
    env.set_cstr(name, flags, &value);
}

pub unsafe fn environ_clear(env: &mut environ, name: &CStr) {
    env.clear_cstr(name);
}

pub unsafe fn environ_put(env: &mut environ, var: &CStr, flags: ::core::ffi::c_int) {
    let var = var.to_bytes_with_nul();
    let Some(equals) = var[..var.len() - 1].iter().position(|&byte| byte == b'=') else {
        return;
    };
    let name = CString::new(&var[..equals]).expect("environment name contains no NUL");
    let value =
        CStr::from_bytes_with_nul(&var[equals + 1..]).expect("environment value ends at input NUL");
    env.set_cstr(&name, flags, value);
}

pub unsafe fn environ_unset(env: &mut environ, name: &CStr) {
    env.unset_cstr(name);
}

pub unsafe fn environ_update(oo: *mut options, src: &environ, dst: &mut environ) {
    let Some(patterns) =
        crate::src::options::options_read_entry(&*oo, c"update-environment", |entry| {
            crate::src::options::options_array_iter(entry)
                .map(|item| {
                    item.value
                        .string_ptr()
                        .expect("environment pattern")
                        .to_owned()
                })
                .collect::<Vec<_>>()
        })
    else {
        return;
    };
    for pattern in patterns {
        let mut found = false;
        for entry in environ_iter(src) {
            if fnmatch(pattern.as_ptr(), entry.name.as_ptr(), 0) == 0 {
                environ_set(dst, &entry.name, 0, |out| {
                    write_cstr(out, entry.value.as_deref())
                });
                found = true;
            }
        }
        if !found {
            environ_clear(dst, &pattern);
        }
    }
}
pub unsafe fn environ_push(env: &environ) {
    let seed = xcalloc(::core::mem::size_of::<*mut ::core::ffi::c_char>() as size_t)
        as *mut *mut ::core::ffi::c_char;
    environ = seed;
    let seed_owner = ProcessEnvironmentSeed(seed);
    for envent in environ_iter(env) {
        if !envent.value.is_none()
            && *envent.name.as_ptr() as ::core::ffi::c_int != '\0' as i32
            && !envent.flags & ENVIRON_HIDDEN != 0
        {
            setenv(
                (envent.name).as_ptr().cast_mut(),
                (envent.value)
                    .as_ref()
                    .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut()),
                1 as ::core::ffi::c_int,
            );
        }
    }
    drop(seed_owner);
}
pub unsafe fn environ_log(
    env: &environ,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let prefix = format_message_with(write);
    for envent in environ_iter(env) {
        if !envent.value.is_none() && *envent.name.as_ptr() as ::core::ffi::c_int != '\0' as i32 {
            log_debug(format_args!(
                "{}{}={}",
                log_cstr(&prefix),
                log_cstr(&envent.name),
                log_cstr(envent.value.as_deref().unwrap_or(c"(null)"))
            ));
        }
    }
}
pub unsafe fn environ_for_session(
    s_owner: Option<&SessionRef>,
    mut no_TERM: ::core::ffi::c_int,
) -> Box<environ> {
    let _value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut idx: ::core::ffi::c_int = 0;
    let mut env = environ_create();
    environ_copy(
        global_environ.as_deref().expect("global environment"),
        &mut env,
    );
    if let Some(session) = s_owner {
        session.with_environment_mut(|source| environ_copy(source, &mut env));
    }
    if no_TERM == 0 {
        let terminal = options_get_string(global_options, c"default-terminal");
        environ_set(&mut env, c"TERM", 0 as ::core::ffi::c_int, |out| {
            write_cstr(out, &*terminal)
        });
        environ_set(&mut env, c"TERM_PROGRAM", 0 as ::core::ffi::c_int, |out| {
            write_cstr(out, c"tmux")
        });
        environ_set(
            &mut env,
            c"TERM_PROGRAM_VERSION",
            0 as ::core::ffi::c_int,
            |out| out.write_all(getversion().to_bytes()),
        );
        environ_set(&mut env, c"COLORTERM", 0 as ::core::ffi::c_int, |out| {
            out.write_all(b"truecolor")
        });
    }
    environ_clear(&mut env, c"LISTEN_PID");
    environ_clear(&mut env, c"LISTEN_FDS");
    environ_clear(&mut env, c"LISTEN_FDNAMES");
    idx = s_owner.map_or(-1, |session| session.id() as i32);
    environ_set(&mut env, c"TMUX", 0 as ::core::ffi::c_int, |out| {
        out.write_all(socket_path_cstr().to_bytes())?;
        write!(
            out,
            ",{},{}",
            (getpid() as ::core::ffi::c_long) as ::core::ffi::c_long,
            (idx) as i32
        )
    });
    env
}
