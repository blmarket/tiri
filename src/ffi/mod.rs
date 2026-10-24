//! Foreign declarations and the owned wrappers that bound them, for the C
//! libraries other than libc (whose wrappers live in `tiri-sys`): the
//! terminfo database, utf8proc's Unicode tables and utempter. Nothing here is
//! implemented in Rust except `utf8proc.rs::codepoint_width`.
//! Keep provider signatures C-compatible, including variadics and nullable callbacks.

pub mod ncurses;
#[cfg(target_os = "linux")]
pub mod utempter;
pub mod utf8proc;
