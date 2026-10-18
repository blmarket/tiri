use crate::ffi::vis::{is_alpha, vis_into};
use crate::log::{fatalx, log_bytes, log_debug};
use crate::options::OptionsScope;
use crate::shared::abi::ssize_t;
use crate::shared::abi::*;
use crate::shared::grid::*;
use crate::shared::utf8::wchar_t;
use crate::shared::utf8::*;
use crate::shared::vis::VIS_DQ;
use crate::text::utf8_cache::{UTF8_ITEMS, UTF8_WIDTHS};
use crate::text::utf8_decode::{decode_utf8, DecodeResult};
use crate::text::utf8_width::parse_width_override;
use std::ffi::{CStr, CString};

fn utf8_find_in_width_cache(wc: wchar_t) -> Option<u_int> {
    UTF8_WIDTHS
        .lock()
        .expect("UTF-8 width cache poisoned")
        .find(wc)
}

fn utf8_insert_width_cache(wc: wchar_t, width: u_int) {
    log_debug(format_args!(
        "Unicode width cache: {:08X}={}",
        wc as u_int, width
    ));
    UTF8_WIDTHS
        .lock()
        .expect("UTF-8 width cache poisoned")
        .insert(wc, width);
}
fn utf8_add_to_width_cache(input: &CStr) {
    if let Some(parsed) = parse_width_override(input) {
        for codepoint in parsed.codepoints {
            utf8_insert_width_cache(codepoint, parsed.width);
        }
    }
}
pub fn utf8_update_width_cache() {
    UTF8_WIDTHS
        .lock()
        .expect("UTF-8 width cache poisoned")
        .reset_defaults();
    // SAFETY: the visitor only copies strings out and does not reenter options.
    let values = unsafe {
        OptionsScope::GlobalServer.with_entry(c"codepoint-widths", |entry| {
            entry
                .value
                .array()
                .into_iter()
                .flat_map(|array| array.iter())
                .map(|item| item.value.string_ptr().expect("width override").to_owned())
                .collect::<Vec<_>>()
        })
    }
    .expect("codepoint-widths option");
    for value in values {
        utf8_add_to_width_cache(&value);
    }
}
fn utf8_put_item(data: &[u8]) -> Option<u_int> {
    let (index, inserted) = UTF8_ITEMS
        .lock()
        .expect("UTF-8 item cache poisoned")
        .intern(data)?;
    log_debug(format_args!(
        "utf8_put_item: {} {} = {}",
        if inserted { "added" } else { "found" },
        log_bytes(data),
        index,
    ));
    Some(index)
}

pub fn utf8_from_data(ud: &utf8_data, uc: &mut utf8_char) -> utf8_state {
    if ud.width > 2 {
        fatalx(|out| write!(out, "invalid UTF-8 width: {}", ud.width));
    }
    let index = if ud.size as usize > ud.data.len() {
        None
    } else if ud.size <= 3 {
        Some((u32::from(ud.data[2]) << 16) | (u32::from(ud.data[1]) << 8) | u32::from(ud.data[0]))
    } else {
        utf8_put_item(&ud.data[..ud.size as usize])
    };
    if let Some(index) = index {
        *uc = (u32::from(ud.size) << 24) | ((u32::from(ud.width) + 1) << 29) | index;
        log_debug(format_args!(
            "utf8_from_data: ({} {} {}) -> {:08x}",
            ud.width,
            ud.size,
            log_bytes(&ud.data[..ud.size as usize]),
            *uc,
        ));
        return UTF8_DONE;
    }
    *uc = match ud.width {
        0 => 1 << 29,
        1 => (1 << 24) | (2 << 29) | 0x20,
        _ => (1 << 24) | (2 << 29) | 0x2020,
    };
    UTF8_ERROR
}

