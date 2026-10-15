//! Legacy buffer helpers over plain segmented byte storage.
//! Stream scheduling belongs to bufferevent, not these storage operations.
use crate::src::shared::abi::{size_t, ssize_t};
use std::ffi::c_int;
use tiri_buffer::{Buf, BufMut, Buffer, LineEnding, SegmentedBuf};

pub fn evbuffer_new() -> Box<SegmentedBuf> {
    Box::new(SegmentedBuf::default())
}
pub fn evbuffer_get_length(b: &SegmentedBuf) -> size_t {
    b.remaining() as size_t
}
pub fn evbuffer_add(b: &mut SegmentedBuf, data: &[u8]) -> c_int {
    b.put_slice(data);
    0
}
pub fn evbuffer_drain(b: &mut SegmentedBuf, len: size_t) -> c_int {
    let count = len.min(b.remaining());
    b.advance(count);
    0
}
/// Borrow a contiguous prefix, or all readable bytes when `size` is negative.
/// Empty buffers and requests larger than the readable length return `None`.
pub fn evbuffer_pullup(b: &mut SegmentedBuf, size: ssize_t) -> Option<&mut [u8]> {
    if !b.has_remaining() || (size >= 0 && size as usize > b.remaining()) {
        None
    } else {
        let count = if size < 0 {
            b.remaining()
        } else {
            size as usize
        };
        b.pullup(count)
    }
}
/// Copy a bounded prefix into task-owned storage before awaiting a write.
/// The source remains queued until the write reports progress.
pub(crate) fn buffer_prefix(b: &SegmentedBuf, limit: usize) -> Vec<u8> {
    let count = b.remaining().min(limit);
    let mut bytes = Vec::with_capacity(count);
    for chunk in b.chunks() {
        let n = chunk.len().min(count - bytes.len());
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() == count {
            break;
        }
    }
    bytes
}

fn read_line(b: &mut SegmentedBuf, ending: LineEnding) -> Option<Vec<u8>> {
    let mut line = b.read_line(ending)?;
    line.push(0);

    Some(line)
}
pub fn evbuffer_readln(b: &mut SegmentedBuf) -> Option<Vec<u8>> {
    read_line(b, LineEnding::Lf)
}

pub fn evbuffer_readline(b: &mut SegmentedBuf) -> Option<Vec<u8>> {
    read_line(b, LineEnding::Legacy)
}
pub fn evbuffer_add_formatted(
    b: &mut SegmentedBuf,
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> c_int {
    let Some(mut formatted) = format_buffer(write) else {
        return -1;
    };
    let count = formatted.remaining();
    b.append(&mut formatted);
    count as c_int
}

/// Keep a trailing NUL in spare capacity, excluded from the readable payload.
/// Moving this segment with append preserves its allocation and capacity.
fn format_buffer(
    write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
) -> Option<SegmentedBuf> {
    let mut bytes = crate::src::format::bytes::format_bytes_with(write).ok()?;
    if bytes.len() > c_int::MAX as usize {
        return None;
    }
    bytes.try_reserve(1).ok()?;
    bytes.push(0);
    bytes.pop();
    Some(SegmentedBuf::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::src::format::bytes::write_cstr;
    use std::ffi::CString;

    #[test]
    fn pullup_borrows_prefix_and_preserves_request_bounds() {
        let mut buffer = evbuffer_new();
        assert!(evbuffer_pullup(&mut buffer, -1).is_none());
        buffer.put_slice(b"abc");
        buffer.put_slice(b"def");

        assert!(evbuffer_pullup(&mut buffer, 7).is_none());
        let prefix = evbuffer_pullup(&mut buffer, 4).unwrap();
        assert_eq!(prefix, b"abcd");
        prefix[3] = b'D';

        assert_eq!(evbuffer_get_length(&buffer), 6);
        assert_eq!(evbuffer_pullup(&mut buffer, -1).unwrap(), b"abcDef");
        evbuffer_drain(&mut buffer, 4);
        assert_eq!(evbuffer_pullup(&mut buffer, -1).unwrap(), b"ef");
        evbuffer_drain(&mut buffer, usize::MAX);
        assert!(evbuffer_pullup(&mut buffer, 0).is_none());
    }

    fn append_formatted(
        destination: &mut SegmentedBuf,
        write: impl FnOnce(&mut dyn std::io::Write) -> std::io::Result<()>,
    ) -> c_int {
        let Some(mut formatted) = format_buffer(write) else {
            return -1;
        };
        let count = formatted.remaining();
        let pointer = formatted.chunk().as_ptr();
        destination.append(&mut formatted);
        assert_eq!(formatted.remaining(), 0);
        if count != 0 {
            assert_eq!(destination.chunks().next_back().unwrap().as_ptr(), pointer);
        }
        count as c_int
    }

    #[test]
    fn formatted_storage_survives_append_and_terminator_commit() {
        for size in [1, 1023, 1024, 4095, 4096, 4097, 40076, 100_000] {
            let text = CString::new("x".repeat(size)).unwrap();
            let mut buffer = SegmentedBuf::default();
            assert_eq!(
                append_formatted(&mut buffer, |out| {
                    write_cstr(out, &*text)?;
                    write!(out, ":{}:{}", { -7i32 }, { 42usize })
                }),
                (size + 6) as c_int
            );
            assert_eq!(buffer.chunks().len(), 1);
            let length = buffer.remaining();
            let pointer = buffer.pullup(length).unwrap().as_ptr();
            assert_eq!(
                unsafe { std::ffi::CStr::from_ptr(pointer.cast()) }.to_bytes(),
                buffer.chunk()
            );
            assert_eq!(
                buffer.chunk(),
                format!("{}:-7:42", "x".repeat(size)).as_bytes()
            );
            buffer.put_slice(b"\0");
            assert_eq!(buffer.chunks().len(), 1);
            assert_eq!(buffer.chunk().as_ptr(), pointer);
            assert_eq!(buffer.remaining(), length + 1);
            assert_eq!(buffer.chunk()[length], 0);
        }
    }

    #[test]
    fn formatting_preserves_prefix_empty_output_and_embedded_nul() {
        let mut buffer = SegmentedBuf::from(b"prefix".to_vec());
        assert_eq!(
            append_formatted(&mut buffer, |out| { write_cstr(out, c"") }),
            0
        );
        assert_eq!(buffer.remaining(), 6);
        assert_eq!(
            append_formatted(&mut buffer, |out| {
                out.write_all(&[(0i32) as u8])?;
                write!(out, ":{}", { 7i32 })
            }),
            3
        );
        assert_eq!(
            buffer.chunks().flatten().copied().collect::<Vec<_>>(),
            b"prefix\0:7"
        );
    }
}
