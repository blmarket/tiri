//! Foreign ABI boundary. Rust implementations are imported from their owning modules.
//! Keep provider signatures C-compatible, including variadics and nullable callbacks.

pub mod libc;
pub mod libm;
pub mod ncurses;
pub mod numbers;
pub mod regex;
pub mod resolv;
#[cfg(target_os = "linux")]
pub mod utempter;
pub mod utf8proc;
pub mod vis;
