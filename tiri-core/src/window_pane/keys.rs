//! Encode pane process input without lending pane storage to consumers.
use super::WindowPane;
use crate::input_keys::{input_key, input_key_get_mouse};
use crate::key_string::key_string_format;
use crate::log::{log_cstr, log_cstr_n, log_debug, log_get_level};
use crate::reactor::BufferEvent;
use crate::shared::key::*;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::screen::ALL_MOUSE_MODES;

use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn write_key(
    pane: &Rc<RefCell<window_pane>>,
    key: key_code,
    mouse: Option<&mouse_event>,
) -> i32 {
    let id = pane.id();
    if log_get_level() != 0 {
        let key_string = key_string_format(key, true);
        log_debug(format_args!(
            "writing key 0x{:x} ({}) to %{}",
            key,
            log_cstr(&key_string),
            id,
        ));
    }
    if key & KEYC_MASK_KEY == KEYC_MOUSE as key_code
        || key & KEYC_MASK_TYPE >= (KEYC_TYPE_MOUSEMOVE as key_code) << 32
            && key & KEYC_MASK_TYPE <= (KEYC_TYPE_TRIPLECLICK as key_code) << 32
    {
        if let Some(mouse) = mouse.filter(|mouse| mouse.wp != -1 && mouse.wp as u32 == id) {
            write_mouse(pane, mouse);
        }
        return 0;
    }
    let screen = pane.screen_mode(true).expect("displayed pane screen");
    let stream = pane.borrow().event.clone();
    stream
        .with_stream(|event| input_key(screen, event, key))
        .unwrap_or(0)
}

fn write_mouse(pane: &Rc<RefCell<window_pane>>, mouse: &mouse_event) {
    if mouse.ignore != 0 || !pane.is_visible() {
        return;
    }
    let screen = pane.screen_mode(true).expect("displayed pane screen");
    let stream = pane.borrow().event.clone();
    if screen.mode & ALL_MOUSE_MODES == 0 {
        return;
    }
    let Some((x, y)) = pane.mouse_position(mouse, false) else {
        return;
    };
    let Some(bytes) = input_key_get_mouse(screen, mouse, x, y) else {
        return;
    };
    log_debug(format_args!(
        "writing mouse {} to %{}",
        log_cstr_n(&bytes),
        pane.id(),
    ));
    log_debug(format_args!("input_key_mouse: {}", log_cstr_n(&bytes),));
    let _ = stream.write(&bytes);
}
