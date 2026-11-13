//! OpenBSD strunvis(3) over bytes.

#[derive(Clone, Copy)]
enum State {
    Ground,
    Start,
    Meta,
    Meta1,
    Ctrl,
    Octal2,
    Octal3,
}

/// What `unvis(3)` reports for one input byte; `None` is `UNVIS_SYNBAD`.
enum Step {
    /// The byte was consumed with nothing to emit (0 or `UNVIS_NOCHAR`).
    Pending,
    /// The value is complete (`UNVIS_VALID`).
    Valid,
    /// The value is complete and the byte must be read again
    /// (`UNVIS_VALIDPUSH`), after one or two octal digits.
    ValidPush,
}

/// One step of the state machine. `value` is the output cell the C function
/// wrote through `cp`: cleared on every byte in the ground state, and only
/// partly set by the states that build it.
fn unvis(state: &mut State, value: &mut u8, c: u8) -> Option<Step> {
    let step = match *state {
        State::Ground => {
            *value = 0;
            if c == b'\\' {
                *state = State::Start;
                return Some(Step::Pending);
            }
            *value = c;
            Step::Valid
        }
        State::Start => {
            *state = State::Ground;
            let decoded = match c {
                b'\\' => c,
                b'0'..=b'7' => {
                    *value = c - b'0';
                    *state = State::Octal2;
                    return Some(Step::Pending);
                }
                b'M' => {
                    *value = 0o200;
                    *state = State::Meta;
                    return Some(Step::Pending);
                }
                b'^' => {
                    *state = State::Ctrl;
                    return Some(Step::Pending);
                }
                b'n' => b'\n',
                b'r' => b'\r',
                b'b' => 0x08,
                b'a' => 0x07,
                b'v' => 0x0b,
                b't' => b'\t',
                b'f' => 0x0c,
                b's' => b' ',
                b'E' => 0x1b,
                // An escaped newline or `$` produces nothing.
                b'\n' | b'$' => return Some(Step::Pending),
                _ => return None,
            };
            *value = decoded;
            Step::Valid
        }
        State::Meta => {
            *state = match c {
                b'-' => State::Meta1,
                b'^' => State::Ctrl,
                _ => {
                    *state = State::Ground;
                    return None;
                }
            };
            Step::Pending
        }
        State::Meta1 => {
            *state = State::Ground;
            *value |= c;
            Step::Valid
        }
        State::Ctrl => {
            *state = State::Ground;
            *value |= if c == b'?' { 0o177 } else { c & 0o37 };
            Step::Valid
        }
        State::Octal2 | State::Octal3 if (b'0'..=b'7').contains(&c) => {
            // Three digits can exceed a byte; the C `char` kept the low bits.
            *value = (*value << 3) + (c - b'0');
            if matches!(*state, State::Octal3) {
                *state = State::Ground;
                Step::Valid
            } else {
                *state = State::Octal3;
                Step::Pending
            }
        }
        State::Octal2 | State::Octal3 => {
            *state = State::Ground;
            Step::ValidPush
        }
    };
    Some(step)
}

/// Decode `src` as OpenBSD `strunvis(3)` does, returning `None` where it
/// returned -1.
///
/// Input ends at the first NUL, as the C loop did; callers pass C string
/// bytes. At the end a pending octal value is emitted, and an unfinished
/// `\`, `\M`, `\M-` or `\^` is dropped without an error, because `strunvis`
/// ignores everything `unvis(UNVIS_END)` reports except a valid character.
/// The output may contain NUL bytes (`\0`, `\^@`).
pub fn strunvis(src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(src.len());
    let mut state = State::Ground;
    let mut value = 0;
    for &c in src.iter().take_while(|&&c| c != 0) {
        loop {
            match unvis(&mut state, &mut value, c)? {
                Step::Pending => break,
                Step::Valid => {
                    out.push(value);
                    break;
                }
                Step::ValidPush => out.push(value),
            }
        }
    }
    if matches!(state, State::Octal2 | State::Octal3) {
        out.push(value);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(src: &[u8]) -> Vec<u8> {
        strunvis(src).expect("valid escapes")
    }

    #[test]
    fn plain_bytes_and_escaped_backslashes_pass_through() {
        assert_eq!(decoded(b""), b"");
        assert_eq!(decoded(b"ab\xff"), b"ab\xff");
        assert_eq!(decoded(b"a\\\\b"), b"a\\b");
    }

    #[test]
    fn each_named_escape() {
        let cases: [(&[u8], u8); 9] = [
            (b"\\n", b'\n'),
            (b"\\r", b'\r'),
            (b"\\b", 0x08),
            (b"\\a", 0x07),
            (b"\\v", 0x0b),
            (b"\\t", b'\t'),
            (b"\\f", 0x0c),
            (b"\\s", b' '),
            (b"\\E", 0x1b),
        ];
        for (src, byte) in cases {
            assert_eq!(decoded(src), [byte], "{src:?}");
        }
        assert_eq!(decoded(b"a\\\nb\\$c"), b"abc");
    }

    #[test]
    fn meta_and_control_forms() {
        assert_eq!(decoded(b"\\M-a"), [0xe1]);
        assert_eq!(decoded(b"\\M-\xff"), [0xff]);
        assert_eq!(decoded(b"\\M^A"), [0x81]);
        assert_eq!(decoded(b"\\M^?"), [0xff]);
        assert_eq!(decoded(b"\\^A"), [0x01]);
        assert_eq!(decoded(b"\\^a"), [0x01]);
        assert_eq!(decoded(b"\\^?"), [0x7f]);
        assert_eq!(decoded(b"\\^@"), [0x00]);
    }

    #[test]
    fn octal_digits_and_the_push_back() {
        assert_eq!(decoded(b"\\101"), b"A");
        assert_eq!(decoded(b"\\1012"), b"A2");
        assert_eq!(decoded(b"\\377"), [0xff]);
        // Three digits keep only the low eight bits.
        assert_eq!(decoded(b"\\777"), [0xff]);
        // The first non-octal byte ends the number and is read again.
        assert_eq!(decoded(b"\\1x"), b"\x01x");
        assert_eq!(decoded(b"\\18"), b"\x018");
        assert_eq!(decoded(b"\\12x"), b"\x0ax");
        assert_eq!(decoded(b"\\1\\n"), b"\x01\n");
        assert_eq!(decoded(b"\\12\\n"), b"\x0a\n");
    }

    #[test]
    fn syntax_errors() {
        assert_eq!(strunvis(b"\\x"), None);
        assert_eq!(strunvis(b"a\\8"), None);
        assert_eq!(strunvis(b"\\Mx"), None);
        assert_eq!(strunvis(b"\\M"), Some(Vec::new()));
        assert_eq!(strunvis(b"ok\\q"), None);
    }

    #[test]
    fn end_of_input_emits_octal_and_drops_unfinished_escapes() {
        assert_eq!(decoded(b"\\1"), [0x01]);
        assert_eq!(decoded(b"\\12"), [0x0a]);
        assert_eq!(decoded(b"a\\"), b"a");
        assert_eq!(decoded(b"a\\M"), b"a");
        assert_eq!(decoded(b"a\\M-"), b"a");
        assert_eq!(decoded(b"a\\M^"), b"a");
        assert_eq!(decoded(b"a\\^"), b"a");
    }

    #[test]
    fn input_ends_at_the_first_nul() {
        assert_eq!(decoded(b"a\0\\x"), b"a");
        assert_eq!(decoded(b"\\1\0"), [0x01]);
    }
}
