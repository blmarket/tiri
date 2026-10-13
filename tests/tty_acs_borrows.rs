use tiri::src::shared::borders::{CELL_LR, CELL_RD};
use tiri::src::shared::tty::{tty, tty_code, tty_term, TTYC_U8};
use tiri::src::tty_acs::{
    tty_acs_double_borders, tty_acs_get, tty_acs_heavy_borders, tty_acs_needed,
    tty_acs_reverse_get, tty_acs_rounded_borders,
};

#[test]
fn terminal_capabilities_choose_unicode_or_inline_legacy_mappings() {
    unsafe {
        assert_eq!(tty_acs_needed(None, true), 0);
        assert_eq!(tty_acs_get(None, true, b'q'), Some(c"─"));
        assert!(tty_acs_get(None, true, b'A').is_none());
        let mut term = tty_term::empty();
        term.codes = vec![tty_code::None; TTYC_U8 as usize + 1].into_boxed_slice();
        term.acs[b'q' as usize] = [0x80, 0];
        let mut terminal = tty {
            term: Some(Box::new(term)),
            ..Default::default()
        };
        for (utf8, u8_cap, legacy) in [
            (false, None, true),
            (false, Some(1), true),
            (true, None, false),
            (true, Some(1), false),
            (true, Some(0), true),
        ] {
            terminal.term.as_deref_mut().unwrap().codes[TTYC_U8 as usize] =
                u8_cap.map_or(tty_code::None, tty_code::Number);
            assert_eq!(tty_acs_needed(Some(&terminal), utf8) != 0, legacy);
            let mapped = tty_acs_get(Some(&terminal), utf8, b'q').unwrap();
            assert_eq!(
                mapped.to_bytes(),
                if legacy {
                    &[0x80][..]
                } else {
                    "─".as_bytes()
                }
            );
            if legacy {
                assert_eq!(
                    mapped.to_bytes().as_ptr(),
                    terminal.term.as_deref().unwrap().acs[b'q' as usize].as_ptr()
                );
            }
            assert!(tty_acs_get(Some(&terminal), utf8, b'A').is_none());
        }
    }
}

#[test]
fn border_lookup_and_reverse_translation_preserve_table_selection() {
    for (cell, expected) in [
        (tty_acs_double_borders(CELL_LR), "═"),
        (tty_acs_heavy_borders(CELL_LR), "━"),
        (tty_acs_rounded_borders(CELL_RD), "╭"),
    ] {
        assert_eq!(&cell.data[..cell.size as usize], expected.as_bytes());
        assert_eq!(cell.width, 1);
    }
    assert_eq!(tty_acs_reverse_get(c"─", 3), Some(b'q'));
    assert_eq!(tty_acs_reverse_get(c"│", 3), Some(b'x'));
    assert_eq!(tty_acs_reverse_get(c"·", 2), Some(b'~'));
    assert_eq!(tty_acs_reverse_get(c"─", 2), None);
    assert_eq!(tty_acs_reverse_get(c"─", 4), None);
    assert_eq!(tty_acs_reverse_get(c"─x", 3), None);
    assert_eq!(tty_acs_reverse_get(c"漢", 3), None);
}
