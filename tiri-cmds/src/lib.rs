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

mod arguments;
mod catalog;
mod cfg;
mod cmd;
mod entries;
mod hooks;
mod key_bindings;
mod runtime;
mod shared;

pub fn initialize() {
    tiri_core::actions::install(&runtime::RUNTIME);
}

#[cfg(test)]
mod command_registration;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod test_support;
