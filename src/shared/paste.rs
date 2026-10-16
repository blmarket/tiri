//! Authoritative paste model declarations.

use super::abi::{size_t, time_t, u_int};
use refbox::{Borrow, BorrowError, RefBox, Weak};

/// An observer of a registry-owned buffer. Cloning never retains its contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteBufferRef(Weak<paste_buffer>);

impl PasteBufferRef {
    pub fn observe(owner: &RefBox<paste_buffer>) -> Self {
        Self(owner.downgrade())
    }

    pub fn try_borrow(&self) -> Option<Borrow<'_, paste_buffer>> {
        match self.0.try_borrow_mut() {
            Ok(value) => Some(value),
            Err(BorrowError::Dropped) => None,
            Err(BorrowError::Borrowed) => panic!("paste buffer already borrowed"),
        }
    }

    /// Use only for an immediate lookup with no intervening callback dispatch.
    pub fn borrow(&self) -> Borrow<'_, paste_buffer> {
        self.try_borrow().expect("live paste buffer")
    }

    pub fn borrow_mut(&self) -> Borrow<'_, paste_buffer> {
        self.borrow()
    }
}

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
