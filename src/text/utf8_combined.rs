use crate::src::shared::abi::*;
use crate::src::shared::grid::*;
use crate::src::shared::utf8::wchar_t;
use crate::src::shared::utf8::*;
use crate::src::shared::utf8::{
    hanguljamo_state, HANGULJAMO_STATE_CHOSEONG, HANGULJAMO_STATE_COMPOSABLE,
    HANGULJAMO_STATE_NOT_COMPOSABLE, HANGULJAMO_STATE_NOT_HANGULJAMO,
};
use crate::src::text::utf8::utf8_towc;

pub const HANGULJAMO_CLASS_NOT_HANGULJAMO: hanguljamo_class = 0;
pub const HANGULJAMO_CLASS_JUNGSEONG: hanguljamo_class = 2;
pub type hanguljamo_class = ::core::ffi::c_uint;
pub const HANGULJAMO_CLASS_JONGSEONG: hanguljamo_class = 3;
pub const HANGULJAMO_CLASS_CHOSEONG: hanguljamo_class = 1;
pub const HANGULJAMO_SUBCLASS_NOT_HANGULJAMO: hanguljamo_subclass = 0;
pub const HANGULJAMO_SUBCLASS_EXTENDED_OLD_JONGSEONG: hanguljamo_subclass = 11;
pub const HANGULJAMO_SUBCLASS_OLD_JONGSEONG: hanguljamo_subclass = 8;
pub const HANGULJAMO_SUBCLASS_JONGSEONG: hanguljamo_subclass = 7;
pub const HANGULJAMO_SUBCLASS_EXTENDED_OLD_JUNGSEONG: hanguljamo_subclass = 10;
pub const HANGULJAMO_SUBCLASS_OLD_JUNGSEONG: hanguljamo_subclass = 6;
pub const HANGULJAMO_SUBCLASS_JUNGSEONG_FILLER: hanguljamo_subclass = 4;
pub const HANGULJAMO_SUBCLASS_JUNGSEONG: hanguljamo_subclass = 5;
pub const HANGULJAMO_SUBCLASS_EXTENDED_OLD_CHOSEONG: hanguljamo_subclass = 9;
pub const HANGULJAMO_SUBCLASS_OLD_CHOSEONG: hanguljamo_subclass = 2;
pub const HANGULJAMO_SUBCLASS_CHOSEONG_FILLER: hanguljamo_subclass = 3;
pub const HANGULJAMO_SUBCLASS_CHOSEONG: hanguljamo_subclass = 1;
pub type hanguljamo_subclass = ::core::ffi::c_uint;
pub fn utf8_has_zwj(ud: &utf8_data) -> ::core::ffi::c_int {
    ud.data
        .get(..ud.size as usize)
        .is_some_and(|bytes| bytes.ends_with(b"\xe2\x80\x8d")) as i32
}

pub fn utf8_is_zwj(ud: &utf8_data) -> ::core::ffi::c_int {
    (ud.size == 3 && ud.data[..3] == *b"\xe2\x80\x8d") as i32
}

pub fn utf8_is_vs(ud: &utf8_data) -> ::core::ffi::c_int {
    (ud.size == 3 && ud.data[..3] == *b"\xef\xb8\x8f") as i32
}

pub fn utf8_is_hangul_filler(ud: &utf8_data) -> ::core::ffi::c_int {
    (ud.size == 3 && ud.data[..3] == *b"\xe3\x85\xa4") as i32
}