pub fn utf8_to_data(uc: utf8_char, ud: &mut utf8_data) {
    *ud = utf8_data::default();
    ud.have = ((uc >> 24) & 0x1f) as u_char;
    ud.size = ud.have;
    ud.width = (uc >> 29).wrapping_sub(1) as u_char;
    if ud.size <= 3 {
        ud.data[2] = (uc >> 16) as u_char;
        ud.data[1] = (uc >> 8) as u_char;
        ud.data[0] = uc as u_char;
    } else {
        let cache = UTF8_ITEMS.lock().expect("UTF-8 item cache poisoned");
        let data = &mut ud.data[..ud.size as usize];
        if let Some(item) = cache.get(uc & 0xffffff) {
            data.copy_from_slice(&item[..data.len()]);
        } else {
            data.fill(b' ');
        }
    }
    log_debug(format_args!(
        "utf8_to_data: {:08x} -> ({} {} {})",
        uc,
        ud.width,
        ud.size,
        log_bytes(&ud.data[..ud.size as usize]),
    ));
}
pub fn utf8_build_one(mut ch: u_char) -> utf8_char {
    (1 as ::core::ffi::c_int as utf8_char) << 24 as ::core::ffi::c_int
        | (1 as ::core::ffi::c_int as utf8_char).wrapping_add(1 as utf8_char)
            << 29 as ::core::ffi::c_int
        | ch as utf8_char
}
pub fn utf8_set(ud: &mut utf8_data, ch: u_char) {
    *ud = utf8_data {
        data: [0; 32],
        have: 1,
        size: 1,
        width: 1,
    };
    ud.data[0] = ch;
}

/// Return a stack value so a caller may safely copy onto its own source.
pub fn utf8_copy(from: &utf8_data) -> utf8_data {
    let mut copied = *from;
    let size = (copied.size as usize).min(copied.data.len());
    copied.data[size..].fill(0);
    copied
}

fn utf8_width(ud: &utf8_data) -> Option<i32> {
    let mut wc = 0;
    if utf8_towc(ud, &mut wc) != UTF8_DONE {
        return None;
    }
    if let Some(cached) = utf8_find_in_width_cache(wc) {
        log_debug(format_args!(
            "cached width for {:08X} is {}",
            wc as u_int, cached
        ));
        return Some(cached as i32);
    }
    let width = crate::ffi::utf8proc::codepoint_width(wc);
    log_debug(format_args!(
        "utf8proc_wcwidth({:05X}) returned {}",
        wc as u_int, width
    ));
    (0..=0xff).contains(&width).then_some(width)
}

pub fn utf8_towc(ud: &utf8_data, wc: &mut wchar_t) -> utf8_state {
    let Some(bytes) = ud.data.get(..ud.size as usize) else {
        return UTF8_ERROR;
    };
    match decode_utf8(bytes) {
        DecodeResult::Complete { codepoint, .. } => {
            *wc = codepoint as wchar_t;
            log_debug(format_args!(
                "UTF-8 {} is U+{:06X}",
                log_bytes(bytes),
                *wc as u_int
            ));
            UTF8_DONE
        }
        _ => {
            log_debug(format_args!("UTF-8 {} is invalid", log_bytes(bytes)));
            UTF8_ERROR
        }
    }
}

pub fn utf8_has_whitespace(ud: &utf8_data) -> ::core::ffi::c_int {
    let Some(mut remaining) = ud.data.get(..ud.size as usize) else {
        return 0;
    };
    while !remaining.is_empty() {
        let DecodeResult::Complete { codepoint, len } = decode_utf8(remaining) else {
            return 0;
        };
        remaining = &remaining[len..];
        if matches!(
            codepoint,
            9 | 10
                | 11
                | 12
                | 13
                | 32
                | 133
                | 160
                | 5760
                | 8192
                | 8193
                | 8194
                | 8195
                | 8196
                | 8197
                | 8198
                | 8199
                | 8200
                | 8201
                | 8202
                | 8232
                | 8233
                | 8239
                | 8287
                | 12288
        ) {
            return 1;
        }
    }
    0
}

