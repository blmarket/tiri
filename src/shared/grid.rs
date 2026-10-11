//! Authoritative grid storage types and grid-related flags.

use super::abi::*;

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct utf8_data {
    pub data: [u_char; 32],
    pub have: u_char,
    pub size: u_char,
    pub width: u_char,
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct grid_cell {
    pub data: utf8_data,
    pub attr: u_short,
    pub flags: u_char,
    pub fg: ::core::ffi::c_int,
    pub bg: ::core::ffi::c_int,
    pub us: ::core::ffi::c_int,
    pub link: u_int,
}

/// Heap header for a `GridArray`. Boxing the Vec header (rather than holding
/// it inline) keeps `GridArray` pointer-sized; see `GridArray`.
#[derive(Clone)]
struct GridArrayStorage<T>(Vec<T>);

/// Pointer-sized owner preserves the line-header size exposed by history byte
/// formats. The Vec owns its elements; unused arrays need no allocation.
#[derive(Clone)]
#[repr(transparent)]
pub struct GridArray<T>(Option<Box<GridArrayStorage<T>>>);

impl<T> Default for GridArray<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<T> std::ops::Deref for GridArray<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.0.as_deref().map_or(&[], |items| items.0.as_slice())
    }
}
impl<T> std::ops::DerefMut for GridArray<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.storage().as_mut_slice()
    }
}
impl<T> GridArray<T> {
    fn storage(&mut self) -> &mut Vec<T> {
        &mut self
            .0
            .get_or_insert_with(|| Box::new(GridArrayStorage(Vec::new())))
            .0
    }
    // Vec pointer access does not materialize a slice reference. Callers may
    // retain disjoint element pointers until an operation changes the array.
    pub(crate) fn as_mut_ptr(&mut self) -> *mut T {
        self.0
            .as_mut()
            .map_or(std::ptr::NonNull::dangling().as_ptr(), |items| {
                items.0.as_mut_ptr()
            })
    }
    pub(crate) fn as_ptr(&self) -> *const T {
        self.0
            .as_ref()
            .map_or(std::ptr::NonNull::dangling().as_ptr(), |items| {
                items.0.as_ptr()
            })
    }
    pub(crate) fn resize_with(&mut self, len: usize, init: impl FnMut() -> T) {
        self.storage().resize_with(len, init);
    }
    pub(crate) fn clear(&mut self) {
        // Keep the translated explicit release behavior, including capacity.
        self.0 = None;
    }
}

#[derive(Default)]
#[repr(C)]
/// Owns its lines and their cell allocations. Screens and temporary reflow
/// operations keep this record in a Box; raw grid pointers are scoped borrows.
pub struct grid {
    pub flags: ::core::ffi::c_int,
    pub sx: u_int,
    pub sy: u_int,
    pub hscrolled: u_int,
    pub hsize: u_int,
    pub hlimit: u_int,
    pub scroll_added: u_int,
    pub scroll_collected: u_int,
    pub scroll_generation: u_int,
    pub linedata: GridArray<grid_line>,
}

