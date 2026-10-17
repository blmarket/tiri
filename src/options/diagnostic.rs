//! Option diagnostics.
use std::ffi::{CStr, CString};

/// The option diagnostics use only literal text and `%s` substitutions.
pub(super) fn options_string_cause(fmt: &CStr, args: &[&CStr]) -> CString {
    let fmt = fmt.to_bytes();
    let mut message = Vec::with_capacity(fmt.len());
    let mut at = 0;
    let mut arg = 0;
    while at < fmt.len() {
        if fmt[at..].starts_with(b"%s") {
            message.extend_from_slice(args[arg].to_bytes());
            at += 2;
            arg += 1;
        } else {
            message.push(fmt[at]);
            at += 1;
        }
    }
    debug_assert_eq!(arg, args.len());
    CString::new(message).expect("option diagnostic contains no NUL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatted_diagnostic_preserves_non_utf8_arguments() {
        let message = options_string_cause(c"value is %s: %s", &[c"invalid", c"\xff"]);
        assert_eq!(message.to_bytes(), b"value is invalid: \xff");
    }
}
