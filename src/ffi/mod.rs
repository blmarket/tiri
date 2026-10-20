//! Foreign declarations and the owned wrappers that bound them. Nothing here is
//! implemented in Rust except the wrappers in `regex.rs` and
//! `utf8proc.rs::codepoint_width`.
//! Keep provider signatures C-compatible, including variadics and nullable callbacks.

pub mod libc;
pub mod ncurses;
pub mod regex;
pub mod resolv;
#[cfg(target_os = "linux")]
pub mod utempter;
pub mod utf8proc;
