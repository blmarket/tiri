use crate::format::format_skip;
use crate::grid::grid_default_cell;
use crate::shared::abi::*;
use crate::shared::grid::*;
use crate::shared::style::*;
use crate::shared::utf8::*;
use crate::style::{style_parse, style_set};
use crate::text::utf8::{utf8_append, utf8_open, utf8_set};
use std::ffi::{CStr, CString};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct fuzzy_char {
    pub align: style_align,
    pub ud: utf8_data,
    pub width: u_int,
    pub offset: u_int,
}
#[derive(Copy, Clone)]
pub struct fuzzy_term<'a> {
    pub inverse: ::core::ffi::c_int,
    pub exact: ::core::ffi::c_int,
    pub prefix: ::core::ffi::c_int,
    pub suffix: ::core::ffi::c_int,
    pub text: &'a [u8],
}
pub const FUZZY_BONUS_EXACT: ::core::ffi::c_int = 1000 as ::core::ffi::c_int;
pub const FUZZY_BONUS_PREFIX: ::core::ffi::c_int = 200 as ::core::ffi::c_int;
pub const FUZZY_BONUS_SUFFIX: ::core::ffi::c_int = 100 as ::core::ffi::c_int;
pub const FUZZY_BONUS_START: ::core::ffi::c_int = 12 as ::core::ffi::c_int;
pub const FUZZY_BONUS_BOUNDARY: ::core::ffi::c_int = 8 as ::core::ffi::c_int;
pub const FUZZY_BONUS_CONSECUTIVE: ::core::ffi::c_int = 6 as ::core::ffi::c_int;
pub const FUZZY_PENALTY_LEADING: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
pub const FUZZY_PENALTY_LEADING_MAX: ::core::ffi::c_int = 10 as ::core::ffi::c_int;
pub const FUZZY_PENALTY_GAP: ::core::ffi::c_int = 1 as ::core::ffi::c_int;
fn fuzzy_is_boundary(ud: &utf8_data) -> bool {
    ud.size == 1 && b" -_/.:\0".contains(&ud.data[0])
}

fn fuzzy_char_equal(a: &utf8_data, b: &utf8_data, fold: bool) -> bool {
    if fold && a.size == 1 && b.size == 1 && a.data[0] < 0x80 && b.data[0] < 0x80 {
        return a.data[0].eq_ignore_ascii_case(&b.data[0]);
    }
    a.size == b.size && a.data[..a.size as usize] == b.data[..b.size as usize]
}

