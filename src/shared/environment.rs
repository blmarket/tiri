//! Authoritative environment declarations.

use std::collections::BTreeMap;

pub const ENVIRON_HIDDEN: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

/// Ordered environment storage. Boxes keep entry addresses stable for UI tags.
#[derive(Default, Clone)]
#[repr(C)]
pub struct environ {
    pub(crate) entries: BTreeMap<std::ffi::CString, Box<environ_entry>>,
}

#[derive(Clone)]
#[repr(C)]
pub struct environ_entry {
    pub name: std::ffi::CString,
    pub value: Option<std::ffi::CString>,
    pub flags: ::core::ffi::c_int,
}

impl environ_entry {
    pub(crate) fn new(
        name: std::ffi::CString,
        flags: ::core::ffi::c_int,
        value: Option<std::ffi::CString>,
    ) -> Self {
        Self { name, value, flags }
    }

    /// Nonowning tag for this allocation, valid until the entry is removed.
    pub fn id(&self) -> u64 {
        self as *const Self as u64
    }
}
