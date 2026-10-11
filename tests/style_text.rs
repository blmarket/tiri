//! Compatibility observations recorded before replacing the text APIs.
use hmux::src::shared::colour::{COLOUR_FLAG_256, COLOUR_FLAG_RGB, COLOUR_FLAG_THEME};
use hmux::src::{shared::grid::*, style::attributes::*, style::colour::*};
use std::ffi::{CStr, CString};

#[test]
fn legacy_text_contract() {
    unsafe {
        for (input, expected) in [
            ("bold", GRID_ATTR_BRIGHT),
            ("BoLd, |dim", GRID_ATTR_BRIGHT | GRID_ATTR_DIM),
            ("default", 0),
            ("none", 0),
            ("", -1),
            (" bold", -1),
            ("bold,", -1),
            ("bold\tdim", -1),
            ("none,bold", -1),
            ("noattr", -1),
        ] {
            assert_eq!(
                attributes_fromstring(CString::new(input).unwrap().as_ptr()),
                expected,
                "{input}"
            );
        }
        assert_eq!(
            CStr::from_ptr(attributes_tostring(GRID_ATTR_DIM | GRID_ATTR_BRIGHT)).to_bytes(),
            b"bright,dim"
        );
        assert_eq!(CStr::from_ptr(attributes_tostring(1 << 30)).to_bytes(), b"");
        for (input, expected) in [
            ("red", 1),
            ("1", 1),
            ("01", -1),
            ("90", 90),
            ("8", -1),
            ("default", 8),
            ("terminal", 9),
            ("none", -1),
            ("colour0", COLOUR_FLAG_256),
            ("COLOR255", COLOUR_FLAG_256 | 255),
            ("colour+1", COLOUR_FLAG_256 | 1),
            ("colour 1", COLOUR_FLAG_256 | 1),
            ("colour-0", COLOUR_FLAG_256),
            ("colour256", -1),
            ("colour-1", -1),
            ("colour1 ", -1),
            ("colour9223372036854775808", -1),
            ("#abcdef", COLOUR_FLAG_RGB | 0xabcdef),
            ("#abc", -1),
            ("#fffffg", -1),
            ("gray", COLOUR_FLAG_RGB | 0xbebebe),
            ("grey100", COLOUR_FLAG_RGB | 0xffffff),
            ("grey101", -1),
            ("grey-1", -1),
            ("grey+0", COLOUR_FLAG_RGB),
            ("themeblack", COLOUR_FLAG_THEME),
            ("themelightgray", -1),
        ] {
            assert_eq!(
                colour_fromstring(CString::new(input).unwrap().as_ptr()),
                expected,
                "{input}"
            );
        }
        for (value, expected) in [
            (-1, "none"),
            (10, "invalid"),
            (COLOUR_FLAG_THEME | COLOUR_FLAG_RGB | 1, "themewhite"),
            (COLOUR_FLAG_THEME | 10, "invalid"),
            (COLOUR_FLAG_256 | 256, "colour0"),
            (COLOUR_FLAG_RGB | COLOUR_FLAG_256 | 0xabcdef, "#abcdef"),
        ] {
            assert_eq!(
                CStr::from_ptr(colour_tostring(value)).to_bytes(),
                expected.as_bytes()
            );
        }
        for (input, expected) in [
            ("rgb:12/34/56", 0x123456),
            ("rgb:1234/5678/9abc", 0x12569a),
            ("#123456789abc", 0x12569a),
            ("256,-1,258tail", 0x00ff02),
            ("cmy:0/0/0", 0xffffff),
            ("cmyk:0/0/0/1", 0),
            (" red ", 0xff0000),
        ] {
            assert_eq!(
                colour_parseX11(CString::new(input).unwrap().as_ptr()),
                COLOUR_FLAG_RGB | expected,
                "{input}"
            );
        }
        assert_eq!(attributes_fromstring(c"\x7f".as_ptr()), -1);
        let bytes = CString::new(vec![0xff]).unwrap();
        assert_eq!(attributes_fromstring(bytes.as_ptr()), -1);
        assert_eq!(colour_fromstring(bytes.as_ptr()), -1);
    }
}

