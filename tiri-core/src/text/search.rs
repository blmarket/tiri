//! Byte-string substring search with the strstr(3) and strcasestr(3) rules.

/// Whether `needle` occurs in `haystack`. An empty needle always matches.
/// `icase` folds ASCII letters only, as strcasestr(3) does in the UTF-8 locale
/// tiri requires.
pub(crate) fn contains(haystack: &[u8], needle: &[u8], icase: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|candidate| {
        if icase {
            candidate.eq_ignore_ascii_case(needle)
        } else {
            candidate == needle
        }
    })
}

#[cfg(test)]
mod tests {
    use super::contains;

    #[test]
    fn matches_like_strstr_and_strcasestr() {
        for (haystack, needle, sensitive, insensitive) in [
            (&b"vim"[..], &b"vi"[..], true, true),
            (&b"nvim"[..], &b"VI"[..], false, true),
            (&b"en_US.utf8"[..], &b"UTF8"[..], false, true),
            (&b"abc"[..], &b""[..], true, true),
            (&b""[..], &b""[..], true, true),
            (&b""[..], &b"a"[..], false, false),
            (&b"ab"[..], &b"abc"[..], false, false),
            ("É".as_bytes(), "é".as_bytes(), false, false),
        ] {
            assert_eq!(contains(haystack, needle, false), sensitive, "{needle:?}");
            assert_eq!(contains(haystack, needle, true), insensitive, "{needle:?}");
        }
    }
}
