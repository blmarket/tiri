use crate::src::compat::stdio::CFile;
use crate::src::ffi::libc::__errno_location;
use crate::src::log::{log_bytes, log_cstr, log_debug};
use crate::src::options::{options_get_number, options_get_string};
use crate::src::prompt::prompt_type_string;
use crate::src::shared::abi::*;
use crate::src::shared::prompt::PROMPT_NTYPES;
use crate::src::shared::prompt::*;
use crate::src::shared::stdio::FILE;
use crate::src::tmux::{find_home_cstr, global_options};
use libc::{fgetc, fopen, fputc, fputs, strerror};
use std::cell::RefCell;
use std::ffi::{CStr, CString};

unsafe fn read_history_line(stream: &mut CFile, line: &mut Vec<u8>) -> bool {
    line.clear();
    loop {
        let ch = fgetc(stream.as_ptr());
        if ch == -1 {
            return !line.is_empty();
        }
        if ch == '\n' as i32 {
            return true;
        }
        line.push(ch as u8);
    }
}
thread_local! {
    static PROMPT_HISTORY: RefCell<[Vec<CString>; PROMPT_NTYPES as usize]> =
        const { RefCell::new([Vec::new(), Vec::new()]) };
}

unsafe fn prompt_find_history_file() -> Option<CString> {
    let history_file = options_get_string(&*global_options, c"history-file");
    if history_file.as_bytes().is_empty() {
        return None;
    }
    if history_file.as_bytes()[0] == b'/' {
        return Some(history_file);
    }
    if !history_file.as_bytes().starts_with(b"~/") {
        return None;
    }
    let home = find_home_cstr()?;
    let mut path = home.to_bytes().to_vec();
    path.extend_from_slice(&history_file.as_bytes()[1..]);
    Some(CString::new(path).expect("C string paths contain no interior NUL"))
}
unsafe fn prompt_add_typed_history(line: &CStr) {
    let bytes = line.to_bytes();
    if let Some(colon) = bytes.iter().position(|&byte| byte == b':') {
        let prefix = &bytes[..colon];
        let history_type = if prefix == b"command" {
            Some(PROMPT_TYPE_COMMAND)
        } else if prefix == b"search" {
            Some(PROMPT_TYPE_SEARCH)
        } else {
            None
        };
        if let Some(history_type) = history_type {
            let content = CStr::from_bytes_with_nul(&line.to_bytes_with_nul()[colon + 1..])
                .expect("history content is a C string suffix");
            prompt_add_history(content, history_type);
            return;
        }
    }
    prompt_add_history(line, PROMPT_TYPE_COMMAND);
}
pub unsafe fn prompt_load_history() {
    let mut f: *mut FILE = ::core::ptr::null_mut::<FILE>();
    let mut line = Vec::<u8>::new();
    let history_file = match prompt_find_history_file() {
        Some(path) => path,
        None => return,
    };
    log_debug(format_args!(
        "loading history from {}",
        log_bytes(history_file.as_bytes())
    ));
    f = fopen(history_file.as_ptr(), c"r".as_ptr());
    if f.is_null() {
        log_debug(format_args!(
            "{}: {}",
            log_bytes(history_file.as_bytes()),
            log_cstr(CStr::from_ptr(strerror(*__errno_location())))
        ));
        return;
    }
    let mut stream = CFile::from_raw(f).expect("fopen returned a non-null stream");
    while read_history_line(&mut stream, &mut line) {
        line.push(0);
        let line = CStr::from_bytes_until_nul(&line).expect("history line has a terminator");
        prompt_add_typed_history(line);
    }
    drop(stream);
}
pub unsafe fn prompt_save_history() {
    let mut f: *mut FILE = ::core::ptr::null_mut::<FILE>();
    let history_file = match prompt_find_history_file() {
        Some(path) => path,
        None => return,
    };
    log_debug(format_args!(
        "saving history to {}",
        log_bytes(history_file.as_bytes())
    ));
    f = fopen(history_file.as_ptr(), c"w".as_ptr());
    if f.is_null() {
        log_debug(format_args!(
            "{}: {}",
            log_bytes(history_file.as_bytes()),
            log_cstr(CStr::from_ptr(strerror(*__errno_location())))
        ));
        return;
    }
    let stream = CFile::from_raw(f).expect("fopen returned a non-null stream");
    PROMPT_HISTORY.with_borrow(|histories| {
        for (kind, history) in histories.iter().enumerate() {
            for entry in history {
                fputs(
                    prompt_type_string(kind as prompt_type).as_ptr(),
                    stream.as_ptr(),
                );
                fputc(b':' as i32, stream.as_ptr());
                fputs(entry.as_ptr(), stream.as_ptr());
                fputc(b'\n' as i32, stream.as_ptr());
            }
        }
    });
    drop(stream);
}
/// Move backward through history. None leaves the prompt input unchanged.
pub fn prompt_up_history(
    indexes: &mut [u_int; PROMPT_NTYPES as usize],
    kind: prompt_type,
) -> Option<CString> {
    PROMPT_HISTORY.with_borrow(|histories| {
        let history = histories.get(kind as usize)?;
        let index = &mut indexes[kind as usize];
        if *index as usize >= history.len() {
            return None;
        }
        *index += 1;
        Some(history[history.len() - *index as usize].clone())
    })
}

/// Move forward through history. None represents the empty input at its end.
pub fn prompt_down_history(
    indexes: &mut [u_int; PROMPT_NTYPES as usize],
    kind: prompt_type,
) -> Option<CString> {
    PROMPT_HISTORY.with_borrow(|histories| {
        let history = histories.get(kind as usize)?;
        let index = &mut indexes[kind as usize];
        if history.is_empty() || *index == 0 {
            return None;
        }
        *index -= 1;
        if *index == 0 {
            return None;
        }
        history
            .get(history.len().checked_sub(*index as usize)?)
            .cloned()
    })
}

pub unsafe fn prompt_add_history(line: &CStr, kind: prompt_type) {
    if kind >= PROMPT_NTYPES as prompt_type {
        return;
    }
    let limit = options_get_number(&*global_options, c"prompt-history-limit") as u_int;
    PROMPT_HISTORY.with_borrow_mut(|histories| {
        let history = &mut histories[kind as usize];
        let old_size = history.len() as u_int;
        let new = !history.last().is_some_and(|last| last.as_c_str() == line);
        if limit > old_size {
            if !new {
                return;
            }
        } else if old_size + u_int::from(new) - limit == 0 {
            return;
        }

        let added = (new && limit != 0).then(|| line.to_owned());
        if limit <= old_size {
            let free_count = (old_size + u_int::from(new) - limit).min(old_size) as usize;
            history.drain(..free_count);
        }
        if limit == 0 {
            *history = Vec::new();
        } else if let Some(added) = added {
            history.push(added);
        }
    });
}

pub fn prompt_history_size(kind: prompt_type) -> u_int {
    PROMPT_HISTORY.with_borrow(|histories| {
        histories
            .get(kind as usize)
            .map_or(0, |history| history.len() as u_int)
    })
}

pub fn prompt_history_get(kind: prompt_type, index: u_int) -> Option<CString> {
    PROMPT_HISTORY
        .with_borrow(|histories| histories.get(kind as usize)?.get(index as usize).cloned())
}

pub fn prompt_history_clear(kind: prompt_type) {
    PROMPT_HISTORY.with_borrow_mut(|histories| {
        if let Some(history) = histories.get_mut(kind as usize) {
            *history = Vec::new();
        }
    });
}
