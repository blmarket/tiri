//! Pane identities and shared ordering, mode and resize components.
pub use crate::src::window_pane::window_pane;

use crate::src::shared::client::ClientWeak;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::rc::Weak;

use super::abi::{size_t, u_int};
use super::prompt::{prompt_free_cb, prompt_type};
use super::status::status_prompt_input_cb;
use super::window::window_mode_entry;

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct window_pane_offset {
    pub used: size_t,
}

/// Unconsumed output for a copied pane offset, without borrowing a pane model.
pub fn pane_output_data<'a>(
    input: &'a mut tiri_buffer::SegmentedBuf,
    base_offset: size_t,
    offset: &window_pane_offset,
) -> &'a [u8] {
    let used = offset.used.wrapping_sub(base_offset);
    let data = crate::src::reactor::evbuffer_pullup(input, -1).unwrap_or_default();
    data.get(used..)
        .expect("pane offset is within input buffer")
}

/// Boxes preserve resize addresses while the queue grows or removes entries.
pub type window_pane_resizes = VecDeque<Box<window_pane_resize>>;

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct window_pane_resize {
    pub sx: u_int,
    pub sy: u_int,
    pub osx: u_int,
    pub osy: u_int,
}
pub const PANE_CHANGED: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const PANE_STYLECHANGED: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;
pub const PANE_THEMECHANGED: ::core::ffi::c_int = 0x2000 as ::core::ffi::c_int;
pub const PANE_INPUTOFF: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const PANE_MINIMUM: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const PANE_REDRAW: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const PANE_STATUS_TOP: ::core::ffi::c_int = 1;
pub const PANE_STATUS_BOTTOM: ::core::ffi::c_int = 2;
pub const PANE_SCROLLBARS_RIGHT: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_LEFT: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const PANE_STATUSDRAWN: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;
pub const PANE_UNSEENCHANGES: ::core::ffi::c_int = 0x4000 as ::core::ffi::c_int;
pub const PANE_CMDRUNNING: ::core::ffi::c_int = 0x20000 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_ALWAYS: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const PANE_ACTIVITY: ::core::ffi::c_int = 0x40000 as ::core::ffi::c_int;
pub const PANE_REDRAWSCROLLBAR: ::core::ffi::c_int = 0x8000 as ::core::ffi::c_int;
pub const PANE_BORDER_COLOUR: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const PANE_STATUS_OFF: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_OFF: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const PANE_BORDER_ARROWS: ::core::ffi::c_int = 2 as ::core::ffi::c_int;
pub const PANE_BORDER_BOTH: ::core::ffi::c_int = 3 as ::core::ffi::c_int;
pub const PANE_NEWSTATUS: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const PANE_DROP: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const PANE_EXITED: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_MODAL: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_AUTOHIDE: ::core::ffi::c_int = 3 as ::core::ffi::c_int;
pub const PANE_EMPTY: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_DEFAULT_PADDING: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_DEFAULT_WIDTH: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const PANE_SCROLLBARS_CHARACTER: ::core::ffi::c_int = ' ' as i32;
pub const PANE_FOCUSED: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const PANE_DESTROYED: ::core::ffi::c_int = 0x10000 as ::core::ffi::c_int;

/// The prompt cleanup closure owns this record; all other handles observe it.
pub struct window_pane_prompt {
    pub wp_id: u_int,
    pub c: ClientWeak,
    pub inputcb: status_prompt_input_cb,
    pub freecb: prompt_free_cb,
    pub type_0: prompt_type,
}

#[derive(Default)]
pub enum PaneScreenSource {
    #[default]
    Base,
    Mode(refbox::Weak<window_mode_entry>),
}

pub type window_pane_modes = Vec<refbox::RefBox<window_mode_entry>>;

/// Ordered, non-owning pane handles. Retained Rc references keep allocations
/// alive; this collection only records order.
#[derive(Default)]
pub struct window_panes {
    pub storage: Vec<std::rc::Weak<std::cell::UnsafeCell<window_pane>>>,
}

impl window_panes {
    /// Retain every pane in display order for operations that may outlive membership.
    pub fn snapshot(&self) -> Vec<std::rc::Rc<std::cell::UnsafeCell<window_pane>>> {
        self.storage
            .iter()
            .map(|observer| observer.upgrade().expect("live pane in ordering"))
            .collect()
    }

    pub fn first(&self) -> Option<std::rc::Weak<std::cell::UnsafeCell<window_pane>>> {
        self.storage.first().cloned()
    }

    pub fn next(
        &self,
        pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    ) -> Option<std::rc::Weak<std::cell::UnsafeCell<window_pane>>> {
        let position = self.position(pane)?;
        self.storage.get(position + 1).cloned()
    }

    pub fn last(&self) -> Option<std::rc::Weak<std::cell::UnsafeCell<window_pane>>> {
        self.storage.last().cloned()
    }

    pub fn previous(
        &self,
        pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    ) -> Option<std::rc::Weak<std::cell::UnsafeCell<window_pane>>> {
        self.position(pane)
            .and_then(|position| position.checked_sub(1))
            .and_then(|position| self.storage.get(position))
            .cloned()
    }

    pub fn position(
        &self,
        pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    ) -> Option<usize> {
        self.storage.iter().position(|weak| weak.ptr_eq(pane))
    }

    pub fn remove(&mut self, pane: &std::rc::Weak<std::cell::UnsafeCell<window_pane>>) -> bool {
        let storage = &mut self.storage;
        let Some(position) = storage.iter().position(|weak| weak.ptr_eq(pane)) else {
            return false;
        };
        storage.remove(position);
        true
    }
}

/// Most-recently-visited pane handles, with the newest pane at the front.
#[derive(Default)]
pub struct PaneHistory {
    panes: VecDeque<Weak<UnsafeCell<window_pane>>>,
}

impl PaneHistory {
    pub fn first(&self) -> Weak<UnsafeCell<window_pane>> {
        self.panes.front().cloned().unwrap_or_default()
    }

    pub fn position(&self, pane: &Weak<UnsafeCell<window_pane>>) -> Option<usize> {
        self.panes.iter().position(|weak| weak.ptr_eq(pane))
    }

    pub fn push(&mut self, pane: Weak<UnsafeCell<window_pane>>) {
        assert!(pane.strong_count() != 0, "live pane for insertion");
        self.remove(&pane);
        self.panes.push_front(pane);
    }

    pub fn remove(&mut self, pane: &Weak<UnsafeCell<window_pane>>) {
        self.panes.retain(|weak| !weak.ptr_eq(pane));
    }
}

#[repr(C)]
pub struct window_pane_tree {
    /// The global pane index owns both its map and the Rc pane records.
    pub storage: std::collections::BTreeMap<u_int, std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
}
