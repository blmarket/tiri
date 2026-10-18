//! Style parsing, formatting, and application, with attribute and colour conversion.

pub mod attributes;
pub mod colour;
mod parsing;
mod scoped;
pub use scoped::{style_apply_with_options, style_resolve_with_options};

pub use self::parsing::{
    style_copy, style_link, style_parse, style_parse_colour, style_ranges_clear, style_ranges_init,
    style_set, style_set_scrollbar_style_from_option, style_tostring,
};
