//! Short-option scanning with the rules of OpenBSD getopt(3) as `getopt_long`
//! implements it for a plain `getopt` call: no permutation, so scanning stops
//! at the first operand, and no long options.
//!
//! The spec may not contain `-`, and may not start with `+`, `-` or `:`;
//! those select modes the only caller never used. The scan position after an
//! error is unspecified, because the caller exits: the C code left `optind`
//! on an unknown option that had more text after it, and that is not
//! reproduced.

use std::ffi::{CStr, CString};
use std::fmt;

pub struct Getopt<'a> {
    args: &'a [CString],
    spec: &'static [u8],
    index: usize,
    /// The unread rest of a group of flags in `args[index]`.
    place: &'a CStr,
}

pub struct Opt<'a> {
    pub flag: u8,
    pub arg: Option<&'a CStr>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GetoptError {
    UnknownOption(u8),
    MissingArgument(u8),
}

impl fmt::Display for GetoptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            GetoptError::UnknownOption(flag) => {
                write!(f, "unknown option -- {}", flag as char)
            }
            GetoptError::MissingArgument(flag) => {
                write!(f, "option requires an argument -- {}", flag as char)
            }
        }
    }
}

/// `text` without its first byte, still borrowed from the argument.
fn rest(text: &CStr) -> &CStr {
    CStr::from_bytes_with_nul(&text.to_bytes_with_nul()[1..]).expect("suffix of a C string")
}

impl<'a> Getopt<'a> {
    /// Scan `args` from `args[1]`, as `argv[0]` is the program name.
    pub fn new(args: &'a [CString], spec: &'static [u8]) -> Self {
        assert!(
            !spec.contains(&b'-') && !matches!(spec.first(), Some(b'+' | b':')),
            "unsupported getopt spec"
        );
        Getopt {
            args,
            spec,
            index: 1,
            place: c"",
        }
    }

    fn scan(&mut self) -> Result<Option<Opt<'a>>, GetoptError> {
        if self.place.is_empty() {
            let Some(arg) = self.args.get(self.index) else {
                return Ok(None);
            };
            let bytes = arg.to_bytes();
            // A lone `-` is an operand.
            if bytes.first() != Some(&b'-') || bytes.len() == 1 {
                return Ok(None);
            }
            if bytes == b"--" {
                self.index += 1;
                return Ok(None);
            }
            self.place = rest(arg);
        }
        let flag = self.place.to_bytes()[0];
        self.place = rest(self.place);
        let found = if flag == b':' || flag == b'-' && !self.place.is_empty() {
            None
        } else {
            self.spec.iter().position(|&byte| byte == flag)
        };
        let Some(position) = found else {
            // A `-` ending a group stops scanning and leaves the whole
            // argument, `-v-` say, as the first operand.
            if flag == b'-' && self.place.is_empty() {
                return Ok(None);
            }
            return Err(GetoptError::UnknownOption(flag));
        };
        if self.spec.get(position + 1) != Some(&b':') {
            if self.place.is_empty() {
                self.index += 1;
            }
            return Ok(Some(Opt { flag, arg: None }));
        }
        let arg = if !self.place.is_empty() {
            self.place
        } else {
            self.index += 1;
            self.args
                .get(self.index)
                .ok_or(GetoptError::MissingArgument(flag))?
                .as_c_str()
        };
        self.place = c"";
        self.index += 1;
        Ok(Some(Opt {
            flag,
            arg: Some(arg),
        }))
    }

    /// The operands after scanning stopped.
    pub fn remaining(&self) -> &'a [CString] {
        &self.args[self.index.min(self.args.len())..]
    }
}

/// Each option until scanning stops; the caller stops at the first error.
impl<'a> Iterator for Getopt<'a> {
    type Item = Result<Opt<'a>, GetoptError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.scan().transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &[u8] = b"2c:CDdf:hlL:NqS:T:uUvV";

    fn args(list: &[&str]) -> Vec<CString> {
        std::iter::once("tiri")
            .chain(list.iter().copied())
            .map(|arg| CString::new(arg).unwrap())
            .collect()
    }

