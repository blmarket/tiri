//! Authoritative errno declarations, supplied by the platform libc.

pub use ::libc::{
    E2BIG, EAGAIN, EBADMSG, ECHILD, EINTR, EINVAL, ENAMETOOLONG, ENOENT, ENOMEM, ERANGE,
};
