//! Authoritative screen display state and cursor domains.

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct progress_bar {
    pub state: progress_bar_state,
    pub progress: ::core::ffi::c_int,
}

pub type progress_bar_state = ::core::ffi::c_uint;
pub const PROGRESS_BAR_INDETERMINATE: progress_bar_state = 3;
pub const PROGRESS_BAR_HIDDEN: progress_bar_state = 0;

pub type screen_cursor_style = ::core::ffi::c_uint;
pub const SCREEN_CURSOR_BAR: screen_cursor_style = 3;
pub const SCREEN_CURSOR_UNDERLINE: screen_cursor_style = 2;
pub const SCREEN_CURSOR_BLOCK: screen_cursor_style = 1;
pub const SCREEN_CURSOR_DEFAULT: screen_cursor_style = 0;

use super::abi::u_int;
#[derive(Default)]
pub struct visible_ranges {
    pub used: u_int,
    /// Owned range storage. Its length tracks the number of initialized slots.
    pub(crate) storage: Vec<visible_range>,
}

impl visible_ranges {
    pub fn is_empty(&self) -> bool {
        self.storage[..self.used as usize]
            .iter()
            .all(|range| range.nx == 0)
    }

    pub fn ensure(&mut self, n: u_int) {
        if self.storage.len() < n as usize {
            self.storage.resize(n as usize, visible_range::default());
        }
    }

    pub fn clear(&mut self) {
        self.storage = Vec::new();
        self.used = 0;
    }
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct visible_range {
    pub px: u_int,
    pub nx: u_int,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_ranges_owns_initialized_storage() {
        let mut ranges = visible_ranges::default();
        ranges.ensure(2);
        assert_eq!(ranges.storage.len(), 2);
        ranges.storage[0].px = 4;
        ranges.storage[0].nx = 7;
        ranges.ensure(128);
        assert_eq!(ranges.storage[0].px, 4);
        assert_eq!(ranges.storage[0].nx, 7);

        ranges.used = 1;
        ranges.clear();
        assert!(ranges.storage.is_empty());
        assert_eq!(ranges.used, 0);
    }
}