fn fuzzy_align(mut align: style_align) -> style_align {
    if align as ::core::ffi::c_uint
        == STYLE_ALIGN_DEFAULT as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return STYLE_ALIGN_LEFT;
    }
    align
}
fn fuzzy_add(cs: &mut Vec<fuzzy_char>, a: style_align, ud: &utf8_data, widths: &mut [u_int; 5]) {
    cs.push(fuzzy_char {
        align: a,
        ud: *ud,
        width: ud.width as u_int,
        offset: widths[a as usize],
    });
    widths[a as usize] = widths[a as usize].wrapping_add(ud.width as u_int);
}
fn fuzzy_decode_one<'a>(input: &'a [u8], ud: &mut utf8_data) -> &'a [u8] {
    let mut more = utf8_open(ud, input[0]);
    if more == UTF8_MORE {
        let mut consumed = 1;
        while consumed < input.len() && more == UTF8_MORE {
            more = utf8_append(ud, input[consumed]);
            consumed += 1;
        }
        if more == UTF8_DONE {
            return &input[consumed..];
        }
    }
    utf8_set(ud, input[0]);
    &input[1..]
}
unsafe fn fuzzy_scan(text: &CStr, widths: &mut [u_int; 5]) -> Vec<fuzzy_char> {
    let mut cs = Vec::new();
    let mut current: style_align = STYLE_ALIGN_LEFT;
    let mut sy: style = style {
        gc: grid_cell {
            data: utf8_data {
                data: [0; 32],
                have: 0,
                size: 0,
                width: 0,
            },
            attr: 0,
            flags: 0,
            fg: 0,
            bg: 0,
            us: 0,
            link: 0,
        },
        ignore: 0,
        dim: 0,
        fill: 0,
        align: STYLE_ALIGN_DEFAULT,
        list: STYLE_LIST_OFF,
        range_type: STYLE_RANGE_NONE,
        range_argument: 0,
        range_string: [0; 16],
        width: 0,
        width_percentage: 0,
        pad: 0,
        default_type: STYLE_DEFAULT_BASE,
        link: 0,
    };
    let mut remaining = text.to_bytes();
    let mut ud: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut hash: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    let mut bracket: utf8_data = utf8_data {
        data: [0; 32],
        have: 0,
        size: 0,
        width: 0,
    };
    widths.fill(0);
    style_set(&mut sy, &grid_default_cell);
    utf8_set(&mut hash, b'#');
    utf8_set(&mut bracket, b'[');
    while !remaining.is_empty() {
        if remaining[0] == b'#' {
            let hashes = remaining.iter().take_while(|&&byte| byte == b'#').count();
            if remaining.get(hashes) != Some(&b'[') {
                for _ in 0..hashes.div_ceil(2) {
                    fuzzy_add(&mut cs, current, &hash, widths);
                }
                remaining = &remaining[hashes..];
            } else {
                for _ in 0..hashes / 2 {
                    fuzzy_add(&mut cs, current, &hash, widths);
                }
                if hashes % 2 == 0 {
                    fuzzy_add(&mut cs, current, &bracket, widths);
                    remaining = &remaining[hashes + 1..];
                } else {
                    let style_start = &remaining[hashes + 1..];
                    let Some(len) = format_skip(style_start) else {
                        break;
                    };
                    let style_text = CString::new(&style_start[..len]).unwrap();
                    if style_parse(&mut sy, &grid_default_cell, &style_text) == 0 {
                        current = fuzzy_align(sy.align);
                    }
                    remaining = &style_start[len + 1..];
                }
            }
        } else {
            remaining = fuzzy_decode_one(remaining, &mut ud);
            if ud.size == 1 && (ud.data[0] <= 0x1f || ud.data[0] >= 0x7f) {
                continue;
            }
            fuzzy_add(&mut cs, current, &ud, widths);
        }
    }
    cs
}
fn fuzzy_column(
    fc: &fuzzy_char,
    start: &[u_int; 5],
    src: &[u_int; 5],
    vis: &[u_int; 5],
) -> Option<u_int> {
    let a = fc.align as usize;
    if fc.offset < src[a] || fc.offset >= src[a].wrapping_add(vis[a]) {
        return None;
    }
    Some(start[a].wrapping_add(fc.offset.wrapping_sub(src[a])))
}

/// Decode a UTF-8 term, or `None` if it has more characters than `out` holds.
fn fuzzy_decode<'a>(mut text: &[u8], out: &'a mut [utf8_data]) -> Option<&'a [utf8_data]> {
    let mut count = 0;
    while !text.is_empty() {
        if count == out.len() {
            return None;
        }
        text = fuzzy_decode_one(text, &mut out[count]);
        count += 1;
    }
    Some(&out[..count])
}

fn fuzzy_score_positions(positions: &[usize], cs: &[fuzzy_char]) -> i32 {
    let Some(&first) = positions.first() else {
        return 0;
    };
    let mut score = 0;
    if first == 0 {
        score += FUZZY_BONUS_START;
    } else {
        if fuzzy_is_boundary(&cs[first - 1].ud) {
            score += FUZZY_BONUS_BOUNDARY;
        }
        score -= first.min(FUZZY_PENALTY_LEADING_MAX as usize) as i32 * FUZZY_PENALTY_LEADING;
    }
    for pair in positions.windows(2) {
        if pair[1] == pair[0] + 1 {
            score += FUZZY_BONUS_CONSECUTIVE;
        } else if fuzzy_is_boundary(&cs[pair[1] - 1].ud) {
            score += FUZZY_BONUS_BOUNDARY;
        }
    }
    let span = positions[positions.len() - 1] - first + 1;
    score.wrapping_sub((span - positions.len()) as i32 * FUZZY_PENALTY_GAP)
}

