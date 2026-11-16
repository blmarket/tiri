//! Storage contracts and differential tests against a flat byte sequence.
use std::io::Write as _;
use tiri_buffer::{LineEnding, SegmentedBuf};

fn bytes(buffer: &SegmentedBuf) -> Vec<u8> {
    buffer.chunks().flatten().copied().collect()
}

fn parts(parts: &[&[u8]]) -> SegmentedBuf {
    let mut buffer = SegmentedBuf::default();
    for part in parts {
        let mut source = SegmentedBuf::default();
        source.extend_from_slice(part);
        buffer.append(&mut source);
        assert!(source.is_empty());
    }
    buffer
}

#[test]
fn append_drain_and_reuse_preserve_binary_data() {
    let mut buffer = SegmentedBuf::default();
    assert!(buffer.chunks().next().is_none());
    buffer.extend_from_slice(&[]);
    let data: Vec<_> = (0..20_000).map(|i| (i % 256) as u8).collect();
    buffer.extend_from_slice(&data);
    assert!(buffer.chunks().count() > 1);
    buffer.advance(4101);
    buffer.extend_from_slice(b"\0tail");
    assert_eq!(bytes(&buffer), [&data[4101..], b"\0tail"].concat());
    assert_eq!(buffer.len(), 20_000 - 4101 + 5);
    buffer.advance(buffer.len());
    assert!(buffer.is_empty());
    assert_eq!(buffer.chunks().count(), 0);
    buffer.extend_from_slice(b"again");
    assert_eq!(bytes(&buffer), b"again");
}

#[test]
fn append_moves_payload_allocations_and_drains_preserve_them() {
    let mut source = parts(&[b"abc", b"defg", b"hij"]);
    source.advance(1);
    let addresses: Vec<_> = source.chunks().map(|s| s.as_ptr()).collect();
    let mut dest = parts(&[b"prefix"]);
    dest.append(&mut source);
    assert_eq!(
        dest.chunks()
            .skip(1)
            .map(|s| s.as_ptr())
            .collect::<Vec<_>>(),
        addresses
    );
    assert_eq!(bytes(&dest), b"prefixbcdefghij");
    assert!(source.is_empty());
    let mut empty = SegmentedBuf::default();
    let original = dest.chunks().next().unwrap().as_ptr();
    empty.append(&mut dest);
    assert_eq!(empty.chunks().next().unwrap().as_ptr(), original);
    assert!(dest.is_empty());
}

#[test]
fn make_contiguous_and_copy_helpers_preserve_contents_and_single_chunk_pointer() {
    let mut empty = SegmentedBuf::default();
    assert!(empty.make_contiguous().is_empty());
    assert!(empty.copy_prefix(10).is_empty());
    assert_eq!(empty.copy_slice(0, &mut [0; 4]), 0);

    let mut single = SegmentedBuf::from(b"hello".to_vec());
    let ptr = single.chunks().next().unwrap().as_ptr();
    assert_eq!(single.make_contiguous().as_ptr(), ptr);

    let mut buffer = parts(&[b"abc", b"defg", b"hijk"]);
    assert_eq!(buffer.copy_prefix(5), b"abcde");
    assert_eq!(buffer.copy_prefix(99), b"abcdefghijk");
    let mut slice = [0; 6];
    assert_eq!(buffer.copy_slice(2, &mut slice), 6);
    assert_eq!(&slice, b"cdefgh");
    assert_eq!(buffer.copy_slice(9, &mut slice), 2);
    assert_eq!(&slice[..2], b"jk");
    assert_eq!(buffer.copy_slice(11, &mut slice), 0);
    assert_eq!(buffer.chunks().count(), 3);

    buffer.make_contiguous()[1] = b'X';
    assert_eq!(bytes(&buffer), b"aXcdefghijk");
    assert_eq!(buffer.chunks().count(), 1);
}

