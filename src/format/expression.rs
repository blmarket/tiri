use super::bytes::format_cstring;
use super::*;
use crate::src::format::bytes::nullable_cstr;
use crate::src::format::bytes::write_cstr;
use crate::src::format::bytes::xformat;
use crate::src::options::options_is_array;
use crate::src::options::options_parse_owned;
use crate::src::options::options_to_cstring;
use crate::src::server_client::Client;
use crate::src::session::Session;
use crate::src::shared::client::ClientRef;
use crate::src::shared::session::SessionRef;
use crate::src::window::{Window, WindowPane};
use std::collections::btree_map;
use std::ffi::{CStr, CString};

pub(super) unsafe fn format_strftime(
    mut s: *mut ::core::ffi::c_char,
    mut max: size_t,
    mut fmt: *const ::core::ffi::c_char,
    mut tm: *const tm,
) -> size_t {
    strftime(s, max, fmt, tm)
}
pub(super) fn format_quote_shell(s: &CStr) -> CString {
    let input = s.to_bytes();
    let mut quoted = Vec::with_capacity(input.len().saturating_mul(2));
    const SHELL_SPECIAL: &[u8] = b"|&;<>(){}$`\\\"'*?[# =%\n\t";
    for &byte in input {
        if SHELL_SPECIAL.contains(&byte) {
            quoted.push(b'\\');
        }
        quoted.push(byte);
    }
    CString::new(quoted).expect("shell-quoted C string contains no NUL")
}
pub(crate) fn format_quote_shell_single(s: &CStr) -> CString {
    let input = s.to_bytes();
    let mut quoted = Vec::with_capacity(input.len().saturating_mul(4).saturating_add(2));
    quoted.push(b'\'');
    for &byte in input {
        if byte == b'\'' {
            quoted.extend_from_slice(b"'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    CString::new(quoted).expect("shell-quoted C string contains no NUL")
}
pub(super) fn format_quote_style(s: &CStr) -> CString {
    let input = s.to_bytes();
    let mut quoted = Vec::with_capacity(input.len().saturating_mul(2));
    for &byte in input {
        if byte == b'#' {
            quoted.push(b'#');
        }
        quoted.push(byte);
    }
    CString::new(quoted).expect("style-quoted C string contains no NUL")
}
pub(crate) unsafe fn format_pretty_time_cstring(mut t: time_t) -> CString {
    let mut now_tm: tm = unsafe { ::core::mem::zeroed() };
    let mut tm: tm = unsafe { ::core::mem::zeroed() };
    let mut now: time_t = 0;
    let mut age: time_t = 0;
    let mut s: [::core::ffi::c_char; 9] = [0; 9];
    time(&raw mut now);
    if now < t {
        now = t;
    }
    age = now - t;
    if localtime_r(&raw mut now, &raw mut now_tm).is_null()
        || localtime_r(&raw mut t, &raw mut tm).is_null()
    {
        return CString::default();
    }
    if age < (24 as ::core::ffi::c_int * 3600 as ::core::ffi::c_int) as time_t {
        strftime(
            &raw mut s as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 9]>() as size_t,
            c"%H:%M".as_ptr(),
            &raw mut tm,
        );

        return CStr::from_ptr(s.as_ptr()).to_owned();
    }
    if tm.tm_year == now_tm.tm_year && tm.tm_mon == now_tm.tm_mon
        || age
            < (28 as ::core::ffi::c_int * 24 as ::core::ffi::c_int * 3600 as ::core::ffi::c_int)
                as time_t
    {
        strftime(
            &raw mut s as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 9]>() as size_t,
            c"%a%d".as_ptr(),
            &raw mut tm,
        );
        return CStr::from_ptr(s.as_ptr()).to_owned();
    }
    if tm.tm_year == now_tm.tm_year && tm.tm_mon < now_tm.tm_mon
        || tm.tm_year == now_tm.tm_year - 1 as ::core::ffi::c_int && tm.tm_mon > now_tm.tm_mon
    {
        strftime(
            &raw mut s as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 9]>() as size_t,
            c"%d%b".as_ptr(),
            &raw mut tm,
        );
        return CStr::from_ptr(s.as_ptr()).to_owned();
    }
    strftime(
        &raw mut s as *mut ::core::ffi::c_char,
        ::core::mem::size_of::<[::core::ffi::c_char; 9]>() as size_t,
        c"%h%y".as_ptr(),
        &raw mut tm,
    );
    CStr::from_ptr(s.as_ptr()).to_owned()
}
pub(super) unsafe fn format_relative_time(mut t: time_t) -> Option<CString> {
    let mut now: time_t = 0;
    let mut age: time_t = 0;
    let mut d: u_int = 0;
    let mut h: u_int = 0;
    let mut m: u_int = 0;
    let mut s: u_int = 0;
    let mut out: [u8; 32] = [0; 32];
    time(&raw mut now);
    if t > now {
        return None;
    }
    if t == now {
        return Some(c"0s".to_owned());
    }
    age = now - t;
    d = (age / 86400 as time_t) as u_int;
    h = (age % 86400 as time_t / 3600 as time_t) as u_int;
    m = (age % 3600 as time_t / 60 as time_t) as u_int;
    s = (age % 60 as time_t) as u_int;
    if d != 0 as u_int {
        if h != 0 as u_int {
            xformat(&mut out, format_args!("{}d{}h", d as u32, h as u32));
        } else {
            xformat(&mut out, format_args!("{}d", d as u32));
        }
    } else if h != 0 as u_int {
        if m != 0 as u_int {
            xformat(&mut out, format_args!("{}h{}m", h as u32, m as u32));
        } else {
            xformat(&mut out, format_args!("{}h", h as u32));
        }
    } else if m != 0 as u_int {
        if s != 0 as u_int {
            xformat(&mut out, format_args!("{}m{}s", m as u32, s as u32));
        } else {
            xformat(&mut out, format_args!("{}m", m as u32));
        }
    } else {
        xformat(&mut out, format_args!("{}s", s as u32));
    }
    Some(
        CStr::from_bytes_until_nul(&out)
            .expect("xformat output is terminated")
            .to_owned(),
    )
}
unsafe fn format_time_difference(t: time_t) -> CString {
    CString::new((time(std::ptr::null_mut()) - t).to_string())
        .expect("time difference contains no NUL")
}

pub(super) unsafe fn format_find(
    mut ft: *mut format_tree,
    mut key: *const ::core::ffi::c_char,
    mut modifiers: uint64_t,
    mut time_format: *const ::core::ffi::c_char,
) -> Option<CString> {
    let format_pane_owner = (*ft).wp.upgrade();
    let format_window_owner = (*ft).w.upgrade();
    let format_session_owner = (*ft).s.upgrade();
    let result = (|| {
        let mut current_block: u64;
        let mut found: Option<CString> = None;
        let mut s: [::core::ffi::c_char; 512] = [0; 512];
        let mut t: time_t = 0 as time_t;
        let mut tm: tm = unsafe { ::core::mem::zeroed() };
        let parsed_option = options_parse_owned(CStr::from_ptr(key));
        if let Some(parsed) = &parsed_option {
            let name = parsed.name.as_ptr();
            let array_key = parsed
                .array_key
                .as_ref()
                .map_or(std::ptr::null(), |key| key.as_ptr());
            let lookup = |options: &mut options| {
                crate::src::options::options_read_entry(options, CStr::from_ptr(name), |entry| {
                    options_to_cstring(entry, nullable_cstr(array_key), 1)
                })
            };
            found = lookup(global_options());
            if found.is_none() {
                found = format_pane_owner
                    .as_ref()
                    .and_then(|pane| lookup(pane.borrow_options_mut()));
            }
            if found.is_none() {
                found = format_window_owner
                    .as_ref()
                    .and_then(|window| lookup(window.borrow_options_mut()));
            }
            if found.is_none() {
                found = lookup(global_w_options());
            }
            if found.is_none() {
                found = format_session_owner
                    .as_ref()
                    .and_then(|session| lookup(session.borrow_options_mut()));
            }
            if found.is_none() {
                found = lookup(global_s_options());
            }
        }
        if found.is_none() {
            if let Some(entry) = format_table_get(CStr::from_ptr(key)) {
                match entry.get(ft) {
                    Some(FormatValue::String(value)) => found = Some(value),
                    Some(FormatValue::Time(value)) => t = value,
                    None => {}
                }
            } else {
                let entry_key = CStr::from_ptr(key).to_owned();
                if format_entry_tree_find(&(*ft).tree, &entry_key).is_some() {
                    match format_entry_get_value(ft, &entry_key) {
                        Some(FormatValue::String(value)) => found = Some(value),
                        Some(FormatValue::Time(value)) => t = value,
                        None => {}
                    }
                } else if let Some(value) =
                    crate::src::plugin::find(format_plugin_pane(ft), &entry_key)
                {
                    found = Some(value);
                } else {
                    if !modifiers & FORMAT_TIMESTRING as uint64_t != 0 {
                        let entry = format_session_owner
                            .as_ref()
                            .and_then(|session| {
                                session.with_environment_mut(|environment| {
                                    environ_find(environment, CStr::from_ptr(key)).cloned()
                                })
                            })
                            .or_else(|| {
                                environ_find(
                                    global_environ.as_deref().expect("environment"),
                                    CStr::from_ptr(key),
                                )
                                .cloned()
                            });
                        if let Some(value) = entry.and_then(|entry| entry.value) {
                            found = Some(value);
                            current_block = 11739001764845178280;
                        } else {
                            current_block = 1836292691772056875;
                        }
                    } else {
                        current_block = 1836292691772056875;
                    }
                    match current_block {
                        11739001764845178280 => {}
                        _ => return None,
                    }
                }
            }
        }
        if modifiers & FORMAT_TIMESTRING as uint64_t != 0 {
            if t == 0 as time_t && found.is_some() {
                let parsed = strtonum(
                    found.as_deref().unwrap(),
                    0 as ::core::ffi::c_longlong,
                    INT64_MAX as ::core::ffi::c_longlong,
                );
                t = parsed.unwrap_or(0) as time_t;
                if parsed.is_err() {
                    t = 0 as time_t;
                }
                found = None;
            }
            if t == 0 as time_t {
                return None;
            }
            if modifiers & FORMAT_RELATIVE as uint64_t != 0 {
                found = format_relative_time(t);
            } else if modifiers as ::core::ffi::c_ulonglong & FORMAT_DIFFERENCE != 0 {
                found = Some(format_time_difference(t));
            } else if modifiers & FORMAT_PRETTY as uint64_t != 0 {
                found = Some(format_pretty_time_cstring(t));
            } else {
                if !time_format.is_null() {
                    if localtime_r(&raw mut t, &raw mut tm).is_null() {
                        return None;
                    }
                    format_strftime(
                        &raw mut s as *mut ::core::ffi::c_char,
                        ::core::mem::size_of::<[::core::ffi::c_char; 512]>() as size_t,
                        time_format,
                        &raw mut tm,
                    );
                } else {
                    if ctime_r(&raw mut t, &raw mut s as *mut ::core::ffi::c_char).is_null() {
                        return None;
                    }
                    s[strcspn(&raw mut s as *mut ::core::ffi::c_char, c"\n".as_ptr()) as usize] =
                        '\0' as i32 as ::core::ffi::c_char;
                }
                found = Some(CStr::from_ptr(s.as_ptr()).to_owned());
            }
            return found;
        }
        let mut found = if t != 0 {
            CString::new(t.to_string()).expect("timestamp contains no NUL")
        } else {
            found?
        };
        if modifiers & FORMAT_BASENAME as uint64_t != 0 {
            let mut scratch = found.into_bytes_with_nul();
            found = CStr::from_ptr(basename(scratch.as_mut_ptr().cast())).to_owned();
        }
        if modifiers & FORMAT_DIRNAME as uint64_t != 0 {
            let mut scratch = found.into_bytes_with_nul();
            found = CStr::from_ptr(dirname(scratch.as_mut_ptr().cast())).to_owned();
        }
        if modifiers & FORMAT_QUOTE_SHELL as uint64_t != 0 {
            found = format_quote_shell(found.as_c_str());
        }
        if modifiers & FORMAT_QUOTE_SHELL_SQ as uint64_t != 0 {
            found = format_quote_shell_single(found.as_c_str());
        }
        if modifiers & FORMAT_QUOTE_STYLE as uint64_t != 0 {
            found = format_quote_style(found.as_c_str());
        }
        if modifiers & FORMAT_QUOTE_ARGUMENTS as uint64_t != 0 {
            found = args_escape_cstring(found.as_c_str());
        }
        Some(found)
    })();
    if let Some(window) = format_window_owner {
        drop(window);
    }
    result
}

pub(super) unsafe fn format_check_time(
    mut es: *mut format_expand_state,
    mut check: *mut u_int,
) -> ::core::ffi::c_int {
    let mut t: uint64_t = 0;
    if !check.is_null() && {
        *check = (*check).wrapping_add(1);
        (*check).wrapping_rem(FORMAT_TIME_LOOP_CHECK as u_int) != 0 as u_int
    } {
        return 1 as ::core::ffi::c_int;
    }
    t = get_timer();
    if t.wrapping_sub((*es).start_time) < FORMAT_TIME_LIMIT as uint64_t {
        return 1 as ::core::ffi::c_int;
    }
    t = t.wrapping_sub((*es).start_time);
    format_log1(es, c"format_check_time".as_ptr(), |out| {
        write!(
            out,
            "reached time limit ({})",
            (t as ::core::ffi::c_ulonglong) as u64
        )
    });
    0 as ::core::ffi::c_int
}
unsafe fn format_unescape_cstring(
    es: *mut format_expand_state,
    mut s: *const ::core::ffi::c_char,
    n: size_t,
) -> CString {
    let end = s.add(n);
    let mut out = Vec::with_capacity(n);
    let mut brackets: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut check: u_int = 0 as u_int;
    while s != end {
        if format_check_time(es, &raw mut check) == 0 {
            return CString::default();
        }
        if *s as ::core::ffi::c_int == '#' as i32
            && s.offset(1 as ::core::ffi::c_int as isize) != end
            && *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '{' as i32
        {
            brackets += 1;
        }
        if brackets == 0 as ::core::ffi::c_int
            && *s as ::core::ffi::c_int == '#' as i32
            && s.offset(1 as ::core::ffi::c_int as isize) != end
            && !strchr(
                c",#{}:".as_ptr(),
                *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int,
            )
            .is_null()
        {
            s = s.offset(1);
            out.push(*s as u8);
        } else {
            if *s as ::core::ffi::c_int == '}' as i32 {
                brackets -= 1;
            }
            out.push(*s as u8);
        }
        s = s.offset(1);
    }
    if let Some(first_nul) = out.iter().position(|&byte| byte == 0) {
        out.truncate(first_nul);
    }
    CString::new(out).expect("unescaped C-string view has no NUL")
}

