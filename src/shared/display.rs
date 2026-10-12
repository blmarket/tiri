//! Authoritative screen display state and cursor domains.

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct progress_bar {
    pub state: progress_bar_state,
    pub progress: ::core::ffi::c_int,
}

pub type progress_bar_state = ::core::ffi::c_uint;
pub const PROGRESS_BAR_PAUSED: progress_bar_state = 4;
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

    /// Keep the parts of a row outside the overlay's rectangle.
    pub fn exclude_box(&mut self, area: (u_int, u_int, u_int, u_int), row: (u_int, u_int, u_int)) {
        let (x, y, sx, sy) = area;
        let (px, py, nx) = row;
        if py < y || py > y.wrapping_add(sy).wrapping_sub(1) {
            self.ensure(1);
            self.storage[0] = visible_range { px, nx };
            self.used = 1;
            return;
        }
        self.ensure(2);
        self.storage[0] = if px < x {
            visible_range {
                px,
                nx: x.wrapping_sub(px).min(nx),
            }
        } else {
            visible_range::default()
        };
        let right = x.wrapping_add(sx).max(px);
        let end = px.wrapping_add(nx);
        self.storage[1] = if end > right {
            visible_range {
                px: right,
                nx: end.wrapping_sub(right),
            }
        } else {
            visible_range::default()
        };
        self.used = 2;
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
    use ::core::mem::{align_of, offset_of, size_of};

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

    #[test]
    fn display_layout_matches_translated_c_baseline() {
        assert_eq!(size_of::<progress_bar>(), 8);
        assert_eq!(align_of::<progress_bar>(), 4);
        assert_eq!(offset_of!(progress_bar, state), 0);
        assert_eq!(offset_of!(progress_bar, progress), 4);
        assert_eq!(size_of::<progress_bar_state>(), 4);
        assert_eq!(align_of::<progress_bar_state>(), 4);
        assert_eq!(size_of::<screen_cursor_style>(), 4);
        assert_eq!(align_of::<screen_cursor_style>(), 4);
        assert_eq!(PROGRESS_BAR_HIDDEN, 0);
        assert_eq!(PROGRESS_BAR_PAUSED, 4);
        assert_eq!(SCREEN_CURSOR_DEFAULT, 0);
        assert_eq!(SCREEN_CURSOR_BAR, 3);
    }
}
