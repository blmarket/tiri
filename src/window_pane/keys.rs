//! Encode pane process input without lending pane storage to consumers.
use super::WindowPane;
use crate::src::input_keys::{input_key, input_key_get_mouse};
use crate::src::key_string::key_string_format;
use crate::src::log::{log_cstr, log_cstr_n, log_debug, log_get_level};
use crate::src::reactor::BufferEvent;
use crate::src::shared::key::*;
use crate::src::shared::mouse::mouse_event;
use crate::src::shared::pane::window_pane;
use crate::src::shared::screen::ALL_MOUSE_MODES;

use std::cell::UnsafeCell;
use std::rc::Rc;

pub(super) unsafe fn write_key(
    pane: &Rc<UnsafeCell<window_pane>>,
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
    let (screen, stream) = {
        let state = &mut *pane.get();
        (state.screen_ptr(), state.event.clone())
    };
    stream
        .with_ptr(|event| input_key(screen, event, key))
        .unwrap_or(0)
}

unsafe fn write_mouse(pane: &Rc<UnsafeCell<window_pane>>, mouse: &mouse_event) {
    if mouse.ignore != 0 {
        return;
    }
    let (screen, stream) = {
        let state = &mut *pane.get();
        (state.screen_ptr(), state.event.clone())
    };
    if (*screen).mode & ALL_MOUSE_MODES == 0 {
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