pub fn utf8_fromwc(wc: wchar_t, ud: &mut utf8_data) -> utf8_state {
    let size = match crate::ffi::utf8proc::encode_cell(wc, &mut ud.data) {
        Ok(size) => size,
        Err(error) => {
            log_debug(format_args!(
                "UTF-8 {}, wctomb() {}",
                wc,
                error.raw_os_error().unwrap_or(0)
            ));
            crate::ffi::utf8proc::reset_wctomb();
            return UTF8_ERROR;
        }
    };
    if size == 0 {
        return UTF8_ERROR;
    }
    ud.have = size as u_char;
    ud.size = ud.have;
    if let Some(width) = utf8_width(ud) {
        ud.width = width as u_char;
        UTF8_DONE
    } else {
        UTF8_ERROR
    }
}

pub fn utf8_open(ud: &mut utf8_data, ch: u_char) -> utf8_state {
    *ud = utf8_data::default();
    let DecodeResult::Incomplete { expected } = decode_utf8(&[ch]) else {
        return UTF8_ERROR;
    };
    if !(2..=4).contains(&expected) {
        return UTF8_ERROR;
    }
    ud.size = expected as u_char;
    utf8_append(ud, ch);
    UTF8_MORE
}

pub fn utf8_append(ud: &mut utf8_data, ch: u_char) -> utf8_state {
    if ud.have >= ud.size {
        fatalx(|out| out.write_all(b"UTF-8 character overflow"));
    }
    if ud.size as usize > ud.data.len() {
        fatalx(|out| out.write_all(b"UTF-8 character size too large"));
    }
    if ud.have != 0 && ch & 0xc0 != 0x80 {
        ud.width = 0xff;
    }
    ud.data[ud.have as usize] = ch;
    ud.have += 1;
    match decode_utf8(&ud.data[..ud.have as usize]) {
        DecodeResult::Complete { len, .. } if ud.have == ud.size && len == ud.size as usize => {
            if ud.width == 0xff {
                return UTF8_ERROR;
            }
            let Some(width) = utf8_width(ud) else {
                return UTF8_ERROR;
            };
            ud.width = width as u_char;
            UTF8_DONE
        }
        DecodeResult::Invalid { .. } if ud.have == ud.size => UTF8_ERROR,
        _ => UTF8_MORE,
    }
}
pub fn utf8_strvis(dst: &mut [u8], src: &[u8], flag: i32) -> usize {
    let capacity = src
        .len()
        .checked_mul(4)
        .and_then(|size| size.checked_add(1))
        .expect("escaped UTF-8 string is too large");
    assert!(dst.len() >= capacity, "escape destination is too small");
    let mut cell = utf8_data::default();
    let mut offset = 0;
    let mut written = 0;
    while offset < src.len() {
        let start = offset;
        let mut more = utf8_open(&mut cell, src[offset]);
        if more == UTF8_MORE {
            offset += 1;
            while offset < src.len() && more == UTF8_MORE {
                more = utf8_append(&mut cell, src[offset]);
                offset += 1;
            }
            if more == UTF8_DONE {
                let size = cell.size as usize;
                dst[written..written + size].copy_from_slice(&cell.data[..size]);
                written += size;
                continue;
            }
            offset = start;
        }
        let next = src.get(offset + 1).copied();
        if let Some(next) = next.filter(|_| flag & VIS_DQ != 0 && src[offset] == b'$') {
            if is_alpha(next) || matches!(next, b'_' | b'{') {
                dst[written] = b'\\';
                written += 1;
            }
            dst[written] = b'$';
            written += 1;
        } else {
            written += vis_into(&mut dst[written..], src[offset], flag, next.unwrap_or(0));
        }
        offset += 1;
    }
    dst[written] = 0;
    written
}
/// Escape a C string using the same byte conversion as `utf8_strvis`, with
/// the result owned by Rust.
pub(crate) fn utf8_stravis_cstring(src: &CStr, flag: i32) -> CString {
    let source_len = src.to_bytes().len();
    let capacity = source_len
        .checked_mul(4)
        .and_then(|size| size.checked_add(1))
        .expect("escaped UTF-8 string is too large");
    let mut buffer = vec![0u8; capacity];
    let escaped_len = utf8_strvis(&mut buffer, src.to_bytes(), flag);
    buffer.truncate(escaped_len + 1);
    CString::from_vec_with_nul(buffer).expect("utf8_strvis output has no interior NUL")
}

