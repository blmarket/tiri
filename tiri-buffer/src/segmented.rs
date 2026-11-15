use bytes::{Buf, BufMut, buf::UninitSlice};
use std::collections::{VecDeque, vec_deque};
use std::io::IoSlice;
use std::iter::FusedIterator;

use crate::LineEnding;

const SEGMENT_CAPACITY: usize = 4096;

#[derive(Debug)]
struct Segment {
    bytes: Vec<u8>,
    start: usize,
}

impl Segment {
    fn as_slice(&self) -> &[u8] {
        &self.bytes[self.start..]
    }
}

/// Binary-safe storage with borrowed chunks and cheap prefix consumption.
#[derive(Debug, Default)]
pub struct SegmentedBuf {
    segments: VecDeque<Segment>,
    len: usize,
    spare: Vec<u8>,
    scan: Option<Scan>,
}

impl From<Vec<u8>> for SegmentedBuf {
    /// Adopt an initialized allocation without copying its bytes.
    fn from(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let mut result = Self::default();
        if len != 0 {
            result.segments.push_back(Segment { bytes, start: 0 });
            result.len = len;
        } else {
            result.spare = bytes;
        }
        result
    }
}

#[derive(Debug)]
struct Scan {
    ending: LineEnding,
    segment: usize,
    offset: usize,
    scanned: usize,
}

impl Scan {
    fn new(ending: LineEnding) -> Self {
        Self {
            ending,
            segment: 0,
            offset: 0,
            scanned: 0,
        }
    }
}

/// Iterator over a buffer's nonempty borrowed chunks, without coalescing.
#[derive(Clone, Debug)]
pub struct Chunks<'a> {
    inner: vec_deque::Iter<'a, Segment>,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Segment::as_slice)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for Chunks<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(Segment::as_slice)
    }
}

impl ExactSizeIterator for Chunks<'_> {}
impl FusedIterator for Chunks<'_> {}

impl Buf for SegmentedBuf {
    fn remaining(&self) -> usize {
        self.len
    }

    fn chunk(&self) -> &[u8] {
        self.segments.front().map_or(&[], Segment::as_slice)
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        let count = dst.len().min(self.segments.len());
        for (slot, segment) in dst.iter_mut().zip(&self.segments) {
            *slot = IoSlice::new(segment.as_slice());
        }
        count
    }

    fn advance(&mut self, count: usize) {
        assert!(count <= self.len, "advance exceeds remaining bytes");
        let mut remaining = count;
        if remaining == 0 {
            return;
        }
        self.scan = None;
        self.len -= remaining;
        while remaining != 0 {
            let front = self.segments.front_mut().expect("nonempty buffer");
            let available = front.as_slice().len();
            if remaining < available {
                front.start += remaining;
                break;
            }
            remaining -= available;
            let drained = self.segments.pop_front().expect("nonempty buffer");
            self.recycle(drained.bytes);
        }
    }
}

unsafe impl BufMut for SegmentedBuf {
    fn remaining_mut(&self) -> usize {
        usize::MAX - self.len
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        let limit = self.remaining_mut();
        let writable = if let Some(tail) = self
            .segments
            .back_mut()
            .filter(|tail| tail.bytes.len() < tail.bytes.capacity())
        {
            &mut tail.bytes
        } else {
            if self.spare.capacity() == 0 {
                self.spare.reserve(SEGMENT_CAPACITY);
            }
            &mut self.spare
        };
        let count = (writable.capacity() - writable.len()).min(limit);
        &mut writable.chunk_mut()[..count]
    }

    unsafe fn advance_mut(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        let len = self.len.checked_add(count).expect("buffer too large");
        if let Some(tail) = self
            .segments
            .back_mut()
            .filter(|tail| tail.bytes.len() < tail.bytes.capacity())
        {
            assert!(count <= tail.bytes.capacity() - tail.bytes.len());
            unsafe { tail.bytes.advance_mut(count) };
        } else {
            assert!(count <= self.spare.capacity());
            self.segments.reserve(1);
            unsafe { self.spare.advance_mut(count) };
            self.segments.push_back(Segment {
                bytes: std::mem::take(&mut self.spare),
                start: 0,
            });
        }
        self.len = len;
    }
}

impl SegmentedBuf {
    /// Borrow storage for scanning or scatter/gather writes without pullup.
    pub fn chunks(&self) -> Chunks<'_> {
        Chunks {
            inner: self.segments.iter(),
        }
    }

    /// Expose exactly `count` prefix bytes, coalescing that prefix if necessary.
    pub fn pullup(&mut self, count: usize) -> Option<&mut [u8]> {
        if count > self.len {
            return None;
        }
        if count == 0 {
            return Some(&mut []);
        }
        self.scan = None;
        if self.segments.front()?.as_slice().len() < count {
            let bytes = self.copy_prefix(count);
            self.advance(count);
            self.segments.push_front(Segment { bytes, start: 0 });
            self.len += count;
        }
        let front = self.segments.front_mut()?;
        Some(&mut front.bytes[front.start..front.start + count])
    }

    /// Extract the first complete line, consuming it and its delimiter.
    pub fn read_line(&mut self, ending: LineEnding) -> Option<Vec<u8>> {
        let (line_len, consumed) = self.find_line(ending)?;
        let line = self.copy_prefix(line_len);
        self.advance(consumed);
        Some(line)
    }

    /// Move all source segments into this buffer without copying their payloads.
    pub fn append(&mut self, source: &mut Self) {
        if !source.has_remaining() {
            return;
        }
        if !self.has_remaining() {
            std::mem::swap(self, source);
            source.scan = None;
            return;
        }
        let len = self.len.checked_add(source.len).expect("buffer too large");
        self.segments.append(&mut source.segments);
        self.len = len;
        source.len = 0;
        source.scan = None;
    }

    fn recycle(&mut self, mut bytes: Vec<u8>) {
        if self.spare.capacity() == 0 && bytes.capacity() <= SEGMENT_CAPACITY {
            bytes.clear();
            self.spare = bytes;
        }
    }

    fn copy_prefix(&self, count: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(count);
        for chunk in self.chunks() {
            let take = chunk.len().min(count - bytes.len());
            bytes.extend_from_slice(&chunk[..take]);
            if bytes.len() == count {
                break;
            }
        }
        bytes
    }

    fn next_byte(&self, scan: &mut Scan) -> Option<u8> {
        loop {
            let chunk = self.segments.get(scan.segment)?.as_slice();
            if let Some(&byte) = chunk.get(scan.offset) {
                scan.offset += 1;
                scan.scanned += 1;
                return Some(byte);
            }
            if scan.segment + 1 == self.segments.len() {
                return None;
            }
            scan.segment += 1;
            scan.offset = 0;
        }
    }

    fn find_line(&mut self, ending: LineEnding) -> Option<(usize, usize)> {
        let mut scan = self
            .scan
            .take()
            .filter(|scan| scan.ending == ending)
            .unwrap_or_else(|| Scan::new(ending));
        while let Some(byte) = self.next_byte(&mut scan) {
            let position = scan.scanned - 1;
            match ending {
                LineEnding::Lf if byte == b'\n' => return Some((position, scan.scanned)),
                LineEnding::Legacy if matches!(byte, b'\r' | b'\n') => {
                    let mut consumed = scan.scanned;
                    if self
                        .next_byte(&mut scan)
                        .is_some_and(|next| matches!(next, b'\r' | b'\n') && next != byte)
                    {
                        consumed = scan.scanned;
                    }
                    return Some((position, consumed));
                }
                _ => {}
            }
        }
        self.scan = Some(scan);
        None
    }
}
