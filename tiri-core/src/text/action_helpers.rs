use crate::arguments::args_escape_cstring;
use crate::format::bytes::cstr_copy_into;
use crate::log::{fatalx, log_bytes, log_debug};
use crate::session::Session as _;
use crate::shared::abi::*;
use crate::shared::mouse::mouse_event;
use crate::shared::pane::window_pane;
use crate::shared::session::SessionRef;
use crate::shared::window::winlink;
use crate::window::winlink_find_by_window;
use crate::window::Window as _;
use crate::window_pane::WindowPane as _;
use crate::winlink::winlink_window;
use std::ffi::{CStr, CString};
const DQ: u32 = 2;
const SQ: u32 = 1;
const NQ: u32 = 0;
pub fn log_argv(argv: &[CString], prefix: &CStr) {
    for (i, arg) in argv.iter().enumerate() {
        log_debug(format_args!(
            "{}: argv[{}]={}",
            log_bytes(prefix.to_bytes()),
            i as ::core::ffi::c_int,
            log_bytes(arg.as_bytes())
        ));
    }
}
pub fn append_argv(argv: &mut Vec<CString>, arg: &CStr) {
    argv.push(arg.to_owned());
}
pub fn pack_argv(argv: &[CString], mut buf: &mut [u8]) -> ::core::ffi::c_int {
    if argv.is_empty() {
        return 0 as ::core::ffi::c_int;
    }
    log_argv(argv, c"cmd_pack_argv");
    buf[0] = 0;
    for arg in argv {
        if cstr_copy_into(buf, arg) >= buf.len() {
            return -(1 as ::core::ffi::c_int);
        }
        buf = &mut buf[arg.as_bytes_with_nul().len()..];
    }
    0 as ::core::ffi::c_int
}
pub fn stringify_argv(argv: &[CString]) -> Option<CString> {
    let mut bytes = Vec::new();
    for (i, argument) in argv.iter().enumerate() {
        let escaped = args_escape_cstring(argument);
        log_debug(format_args!(
            "{}: {} {} = {}",
            "cmd_stringify_argv",
            (i) as u32,
            log_bytes(argument.as_bytes()),
            log_bytes(escaped.as_bytes())
        ));
        if i != 0 {
            bytes.push(b' ');
        }
        bytes.extend_from_slice(escaped.as_bytes());
    }
    Some(CString::new(bytes).expect("escaped argv contains no interior NUL"))
}
pub fn replace_template(template: &CStr, s: &CStr, idx: ::core::ffi::c_int) -> CString {
    let template_bytes = template.to_bytes();
    if !template_bytes.contains(&b'%') {
        return template.to_owned();
    }

    let mut output = Vec::new();
    let mut replaced = false;
    let mut offset = 0;
    while offset < template_bytes.len() {
        let ch = template_bytes[offset];
        offset += 1;
        if ch == b'%' {
            let next = template_bytes.get(offset).copied().unwrap_or(0);
            let quote = if (b'1'..=b'9').contains(&next) && (next - b'0') as i32 == idx {
                offset += 1;
                if template_bytes.get(offset) == Some(&b'%') {
                    offset += 1;
                    Some(DQ)
                } else {
                    Some(NQ)
                }
            } else if next == b'%' && !replaced {
                replaced = true;
                offset += 1;
                if template_bytes.get(offset) == Some(&b'%') {
                    offset += 1;
                    Some(DQ)
                } else {
                    Some(SQ)
                }
            } else {
                None
            };
            if let Some(quote) = quote {
                let replacement = s.to_bytes();
                if replacement.len() >= usize::MAX / 4
                    || output.len() > usize::MAX - replacement.len() * 4 - 1
                {
                    fatalx(|out| out.write_all(b"argument too long"));
                }
                output.reserve(replacement.len() * 4);
                for &byte in replacement {
                    if quote == SQ && byte == b'\'' {
                        output.extend_from_slice(b"'\\''");
                    } else {
                        if quote == DQ && b"\"\\$;~".contains(&byte) {
                            output.push(b'\\');
                        }
                        output.push(byte);
                    }
                }
                continue;
            }
        }
        if output.len() > usize::MAX - 2 {
            fatalx(|out| out.write_all(b"argument too long"));
        }
        output.push(ch);
    }
    let text = CString::new(output).expect("C string template contains no embedded NUL");
    log_debug(format_args!(
        "{}: {} -> {}",
        "cmd_template_replace",
        log_bytes(template.to_bytes()),
        log_bytes(text.as_bytes())
    ));
    text
}
pub fn mouse_at(
    pane: &std::rc::Rc<std::cell::RefCell<window_pane>>,
    mouse: &mouse_event,
    x: &mut u_int,
    y: &mut u_int,
    last: i32,
) -> i32 {
    let Some((mx, my)) = pane.mouse_position(mouse, last != 0) else {
        return -1;
    };
    *x = mx;
    *y = my;
    0
}
pub fn mouse_window(m: &mouse_event, sp: Option<&mut Option<SessionRef>>) -> refbox::Weak<winlink> {
    let mut s: Option<SessionRef> = None;
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if m.valid == 0 {
        return refbox::Weak::new();
    }
    if m.s == -1 {
        return refbox::Weak::new();
    }
    let Some(session_owner) = crate::session::sessions::find_by_id(m.s as u_int) else {
        return refbox::Weak::new();
    };
    s = Some(session_owner.clone());
    if m.w == -(1 as ::core::ffi::c_int) {
        wl = s.as_ref().expect("live session").current_winlink();
    } else {
        let Some(window) = crate::window::windows::resolve(m.w as u_int) else {
            return refbox::Weak::new();
        };
        wl = s
            .as_ref()
            .expect("live session")
            .with_winlinks(|links| winlink_find_by_window(links, &window));
        drop(window);
    }
    if let Some(sp) = sp {
        *sp = Some(session_owner);
    }
    wl
}
pub fn mouse_pane(
    m: &mouse_event,
    sp: Option<&mut Option<SessionRef>>,
    wlp: Option<&mut refbox::Weak<winlink>>,
) -> Option<std::rc::Rc<std::cell::RefCell<window_pane>>> {
    let link = mouse_window(m, sp);
    if !link.is_alive() {
        return None;
    }
    let window = winlink_window(&link).expect("linked window");
    let pane = if m.wp == -1 {
        window.active_pane().upgrade()
    } else {
        let pane = std::rc::Rc::<std::cell::RefCell<window_pane>>::find_by_id(m.wp as u_int)?;
        if !window.contains_pane(&std::rc::Rc::downgrade(&pane)) {
            return None;
        }
        Some(pane)
    };
    if let Some(wlp) = wlp {
        *wlp = link;
    }
    pane
}