fn fuzzy_match_fuzzy(
    tok: &[utf8_data],
    cs: &[fuzzy_char],
    fold: bool,
    matched: &mut [u8],
) -> Option<i32> {
    if tok.is_empty() || cs.is_empty() {
        return None;
    }
    let mut positions = vec![0; tok.len()];
    let mut ci = 0;
    for (pi, token) in tok.iter().enumerate() {
        while ci < cs.len() && !fuzzy_char_equal(token, &cs[ci].ud, fold) {
            ci += 1;
        }
        if ci == cs.len() {
            return None;
        }
        positions[pi] = ci;
        ci += 1;
    }
    ci = positions[tok.len() - 1];
    for pi in (0..tok.len()).rev() {
        while !fuzzy_char_equal(&tok[pi], &cs[ci].ud, fold) {
            ci = ci.checked_sub(1)?;
        }
        positions[pi] = ci;
        if pi != 0 {
            ci = ci.checked_sub(1)?;
        }
    }
    let score = fuzzy_score_positions(&positions, cs);
    for position in positions {
        matched[position] = 1;
    }
    Some(score)
}

fn fuzzy_score_exact(
    start: usize,
    toklen: usize,
    cs: &[fuzzy_char],
    prefix: bool,
    suffix: bool,
) -> i32 {
    let mut score =
        FUZZY_BONUS_EXACT.wrapping_add((toklen as i32).wrapping_mul(FUZZY_BONUS_CONSECUTIVE));
    if prefix {
        score += FUZZY_BONUS_PREFIX;
    }
    if suffix {
        score += FUZZY_BONUS_SUFFIX;
    }
    if start == 0 {
        score += FUZZY_BONUS_START;
    } else if fuzzy_is_boundary(&cs[start - 1].ud) {
        score += FUZZY_BONUS_BOUNDARY;
    }
    score -= start.min(FUZZY_PENALTY_LEADING_MAX as usize) as i32 * FUZZY_PENALTY_LEADING;
    if !prefix && !suffix {
        score = score.wrapping_sub((cs.len() - start - toklen) as i32);
    }
    score
}

fn fuzzy_match_exact(
    tok: &[utf8_data],
    cs: &[fuzzy_char],
    fold: bool,
    prefix: bool,
    suffix: bool,
    matched: Option<&mut [u8]>,
) -> Option<i32> {
    if tok.is_empty() || tok.len() > cs.len() || (prefix && suffix && tok.len() != cs.len()) {
        return None;
    }
    let starts = if prefix {
        0..1
    } else if suffix {
        cs.len() - tok.len()..cs.len() - tok.len() + 1
    } else {
        0..cs.len() - tok.len() + 1
    };
    let mut best = None;
    for start in starts {
        if tok
            .iter()
            .zip(&cs[start..])
            .any(|(a, b)| !fuzzy_char_equal(a, &b.ud, fold))
        {
            continue;
        }
        let score = fuzzy_score_exact(start, tok.len(), cs, prefix, suffix);
        if best.is_none_or(|(_, bestscore)| score > bestscore) {
            best = Some((start, score));
        }
    }
    let (start, score) = best?;
    if let Some(matched) = matched {
        matched[start..start + tok.len()].fill(1);
    }
    Some(score)
}
fn fuzzy_parse_term(mut text: &[u8]) -> Option<fuzzy_term<'_>> {
    let mut term = fuzzy_term {
        inverse: 0,
        exact: 0,
        prefix: 0,
        suffix: 0,
        text: &[],
    };
    if text.first()? == &b'!' {
        term.inverse = 1;
        text = &text[1..];
    }
    match text.first()? {
        b'\'' => {
            term.exact = 1;
            text = &text[1..];
        }
        b'^' => {
            term.exact = 1;
            term.prefix = 1;
            text = &text[1..];
        }
        _ => {}
    }
    if text.last()? == &b'$' {
        term.exact = 1;
        term.suffix = 1;
        text = &text[..text.len() - 1];
    }
    if text.is_empty() {
        return None;
    }
    if term.inverse != 0 {
        term.exact = 1;
    }
    term.text = text;
    Some(term)
}
fn fuzzy_match_term(
    term: &fuzzy_term<'_>,
    tok: &mut [utf8_data],
    cs: &[fuzzy_char],
    fold: bool,
    matched: &mut [u8],
) -> Option<i32> {
    let Some(tok) = fuzzy_decode(term.text, tok) else {
        return (term.inverse != 0).then_some(0);
    };
    let value = if term.exact != 0 {
        fuzzy_match_exact(
            tok,
            cs,
            fold,
            term.prefix != 0,
            term.suffix != 0,
            if term.inverse != 0 {
                None
            } else {
                Some(matched)
            },
        )
    } else {
        fuzzy_match_fuzzy(tok, cs, fold, matched)
    };
    if term.inverse != 0 {
        value.is_none().then_some(0)
    } else {
        value
    }
}

