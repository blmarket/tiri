use crate::format::bytes::cstr_suffix;
use crate::log::{log_bytes, log_debug};
use crate::options::OptionsScope;
use crate::options::{options_get_number, options_get_string};
use crate::prompt::prompt_type_string;
use crate::shared::abi::*;
use crate::shared::errno::ErrorText;
use crate::shared::prompt::PROMPT_NTYPES;
use crate::shared::prompt::*;
use crate::tmux::find_home_cstr;
use std::cell::RefCell;
use std::ffi::{CStr, CString, OsStr};
use std::io::{BufWriter, Write as _};
use std::os::unix::ffi::OsStrExt;

thread_local! {
    static PROMPT_HISTORY: RefCell<[Vec<CString>; PROMPT_NTYPES as usize]> =
        const { RefCell::new([Vec::new(), Vec::new()]) };
}

unsafe fn prompt_find_history_file() -> Option<CString> {
    let history_file =
        OptionsScope::GlobalServer.with_local(|oo| options_get_string(oo, c"history-file"));
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
fn prompt_add_typed_history(line: &CStr) {
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
            let content = cstr_suffix(line, colon + 1);
            prompt_add_history(content, history_type);
            return;
        }
    }
    prompt_add_history(line, PROMPT_TYPE_COMMAND);
}
pub unsafe fn prompt_load_history() {
    let history_file = match prompt_find_history_file() {
        Some(path) => path,
        None => return,
    };
    log_debug(format_args!(
        "loading history from {}",
        log_bytes(history_file.as_bytes())
    ));
    let bytes = match std::fs::read(OsStr::from_bytes(history_file.as_bytes())) {
        Ok(bytes) => bytes,
        Err(error) => {
            log_debug(format_args!(
                "{}: {}",
                log_bytes(history_file.as_bytes()),
                ErrorText(&error)
            ));
            return;
        }
    };
    for line in bytes.split_inclusive(|&byte| byte == b'\n') {
        let mut line = line.strip_suffix(b"\n").unwrap_or(line).to_vec();
        line.push(0);
        let line = CStr::from_bytes_until_nul(&line).expect("history line has a terminator");
        prompt_add_typed_history(line);
    }
}
pub unsafe fn prompt_save_history() {
    let history_file = match prompt_find_history_file() {
        Some(path) => path,
        None => return,
    };
    log_debug(format_args!(
        "saving history to {}",
        log_bytes(history_file.as_bytes())
    ));
    let mut stream = match std::fs::File::create(OsStr::from_bytes(history_file.as_bytes())) {
        Ok(file) => BufWriter::new(file),
        Err(error) => {
            log_debug(format_args!(
                "{}: {}",
                log_bytes(history_file.as_bytes()),
                ErrorText(&error)
            ));
            return;
        }
    };
    // Write failures are ignored, as with the unchecked stdio writes in tmux.
    let _ = PROMPT_HISTORY.with_borrow(|histories| -> std::io::Result<()> {
        for (kind, history) in histories.iter().enumerate() {
            for entry in history {
                stream.write_all(prompt_type_string(kind as prompt_type).to_bytes())?;
                stream.write_all(b":")?;
                stream.write_all(entry.to_bytes())?;
                stream.write_all(b"\n")?;
            }
        }
        stream.flush()
    });
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

pub fn prompt_add_history(line: &CStr, kind: prompt_type) {
    if kind >= PROMPT_NTYPES as prompt_type {
        return;
    }
    let limit = OptionsScope::GlobalServer
        .with_local(|oo| options_get_number(oo, c"prompt-history-limit")) as u_int;
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
