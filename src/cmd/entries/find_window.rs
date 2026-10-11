use crate::src::arguments::{args_create, args_has, args_set_owned_string, args_string};
use crate::src::cmd::cmd_get_args_mut;
use crate::src::shared::abi::*;
use crate::src::shared::arguments::{args, args_parse};
use crate::src::shared::command::*;
use crate::src::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmd_find_state, cmdq_item};
use crate::src::window_pane::WindowPane as _;
use crate::src::window_tree::window_tree_mode;
use std::ffi::{CStr, CString};
pub static cmd_find_window_entry: cmd_entry = {
    cmd_entry {
        name: c"find-window",
        alias: Some(c"findw"),
        args: args_parse {
            template: c"CiNrt:T",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-CiNrT] [-t target-pane] match-string",
        source: cmd_entry_flag {
            flag: 0,
            type_0: CMD_FIND_PANE,
            flags: 0,
        },
        target: cmd_entry_flag {
            flag: 't' as i32 as ::core::ffi::c_char,
            type_0: CMD_FIND_PANE,
            flags: 0 as ::core::ffi::c_int,
        },
        flags: 0 as ::core::ffi::c_int,
        exec: Some(cmd_find_window_exec),
    }
};
unsafe fn cmd_find_window_exec(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
) -> cmd_retval {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let wp = (*target).pane_handle();
    let mut s: *const ::core::ffi::c_char =
        args_string(&mut *(args), 0 as u_int).map_or(std::ptr::null(), |value| value.as_ptr());
    let mut suffix: *const ::core::ffi::c_char = c"".as_ptr();
    let mut star: *const ::core::ffi::c_char = c"*".as_ptr();

    let mut C: ::core::ffi::c_int = 0;
    let mut N: ::core::ffi::c_int = 0;
    let mut T: ::core::ffi::c_int = 0;
    C = args_has(args, 'C' as i32 as u_char);
    N = args_has(args, 'N' as i32 as u_char);
    T = args_has(args, 'T' as i32 as u_char);
    if args_has(args, 'r' as i32 as u_char) != 0 {
        star = c"".as_ptr();
    }
    if args_has(args, 'r' as i32 as u_char) != 0 && args_has(args, 'i' as i32 as u_char) != 0 {
        suffix = c"/ri".as_ptr();
    } else if args_has(args, 'r' as i32 as u_char) != 0 {
        suffix = c"/r".as_ptr();
    } else if args_has(args, 'i' as i32 as u_char) != 0 {
        suffix = c"/i".as_ptr();
    }
    if C == 0 && N == 0 && T == 0 {
        T = 1 as ::core::ffi::c_int;
        N = T;
        C = N;
    }
    let filter_value: CString = find_window_filter(
        CStr::from_ptr(s),
        CStr::from_ptr(suffix),
        CStr::from_ptr(star),
        C != 0,
        N != 0,
        T != 0,
    );
    let mut new_args = args_create();
    args_set_owned_string(&mut *new_args, filter_value);
    wp.as_ref().expect("mode target pane").set_mode(
        None,
        &window_tree_mode,
        Some(item_handle),
        Some(&mut *target),
        Some(&mut *new_args),
    );
    drop(new_args);
    CMD_RETURN_NORMAL
}

fn append_find_window_c_match(out: &mut Vec<u8>, suffix: &[u8], pattern: &[u8]) {
    out.extend_from_slice(b"#{C");
    out.extend_from_slice(suffix);
    out.push(b':');
    out.extend_from_slice(pattern);
    out.push(b'}');
}

fn append_find_window_name_match(
    out: &mut Vec<u8>,
    suffix: &[u8],
    star: &[u8],
    pattern: &[u8],
    field: &[u8],
) {
    out.extend_from_slice(b"#{m");
    out.extend_from_slice(suffix);
    out.push(b':');
    out.extend_from_slice(star);
    out.extend_from_slice(pattern);
    out.extend_from_slice(star);
    out.extend_from_slice(b",#{");
    out.extend_from_slice(field);
    out.extend_from_slice(b"}}");
}

fn find_window_filter(
    pattern: &CStr,
    suffix: &CStr,
    star: &CStr,
    compare: bool,
    name: bool,
    title: bool,
) -> CString {
    let pattern = pattern.to_bytes();
    let suffix = suffix.to_bytes();
    let star = star.to_bytes();
    let mut out = Vec::new();
    if compare && name && title {
        out.extend_from_slice(b"#{||:");
        append_find_window_c_match(&mut out, suffix, pattern);
        out.extend_from_slice(b",#{||:");
        append_find_window_name_match(&mut out, suffix, star, pattern, b"window_name");
        out.push(b',');
        append_find_window_name_match(&mut out, suffix, star, pattern, b"pane_title");
        out.extend_from_slice(b"}}");
    } else if compare && name {
        out.extend_from_slice(b"#{||:");
        append_find_window_c_match(&mut out, suffix, pattern);
        out.push(b',');
        append_find_window_name_match(&mut out, suffix, star, pattern, b"window_name");
        out.push(b'}');
    } else if compare && title {
        out.extend_from_slice(b"#{||:");
        append_find_window_c_match(&mut out, suffix, pattern);
        out.push(b',');
        append_find_window_name_match(&mut out, suffix, star, pattern, b"pane_title");
        out.push(b'}');
    } else if name && title {
        out.extend_from_slice(b"#{||:");
        append_find_window_name_match(&mut out, suffix, star, pattern, b"window_name");
        out.push(b',');
        append_find_window_name_match(&mut out, suffix, star, pattern, b"pane_title");
        out.push(b'}');
    } else if compare {
        append_find_window_c_match(&mut out, suffix, pattern);
    } else if name {
        append_find_window_name_match(&mut out, suffix, star, pattern, b"window_name");
    } else {
        append_find_window_name_match(&mut out, suffix, star, pattern, b"pane_title");
    }
    CString::new(out).expect("find-window filter fragments contain no NUL bytes")
}

#[cfg(test)]
mod tests {
    use super::find_window_filter;
    

    #[test]
    fn find_window_filter_preserves_all_flag_combinations_and_bytes() {
        let pattern = c"a\xff";
        let suffix = c"/ri";
        let star = c"*";
        let cases: &[(bool, bool, bool, &[u8])] = &[
            (
                true,
                true,
                true,
                b"#{||:#{C/ri:a\xff},#{||:#{m/ri:*a\xff*,#{window_name}},#{m/ri:*a\xff*,#{pane_title}}}}",
            ),
            (
                true,
                true,
                false,
                b"#{||:#{C/ri:a\xff},#{m/ri:*a\xff*,#{window_name}}}",
            ),
            (
                true,
                false,
                true,
                b"#{||:#{C/ri:a\xff},#{m/ri:*a\xff*,#{pane_title}}}",
            ),
            (
                false,
                true,
                true,
                b"#{||:#{m/ri:*a\xff*,#{window_name}},#{m/ri:*a\xff*,#{pane_title}}}",
            ),
            (true, false, false, b"#{C/ri:a\xff}"),
            (false, true, false, b"#{m/ri:*a\xff*,#{window_name}}"),
            (false, false, true, b"#{m/ri:*a\xff*,#{pane_title}}"),
        ];

        for &(compare, name, title, expected) in cases {
            let filter = find_window_filter(pattern, suffix, star, compare, name, title);
            assert_eq!(filter.as_bytes(), expected);
        }
    }
}
