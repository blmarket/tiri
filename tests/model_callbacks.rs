//! Shared declarations retain their types across former translation-unit boundaries.
use std::ptr::null_mut;
use tiri::src::{arguments, screen, tty, window};

fn parse(
    _: &mut arguments::args,
    index: tiri::src::shared::abi::u_int,
) -> Result<
    tiri::src::shared::arguments::args_parse_type,
    tiri::src::shared::arguments::ArgsParseError,
> {
    if index == 7 {
        Ok(tiri::src::shared::arguments::ARGS_PARSE_STRING)
    } else {
        Err(tiri::src::shared::arguments::ArgsParseError::Usage)
    }
}

unsafe fn mode_screen(_: refbox::Weak<window::window_mode_entry>) -> *mut screen::screen {
    null_mut()
}

/// Invokes a parser through the shared-module slot type.
fn parse_through(
    callback: tiri::src::shared::arguments::args_parse_cb,
    args: &mut arguments::args,
    index: tiri::src::shared::abi::u_int,
) -> tiri::src::shared::arguments::args_parse_type {
    callback.expect("parser assigned")(args, index).expect("index parses")
}

#[test]
fn callbacks_cross_original_module_paths_without_conversion() {
    let parse_callback: arguments::args_parse_cb = Some(parse);
    let parser_callback: tiri::src::shared::arguments::args_parse_cb = parse_callback;
    let tty_callback: tty::tty_ctx_set_client_cb = Some(Box::new(|_, _| 17));
    let draw_callback: tiri::src::shared::tty::tty_ctx_set_client_cb = tty_callback;
    let mut mode = window::window_mode::default();
    assert!(mode.get_screen.is_none());
    mode.get_screen = Some(mode_screen);
    let copy_mode: tiri::src::window_copy::window_mode = mode;
    let mut parsed_args = arguments::args::empty();
    assert_eq!(
        parse_through(parser_callback, &mut parsed_args, 7),
        tiri::src::shared::arguments::ARGS_PARSE_STRING
    );
    assert!(draw_callback.is_some());
    unsafe {
        assert!(copy_mode.get_screen.unwrap()(refbox::Weak::new()).is_null());
    }
}
