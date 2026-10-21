//! Foreign declarations and the owned wrappers that bound them. Only foreign
//! code without a cheap native equivalent belongs here: the terminfo database,
//! regex, utf8proc's Unicode tables, utempter, and the libc `environ` and
//! `tzset` that the libc crate lacks. Nothing here is implemented in Rust except
//! the wrappers in `fnmatch.rs`, `regex.rs` and `utf8proc.rs::codepoint_width`.
//! Keep provider signatures C-compatible, including variadics and nullable callbacks.

pub mod fnmatch;
pub mod libc;
pub mod ncurses;
pub mod regex;
#[cfg(target_os = "linux")]
pub mod utempter;
pub mod utf8proc;
