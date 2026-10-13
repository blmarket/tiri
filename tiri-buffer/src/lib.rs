//! Segmented byte storage implementing Buf, BufMut, and tiri buffer extensions.
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(min_specialization)]

mod buffer;
mod segmented;

pub use buffer::{Buffer, LineEnding};
pub use bytes::{Buf, BufMut};
pub use segmented::SegmentedBuf;