/// Escape an explicit byte length without losing bytes after an input NUL.
pub(crate) fn utf8_stravisx_bytes(src: &[u8], flag: ::core::ffi::c_int) -> Vec<u8> {
    if src.is_empty() {
        return Vec::new();
    }
    let capacity = src
        .len()
        .checked_mul(4)
        .and_then(|size| size.checked_add(1))
        .expect("escaped UTF-8 bytes are too large");
    let mut buffer = vec![0u8; capacity];
    let escaped_len = utf8_strvis(&mut buffer, src, flag);
    buffer.truncate(escaped_len);
    buffer
}
pub fn utf8_isvalid(s: &CStr) -> bool {
    let bytes = s.to_bytes();
    let mut cell = utf8_data::default();
    let mut offset = 0;
    while offset < bytes.len() {
        let mut more = utf8_open(&mut cell, bytes[offset]);
        if more == UTF8_MORE {
            offset += 1;
            while offset < bytes.len() && more == UTF8_MORE {
                more = utf8_append(&mut cell, bytes[offset]);
                offset += 1;
            }
            if more != UTF8_DONE {
                return false;
            }
        } else {
            if !(0x20..=0x7e).contains(&bytes[offset]) {
                return false;
            }
            offset += 1;
        }
    }
    true
}
/// The input is a NUL-terminated C string. The result is ASCII and retains
/// the old sanitizer's first-NUL view and underscore width for UTF-8 cells.
pub(crate) fn utf8_sanitize_cstring(src: &CStr) -> CString {
    let mut dst = Vec::new();
    let mut more: utf8_state = UTF8_MORE;
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let source = src.to_bytes();
    let mut offset = 0;
    while offset < source.len() {
        let candidate_start = offset;
        more = utf8_open(&mut ud, source[offset]);
        if more as ::core::ffi::c_uint == UTF8_MORE as ::core::ffi::c_int as ::core::ffi::c_uint {
            loop {
                offset += 1;
                if offset >= source.len() || more != UTF8_MORE {
                    break;
                }
                more = utf8_append(&mut ud, source[offset]);
            }
            if more as ::core::ffi::c_uint == UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                if dst.is_empty() && ud.width == 0 {
                    fatalx(|out| out.write_all(b"xreallocarray: zero size"));
                }
                dst.resize(dst.len() + ud.width as usize, b'_');
                continue;
            } else {
                offset = candidate_start;
            }
        }
        if source[offset] > 0x1f && source[offset] < 0x7f {
            dst.push(source[offset]);
        } else {
            dst.push(b'_');
        }
        offset += 1;
    }
    CString::new(dst).expect("sanitized bytes contain no interior NUL")
}
pub fn utf8_strlen(cells: &[utf8_data]) -> size_t {
    cells
        .iter()
        .position(|cell| cell.size == 0)
        .unwrap_or(cells.len())
}