#[test]
fn legacy_escape_contract() {
    unsafe {
        for (value, bg, expected) in [
            (1, 0, Some("\x1b[31m")),
            (90, 1, Some("\x1b[100m")),
            (8, 0, Some("\x1b[39m")),
            (9, 1, Some("\x1b[49m")),
            (COLOUR_FLAG_256 | 255, 0, Some("\x1b[38;5;255m")),
            (COLOUR_FLAG_RGB | 0x123456, 1, Some("\x1b[48;2;18;52;86m")),
            (COLOUR_FLAG_THEME, 0, Some("\x1b[30m")),
            (-1, 0, Some("\x1b[39m")),
            (10, 0, None),
        ] {
            let p = colour_toescape(None, value, bg);
            assert_eq!(
                if p.is_null() {
                    None
                } else {
                    Some(CStr::from_ptr(p).to_str().unwrap())
                },
                expected
            );
        }
    }
}

#[test]
fn owned_results_and_bounded_inputs() {
    let attrs = attributes_format(GRID_ATTR_BRIGHT | GRID_ATTR_DIM);
    let rgb = colour_format(COLOUR_FLAG_RGB | 0x123456);
    let indexed = colour_format(COLOUR_FLAG_256 | 123);
    let escape = colour_format_escape(COLOUR_FLAG_RGB | 0x123456, false, 0).unwrap();
    for n in 0..256 {
        let _ = attributes_format(n);
        let _ = colour_format(COLOUR_FLAG_RGB | n);
        let _ = colour_format_escape(COLOUR_FLAG_256 | n, true, 0);
        unsafe {
            let _ = attributes_tostring(n);
            let _ = colour_tostring(COLOUR_FLAG_RGB | n);
            let _ = colour_toescape(None, n, 0);
        }
    }
    assert_eq!(attrs.to_bytes(), b"bright,dim");
    assert_eq!(rgb.to_bytes(), b"#123456");
    assert_eq!(indexed.to_bytes(), b"colour123");
    assert_eq!(escape.to_bytes(), b"\x1b[38;2;18;52;86m");
    for input in [&b""[..], b"\xff", b"red\0blue", b"bold\0dim"] {
        assert_eq!(attributes_parse(input), None);
        assert_eq!(colour_parse(input), None);
        assert_eq!(colour_parse_name(input), None);
        assert_eq!(colour_parse_x11(input), None);
    }
    assert_eq!(attributes_parse_cstr(c"bold"), Some(GRID_ATTR_BRIGHT));
    assert_eq!(colour_parse_cstr(c"red"), Some(1));
    assert_eq!(
        colour_parse_name_cstr(c"red"),
        Some(COLOUR_FLAG_RGB | 0xff0000)
    );
    // scanf accepts a trailing non-UTF-8 byte, without Unicode replacement.
    assert_eq!(
        colour_parse_x11(b"1,2,3\xff"),
        Some(COLOUR_FLAG_RGB | 0x010203)
    );
    assert_eq!(colour_parse_x11(b"cmy:NaN/0/0"), None);
    assert_eq!(colour_parse_x11(b"cmy:inf/0/0"), None);
    assert_eq!(colour_parse_x11(b"cmy:1.01/0/0"), None);
    assert_eq!(colour_parse_x11(b"cmy:-0.01/0/0"), None);
}

