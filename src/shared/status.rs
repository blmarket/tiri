//! Authoritative status model declarations.

use super::abi::u_int;
use super::event::Timer;
use super::grid::grid_cell;
use super::prompt::{prompt_key_result, prompt_result};
use super::screen::screen;
use super::style::style_line_entry;
use crate::shared::client::ClientRef;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::time::SystemTime;

#[derive(Default)]
#[repr(C)]
pub struct status_line {
    pub timer: Option<Timer>,
    pub screen: screen,
    /// Owns the temporary message/prompt screen; None selects the base screen.
    pub active: Option<Box<screen>>,
    pub screen_users: ::core::ffi::c_int,
    pub prompt_cx: u_int,
    pub style: grid_cell,
    pub entries: [style_line_entry; 5],
}

impl status_line {
    /// The returned screen is borrowed until the next status push/pop or teardown.
    pub fn active_screen(&mut self) -> &mut screen {
        self.active.as_deref_mut().unwrap_or(&mut self.screen)
    }

    pub fn empty() -> Self {
        Self::default()
    }
}

pub type status_prompt_input_cb =
    Option<Box<dyn FnMut(Option<&ClientRef>, Option<&CStr>, prompt_key_result) -> prompt_result>>;

/// An owned server message. The message text and its display metadata share
/// the lifetime of the containing [`message_list`].
pub struct message_entry {
    pub msg: CString,
    pub msg_num: u_int,
    pub msg_time: SystemTime,
}

/// The message log owns its records and stores them in insertion order.
pub struct message_list {
    entries: VecDeque<message_entry>,
}

impl message_list {
    pub const fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn push_back(&mut self, msg: CString, msg_num: u_int, msg_time: SystemTime) {
        self.entries.push_back(message_entry {
            msg,
            msg_num,
            msg_time,
        });
    }

    /// Drop messages outside the configured rolling window.
    pub fn trim(&mut self, message_next: u_int, limit: u_int) {
        while self
            .entries
            .front()
            .is_some_and(|msg| msg.msg_num.wrapping_add(limit) < message_next)
        {
            self.entries.pop_front();
        }
    }

    pub fn iter_rev(&self) -> impl DoubleEndedIterator<Item = &message_entry> {
        self.entries.iter().rev()
    }
}

impl Default for message_list {
    fn default() -> Self {
        Self::new()
    }
}
