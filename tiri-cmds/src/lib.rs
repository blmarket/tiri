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
mod cmd;
mod engine;
mod entries;
mod shared;

pub use engine::ENGINE;

#[cfg(feature = "test-support")]
pub mod test_cases {
    pub use crate::cmd::queue::client_queue_tests;
    pub use crate::entries::confirm_before::confirm_before_tests;
    pub use crate::entries::load_buffer::load_buffer_tests;
    pub use crate::entries::run_shell::run_shell_tests;
    pub use crate::entries::wait_for::wait_for_tests;
}

#[cfg(test)]
mod command_registration;