#[test]
fn caller_buffers_are_bounded_and_match_owned_formatting() {
    let values = [
        (0, b"none".as_slice()),
        (GRID_ATTR_BRIGHT | GRID_ATTR_DIM, b"bright,dim".as_slice()),
        (1 << 30, b"".as_slice()),
    ];
    for (value, expected) in values {
        let mut output = [0xa5; 64];
        let length = attributes_format_into(value, &mut output).unwrap();
        assert_eq!(&output[..length], expected);
        assert_eq!(output[length], 0);
        assert_eq!(
            &output[..length],
            &attributes_format(value).as_bytes_with_nul()[..length]
        );
    }

    let values = [
        (-1, b"none".as_slice()),
        (COLOUR_FLAG_THEME | 1, b"themewhite".as_slice()),
        (COLOUR_FLAG_RGB | 0xabcdef, b"#abcdef".as_slice()),
        (COLOUR_FLAG_256, b"colour0".as_slice()),
        (COLOUR_FLAG_256 | 255, b"colour255".as_slice()),
        (10, b"invalid".as_slice()),
    ];
    for (value, expected) in values {
        let mut output = [0xa5; 64];
        let length = colour_format_into(value, &mut output).unwrap();
        assert_eq!(&output[..length], expected);
        assert_eq!(output[length], 0);
        assert_eq!(
            &output[..length],
            &colour_format(value).as_bytes_with_nul()[..length]
        );
    }

    let mut too_small = [0xa5; 4];
    assert_eq!(
        attributes_format_into(GRID_ATTR_BRIGHT, &mut too_small),
        None
    );
    assert!(too_small.iter().all(|&byte| byte == 0xa5));
    assert_eq!(
        colour_format_into(COLOUR_FLAG_RGB | 0x123456, &mut too_small),
        None
    );
    assert!(too_small.iter().all(|&byte| byte == 0xa5));

    let mut escape = [0xa5; 64];
    let length =
        colour_format_escape_into(COLOUR_FLAG_RGB | 0x123456, false, 0, &mut escape).unwrap();
    assert_eq!(&escape[..length], b"\x1b[38;2;18;52;86m");
    assert_eq!(escape[length], 0);
    let mut short_escape = [0xa5; 4];
    assert_eq!(
        colour_format_escape_into(COLOUR_FLAG_RGB | 0x123456, false, 0, &mut short_escape),
        None
    );
    assert!(short_escape.iter().all(|&byte| byte == 0xa5));
}

#[test]
fn every_attribute_and_index() {
    let names = [
        "acs",
        "bright",
        "dim",
        "underscore",
        "blink",
        "reverse",
        "hidden",
        "italics",
        "strikethrough",
        "double-underscore",
        "curly-underscore",
        "dotted-underscore",
        "dashed-underscore",
        "overline",
    ];
    let mut bits = 0;
    for name in names {
        let bit = attributes_parse(name.as_bytes()).unwrap();
        assert_eq!(attributes_parse(name.to_uppercase().as_bytes()), Some(bit));
        assert_eq!(attributes_format(bit).to_bytes(), name.as_bytes());
        bits |= bit;
    }
    assert_eq!(
        attributes_format(bits).to_bytes(),
        names.join(",").as_bytes()
    );
    assert_eq!(
        attributes_format(bits | GRID_ATTR_NOATTR).to_bytes(),
        format!("{},noattr", names.join(",")).as_bytes()
    );
    for n in 0..=255 {
        let value = COLOUR_FLAG_256 | n;
        assert_eq!(colour_parse_cstr(&colour_format(value)), Some(value));
        assert_eq!(
            colour_parse(format!("COLOR+{n:03}").as_bytes()),
            Some(value)
        );
    }
    for n in 0..10 {
        let value = COLOUR_FLAG_THEME | n;
        assert_eq!(colour_parse_cstr(&colour_format(value)), Some(value));
    }
    for input in [
        b"colour2147483648".as_slice(),
        b"colour-9223372036854775809",
        b"colour9223372036854775807",
        b"colour9999999999999999999999999999999",
        b"grey101",
        b"grey-1",
        b"grey100 ",
        b"#12345\xff",
    ] {
        assert_eq!(colour_parse(input), None, "{input:?}");
    }
    for (value, expected) in [
        (i32::MIN, "invalid"),
        (i32::MAX, "invalid"),
        (-2, "invalid"),
        (-1, "none"),
        (0, "black"),
        (9, "terminal"),
        (10, "invalid"),
        (97, "brightwhite"),
        (98, "invalid"),
    ] {
        assert_eq!(colour_format(value).to_bytes(), expected.as_bytes());
    }
}

