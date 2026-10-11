use std::ffi::CStr;

use hmux::src::cmd::{cmd_copy, cmd_parse};
use hmux::src::shared::arguments::ArgumentValue;

unsafe fn parse_display_message(file: Option<&CStr>) -> refbox::RefBox<hmux::src::cmd::cmd> {
    let command_name = c"display-message";
    let value = ArgumentValue::borrowed_string(command_name);
    cmd_parse(std::slice::from_ref(&value), file, 37, 0).expect("command parse reported an error")
}

#[test]
fn filename_survives_input_mutation_and_original_command_free() {
    unsafe {
        let mut filename = *b"source-\xff.conf\0";
        let command = parse_display_message(Some(CStr::from_bytes_with_nul(&filename).unwrap()));
        let copied = cmd_copy(&command.try_borrow_mut().unwrap(), &Vec::new());

        assert_ne!(
            command
                .try_borrow_mut()
                .unwrap()
                .file
                .as_ref()
                .unwrap()
                .as_ptr(),
            filename.as_ptr().cast()
        );
        assert_ne!(
            copied
                .try_borrow_mut()
                .unwrap()
                .file
                .as_ref()
                .unwrap()
                .as_ptr(),
            command
                .try_borrow_mut()
                .unwrap()
                .file
                .as_ref()
                .unwrap()
                .as_ptr()
        );

        // The parser must retain its own bytes, including non-UTF-8 bytes.
        filename[0] = b'X';
        assert_eq!(&filename[..7], b"Xource-");
        assert_eq!(
            CStr::from_ptr(
                (command.try_borrow_mut().unwrap().file)
                    .as_ref()
                    .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            )
            .to_bytes(),
            b"source-\xff.conf"
        );
        assert_eq!(
            CStr::from_ptr(
                (copied.try_borrow_mut().unwrap().file)
                    .as_ref()
                    .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            )
            .to_bytes(),
            b"source-\xff.conf"
        );
        assert_eq!(copied.try_borrow_mut().unwrap().line, 37);

        drop(command);
        assert_eq!(
            CStr::from_ptr(
                (copied.try_borrow_mut().unwrap().file)
                    .as_ref()
                    .map_or(::core::ptr::null_mut(), |value| value.as_ptr().cast_mut())
            )
            .to_bytes(),
            b"source-\xff.conf"
        );
        drop(copied);
    }
}

#[test]
fn absent_filename_stays_null_in_copy() {
    unsafe {
        let command = parse_display_message(None);
        let copied = cmd_copy(&command.try_borrow_mut().unwrap(), &Vec::new());
        assert!(command.try_borrow_mut().unwrap().file.is_none());
        assert!(copied.try_borrow_mut().unwrap().file.is_none());
        drop(command);
        drop(copied);
    }
}
