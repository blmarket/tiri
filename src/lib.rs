#![feature(local_waker)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(static_mut_refs)]
#![allow(unused_assignments)]
#![allow(unused_mut)]
// The C2Rust translation intentionally retains these expression shapes while
// the generated modules are migrated incrementally.
#![allow(clippy::eq_op)]
#![allow(clippy::self_assignment)]
#![allow(clippy::while_immutable_condition)]
// systemd bindings use opaque extern types.
#![cfg_attr(target_os = "linux", feature(extern_types))]

extern crate libc;

#[path = "modules.rs"]
pub mod src;
