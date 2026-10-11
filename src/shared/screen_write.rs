//! Authoritative screen_write declarations, shared by the C translation units.

use super::abi::u_int;
use super::grid::{grid_cell, GridArray};
use super::pane::window_pane;
use super::screen::screen;
use super::tty::tty_ctx;
use std::collections::VecDeque;

/// Element of the screen-owned boxed write-list slice.
#[derive(Default)]
pub struct screen_write_cline {
    /// Text bytes owned by this row; scrolling moves the collection.
    pub data: GridArray<u8>,
    pub items: screen_write_items,
}
#[derive(Default)]
pub struct screen_write_citem {
    pub x: u_int,
    pub wrapped: ::core::ffi::c_int,
    pub type_0: screen_write_item_type,
    pub used: u_int,
    pub bg: u_int,
    pub gc: grid_cell,
}

/// Row-owned collection. Boxes keep item addresses stable as the deque grows,
/// inserts, removes, and moves between rows. Indexed lookups are O(1); middle
/// deque edits are O(n), so repeated arbitrary edits can be O(n²).
pub type screen_write_items = VecDeque<Box<screen_write_citem>>;

pub type screen_write_item_type = ::core::ffi::c_uint;
pub const CLEAR: screen_write_item_type = 1;

#[derive(Default)]
pub struct screen_write_ctx {
    /// Optional pane observer for redraw and terminal output.
    pub wp: std::rc::Weak<std::cell::UnsafeCell<window_pane>>,
    pub(crate) target: ScreenWriteTarget,
    pub flags: ::core::ffi::c_int,
    pub init_ctx_cb: screen_write_init_ctx_cb,
    pub item: Option<Box<screen_write_citem>>,
    pub scrolled: u_int,
    pub bg: u_int,
}

/// The active write target. A borrowed target is valid only between start and
/// stop; the pane operation installs the same bounded component target.
#[derive(Default)]
pub(crate) enum ScreenWriteTarget {
    #[default]
    None,
    Borrowed(std::ptr::NonNull<screen>),
}

impl screen_write_ctx {
    pub fn screen_ptr(&self) -> *mut screen {
        match &self.target {
            ScreenWriteTarget::None => std::ptr::null_mut(),
            ScreenWriteTarget::Borrowed(screen) => screen.as_ptr(),
        }
    }

    /// # Safety
    /// The screen must remain allocated until this context stops or changes target.
    pub unsafe fn borrow_screen(&mut self, screen: *mut screen) {
        self.target = ScreenWriteTarget::Borrowed(
            std::ptr::NonNull::new(screen).expect("screen write target"),
        );
    }
}

impl ScreenWriteTarget {
    pub(crate) const fn none() -> Self {
        Self::None
    }
}

pub type screen_write_init_ctx_cb = Option<Box<dyn FnMut(&mut tty_ctx)>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn address(item: &screen_write_citem) -> usize {
        item as *const screen_write_citem as usize
    }

    #[test]
    fn owner_operations_preserve_item_addresses_and_order() {
        let mut dst = screen_write_items::default();
        let mut src = screen_write_items::default();
        let make_item = || Box::default();

        let first = make_item();
        let first_ptr = address(&first);
        dst.push_back(first);
        let second = make_item();
        let second_ptr = address(&second);
        dst.push_back(second);

        let inserted = make_item();
        let inserted_ptr = address(&inserted);
        dst.insert(1, inserted);
        assert_eq!(address(dst.front().unwrap()), first_ptr);
        assert_eq!(address(dst.get(1).unwrap()), inserted_ptr);
        assert_eq!(address(dst.get(2).unwrap()), second_ptr);

        let mut src_last = 0;
        for _ in 0..64 {
            let item = make_item();
            src_last = address(&item);
            src.push_back(item);
        }
        let src_first = address(src.front().unwrap());
        dst.append(&mut src);
        assert!(src.is_empty());
        assert_eq!(address(dst.get(2).unwrap()), second_ptr);
        assert_eq!(address(dst.get(3).unwrap()), src_first);
        assert_eq!(address(dst.get(66).unwrap()), src_last);
        assert!(dst.get(67).is_none());
        assert_eq!(address(dst.get(1).unwrap()), inserted_ptr);
        dst.append(&mut src);
        assert_eq!(address(dst.front().unwrap()), first_ptr);

        let tail = make_item();
        let tail_ptr = address(&tail);
        dst.push_back(tail);
        assert_eq!(address(dst.get(66).unwrap()), src_last);
        assert_eq!(address(dst.get(67).unwrap()), tail_ptr);

        let removed = dst.remove(1).expect("linked write command");
        assert_eq!(address(&removed), inserted_ptr);
        assert_eq!(address(dst.front().unwrap()), first_ptr);
        assert_eq!(address(dst.get(1).unwrap()), second_ptr);
        assert_eq!(address(dst.front().unwrap()), first_ptr);
    }
}
