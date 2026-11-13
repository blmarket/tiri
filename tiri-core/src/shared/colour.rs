//! Authoritative colour-theme and palette declarations.

pub type client_theme = ::core::ffi::c_uint;
pub const THEME_DARK: client_theme = 2;
pub const THEME_LIGHT: client_theme = 1;
pub const THEME_UNKNOWN: client_theme = 0;

#[derive(Clone, Default)]
#[repr(C)]
pub struct colour_palette {
    pub fg: ::core::ffi::c_int,
    pub bg: ::core::ffi::c_int,
    /// Box-owned fixed array, borrowed by palette lookup until clear/free.
    pub palette: Option<Box<[::core::ffi::c_int; 256]>>,
    /// Box-owned fixed array, rebuilt from `pane-colours` options.
    pub default_palette: Option<Box<[::core::ffi::c_int; 256]>>,
}

pub const COLOUR_FLAG_256: ::core::ffi::c_int = 0x1000000 as ::core::ffi::c_int;
pub const COLOUR_FLAG_RGB: ::core::ffi::c_int = 0x2000000 as ::core::ffi::c_int;
pub const COLOUR_FLAG_THEME: ::core::ffi::c_int = 0x4000000 as ::core::ffi::c_int;
pub const COLOUR_THEME_COUNT: ::core::ffi::c_int = 10 as ::core::ffi::c_int;
pub type colour_theme = ::core::ffi::c_uint;
pub const COLOUR_THEME_CYAN: colour_theme = 8;
pub const COLOUR_THEME_LIGHT_GREY: colour_theme = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, offset_of, size_of};

    #[test]
    fn colour_palette_layout_matches_translated_c_baseline() {
        assert_eq!(size_of::<colour_palette>(), 24);
        assert_eq!(align_of::<colour_palette>(), 8);
        assert_eq!(offset_of!(colour_palette, fg), 0);
        assert_eq!(offset_of!(colour_palette, bg), 4);
        assert_eq!(offset_of!(colour_palette, palette), 8);
        assert_eq!(offset_of!(colour_palette, default_palette), 16);
    }
}
