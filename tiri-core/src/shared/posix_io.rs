//! Authoritative posix io declarations, supplied by the platform libc.

pub use ::libc::{
    glob_t, stat, FNM_CASEFOLD, O_APPEND, O_CREAT, O_NONBLOCK, O_TRUNC, O_WRONLY, STDERR_FILENO,
    STDIN_FILENO, STDOUT_FILENO, S_IRWXU, WNOHANG, X_OK,
};

pub const WAIT_ANY: ::libc::pid_t = -1;

pub const _PATH_BSHELL: &std::ffi::CStr = c"/bin/sh";

pub const _PATH_DEVNULL: &std::ffi::CStr = c"/dev/null";
