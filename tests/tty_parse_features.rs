use tiri::src::tty::{tty_get_features, tty_parse_features};

fn feature_names(bits: i32) -> String {
    tty_get_features(bits).to_str().unwrap().to_owned()
}

#[test]
fn feature_list_separators_removal_and_unknown_feature() {
    unsafe {
        let mut enabled = 0;
        let mut disabled = 0;
        let separators = c":,";

        tty_parse_features(
            c"RGB:256,256@:sixel",
            separators,
            &mut enabled,
            &mut disabled,
        );
        assert_eq!(feature_names(enabled), "RGB,sixel");
        assert_eq!(feature_names(disabled), "256");

        tty_parse_features(c"256:mouse", separators, &mut enabled, &mut disabled);
        assert_eq!(feature_names(enabled), "mouse,RGB,sixel");

        tty_parse_features(
            c"title:missing:clipboard",
            separators,
            &mut enabled,
            &mut disabled,
        );
        assert_eq!(feature_names(enabled), "mouse,RGB,sixel,title");

        tty_parse_features(
            std::ffi::CStr::from_bytes_until_nul(b"osc7\0,clipboard\0").unwrap(),
            separators,
            &mut enabled,
            &mut disabled,
        );
        assert_eq!(feature_names(enabled), "mouse,osc7,RGB,sixel,title");
    }
}
