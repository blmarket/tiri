use crate::ffi::libc::environ;
use crate::format::bytes::format_message_with;
use crate::format::bytes::write_cstr;
use crate::log::{log_cstr, log_debug};
use crate::options::{options_get_string, OptionsScope};
use crate::session::Session;
use crate::shared::session::SessionRef;
use crate::tmux::{getversion, global_environ, socket_path_cstr};
use crate::xmalloc::xcalloc;
use libc::{fnmatch, free, setenv};
use std::ffi::{CStr, CString};

struct ProcessEnvironmentSeed(*mut *mut ::core::ffi::c_char);

impl Drop for ProcessEnvironmentSeed {
    fn drop(&mut self) {
        unsafe {
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

pub fn environ_iter(env: &environ) -> impl Iterator<Item = &environ_entry> {
    env.entries()
}

use crate::shared::abi::*;
pub use crate::shared::environment::ENVIRON_HIDDEN;
pub use crate::shared::environment::{environ, environ_entry};

pub fn environ_create() -> Box<environ> {
    Box::default()
}

pub fn environ_copy(src: &environ, dst: &mut environ) {
    dst.copy_from(src);
}

pub fn environ_find<'a>(env: &'a environ, name: &CStr) -> Option<&'a environ_entry> {
    env.find(name)
}

pub fn environ_set(
    env: &mut environ,
    name: &CStr,
    flags: ::core::ffi::c_int,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) {
    let value = format_message_with(write);
    env.set_cstr(name, flags, &value);
}

pub fn environ_clear(env: &mut environ, name: &CStr) {
    env.clear_cstr(name);
}

pub fn environ_put(env: &mut environ, var: &CStr, flags: ::core::ffi::c_int) {
    let var = var.to_bytes_with_nul();
    let Some(equals) = var[..var.len() - 1].iter().position(|&byte| byte == b'=') else {
        return;
    };
    let name = CString::new(&var[..equals]).expect("environment name contains no NUL");
    let value =
        CStr::from_bytes_with_nul(&var[equals + 1..]).expect("environment value ends at input NUL");
    env.set_cstr(&name, flags, value);
}

pub fn environ_unset(env: &mut environ, name: &CStr) {
    env.unset_cstr(name);
}

pub unsafe fn environ_update(scope: &OptionsScope, src: &environ, dst: &mut environ) {
    let Some(patterns) = scope.with_local(|oo| {
        crate::options::options_read_entry(oo, c"update-environment", |entry| {
            entry
                .value
                .array()
                .into_iter()
                .flat_map(|array| array.iter())
                .map(|item| {
                    item.value
                        .string_ptr()
                        .expect("environment pattern")
                        .to_owned()
                })
                .collect::<Vec<_>>()
        })
    }) else {
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
/// The process's `environ` strings as they are, read once. `std::env::vars_os`
/// is no substitute: it skips entries without `=` and reparses names that
/// begin with `=`, while tmux passes every raw string on.
pub(crate) fn process_environment() -> Vec<CString> {
    let mut entries = Vec::new();
    // SAFETY: `environ` is a NULL-terminated array of C strings, and its
    // writers (environ_push, setenv) run only in forked children or after
    // the two readers are done.
    unsafe {
        let mut var = environ;
        while !var.is_null() && !(*var).is_null() {
            entries.push(CStr::from_ptr(*var).to_owned());
            var = var.add(1);
        }
    }
    entries
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
        let terminal =
            OptionsScope::GlobalServer.with_local(|oo| options_get_string(oo, c"default-terminal"));
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
        write!(out, ",{},{}", std::process::id(), (idx) as i32)
    });
    env
}
