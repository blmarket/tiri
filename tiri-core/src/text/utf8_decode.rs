//! Locale-independent decoding of one UTF-8 code point from a byte slice.

/// The result of decoding the first code point in a byte slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeResult {
    /// The first code point is valid. `len` is the number of bytes it uses;
    /// bytes after it are intentionally left for the caller.
    Complete { codepoint: u32, len: usize },
    /// The prefix is valid so far, but another byte is needed to decide it.
    Incomplete { expected: usize },
    /// The prefix cannot be a UTF-8 code point. `consumed` reaches through
    /// the byte that made the prefix invalid.
    Invalid { consumed: usize },
}

const MAX_CODEPOINT: u32 = 0x10ffff;

fn is_continuation(byte: u8) -> bool {
    byte & 0xc0 == 0x80
}

fn valid_second_byte(first: u8, second: u8) -> bool {
    if !is_continuation(second) {
        return false;
    }
    match first {
        0xe0 => second >= 0xa0,
        0xed => second <= 0x9f,
        0xf0 => second >= 0x90,
        0xf4 => second <= 0x8f,
        _ => true,
    }
}

/// Decode the first UTF-8 code point in `input`.
///
/// This deliberately does not consult a locale or a display-width table.
/// `Complete` consumes only the first code point, while `Incomplete` and
/// `Invalid` describe the prefix supplied by the caller. This makes the
/// helper usable by both bounded input and streaming adapters.
pub fn decode_utf8(input: &[u8]) -> DecodeResult {
    let Some(&first) = input.first() else {
        return DecodeResult::Incomplete { expected: 1 };
    };

    let (expected, minimum) = match first {
        0x00..=0x7f => {
            return DecodeResult::Complete {
                codepoint: first as u32,
                len: 1,
            };
        }
        0xc2..=0xdf => (2, 0x80),
        0xe0..=0xef => (3, 0x800),
        0xf0..=0xf4 => (4, 0x10000),
        _ => return DecodeResult::Invalid { consumed: 1 },
    };

    if input.len() < expected {
        if input.len() >= 2 && !valid_second_byte(first, input[1]) {
            return DecodeResult::Invalid { consumed: 2 };
        }
        for (index, &byte) in input.iter().enumerate().skip(2) {
            if !is_continuation(byte) {
                return DecodeResult::Invalid {
                    consumed: index + 1,
                };
            }
        }
        return DecodeResult::Incomplete { expected };
    }

    if !valid_second_byte(first, input[1]) {
        return DecodeResult::Invalid { consumed: 2 };
    }
    for (index, &byte) in input.iter().enumerate().take(expected).skip(2) {
        if !is_continuation(byte) {
            return DecodeResult::Invalid {
                consumed: index + 1,
            };
        }
    }

    let codepoint = match expected {
        2 => ((first as u32 & 0x1f) << 6) | (input[1] as u32 & 0x3f),
        3 => {
            ((first as u32 & 0x0f) << 12)
                | ((input[1] as u32 & 0x3f) << 6)
                | (input[2] as u32 & 0x3f)
        }
        4 => {
            ((first as u32 & 0x07) << 18)
                | ((input[1] as u32 & 0x3f) << 12)
                | ((input[2] as u32 & 0x3f) << 6)
                | (input[3] as u32 & 0x3f)
        }
        _ => unreachable!(),
    };

    if codepoint < minimum || codepoint > MAX_CODEPOINT || (0xd800..=0xdfff).contains(&codepoint) {
        return DecodeResult::Invalid { consumed: expected };
    }

    DecodeResult::Complete {
        codepoint,
        len: expected,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_utf8, DecodeResult};

    #[test]
    fn truncated_sequences_are_incomplete() {
        assert_eq!(decode_utf8(&[]), DecodeResult::Incomplete { expected: 1 });
        assert_eq!(
            decode_utf8(&[0xc2]),
            DecodeResult::Incomplete { expected: 2 }
        );
        assert_eq!(
            decode_utf8(&[0xe2, 0x82]),
            DecodeResult::Incomplete { expected: 3 }
        );
        assert_eq!(
            decode_utf8(&[0xf0, 0x9f, 0x98]),
            DecodeResult::Incomplete { expected: 4 }
        );
    }

    #[test]
    fn invalid_continuations_report_the_consumed_prefix() {
        assert_eq!(
            decode_utf8(&[0xe2, 0x28, 0xa1]),
            DecodeResult::Invalid { consumed: 2 }
        );
        assert_eq!(
            decode_utf8(&[0xf0, 0x90, 0x28, 0x80]),
            DecodeResult::Invalid { consumed: 3 }
        );
        assert_eq!(decode_utf8(&[0x80]), DecodeResult::Invalid { consumed: 1 });
        assert_eq!(
            decode_utf8(&[0xc0, 0x80]),
            DecodeResult::Invalid { consumed: 1 }
        );
    }

    #[test]
    fn overlong_and_surrogate_encodings_are_invalid() {
        for (bytes, consumed) in [
            (&[0xe0, 0x80, 0x80][..], 2),
            (&[0xe0, 0x9f, 0xbf][..], 2),
            (&[0xf0, 0x80, 0x80, 0x80][..], 2),
            (&[0xf0, 0x8f, 0xbf, 0xbf][..], 2),
            (&[0xed, 0xa0, 0x80][..], 2),
            (&[0xed, 0xbf, 0xbf][..], 2),
        ] {
            assert_eq!(decode_utf8(bytes), DecodeResult::Invalid { consumed });
        }
    }

    #[test]
    fn nul_and_scalar_boundaries_decode_without_locale_state() {
        let cases = [
            (&[0x00][..], 0x0000),
            (&[0x7f][..], 0x007f),
            (&[0xc2, 0x80][..], 0x0080),
            (&[0xdf, 0xbf][..], 0x07ff),
            (&[0xe0, 0xa0, 0x80][..], 0x0800),
            (&[0xed, 0x9f, 0xbf][..], 0xd7ff),
            (&[0xee, 0x80, 0x80][..], 0xe000),
            (&[0xef, 0xbf, 0xbf][..], 0xffff),
            (&[0xf0, 0x90, 0x80, 0x80][..], 0x10000),
            (&[0xf4, 0x8f, 0xbf, 0xbf][..], 0x10ffff),
        ];
        for (bytes, codepoint) in cases {
            assert_eq!(
                decode_utf8(bytes),
                DecodeResult::Complete {
                    codepoint,
                    len: bytes.len(),
                }
            );
        }
        assert_eq!(
            decode_utf8(&[0x00, b'x']),
            DecodeResult::Complete {
                codepoint: 0,
                len: 1,
            }
        );
    }
}