fn reference_line(data: &mut Vec<u8>, policy: LineEnding) -> Option<Vec<u8>> {
    let (end, consumed) = match policy {
        LineEnding::Lf => {
            let p = data.iter().position(|b| *b == b'\n')?;
            (p, p + 1)
        }
        LineEnding::Legacy => {
            let p = data.iter().position(|b| matches!(b, b'\r' | b'\n'))?;
            let count = 1 + usize::from(
                data.get(p + 1)
                    .is_some_and(|b| matches!(b, b'\r' | b'\n') && *b != data[p]),
            );
            (p, p + count)
        }
    };
    let line = data[..end].to_vec();
    data.drain(..consumed);
    Some(line)
}

const POLICIES: [LineEnding; 2] = [LineEnding::Lf, LineEnding::Legacy];

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
    assert_eq!(buffer.read_line(LineEnding::Lf), None);
    for _ in 0..5000 {
        buffer.extend_from_slice(b"x");
        assert_eq!(buffer.read_line(LineEnding::Lf), None);
    }
    buffer.extend_from_slice(b"\r");
    assert_eq!(buffer.read_line(LineEnding::Lf), None);
    let mut tail = parts(&[b"\n", b"end"]);
    buffer.append(&mut tail);
    let mut expected = vec![b'x'; 5000];
    expected.push(b'\r');
    assert_eq!(buffer.read_line(LineEnding::Lf), Some(expected));
    assert_eq!(buffer.read_line(LineEnding::Legacy), None);
    buffer.make_contiguous()[1] = b'\n';
    assert_eq!(buffer.read_line(LineEnding::Legacy), Some(b"e".to_vec()));
    assert_eq!(bytes(&buffer), b"d");
    buffer.extend_from_slice(b"\r");
    assert_eq!(buffer.read_line(LineEnding::Lf), None);
    assert_eq!(buffer.read_line(LineEnding::Legacy), Some(b"d".to_vec()));
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
                    buffer.extend_from_slice(&data);
                } else {
                    let mut src = parts(&[&data]);
                    buffer.append(&mut src);
                }
                flat.extend(data);
            }
            2 => {
                let n = random as usize % (flat.len() + 5);
                buffer.advance(n.min(buffer.len()));
                flat.drain(..n.min(flat.len()));
            }
            3 => {
                let policy = POLICIES[(random >> 8) as usize % POLICIES.len()];
                assert_eq!(buffer.read_line(policy), reference_line(&mut flat, policy));
            }
            4 => {
                let n = random as usize % (flat.len() + 3);
                assert_eq!(buffer.copy_prefix(n), flat[..n.min(flat.len())]);
            }
            _ => {
                let contiguous = buffer.make_contiguous();
                assert_eq!(contiguous, flat.as_slice());
                if !contiguous.is_empty() {
                    let idx = random as usize % contiguous.len();
                    contiguous[idx] = b'\n';
                    flat[idx] = b'\n';
                }
            }
        }
        assert_eq!(buffer.len(), flat.len());
        assert_eq!(bytes(&buffer), flat);
        assert!(buffer.chunks().all(|chunk| !chunk.is_empty()));
    }
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
    assert_eq!(buffer.len(), 3);
}

#[test]
fn write_trait_and_owned_vec_adoption_preserve_tail_capacity() {
    let mut bytes = b"header".to_vec();
    bytes.reserve(8);
    let pointer = bytes.as_ptr();
    let mut source = SegmentedBuf::from(bytes);
    assert_eq!(source.chunks().next().unwrap().as_ptr(), pointer);
    let mut destination = SegmentedBuf::default();
    destination.append(&mut source);
    assert!(source.is_empty());
    write!(&mut destination, ":{}", 42).unwrap();
    assert_eq!(destination.chunks().count(), 1);
    assert_eq!(destination.chunks().next().unwrap().as_ptr(), pointer);
    assert_eq!(destination.make_contiguous(), b"header:42");

    let mut empty = SegmentedBuf::from(Vec::with_capacity(128));
    empty.extend_from_slice(b"ready");
    assert_eq!(empty.chunks().next().unwrap(), b"ready");
}