unsafe fn format_strip_cstring(es: *mut format_expand_state, s: &CStr) -> CString {
    let input = s.to_bytes();
    let mut out = Vec::with_capacity(input.len());
    let mut brackets: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut check: u_int = 0 as u_int;
    for (index, &byte) in input.iter().enumerate() {
        if format_check_time(es, &raw mut check) == 0 {
            return CString::default();
        }
        let next = input.get(index + 1).copied().unwrap_or(0);
        if byte == b'#' && next == b'{' {
            brackets += 1;
        }
        if byte == b'#' && (next == 0 || b",#{}:".contains(&next)) {
            if brackets != 0 as ::core::ffi::c_int {
                out.push(byte);
            }
        } else {
            if byte == b'}' {
                brackets -= 1;
            }
            out.push(byte);
        }
    }
    CString::new(out).expect("stripped C string contains no NUL")
}
pub(super) unsafe fn format_skip1(
    mut es: *mut format_expand_state,
    mut s: *const ::core::ffi::c_char,
    mut end: *const ::core::ffi::c_char,
) -> *const ::core::ffi::c_char {
    let mut brackets: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut check: u_int = 0 as u_int;
    if !es.is_null() && format_check_time(es, ::core::ptr::null_mut::<u_int>()) == 0 {
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    while *s as ::core::ffi::c_int != '\0' as i32 {
        if !es.is_null() && format_check_time(es, &raw mut check) == 0 {
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        if *s as ::core::ffi::c_int == '#' as i32
            && *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int == '{' as i32
        {
            brackets += 1;
        }
        if *s as ::core::ffi::c_int == '#' as i32
            && *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\0' as i32
            && !strchr(
                c",#{}:".as_ptr(),
                *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int,
            )
            .is_null()
        {
            s = s.offset(1);
        } else {
            if *s as ::core::ffi::c_int == '}' as i32 {
                brackets -= 1;
            }
            if !strchr(end, *s as ::core::ffi::c_int).is_null()
                && brackets == 0 as ::core::ffi::c_int
            {
                break;
            }
        }
        s = s.offset(1);
    }
    if *s as ::core::ffi::c_int == '\0' as i32 {
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    s
}
pub unsafe fn format_skip(mut s: *const ::core::ffi::c_char) -> *const ::core::ffi::c_char {
    let mut end: *const ::core::ffi::c_char = c"]".as_ptr();
    format_skip1(::core::ptr::null_mut::<format_expand_state>(), s, end)
}
unsafe fn format_choose(
    mut es: *mut format_expand_state,
    mut s: *const ::core::ffi::c_char,
) -> Option<(CString, CString)> {
    let cp = format_skip1(es, s, c",".as_ptr());
    if cp.is_null() {
        return None;
    }
    let split = cp.offset_from(s) as usize;
    let left0 = CString::new(&CStr::from_ptr(s).to_bytes()[..split]).unwrap();
    let right0 = CStr::from_ptr(cp.add(1)).to_owned();
    let left = format_expand1_cstring(es, left0.as_ptr());
    drop(left0);
    let right = format_expand1_cstring(es, right0.as_ptr());
    Some((left, right))
}

unsafe fn format_choose_loop(
    es: *mut format_expand_state,
    fmt: *const ::core::ffi::c_char,
) -> (CString, Option<CString>) {
    let cp = format_skip1(es, fmt, c",".as_ptr());
    if cp.is_null() {
        return (CStr::from_ptr(fmt).to_owned(), None);
    }
    let split = cp.offset_from(fmt) as usize;
    let all = CString::new(&CStr::from_ptr(fmt).to_bytes()[..split]).unwrap();
    let active = CStr::from_ptr(cp.add(1)).to_owned();
    (all, Some(active))
}
pub unsafe fn format_true(mut s: *const ::core::ffi::c_char) -> ::core::ffi::c_int {
    if !s.is_null()
        && *s as ::core::ffi::c_int != '\0' as i32
        && (*s.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '0' as i32
            || *s.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int != '\0' as i32)
    {
        return 1 as ::core::ffi::c_int;
    }
    0 as ::core::ffi::c_int
}
pub(super) unsafe fn format_is_end(mut c: ::core::ffi::c_char) -> ::core::ffi::c_int {
    (c as ::core::ffi::c_int == ';' as i32 || c as ::core::ffi::c_int == ':' as i32)
        as ::core::ffi::c_int
}
pub(super) unsafe fn format_add_modifier(
    list: &mut Vec<format_modifier>,
    c: *const ::core::ffi::c_char,
    n: size_t,
    argv: Vec<CString>,
) {
    let mut fm = format_modifier {
        modifier: [0; 3],
        size: n as u_int,
        argv,
    };
    memcpy(
        fm.modifier.as_mut_ptr() as *mut ::core::ffi::c_void,
        c as *const ::core::ffi::c_void,
        n,
    );
    list.push(fm);
}
unsafe fn format_expand_modifier_arg(
    es: *mut format_expand_state,
    value: *const ::core::ffi::c_char,
) -> CString {
    format_expand1_cstring(es, value)
}
pub(super) unsafe fn format_build_modifiers(
    mut es: *mut format_expand_state,
    s: &mut &CStr,
) -> Vec<format_modifier> {
    let mut cp = s.as_ptr();
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut list: Vec<format_modifier> = Vec::new();
    let mut c: ::core::ffi::c_char = 0;
    let mut last: [::core::ffi::c_char; 4] =
        ::core::mem::transmute::<[u8; 4], [::core::ffi::c_char; 4]>(*b"X;:\0");
    let mut argv: Vec<CString> = Vec::new();
    while *cp as ::core::ffi::c_int != '\0' as i32 && *cp as ::core::ffi::c_int != ':' as i32 {
        if *cp as ::core::ffi::c_int == ';' as i32 {
            cp = cp.offset(1);
        }
        if *cp as ::core::ffi::c_int == '\0' as i32 {
            break;
        }
        if !strchr(
            c"labdnwETSWPOVL!<>A".as_ptr(),
            *cp.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int,
        )
        .is_null()
            && format_is_end(*cp.offset(1 as ::core::ffi::c_int as isize)) != 0
        {
            format_add_modifier(&mut list, cp, 1 as size_t, Vec::new());
            cp = cp.offset(1);
        } else if (memcmp(
            c"||".as_ptr() as *const ::core::ffi::c_void,
            cp as *const ::core::ffi::c_void,
            2 as size_t,
        ) == 0 as ::core::ffi::c_int
            || memcmp(
                c"&&".as_ptr() as *const ::core::ffi::c_void,
                cp as *const ::core::ffi::c_void,
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
            || memcmp(
                c"!!".as_ptr() as *const ::core::ffi::c_void,
                cp as *const ::core::ffi::c_void,
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
            || memcmp(
                c"!=".as_ptr() as *const ::core::ffi::c_void,
                cp as *const ::core::ffi::c_void,
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
            || memcmp(
                c"==".as_ptr() as *const ::core::ffi::c_void,
                cp as *const ::core::ffi::c_void,
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
            || memcmp(
                c"<=".as_ptr() as *const ::core::ffi::c_void,
                cp as *const ::core::ffi::c_void,
                2 as size_t,
            ) == 0 as ::core::ffi::c_int
            || memcmp(
                c">=".as_ptr() as *const ::core::ffi::c_void,
                cp as *const ::core::ffi::c_void,
                2 as size_t,
            ) == 0 as ::core::ffi::c_int)
            && format_is_end(*cp.offset(2 as ::core::ffi::c_int as isize)) != 0
        {
            format_add_modifier(&mut list, cp, 2 as size_t, Vec::new());
            cp = cp.offset(2 as ::core::ffi::c_int as isize);
        } else {
            if strchr(
                c"ImCLNPSOVst=pReqWcA".as_ptr(),
                *cp.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int,
            )
            .is_null()
            {
                break;
            }
            c = *cp.offset(0 as ::core::ffi::c_int as isize);
            if format_is_end(*cp.offset(1 as ::core::ffi::c_int as isize)) != 0 {
                format_add_modifier(&mut list, cp, 1 as size_t, Vec::new());
                cp = cp.offset(1);
            } else {
                argv = Vec::new();
                if ::libc::ispunct(
                    *cp.offset(1 as ::core::ffi::c_int as isize) as u_char as ::core::ffi::c_int
                ) == 0
                    || *cp.offset(1 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
                        == '-' as i32
                {
                    end = format_skip1(
                        es,
                        cp.offset(1 as ::core::ffi::c_int as isize),
                        c":;".as_ptr(),
                    );
                    if end.is_null() {
                        break;
                    }
                    let value = format_unescape_cstring(
                        es,
                        cp.offset(1 as ::core::ffi::c_int as isize),
                        end.offset_from(cp.offset(1 as ::core::ffi::c_int as isize))
                            as ::core::ffi::c_long as size_t,
                    );
                    argv.push(format_expand_modifier_arg(es, value.as_ptr()));
                    format_add_modifier(&mut list, &raw mut c, 1 as size_t, argv);
                    cp = end;
                } else {
                    last[0 as ::core::ffi::c_int as usize] =
                        *cp.offset(1 as ::core::ffi::c_int as isize);
                    cp = cp.offset(1);
                    loop {
                        if *cp.offset(0 as ::core::ffi::c_int as isize) as ::core::ffi::c_int
                            == last[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int
                            && format_is_end(*cp.offset(1 as ::core::ffi::c_int as isize)) != 0
                        {
                            cp = cp.offset(1);
                            break;
                        } else {
                            end = format_skip1(
                                es,
                                cp.offset(1 as ::core::ffi::c_int as isize),
                                &raw mut last as *mut ::core::ffi::c_char,
                            );
                            if end.is_null() {
                                break;
                            }
                            cp = cp.offset(1);
                            let value = format_unescape_cstring(
                                es,
                                cp,
                                end.offset_from(cp) as ::core::ffi::c_long as size_t,
                            );
                            argv.push(format_expand_modifier_arg(es, value.as_ptr()));
                            cp = end;
                            if !(format_is_end(*cp.offset(0 as ::core::ffi::c_int as isize)) == 0) {
                                break;
                            }
                        }
                    }
                    format_add_modifier(&mut list, &raw mut c, 1 as size_t, argv);
                }
            }
        }
    }
    if *cp as ::core::ffi::c_int != ':' as i32 {
        drop(list);
        return Vec::new();
    }
    *s = CStr::from_bytes_with_nul_unchecked(
        &s.to_bytes_with_nul()[cp.offset_from(s.as_ptr()) as usize + 1..],
    );
    list
}
pub(super) unsafe fn format_match_fuzzy(
    pattern: &CStr,
    text: &CStr,
    mut positions: ::core::ffi::c_int,
) -> CString {
    let mut width: u_int = 0;
    width = format_width(text.as_ptr());
    if width == 0 as u_int {
        width = 1 as u_int;
    }
    let Some(bs) = fuzzy_match_owned(pattern, text, width, None) else {
        return if positions != 0 {
            CString::default()
        } else {
            c"0".to_owned()
        };
    };
    if positions == 0 {
        return c"1".to_owned();
    }
    let mut indexes = Vec::new();
    for i in 0..width {
        if bs[(i >> 3) as usize] & (1 << (i & 7)) != 0 {
            indexes.push(i.to_string());
        }
    }
    CString::new(indexes.join(",")).expect("fuzzy positions contain no NUL")
}

pub(super) unsafe fn format_match(fm: &format_modifier, pattern: &CStr, text: &CStr) -> CString {
    let options = fm.argv.first().map_or(&[][..], |s| s.as_bytes());
    if options.contains(&b'p') {
        return format_match_fuzzy(pattern, text, 1);
    }
    if options.contains(&b'z') {
        return format_match_fuzzy(pattern, text, 0);
    }
    let matched = if options.contains(&b'r') {
        let mut flags = REG_EXTENDED | REG_NOSUB;
        if options.contains(&b'i') {
            flags |= REG_ICASE;
        }
        let mut storage = RegexStorage::default();
        let Ok(regex) = storage.compile(pattern, flags) else {
            return c"0".to_owned();
        };
        regex.is_match(text)
    } else {
        let flags = if options.contains(&b'i') {
            FNM_CASEFOLD
        } else {
            0
        };
        fnmatch(pattern.as_ptr(), text.as_ptr(), flags) == 0
    };
    if matched {
        c"1".to_owned()
    } else {
        c"0".to_owned()
    }
}
pub(super) fn format_sub(
    fm: &format_modifier,
    text: &CStr,
    pattern: &CStr,
    with: &CStr,
) -> CString {
    let mut flags = REG_EXTENDED;
    if fm.argv.get(2).is_some_and(|s| s.as_bytes().contains(&b'i')) {
        flags |= REG_ICASE;
    }
    regsub_cstring(pattern, with, text, flags).unwrap_or_else(|| text.to_owned())
}

pub(super) unsafe fn format_search(
    fm: &format_modifier,
    pane: &std::rc::Rc<std::cell::UnsafeCell<window_pane>>,
    s: &CStr,
) -> CString {
    let options = fm.argv.first().map_or(&[][..], |s| s.as_bytes());
    let ignore = options.contains(&b'i');
    let regex = options.contains(&b'r');
    CString::new(pane.search(s, regex, ignore).to_string()).expect("search count contains no NUL")
}

pub(super) unsafe fn format_bool_op_1(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
    mut not: ::core::ffi::c_int,
) -> CString {
    let mut result: ::core::ffi::c_int = 0;
    let expanded = format_expand1_cstring(es, fmt);
    result = format_true(expanded.as_ptr());
    if not != 0 {
        result = (result == 0) as ::core::ffi::c_int;
    }
    if result != 0 {
        c"1".to_owned()
    } else {
        c"0".to_owned()
    }
}
pub(super) unsafe fn format_bool_op_n(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
    mut and: ::core::ffi::c_int,
) -> CString {
    let mut result: ::core::ffi::c_int = 0;
    let mut cp1: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cp2: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    result = if and != 0 {
        1 as ::core::ffi::c_int
    } else {
        0 as ::core::ffi::c_int
    };
    cp1 = fmt;
    while if and != 0 {
        result
    } else {
        (result == 0) as ::core::ffi::c_int
    } != 0
    {
        cp2 = format_skip1(es, cp1, c",".as_ptr());
        let operand = if cp2.is_null() {
            CStr::from_ptr(cp1).to_owned()
        } else {
            let length = cp2.offset_from(cp1) as usize;
            CString::new(std::slice::from_raw_parts(cp1.cast::<u8>(), length))
                .expect("format operand contains no NUL before its delimiter")
        };
        let expanded = format_expand1_cstring(es, operand.as_ptr());
        format_log1(es, c"format_bool_op_n".as_ptr(), |out| {
            out.write_all(b"operator ")?;
            write_cstr(
                out,
                nullable_cstr(if and != 0 {
                    c"&&".as_ptr()
                } else {
                    c"||".as_ptr()
                }),
            )?;
            out.write_all(b" has operand: ")?;
            write_cstr(out, &*expanded)
        });
        if and != 0 {
            result = (result != 0 && format_true(expanded.as_ptr()) != 0) as ::core::ffi::c_int;
        } else {
            result = (result != 0 || format_true(expanded.as_ptr()) != 0) as ::core::ffi::c_int;
        }
        if cp2.is_null() {
            break;
        }
        cp1 = cp2.offset(1 as ::core::ffi::c_int as isize);
    }
    if result != 0 {
        c"1".to_owned()
    } else {
        c"0".to_owned()
    }
}
pub(super) unsafe fn format_session_name(
    es: *mut format_expand_state,
    fmt: *const ::core::ffi::c_char,
) -> CString {
    let name = format_expand1_cstring(es, fmt);
    if crate::src::session::sessions::find(&name).is_some() {
        c"1".to_owned()
    } else {
        c"0".to_owned()
    }
}
pub(super) unsafe fn format_loop_sessions(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
) -> CString {
    let mut sc: *mut sort_criteria = &raw mut sort_crit;
    let mut ft: *mut format_tree = (*es).ft;
    let format_client_owner = (*ft).c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let client_owner = (*ft).client.clone();
    let item_owner = (*ft).item.upgrade();
    let mut nft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut next: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    let mut buffer = Vec::new();
    let mut i: ::core::ffi::c_int = 0;
    let (all, active) = format_choose_loop(es, fmt);
    let l = sort_get_sessions(&*sc);
    let n = ::core::ffi::c_int::try_from(l.len()).expect("too many sessions to format");
    i = 0 as ::core::ffi::c_int;
    while i < n {
        let session = &l[i as usize];
        format_log1(es, c"format_loop_sessions".as_ptr(), |out| {
            write!(out, "session loop: ${}", session.id())
        });
        let use_0 = match &active {
            Some(active_fmt)
                if format_client_owner.as_ref().is_some_and(|client| {
                    client
                        .attached_session()
                        .upgrade()
                        .is_some_and(|attached| std::rc::Rc::ptr_eq(&attached, session))
                }) =>
            {
                active_fmt.as_ptr()
            }
            _ => all.as_ptr(),
        };
        let mut nft_owner = format_create_with_client(
            client_owner.as_ref(),
            item_owner.as_ref(),
            FORMAT_NONE,
            (*ft).flags,
        );
        nft = &raw mut *nft_owner;
        format_add(nft, c"loop_index", |out| write!(out, "{}", (i) as i32));
        format_add(nft, c"loop_last_flag", |out| {
            write!(
                out,
                "{}",
                ((i == n - 1 as ::core::ffi::c_int) as ::core::ffi::c_int) as i32
            )
        });
        format_defaults(
            nft,
            format_client.as_ref(),
            Some(session),
            (refbox::Weak::new()).clone(),
            None,
        );
        format_copy_state(&raw mut next, es, 0 as ::core::ffi::c_int);
        next.ft = nft;
        let expanded = format_expand1_cstring(&raw mut next, use_0);
        drop(nft_owner);
        buffer.extend_from_slice(expanded.as_bytes());
        i += 1;
    }
    drop(active);
    drop(all);
    CString::new(buffer).expect("format loop output contains no NUL")
}

pub(super) unsafe fn format_window_name(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
) -> Option<CString> {
    let mut ft: *mut format_tree = (*es).ft;
    let format_session_owner = (*ft).s.upgrade();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    if format_session_owner.is_none() {
        format_log1(es, c"format_window_name".as_ptr(), |out| {
            out.write_all(b"window name but no session")
        });
        return None;
    }
    let name = format_expand1_cstring(es, fmt);
    wl = format_session_owner
        .as_ref()
        .expect("format session")
        .with_winlinks(|links| winlinks_minmax(links, RB_NEGINF));
    while wl.is_alive() {
        if wl
            .get_unchecked()
            .window_handle()
            .expect("linked window")
            .name()
            == name
        {
            return Some(c"1".to_owned());
        }
        wl = winlinks_next(wl.get_unchecked());
    }
    Some(c"0".to_owned())
}
pub(super) unsafe fn format_add_window_neighbour(
    mut nft: *mut format_tree,
    mut wl: refbox::Weak<winlink>,
    s_owner: &SessionRef,
    mut prefix: *const ::core::ffi::c_char,
) {
    let prefix = CStr::from_ptr(prefix).to_bytes();
    let key = CString::new([prefix, b"_window_index"].concat()).expect("C string key");
    format_add(nft, &key, |out| {
        write!(out, "{}", (wl.get_unchecked().idx) as u32)
    });
    let key = CString::new([prefix, b"_window_active"].concat()).expect("C string key");
    format_add(nft, &key, |out| {
        write!(out, "{}", {
            (wl == s_owner.current_winlink()) as ::core::ffi::c_int
        })
    });
    let entries = crate::src::options::options_iter(
        wl.get_unchecked()
            .window_handle()
            .expect("neighbour window")
            .borrow_options_mut(),
    )
    .filter(|(name, _)| name.to_bytes().first() == Some(&b'@'))
    .map(|(name, entry)| {
        let value = options_to_cstring(entry, nullable_cstr(std::ptr::null()), 1);
        (name.to_owned(), value)
    })
    .collect::<Vec<_>>();
    for (name, value) in entries {
        let prefixed =
            CString::new([prefix, b"_", name.to_bytes()].concat()).expect("C string key");
        format_add_cstr(nft, &prefixed, &value);
    }
}
pub(super) unsafe fn format_loop_windows(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
) -> Option<CString> {
    let mut sc: *mut sort_criteria = &raw mut sort_crit;
    let mut ft: *mut format_tree = (*es).ft;
    let format_session_owner = (*ft).s.upgrade();
    let format_client_owner = (*ft).c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let client_owner = (*ft).client.clone();
    let item_owner = (*ft).item.upgrade();
    let mut nft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut next: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    let mut buffer = Vec::new();
    let mut wl: refbox::Weak<winlink> = refbox::Weak::new();
    let mut i: ::core::ffi::c_int = 0;
    if format_session_owner.is_none() {
        format_log1(es, c"format_loop_windows".as_ptr(), |out| {
            out.write_all(b"window loop but no session")
        });
        return None;
    }
    let session = format_session_owner.as_ref().expect("format session");
    let (all, active) = format_choose_loop(es, fmt);
    let l = sort_get_winlinks_session(session, sc);
    let n = ::core::ffi::c_int::try_from(l.len()).expect("too many winlinks in format loop");
    i = 0 as ::core::ffi::c_int;
    while i < n {
        wl = l[i as usize].clone();
        let window_id = wl
            .get_unchecked()
            .window_handle()
            .expect("format loop window")
            .id();
        format_log1(es, c"format_loop_windows".as_ptr(), |out| {
            write!(
                out,
                "window loop: {} @{}",
                (wl.get_unchecked().idx) as u32,
                { window_id }
            )
        });
        let use_0 = match &active {
            Some(active_fmt) if wl == session.current_winlink() => active_fmt.as_ptr(),
            _ => all.as_ptr(),
        };
        let mut nft_owner = format_create_with_client(
            client_owner.as_ref(),
            item_owner.as_ref(),
            (FORMAT_WINDOW | window_id) as ::core::ffi::c_int,
            (*ft).flags,
        );
        nft = &raw mut *nft_owner;
        format_add(nft, c"loop_index", |out| write!(out, "{}", (i) as i32));
        format_add(nft, c"loop_last_flag", |out| {
            write!(
                out,
                "{}",
                ((i == n - 1 as ::core::ffi::c_int) as ::core::ffi::c_int) as i32
            )
        });
        if i > 0 as ::core::ffi::c_int
            && l[(i - 1 as ::core::ffi::c_int) as usize] == session.current_winlink()
        {
            format_add(nft, c"window_after_active", |out| out.write_all(b"1"));
        } else {
            format_add(nft, c"window_after_active", |out| out.write_all(b"0"));
        }
        if (i + 1 as ::core::ffi::c_int) < n
            && l[(i + 1 as ::core::ffi::c_int) as usize] == session.current_winlink()
        {
            format_add(nft, c"window_before_active", |out| out.write_all(b"1"));
        } else {
            format_add(nft, c"window_before_active", |out| out.write_all(b"0"));
        }
        if (i + 1 as ::core::ffi::c_int) < n {
            format_add_window_neighbour(
                nft,
                (l[(i + 1 as ::core::ffi::c_int) as usize]).clone(),
                session,
                c"next".as_ptr(),
            );
        }
        if i > 0 as ::core::ffi::c_int {
            format_add_window_neighbour(
                nft,
                (l[(i - 1 as ::core::ffi::c_int) as usize]).clone(),
                session,
                c"prev".as_ptr(),
            );
        }
        format_defaults(nft, format_client.as_ref(), Some(session), wl.clone(), None);
        format_copy_state(&raw mut next, es, 0 as ::core::ffi::c_int);
        next.ft = nft;
        let expanded = format_expand1_cstring(&raw mut next, use_0);
        drop(nft_owner);
        buffer.extend_from_slice(expanded.as_bytes());
        i += 1;
    }
    drop(active);
    drop(all);
    Some(CString::new(buffer).expect("format loop output contains no NUL"))
}

pub(super) unsafe fn format_loop_panes(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
) -> Option<CString> {
    let mut sc: *mut sort_criteria = &raw mut sort_crit;
    let mut ft: *mut format_tree = (*es).ft;
    let format_window_owner = (*ft).w.upgrade();
    let format_session_owner = (*ft).s.upgrade();
    let format_client_owner = (*ft).c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let client_owner = (*ft).client.clone();
    let item_owner = (*ft).item.upgrade();
    let mut nft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut next: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    let mut buffer = Vec::new();
    let mut i: ::core::ffi::c_int = 0;
    if format_window_owner.is_none() {
        format_log1(es, c"format_loop_panes".as_ptr(), |out| {
            out.write_all(b"pane loop but no window")
        });
        return None;
    }
    let (all, active) = format_choose_loop(es, fmt);
    let l = sort_get_panes_window(format_window_owner.as_ref().expect("format window"), &*sc);
    let n = i32::try_from(l.len()).expect("too many panes in format loop");
    i = 0 as ::core::ffi::c_int;
    while i < n {
        let pane = &l[i as usize];
        format_log1(es, c"format_loop_panes".as_ptr(), |out| {
            write!(out, "pane loop: %{}", pane.id())
        });
        let use_0 = match &active {
            Some(active_fmt)
                if format_window_owner
                    .as_ref()
                    .expect("live window")
                    .active_pane()
                    .ptr_eq(&std::rc::Rc::downgrade(pane)) =>
            {
                active_fmt.as_ptr()
            }
            _ => all.as_ptr(),
        };
        let mut nft_owner = format_create_with_client(
            client_owner.as_ref(),
            item_owner.as_ref(),
            (FORMAT_PANE | pane.id()) as ::core::ffi::c_int,
            (*ft).flags,
        );
        nft = &raw mut *nft_owner;
        format_add(nft, c"loop_index", |out| write!(out, "{}", (i) as i32));
        format_add(nft, c"loop_last_flag", |out| {
            write!(
                out,
                "{}",
                ((i == n - 1 as ::core::ffi::c_int) as ::core::ffi::c_int) as i32
            )
        });
        format_defaults(
            nft,
            format_client.as_ref(),
            format_session_owner.as_ref(),
            ((*ft).winlink_handle()).clone(),
            Some(pane),
        );
        format_copy_state(&raw mut next, es, 0 as ::core::ffi::c_int);
        next.ft = nft;
        let expanded = format_expand1_cstring(&raw mut next, use_0);
        drop(nft_owner);
        buffer.extend_from_slice(expanded.as_bytes());
        i += 1;
    }
    drop(active);
    drop(all);
    drop(format_window_owner.expect("format window"));
    Some(CString::new(buffer).expect("format loop output contains no NUL"))
}

unsafe fn format_option_loop_values(
    mut occupied: btree_map::OccupiedEntry<
        '_,
        CString,
        Box<crate::src::shared::options::options_entry>,
    >,
    array_key: Option<&CStr>,
    count: u_int,
    index: u_int,
    last_option: bool,
) -> Vec<(&'static CStr, CString)> {
    let name = occupied.key().clone();
    let option = &mut **occupied.get_mut();
    let is_array = options_is_array(option) != 0;
    let is_hook = option
        .tableentry_ptr()
        .is_some_and(|entry| entry.flags & OPTIONS_TABLE_IS_HOOK != 0);
    let is_user = option.tableentry_ptr().is_none();
    let key = array_key.unwrap_or(c"");
    let first = match array_key {
        None => is_array,
        Some(key) => crate::src::options::options_array_iter(option)
            .next()
            .is_some_and(|item| item.key.as_c_str() == key),
    };
    let array_last = match array_key {
        None => is_array,
        Some(key) => !crate::src::options::options_array_iter(option).any(|item| {
            crate::src::options::options_array_index(&item.key)
                > crate::src::options::options_array_index(key)
        }),
    };
    let number = |value: u_int| CString::new(value.to_string()).expect("integer format");
    vec![
        (c"option_name", name),
        (c"option_value", options_to_cstring(option, array_key, 0)),
        (c"option_is_array", number(u32::from(is_array))),
        (c"option_array_key", key.to_owned()),
        (c"option_array_index", key.to_owned()),
        (c"option_array_first", number(u32::from(first))),
        (c"option_array_last", number(u32::from(array_last))),
        (c"option_array_count", number(count)),
        (c"option_is_hook", number(u32::from(is_hook))),
        (c"option_is_user", number(u32::from(is_user))),
        (
            c"loop_last_flag",
            number(u32::from(
                last_option && (array_key.is_none() || array_last),
            )),
        ),
        (c"loop_index", number(index)),
    ]
}

unsafe fn format_loop_emit_option(
    es: *mut format_expand_state,
    fmt: *const ::core::ffi::c_char,
    buffer: &mut Vec<u8>,
    values: Vec<(&'static CStr, CString)>,
) {
    let ft = (*es).ft;
    let pane = (*ft).wp.upgrade();
    let session = (*ft).s.upgrade();
    let client = (*ft).c.upgrade();
    let item = (*ft).item.upgrade();
    format_log1(es, c"format_loop_options".as_ptr(), |out| {
        out.write_all(b"option loop: ")?;
        write_cstr(out, nullable_cstr(values[0].1.as_ptr()))?;
        if !values[3].1.as_bytes().is_empty() {
            out.write_all(b"[")?;
            write_cstr(out, nullable_cstr(values[3].1.as_ptr()))?;
            out.write_all(b"]")?;
        }
        Ok(())
    });
    let mut context = format_create_with_client(
        (*ft).client.as_ref(),
        item.as_ref(),
        FORMAT_NONE,
        (*ft).flags,
    );
    for (key, value) in values {
        format_add_cstr(&mut *context, key, &value);
    }
    format_defaults(
        &mut *context,
        client.as_ref(),
        session.as_ref(),
        (*ft).winlink_handle(),
        pane.as_ref(),
    );
    let mut next = format_expand_state::default();
    format_copy_state(&mut next, es, 0);
    next.ft = &mut *context;
    let expanded = format_expand1_cstring(&mut next, fmt);
    drop(context);
    buffer.extend_from_slice(expanded.as_bytes());
}

pub(super) unsafe fn format_loop_options(
    es: *mut format_expand_state,
    fmt: *const ::core::ffi::c_char,
    flags: *const ::core::ffi::c_char,
) -> CString {
    let ft = (*es).ft;
    let pane = (*ft).wp.upgrade();
    let window = (*ft).w.upgrade();
    let session = (*ft).s.upgrade();
    let flags = flags
        .as_ref()
        .map(|_| CStr::from_ptr(flags))
        .filter(|flags| !flags.to_bytes().is_empty())
        .unwrap_or(c"s");
    let has = |flag| flags.to_bytes().contains(&flag);
    let global = has(b'g');
    let mut access = |read: &mut dyn FnMut(&mut options)| {
        if has(b'v') {
            read(global_options());
        } else if has(b'w') {
            if global {
                read(global_w_options());
            } else if let Some(window) = window.as_ref() {
                read(window.borrow_options_mut());
            }
        } else if has(b's') {
            if global {
                read(global_s_options());
            } else if let Some(session) = session.as_ref() {
                read(session.borrow_options_mut());
            }
        } else if has(b'p') {
            if !global {
                if let Some(pane) = pane.as_ref() {
                    read(pane.borrow_options_mut());
                }
            }
        } else if global {
            read(global_s_options());
        }
    };
    let mut names = Vec::new();
    access(&mut |options| {
        names.extend(crate::src::options::options_iter(options).map(|(name, _)| name.to_owned()))
    });
    let mut buffer = Vec::new();
    let mut index: u_int = 0;
    for name in names {
        let mut keys = None;
        access(&mut |options| {
            if let btree_map::Entry::Occupied(mut entry) =
                crate::src::options::options_local_entry(options, &name)
            {
                let entry = &mut **entry.get_mut();
                keys = Some(if options_is_array(entry) != 0 {
                    crate::src::options::options_array_iter(entry)
                        .map(|item| item.key.clone())
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                });
            }
        });
        let Some(keys) = keys else { break };
        let count = u_int::try_from(keys.len()).expect("option array count");
        if keys.is_empty() {
            let mut values = None;
            access(&mut |options| {
                let last_option = !crate::src::options::options_iter(options)
                    .any(|(other, _)| other > name.as_c_str());
                if let btree_map::Entry::Occupied(occupied) =
                    crate::src::options::options_local_entry(options, &name)
                {
                    values = Some(format_option_loop_values(
                        occupied,
                        None,
                        0,
                        index,
                        last_option,
                    ));
                }
            });
            if let Some(values) = values {
                format_loop_emit_option(es, fmt, &mut buffer, values);
                index = index.wrapping_add(1);
            }
        } else {
            for key in keys {
                let mut values = None;
                access(&mut |options| {
                    let last_option = !crate::src::options::options_iter(options)
                        .any(|(other, _)| other > name.as_c_str());
                    if let btree_map::Entry::Occupied(occupied) =
                        crate::src::options::options_local_entry(options, &name)
                    {
                        if crate::src::options::options_array_get(occupied.get(), &key).is_some() {
                            values = Some(format_option_loop_values(
                                occupied,
                                Some(&key),
                                count,
                                index,
                                last_option,
                            ));
                        }
                    }
                });
                let Some(values) = values else { break };
                format_loop_emit_option(es, fmt, &mut buffer, values);
                index = index.wrapping_add(1);
            }
        }
    }
    let result = CString::new(buffer).expect("format loop output contains no NUL");
    if let Some(window) = window {
        drop(window);
    }
    result
}

pub(super) unsafe fn format_loop_environ(
    es: *mut format_expand_state,
    fmt: *const ::core::ffi::c_char,
    flags: *const ::core::ffi::c_char,
) -> CString {
    let ft = (*es).ft;
    let pane = (*ft).wp.upgrade();
    let session = (*ft).s.upgrade();
    let format_client = (*ft).c.upgrade();
    let client = (*ft).client.clone();
    let item = (*ft).item.upgrade();
    let flags = flags
        .as_ref()
        .map(|_| CStr::from_ptr(flags))
        .filter(|flags| !flags.to_bytes().is_empty())
        .unwrap_or(c"s");
    let mut access = |read: &mut dyn FnMut(&environ)| {
        if flags == c"s" {
            if let Some(session) = session.as_ref() {
                session.with_environment_mut(|environment| read(environment));
            }
        } else if flags == c"g" {
            if let Some(environment) = global_environ.as_deref() {
                read(environment);
            }
        } else if flags == c"c" {
            if let Some(client) = client.as_ref() {
                client.with_environment(|environment| {
                    if let Some(environment) = environment {
                        read(environment);
                    }
                });
            }
        }
    };
    let mut names = Vec::new();
    access(&mut |environment| {
        names.extend(environ_iter(environment).map(|entry| entry.name.clone()))
    });
    let count = names.len();
    let mut buffer = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let mut entry = None;
        access(&mut |environment| entry = environ_find(environment, &name).cloned());
        let Some(entry) = entry else { continue };
        format_log1(es, c"format_loop_environ".as_ptr(), |out| {
            out.write_all(b"environment loop: ")?;
            write_cstr(out, &*entry.name)
        });
        let mut context =
            format_create_with_client(client.as_ref(), item.as_ref(), FORMAT_NONE, (*ft).flags);
        format_add_cstr(&mut *context, c"environ_name", &entry.name);
        format_add_cstr(
            &mut *context,
            c"environ_value",
            entry.value.as_deref().unwrap_or(c""),
        );
        format_add_cstr(
            &mut *context,
            c"environ_hidden",
            if entry.flags & ENVIRON_HIDDEN != 0 {
                c"1"
            } else {
                c"0"
            },
        );
        format_add_cstr(
            &mut *context,
            c"environ_removed",
            if entry.value.is_none() { c"1" } else { c"0" },
        );
        format_add_cstr(
            &mut *context,
            c"loop_last_flag",
            if index + 1 == count { c"1" } else { c"0" },
        );
        format_add(&mut *context, c"loop_index", |out| {
            write!(out, "{}", index as u_int)
        });
        format_defaults(
            &mut *context,
            format_client.as_ref(),
            session.as_ref(),
            (*ft).winlink_handle(),
            pane.as_ref(),
        );
        let mut next = format_expand_state::default();
        format_copy_state(&mut next, es, 0);
        next.ft = &mut *context;
        let expanded = format_expand1_cstring(&mut next, fmt);
        drop(context);
        buffer.extend_from_slice(expanded.as_bytes());
    }
    CString::new(buffer).expect("format loop output contains no NUL")
}

pub(super) unsafe fn format_loop_clients(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
) -> CString {
    let mut sc: *mut sort_criteria = &raw mut sort_crit;
    let mut ft: *mut format_tree = (*es).ft;
    let format_pane_owner = (*ft).wp.upgrade();
    let format_session_owner = (*ft).s.upgrade();
    let mut c: Option<ClientRef> = None;
    let item_owner = (*ft).item.upgrade();
    let mut nft: *mut format_tree = ::core::ptr::null_mut::<format_tree>();
    let mut next: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    let mut buffer = Vec::new();
    let mut i: ::core::ffi::c_int = 0;
    let clients_sorted = sort_get_clients(sc);
    let n = ::core::ffi::c_int::try_from(clients_sorted.len())
        .expect("too many clients for format loop");
    i = 0 as ::core::ffi::c_int;
    while i < n {
        c = Some(clients_sorted[i as usize].clone());
        format_log1(es, c"format_loop_clients".as_ptr(), |out| {
            out.write_all(b"client loop: ")?;
            write_cstr(out, (c.as_ref().expect("live client").name()).as_deref())
        });
        let mut nft_owner = format_create_with_client(
            Some(&clients_sorted[i as usize]),
            item_owner.as_ref(),
            0 as ::core::ffi::c_int,
            (*ft).flags,
        );
        nft = &raw mut *nft_owner;
        format_add(nft, c"loop_index", |out| write!(out, "{}", (i) as i32));
        format_add(nft, c"loop_last_flag", |out| {
            write!(
                out,
                "{}",
                ((i == n - 1 as ::core::ffi::c_int) as ::core::ffi::c_int) as i32
            )
        });
        format_defaults(
            nft,
            c.as_ref(),
            format_session_owner.as_ref(),
            ((*ft).winlink_handle()).clone(),
            format_pane_owner.as_ref(),
        );
        format_copy_state(&raw mut next, es, 0 as ::core::ffi::c_int);
        next.ft = nft;
        let expanded = format_expand1_cstring(&raw mut next, fmt);
        drop(nft_owner);
        buffer.extend_from_slice(expanded.as_bytes());
        i += 1;
    }
    CString::new(buffer).expect("format loop output contains no NUL")
}

fn format_float(value: f64, precision: ::core::ffi::c_int) -> CString {
    if value.is_nan() {
        return if value.is_sign_negative() {
            c"-nan"
        } else {
            c"nan"
        }
        .to_owned();
    }
    let precision = if precision < 0 { 6 } else { precision as usize };
    format_cstring(format_args!("{value:.precision$}")).expect("formatted number contains no NUL")
}

fn format_expression_integer(value: f64) -> ::core::ffi::c_longlong {
    #[cfg(target_arch = "x86_64")]
    if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
        return i64::MIN;
    }
    value as ::core::ffi::c_longlong
}

pub(super) unsafe fn format_replace_expression(
    mut mexp: *mut format_modifier,
    mut es: *mut format_expand_state,
    mut copy: *const ::core::ffi::c_char,
) -> Option<CString> {
    let mut current_block: u64;
    let mut argc: ::core::ffi::c_int = (*mexp).argc();
    let mut endch: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut use_fp: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut prec: u_int = 0 as u_int;
    let mut mleft: ::core::ffi::c_double = 0.;
    let mut mright: ::core::ffi::c_double = 0.;
    let mut result: ::core::ffi::c_double = 0.;
    let mut operator: C2RustUnnamed_44 = ADD;
    if (*mexp).arg(0) == c"+" {
        operator = ADD;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"-" {
        operator = SUBTRACT;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"*" {
        operator = MULTIPLY;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"/" {
        operator = DIVIDE;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"%" || (*mexp).arg(0) == c"m" {
        operator = MODULUS;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"==" {
        operator = EQUAL;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"!=" {
        operator = NOT_EQUAL;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c">" {
        operator = GREATER_THAN;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"<" {
        operator = LESS_THAN;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c">=" {
        operator = GREATER_THAN_EQUAL;
        current_block = 4495394744059808450;
    } else if (*mexp).arg(0) == c"<=" {
        operator = LESS_THAN_EQUAL;
        current_block = 4495394744059808450;
    } else {
        format_log1(es, c"format_replace_expression".as_ptr(), |out| {
            out.write_all(b"expression has no valid operator: '")?;
            out.write_all((*mexp).arg(0).to_bytes())?;
            out.write_all(b"'")
        });
        current_block = 7376217411786091060;
    }
    if current_block == 4495394744059808450 {
        if argc >= 2 as ::core::ffi::c_int && (*mexp).arg(1).to_bytes().contains(&b'f') {
            use_fp = 1 as ::core::ffi::c_int;
            prec = 2 as u_int;
        }
        if argc >= 3 as ::core::ffi::c_int {
            let parsed = strtonum(
                (*mexp).arg(2),
                -FORMAT_MAX_PRECISION as ::core::ffi::c_longlong,
                FORMAT_MAX_PRECISION as ::core::ffi::c_longlong,
            );
            prec = parsed.unwrap_or(0) as u_int;
            if let Err(errstr) = parsed {
                format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                    out.write_all(b"expression precision ")?;
                    write_cstr(out, errstr)?;
                    out.write_all(b": ")?;
                    out.write_all((*mexp).arg(2).to_bytes())
                });
                current_block = 7376217411786091060;
            } else {
                current_block = 3437258052017859086;
            }
        } else {
            current_block = 3437258052017859086;
        }
        match current_block {
            7376217411786091060 => {}
            _ => {
                let operands = format_choose(es, copy);
                match operands {
                    None => {
                        format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                            out.write_all(b"expression syntax error")
                        });
                    }
                    Some((left, right)) => {
                        mleft = strtod(left.as_ptr(), &raw mut endch);
                        if *endch as ::core::ffi::c_int != '\0' as i32 {
                            format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                                out.write_all(b"expression left side is invalid: ")?;
                                write_cstr(out, &*left)
                            });
                        } else {
                            mright = strtod(right.as_ptr(), &raw mut endch);
                            if *endch as ::core::ffi::c_int != '\0' as i32 {
                                format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                                    out.write_all(b"expression right side is invalid: ")?;
                                    write_cstr(out, &*right)
                                });
                            } else {
                                if use_fp == 0 {
                                    mleft = format_expression_integer(mleft) as f64;
                                    mright = format_expression_integer(mright) as f64;
                                }
                                format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                                    out.write_all(b"expression left side is: ")?;
                                    out.write_all(format_float(mleft, prec as i32).as_bytes())
                                });
                                format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                                    out.write_all(b"expression right side is: ")?;
                                    out.write_all(format_float(mright, prec as i32).as_bytes())
                                });
                                match operator as ::core::ffi::c_uint {
                                    0 => {
                                        result = mleft + mright;
                                    }
                                    1 => {
                                        result = mleft - mright;
                                    }
                                    2 => {
                                        result = mleft * mright;
                                    }
                                    3 => {
                                        result = mleft / mright;
                                    }
                                    4 => {
                                        result = fmod(mleft, mright);
                                    }
                                    5 => {
                                        result = (fabs(mleft - mright) < 1e-9f64)
                                            as ::core::ffi::c_int
                                            as ::core::ffi::c_double;
                                    }
                                    6 => {
                                        result = (fabs(mleft - mright) > 1e-9f64)
                                            as ::core::ffi::c_int
                                            as ::core::ffi::c_double;
                                    }
                                    7 => {
                                        result = (mleft > mright) as ::core::ffi::c_int
                                            as ::core::ffi::c_double;
                                    }
                                    8 => {
                                        result = (mleft >= mright) as ::core::ffi::c_int
                                            as ::core::ffi::c_double;
                                    }
                                    9 => {
                                        result = (mleft < mright) as ::core::ffi::c_int
                                            as ::core::ffi::c_double;
                                    }
                                    10 => {
                                        result = (mleft <= mright) as ::core::ffi::c_int
                                            as ::core::ffi::c_double;
                                    }
                                    _ => {}
                                }
                                let value = format_float(
                                    if use_fp != 0 {
                                        result
                                    } else {
                                        format_expression_integer(result) as f64
                                    },
                                    prec as ::core::ffi::c_int,
                                );
                                format_log1(es, c"format_replace_expression".as_ptr(), |out| {
                                    out.write_all(b"expression result is ")?;
                                    write_cstr(out, &*value)
                                });
                                return Some(value);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
pub(super) unsafe fn format_cycle_start_timer(client: &ClientRef) {
    client.schedule_format_cycle(FORMAT_CYCLE_PERIOD);
}
pub(super) unsafe fn format_cycle(
    mut es: *mut format_expand_state,
    mut frames: *const ::core::ffi::c_char,
    mut count: u_int,
) -> CString {
    let mut ft: *mut format_tree = (*es).ft;
    let mut start: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cp: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut n: u_int = 0;
    let mut index: u_int = 0;
    let mut i: u_int = 0;
    if (*ft).flags & FORMAT_STATUS == 0 || (*es).flags & FORMAT_EXPAND_NOCYCLE != 0 {
        return c"".to_owned();
    }
    if *frames as ::core::ffi::c_int == '\0' as i32 {
        return c"".to_owned();
    }
    n = 1 as u_int;
    cp = frames;
    while *cp as ::core::ffi::c_int != '\0' as i32 {
        if *cp as ::core::ffi::c_int == ',' as i32 {
            n = n.wrapping_add(1);
        }
        cp = cp.offset(1);
    }
    index = (*es)
        .start_time
        .wrapping_div(count.wrapping_mul(FORMAT_CYCLE_PERIOD as u_int) as uint64_t)
        .wrapping_rem(n as uint64_t) as u_int;
    if n > 1 as u_int && (*ft).client.is_some() {
        format_cycle_start_timer((*ft).client.as_ref().expect("format client"));
    }
    start = frames;
    i = 0 as u_int;
    while i < index {
        start = strchr(start, ',' as i32).offset(1 as ::core::ffi::c_int as isize);
        i = i.wrapping_add(1);
    }
    end = strchr(start, ',' as i32);
    if end.is_null() {
        end = start.add(strlen(start));
    }
    CString::new(std::slice::from_raw_parts(
        start.cast::<u8>(),
        end.offset_from(start) as usize,
    ))
    .expect("cycle frame contains no NUL")
}
pub(super) unsafe fn format_replace(
    mut es: *mut format_expand_state,
    mut key: *const ::core::ffi::c_char,
    mut keylen: size_t,
    output: &mut Vec<u8>,
) -> ::core::ffi::c_int {
    let mut current_block: u64;
    let mut sc: *mut sort_criteria = &raw mut sort_crit;
    let mut ft: *mut format_tree = (*es).ft;
    let format_pane_owner = (*ft).wp.upgrade();
    let format_client_owner = (*ft).c.upgrade();
    let mut format_client: Option<ClientRef> = format_client_owner.clone();
    let mut copy: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cp: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut cp2: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut marker: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut time_format: Option<CString> = None;
    let mut value = CString::default();
    let mut modifiers: uint64_t = 0 as uint64_t;
    let mut limit: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut width: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    let mut j: ::core::ffi::c_int = 0;
    let mut c: ::core::ffi::c_int = 0;
    let mut list: Vec<format_modifier> = Vec::new();
    let mut cmp: *mut format_modifier = ::core::ptr::null_mut::<format_modifier>();
    let mut search: *mut format_modifier = ::core::ptr::null_mut::<format_modifier>();
    let mut sub: Vec<*mut format_modifier> = Vec::new();
    let mut mexp: *mut format_modifier = ::core::ptr::null_mut::<format_modifier>();
    let mut fm: *mut format_modifier = ::core::ptr::null_mut::<format_modifier>();
    let mut bool_op_n: *mut format_modifier = ::core::ptr::null_mut::<format_modifier>();
    let mut cycle_count: u_int = 1 as u_int;
    let mut i: u_int = 0;
    let mut nrep: u_int = 0;
    let mut loop_flags: *const ::core::ffi::c_char = c"".as_ptr();
    let mut next: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    let _envent: Option<&environ_entry> = None;
    (*sc).order = SORT_ORDER;
    (*sc).reversed = 0 as ::core::ffi::c_int;
    let key_end = libc::strnlen(key, keylen);
    let copy0 = CString::new(std::slice::from_raw_parts(key.cast::<u8>(), key_end))
        .expect("format key contains no NUL");
    let mut remaining = copy0.as_c_str();
    list = format_build_modifiers(es, &mut remaining);
    copy = remaining.as_ptr();
    i = 0 as u_int;
    while (i as usize) < list.len() {
        fm = list.as_mut_ptr().add(i as usize);
        if format_logging(ft) != 0 {
            format_log1(es, c"format_replace".as_ptr(), |out| {
                write!(out, "modifier {} is ", (i) as u32)?;
                write_cstr(
                    out,
                    nullable_cstr(&raw mut (*fm).modifier as *mut ::core::ffi::c_char),
                )
            });
            j = 0 as ::core::ffi::c_int;
            while j < (*fm).argc() {
                format_log1(es, c"format_replace".as_ptr(), |out| {
                    write!(out, "modifier {} argument {}: ", (i) as u32, (j) as i32)?;
                    out.write_all((*fm).arg(j as usize).to_bytes())
                });
                j += 1;
            }
        }
        if (*fm).size == 1 as u_int {
            match (*fm).modifier[0 as ::core::ffi::c_int as usize] as ::core::ffi::c_int {
                109 | 60 | 62 => {
                    cmp = fm;
                }
                33 => {
                    modifiers |= FORMAT_NOT as uint64_t;
                }
                67 => {
                    search = fm;
                }
                115 => {
                    if !((*fm).argc() < 2 as ::core::ffi::c_int) {
                        sub.push(fm);
                    }
                }
                61 => {
                    if !((*fm).argc() < 1 as ::core::ffi::c_int) {
                        let parsed = strtonum(
                            (*fm).arg(0),
                            -FORMAT_MAX_WIDTH as ::core::ffi::c_longlong,
                            FORMAT_MAX_WIDTH as ::core::ffi::c_longlong,
                        );
                        limit = parsed.unwrap_or(0) as ::core::ffi::c_int;
                        if parsed.is_err() {
                            limit = 0 as ::core::ffi::c_int;
                        }
                        if (*fm).argc() >= 2 as ::core::ffi::c_int {
                            marker = (*fm).arg(1).as_ptr();
                        }
                    }
                }
                112 => {
                    if !((*fm).argc() < 1 as ::core::ffi::c_int) {
                        let parsed = strtonum(
                            (*fm).arg(0),
                            -FORMAT_MAX_WIDTH as ::core::ffi::c_longlong,
                            FORMAT_MAX_WIDTH as ::core::ffi::c_longlong,
                        );
                        width = parsed.unwrap_or(0) as ::core::ffi::c_int;
                        if parsed.is_err() {
                            width = 0 as ::core::ffi::c_int;
                        }
                    }
                }
                65 => {
                    modifiers = (modifiers as ::core::ffi::c_ulonglong | FORMAT_CYCLE) as uint64_t;
                    if !((*fm).argc() < 1 as ::core::ffi::c_int) {
                        let parsed = strtonum(
                            (*fm).arg(0),
                            1 as ::core::ffi::c_longlong,
                            100 as ::core::ffi::c_longlong,
                        );
                        cycle_count = parsed.unwrap_or(0) as u_int;
                        if parsed.is_err() {
                            cycle_count = 1 as u_int;
                        }
                    }
                }
                119 => {
                    modifiers |= FORMAT_WIDTH as uint64_t;
                }
                101 => {
                    if !((*fm).argc() < 1 as ::core::ffi::c_int
                        || (*fm).argc() > 3 as ::core::ffi::c_int)
                    {
                        mexp = fm;
                    }
                }
                108 => {
                    modifiers |= FORMAT_LITERAL as uint64_t;
                }
                97 => {
                    modifiers |= FORMAT_CHARACTER as uint64_t;
                }
                98 => {
                    modifiers |= FORMAT_BASENAME as uint64_t;
                }
                99 => {
                    modifiers |= FORMAT_COLOUR as uint64_t;
                    if !((*fm).argc() < 1 as ::core::ffi::c_int) {
                        if (*fm).arg(0).to_bytes().contains(&b'f') {
                            modifiers |= FORMAT_COLOUR_ESC_FG as uint64_t;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'b') {
                            modifiers |= FORMAT_COLOUR_ESC_BG as uint64_t;
                        }
                    }
                }
                100 => {
                    modifiers |= FORMAT_DIRNAME as uint64_t;
                }
                110 => {
                    modifiers |= FORMAT_LENGTH as uint64_t;
                }
                73 => {
                    if !((*fm).argc() < 1 as ::core::ffi::c_int) {
                        if (*fm).arg(0).to_bytes().contains(&b'f') {
                            modifiers |= FORMAT_CLIENT_TERMFEAT as uint64_t;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'c') {
                            modifiers |= FORMAT_CLIENT_TERMCAP as uint64_t;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'e') {
                            modifiers |= FORMAT_CLIENT_ENVIRON as uint64_t;
                        }
                    }
                }
                116 => {
                    modifiers |= FORMAT_TIMESTRING as uint64_t;
                    if !((*fm).argc() < 1 as ::core::ffi::c_int) {
                        if (*fm).arg(0).to_bytes().contains(&b'p') {
                            modifiers |= FORMAT_PRETTY as uint64_t;
                        } else if (*fm).arg(0).to_bytes().contains(&b'r') {
                            modifiers |= FORMAT_RELATIVE as uint64_t;
                        } else if (*fm).arg(0).to_bytes().contains(&b'd') {
                            modifiers = (modifiers as ::core::ffi::c_ulonglong | FORMAT_DIFFERENCE)
                                as uint64_t;
                        } else if (*fm).argc() >= 2 as ::core::ffi::c_int
                            && (*fm).arg(0).to_bytes().contains(&b'f')
                        {
                            time_format = Some(format_strip_cstring(es, (*fm).arg(1)));
                        }
                    }
                }
                113 => {
                    if (*fm).argc() < 1 as ::core::ffi::c_int {
                        modifiers |= FORMAT_QUOTE_SHELL as uint64_t;
                    } else if (*fm).arg(0).to_bytes().contains(&b's') {
                        modifiers |= FORMAT_QUOTE_SHELL_SQ as uint64_t;
                    } else if (*fm).arg(0).to_bytes().contains(&b'e')
                        || (*fm).arg(0).to_bytes().contains(&b'h')
                    {
                        modifiers |= FORMAT_QUOTE_STYLE as uint64_t;
                    } else if (*fm).arg(0).to_bytes().contains(&b'a') {
                        modifiers |= FORMAT_QUOTE_ARGUMENTS as uint64_t;
                    }
                }
                69 => {
                    modifiers |= FORMAT_EXPAND as uint64_t;
                }
                84 => {
                    modifiers |= FORMAT_EXPANDTIME as uint64_t;
                }
                78 => {
                    if (*fm).argc() < 1 as ::core::ffi::c_int
                        || (*fm).arg(0).to_bytes().contains(&b'w')
                    {
                        modifiers |= FORMAT_WINDOW_NAME as uint64_t;
                    } else if (*fm).arg(0).to_bytes().contains(&b's') {
                        modifiers |= FORMAT_SESSION_NAME as uint64_t;
                    }
                }
                83 => {
                    modifiers |= FORMAT_SESSIONS as uint64_t;
                    if (*fm).argc() < 1 as ::core::ffi::c_int {
                        (*sc).order = SORT_INDEX;
                        (*sc).reversed = 0 as ::core::ffi::c_int;
                    } else {
                        if (*fm).arg(0).to_bytes().contains(&b'i') {
                            (*sc).order = SORT_INDEX;
                        } else if (*fm).arg(0).to_bytes().contains(&b'n') {
                            (*sc).order = SORT_NAME;
                        } else if (*fm).arg(0).to_bytes().contains(&b't') {
                            (*sc).order = SORT_ACTIVITY;
                        } else {
                            (*sc).order = SORT_INDEX;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'r') {
                            (*sc).reversed = 1 as ::core::ffi::c_int;
                        } else {
                            (*sc).reversed = 0 as ::core::ffi::c_int;
                        }
                    }
                }
                87 => {
                    modifiers |= FORMAT_WINDOWS as uint64_t;
                    if (*fm).argc() < 1 as ::core::ffi::c_int {
                        (*sc).order = SORT_ORDER;
                        (*sc).reversed = 0 as ::core::ffi::c_int;
                    } else {
                        if (*fm).arg(0).to_bytes().contains(&b'i') {
                            (*sc).order = SORT_ORDER;
                        } else if (*fm).arg(0).to_bytes().contains(&b'n') {
                            (*sc).order = SORT_NAME;
                        } else if (*fm).arg(0).to_bytes().contains(&b't') {
                            (*sc).order = SORT_ACTIVITY;
                        } else {
                            (*sc).order = SORT_ORDER;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'r') {
                            (*sc).reversed = 1 as ::core::ffi::c_int;
                        } else {
                            (*sc).reversed = 0 as ::core::ffi::c_int;
                        }
                    }
                }
                80 => {
                    modifiers |= FORMAT_PANES as uint64_t;
                    (*sc).order = SORT_CREATION;
                    if (*fm).argc() < 1 as ::core::ffi::c_int {
                        (*sc).reversed = 0 as ::core::ffi::c_int;
                    } else {
                        if (*fm).arg(0).to_bytes().contains(&b'i') {
                            (*sc).order = SORT_INDEX;
                        } else {
                            (*sc).order = SORT_CREATION;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'r') {
                            (*sc).reversed = 1 as ::core::ffi::c_int;
                        } else {
                            (*sc).reversed = 0 as ::core::ffi::c_int;
                        }
                    }
                }
                79 => {
                    modifiers |= FORMAT_OPTIONS as uint64_t;
                    if (*fm).argc() == 1 as ::core::ffi::c_int {
                        loop_flags = (*fm).arg(0).as_ptr();
                    }
                }
                86 => {
                    modifiers =
                        (modifiers as ::core::ffi::c_ulonglong | FORMAT_ENVIRON) as uint64_t;
                    if (*fm).argc() == 1 as ::core::ffi::c_int {
                        loop_flags = (*fm).arg(0).as_ptr();
                    }
                }
                76 => {
                    modifiers |= FORMAT_CLIENTS as uint64_t;
                    if (*fm).argc() < 1 as ::core::ffi::c_int {
                        (*sc).order = SORT_ORDER;
                        (*sc).reversed = 0 as ::core::ffi::c_int;
                    } else {
                        if (*fm).arg(0).to_bytes().contains(&b'i') {
                            (*sc).order = SORT_ORDER;
                        } else if (*fm).arg(0).to_bytes().contains(&b'n') {
                            (*sc).order = SORT_NAME;
                        } else if (*fm).arg(0).to_bytes().contains(&b't') {
                            (*sc).order = SORT_ACTIVITY;
                        } else {
                            (*sc).order = SORT_ORDER;
                        }
                        if (*fm).arg(0).to_bytes().contains(&b'r') {
                            (*sc).reversed = 1 as ::core::ffi::c_int;
                        } else {
                            (*sc).reversed = 0 as ::core::ffi::c_int;
                        }
                    }
                }
                82 => {
                    modifiers |= FORMAT_REPEAT as uint64_t;
                }
                _ => {}
            }
        } else if (*fm).size == 2 as u_int {
            if strcmp(
                &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                c"||".as_ptr(),
            ) == 0 as ::core::ffi::c_int
                || strcmp(
                    &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                    c"&&".as_ptr(),
                ) == 0 as ::core::ffi::c_int
            {
                bool_op_n = fm;
            } else if strcmp(
                &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                c"!!".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                modifiers |= FORMAT_NOT_NOT as uint64_t;
            } else if strcmp(
                &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                c"==".as_ptr(),
            ) == 0 as ::core::ffi::c_int
                || strcmp(
                    &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                    c"!=".as_ptr(),
                ) == 0 as ::core::ffi::c_int
                || strcmp(
                    &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                    c">=".as_ptr(),
                ) == 0 as ::core::ffi::c_int
                || strcmp(
                    &raw mut (*fm).modifier as *mut ::core::ffi::c_char,
                    c"<=".as_ptr(),
                ) == 0 as ::core::ffi::c_int
            {
                cmp = fm;
            }
        }
        i = i.wrapping_add(1);
    }
    if modifiers & FORMAT_CLIENT_TERMCAP as uint64_t != 0
        || modifiers & FORMAT_CLIENT_TERMFEAT as uint64_t != 0
        || modifiers & FORMAT_CLIENT_ENVIRON as uint64_t != 0
    {
        let ready = format_client.as_ref().is_some_and(|client| {
            let terminal_present = { client.borrow_terminal().term.is_some() };
            terminal_present && client.flags() & CLIENT_UNATTACHEDFLAGS as u64 == 0
        });
        if !ready {
            value = c"".to_owned();
        } else {
            let client = format_client.as_ref().expect("live client");
            if modifiers & FORMAT_CLIENT_TERMCAP as u64 != 0 {
                let terminal = client.borrow_terminal();
                value = if tty_term_has_name(
                    terminal.term.as_deref().expect("terminal description"),
                    CStr::from_ptr(copy),
                ) != 0
                {
                    c"1".to_owned()
                } else {
                    c"0".to_owned()
                };
            }
            if modifiers & FORMAT_CLIENT_TERMFEAT as u64 != 0 {
                let utf8 = client.flags() & crate::src::shared::client::CLIENT_UTF8 as u64 != 0;
                let terminal = client.borrow_terminal();
                value = if tty_feature_present(
                    terminal.term.as_deref().expect("terminal description"),
                    CStr::from_ptr(copy),
                    utf8,
                ) != 0
                {
                    c"1".to_owned()
                } else {
                    c"0".to_owned()
                };
            }
            if modifiers & FORMAT_CLIENT_ENVIRON as u64 != 0 {
                value = client.with_environment(|environment| {
                    environ_find(environment.expect("environment"), CStr::from_ptr(copy))
                        .and_then(|entry| entry.value.clone())
                        .unwrap_or_default()
                });
            }
        }
    } else if modifiers as ::core::ffi::c_ulonglong & FORMAT_CYCLE != 0 {
        value = format_cycle(es, copy, cycle_count);
        format_log1(es, c"format_replace".as_ptr(), |out| {
            out.write_all(b"cycle '")?;
            write_cstr(out, nullable_cstr(copy))?;
            out.write_all(b"' is: ")?;
            write_cstr(out, &*value)
        });
    } else if modifiers & FORMAT_LITERAL as uint64_t != 0 {
        format_log1(es, c"format_replace".as_ptr(), |out| {
            out.write_all(b"literal string is '")?;
            write_cstr(out, nullable_cstr(copy))?;
            out.write_all(b"'")
        });
        value = format_unescape_cstring(es, copy, strlen(copy));
    } else if modifiers & FORMAT_CHARACTER as uint64_t != 0 {
        let new = format_expand1_cstring(es, copy);
        let parsed = strtonum(
            &new,
            32 as ::core::ffi::c_longlong,
            126 as ::core::ffi::c_longlong,
        );
        c = parsed.unwrap_or(0) as ::core::ffi::c_int;
        if parsed.is_err() {
            value = c"".to_owned();
        } else {
            value = CString::new(vec![c as u8]).expect("printable character contains no NUL");
        }
    } else if modifiers & FORMAT_COLOUR as uint64_t != 0 {
        let new = format_expand1_cstring(es, copy);
        if modifiers & (FORMAT_COLOUR_ESC_FG | FORMAT_COLOUR_ESC_BG) as uint64_t != 0 {
            if strcasecmp(new.as_ptr(), c"none".as_ptr()) == 0 as ::core::ffi::c_int {
                value = c"\x1B[0m".to_owned();
            } else {
                c = colour_parse_cstr(new.as_c_str()).unwrap_or(-1);
                if c == -(1 as ::core::ffi::c_int) {
                    value = c"".to_owned();
                } else {
                    let escape = colour_format_escape_for_client(
                        format_client.as_ref(),
                        c,
                        modifiers & FORMAT_COLOUR_ESC_BG as uint64_t != 0,
                    );
                    value = escape.unwrap_or_default();
                }
            }
        } else {
            c = colour_parse_cstr(new.as_c_str()).unwrap_or(-1);
            if c == -(1 as ::core::ffi::c_int) || {
                c = colour_force_rgb(c);
                c == -(1 as ::core::ffi::c_int)
            } {
                value = c"".to_owned();
            } else {
                value = CString::new(format!("{:06x}", c & 0xffffff)).expect("RGB contains no NUL");
            }
        }
    } else {
        if modifiers & FORMAT_SESSIONS as uint64_t != 0 {
            value = format_loop_sessions(es, copy);
            current_block = 1803726662341650892;
        } else if modifiers & FORMAT_WINDOWS as uint64_t != 0 {
            if let Some(result) = format_loop_windows(es, copy) {
                value = result;
                current_block = 1803726662341650892;
            } else {
                current_block = 6506207624831006569;
            }
        } else if modifiers & FORMAT_PANES as uint64_t != 0 {
            if let Some(result) = format_loop_panes(es, copy) {
                value = result;
                current_block = 1803726662341650892;
            } else {
                current_block = 6506207624831006569;
            }
        } else if modifiers & FORMAT_CLIENTS as uint64_t != 0 {
            value = format_loop_clients(es, copy);
            current_block = 1803726662341650892;
        } else if modifiers & FORMAT_OPTIONS as uint64_t != 0 {
            value = format_loop_options(es, copy, loop_flags);
            current_block = 1803726662341650892;
        } else if modifiers as ::core::ffi::c_ulonglong & FORMAT_ENVIRON != 0 {
            value = format_loop_environ(es, copy, loop_flags);
            current_block = 1803726662341650892;
        } else if modifiers & FORMAT_WINDOW_NAME as uint64_t != 0 {
            if let Some(result) = format_window_name(es, copy) {
                value = result;
                current_block = 1803726662341650892;
            } else {
                current_block = 6506207624831006569;
            }
        } else if modifiers & FORMAT_SESSION_NAME as uint64_t != 0 {
            value = format_session_name(es, copy);
            current_block = 1803726662341650892;
        } else if !search.is_null() {
            let new = format_expand1_cstring(es, copy);
            match &format_pane_owner {
                None => {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"search '")?;
                        write_cstr(out, &*new)?;
                        out.write_all(b"' but no pane")
                    });
                    value = c"0".to_owned();
                }
                Some(search_pane) => {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"search '")?;
                        write_cstr(out, &*new)?;
                        write!(out, "' pane %{}", search_pane.id())
                    });
                    value = format_search(&*search, search_pane, &new);
                }
            }
            current_block = 1803726662341650892;
        } else if modifiers & FORMAT_REPEAT as uint64_t != 0 {
            let operands = format_choose(es, copy);
            match operands {
                None => {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"repeat syntax error: ")?;
                        write_cstr(out, nullable_cstr(copy))
                    });
                    current_block = 6506207624831006569;
                }
                Some((left, right)) => {
                    let parsed = strtonum(
                        &right,
                        1 as ::core::ffi::c_longlong,
                        FORMAT_MAX_REPEAT as ::core::ffi::c_longlong,
                    );
                    nrep = parsed.unwrap_or(0) as u_int;
                    let n = left.as_bytes().len();
                    if parsed.is_err() {
                        value = CString::default();
                    } else if n != 0 && nrep as usize > FORMAT_MAX_REPEAT_SIZE / n {
                        format_log1(es, c"format_replace".as_ptr(), |out| {
                            out.write_all(b"repeat is too long: ")?;
                            write_cstr(out, nullable_cstr(copy))
                        });
                        value = CString::default();
                    } else {
                        value = CString::new(left.as_bytes().repeat(nrep as usize))
                            .expect("repeated C string contains no NUL");
                    }
                    current_block = 1803726662341650892;
                }
            }
        } else if modifiers & FORMAT_NOT as uint64_t != 0 {
            value = format_bool_op_1(es, copy, 1 as ::core::ffi::c_int);
            current_block = 1803726662341650892;
        } else if modifiers & FORMAT_NOT_NOT as uint64_t != 0 {
            value = format_bool_op_1(es, copy, 0 as ::core::ffi::c_int);
            current_block = 1803726662341650892;
        } else if !bool_op_n.is_null() {
            if strcmp(
                &raw mut (*bool_op_n).modifier as *mut ::core::ffi::c_char,
                c"||".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                value = format_bool_op_n(es, copy, 0 as ::core::ffi::c_int);
            } else if strcmp(
                &raw mut (*bool_op_n).modifier as *mut ::core::ffi::c_char,
                c"&&".as_ptr(),
            ) == 0 as ::core::ffi::c_int
            {
                value = format_bool_op_n(es, copy, 1 as ::core::ffi::c_int);
            }
            current_block = 1803726662341650892;
        } else if !cmp.is_null() {
            let operands = format_choose(es, copy);
            match operands {
                None => {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"compare ")?;
                        write_cstr(
                            out,
                            nullable_cstr(&raw mut (*cmp).modifier as *mut ::core::ffi::c_char),
                        )?;
                        out.write_all(b" syntax error: ")?;
                        write_cstr(out, nullable_cstr(copy))
                    });
                    current_block = 6506207624831006569;
                }
                Some((left, right)) => {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"compare ")?;
                        write_cstr(
                            out,
                            nullable_cstr(&raw mut (*cmp).modifier as *mut ::core::ffi::c_char),
                        )?;
                        out.write_all(b" left is: ")?;
                        write_cstr(out, &*left)
                    });
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"compare ")?;
                        write_cstr(
                            out,
                            nullable_cstr(&raw mut (*cmp).modifier as *mut ::core::ffi::c_char),
                        )?;
                        out.write_all(b" right is: ")?;
                        write_cstr(out, &*right)
                    });
                    if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c"==".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        if strcmp(left.as_ptr(), right.as_ptr()) == 0 as ::core::ffi::c_int {
                            value = c"1".to_owned();
                        } else {
                            value = c"0".to_owned();
                        }
                    } else if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c"!=".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        if strcmp(left.as_ptr(), right.as_ptr()) != 0 as ::core::ffi::c_int {
                            value = c"1".to_owned();
                        } else {
                            value = c"0".to_owned();
                        }
                    } else if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c"<".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        if strcmp(left.as_ptr(), right.as_ptr()) < 0 as ::core::ffi::c_int {
                            value = c"1".to_owned();
                        } else {
                            value = c"0".to_owned();
                        }
                    } else if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c">".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        if strcmp(left.as_ptr(), right.as_ptr()) > 0 as ::core::ffi::c_int {
                            value = c"1".to_owned();
                        } else {
                            value = c"0".to_owned();
                        }
                    } else if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c"<=".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        if strcmp(left.as_ptr(), right.as_ptr()) <= 0 as ::core::ffi::c_int {
                            value = c"1".to_owned();
                        } else {
                            value = c"0".to_owned();
                        }
                    } else if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c">=".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        if strcmp(left.as_ptr(), right.as_ptr()) >= 0 as ::core::ffi::c_int {
                            value = c"1".to_owned();
                        } else {
                            value = c"0".to_owned();
                        }
                    } else if strcmp(
                        &raw mut (*cmp).modifier as *mut ::core::ffi::c_char,
                        c"m".as_ptr(),
                    ) == 0 as ::core::ffi::c_int
                    {
                        value = format_match(&*cmp, &left, &right);
                    }
                    current_block = 1803726662341650892;
                }
            }
        } else {
            if *copy as ::core::ffi::c_int == '?' as i32 {
                cp = copy.offset(1 as ::core::ffi::c_int as isize);
                loop {
                    cp2 = format_skip1(es, cp, c",".as_ptr());
                    if cp2.is_null() {
                        format_log1(es, c"format_replace".as_ptr(), |out| {
                            out.write_all(b"no condition matched in '")?;
                            write_cstr(
                                out,
                                nullable_cstr(copy.offset(1 as ::core::ffi::c_int as isize)),
                            )?;
                            out.write_all(b"'; using last arg")
                        });
                        value = format_expand1_cstring(es, cp);
                        break;
                    } else {
                        let condition = CString::new(std::slice::from_raw_parts(
                            cp.cast::<u8>(),
                            cp2.offset_from(cp) as usize,
                        ))
                        .expect("format condition contains no NUL");
                        format_log1(es, c"format_replace".as_ptr(), |out| {
                            out.write_all(b"condition is: ")?;
                            write_cstr(out, &*condition)
                        });
                        let found = format_find(
                            ft,
                            condition.as_ptr(),
                            modifiers,
                            time_format
                                .as_ref()
                                .map_or(::core::ptr::null(), |s| s.as_ptr()),
                        );
                        let condition_is_true = match &found {
                            None => {
                                let expanded = format_expand1_cstring(es, condition.as_ptr());
                                if expanded.as_c_str() == condition.as_c_str() {
                                    format_log1(es, c"format_replace".as_ptr(), |out| {
                                        out.write_all(b"condition '")?;
                                        write_cstr(out, &*condition)?;
                                        out.write_all(b"' not found; assuming false")
                                    });
                                    false
                                } else {
                                    format_true(expanded.as_ptr()) != 0
                                }
                            }
                            Some(found_value) => {
                                format_log1(es, c"format_replace".as_ptr(), |out| {
                                    out.write_all(b"condition '")?;
                                    write_cstr(out, &*condition)?;
                                    out.write_all(b"' found: ")?;
                                    write_cstr(out, found_value.as_c_str())
                                });
                                format_true(found_value.as_ptr()) != 0
                            }
                        };
                        drop(found);
                        cp = cp2.offset(1 as ::core::ffi::c_int as isize);
                        cp2 = format_skip1(es, cp, c",".as_ptr());
                        if condition_is_true {
                            format_log1(es, c"format_replace".as_ptr(), |out| {
                                out.write_all(b"condition '")?;
                                write_cstr(out, &*condition)?;
                                out.write_all(b"' is true")
                            });
                            if cp2.is_null() {
                                value = format_expand1_cstring(es, cp);
                            } else {
                                let right = CString::new(std::slice::from_raw_parts(
                                    cp.cast::<u8>(),
                                    cp2.offset_from(cp) as usize,
                                ))
                                .expect("format branch contains no NUL");
                                value = format_expand1_cstring(es, right.as_ptr());
                                drop(right);
                            }
                            drop(condition);
                            break;
                        } else {
                            format_log1(es, c"format_replace".as_ptr(), |out| {
                                out.write_all(b"condition '")?;
                                write_cstr(out, &*condition)?;
                                out.write_all(b"' is false")
                            });
                            drop(condition);
                            if cp2.is_null() {
                                format_log1(es, c"format_replace".as_ptr(), |out| {
                                    out.write_all(b"no condition matched in '")?;
                                    write_cstr(
                                        out,
                                        nullable_cstr(
                                            copy.offset(1 as ::core::ffi::c_int as isize),
                                        ),
                                    )?;
                                    out.write_all(b"'; using empty string")
                                });
                                value = c"".to_owned();
                                break;
                            } else {
                                cp = cp2.offset(1 as ::core::ffi::c_int as isize);
                            }
                        }
                    }
                }
            } else if !mexp.is_null() {
                value = format_replace_expression(mexp, es, copy).unwrap_or_default();
            } else if !strstr(copy, c"#{".as_ptr()).is_null() {
                format_log1(es, c"format_replace".as_ptr(), |out| {
                    out.write_all(b"expanding inner format '")?;
                    write_cstr(out, nullable_cstr(copy))?;
                    out.write_all(b"'")
                });
                value = format_expand1_cstring(es, copy);
            } else {
                value = if let Some(found) = format_find(
                    ft,
                    copy,
                    modifiers,
                    time_format
                        .as_ref()
                        .map_or(std::ptr::null(), |s| s.as_ptr()),
                ) {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"format '")?;
                        write_cstr(out, nullable_cstr(copy))?;
                        out.write_all(b"' found: ")?;
                        write_cstr(out, &*found)
                    });
                    found
                } else {
                    format_log1(es, c"format_replace".as_ptr(), |out| {
                        out.write_all(b"format '")?;
                        write_cstr(out, nullable_cstr(copy))?;
                        out.write_all(b"' not found")
                    });
                    CString::default()
                };
            }
            current_block = 1803726662341650892;
        }
        match current_block {
            1803726662341650892 => {}
            _ => {
                format_log1(es, c"format_replace".as_ptr(), |out| {
                    out.write_all(b"failed ")?;
                    write_cstr(out, &*copy0)
                });
                drop(sub);
                drop(list);
                drop(copy0);
                return -(1 as ::core::ffi::c_int);
            }
        }
    }
    if modifiers & FORMAT_EXPAND as uint64_t != 0 {
        value = format_expand1_cstring(es, value.as_ptr());
    } else if modifiers & FORMAT_EXPANDTIME as uint64_t != 0 {
        format_copy_state(&raw mut next, es, FORMAT_EXPAND_TIME);
        value = format_expand1_cstring(&raw mut next, value.as_ptr());
    }
    for &modifier in &sub {
        let left = format_expand1_cstring(es, (*modifier).arg(0).as_ptr());
        let right = format_expand1_cstring(es, (*modifier).arg(1).as_ptr());
        value = format_sub(&*modifier, &value, &left, &right);
        format_log1(es, c"format_replace".as_ptr(), |out| {
            out.write_all(b"substitute '")?;
            write_cstr(out, &*left)?;
            out.write_all(b"' to '")?;
            write_cstr(out, &*right)?;
            out.write_all(b"': ")?;
            write_cstr(out, &*value)
        });
    }
    if limit != 0 {
        let mut trimmed = if limit > 0 {
            format_trim_left_bytes(value.as_c_str(), limit as u_int)
        } else {
            format_trim_right_bytes(value.as_c_str(), -limit as u_int)
        };
        if !marker.is_null() && trimmed != value.as_bytes() {
            let marker = CStr::from_ptr(marker).to_bytes();
            if limit > 0 {
                trimmed.extend_from_slice(marker);
            } else {
                let mut prefixed = Vec::with_capacity(marker.len() + trimmed.len());
                prefixed.extend_from_slice(marker);
                prefixed.extend_from_slice(&trimmed);
                trimmed = prefixed;
            }
        }
        value = CString::new(trimmed).expect("trimmed C string contains no NUL");
        format_log1(es, c"format_replace".as_ptr(), |out| {
            write!(out, "applied length limit {}: ", (limit) as i32)?;
            write_cstr(out, &*value)
        });
    }
    if width != 0 {
        value = utf8_pad_cstring(value.as_c_str(), width.unsigned_abs(), width < 0);
        format_log1(es, c"format_replace".as_ptr(), |out| {
            write!(out, "applied padding width {}: ", (width) as i32)?;
            write_cstr(out, &*value)
        });
    }
    if modifiers & FORMAT_LENGTH as uint64_t != 0 {
        value = CString::new(value.as_bytes().len().to_string()).expect("length contains no NUL");
        format_log1(es, c"format_replace".as_ptr(), |out| {
            out.write_all(b"replacing with length: ")?;
            write_cstr(out, &*value)
        });
    }
    if modifiers & FORMAT_WIDTH as uint64_t != 0 {
        value =
            CString::new(format_width(value.as_ptr()).to_string()).expect("width contains no NUL");
        format_log1(es, c"format_replace".as_ptr(), |out| {
            out.write_all(b"replacing with width: ")?;
            write_cstr(out, &*value)
        });
    }
    output.extend_from_slice(value.as_bytes());
    format_log1(es, c"format_replace".as_ptr(), |out| {
        out.write_all(b"replaced '")?;
        write_cstr(out, &*copy0)?;
        out.write_all(b"' with '")?;
        write_cstr(out, &*value)?;
        out.write_all(b"'")
    });
    0
}
pub(super) unsafe fn format_expand1_cstring(
    mut es: *mut format_expand_state,
    mut fmt: *const ::core::ffi::c_char,
) -> CString {
    let mut ft: *mut format_tree = (*es).ft;
    let mut ptr: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut s: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut style_end: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut n: size_t = 0;
    let mut ch: ::core::ffi::c_int = 0;
    let mut brackets: ::core::ffi::c_int = 0;
    let mut expanded: [::core::ffi::c_char; 8192] = [0; 8192];
    if fmt.is_null()
        || *fmt as ::core::ffi::c_int == '\0' as i32
        || format_check_time(es, ::core::ptr::null_mut::<u_int>()) == 0
    {
        return CString::default();
    }
    if (*es).loop_0 == FORMAT_LOOP_LIMIT as u_int {
        format_log1(es, c"format_expand1".as_ptr(), |out| {
            write!(out, "reached loop limit ({})", (FORMAT_LOOP_LIMIT) as u32)
        });
        return CString::default();
    }
    (*es).loop_0 = (*es).loop_0.wrapping_add(1);
    format_log1(es, c"format_expand1".as_ptr(), |out| {
        out.write_all(b"expanding format: ")?;
        write_cstr(out, nullable_cstr(fmt))
    });
    if (*es).flags & FORMAT_EXPAND_TIME != 0 && !strchr(fmt, '%' as i32).is_null() {
        if (*es).time == 0 as time_t {
            (*es).time = time(::core::ptr::null_mut::<time_t>());
            localtime_r(&raw mut (*es).time, &raw mut (*es).tm);
        }
        if format_strftime(
            &raw mut expanded as *mut ::core::ffi::c_char,
            ::core::mem::size_of::<[::core::ffi::c_char; 8192]>() as size_t,
            fmt,
            &raw mut (*es).tm,
        ) == 0 as size_t
        {
            format_log1(es, c"format_expand1".as_ptr(), |out| {
                out.write_all(b"format is too long")
            });
            return CString::default();
        }
        if format_logging(ft) != 0
            && strcmp(&raw mut expanded as *mut ::core::ffi::c_char, fmt) != 0 as ::core::ffi::c_int
        {
            format_log1(es, c"format_expand1".as_ptr(), |out| {
                out.write_all(b"after time expanded: ")?;
                write_cstr(out, CStr::from_ptr(expanded.as_ptr()))
            });
        }
        fmt = &raw mut expanded as *mut ::core::ffi::c_char;
    }
    let mut output = Vec::<u8>::with_capacity(64);
    while *fmt as ::core::ffi::c_int != '\0' as i32 {
        if *fmt as ::core::ffi::c_int != '#' as i32 {
            let fresh10 = fmt;
            fmt = fmt.offset(1);
            output.push(*fresh10 as u8);
        } else {
            fmt = fmt.offset(1);
            if *fmt as ::core::ffi::c_int == '\0' as i32 {
                break;
            }
            let fresh12 = fmt;
            fmt = fmt.offset(1);
            ch = *fresh12 as u_char as ::core::ffi::c_int;
            match ch {
                40 => {
                    brackets = 1 as ::core::ffi::c_int;
                    ptr = fmt;
                    while *ptr as ::core::ffi::c_int != '\0' as i32 {
                        if *ptr as ::core::ffi::c_int == '(' as i32 {
                            brackets += 1;
                        }
                        if *ptr as ::core::ffi::c_int == ')' as i32 && {
                            brackets -= 1;
                            brackets == 0 as ::core::ffi::c_int
                        } {
                            break;
                        }
                        ptr = ptr.offset(1);
                    }
                    if *ptr as ::core::ffi::c_int != ')' as i32
                        || brackets != 0 as ::core::ffi::c_int
                    {
                        break;
                    }
                    n = ptr.offset_from(fmt) as ::core::ffi::c_long as size_t;
                    let name = CString::new(std::slice::from_raw_parts(fmt.cast::<u8>(), n))
                        .expect("format job name contains no NUL");
                    format_log1(es, c"format_expand1".as_ptr(), |out| {
                        out.write_all(b"found #(): ")?;
                        write_cstr(out, &*name)
                    });
                    let out = if (*ft).flags & FORMAT_NOJOBS != 0
                        || (*es).flags & FORMAT_EXPAND_NOJOBS != 0
                    {
                        format_log1(es, c"format_expand1".as_ptr(), |out| {
                            out.write_all(b"#() is disabled")
                        });
                        CString::default()
                    } else {
                        let out = format_job_get(es, &name);
                        format_log1(es, c"format_expand1".as_ptr(), |writer| {
                            writer.write_all(b"#() result: ")?;
                            write_cstr(writer, &*out)
                        });
                        out
                    };
                    output.extend_from_slice(out.as_bytes());
                    fmt = fmt.add(n.wrapping_add(1 as size_t));
                    continue;
                }
                123 => {
                    ptr = format_skip1(
                        es,
                        (fmt as *mut ::core::ffi::c_char)
                            .offset(-(2 as ::core::ffi::c_int as isize)),
                        c"}".as_ptr(),
                    );
                    if ptr.is_null() {
                        break;
                    }
                    n = ptr.offset_from(fmt) as ::core::ffi::c_long as size_t;
                    format_log1(es, c"format_expand1".as_ptr(), |out| {
                        out.write_all(b"found #{}: ")?;
                        out.write_all(std::slice::from_raw_parts(fmt.cast::<u8>(), n))
                    });
                    if format_replace(es, fmt, n, &mut output) != 0 as ::core::ffi::c_int {
                        break;
                    }
                    fmt = fmt.add(n.wrapping_add(1 as size_t));
                    continue;
                }
                91 | 35 => {
                    ptr = fmt.offset(-((ch == '[' as i32) as ::core::ffi::c_int as isize));
                    n = (2 as ::core::ffi::c_int - (ch == '[' as i32) as ::core::ffi::c_int)
                        as size_t;
                    while *ptr as ::core::ffi::c_int == '#' as i32 {
                        ptr = ptr.offset(1);
                        n = n.wrapping_add(1);
                    }
                    if *ptr as ::core::ffi::c_int == '[' as i32 {
                        style_end = format_skip1(
                            es,
                            fmt.offset(-(2 as ::core::ffi::c_int as isize)),
                            c"]".as_ptr(),
                        );
                        format_log1(es, c"format_expand1".as_ptr(), |out| {
                            write!(out, "found #*{}[", (n) as usize)
                        });
                        output.extend_from_slice(std::slice::from_raw_parts(
                            fmt.offset(-(2 as ::core::ffi::c_int as isize)).cast::<u8>(),
                            n.wrapping_add(1 as size_t),
                        ));
                        fmt = ptr.offset(1 as ::core::ffi::c_int as isize);
                        continue;
                    }
                    if ch == '#' as i32 {
                        output.resize(output.len() + n / 2, b'#');
                        fmt = if n % 2 != 0 { ptr.offset(-1) } else { ptr };
                        continue;
                    }
                }
                125 | 44 => {}
                _ => {
                    s = ::core::ptr::null::<::core::ffi::c_char>();
                    if fmt > style_end {
                        if ch >= 'A' as i32 && ch <= 'Z' as i32 {
                            s = format_upper[(ch - 'A' as i32) as usize]
                                .map_or(std::ptr::null(), CStr::as_ptr);
                        } else if ch >= 'a' as i32 && ch <= 'z' as i32 {
                            s = format_lower[(ch - 'a' as i32) as usize]
                                .map_or(std::ptr::null(), CStr::as_ptr);
                        }
                    }
                    if s.is_null() {
                        output.push(b'#');
                        output.push(ch as u8);
                        continue;
                    } else {
                        n = strlen(s);
                        format_log1(es, c"format_expand1".as_ptr(), |out| {
                            out.write_all(b"found #")?;
                            out.write_all(&[(ch) as u8])?;
                            out.write_all(b": ")?;
                            write_cstr(out, nullable_cstr(s))
                        });
                        if format_replace(es, s, n, &mut output) != 0 as ::core::ffi::c_int {
                            break;
                        } else {
                            continue;
                        }
                    }
                }
            }
            format_log1(es, c"format_expand1".as_ptr(), |out| {
                out.write_all(b"found #")?;
                out.write_all(&[(ch) as u8])
            });
            output.push(ch as u8);
        }
    }
    let buf = CString::new(output).expect("format expansion contains no NUL");
    format_log1(es, c"format_expand1".as_ptr(), |out| {
        out.write_all(b"result is: ")?;
        write_cstr(out, &*buf)
    });
    (*es).loop_0 = (*es).loop_0.wrapping_sub(1);
    buf
}
/// Expand a time-aware format into Rust-owned storage.
pub(crate) unsafe fn format_expand_time_cstring(
    mut ft: *mut format_tree,
    mut fmt: *const ::core::ffi::c_char,
) -> CString {
    let mut es: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    memset(
        &raw mut es as *mut ::core::ffi::c_void,
        0 as ::core::ffi::c_int,
        ::core::mem::size_of::<format_expand_state>() as size_t,
    );
    es.ft = ft;
    es.flags = FORMAT_EXPAND_TIME;
    es.start_time = get_timer();
    format_expand1_cstring(&raw mut es, fmt)
}
/// Own the expanded bytes independently of the tree. A null format is empty.
pub unsafe fn format_expand_cstring(
    mut ft: *mut format_tree,
    mut fmt: *const ::core::ffi::c_char,
) -> CString {
    let mut es: format_expand_state = format_expand_state {
        ft: ::core::ptr::null_mut::<format_tree>(),
        loop_0: 0,
        start_time: 0,
        flags: 0,
        time: 0,
        tm: unsafe { ::core::mem::zeroed() },
    };
    memset(
        &raw mut es as *mut ::core::ffi::c_void,
        0 as ::core::ffi::c_int,
        ::core::mem::size_of::<format_expand_state>() as size_t,
    );
    es.ft = ft;
    es.flags = 0 as ::core::ffi::c_int;
    es.start_time = get_timer();
    format_expand1_cstring(&raw mut es, fmt)
}
pub(crate) unsafe fn format_single_cstring(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    mut fmt: *const ::core::ffi::c_char,
    c_owner: Option<&ClientRef>,
    s_owner: Option<&SessionRef>,
    mut wl: refbox::Weak<winlink>,
    wp_owner: Option<&std::rc::Rc<std::cell::UnsafeCell<window_pane>>>,
) -> CString {
    let mut ft_owner = format_create_defaults(item_handle, c_owner, s_owner, wl.clone(), wp_owner);
    let ft = &raw mut *ft_owner;
    let expanded = format_expand_cstring(ft, fmt);
    drop(ft_owner);
    expanded
}
pub(crate) unsafe fn format_single_from_state_cstring(
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
    fmt: *const ::core::ffi::c_char,
    c_owner: Option<&ClientRef>,
    fs: *mut cmd_find_state,
) -> CString {
    format_single_cstring(
        item_handle,
        fmt,
        c_owner,
        (*fs).s.upgrade().as_ref(),
        ((*fs).winlink_handle()).clone(),
        (*fs).wp.upgrade().as_ref(),
    )
}
pub(crate) unsafe fn format_single_from_target_cstring(
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    fmt: *const ::core::ffi::c_char,
) -> CString {
    let item = item_handle.get();
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut tc: Option<ClientRef> = tc_owner.clone();
    format_single_from_state_cstring(
        Some(item_handle),
        fmt,
        tc.as_ref(),
        crate::src::cmd::queue::cmdq_get_target_mut(&mut *item),
    )
}

