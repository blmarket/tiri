use super::*;

#[test]
fn cursor_observation_preserves_terminal_shape_and_blink() {
    let mut screen = screen::default();
    assert_eq!(cursor_shape(&screen), 0);
    for (shape, steady) in [
        (SCREEN_CURSOR_BLOCK, 2),
        (SCREEN_CURSOR_UNDERLINE, 4),
        (SCREEN_CURSOR_BAR, 6),
    ] {
        screen.cstyle = shape;
        screen.mode = 0;
        assert_eq!(cursor_shape(&screen), steady);
        screen.mode = MODE_CURSOR_BLINKING;
        assert_eq!(cursor_shape(&screen), steady - 1);
    }
}