#[test]
fn locale_child() {
    let Ok(locale) = std::env::var("HMUX_STYLE_TEST_LOCALE") else {
        return;
    };
    let locale = CString::new(locale).unwrap();
    // This test is invoked alone in a subprocess; no concurrent setlocale.
    let available = unsafe { libc::setlocale(libc::LC_ALL, locale.as_ptr()) };
    assert!(!available.is_null());
    for (input, canonical, bit) in [
        (
            b"ITALICS".as_slice(),
            b"italics".as_slice(),
            GRID_ATTR_ITALICS,
        ),
        (
            b"\xddtalics".as_slice(),
            b"italics".as_slice(),
            GRID_ATTR_ITALICS,
        ),
        (b"BRIGHT".as_slice(), b"bright".as_slice(), GRID_ATTR_BRIGHT),
    ] {
        let input_c = CString::new(input).unwrap();
        let name_c = CString::new(canonical).unwrap();
        let equal = unsafe { libc::strcasecmp(input_c.as_ptr(), name_c.as_ptr()) == 0 };
        assert_eq!(attributes_parse(input), equal.then_some(bit));
    }
    let equal = unsafe { libc::strcasecmp(c"BRIGHTWHITE".as_ptr(), c"brightwhite".as_ptr()) == 0 };
    assert_eq!(colour_parse(b"BRIGHTWHITE"), equal.then_some(97));
    let decimal = unsafe { CStr::from_ptr((*libc::localeconv()).decimal_point) }.to_bytes();
    let mut input = b"cmy:0".to_vec();
    input.extend_from_slice(decimal);
    input.extend_from_slice(b"5/0/0");
    assert_eq!(colour_parse_x11(&input), Some(COLOUR_FLAG_RGB | 0x7fffff));
    assert_eq!(
        colour_parse_x11(b"1,2,3\xff"),
        Some(COLOUR_FLAG_RGB | 0x010203)
    );
}

#[test]
fn locale_contract_in_isolated_processes() {
    let locales = std::process::Command::new("locale")
        .arg("-a")
        .output()
        .unwrap();
    let available = String::from_utf8(locales.stdout).unwrap();
    for locale in ["C", "C.utf8", "tr_TR.iso88599", "tr_TR.utf8", "de_DE.utf8"] {
        if !available.lines().any(|name| name == locale) {
            eprintln!("locale {locale} unavailable; skipped");
            continue;
        }
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "locale_child", "--nocapture"])
                .env("HMUX_STYLE_TEST_LOCALE", locale)
                .status()
                .unwrap()
                .success(),
            "{locale}"
        );
    }
}

#[test]
fn frozen_x11_aliases() {
    for line in include_str!("fixtures/colour-names.txt").lines() {
        let (hex, name) = line.split_once(' ').unwrap();
        let expected = COLOUR_FLAG_RGB | i32::from_str_radix(hex, 16).unwrap();
        assert_eq!(colour_parse_name(name.as_bytes()), Some(expected), "{name}");
        let basic = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ];
        let application = basic
            .iter()
            .position(|&entry| entry == name)
            .map_or(expected, |index| index as i32);
        assert_eq!(colour_parse(name.as_bytes()), Some(application), "{name}");
        assert_eq!(
            colour_parse_name(name.to_uppercase().as_bytes()),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn x11_name_trimming_preserves_bytes_and_c_termination() {
    let alice_blue = COLOUR_FLAG_RGB | 0xf0f8ff;
    assert_eq!(colour_parse_x11(b"  AliceBlue  "), Some(alice_blue));
    assert_eq!(colour_parse_x11(b"  ALICEBLUE  "), Some(alice_blue));
    assert_eq!(colour_parse_x11(b"AliceBlue\t"), None);
    assert_eq!(colour_parse_x11(b"   "), None);
    assert_eq!(colour_parse_x11(b" \xff "), None);

    // The C ABI sees only the prefix before the first NUL.
    let c_input = b"  AliceBlue  \0red\0";
    assert_eq!(
        unsafe { colour_parseX11(c_input.as_ptr().cast()) },
        alice_blue
    );
}

#[test]
fn safe_formatting_is_independent_across_threads() {
    let threads: Vec<_> = (0..8)
        .map(|n| {
            std::thread::spawn(move || {
                let text = colour_format(COLOUR_FLAG_256 | n);
                let attrs = attributes_format(GRID_ATTR_BRIGHT);
                for m in 0..256 {
                    assert_eq!(
                        colour_parse_cstr(&colour_format(COLOUR_FLAG_256 | m)),
                        Some(COLOUR_FLAG_256 | m)
                    );
                    assert_eq!(
                        attributes_parse_cstr(&attributes_format(GRID_ATTR_DIM)),
                        Some(GRID_ATTR_DIM)
                    );
                }
                assert_eq!(text.to_bytes(), format!("colour{n}").as_bytes());
                assert_eq!(attrs.to_bytes(), b"bright");
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