#[cfg(test)]
mod format_float_tests {
    use super::format_float;
    use std::ffi::CString;

    #[test]
    fn matches_printf_for_precision_rounding_and_special_values() {
        let values = [
            0.0,
            -0.0,
            0.125,
            -0.125,
            2.5,
            3.5,
            9.999,
            1.23456789,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::MAX,
            -f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            -f64::NAN,
        ];
        for value in values {
            for precision in [-100, -1, 0, 1, 2, 6, 17, 100] {
                let expected = if value.is_nan() {
                    if value.is_sign_negative() {
                        c"-nan".to_owned()
                    } else {
                        c"nan".to_owned()
                    }
                } else {
                    unsafe {
                        let len = libc::snprintf(
                            std::ptr::null_mut(),
                            0,
                            c"%.*f".as_ptr(),
                            precision,
                            value,
                        );
                        assert!(len >= 0);
                        let mut bytes = vec![0u8; len as usize + 1];
                        assert_eq!(
                            libc::snprintf(
                                bytes.as_mut_ptr().cast(),
                                bytes.len(),
                                c"%.*f".as_ptr(),
                                precision,
                                value,
                            ),
                            len
                        );
                        CString::from_vec_with_nul(bytes).unwrap()
                    }
                };
                assert_eq!(
                    format_float(value, precision),
                    expected,
                    "value={value:?}, precision={precision}"
                );
            }
        }
    }
}

