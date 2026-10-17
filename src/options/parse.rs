//! Byte-oriented parsing for option names and array keys.
//!
//! The option table and option tree remain C-shaped data structures. This
//! module only describes the text grammar and name matching; callers that need
//! C strings continue to own the conversion and allocation at their ABI
//! boundary.

/// A parsed array key. Numeric keys are represented by their canonical value;
/// all other nonempty keys retain their original bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayIndex<'a> {
    Numeric(u32),
    Text(&'a [u8]),
}

/// Errors produced while parsing an array key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayIndexError {
    Empty,
    NumericOverflow,
}

/// Parse one complete array key without converting it to UTF-8.
pub fn parse_array_index(input: &[u8]) -> Result<ArrayIndex<'_>, ArrayIndexError> {
    if input.is_empty() {
        return Err(ArrayIndexError::Empty);
    }

    if !input.iter().all(u8::is_ascii_digit) {
        return Ok(ArrayIndex::Text(input));
    }

    let mut value = 0u32;
    for &byte in input {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u32::from(byte - b'0')))
            .ok_or(ArrayIndexError::NumericOverflow)?;
    }
    Ok(ArrayIndex::Numeric(value))
}

/// The result of splitting an option name from an optional array key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedOptionName<'a> {
    pub name: &'a [u8],
    pub array_key: Option<ArrayIndex<'a>>,
}

/// Errors produced while parsing an option name and its optional array key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionNameError {
    EmptyInput,
    MissingClosingBracket,
    TrailingInput,
    EmptyIndex,
    NumericOverflow,
}

/// Parse one complete option name, including its optional `[array-key]`
/// suffix.
pub fn parse_option_name(input: &[u8]) -> Result<ParsedOptionName<'_>, OptionNameError> {
    if input.is_empty() {
        return Err(OptionNameError::EmptyInput);
    }

    let Some(open) = input.iter().position(|&byte| byte == b'[') else {
        return Ok(ParsedOptionName {
            name: input,
            array_key: None,
        });
    };

    let Some(relative_close) = input[open + 1..].iter().position(|&byte| byte == b']') else {
        return Err(OptionNameError::MissingClosingBracket);
    };
    let close = open + 1 + relative_close;
    if close + 1 != input.len() {
        return Err(OptionNameError::TrailingInput);
    }

    let array_key = parse_array_index(&input[open + 1..close]).map_err(|error| match error {
        ArrayIndexError::Empty => OptionNameError::EmptyIndex,
        ArrayIndexError::NumericOverflow => OptionNameError::NumericOverflow,
    })?;
    Ok(ParsedOptionName {
        name: &input[..open],
        array_key: Some(array_key),
    })
}

/// The result of matching a parsed name against the built-in option table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionNameMatch {
    User,
    BuiltIn(usize),
}

/// Errors produced while resolving a name against the built-in option table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionNameMatchError {
    NotFound,
    Ambiguous,
}

/// Resolve a parsed, canonical option name against canonical table names.
///
/// `BuiltIn` contains the index of the matching canonical name in
/// `candidates`, allowing a C-facing adapter to retain the original table
/// pointer and allocation rules.
pub fn match_option_name(
    lookup_name: &[u8],
    candidates: &[&[u8]],
) -> Result<OptionNameMatch, OptionNameMatchError> {
    if lookup_name.first() == Some(&b'@') {
        return Ok(OptionNameMatch::User);
    }

    let mut found = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if **candidate == *lookup_name {
            return Ok(OptionNameMatch::BuiltIn(index));
        }
        if candidate.starts_with(lookup_name) {
            if found.is_some() {
                return Err(OptionNameMatchError::Ambiguous);
            }
            found = Some(index);
        }
    }

    found
        .map(OptionNameMatch::BuiltIn)
        .ok_or(OptionNameMatchError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::{
        match_option_name, parse_array_index, parse_option_name, ArrayIndex, ArrayIndexError,
        OptionNameError, OptionNameMatch, OptionNameMatchError,
    };

    #[test]
    fn array_index_cases() {
        let accepted = [
            (b"0".as_slice(), Ok(ArrayIndex::Numeric(0))),
            (b"0007".as_slice(), Ok(ArrayIndex::Numeric(7))),
            (b"4294967295".as_slice(), Ok(ArrayIndex::Numeric(u32::MAX))),
            (b"hook".as_slice(), Ok(ArrayIndex::Text(b"hook"))),
            (b"999999x".as_slice(), Ok(ArrayIndex::Text(b"999999x"))),
            (b"\xff".as_slice(), Ok(ArrayIndex::Text(b"\xff"))),
        ];
        for (input, expected) in accepted {
            assert_eq!(parse_array_index(input), expected, "input={input:?}");
        }

        let rejected = [
            (b"".as_slice(), ArrayIndexError::Empty),
            (b"4294967296".as_slice(), ArrayIndexError::NumericOverflow),
        ];
        for (input, expected) in rejected {
            assert_eq!(parse_array_index(input), Err(expected), "input={input:?}");
        }
    }

    #[test]
    fn option_name_cases() {
        let accepted = [
            (
                b"status".as_slice(),
                Ok(super::ParsedOptionName {
                    name: b"status",
                    array_key: None,
                }),
            ),
            (
                b"status[0007]".as_slice(),
                Ok(super::ParsedOptionName {
                    name: b"status",
                    array_key: Some(ArrayIndex::Numeric(7)),
                }),
            ),
            (
                b"@user[hook]".as_slice(),
                Ok(super::ParsedOptionName {
                    name: b"@user",
                    array_key: Some(ArrayIndex::Text(b"hook")),
                }),
            ),
            (
                b"[key]".as_slice(),
                Ok(super::ParsedOptionName {
                    name: b"",
                    array_key: Some(ArrayIndex::Text(b"key")),
                }),
            ),
            (
                b"status[[x]".as_slice(),
                Ok(super::ParsedOptionName {
                    name: b"status",
                    array_key: Some(ArrayIndex::Text(b"[x")),
                }),
            ),
        ];
        for (input, expected) in accepted {
            assert_eq!(parse_option_name(input), expected, "input={input:?}");
        }

        let rejected = [
            (b"".as_slice(), OptionNameError::EmptyInput),
            (b"status[]".as_slice(), OptionNameError::EmptyIndex),
            (
                b"status[0".as_slice(),
                OptionNameError::MissingClosingBracket,
            ),
            (b"status[0]tail".as_slice(), OptionNameError::TrailingInput),
            (
                b"status[4294967296]".as_slice(),
                OptionNameError::NumericOverflow,
            ),
        ];
        for (input, expected) in rejected {
            assert_eq!(parse_option_name(input), Err(expected), "input={input:?}");
        }
    }

    #[test]
    fn option_name_matching_cases() {
        let candidates = [b"status".as_slice(), b"status-bg", b"prefix", b"prefix2"];
        let cases = [
            (b"status".as_slice(), Ok(OptionNameMatch::BuiltIn(0))),
            (b"status-b".as_slice(), Ok(OptionNameMatch::BuiltIn(1))),
            (b"@user".as_slice(), Ok(OptionNameMatch::User)),
            (b"".as_slice(), Err(OptionNameMatchError::Ambiguous)),
            (b"prefix".as_slice(), Ok(OptionNameMatch::BuiltIn(2))),
            (b"pre".as_slice(), Err(OptionNameMatchError::Ambiguous)),
            (b"missing".as_slice(), Err(OptionNameMatchError::NotFound)),
        ];
        for (input, expected) in cases {
            assert_eq!(
                match_option_name(input, &candidates),
                expected,
                "input={input:?}"
            );
        }
    }
}
