//! Authoritative paste model declarations.

use super::abi::{size_t, time_t, u_int};

#[repr(C)]
pub struct paste_buffer {
    pub data: Option<Box<[u8]>>,
    pub name: std::ffi::CString,
    pub created: time_t,
    pub automatic: ::core::ffi::c_int,
    pub order: u_int,
}

impl paste_buffer {
    pub fn size(&self) -> size_t {
        self.data.as_ref().map_or(0, |bytes| bytes.len())
    }

    pub fn empty() -> Self {
        Self {
            data: Default::default(),
            name: Default::default(),
            created: Default::default(),
            automatic: Default::default(),
            order: Default::default(),
        }
    }
}