pub fn utf8_strwidth(cells: &[utf8_data], n: ssize_t) -> u_int {
    let limit = usize::try_from(n).unwrap_or(usize::MAX);
    cells
        .iter()
        .take_while(|cell| cell.size != 0)
        .take(limit)
        .fold(0, |width, cell| width.wrapping_add(cell.width as u_int))
}
pub(crate) fn utf8_fromcstr_vec(src: &CStr) -> Vec<utf8_data> {
    let bytes = src.to_bytes();
    let mut cells = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let mut cell = utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        };
        let mut more = utf8_open(&mut cell, bytes[index] as u_char);
        if more == UTF8_MORE {
            loop {
                index += 1;
                if index == bytes.len() || more != UTF8_MORE {
                    break;
                }
                more = utf8_append(&mut cell, bytes[index] as u_char);
            }
            if more == UTF8_DONE {
                cells.push(cell);
                continue;
            }
            index -= cell.have as usize;
        }
        utf8_set(&mut cell, bytes[index] as u_char);
        cells.push(cell);
        index += 1;
    }
    cells.push(utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    });
    cells
}

/// Copy the C-visible part of a sentinel-terminated cell stream into Rust-owned storage.
/// C consumers stop at the first NUL, even when it occurs inside a cell.
pub(crate) fn utf8_tocstr_cstring(cells: &[utf8_data]) -> CString {
    let mut bytes = Vec::new();
    for cell in cells.iter().take_while(|cell| cell.size != 0) {
        let data = &cell.data[..cell.size as usize];
        if let Some(end) = data.iter().position(|&byte| byte == 0) {
            bytes.extend_from_slice(&data[..end]);
            break;
        }
        bytes.extend_from_slice(data);
    }
    CString::new(bytes).expect("the first NUL ends the copied string")
}
pub fn utf8_cstrwidth(s: &CStr) -> u_int {
    let bytes = s.to_bytes();
    let mut cell = utf8_data::default();
    let mut width: u_int = 0;
    let mut offset = 0;
    while offset < bytes.len() {
        let start = offset;
        let mut more = utf8_open(&mut cell, bytes[offset]);
        if more == UTF8_MORE {
            offset += 1;
            while offset < bytes.len() && more == UTF8_MORE {
                more = utf8_append(&mut cell, bytes[offset]);
                offset += 1;
            }
            if more == UTF8_DONE {
                width = width.wrapping_add(cell.width as u_int);
                continue;
            }
            offset = start;
        }
        let byte = bytes[offset] as std::ffi::c_char;
        if byte > 0x1f && byte != 0x7f {
            width = width.wrapping_add(1);
        }
        offset += 1;
    }
    width
}
pub(crate) fn utf8_pad_cstring(s: &CStr, width: u_int, left: bool) -> CString {
    let bytes = s.to_bytes();
    let padding = width.saturating_sub(utf8_cstrwidth(s)) as usize;
    let mut output = Vec::with_capacity(bytes.len() + padding);
    if left {
        output.resize(padding, b' ');
        output.extend_from_slice(bytes);
    } else {
        output.extend_from_slice(bytes);
        output.resize(bytes.len() + padding, b' ');
    }
    CString::new(output).expect("padded C string contains no NUL")
}
pub fn utf8_cstrhas(s: &CStr, ud: &utf8_data) -> bool {
    let bytes = s.to_bytes();
    let mut offset = 0;
    let mut found = 0;
    while offset < bytes.len() {
        let mut cell = utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        };
        let mut more = utf8_open(&mut cell, bytes[offset]);
        if more == UTF8_MORE {
            let start = offset;
            offset += 1;
            while offset < bytes.len() && more == UTF8_MORE {
                more = utf8_append(&mut cell, bytes[offset]);
                offset += 1;
            }
            if more != UTF8_DONE {
                offset = start;
            }
        }
        if more != UTF8_DONE {
            utf8_set(&mut cell, bytes[offset]);
            offset += 1;
        }
        let matches = {
            cell.size == ud.size && cell.data[..cell.size as usize] == ud.data[..ud.size as usize]
        };
        if matches {
            found = 1;
        }
    }
    found != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_validation_and_width_keep_first_nul_and_invalid_byte_rules() {
        for (source, valid, width) in [
            (&b"\0"[..], true, 0),
            (&b"a\0\xff\0"[..], true, 1),
            (&b"\xc3\xa9\xe7\x95\x8c\0"[..], true, 3),
            (&b"\xcc\x81\0"[..], true, 0),
            (&b"\tA\x7fB\0"[..], false, 2),
            (&b"\xff\0"[..], false, 0),
            (&b"\xe2(\xa1\0"[..], false, 1),
            (&b"\xf0\x9f\x92\0"[..], false, 0),
            (&b"\xe0\x80\x80\0"[..], false, 0),
            (&b"\xed\xa0\x80\0"[..], false, 0),
        ] {
            let input = CStr::from_bytes_until_nul(source).unwrap();
            assert_eq!(utf8_isvalid(input), valid, "{source:?}");
            assert_eq!(utf8_cstrwidth(input), width, "{source:?}");
        }
    }

    #[test]
    fn cell_slices_stop_at_the_sentinel_or_the_slice_boundary() {
        let mut cells = utf8_fromcstr_vec(c"é漢");
        cells.push(utf8_fromcstr_vec(c"Z")[0]);
        assert_eq!(utf8_strlen(&cells), 2);
        assert_eq!(utf8_strwidth(&cells, -1), 3);
        assert_eq!(utf8_strwidth(&cells, -2), 3);
        assert_eq!(utf8_strwidth(&cells, 0), 0);
        assert_eq!(utf8_strwidth(&cells, 1), 1);
        assert_eq!(utf8_strwidth(&cells, 9), 3);
        assert_eq!(utf8_tocstr_cstring(&cells).as_c_str(), c"é漢");
        assert_eq!(utf8_strlen(&cells[..2]), 2);
        assert_eq!(utf8_strwidth(&cells[..2], -1), 3);
        assert_eq!(utf8_tocstr_cstring(&cells[..2]).as_c_str(), c"é漢");
        assert_eq!(utf8_strlen(&[]), 0);
        assert_eq!(utf8_strwidth(&[], -1), 0);
        assert_eq!(utf8_tocstr_cstring(&[]).as_c_str(), c"");
    }

    #[test]
    fn sanitize_owns_printable_ascii_and_preserves_legacy_widths() {
        {
            for (input, expected) in [
                (&b"\0"[..], &b""[..]),
                (&b"A\x01 \x7fB\0"[..], &b"A_ _B"[..]),
                (&b"caf\xc3\xa9\0"[..], &b"caf_"[..]),
                (&b"\xe4\xb8\xad\0"[..], &b"__"[..]),
                (&b"\xff\0"[..], &b"_"[..]),
                (&b"\xe2\x82\0"[..], &b"__"[..]),
                (&b"\xe2(\xa1\0"[..], &b"_(_"[..]),
                (&b"A\0B\0"[..], &b"A"[..]),
            ] {
                let nul = input.iter().position(|&byte| byte == 0).unwrap();
                let input = CStr::from_bytes_with_nul(&input[..=nul]).unwrap();
                assert_eq!(utf8_sanitize_cstring(input).as_bytes(), expected);
            }
        }
    }

    #[test]
    fn owned_cell_decoder_retries_each_byte_after_a_bad_sequence() {
        let cells = utf8_fromcstr_vec(c"\xe2(\xa1");
        assert_eq!(cells.len(), 4);
        for (cell, byte) in cells[..3].iter().zip([0xe2, b'(', 0xa1]) {
            assert_eq!(cell.size, 1);
            assert_eq!(cell.data[0], byte);
        }
        assert_eq!(cells[3].size, 0);
    }

    #[test]
    fn utf8_tocstr_cstring_stops_at_the_first_nul_in_a_cell_or_between_cells() {
        {
            for parts in [
                vec![],
                vec![vec![b'a'], vec![0xe2, 0x82, 0xac], vec![0xff]],
                vec![vec![b'a', 0, b'b'], vec![b'c']],
                vec![vec![b'a'], vec![0], vec![b'b']],
            ] {
                let mut cells = Vec::new();
                for part in parts {
                    let mut cell = utf8_data::default();
                    cell.data[..part.len()].copy_from_slice(&part);
                    cell.size = part.len() as u_char;
                    cells.push(cell);
                }
                cells.push(utf8_data::default());

                let mut expected = Vec::new();
                for cell in &cells {
                    if cell.size == 0 {
                        break;
                    }
                    let bytes = &cell.data[..cell.size as usize];
                    if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
                        expected.extend_from_slice(&bytes[..end]);
                        break;
                    }
                    expected.extend_from_slice(bytes);
                }
                assert_eq!(utf8_tocstr_cstring(&cells).as_bytes(), expected);
            }
        }
    }

    #[test]
    fn owned_vis_helpers_preserve_multibyte_and_explicit_length_bytes() {
        {
            let input = CString::new(&b"a\xc3\xa9\xff"[..]).unwrap();
            let escaped = utf8_stravis_cstring(input.as_c_str(), crate::shared::vis::VIS_OCTAL);
            assert_eq!(escaped.as_bytes(), b"a\xc3\xa9\\377");

            let bytes = utf8_stravisx_bytes(b"a\0b", crate::shared::vis::VIS_OCTAL);
            assert_eq!(bytes, b"a\\000b");
        }
    }

    #[test]
    fn escaping_preserves_lookahead_and_retries_invalid_utf8_bytes() {
        use crate::compat::vis::VIS_ALL;
        use crate::shared::vis::{VIS_CSTYLE, VIS_OCTAL};

        for (source, flags, expected) in [
            (
                &b"$a $_ ${ $1 $! $"[..],
                VIS_DQ,
                &b"\\$a \\$_ \\${ $1 $! $"[..],
            ),
            (&b"\x007\x008"[..], VIS_CSTYLE, &b"\\0007\\08"[..]),
            ("é界🦀".as_bytes(), VIS_ALL | VIS_OCTAL, "é界🦀".as_bytes()),
            (&b"\xe2(\xa1"[..], VIS_OCTAL, &b"\\342(\\241"[..]),
            (&b"\xf0\x9f\x92"[..], VIS_OCTAL, &b"\\360\\237\\222"[..]),
        ] {
            assert_eq!(utf8_stravisx_bytes(source, flags), expected);
        }
    }

    #[test]
    fn escaping_terminates_reused_and_exact_capacity_buffers() {
        use crate::shared::vis::VIS_OCTAL;

        let mut output = [0x55; 9];
        assert_eq!(utf8_strvis(&mut output, &[0xff, 0xff], VIS_OCTAL), 8);
        assert_eq!(&output, b"\\377\\377\0");
        assert_eq!(utf8_strvis(&mut output, b"a", VIS_OCTAL), 1);
        assert_eq!(CStr::from_bytes_until_nul(&output).unwrap(), c"a");
        assert_eq!(utf8_strvis(&mut output[..1], &[], VIS_OCTAL), 0);
        assert_eq!(output[0], 0);
    }

    #[test]
    fn packed_cells_preserve_inline_and_cached_bytes() {
        for width in 0..=2 {
            for size in 0..=31 {
                let mut original = utf8_data {
                    have: size,
                    size,
                    width,
                    ..utf8_data::default()
                };
                for (index, byte) in original.data[..size as usize].iter_mut().enumerate() {
                    *byte = (index as u8).wrapping_mul(31).wrapping_add(0x80);
                }
                let mut encoded = 0;
                assert_eq!(utf8_from_data(&original, &mut encoded), UTF8_DONE);
                let mut repeated = 0;
                assert_eq!(utf8_from_data(&original, &mut repeated), UTF8_DONE);
                assert_eq!(encoded, repeated);
                let mut decoded = utf8_data::default();
                utf8_to_data(encoded, &mut decoded);
                assert_eq!(
                    (decoded.have, decoded.size, decoded.width),
                    (size, size, width)
                );
                assert_eq!(decoded.data, original.data);
            }
        }
        let mut cell = utf8_data {
            size: 1,
            width: 1,
            ..utf8_data::default()
        };
        cell.data[..3].copy_from_slice(b"abc");
        let mut encoded = 0;
        assert_eq!(utf8_from_data(&cell, &mut encoded), UTF8_DONE);
        assert_eq!(encoded, (1 << 24) | (2 << 29) | 0x636261);
    }

    #[test]
    fn packed_cells_preserve_fallback_and_size_boundary() {
        for (width, expected) in [(0, 1 << 29), (1, 0x41000020), (2, 0x41002020)] {
            let cell = utf8_data {
                size: 33,
                width,
                ..utf8_data::default()
            };
            let mut encoded = 0;
            assert_eq!(utf8_from_data(&cell, &mut encoded), UTF8_ERROR);
            assert_eq!(encoded, expected);
        }
        let cell = utf8_data {
            data: [0xab; 32],
            size: 32,
            width: 1,
            ..utf8_data::default()
        };
        let mut encoded = 0;
        assert_eq!(utf8_from_data(&cell, &mut encoded), UTF8_DONE);
        assert_eq!(encoded & 0xff000000, 0x60000000);
        let mut decoded = utf8_data::default();
        utf8_to_data((4 << 24) | (3 << 29) | 0xfffffe, &mut decoded);
        assert_eq!((decoded.have, decoded.size, decoded.width), (4, 4, 2));
        assert_eq!(&decoded.data[..4], b"    ");
        assert!(decoded.data[4..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn utf8_width_cache_parses_entries_and_ignores_invalid_ones() {
        for entry in [
            &b"U+E010=2\0"[..],
            &b"U+E011-U+E013=0\0"[..],
            &b"\xee\x80\xa0=1\0"[..],
            &b"U+E040=1\0U+E041=2\0"[..],
            &b"z=2\0"[..],
        ] {
            utf8_add_to_width_cache(CStr::from_bytes_until_nul(entry).unwrap());
        }

        for (codepoint, expected) in [
            (0xE010, 2),
            (0xE011, 0),
            (0xE012, 0),
            (0xE013, 0),
            (0xE020, 1),
            (0xE040, 1),
            ('z' as wchar_t, 2),
        ] {
            assert_eq!(
                utf8_find_in_width_cache(codepoint),
                Some(expected),
                "wrong width for U+{codepoint:04X}"
            );
        }

        for entry in [
            &b"U+E030\0"[..],
            &b"U+E030=3\0"[..],
            &b"U+E030-U+E02F=1\0"[..],
            &b"ab=1\0"[..],
            &b"=1\0"[..],
            &b"\xee\x80\xa2z=1\0"[..],
            &b"\xc3(=1\0"[..],
            &b"\xe2\x82=1\0"[..],
            &b"\xff=1\0"[..],
            &b"\xc0\xaf=1\0"[..],
        ] {
            utf8_add_to_width_cache(CStr::from_bytes_until_nul(entry).unwrap());
        }
        for codepoint in [
            0xE030,
            0xE041,
            0xE022,
            'a' as wchar_t,
            'b' as wchar_t,
            '(' as wchar_t,
        ] {
            assert!(
                utf8_find_in_width_cache(codepoint).is_none(),
                "unexpected U+{codepoint:04X}"
            );
        }

        for codepoint in [
            0xE010,
            0xE011,
            0xE012,
            0xE013,
            0xE020,
            0xE040,
            'z' as wchar_t,
        ] {
            UTF8_WIDTHS.lock().unwrap().remove(codepoint);
        }
    }
}
