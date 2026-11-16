//! Segmented byte storage and line scanning for `tiri`.
#![forbid(unsafe_code)]

mod segmented;

pub use segmented::SegmentedBuf;

/// Line termination policies used by tiri's buffer readers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// Consume one LF; a preceding CR remains in the returned bytes.
    Lf,
    /// Consume one CR or LF, plus an immediately following opposite terminator
    /// if present. Repeated identical terminators produce separate lines.
    Legacy,
}