fn utf8_regional_count(ud: &utf8_data) -> u_int {
    let Some(bytes) = ud.data.get(..ud.size as usize) else {
        return 0;
    };
    bytes
        .windows(4)
        .filter(|part| part[..3] == *b"\xf0\x9f\x87" && (0xa6..=0xbf).contains(&part[3]))
        .count() as u_int
}
pub fn utf8_should_combine(with: &utf8_data, add: &utf8_data) -> ::core::ffi::c_int {
    let mut w: wchar_t = 0;
    let mut a: wchar_t = 0;
    if utf8_towc(with, &mut w) as ::core::ffi::c_uint
        != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return 0 as ::core::ffi::c_int;
    }
    if utf8_towc(add, &mut a) as ::core::ffi::c_uint
        != UTF8_DONE as ::core::ffi::c_int as ::core::ffi::c_uint
    {
        return 0 as ::core::ffi::c_int;
    }
    if a >= 0x1f1e6 as wchar_t
        && a <= 0x1f1ff as wchar_t
        && (w >= 0x1f1e6 as wchar_t && w <= 0x1f1ff as wchar_t)
    {
        if utf8_regional_count(with) != 1 as u_int {
            return 0 as ::core::ffi::c_int;
        }
        if utf8_regional_count(add) != 1 as u_int {
            return 0 as ::core::ffi::c_int;
        }
        return 1 as ::core::ffi::c_int;
    }
    match a {
        128075 | 128076 | 128077 | 128078 | 128079 | 128080 | 128102 | 128103 | 128104 | 128105
        | 128110 | 128112 | 128113 | 128114 | 128115 | 128116 | 128117 | 128118 | 128119
        | 128120 | 128124 | 128129 | 128130 | 128131 | 128133 | 128134 | 128135 | 128170
        | 128373 | 128378 | 128400 | 128405 | 128406 | 128581 | 128582 | 128583 | 128587
        | 128588 | 128589 | 128590 | 128591 | 128692 | 128693 | 128694 | 129318 | 129335
        | 129336 | 129337 | 129341 | 129342 | 129461 | 129462 | 129464 | 129465 | 129485
        | 129486 | 129487 | 129489 | 129490 | 129491 | 129492 | 129493 | 129494 | 129495
        | 129496 | 129497 | 129498 | 129499 | 129500 | 129501 | 129502 | 129503
            if w >= 0x1f3fb as wchar_t && w <= 0x1f3ff as wchar_t =>
        {
            return 1 as ::core::ffi::c_int;
        }
        _ => {}
    }
    0 as ::core::ffi::c_int
}
fn hanguljamo_get_subclass(s: &[u_char; 3]) -> hanguljamo_subclass {
    match s[0] as ::core::ffi::c_int {
        225 => match s[1] as ::core::ffi::c_int {
            132 => {
                if s[2] as ::core::ffi::c_int >= 0x80 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0x92 as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_CHOSEONG;
                }
                if s[2] as ::core::ffi::c_int >= 0x93 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xbf as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_OLD_CHOSEONG;
                }
            }
            133 => {
                if s[2] as ::core::ffi::c_int == 0x9f as ::core::ffi::c_int {
                    return HANGULJAMO_SUBCLASS_CHOSEONG_FILLER;
                }
                if s[2] as ::core::ffi::c_int == 0xa0 as ::core::ffi::c_int {
                    return HANGULJAMO_SUBCLASS_JUNGSEONG_FILLER;
                }
                if s[2] as ::core::ffi::c_int >= 0x80 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0x9e as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_OLD_CHOSEONG;
                }
                if s[2] as ::core::ffi::c_int >= 0xa1 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xb5 as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_JUNGSEONG;
                }
                if s[2] as ::core::ffi::c_int >= 0xb6 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xbf as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_OLD_JUNGSEONG;
                }
            }
            134 => {
                if s[2] as ::core::ffi::c_int >= 0x80 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xa7 as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_OLD_JUNGSEONG;
                }
                if s[2] as ::core::ffi::c_int >= 0xa8 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xbf as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_JONGSEONG;
                }
            }
            135 => {
                if s[2] as ::core::ffi::c_int >= 0x80 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0x82 as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_JONGSEONG;
                }
                if s[2] as ::core::ffi::c_int >= 0x83 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xbf as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_OLD_JONGSEONG;
                }
            }
            _ => {}
        },
        234 => {
            if s[1] as ::core::ffi::c_int == 0xa5 as ::core::ffi::c_int
                && s[2] as ::core::ffi::c_int >= 0xa0 as ::core::ffi::c_int
                && s[2] as ::core::ffi::c_int <= 0xbc as ::core::ffi::c_int
            {
                return HANGULJAMO_SUBCLASS_EXTENDED_OLD_CHOSEONG;
            }
        }
        237 => {
            if s[1] as ::core::ffi::c_int == 0x9e as ::core::ffi::c_int
                && s[2] as ::core::ffi::c_int >= 0xb0 as ::core::ffi::c_int
                && s[2] as ::core::ffi::c_int <= 0xbf as ::core::ffi::c_int
            {
                return HANGULJAMO_SUBCLASS_EXTENDED_OLD_JUNGSEONG;
            }
            if !(s[1] as ::core::ffi::c_int != 0x9f as ::core::ffi::c_int) {
                if s[2] as ::core::ffi::c_int >= 0x80 as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0x86 as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_EXTENDED_OLD_JUNGSEONG;
                }
                if s[2] as ::core::ffi::c_int >= 0x8b as ::core::ffi::c_int
                    && s[2] as ::core::ffi::c_int <= 0xbb as ::core::ffi::c_int
                {
                    return HANGULJAMO_SUBCLASS_EXTENDED_OLD_JONGSEONG;
                }
            }
        }
        _ => {}
    }
    HANGULJAMO_SUBCLASS_NOT_HANGULJAMO
}
fn hanguljamo_get_class(s: &[u_char; 3]) -> hanguljamo_class {
    match hanguljamo_get_subclass(s) as ::core::ffi::c_uint {
        1 | 3 | 2 | 9 => return HANGULJAMO_CLASS_CHOSEONG,
        5 | 4 | 6 | 10 => return HANGULJAMO_CLASS_JUNGSEONG,
        7 | 8 | 11 => return HANGULJAMO_CLASS_JONGSEONG,
        0 => return HANGULJAMO_CLASS_NOT_HANGULJAMO,
        _ => {}
    }
    HANGULJAMO_CLASS_NOT_HANGULJAMO
}
fn hanguljamo_last_three(ud: &utf8_data) -> Option<&[u_char; 3]> {
    let size = ud.size as usize;
    let start = size.checked_sub(3)?;
    ud.data.get(start..size)?.try_into().ok()
}
pub fn hanguljamo_check_state(p_ud: &utf8_data, ud: &utf8_data) -> hanguljamo_state {
    if ud.size as ::core::ffi::c_int != 3 as ::core::ffi::c_int {
        return HANGULJAMO_STATE_NOT_HANGULJAMO;
    }
    let bytes: &[u_char; 3] = ud.data.get(..3).unwrap().try_into().unwrap();
    match hanguljamo_get_class(bytes) as ::core::ffi::c_uint {
        1 => return HANGULJAMO_STATE_CHOSEONG,
        2 => {
            let Some(previous) = hanguljamo_last_three(p_ud) else {
                return HANGULJAMO_STATE_NOT_COMPOSABLE;
            };
            if hanguljamo_get_class(previous) as ::core::ffi::c_uint
                == HANGULJAMO_CLASS_CHOSEONG as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                return HANGULJAMO_STATE_COMPOSABLE;
            }
            return HANGULJAMO_STATE_NOT_COMPOSABLE;
        }
        3 => {
            let Some(previous) = hanguljamo_last_three(p_ud) else {
                return HANGULJAMO_STATE_NOT_COMPOSABLE;
            };
            if hanguljamo_get_class(previous) as ::core::ffi::c_uint
                == HANGULJAMO_CLASS_JUNGSEONG as ::core::ffi::c_int as ::core::ffi::c_uint
            {
                return HANGULJAMO_STATE_COMPOSABLE;
            }
            return HANGULJAMO_STATE_NOT_COMPOSABLE;
        }
        0 => return HANGULJAMO_STATE_NOT_HANGULJAMO,
        _ => {}
    }
    HANGULJAMO_STATE_NOT_HANGULJAMO
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(text: &str) -> utf8_data {
        let mut cell = utf8_data::default();
        cell.data[..text.len()].copy_from_slice(text.as_bytes());
        cell.size = text.len() as u8;
        cell.have = cell.size;
        cell
    }

    #[test]
    fn joiner_selector_and_filler_checks_respect_cell_boundaries() {
        assert_eq!(utf8_has_zwj(&cell("👩\u{200d}")), 1);
        assert_eq!(utf8_has_zwj(&cell("👩\u{200d}💻")), 0);
        assert_eq!(utf8_is_zwj(&cell("\u{200d}")), 1);
        assert_eq!(utf8_is_zwj(&cell("a\u{200d}")), 0);
        assert_eq!(utf8_is_vs(&cell("\u{fe0f}")), 1);
        assert_eq!(utf8_is_vs(&cell("\u{fe0e}")), 0);
        assert_eq!(utf8_is_hangul_filler(&cell("\u{3164}")), 1);
        assert_eq!(utf8_is_hangul_filler(&cell("\u{1160}")), 0);
        let mut malformed = cell("\u{200d}");
        malformed.size = 33;
        assert_eq!(utf8_has_zwj(&malformed), 0);
        assert_eq!(utf8_regional_count(&malformed), 0);
    }

    #[test]
    fn combination_preserves_flag_pairing_and_skin_tone_direction() {
        assert_eq!(utf8_should_combine(&cell("🇦"), &cell("🇧")), 1);
        assert_eq!(utf8_should_combine(&cell("🇦🇧"), &cell("🇨")), 0);
        assert_eq!(utf8_should_combine(&cell("🇦"), &cell("🇧🇨")), 0);
        assert_eq!(utf8_should_combine(&cell("🏻"), &cell("👋")), 1);
        assert_eq!(utf8_should_combine(&cell("👋"), &cell("🏻")), 0);
        assert_eq!(utf8_should_combine(&cell("🏻"), &cell("a")), 0);
        let mut invalid = cell("a");
        invalid.data[0] = 0xff;
        assert_eq!(utf8_should_combine(&invalid, &cell("🇦")), 0);
    }

    #[test]
    fn hangul_composition_uses_the_last_component_and_keeps_filler_classes() {
        for (leading, vowel, trailing) in [
            ("ᄀ", "ᅡ", "ᆨ"),
            ("\u{115f}", "\u{1160}", "\u{11c3}"),
            ("\u{a960}", "\u{d7b0}", "\u{d7cb}"),
        ] {
            assert_eq!(
                hanguljamo_check_state(&cell(""), &cell(leading)),
                HANGULJAMO_STATE_CHOSEONG
            );
            assert_eq!(
                hanguljamo_check_state(&cell(leading), &cell(vowel)),
                HANGULJAMO_STATE_COMPOSABLE
            );
            let combined = cell(&format!("{leading}{vowel}"));
            assert_eq!(
                hanguljamo_check_state(&combined, &cell(trailing)),
                HANGULJAMO_STATE_COMPOSABLE
            );
            assert_eq!(
                hanguljamo_check_state(&cell(leading), &cell(trailing)),
                HANGULJAMO_STATE_NOT_COMPOSABLE
            );
            assert_eq!(
                hanguljamo_check_state(&cell("a"), &cell(vowel)),
                HANGULJAMO_STATE_NOT_COMPOSABLE
            );
            assert_eq!(
                hanguljamo_check_state(&cell(leading), &cell("漢")),
                HANGULJAMO_STATE_NOT_HANGULJAMO
            );
        }
    }
}
