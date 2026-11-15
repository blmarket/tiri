//! Segmented byte storage implementing Buf and BufMut.
#![deny(unsafe_op_in_unsafe_fn)]

mod segmented;

pub use bytes::{Buf, BufMut};
pub use segmented::{Chunks, SegmentedBuf};

/// Line termination policies used by tiri's buffer readers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// Consume one LF; a preceding CR remains in the returned bytes.
    Lf,
    /// Consume one CR or LF, plus an immediately following opposite terminator
    /// if present. Repeated identical terminators produce separate lines.
    Legacy,
}