#[cfg(test)]
mod format_choose_tests {
    use super::*;

    #[test]
    fn expanded_owners_outlive_their_inputs_and_tree() {
        unsafe {
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &raw mut *ft_owner;
            let input = CString::new(b"\xff:##:#,:#}:tail#".to_vec()).unwrap();
            let owned = format_expand_cstring(ft, input.as_ptr());
            let timed_owned = format_expand_time_cstring(ft, input.as_ptr());
            drop(input);
            drop(ft_owner);

            assert_eq!(owned.as_bytes(), b"\xff:#:,:}:tail");
            assert_eq!(timed_owned.as_bytes(), owned.as_bytes());
            assert_eq!(owned.as_bytes(), b"\xff:#:,:}:tail");
        }
    }

    #[test]
    fn owned_expansion_preserves_empty_and_limit_results() {
        unsafe {
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &raw mut *ft_owner;
            assert!(format_expand_cstring(ft, std::ptr::null())
                .as_bytes()
                .is_empty());
            assert!(format_expand_cstring(ft, c"".as_ptr())
                .as_bytes()
                .is_empty());
            assert_eq!(
                format_expand_cstring(ft, b"first\0\xffignored\0".as_slice().as_ptr().cast())
                    .as_bytes(),
                b"first"
            );
            let mut es = format_expand_state {
                ft,
                start_time: get_timer(),
                loop_0: FORMAT_LOOP_LIMIT as u_int,
                ..Default::default()
            };
            assert!(format_expand1_cstring(&raw mut es, c"limit".as_ptr())
                .as_bytes()
                .is_empty());
            assert_eq!(es.loop_0, FORMAT_LOOP_LIMIT as u_int);
            es.loop_0 = 0;
            es.start_time = get_timer().wrapping_sub(FORMAT_TIME_LIMIT as uint64_t);
            assert!(format_expand1_cstring(&raw mut es, c"timeout".as_ptr())
                .as_bytes()
                .is_empty());
            es.start_time = get_timer();
            es.flags = FORMAT_EXPAND_TIME;
            let too_long = CString::new(format!("%Y{}", "x".repeat(8192))).unwrap();
            assert!(format_expand1_cstring(&raw mut es, too_long.as_ptr())
                .as_bytes()
                .is_empty());
            drop(ft_owner);
        }
    }

