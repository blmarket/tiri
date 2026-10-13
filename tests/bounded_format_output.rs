use tiri::src::{
    grid::grid_default_cell,
    input_keys::input_key_get_mouse,
    shared::{
        mouse::mouse_event,
        screen::{screen, MODE_MOUSE_BUTTON, MODE_MOUSE_SGR},
        style::{style, STYLE_ALIGN_RIGHT, STYLE_LIST_ON, STYLE_RANGE_USER},
    },
    style::{style_set, style_tostring},
};
use std::ffi::c_char;

#[test]
fn style_appends_preserve_raw_range_bytes_and_numeric_fields() {
    unsafe {
        let mut value = style::default();
        style_set(&mut value, &grid_default_cell);
        value.list = STYLE_LIST_ON;
        value.range_type = STYLE_RANGE_USER;
        value.range_string[0] = 0xffu8 as c_char;
        value.align = STYLE_ALIGN_RIGHT;
        value.dim = 25;
        value.width = 75;
        value.width_percentage = 1;
        value.pad = 2;
        assert_eq!(
            style_tostring(&mut value).to_bytes(),
            b"list=on,range=user|\xff,align=right,dim=25%,width=75%,pad=2",
        );
        // Reusing the static buffer must terminate the new, shorter result.
        style_set(&mut value, &grid_default_cell);
        value.width = 3;
        assert_eq!(style_tostring(&mut value).as_c_str(), c"width=3");
    }
}

#[test]
fn sgr_mouse_output_counts_the_final_character_as_one_byte() {
    let mut screen = screen::default();
    screen.mode = MODE_MOUSE_BUTTON | MODE_MOUSE_SGR;
    for byte in [b'M', b'm', 0xff, 0] {
        let mouse = mouse_event {
            sgr_type: byte as u32,
            sgr_b: 4,
            ..Default::default()
        };
        let mut buf = [0; 40];
        unsafe {
            let len = input_key_get_mouse(&mut screen, &mouse, 9, 19, &mut buf).unwrap();
            let mut expected = b"\x1b[<4;10;20".to_vec();
            expected.push(byte);
            assert_eq!(
                std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len),
                expected
            );
            assert_eq!(buf[len], 0);
        }
    }
}
