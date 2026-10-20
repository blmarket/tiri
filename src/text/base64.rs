//! RFC 4648 base64 with the acceptance rules of the ISC resolver's
//! `b64_ntop`/`b64_pton`, which tmux uses for OSC 52 clipboard payloads.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const PAD: u8 = b'=';

/// Encode with `=` padding, as `b64_ntop` writes it (without its NUL).
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let word = (chunk[0] as u32) << 16
            | (chunk.get(1).copied().unwrap_or(0) as u32) << 8
            | chunk.get(2).copied().unwrap_or(0) as u32;
        for (position, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if position <= chunk.len() {
                ALPHABET[(word >> shift & 0x3f) as usize]
            } else {
                PAD
            });
        }
    }
    out
}

/// Decode by `b64_pton`'s rules, returning `None` where it returned -1:
///
/// - input ends at the first NUL;
/// - C-locale whitespace is skipped anywhere, padding included;
/// - unpadded input with a partial group, `=` after fewer than two symbols
///   of a group, a single `=` after two, and anything but whitespace after
///   the padding are rejected;
/// - the bits past the last whole byte must be zero (`aGk=` decodes, `aGl=`
///   does not).
///
/// `b64_pton`'s target-size checks are not reproduced; every caller sized
/// the target for the longest possible output.
pub fn decode(text: &[u8]) -> Option<Vec<u8>> {
    let text = text
        .iter()
        .position(|&byte| byte == 0)
        .map_or(text, |nul| &text[..nul]);
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
    // The partial byte `b64_pton` keeps in `target[tarindex]`.
    let mut pending = 0u8;
    let mut state = 0;
    let mut rest = text.iter();
    let mut padded = false;
    for &byte in rest.by_ref() {
        if is_space(byte) {
            continue;
        }
        if byte == PAD {
            padded = true;
            break;
        }
        let value = ALPHABET.iter().position(|&symbol| symbol == byte)? as u8;
        match state {
            0 => pending = value << 2,
            1 => {
                out.push(pending | value >> 4);
                pending = (value & 0x0f) << 4;
            }
            2 => {
                out.push(pending | value >> 2);
                pending = (value & 0x03) << 6;
            }
            _ => {
                out.push(pending | value);
                pending = 0;
            }
        }
        state = (state + 1) % 4;
    }
    if !padded {
        return (state == 0).then_some(out);
    }
    let mut rest = rest.copied().skip_while(|&byte| is_space(byte));
    match state {
        0 | 1 => return None,
        2 => {
            if rest.next() != Some(PAD) {
                return None;
            }
        }
        _ => {}
    }
    (rest.all(is_space) && pending == 0).then_some(out)
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_pads_each_partial_group() {
        assert_eq!(encode(b""), b"");
        assert_eq!(encode(b"h"), b"aA==");
        assert_eq!(encode(b"hi"), b"aGk=");
        assert_eq!(encode(b"hel"), b"aGVs");
        assert_eq!(encode(b"hello"), b"aGVsbG8=");
        assert_eq!(encode(&[0xfb, 0xff, 0xbf]), b"+/+/");
    }

    #[test]
    fn decode_round_trips_every_length() {
        let data: Vec<u8> = (0..=255).collect();
        for len in 0..data.len() {
            assert_eq!(decode(&encode(&data[..len])).as_deref(), Some(&data[..len]));
        }
    }

    #[test]
    fn decode_matches_the_osc52_conformance_payloads() {
        // The data after the selection separator in
        // tests/core/clipboard.rs::osc52_pane_payload_forms_match_tmux; the
        // empty payload is dropped by its caller before decoding.
        assert_eq!(decode(b"aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode(b"aGk"), None);
        assert_eq!(decode(b"!!!!"), None);
        assert_eq!(decode(b"").as_deref(), Some(&b""[..]));
        assert_eq!(decode(b"aGVsbG8=").as_deref(), Some(&b"hello"[..]));
    }

    #[test]
    fn decode_stops_at_the_first_nul() {
        assert_eq!(decode(b"aGk=\0!!").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode(b"aGVs\0bG8=").as_deref(), Some(&b"hel"[..]));
        assert_eq!(decode(b"aG\0k="), None);
    }

    #[test]
    fn decode_skips_whitespace_anywhere() {
        let spaces = [b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'];
        for space in spaces {
            for position in 0..=4 {
                let mut text = b"aGk=".to_vec();
                text.insert(position, space);
                assert_eq!(decode(&text).as_deref(), Some(&b"hi"[..]), "{text:?}");
                let mut text = b"aA==".to_vec();
                text.insert(position, space);
                assert_eq!(decode(&text).as_deref(), Some(&b"h"[..]), "{text:?}");
            }
        }
        assert_eq!(decode(b" \t\n").as_deref(), Some(&b""[..]));
        assert_eq!(decode(b"aA= \t=\n ").as_deref(), Some(&b"h"[..]));
    }

    #[test]
    fn decode_rejects_bytes_outside_the_alphabet() {
        assert_eq!(decode(b"aG-k"), None);
        assert_eq!(decode(b"aGk_"), None);
        assert_eq!(decode(&[b'a', b'G', 0x80, b'k']), None);
        assert_eq!(decode(&[b'a', b'G', b'k', 0xa0, b'=']), None);
    }

    #[test]
    fn decode_requires_whole_groups_without_padding() {
        assert_eq!(decode(b"a"), None);
        assert_eq!(decode(b"aG"), None);
        assert_eq!(decode(b"aGk"), None);
        assert_eq!(decode(b"aGVsb"), None);
    }

    #[test]
    fn decode_rejects_padding_after_fewer_than_two_symbols() {
        assert_eq!(decode(b"="), None);
        assert_eq!(decode(b"===="), None);
        assert_eq!(decode(b"a==="), None);
        assert_eq!(decode(b"aGVs="), None);
        assert_eq!(decode(b"aGVsb==="), None);
    }

    #[test]
    fn decode_requires_a_second_pad_after_two_symbols() {
        assert_eq!(decode(b"aA="), None);
        assert_eq!(decode(b"aA=x"), None);
        assert_eq!(decode(b"aA=  "), None);
        assert_eq!(decode(b"aA==").as_deref(), Some(&b"h"[..]));
    }

    #[test]
    fn decode_rejects_anything_but_whitespace_after_padding() {
        assert_eq!(decode(b"aGk=a"), None);
        assert_eq!(decode(b"aGk=="), None);
        assert_eq!(decode(b"aA===").as_deref(), None);
        assert_eq!(decode(b"aA==aGk="), None);
        assert_eq!(decode(b"aGk= \n").as_deref(), Some(&b"hi"[..]));
    }

    #[test]
    fn decode_requires_zero_spill_bits() {
        assert_eq!(decode(b"aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode(b"aA==").as_deref(), Some(&b"h"[..]));
        assert_eq!(decode(b"aGl="), None);
        assert_eq!(decode(b"aG=="), None);
    }
}
