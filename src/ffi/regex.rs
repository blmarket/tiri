//! Owned POSIX regex state and bounded calls into libc.

pub use libc::regmatch_t as RegexMatch;
use libc::{regcomp, regex_t, regexec, regfree};
use std::ffi::{c_int, CStr};
use std::mem::MaybeUninit;

/// Reusable compilation storage. Successful compilation transfers ownership
/// to `CompiledRegex`; failed compilation never constructs an owner or calls `regfree`.
pub struct RegexStorage(MaybeUninit<regex_t>);

impl Default for RegexStorage {
    fn default() -> Self {
        Self(MaybeUninit::zeroed())
    }
}

impl RegexStorage {
    pub fn compile(&mut self, pattern: &CStr, flags: c_int) -> Result<CompiledRegex, c_int> {
        self.0 = MaybeUninit::zeroed();
        let status = unsafe { regcomp(self.0.as_mut_ptr(), pattern.as_ptr(), flags) };
        if status != 0 {
            return Err(status);
        }
        Ok(CompiledRegex(unsafe { self.0.assume_init() }))
    }
}

pub struct CompiledRegex(regex_t);

impl CompiledRegex {
    pub fn is_match(&self, text: &CStr) -> bool {
        self.execute_at(text, 0, &mut [], 0)
    }

    /// Match the suffix starting at a byte offset. Capture offsets are relative
    /// to that suffix, as in regexec. This API does not accept REG_STARTEND:
    /// callers cannot pass unvalidated capture bounds to libc as input ranges.
    pub fn execute_at(
        &self,
        text: &CStr,
        start: usize,
        matches: &mut [RegexMatch],
        flags: c_int,
    ) -> bool {
        assert_eq!(flags & !(libc::REG_NOTBOL | libc::REG_NOTEOL), 0);
        if start > text.to_bytes().len() {
            return false;
        }
        let output = if matches.is_empty() {
            std::ptr::null_mut()
        } else {
            matches.as_mut_ptr()
        };
        unsafe {
            regexec(
                &raw const self.0,
                text.as_ptr().add(start),
                matches.len(),
                output,
                flags,
            ) == 0
        }
    }
}

impl Drop for CompiledRegex {
    fn drop(&mut self) {
        unsafe { regfree(&mut self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_owners_survive_storage_reuse_drop_and_moves() {
        let (mut first, mut second) = {
            let mut storage = RegexStorage::default();
            let first = storage.compile(c"^(a)(b)?$", libc::REG_EXTENDED).unwrap();
            let second = storage.compile(c"^z+$", libc::REG_EXTENDED).unwrap();
            assert!(storage.compile(c"[", libc::REG_EXTENDED).is_err());
            (first, second)
        };

        std::mem::swap(&mut first, &mut second);
        assert!(first.is_match(c"zzz"));
        assert!(!first.is_match(c"a"));
        drop(first);

        let mut matches = [RegexMatch { rm_so: 0, rm_eo: 0 }; 3];
        assert!(second.execute_at(c"xa", 1, &mut matches, 0));
        assert_eq!((matches[0].rm_so, matches[0].rm_eo), (0, 1));
        assert_eq!((matches[1].rm_so, matches[1].rm_eo), (0, 1));
        assert_eq!((matches[2].rm_so, matches[2].rm_eo), (-1, -1));
    }

    #[test]
    fn storage_can_be_recompiled_after_success_and_failure() {
        let mut storage = RegexStorage::default();
        for _ in 0..32 {
            assert!(storage.compile(c"[", libc::REG_EXTENDED).is_err());
            {
                let regex = storage.compile(c"^(a)(b)?$", libc::REG_EXTENDED).unwrap();
                let mut matches = [RegexMatch { rm_so: 0, rm_eo: 0 }; 3];
                assert!(regex.execute_at(c"xa", 1, &mut matches, 0));
                assert_eq!((matches[0].rm_so, matches[0].rm_eo), (0, 1));
                assert_eq!((matches[1].rm_so, matches[1].rm_eo), (0, 1));
                assert_eq!((matches[2].rm_so, matches[2].rm_eo), (-1, -1));
                assert!(!regex.execute_at(c"xa", 1, &mut matches, libc::REG_NOTBOL));
                assert!(!regex.execute_at(c"xa", 3, &mut matches, 0));
            }
            let regex = storage
                .compile(
                    c"b$",
                    libc::REG_EXTENDED | libc::REG_ICASE | libc::REG_NOSUB,
                )
                .unwrap();
            assert!(regex.is_match(c"aB"));
            assert!(!regex.is_match(c"aBc"));
        }
    }

    #[test]
    fn end_of_string_and_empty_matches_remain_valid() {
        let mut storage = RegexStorage::default();
        let regex = storage.compile(c"$", libc::REG_EXTENDED).unwrap();
        let mut matches = [RegexMatch {
            rm_so: -1,
            rm_eo: -1,
        }];
        assert!(regex.execute_at(c"ab", 2, &mut matches, 0));
        assert_eq!((matches[0].rm_so, matches[0].rm_eo), (0, 0));
        assert!(!regex.execute_at(c"ab", 2, &mut matches, libc::REG_NOTEOL));
    }
}
