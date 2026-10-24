//! Bounded calls into ncurses.

use std::ffi::{c_char, c_int, CStr, CString};

#[repr(C)]
pub struct TERMINAL {
    _opaque: [u8; 0],
}

extern "C" {
    static mut cur_term: *mut TERMINAL;
    fn del_curterm(_: *mut TERMINAL) -> c_int;
    fn setupterm(_: *const c_char, _: c_int, _: *mut c_int) -> c_int;
    fn tigetflag(_: *const c_char) -> c_int;
    fn tigetnum(_: *const c_char) -> c_int;
    fn tigetstr(_: *const c_char) -> *mut c_char;
    fn tiparm_s(_: c_int, _: c_int, _: *const c_char, ...) -> *mut c_char;
}

/// A string capability of the loaded terminfo entry, copied out.
pub fn string(capability: &CStr) -> Option<CString> {
    let value = unsafe { tigetstr(capability.as_ptr()) };
    if value.is_null() || value as isize == -1 {
        return None;
    }
    Some(unsafe { CStr::from_ptr(value) }.to_owned())
}

/// A numeric capability of the loaded terminfo entry.
pub fn number(capability: &CStr) -> Option<c_int> {
    match unsafe { tigetnum(capability.as_ptr()) } {
        -1 | -2 => None,
        value => Some(value),
    }
}

/// A boolean capability of the loaded terminfo entry.
pub fn flag(capability: &CStr) -> Option<bool> {
    match unsafe { tigetflag(capability.as_ptr()) } {
        -1 => None,
        value => Some(value != 0),
    }
}

/// Load the terminfo entry for `name` on `fd` (`setupterm`), run `visit`
/// while [`string`], [`number`] and [`flag`] read it, then release it. On
/// failure, return setupterm's error code.
pub fn with_terminfo<R>(name: &CStr, fd: c_int, visit: impl FnOnce() -> R) -> Result<R, c_int> {
    let mut error = 0;
    if unsafe { setupterm(name.as_ptr(), fd, &mut error) } != 0 {
        return Err(error);
    }
    let terminal = unsafe { cur_term };
    let result = visit();
    unsafe { del_curterm(terminal) };
    Ok(result)
}

/// Release the current terminal, if any.
pub fn release_current_terminal() {
    unsafe { del_curterm(cur_term) };
}

/// Expand `capability` with integer parameters; tiparm_s checks the
/// capability against the parameter count, and its static result is copied.
pub fn expand_numbers(capability: &CStr, parameters: &[c_int]) -> Option<CString> {
    let x = capability.as_ptr();
    let result = unsafe {
        match *parameters {
            [a] => tiparm_s(1, 0, x, a),
            [a, b] => tiparm_s(2, 0, x, a, b),
            [a, b, c] => tiparm_s(3, 0, x, a, b, c),
            _ => panic!("unsupported terminfo parameter count"),
        }
    };
    (!result.is_null()).then(|| unsafe { CStr::from_ptr(result) }.to_owned())
}

/// Expand `capability` with string parameters, as [`expand_numbers`] does.
pub fn expand_strings(capability: &CStr, parameters: &[&CStr]) -> Option<CString> {
    let x = capability.as_ptr();
    let result = unsafe {
        match *parameters {
            [a] => tiparm_s(1, 1, x, a.as_ptr()),
            [a, b] => tiparm_s(2, 3, x, a.as_ptr(), b.as_ptr()),
            _ => panic!("unsupported terminfo parameter count"),
        }
    };
    (!result.is_null()).then(|| unsafe { CStr::from_ptr(result) }.to_owned())
}
