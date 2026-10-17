#![feature(local_waker)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(static_mut_refs)]
#![allow(unused_assignments)]
#![allow(unused_mut)]
#![allow(clippy::eq_op)]
#![allow(clippy::self_assignment)]
#![allow(clippy::while_immutable_condition)]
#![cfg_attr(target_os = "linux", feature(extern_types))]

extern crate libc;

#[path = "modules.rs"]
pub mod src;
