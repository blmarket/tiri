//! Storage contracts and differential tests against a flat byte sequence.
use tiri_buffer::{Buf, BufMut, Buffer, LineEnding, SegmentedBuf};

fn bytes(buffer: &SegmentedBuf) -> Vec<u8> {
    buffer.chunks().flatten().copied().collect()
}

fn parts(parts: &[&[u8]]) -> SegmentedBuf {
    let mut buffer = SegmentedBuf::default();
    for part in parts {
        let mut source = SegmentedBuf::default();
        source.put_slice(part);
        buffer.put(&mut source);
        assert!(!source.has_remaining());
    }
    buffer
}

#[test]
fn append_drain_and_reuse_preserve_binary_data() {
    let mut buffer = SegmentedBuf::default();
    assert!(buffer.chunks().next().is_none());
    buffer.put_slice(&[]);
    let data: Vec<_> = (0..20_000).map(|i| (i % 256) as u8).collect();
    buffer.put_slice(&data);
    assert!(buffer.chunks().len() > 1);
    buffer.advance(4101);
    buffer.put_slice(b"\0tail");
    assert_eq!(bytes(&buffer), [&data[4101..], b"\0tail"].concat());
    assert_eq!(buffer.remaining(), 20_000 - 4101 + 5);
    buffer.advance(buffer.remaining());
    assert!(!buffer.has_remaining());
    assert_eq!(buffer.chunks().len(), 0);
    buffer.put_slice(b"again");
    assert_eq!(bytes(&buffer), b"again");
}

#[test]
fn put_moves_payload_allocations_and_drains_preserve_them() {
    let mut source = parts(&[b"abc", b"defg", b"hij"]);
    source.advance(1);
    let addresses: Vec<_> = source.chunks().map(|s| s.as_ptr()).collect();
    let mut dest = parts(&[b"prefix"]);
    dest.put(&mut source);
    assert_eq!(
        dest.chunks()
            .skip(1)
            .map(|s| s.as_ptr())
            .collect::<Vec<_>>(),
        addresses
    );
    assert_eq!(bytes(&dest), b"prefixbcdefghij");
    assert!(!source.has_remaining());
    let mut empty = SegmentedBuf::default();
    let original = dest.chunks().next().unwrap().as_ptr();
    empty.put(&mut dest);
    assert_eq!(empty.chunks().next().unwrap().as_ptr(), original);
    assert!(!dest.has_remaining());
}

#[test]
fn pullup_only_coalesces_requested_prefix() {
    let mut buffer = parts(&[b"abc", b"defg", b"hijk"]);
    let first = buffer.chunks().next().unwrap().as_ptr();
    let last = buffer.chunks().next_back().unwrap().as_ptr();
    assert_eq!(buffer.pullup(2).unwrap().as_ptr(), first);
    assert!(buffer.pullup(12).is_none());
    assert_eq!(buffer.chunks().len(), 3);
    assert_eq!(buffer.pullup(0).unwrap(), b"");
    buffer.pullup(5).unwrap()[1] = b'X';
    assert_eq!(bytes(&buffer), b"aXcdefghijk");
    assert_eq!(
        buffer.chunks().map(|c| c.len()).collect::<Vec<_>>(),
        [5, 2, 4]
    );
    assert_eq!(buffer.chunks().next_back().unwrap().as_ptr(), last);
    assert_eq!(buffer.pullup(11).unwrap(), b"aXcdefghijk");
    assert_eq!(buffer.chunks().len(), 1);
}

fn reference_line(data: &mut Vec<u8>, policy: LineEnding) -> Option<Vec<u8>> {
    let (end, consumed) = match policy {
        LineEnding::Lf | LineEnding::Nul => {
            let delimiter = if policy == LineEnding::Lf { b'\n' } else { 0 };
            let p = data.iter().position(|b| *b == delimiter)?;
            (p, p + 1)
        }
        LineEnding::CrLf => {
            let p = data.iter().position(|b| *b == b'\n')?;
            (p - usize::from(p > 0 && data[p - 1] == b'\r'), p + 1)
        }
        LineEnding::CrLfStrict => {
            let p = data.windows(2).position(|w| w == b"\r\n")?;
            (p, p + 2)
        }
        LineEnding::Any | LineEnding::Legacy => {
            let p = data.iter().position(|b| matches!(b, b'\r' | b'\n'))?;
            let count = if policy == LineEnding::Any {
                data[p..]
                    .iter()
                    .take_while(|b| matches!(b, b'\r' | b'\n'))
                    .count()
            } else {
                1 + usize::from(
                    data.get(p + 1)
                        .is_some_and(|b| matches!(b, b'\r' | b'\n') && *b != data[p]),
                )
            };
            (p, p + count)
        }
    };
    let line = data[..end].to_vec();
    data.drain(..consumed);
    Some(line)
}

