use crate::arguments::{
    args_create, args_has, args_parse, args_set_flag, args_set_string, args_string,
};
use crate::cmd::cmd_get_args;
use crate::shared::abi::*;
use crate::shared::command::*;
use crate::shared::command::{cmd, cmd_entry, cmd_entry_flag, cmdq_item};
use crate::window_pane::WindowPane as _;
use crate::window_tree::window_tree_mode;
use std::ffi::{CStr, CString};
pub static cmd_find_window_entry: cmd_entry = {
    cmd_entry {
        name: c"find-window",
        alias: Some(c"findw"),
        args: args_parse {
            template: c"CiNrt:TZ",
            lower: 1 as ::core::ffi::c_int,
            upper: 1 as ::core::ffi::c_int,
            cb: None,
        },
        usage: c"[-CiNrTZ] [-t target-pane] match-string",
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
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::RefCell<cmdq_item>>,
) -> CmdOutcome {
    let item = item_handle.borrow();
    let args = cmd_get_args(self_0.get_unchecked());
    let target = crate::cmd::queue::cmdq_get_target(&item);
    let wp = target.pane_handle();
    let mut s: &CStr = args_string(args, 0 as u_int).expect("argument is present");
    let mut suffix: &CStr = c"";
    let mut star: &CStr = c"*";

    let mut C: ::core::ffi::c_int = 0;
    let mut N: ::core::ffi::c_int = 0;
    let mut T: ::core::ffi::c_int = 0;
    C = args_has(args, 'C' as i32 as u_char);
    N = args_has(args, 'N' as i32 as u_char);
    T = args_has(args, 'T' as i32 as u_char);
    if args_has(args, 'r' as i32 as u_char) != 0 {
        star = c"";
    }
    if args_has(args, 'r' as i32 as u_char) != 0 && args_has(args, 'i' as i32 as u_char) != 0 {
        suffix = c"/ri";
    } else if args_has(args, 'r' as i32 as u_char) != 0 {
        suffix = c"/r";
    } else if args_has(args, 'i' as i32 as u_char) != 0 {
        suffix = c"/i";
    }
    if C == 0 && N == 0 && T == 0 {
        T = 1 as ::core::ffi::c_int;
        N = T;
        C = N;
    }
    let filter_value: CString = find_window_filter(s, suffix, star, C != 0, N != 0, T != 0);
    let mut new_args = args_create();
    if args_has(args, b'Z') != 0 {
        args_set_flag(&mut new_args, b'Z', false);
    }
    args_set_string(&mut new_args, b'f', filter_value);
    wp.as_ref().expect("mode target pane").set_mode(
        None,
        &window_tree_mode,
        Some(item_handle),
        Some(target),
        Some(&new_args),
    );
    drop(new_args);
    CmdOutcome::Normal
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