#[derive(Clone, Default)]
#[repr(C)]
pub struct grid_line {
    pub celldata: GridArray<grid_cell_entry>,
    pub extddata: GridArray<grid_extd_entry>,
    pub cellused: u_short,
    pub cellsize: u_short,
    pub extdsize: u_int,
    pub time: u_int,
    pub osc133_data: osc133_data,
    pub flags: u_short,
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct osc133_data {
    pub prompt_col: u_short,
    pub cmd_col: u_short,
    pub out_start_col: u_short,
    pub out_end_col: u_short,
    pub exit_status: u_char,
}

#[derive(Copy, Clone, Default)]
#[repr(C, packed)]
pub struct grid_extd_entry {
    pub data: utf8_char,
    pub attr: u_short,
    pub flags: u_char,
    pub fg: ::core::ffi::c_int,
    pub bg: ::core::ffi::c_int,
    pub us: ::core::ffi::c_int,
    pub link: u_int,
}

pub type utf8_char = u_int;

// The C definition contains an anonymous union.  These names describe the
// verified layout without conflating any unrelated C2RustUnnamed type.
#[derive(Copy, Clone)]
#[repr(C)]
pub union grid_cell_entry_storage {
    pub offset: u_int,
    pub data: grid_cell_entry_data,
}

impl Default for grid_cell_entry_storage {
    fn default() -> Self {
        Self { offset: 0 }
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct grid_cell_entry_data {
    pub attr: u_char,
    pub fg: u_char,
    pub bg: u_char,
    pub data: u_char,
}

#[derive(Copy, Clone, Default)]
#[repr(C, packed)]
pub struct grid_cell_entry {
    pub c2rust_unnamed: grid_cell_entry_storage,
    pub flags: u_char,
}

pub const GRID_ATTR_BRIGHT: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const GRID_ATTR_DIM: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const GRID_ATTR_UNDERSCORE: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const GRID_ATTR_BLINK: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const GRID_ATTR_REVERSE: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const GRID_ATTR_HIDDEN: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const GRID_ATTR_ITALICS: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const GRID_ATTR_CHARSET: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const GRID_ATTR_STRIKETHROUGH: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const GRID_ATTR_UNDERSCORE_2: ::core::ffi::c_int = 0x200 as ::core::ffi::c_int;
pub const GRID_ATTR_UNDERSCORE_3: ::core::ffi::c_int = 0x400 as ::core::ffi::c_int;
pub const GRID_ATTR_UNDERSCORE_4: ::core::ffi::c_int = 0x800 as ::core::ffi::c_int;
pub const GRID_ATTR_UNDERSCORE_5: ::core::ffi::c_int = 0x1000 as ::core::ffi::c_int;
pub const GRID_ATTR_OVERLINE: ::core::ffi::c_int = 0x2000 as ::core::ffi::c_int;
pub const GRID_ATTR_NOATTR: ::core::ffi::c_int = 0x4000 as ::core::ffi::c_int;
pub const GRID_ATTR_ALL_UNDERSCORE: ::core::ffi::c_int = GRID_ATTR_UNDERSCORE
    | GRID_ATTR_UNDERSCORE_2
    | GRID_ATTR_UNDERSCORE_3
    | GRID_ATTR_UNDERSCORE_4
    | GRID_ATTR_UNDERSCORE_5;

pub const GRID_FLAG_FG256: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const GRID_FLAG_BG256: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const GRID_FLAG_PADDING: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const GRID_FLAG_EXTENDED: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const GRID_FLAG_SELECTED: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const GRID_FLAG_NOPALETTE: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const GRID_FLAG_CLEARED: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const GRID_FLAG_TAB: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;

pub const GRID_LINE_WRAPPED: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const GRID_LINE_EXTENDED: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const GRID_LINE_DEAD: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const GRID_LINE_START_PROMPT: ::core::ffi::c_int = 0x8 as ::core::ffi::c_int;
pub const GRID_LINE_SECOND_PROMPT: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const GRID_LINE_START_COMMAND: ::core::ffi::c_int = 0x20 as ::core::ffi::c_int;
pub const GRID_LINE_START_OUTPUT: ::core::ffi::c_int = 0x40 as ::core::ffi::c_int;
pub const GRID_LINE_END_OUTPUT: ::core::ffi::c_int = 0x80 as ::core::ffi::c_int;
pub const GRID_LINE_HYPERLINK: ::core::ffi::c_int = 0x100 as ::core::ffi::c_int;
pub const GRID_LINE_OSC133_FLAGS: ::core::ffi::c_int = GRID_LINE_START_PROMPT
    | GRID_LINE_SECOND_PROMPT
    | GRID_LINE_START_COMMAND
    | GRID_LINE_START_OUTPUT
    | GRID_LINE_END_OUTPUT;

pub const GRID_STRING_WITH_SEQUENCES: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;
pub const GRID_STRING_ESCAPE_SEQUENCES: ::core::ffi::c_int = 0x2 as ::core::ffi::c_int;
pub const GRID_STRING_TRIM_SPACES: ::core::ffi::c_int = 0x4 as ::core::ffi::c_int;
pub const GRID_STRING_EMPTY_CELLS: ::core::ffi::c_int = 0x10 as ::core::ffi::c_int;
pub const GRID_HISTORY: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct grid_reader<'a> {
    pub gd: &'a grid,
    pub cx: u_int,
    pub cy: u_int,
}

pub const WHITESPACE: [::core::ffi::c_char; 3] =
    unsafe { ::core::mem::transmute::<[u8; 3], [::core::ffi::c_char; 3]>(*b"\t \0") };

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::{align_of, offset_of, size_of};

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn grid_line_size_preserves_history_byte_formats() {
        assert_eq!(size_of::<GridArray<grid_cell_entry>>(), 8);
        assert_eq!(size_of::<grid_line>(), 40);
    }

    #[test]
    fn grid_cell_storage_layout_matches_translated_c_baseline() {
        assert_eq!(size_of::<utf8_data>(), 35);
        assert_eq!(align_of::<utf8_data>(), 1);

        assert_eq!(size_of::<grid_cell>(), 56);
        assert_eq!(align_of::<grid_cell>(), 4);
        assert_eq!(offset_of!(grid_cell, data), 0);
        assert_eq!(offset_of!(grid_cell, attr), 36);
        assert_eq!(offset_of!(grid_cell, fg), 40);
        assert_eq!(offset_of!(grid_cell, link), 52);

        assert_eq!(size_of::<osc133_data>(), 10);
        assert_eq!(align_of::<osc133_data>(), 2);
        assert_eq!(offset_of!(osc133_data, exit_status), 8);

        assert_eq!(size_of::<grid_extd_entry>(), 23);
        assert_eq!(align_of::<grid_extd_entry>(), 1);
        assert_eq!(offset_of!(grid_extd_entry, fg), 7);
        assert_eq!(offset_of!(grid_extd_entry, link), 19);

        assert_eq!(size_of::<grid_cell_entry_storage>(), 4);
        assert_eq!(align_of::<grid_cell_entry_storage>(), 4);
        assert_eq!(size_of::<grid_cell_entry_data>(), 4);
        assert_eq!(size_of::<grid_cell_entry>(), 5);
        assert_eq!(align_of::<grid_cell_entry>(), 1);
        assert_eq!(offset_of!(grid_cell_entry, flags), 4);
    }
}