const POLICIES: [LineEnding; 6] = [
    LineEnding::Lf,
    LineEnding::CrLf,
    LineEnding::CrLfStrict,
    LineEnding::Nul,
    LineEnding::Any,
    LineEnding::Legacy,
];

#[test]
fn every_line_policy_works_at_every_chunk_boundary() {
    let data = b"\r\nalpha\0beta\r\r\n\n\rgamma\n\0tail\r";
    for policy in POLICIES {
        for split in 0..=data.len() {
            let mut buffer = parts(&[&data[..split], &data[split..]]);
            let mut flat = data.to_vec();
            loop {
                let expected = reference_line(&mut flat, policy);
                assert_eq!(
                    buffer.read_line(policy),
                    expected,
                    "{policy:?}, split {split}"
                );
                assert_eq!(bytes(&buffer), flat);
                if expected.is_none() {
                    break;
                }
            }
        }
    }
}

#[test]
fn incomplete_scans_resume_and_mutation_invalidates_them() {
    let mut buffer = SegmentedBuf::default();
    assert_eq!(buffer.read_line(LineEnding::CrLfStrict), None);
    for _ in 0..5000 {
        buffer.put_slice(b"x");
        assert_eq!(buffer.read_line(LineEnding::CrLfStrict), None);
    }
    buffer.put_slice(b"\r");
    assert_eq!(buffer.read_line(LineEnding::CrLfStrict), None);
    let mut tail = parts(&[b"\n", b"end"]);
    buffer.put(&mut tail);
    assert_eq!(
        buffer.read_line(LineEnding::CrLfStrict),
        Some(vec![b'x'; 5000])
    );
    assert_eq!(buffer.read_line(LineEnding::Lf), None);
    buffer.pullup(2).unwrap()[1] = b'\n';
    assert_eq!(buffer.read_line(LineEnding::Lf), Some(b"e".to_vec()));
    assert_eq!(bytes(&buffer), b"d");
    buffer.put_slice(b"\0");
    assert_eq!(buffer.read_line(LineEnding::Lf), None);
    assert_eq!(buffer.read_line(LineEnding::Nul), Some(b"d".to_vec()));
}

#[test]
fn generated_operation_sequences_match_flat_storage() {
    let mut seed = 0xfeed_beef_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut buffer = SegmentedBuf::default();
    let mut flat = Vec::new();
    for _ in 0..10_000 {
        let random = next();
        match random % 6 {
            0 | 1 => {
                let data: Vec<_> = (0..random as usize % 100)
                    .map(|_| {
                        let n = next();
                        match n % 8 {
                            0 => b'\n',
                            1 => b'\r',
                            2 => 0,
                            _ => n as u8,
                        }
                    })
                    .collect();
                if random % 2 == 0 {
                    buffer.put_slice(&data);
                } else {
                    let mut src = parts(&[&data]);
                    buffer.put(&mut src);
                }
                flat.extend(data);
            }
            2 => {
                let n = random as usize % (flat.len() + 5);
                buffer.advance(n.min(buffer.remaining()));
                flat.drain(..n.min(flat.len()));
            }
            3 => {
                let policy = POLICIES[(random >> 8) as usize % POLICIES.len()];
                assert_eq!(buffer.read_line(policy), reference_line(&mut flat, policy));
            }
            _ => {
                let n = random as usize % (flat.len() + 3);
                match buffer.pullup(n) {
                    Some(prefix) => {
                        assert_eq!(prefix, &flat[..n]);
                        if n > 0 {
                            prefix[n - 1] = b'\n';
                            flat[n - 1] = b'\n';
                        }
                    }
                    None => assert!(n > flat.len()),
                }
            }
        }
        assert_eq!(buffer.remaining(), flat.len());
        assert_eq!(bytes(&buffer), flat);
        assert!(buffer.chunks().all(|chunk| !chunk.is_empty()));
    }
}

