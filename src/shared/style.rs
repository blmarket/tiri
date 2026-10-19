//! Authoritative style value types and style-domain constants.

use super::abi::*;
use super::grid::*;

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct style {
    pub gc: grid_cell,
    pub ignore: ::core::ffi::c_int,
    pub dim: ::core::ffi::c_int,
    pub fill: ::core::ffi::c_int,
    pub align: style_align,
    pub list: style_list,
    pub range_type: style_range_type,
    pub range_argument: u_int,
    pub range_string: [u8; 16],
    pub width: ::core::ffi::c_int,
    pub width_percentage: ::core::ffi::c_int,
    pub pad: ::core::ffi::c_int,
    pub default_type: style_default_type,
    pub link: u_int,
}

pub type style_default_type = ::core::ffi::c_uint;
pub const STYLE_DEFAULT_SET: style_default_type = 3;
pub const STYLE_DEFAULT_POP: style_default_type = 2;
pub const STYLE_DEFAULT_PUSH: style_default_type = 1;
pub const STYLE_DEFAULT_BASE: style_default_type = 0;

pub type style_range_type = ::core::ffi::c_uint;
pub const STYLE_RANGE_CONTROL: style_range_type = 7;
pub const STYLE_RANGE_USER: style_range_type = 6;
pub const STYLE_RANGE_SESSION: style_range_type = 5;
pub const STYLE_RANGE_WINDOW: style_range_type = 4;
pub const STYLE_RANGE_PANE: style_range_type = 3;
pub const STYLE_RANGE_RIGHT: style_range_type = 2;
pub const STYLE_RANGE_LEFT: style_range_type = 1;
pub const STYLE_RANGE_NONE: style_range_type = 0;

pub type style_list = ::core::ffi::c_uint;
pub const STYLE_LIST_RIGHT_MARKER: style_list = 4;
pub const STYLE_LIST_LEFT_MARKER: style_list = 3;
pub const STYLE_LIST_FOCUS: style_list = 2;
pub const STYLE_LIST_ON: style_list = 1;
pub const STYLE_LIST_OFF: style_list = 0;

pub type style_align = ::core::ffi::c_uint;
pub const STYLE_ALIGN_ABSOLUTE_CENTRE: style_align = 4;
pub const STYLE_ALIGN_RIGHT: style_align = 3;
pub const STYLE_ALIGN_CENTRE: style_align = 2;
pub const STYLE_ALIGN_LEFT: style_align = 1;
pub const STYLE_ALIGN_DEFAULT: style_align = 0;

#[derive(Clone, Default)]
#[repr(C)]
pub struct style_line_entry {
    pub expanded: Option<::std::ffi::CString>,
    pub ranges: style_ranges,
}

/// Owns the ranges; boxes keep their addresses stable as the vector grows.
pub type style_ranges = Vec<Box<style_range>>;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct style_range {
    pub type_0: style_range_type,
    pub argument: u_int,
    pub string: [u8; 16],
    pub start: u_int,
    pub end: u_int,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, offset_of, size_of};

    #[test]
    fn style_layout_matches_owned_range_storage() {
        assert_eq!(size_of::<style>(), 120);
        assert_eq!(align_of::<style>(), 4);
        assert_eq!(offset_of!(style, gc), 0);
        assert_eq!(offset_of!(style, ignore), 56);
        assert_eq!(offset_of!(style, align), 68);
        assert_eq!(offset_of!(style, range_string), 84);
        assert_eq!(offset_of!(style, default_type), 112);
        assert_eq!(offset_of!(style, link), 116);

        assert_eq!(size_of::<style_line_entry>(), 40);
        assert_eq!(align_of::<style_line_entry>(), 8);
        assert_eq!(offset_of!(style_line_entry, ranges), 16);
        assert_eq!(size_of::<style_ranges>(), 24);
        assert_eq!(align_of::<style_ranges>(), 8);
        assert_eq!(size_of::<style_range>(), 32);
        assert_eq!(align_of::<style_range>(), 4);
    }
}
