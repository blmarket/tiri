//! Input storage stays in the terminal while decoding uses its contiguous bytes.
//! Decoded events own their payload before callbacks can replace that storage.
use tiri_buffer::SegmentedBuf;

#[derive(Default)]
pub(super) struct TerminalInput {
    buffer: SegmentedBuf,
}

impl TerminalInput {
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    /// Append bytes that arrived after this buffer's remaining bytes.
    pub fn append(&mut self, bytes: impl Into<SegmentedBuf>) {
        self.buffer.append(&mut bytes.into());
    }
    /// Borrow the buffer's allocation, coalescing segments only when necessary.
    pub fn bytes(&mut self) -> &[u8] {
        self.buffer.make_contiguous()
    }
    pub fn drain(&mut self, count: usize) {
        let count = count.min(self.len());
        self.buffer.advance(count);
    }
}

impl From<TerminalInput> for SegmentedBuf {
    fn from(input: TerminalInput) -> Self {
        input.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalInput;
    #[test]
    fn appends_preserve_remaining_input_and_empty_chunks_preserve_storage() {
        let mut input = TerminalInput::default();
        input.append(b"old".to_vec());
        assert_eq!(input.bytes(), b"old");
        input.drain(1);
        input.append(b"new".to_vec());
        assert_eq!(input.bytes(), b"ldnew");
        let current = input.bytes().as_ptr();
        input.append(Vec::new());
        assert_eq!(current, input.bytes().as_ptr());
    }
}