    #[test]
    fn split_operands_preserve_escapes_nesting_and_bytes() {
        unsafe {
            let mut ft_owner = format_create(None, None, 0, 0);
            let ft = &raw mut *ft_owner;
            let mut es = format_expand_state {
                ft,
                start_time: get_timer(),
                ..Default::default()
            };

            for (input, expected_left, expected_right) in [
                (
                    b"one#,two,three\0".as_slice(),
                    b"one#,two".as_slice(),
                    b"three".as_slice(),
                ),
                (b"\xff,\xfe\0", b"\xff", b"\xfe"),
                (b"#{?1,a,b},tail\0", b"#{?1,a,b}", b"tail"),
                (b",\0", b"", b""),
            ] {
                let input = CStr::from_bytes_with_nul(input).unwrap();
                let (left, right) = format_choose_loop(&raw mut es, input.as_ptr());
                assert_eq!(left.to_bytes(), expected_left, "{input:?}");
                assert_eq!(right.unwrap().to_bytes(), expected_right, "{input:?}");
            }
            let (all, active) = format_choose_loop(&raw mut es, c"no delimiter".as_ptr());
            assert_eq!(all.to_bytes(), b"no delimiter");
            assert!(active.is_none());

            for (input, expected_left, expected_right) in [
                (
                    b"one#,two,three\0".as_slice(),
                    b"one,two".as_slice(),
                    b"three".as_slice(),
                ),
                (b"\xff,\xfe\0", b"\xff", b"\xfe"),
                (b",\0", b"", b""),
            ] {
                let input = CStr::from_bytes_with_nul(input).unwrap();
                let (left, right) = format_choose(&raw mut es, input.as_ptr()).unwrap();
                assert_eq!(left.to_bytes(), expected_left, "{input:?}");
                assert_eq!(right.to_bytes(), expected_right, "{input:?}");
            }

            assert!(format_choose(&raw mut es, c"no delimiter".as_ptr()).is_none());
            drop(ft_owner);
        }
    }
}