fn fuzzy_match_group(
    group: &[u8],
    tok: &mut [utf8_data],
    cs: &[fuzzy_char],
    fold: bool,
    matched: &mut [u8],
) -> Option<i32> {
    let mut any = false;
    let mut score = 0;
    for text in group
        .split(|byte| *byte == b' ')
        .filter(|text| !text.is_empty())
    {
        let term = fuzzy_parse_term(text)?;
        any = true;
        score += fuzzy_match_term(&term, tok, cs, fold, matched)?;
    }
    any.then_some(score)
}
pub(crate) unsafe fn fuzzy_match_owned(
    pattern: &CStr,
    text: &CStr,
    mut width: u_int,
    mut score: Option<&mut u_int>,
) -> Option<Vec<bitstr_t>> {
    let mut cs: Vec<fuzzy_char>;
    let mut matched: Vec<u8>;
    let mut best: Vec<u8>;
    let mut tok: Vec<utf8_data>;
    let mut mask: Vec<bitstr_t>;

    let mut i: u_int = 0;
    let mut j: u_int = 0;
    let mut widths: [u_int; 5] = [0; 5];
    let mut start: [u_int; 5] = [0; 5];
    let mut src: [u_int; 5] = [0; 5];
    let mut vis: [u_int; 5] = [0; 5];
    let mut wl: u_int = 0;
    let mut wc: u_int = 0;
    let mut wr: u_int = 0;
    let mut wa: u_int = 0;
    let mut bestscore: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut found: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if width == 0 as u_int {
        return None;
    }
    let pattern = pattern.to_bytes();
    if pattern.iter().all(|byte| matches!(byte, b' ' | b'|')) {
        if let Some(score) = score.as_deref_mut() {
            *score = 0 as u_int;
        }
        return Some(vec![
            0;
            (width.wrapping_add(7 as u_int) >> 3 as ::core::ffi::c_int)
                as usize
        ]);
    }
    let fold = !pattern.iter().any(u8::is_ascii_uppercase);
    cs = fuzzy_scan(text, &mut widths);
    let ncs: u_int = cs.len() as u_int;
    matched = vec![0; ncs.max(1) as usize];
    best = vec![0; ncs.max(1) as usize];
    tok = vec![
        utf8_data {
            data: [0; 32],
            have: 0,
            size: 0,
            width: 0,
        };
        pattern.len().min(cs.len())
    ];
    for group in pattern.split(|byte| *byte == b'|') {
        matched.fill(0);
        if let Some(groupscore) = fuzzy_match_group(group, &mut tok, &cs, fold, &mut matched) {
            if found == 0 || groupscore > bestscore {
                found = 1 as ::core::ffi::c_int;
                bestscore = groupscore;
                best.copy_from_slice(&matched);
            }
        }
    }
    drop(tok);
    if found == 0 {
        drop(best);
        drop(matched);
        drop(cs);
        return None;
    }
    wl = widths[STYLE_ALIGN_LEFT as ::core::ffi::c_int as usize];
    wc = widths[STYLE_ALIGN_CENTRE as ::core::ffi::c_int as usize];
    wr = widths[STYLE_ALIGN_RIGHT as ::core::ffi::c_int as usize];
    wa = widths[STYLE_ALIGN_ABSOLUTE_CENTRE as ::core::ffi::c_int as usize];
    while wl.wrapping_add(wc).wrapping_add(wr) > width {
        if wc > 0 as u_int {
            wc = wc.wrapping_sub(1);
        } else if wr > 0 as u_int {
            wr = wr.wrapping_sub(1);
        } else {
            wl = wl.wrapping_sub(1);
        }
    }
    if wa > width {
        wa = width;
    }
    start[STYLE_ALIGN_LEFT as ::core::ffi::c_int as usize] = 0 as u_int;
    src[STYLE_ALIGN_LEFT as ::core::ffi::c_int as usize] = 0 as u_int;
    vis[STYLE_ALIGN_LEFT as ::core::ffi::c_int as usize] = wl;
    start[STYLE_ALIGN_RIGHT as ::core::ffi::c_int as usize] = width.wrapping_sub(wr);
    src[STYLE_ALIGN_RIGHT as ::core::ffi::c_int as usize] =
        widths[STYLE_ALIGN_RIGHT as ::core::ffi::c_int as usize].wrapping_sub(wr);
    vis[STYLE_ALIGN_RIGHT as ::core::ffi::c_int as usize] = wr;
    start[STYLE_ALIGN_CENTRE as ::core::ffi::c_int as usize] = wl
        .wrapping_add(
            width
                .wrapping_sub(wr)
                .wrapping_sub(wl)
                .wrapping_div(2 as u_int),
        )
        .wrapping_sub(wc.wrapping_div(2 as u_int));
    src[STYLE_ALIGN_CENTRE as ::core::ffi::c_int as usize] = widths
        [STYLE_ALIGN_CENTRE as ::core::ffi::c_int as usize]
        .wrapping_div(2 as u_int)
        .wrapping_sub(wc.wrapping_div(2 as u_int));
    vis[STYLE_ALIGN_CENTRE as ::core::ffi::c_int as usize] = wc;
    start[STYLE_ALIGN_ABSOLUTE_CENTRE as ::core::ffi::c_int as usize] =
        width.wrapping_sub(wa).wrapping_div(2 as u_int);
    src[STYLE_ALIGN_ABSOLUTE_CENTRE as ::core::ffi::c_int as usize] = 0 as u_int;
    vis[STYLE_ALIGN_ABSOLUTE_CENTRE as ::core::ffi::c_int as usize] = wa;
    mask = vec![0; (width.wrapping_add(7 as u_int) >> 3 as ::core::ffi::c_int) as usize];
    i = 0 as u_int;
    while i < ncs {
        if best[i as usize] != 0 {
            if let Some(column) = fuzzy_column(&cs[i as usize], &start, &src, &vis) {
                j = 0 as u_int;
                while j < cs[i as usize].width && column.wrapping_add(j) < width {
                    let fresh0 =
                        &mut mask[(column.wrapping_add(j) >> 3 as ::core::ffi::c_int) as usize];
                    *fresh0 = (*fresh0 as ::core::ffi::c_int
                        | (1 as ::core::ffi::c_int) << (column.wrapping_add(j) & 0x7 as u_int))
                        as bitstr_t;
                    j = j.wrapping_add(1);
                }
            }
        }
        i = i.wrapping_add(1);
    }
    drop(best);
    drop(matched);
    drop(cs);
    if let Some(score) = score {
        *score = if bestscore < 0 as ::core::ffi::c_int {
            0 as u_int
        } else {
            bestscore as u_int
        };
    }
    Some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_preserve_ties_inverse_terms_and_negative_matches() {
        unsafe {
            for (pattern, text, expected_mask, expected_score) in [
                (c"abc", c"abc", 0b111, 24),
                (c"abc", c"aabcbc", 0b001110, 11),
                (c"'ab", c"xx ab ab", 0b00011000, 1014),
                (c"^ab", c"abcd", 0b11, 1224),
                (c"ab$", c"zz ab", 0b11000, 1117),
                (c"^ab$", c"ab", 0b11, 1324),
                (c"!ab", c"axb", 0, 0),
                (c"ab | 'z", c"abz", 0b100, 1004),
                (c"a b", c"ab", 0b11, 11),
                (c"b | a", c"xa xb", 0b10, 0),
                (c" ", c"anything", 0, 0),
            ] {
                let mut score = u32::MAX;
                let mask = fuzzy_match_owned(pattern, text, 8, Some(&mut score)).unwrap();
                assert_eq!(
                    (mask[0], score),
                    (expected_mask, expected_score),
                    "{pattern:?} {text:?}"
                );
            }
            for (pattern, text, width) in [(c"missing", c"abc", 8), (c"a", c"a", 0)] {
                let mut score = 123;
                assert!(fuzzy_match_owned(pattern, text, width, Some(&mut score)).is_none());
                assert_eq!(score, 123);
            }
        }
    }

    #[test]
    fn decoded_terms_reuse_only_the_initialized_prefix() {
        unsafe {
            for (pattern, text, mask) in [
                (c"^longterm$ | ^a$", c"a", Some(1)),
                (c"^a$ | ^longterm$", c"longterm", Some(255)),
                (c"'\xc3 | ^a$", c"a", Some(1)),
                (c"^a$", c"\xe2\x82a", Some(1)),
                (c"^a$", c"\xc3a", Some(1)),
                (c"^a$", c"a\xf0\x9f", Some(1)),
                (c"!a b", c"ab", None),
                (c"a missing | b", c"ab", Some(2)),
            ] {
                assert_eq!(
                    fuzzy_match_owned(pattern, text, 8, None).map(|bits| bits[0]),
                    mask,
                    "{pattern:?} {text:?}"
                );
            }
        }
    }

    #[test]
    fn borrowed_terms_preserve_operators_and_group_boundaries() {
        unsafe {
            for (pattern, text, expected) in [
                (c"^ab$", c"ab", Some(0b11)),
                (c"^ab$", c"abc", None),
                (c"'ab", c"axb", None),
                (c"!ab", c"axb", Some(0)),
                (c"!ab", c"zab", None),
                (c"!^ab$", c"abc", Some(0)),
                (c"!^ab$", c"ab", None),
                (c"  ^ab  cd$ | missing ", c"abcd", Some(0b1111)),
                (c"missing||^ab$|", c"ab", Some(0b11)),
                (c"^é$", c"é", Some(1)),
                (c"^\xff$", c"\xff", None),
                (c" | ", c"ab", Some(0)),
            ] {
                let mask = fuzzy_match_owned(pattern, text, 8, None);
                assert_eq!(mask.map(|mask| mask[0]), expected, "{pattern:?}");
            }
            for pattern in [c"!", c"'", c"^", c"$", c"!^$", c"'$", c"ab ^"] {
                assert!(fuzzy_match_owned(pattern, c"ab", 8, None).is_none());
            }
        }
    }

    #[test]
    fn bracketed_style_controls_fuzzy_match_columns() {
        unsafe {
            let pattern = c"R";
            let right = c"L#[align=right]R";
            let mask = fuzzy_match_owned(pattern, right, 8, None);
            assert_eq!(mask.unwrap()[0], 1 << 7);

            let invalid = c"L#[align=bogus]R";
            let mask = fuzzy_match_owned(pattern, invalid, 8, None);
            assert_eq!(mask.unwrap()[0], 1 << 1);

            let incomplete = c"L#[align=right R";
            let mask = fuzzy_match_owned(pattern, incomplete, 8, None);
            assert!(mask.is_none());
        }
    }

    #[test]
    fn fuzzy_match_backtracks_and_resets_between_alternatives() {
        unsafe {
            let mask = fuzzy_match_owned(c"q|abc", c"aabcbc", 6, None);
            assert_eq!(mask.unwrap()[0], 0b001110);

            let mask = fuzzy_match_owned(c"abc|q", c"aabcbc", 6, None);
            assert_eq!(mask.unwrap()[0], 0b001110);

            assert!(fuzzy_match_owned(c"abc", c"abx", 3, None).is_none());
        }
    }

    #[test]
    fn fuzzy_match_crosses_scan_growth_boundary_and_stops_at_nul() {
        let text = format!("{}zy\0q", "a".repeat(63));
        let text = CStr::from_bytes_until_nul(text.as_bytes()).unwrap();
        unsafe {
            let mask = fuzzy_match_owned(c"zy", text, 65, None);
            let mask = mask.unwrap();
            assert_eq!(mask.as_slice(), &[0, 0, 0, 0, 0, 0, 0, 0x80, 1]);

            assert!(fuzzy_match_owned(c"q", text, 65, None).is_none());
        }
    }
}
