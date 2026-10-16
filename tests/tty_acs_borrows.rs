use tiri::src::shared::borders::{CELL_LR, CELL_RD};
use tiri::src::tty_acs::{
    tty_acs_double_borders, tty_acs_heavy_borders, tty_acs_reverse_get, tty_acs_rounded_borders,
};

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
