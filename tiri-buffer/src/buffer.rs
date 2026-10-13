//! Storage contracts, independent of task scheduling and descriptor I/O.

use bytes::{Buf, BufMut};

/// Binary-safe byte storage shared by standalone buffers and stream adapters.
pub trait Buffer: Buf + BufMut + Default {
    /// Nonempty borrowed chunks in byte order.
    type Chunks<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Borrow storage for scanning or scatter/gather writes without pullup.
    fn chunks(&self) -> Self::Chunks<'_>;

    /// Expose exactly count prefix bytes, coalescing that prefix if necessary.
    fn pullup(&mut self, count: usize) -> Option<&mut [u8]>;

    /// Extract the first complete line, consuming it and its delimiter.
    fn read_line(&mut self, ending: LineEnding) -> Option<Vec<u8>>;
}

/// Line termination policies needed by the existing buffer consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// Consume one LF; a preceding CR remains in the returned bytes.
    Lf,
    /// Consume LF and an optional immediately preceding CR.
    CrLf,
    /// Require and consume CR followed by LF; a trailing CR alone is incomplete.
    CrLfStrict,
    /// Consume one NUL byte.
    Nul,
    /// Consume the entire currently buffered run of CR and/or LF at the first
    /// delimiter. The next non-CR/LF byte begins the following line.
    Any,
    /// Consume one CR or LF, plus an immediately following opposite terminator
    /// if present. Unlike Any, repeated identical terminators are separate lines.
    Legacy,
}
