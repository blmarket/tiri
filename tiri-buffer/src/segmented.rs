use std::collections::VecDeque;
use std::io;

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

impl io::Write for SegmentedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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

impl SegmentedBuf {
    /// Total readable bytes stored across all segments.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Return true when no readable bytes remain.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrow nonempty segments in order without coalescing.
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.segments.iter().map(Segment::as_slice)
    }

    /// Append bytes to the tail, reusing spare segment capacity before allocating.
    pub fn extend_from_slice(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            if let Some(tail) = self
                .segments
                .back_mut()
                .filter(|tail| tail.bytes.len() < tail.bytes.capacity())
            {
                let take = bytes.len().min(tail.bytes.capacity() - tail.bytes.len());
                let len = self.len.checked_add(take).expect("buffer too large");
                tail.bytes.extend_from_slice(&bytes[..take]);
                self.len = len;
                bytes = &bytes[take..];
            } else {
                if self.spare.capacity() == 0 {
                    self.spare.reserve(SEGMENT_CAPACITY);
                }
                let take = bytes.len().min(self.spare.capacity() - self.spare.len());
                let len = self.len.checked_add(take).expect("buffer too large");
                self.segments.reserve(1);
                self.spare.extend_from_slice(&bytes[..take]);
                self.segments.push_back(Segment {
                    bytes: std::mem::take(&mut self.spare),
                    start: 0,
                });
                self.len = len;
                bytes = &bytes[take..];
            }
        }
    }

    /// Consume `count` prefix bytes, panicking if `count` exceeds [`Self::len`].
    pub fn advance(&mut self, count: usize) {
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

    /// Coalesce all readable bytes into a single contiguous slice.
    pub fn make_contiguous(&mut self) -> &mut [u8] {
        self.scan = None;
        if self.segments.is_empty() {
            return &mut [];
        }
        if self.segments.len() > 1 {
            let len = self.len;
            let bytes = self.copy_prefix(len);
            self.advance(len);
            self.segments.push_front(Segment { bytes, start: 0 });
            self.len = len;
        }
        let front = self.segments.front_mut().expect("nonempty buffer");
        &mut front.bytes[front.start..]
    }

    /// Copy up to `limit` prefix bytes into an owned vector without consuming them.
    pub fn copy_prefix(&self, limit: usize) -> Vec<u8> {
        let count = limit.min(self.len);
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

    /// Copy bytes starting at `offset` into `dst` without consuming them.
    pub fn copy_slice(&self, mut offset: usize, dst: &mut [u8]) -> usize {
        if offset >= self.len || dst.is_empty() {
            return 0;
        }
        let mut copied = 0;
        for chunk in self.chunks() {
            if offset >= chunk.len() {
                offset -= chunk.len();
                continue;
            }
            let chunk = &chunk[offset..];
            let count = chunk.len().min(dst.len() - copied);
            dst[copied..copied + count].copy_from_slice(&chunk[..count]);
            copied += count;
            offset = 0;
            if copied == dst.len() {
                break;
            }
        }
        copied
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
        if source.is_empty() {
            return;
        }
        if self.is_empty() {
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