    /// Every option as `flag` or `flag=arg`, then the remaining arguments.
    fn scan(list: &[&str]) -> Result<(Vec<String>, Vec<String>), GetoptError> {
        let args = args(list);
        let mut getopt = Getopt::new(&args, SPEC);
        let mut options = Vec::new();
        for opt in getopt.by_ref() {
            let opt = opt?;
            options.push(match opt.arg {
                Some(arg) => format!("{}={}", opt.flag as char, arg.to_str().unwrap()),
                None => (opt.flag as char).to_string(),
            });
        }
        let remaining = getopt
            .remaining()
            .iter()
            .map(|arg| arg.to_str().unwrap().to_owned())
            .collect();
        Ok((options, remaining))
    }

    fn ok(options: &[&str], remaining: &[&str]) -> Result<(Vec<String>, Vec<String>), GetoptError> {
        Ok((
            options.iter().map(|s| s.to_string()).collect(),
            remaining.iter().map(|s| s.to_string()).collect(),
        ))
    }

    #[test]
    fn scanning_stops_at_the_first_operand() {
        assert_eq!(scan(&[]), ok(&[], &[]));
        assert_eq!(scan(&["-v", "new", "-d"]), ok(&["v"], &["new", "-d"]));
        assert_eq!(scan(&["-v", "-", "-d"]), ok(&["v"], &["-", "-d"]));
        assert_eq!(scan(&["-v", "--", "-d"]), ok(&["v"], &["-d"]));
        assert_eq!(scan(&["--"]), ok(&[], &[]));
    }

    #[test]
    fn grouped_flags_are_read_in_order() {
        assert_eq!(scan(&["-2v", "-Vu"]), ok(&["2", "v", "V", "u"], &[]));
    }

    #[test]
    fn arguments_are_attached_or_the_next_argument_verbatim() {
        assert_eq!(scan(&["-cfoo", "x"]), ok(&["c=foo"], &["x"]));
        assert_eq!(scan(&["-vcfoo"]), ok(&["v", "c=foo"], &[]));
        assert_eq!(scan(&["-c", "foo", "x"]), ok(&["c=foo"], &["x"]));
        assert_eq!(scan(&["-c", "-v"]), ok(&["c=-v"], &[]));
        assert_eq!(scan(&["-c", ""]), ok(&["c="], &[]));
        assert_eq!(scan(&["-c", "--", "x"]), ok(&["c=--"], &["x"]));
        assert_eq!(
            scan(&["-vL", "a", "-S", "b"]),
            ok(&["v", "L=a", "S=b"], &[])
        );
    }

    #[test]
    fn a_trailing_option_without_its_argument_is_an_error() {
        assert_eq!(scan(&["-c"]), Err(GetoptError::MissingArgument(b'c')));
        assert_eq!(scan(&["-vf"]), Err(GetoptError::MissingArgument(b'f')));
    }

    #[test]
    fn unknown_bytes_and_long_options_are_errors() {
        assert_eq!(scan(&["-x"]), Err(GetoptError::UnknownOption(b'x')));
        assert_eq!(scan(&["-2x"]), Err(GetoptError::UnknownOption(b'x')));
        assert_eq!(scan(&["-:"]), Err(GetoptError::UnknownOption(b':')));
        assert_eq!(scan(&["--foo"]), Err(GetoptError::UnknownOption(b'-')));
        assert_eq!(scan(&["-v-x"]), Err(GetoptError::UnknownOption(b'-')));
    }

    #[test]
    fn a_dash_ending_a_group_stops_with_the_group_as_an_operand() {
        assert_eq!(scan(&["-v-", "x"]), ok(&["v"], &["-v-", "x"]));
    }

    #[test]
    fn errors_spell_the_warnx_messages() {
        assert_eq!(
            GetoptError::UnknownOption(b'x').to_string(),
            "unknown option -- x"
        );
        assert_eq!(
            GetoptError::MissingArgument(b'c').to_string(),
            "option requires an argument -- c"
        );
    }
}