#[test]
fn writable_capacity_commits_only_initialized_bytes_and_resumes_scans() {
    let mut buffer = SegmentedBuf::default();
    // Reserving a chunk, or abandoning a write, must not publish readable bytes.
    buffer.chunk_mut()[..3].copy_from_slice(b"old");
    assert_eq!(buffer.remaining(), 0);
    assert!(buffer.chunk().is_empty());
    assert_eq!(buffer.chunks().count(), 0);
    assert_eq!(buffer.read_line(LineEnding::CrLfStrict), None);
    let capacity = buffer.chunk_mut().len();
    buffer.chunk_mut()[..capacity].copy_from_slice(&vec![b'x'; capacity]);
    // SAFETY: the entire returned chunk was initialized above.
    unsafe { buffer.advance_mut(capacity) };
    assert_eq!(buffer.remaining(), capacity);
    assert_eq!(buffer.read_line(LineEnding::CrLfStrict), None);
    buffer.chunk_mut()[..2].copy_from_slice(b"\r\n");
    // SAFETY: exactly these two bytes were initialized above.
    unsafe { buffer.advance_mut(2) };
    assert_eq!(
        buffer.read_line(LineEnding::CrLfStrict),
        Some(vec![b'x'; capacity])
    );
    assert!(!buffer.has_remaining());
    // Committing zero is valid even without any writable allocation.
    unsafe { SegmentedBuf::default().advance_mut(0) };
    buffer.put_u32(0x01020304);
    assert_eq!(buffer.get_u32(), 0x01020304);
}

#[test]
fn standard_buf_consumers_and_vectored_access_cross_segments() {
    use std::io::IoSlice;
    let mut buffer = parts(&[b"ab", b"cde", b"fg"]);
    let mut slots = [IoSlice::new(&[]); 2];
    assert_eq!(buffer.chunks_vectored(&mut slots), 2);
    assert_eq!(&*slots[0], b"ab");
    assert_eq!(&*slots[1], b"cde");
    assert_eq!(buffer.chunks_vectored(&mut []), 0);
    let mut prefix = [0; 4];
    buffer.copy_to_slice(&mut prefix);
    assert_eq!(&prefix, b"abcd");
    assert_eq!(buffer.chunk(), b"e");
    let mut destination = SegmentedBuf::default();
    destination.put(&mut buffer);
    assert!(!buffer.has_remaining());
    assert_eq!(bytes(&destination), b"efg");
    let mut empty_slots = [IoSlice::new(&[]); 2];
    assert_eq!(buffer.chunks_vectored(&mut empty_slots), 0);
}

#[test]
fn advance_rejects_overrun_without_modifying_contents() {
    let mut buffer = parts(&[b"abc"]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        buffer.advance(4);
    }));
    assert!(result.is_err());
    assert_eq!(bytes(&buffer), b"abc");
    buffer.advance(0);
    assert_eq!(buffer.remaining(), 3);
}

#[test]
fn generic_put_specializes_owned_and_borrowed_sources() {
    fn append<D: BufMut, S: Buf>(destination: &mut D, source: S) {
        destination.put(source);
    }
    let mut source = parts(&[b"abc", b"def"]);
    source.advance(1);
    let pointers: Vec<_> = source.chunks().map(|chunk| chunk.as_ptr()).collect();
    let mut destination = parts(&[b"prefix"]);
    append(&mut destination, source);
    assert_eq!(bytes(&destination), b"prefixbcdef");
    assert_eq!(
        destination
            .chunks()
            .skip(1)
            .map(|c| c.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    let mut empty = SegmentedBuf::default();
    let pointers: Vec<_> = destination.chunks().map(|chunk| chunk.as_ptr()).collect();
    append(&mut empty, &mut destination);
    assert!(!destination.has_remaining());
    assert_eq!(
        empty.chunks().map(|c| c.as_ptr()).collect::<Vec<_>>(),
        pointers
    );
}

#[test]
fn generic_put_copies_foreign_and_wrapped_sources_and_consumes_them() {
    let mut destination = SegmentedBuf::default();
    let mut slice = &b"hello"[..];
    destination.put(&mut slice);
    assert!(slice.is_empty());
    let mut source = parts(&[b"ab", b"cdef"]);
    destination.put((&mut source).take(3));
    assert_eq!(bytes(&source), b"def");
    let erased: &mut dyn Buf = &mut source;
    destination.put(erased);
    assert!(!source.has_remaining());
    destination.put((&b"12"[..]).chain(&b"34"[..]));
    assert_eq!(bytes(&destination), b"helloabcdef1234");
}

#[test]
fn owned_vec_is_adopted_without_copying_and_can_be_transferred() {
    let bytes = vec![42; 65536];
    let pointer = bytes.as_ptr();
    let mut source = SegmentedBuf::from(bytes);
    assert_eq!(source.chunk().as_ptr(), pointer);
    let mut destination = SegmentedBuf::default();
    destination.put(&mut source);
    assert!(!source.has_remaining());
    assert_eq!(destination.chunk().as_ptr(), pointer);
    assert_eq!(destination.remaining(), 65536);
    let mut empty = SegmentedBuf::from(Vec::with_capacity(128));
    empty.put_slice(b"ready");
    assert_eq!(empty.chunk(), b"ready");
}
