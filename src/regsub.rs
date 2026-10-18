#![forbid(unsafe_code)]

use crate::ffi::regex::{RegexMatch, RegexStorage};
use std::ffi::{CStr, CString};

fn regsub_copy(buf: &mut Vec<u8>, text: &[u8], start: usize, end: usize) {
    buf.extend_from_slice(&text[start..end]);
}
fn regsub_expand(buf: &mut Vec<u8>, with: &[u8], text: &[u8], matches: &[RegexMatch]) {
    let mut index = 0;
    while index < with.len() {
        if with[index] == b'\\' && index + 1 < with.len() {
            let escaped = with[index + 1];
            if escaped.is_ascii_digit() {
                let group = (escaped - b'0') as usize;
                if let Some(matched) = matches.get(group) {
                    if matched.rm_so >= 0 && matched.rm_eo >= 0 && matched.rm_so != matched.rm_eo {
                        let start = usize::try_from(matched.rm_so)
                            .expect("regex match start is nonnegative");
                        let end =
                            usize::try_from(matched.rm_eo).expect("regex match end is nonnegative");
                        regsub_copy(buf, text, start, end);
                        index += 2;
                        continue;
                    }
                }
            }
            buf.push(escaped);
            index += 2;
        } else {
            buf.push(with[index]);
            index += 1;
        }
    }
}
pub fn regsub_cstring(
    pattern: &CStr,
    with: &CStr,
    text: &CStr,
    flags: ::core::ffi::c_int,
) -> Option<CString> {
    let mut m: [RegexMatch; 10] = [RegexMatch { rm_so: 0, rm_eo: 0 }; 10];
    let text_bytes = text.to_bytes();
    let pattern_bytes = pattern.to_bytes();
    let replacement_bytes = with.to_bytes();
    let mut start = 0;
    let mut last = 0;
    let mut empty = false;
    let mut buf = Vec::<u8>::new();
    if text_bytes.is_empty() {
        return Some(CString::default());
    }
    if pattern_bytes.is_empty() {
        return Some(text.to_owned());
    }
    let mut storage = RegexStorage::default();
    let regex = storage.compile(pattern, flags).ok()?;
    let end = text_bytes.len();
    while start <= end {
        if !regex.execute_at(text, start, &mut m, 0) {
            regsub_copy(&mut buf, text_bytes, start, end);
            break;
        } else {
            let match_start = usize::try_from(m[0].rm_so)
                .expect("successful regex match has a nonnegative start");
            let match_end =
                usize::try_from(m[0].rm_eo).expect("successful regex match has a nonnegative end");
            let absolute_match_start = start + match_start;
            regsub_copy(&mut buf, text_bytes, last, absolute_match_start);
            if pattern_bytes[0] == b'^' {
                regsub_expand(&mut buf, replacement_bytes, &text_bytes[start..], &m);
                last = start + match_end;
                regsub_copy(&mut buf, text_bytes, last, end);
                break;
            } else if empty || absolute_match_start != last || m[0].rm_so != m[0].rm_eo {
                regsub_expand(&mut buf, replacement_bytes, &text_bytes[start..], &m);
                last = start + match_end;
                start += match_end;
                empty = false;
            } else {
                last = start + match_end;
                start += match_end + 1;
                empty = true;
            }
        }
    }
    Some(CString::new(buf).expect("regex substitution contains no NUL"))
}
