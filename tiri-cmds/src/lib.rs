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

use tiri_core::{
    compat, control, environ, events, events_payload, file, format, key_string, log, monitor,
    options, prompt_history, reactor, server, server_client, session, status, text, tmux, window,
    window_copy, window_pane, winlink,
};
pub use tiri_core::{libc, refbox, tiri_buffer, tiri_rt, tiri_sys};
pub mod arguments;
mod catalog;
pub mod cfg;
pub mod cmd;
pub mod entries;
pub mod hooks;
pub mod key_bindings;
mod runtime;
pub mod shared;

pub fn initialize() {
    tiri_core::actions::install(&runtime::RUNTIME);
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod test_support;

use tiri_cmdparse;
