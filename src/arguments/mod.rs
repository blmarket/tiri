//! Command argument storage, parsing, printing and value conversion.

use crate::src::cmd::parse::cmd_parse_from_string;
use crate::src::cmd::queue::{cmdq_error, cmdq_get_target_client};
use crate::src::cmd::{
    cmd_get_args_mut, cmd_get_source, cmd_list_copy, cmd_list_first, cmd_list_print_cstring,
    cmd_log_argv, cmd_template_replace_cstring,
};
use crate::src::compat::strtonum::strtonum;
use crate::src::format::bytes::write_cstr;
use crate::src::format::format_single_from_target_cstring;
use crate::src::log::{fatalx, log_byte, log_bytes, log_cstr, log_debug};

use crate::src::shared::abi::*;
use crate::src::shared::client::ClientRef;
use crate::src::shared::command::{cmd, cmd_find_state, cmd_list, cmd_parse_input, cmdq_item};
use crate::src::shared::vis::{VIS_CSTYLE, VIS_DQ, VIS_NL, VIS_OCTAL, VIS_TAB};
use crate::src::text::utf8::utf8_strvis;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::rc::Rc;

type args_type = ::core::ffi::c_uint;
pub const ARGS_COMMANDS: args_type = 2;
pub const ARGS_STRING: args_type = 1;
const ARGS_NONE: args_type = 0;

pub type args_parse_type = ::core::ffi::c_uint;
const ARGS_PARSE_COMMANDS: args_parse_type = 3;
pub const ARGS_PARSE_COMMANDS_OR_STRING: args_parse_type = 2;
pub const ARGS_PARSE_STRING: args_parse_type = 1;
const ARGS_PARSE_INVALID: args_parse_type = 0;

#[derive(Debug)]
pub enum ArgsParseError {
    Usage,
    Message(std::ffi::CString),
}

#[repr(C)]
pub struct args {
    tree: args_tree,
    pub values: Vec<args_value>,
}

enum ArgsPayload<'a> {
    None,
    String(CString),
    Command(Rc<std::cell::RefCell<cmd_list>>),
    BorrowedString(&'a CStr),
    BorrowedCommand(&'a Rc<std::cell::RefCell<cmd_list>>),
}

/// Parser inputs can borrow payloads; stored arguments contain owned copies.
pub struct ArgumentValue<'a> {
    payload: ArgsPayload<'a>,
    cached: Option<CString>,
}

pub type args_value = ArgumentValue<'static>;

impl<'a> ArgumentValue<'a> {
    fn new(payload: ArgsPayload<'a>) -> Self {
        Self {
            payload,
            cached: None,
        }
    }

    pub fn empty() -> Self {
        Self::new(ArgsPayload::None)
    }

    pub fn string(value: CString) -> Self {
        Self::new(ArgsPayload::String(value))
    }

    pub fn commands(value: Rc<std::cell::RefCell<cmd_list>>) -> Self {
        Self::new(ArgsPayload::Command(value))
    }

    pub fn borrowed_string(value: &'a CStr) -> Self {
        Self::new(ArgsPayload::BorrowedString(value))
    }

    pub fn borrowed_commands(value: &'a Rc<std::cell::RefCell<cmd_list>>) -> Self {
        Self::new(ArgsPayload::BorrowedCommand(value))
    }

    pub fn type_0(&self) -> args_type {
        match self.payload {
            ArgsPayload::None => ARGS_NONE,
            ArgsPayload::String(_) | ArgsPayload::BorrowedString(_) => ARGS_STRING,
            ArgsPayload::Command(_) | ArgsPayload::BorrowedCommand(_) => ARGS_COMMANDS,
        }
    }

    pub fn as_string(&self) -> Option<&CStr> {
        match &self.payload {
            ArgsPayload::String(value) => Some(value),
            ArgsPayload::BorrowedString(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_commands(&self) -> Option<&Rc<std::cell::RefCell<cmd_list>>> {
        match &self.payload {
            ArgsPayload::Command(value) => Some(value),
            ArgsPayload::BorrowedCommand(value) => Some(value),
            _ => None,
        }
    }

    /// Copy into stored arguments without carrying any parser-input borrow or cache.
    fn to_owned(&self) -> args_value {
        if let Some(text) = self.as_string() {
            args_value::string(text.to_owned())
        } else if let Some(commands) = self.as_commands() {
            args_value::commands(Rc::clone(commands))
        } else {
            args_value::empty()
        }
    }
}

type args_tree = BTreeMap<u_char, Box<args_entry>>;

#[repr(C)]
/// Box-owned by the enclosing args tree; dropping it also drops its values.
struct args_entry {
    flag: u_char,
    values: args_values,
    count: u_int,
    flags: ::core::ffi::c_int,
}

const ARGS_ENTRY_OPTIONAL_VALUE: ::core::ffi::c_int = 0x1 as ::core::ffi::c_int;

/// Owns flag-value storage directly. Iteration borrows
/// the boxed records and cannot outlive the containing argument set.
type args_values = Vec<Box<args_value>>;

type args_parse_cb = Option<fn(&mut args, u_int) -> Result<args_parse_type, ArgsParseError>>;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct args_parse {
    pub template: &'static CStr,
    pub lower: ::core::ffi::c_int,
    pub upper: ::core::ffi::c_int,
    pub cb: args_parse_cb,
}

/// Box-owned prepared command; retained model references have typed owners.
pub struct args_command_state {
    cmdlist: Option<Rc<std::cell::RefCell<cmd_list>>>,
    client: Option<ClientRef>,
    cmd: Option<std::ffi::CString>,
    pi: cmd_parse_input,
    file: Option<std::ffi::CString>,
}

impl args_command_state {
    fn empty() -> Self {
        Self {
            cmdlist: None,
            client: None,
            cmd: Default::default(),
            pi: Default::default(),
            file: Default::default(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ArgumentValueError {
    Missing,
    Empty,
    Invalid,
    TooSmall,
    TooLarge,
}

fn parse_flag_error(prefix: &[u8], flag: u_char, suffix: &[u8]) -> CString {
    let mut bytes = Vec::with_capacity(prefix.len() + 1 + suffix.len());
    bytes.extend_from_slice(prefix);
    bytes.push(flag);
    bytes.extend_from_slice(suffix);
    CString::new(bytes).expect("argument flag diagnostics contain no NUL")
}

fn parse_number_error(message: String) -> CString {
    CString::new(message).expect("argument diagnostics contain no NUL")
}

impl ArgumentValueError {
    pub fn message(self) -> &'static CStr {
        unsafe {
            CStr::from_bytes_with_nul_unchecked(match self {
                Self::Missing => b"missing\0",
                Self::Empty => b"empty\0",
                Self::Invalid => b"invalid\0",
                Self::TooSmall => b"too small\0",
                Self::TooLarge => b"too large\0",
            })
        }
    }
}
/// Iterate flags in tmux's byte ordering. Zero remains the legacy end sentinel.
pub fn args_flags(args: &args) -> impl Iterator<Item = u_char> + '_ {
    args.tree.keys().copied().take_while(|flag| *flag != 0)
}

/// Borrow repeated flag values in the order they were supplied.
pub fn args_flag_values(args: &args, flag: u_char) -> impl Iterator<Item = &args_value> {
    args.tree
        .get(&flag)
        .into_iter()
        .flat_map(|entry| entry.values.iter().map(Box::as_ref))
}

fn args_last_value(args: &args, flag: u_char) -> Option<&args_value> {
    args.tree.get(&flag)?.values.last().map(Box::as_ref)
}

fn args_last_string(args: &args, flag: u_char) -> Option<&CStr> {
    args_last_value(args, flag)?.as_string()
}
fn args_copy_value(from: &ArgumentValue<'_>) -> args_value {
    from.to_owned()
}
fn args_type_to_string(kind: args_type) -> &'static CStr {
    match kind {
        ARGS_NONE => c"NONE",
        ARGS_STRING => c"STRING",
        ARGS_COMMANDS => c"COMMANDS",
        _ => c"INVALID",
    }
}
unsafe fn args_value_for_log<'a>(value: &'a ArgumentValue<'_>) -> Cow<'a, CStr> {
    match value.type_0() as ::core::ffi::c_uint {
        0 => Cow::Borrowed(c""),
        1 => Cow::Borrowed(value.as_string().expect("string argument")),
        2 => Cow::Owned(cmd_list_print_cstring(
            &value.as_commands().expect("command argument").borrow(),
            0,
        )),
        _ => fatalx(|out| out.write_all(b"unexpected argument type")),
    }
}
pub fn args_create() -> Box<args> {
    Box::new(args {
        tree: Default::default(),
        values: Vec::new(),
    })
}
fn args_push_positional_owned(args: &mut args, value: args_value) {
    args.values.push(value);
}

unsafe fn args_parse_flag_argument(
    values: &[ArgumentValue<'_>],
    args: &mut args,
    i: &mut usize,
    suffix: &[u8],
    flag: u_char,
    optional_argument: bool,
) -> Result<(), CString> {
    let new = if !suffix.is_empty() {
        args_value::string(CString::new(suffix).expect("flag suffix contains no NUL"))
    } else {
        let argument = values.get(*i);
        if argument.is_some_and(|value| value.type_0() != ARGS_STRING) {
            return Err(parse_flag_error(b"-", flag, b" argument must be a string"));
        }
        let skip_optional = optional_argument
            && argument.is_none_or(|value| {
                let bytes = value.as_string().expect("string argument").to_bytes();
                bytes.first() == Some(&b'-')
                    && bytes.get(1).is_some_and(|next| {
                        *next == b'-' || ::libc::isalpha(*next as ::core::ffi::c_int) != 0
                    })
            });
        if skip_optional {
            log_debug(format_args!(
                "args_parse_flag_argument: -{} (optional)",
                log_byte(flag)
            ));
            args_set_value(args, flag, None, ARGS_ENTRY_OPTIONAL_VALUE);
            return Ok(());
        }
        let argument =
            argument.ok_or_else(|| parse_flag_error(b"-", flag, b" expects an argument"))?;
        *i += 1;
        args_copy_value(argument)
    };
    let printed = args_value_for_log(&new);
    log_debug(format_args!(
        "args_parse_flag_argument: -{} = {}",
        log_byte(flag),
        log_bytes(printed.to_bytes())
    ));
    args_set_value(args, flag, Some(new), 0);
    Ok(())
}

/// Return true when flags end, leaving the first positional value unconsumed.
unsafe fn args_parse_flags(
    parse: &args_parse,
    values: &[ArgumentValue<'_>],
    args: &mut args,
    i: &mut usize,
) -> Result<bool, ArgsParseError> {
    let value = &values[*i];
    if value.type_0() != ARGS_STRING {
        return Ok(true);
    }
    let string = value.as_string().expect("string argument").to_bytes();
    log_debug(format_args!("args_parse_flags: next {}", log_bytes(string)));
    if string.first() != Some(&b'-') || string.len() == 1 {
        return Ok(true);
    }
    *i += 1;
    if string == b"--" {
        return Ok(true);
    }
    let template = parse.template.to_bytes();
    for (offset, &flag) in string.iter().enumerate().skip(1) {
        if flag == b'?' {
            return Err(ArgsParseError::Usage);
        }
        if ::libc::isalnum(flag as ::core::ffi::c_int) == 0 {
            return Err(ArgsParseError::Message(parse_flag_error(
                b"invalid flag -",
                flag,
                b"",
            )));
        }
        let found = template
            .iter()
            .position(|byte| *byte == flag)
            .ok_or_else(|| {
                ArgsParseError::Message(parse_flag_error(b"unknown flag -", flag, b""))
            })?;
        if template.get(found + 1) != Some(&b':') {
            log_debug(format_args!("args_parse_flags: -{}", log_byte(flag)));
            args_set_value(args, flag, None, 0);
        } else {
            return args_parse_flag_argument(
                values,
                args,
                i,
                &string[offset + 1..],
                flag,
                template.get(found + 2) == Some(&b':'),
            )
            .map(|()| false)
            .map_err(ArgsParseError::Message);
        }
    }
    Ok(false)
}

pub unsafe fn args_parse(
    parse: &args_parse,
    values: &[ArgumentValue<'_>],
) -> Result<Box<args>, ArgsParseError> {
    let mut args = args_create();
    if values.is_empty() {
        return Ok(args);
    }
    let mut i = 1;
    while i < values.len() {
        if args_parse_flags(parse, values, &mut args, &mut i)? {
            break;
        }
    }
    log_debug(format_args!(
        "args_parse: flags end at {} of {}",
        i,
        values.len()
    ));
    for (index, value) in values.iter().enumerate().skip(i) {
        let printed = args_value_for_log(value);
        log_debug(format_args!(
            "args_parse: {} = {} (type {})",
            index,
            log_bytes(printed.to_bytes()),
            log_bytes(args_type_to_string(value.type_0()).to_bytes())
        ));
        let kind = if let Some(callback) = parse.cb {
            let index = args.values.len() as u_int;
            callback(&mut args, index)?
        } else {
            ARGS_PARSE_STRING
        };
        match kind {
            ARGS_PARSE_INVALID => return Err(ArgsParseError::Usage),
            ARGS_PARSE_STRING if value.type_0() != ARGS_STRING => {
                return Err(ArgsParseError::Message(parse_number_error(format!(
                    "argument {} must be \"string\"",
                    (args.values.len() as u_int).wrapping_add(1)
                ))));
            }
            ARGS_PARSE_COMMANDS if value.type_0() != ARGS_COMMANDS => {
                return Err(ArgsParseError::Message(parse_number_error(format!(
                    "argument {} must be {{ commands }}",
                    (args.values.len() as u_int).wrapping_add(1)
                ))));
            }
            _ => {}
        }
        let copied = match kind {
            ARGS_PARSE_STRING | ARGS_PARSE_COMMANDS_OR_STRING | ARGS_PARSE_COMMANDS => {
                args_copy_value(value)
            }
            _ => args_value::empty(),
        };
        args_push_positional_owned(&mut args, copied);
    }
    if parse.lower != -1 && (args.values.len() as u_int) < parse.lower as u_int {
        return Err(ArgsParseError::Message(parse_number_error(format!(
            "too few arguments (need at least {})",
            parse.lower as u_int
        ))));
    }
    if parse.upper != -1 && (args.values.len() as u_int) > parse.upper as u_int {
        return Err(ArgsParseError::Message(parse_number_error(format!(
            "too many arguments (need at most {})",
            parse.upper as u_int
        ))));
    }
    Ok(args)
}
unsafe fn args_copy_copy_value(from: &args_value, argv: &Vec<CString>) -> args_value {
    match from.type_0() as ::core::ffi::c_uint {
        1 => {
            let source = from.as_string().expect("string argument");
            if argv.is_empty() {
                return args_value::string(source.to_owned());
            }
            let mut expanded = cmd_template_replace_cstring(source, argv[0].as_c_str(), 1);
            for (i, argument) in argv.iter().enumerate().skip(1) {
                expanded = cmd_template_replace_cstring(
                    expanded.as_c_str(),
                    argument.as_c_str(),
                    (i + 1) as ::core::ffi::c_int,
                );
            }
            args_value::string(expanded)
        }
        2 => args_value::commands(cmd_list_copy(
            &from.as_commands().expect("command argument").borrow(),
            argv,
        )),
        _ => args_value::empty(),
    }
}
pub unsafe fn args_copy(args: &args, argv: &Vec<CString>) -> Box<args> {
    cmd_log_argv(argv, c"args_copy");
    let mut new_args = args_create();
    for entry in args.tree.values() {
        if entry.values.is_empty() {
            for _ in 0..entry.count {
                args_set_flag(&mut *new_args, entry.flag, 0);
            }
        } else {
            for value in &entry.values {
                args_set_value(
                    &mut new_args,
                    entry.flag,
                    Some(args_copy_copy_value(value, argv)),
                    0,
                );
            }
        }
    }
    for value in &args.values {
        args_push_positional_owned(&mut new_args, args_copy_copy_value(value, argv));
    }
    new_args
}
pub unsafe fn args_to_vector(args: &args) -> Vec<CString> {
    let mut argv = Vec::new();
    for value in args.values.iter() {
        match value.type_0() as ::core::ffi::c_uint {
            1 => {
                argv.push(value.as_string().expect("string argument").to_owned());
            }
            2 => {
                let printed = cmd_list_print_cstring(
                    &value.as_commands().expect("command argument").borrow(),
                    0 as ::core::ffi::c_int,
                );
                argv.push(printed);
            }
            _ => {}
        }
    }
    argv
}
unsafe fn args_print_add_value(buf: &mut Vec<u8>, value: &args_value) {
    if !buf.is_empty() {
        buf.push(b' ');
    }
    match value.type_0() as ::core::ffi::c_uint {
        2 => {
            let expanded =
                cmd_list_print_cstring(&value.as_commands().expect("command argument").borrow(), 0);
            buf.extend_from_slice(b"{ ");
            buf.extend_from_slice(expanded.as_bytes());
            buf.extend_from_slice(b" }");
        }
        1 => {
            let expanded = args_escape_cstring(value.as_string().expect("string argument"));
            buf.extend_from_slice(expanded.as_bytes());
        }
        _ => {}
    }
}

pub(crate) unsafe fn args_print_cstring(args: &args) -> CString {
    let mut buf = Vec::new();
    for entry in args.tree.values() {
        if entry.flags & ARGS_ENTRY_OPTIONAL_VALUE == 0 && entry.values.is_empty() {
            if buf.is_empty() {
                buf.push(b'-');
            }
            for _ in 0..entry.count {
                if entry.flag != 0 {
                    buf.push(entry.flag);
                }
            }
        }
    }
    let mut last_optional = false;
    for entry in args.tree.values() {
        if entry.flags & ARGS_ENTRY_OPTIONAL_VALUE != 0 {
            if !buf.is_empty() {
                buf.push(b' ');
            }
            buf.push(b'-');
            if entry.flag != 0 {
                buf.push(entry.flag);
            }
            last_optional = true;
        } else if !entry.values.is_empty() {
            for value in &entry.values {
                if !buf.is_empty() {
                    buf.push(b' ');
                }
                buf.push(b'-');
                if entry.flag != 0 {
                    buf.push(entry.flag);
                }
                args_print_add_value(&mut buf, value);
            }
            last_optional = false;
        }
    }
    if last_optional {
        buf.extend_from_slice(b" --");
    }
    for value in &args.values {
        args_print_add_value(&mut buf, value);
    }
    CString::new(buf).expect("printed arguments contain no interior NUL")
}
pub(crate) unsafe fn args_escape_cstring(s: &CStr) -> CString {
    let source = s.to_bytes();
    if source.is_empty() {
        return CString::new(b"''".to_vec()).expect("literal has no NUL");
    }

    let quotes = if source.iter().any(|byte| b" #';${}%".contains(byte)) {
        b'"'
    } else if source.iter().any(|byte| b" \"".contains(byte)) {
        b'\''
    } else {
        0
    };
    if source.len() == 1 && source[0] != b' ' && (quotes != 0 || source[0] == b'~') {
        return CString::new(vec![b'\\', source[0]]).expect("source has no NUL");
    }

    let mut flags = VIS_OCTAL | VIS_CSTYLE | VIS_TAB | VIS_NL;
    if quotes == b'"' {
        flags |= VIS_DQ;
    }
    let mut escaped = vec![
        0;
        source
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(1))
            .expect("escaped argument too long")
    ];
    let length = utf8_strvis(&mut escaped, source, flags);
    escaped.truncate(length);

    let mut result = Vec::with_capacity(escaped.len() + 3);
    if quotes == b'\'' {
        result.push(b'\'');
        result.extend_from_slice(&escaped);
        result.push(b'\'');
    } else if quotes == b'"' {
        result.push(b'"');
        if escaped.first() == Some(&b'~') {
            result.push(b'\\');
        }
        result.extend_from_slice(&escaped);
        result.push(b'"');
    } else {
        if escaped.first() == Some(&b'~') {
            result.push(b'\\');
        }
        result.extend_from_slice(&escaped);
    }
    CString::new(result).expect("utf8_strvis output has no interior NUL")
}

pub unsafe fn args_has(args: *mut args, flag: u_char) -> ::core::ffi::c_int {
    (&*args)
        .tree
        .get(&flag)
        .map_or(0, |entry| entry.count as ::core::ffi::c_int)
}
fn args_set_value(
    args: &mut args,
    flag: u_char,
    value: Option<args_value>,
    flags: ::core::ffi::c_int,
) {
    let entry = args.tree.entry(flag).or_insert_with(|| {
        Box::new(args_entry {
            flag,
            values: Default::default(),
            count: 0,
            flags,
        })
    });
    entry.count = entry.count.wrapping_add(1);
    if let Some(value) = value {
        if value.type_0() != ARGS_NONE {
            entry.values.push(Box::new(value));
        }
    }
}

pub unsafe fn args_set_owned_string(args: *mut args, value: CString) {
    let flag: u_char = 'f' as i32 as u_char;
    let flags: ::core::ffi::c_int = 0;
    args_set_value(&mut *args, flag, Some(args_value::string(value)), flags);
}

/// Add an occurrence of a flag that has no associated value.
unsafe fn args_set_flag(args: *mut args, flag: u_char, flags: ::core::ffi::c_int) {
    args_set_value(&mut *args, flag, None, flags);
}

pub fn args_get(args: &args, flag: u_char) -> Option<&CStr> {
    args_last_value(args, flag).and_then(args_value::as_string)
}

pub unsafe fn args_count(mut args: *mut args) -> u_int {
    (*args).values.len() as u_int
}
pub fn args_string(args: &mut args, idx: u_int) -> Option<&CStr> {
    if idx >= (args.values.len() as u_int) {
        return None;
    }
    let value = &mut args.values[idx as usize];
    match value.type_0() {
        ARGS_NONE => Some(c""),
        ARGS_STRING => value.as_string(),
        ARGS_COMMANDS => {
            if value.cached.is_none() {
                let printed = unsafe {
                    cmd_list_print_cstring(
                        &value.as_commands().expect("command argument").borrow(),
                        0,
                    )
                };
                value.cached = Some(printed);
            }
            value.cached.as_deref()
        }
        _ => unreachable!("unexpected argument type"),
    }
}
pub unsafe fn args_make_commands_now(
    self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    idx: u_int,
    expand: ::core::ffi::c_int,
) -> Option<Rc<std::cell::RefCell<cmd_list>>> {
    let mut state = args_make_commands_prepare(self_0.clone(), item_handle, idx, None, 0, expand);
    match args_make_commands(&mut state, &Vec::new()) {
        Ok(commands) => Some(commands),
        Err(error) => {
            cmdq_error(item_handle, |out| write_cstr(out, error.as_deref()));
            None
        }
    }
}
pub unsafe fn args_make_commands_prepare(
    mut self_0: refbox::Weak<cmd>,
    item_handle: &std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>,
    mut idx: u_int,
    default_command: Option<&'static CStr>,
    mut wait: ::core::ffi::c_int,
    mut expand: ::core::ffi::c_int,
) -> Box<args_command_state> {
    let item = item_handle.get();
    let mut args: *mut args =
        cmd_get_args_mut(self_0.get_mut_unchecked()).map_or(std::ptr::null_mut(), |args| args);
    let mut target: *mut cmd_find_state = crate::src::cmd::queue::cmdq_get_target_mut(&mut *item);
    let tc_owner = cmdq_get_target_client((item).as_ref());
    let mut state = Box::new(args_command_state::empty());
    let values = &(*args).values;
    let command = if let Some(value) = values.get(idx as usize) {
        if let Some(commands) = value.as_commands() {
            state.cmdlist = Some(commands.clone());
            return state;
        }
        value.as_string().expect("command string argument")
    } else {
        default_command.unwrap_or_else(|| fatalx(|out| out.write_all(b"argument out of range")))
    };
    if expand != 0 {
        state.cmd = Some(format_single_from_target_cstring(
            item_handle,
            command.as_ptr(),
        ));
    } else {
        state.cmd = Some(command.to_owned());
    }

    log_debug(format_args!(
        "{}: {}",
        "args_make_commands_prepare",
        log_cstr(state.cmd.as_deref().unwrap_or(c"(null)"))
    ));
    if wait != 0 {
        state.pi.set_item(Some(item_handle));
    }
    let (source, line) = cmd_get_source(self_0.get_unchecked());
    state.pi.line = line;
    if let Some(file) = source {
        state.file = Some(file.to_owned());
        state.pi.file = state.file.clone();
    }
    state.pi.c = tc_owner
        .as_ref()
        .map_or_else(std::rc::Weak::new, Rc::downgrade);
    state.client = tc_owner;
    state.pi.fs.copy_from(&*target);
    state
}
pub unsafe fn args_make_commands(
    state: &mut args_command_state,
    argv: &Vec<CString>,
) -> Result<Rc<std::cell::RefCell<cmd_list>>, Option<CString>> {
    let mut i: ::core::ffi::c_int = 0;
    if let Some(commands) = state.cmdlist.as_ref() {
        if argv.is_empty() {
            return Ok(Rc::clone(commands));
        }
        return Ok(cmd_list_copy(&commands.borrow(), argv));
    }
    let mut cmd = state.cmd.as_ref().expect("prepared command text").clone();
    log_debug(format_args!(
        "{}: {}",
        "args_make_commands",
        log_bytes(cmd.as_bytes())
    ));
    cmd_log_argv(argv, c"args_make_commands");
    i = 0 as ::core::ffi::c_int;
    while (i as usize) < argv.len() {
        let next = cmd_template_replace_cstring(
            cmd.as_c_str(),
            argv[i as usize].as_c_str(),
            i + 1 as ::core::ffi::c_int,
        );
        log_debug(format_args!(
            "{}: %{} {}: {}",
            "args_make_commands",
            (i + 1 as ::core::ffi::c_int) as u32,
            log_bytes(argv[i as usize].as_bytes()),
            log_bytes(next.as_bytes())
        ));
        cmd = next;
        i += 1;
    }
    log_debug(format_args!(
        "{}: {}",
        "args_make_commands",
        log_bytes(cmd.as_bytes())
    ));
    let mut pr = cmd_parse_from_string(cmd.as_c_str(), &raw mut state.pi);
    drop(cmd);
    match pr.status as ::core::ffi::c_uint {
        0 => Err(pr.error),
        1 => Ok(pr.cmdlist.take().expect("successful command parse")),
        _ => fatalx(|out| out.write_all(b"invalid parse return state")),
    }
}
impl Drop for args_command_state {
    fn drop(&mut self) {
        drop(self.cmdlist.take());
        if let Some(client) = self.client.take() {
            drop(client);
        }
    }
}

pub(crate) unsafe fn args_make_commands_get_command_cstring(state: &args_command_state) -> CString {
    if let Some(commands) = state.cmdlist.as_ref() {
        let commands = commands.borrow();
        let Some(first) = cmd_list_first(&commands) else {
            return CString::new(Vec::new()).expect("empty command name has no NUL");
        };
        return first.entry.name.to_owned();
    }
    let command = state
        .cmd
        .as_ref()
        .expect("prepared command text")
        .as_bytes();
    let n = command
        .iter()
        .position(|byte| b" ,".contains(byte))
        .unwrap_or(command.len()) as ::core::ffi::c_int;
    let prefix = if n < 0 {
        command
    } else {
        &command[..n as usize]
    };
    CString::new(prefix).expect("command prefix has no NUL")
}

fn strtonum_error(errstr: &CStr) -> ArgumentValueError {
    match errstr.to_bytes() {
        b"invalid" => ArgumentValueError::Invalid,
        b"too small" => ArgumentValueError::TooSmall,
        b"too large" => ArgumentValueError::TooLarge,
        _ => ArgumentValueError::Invalid,
    }
}

fn parse_number(value: &CStr, minval: i64, maxval: i64) -> Result<i64, ArgumentValueError> {
    unsafe { strtonum(value, minval, maxval) }.map_err(strtonum_error)
}

fn percentage_share(
    curval: i64,
    percentage: i64,
    minval: i64,
    maxval: i64,
) -> Result<i64, ArgumentValueError> {
    let value = (curval as i128 * percentage as i128) / 100;
    if value < minval as i128 {
        return Err(ArgumentValueError::TooSmall);
    }
    if value > maxval as i128 {
        return Err(ArgumentValueError::TooLarge);
    }
    Ok(value as i64)
}

fn parse_percentage(
    value: &CStr,
    minval: i64,
    maxval: i64,
    curval: i64,
) -> Result<i64, ArgumentValueError> {
    let bytes = value.to_bytes();
    if bytes.is_empty() {
        return Err(ArgumentValueError::Empty);
    }
    if let Some(percentage) = bytes.strip_suffix(b"%") {
        let percentage = CString::new(percentage).map_err(|_| ArgumentValueError::Invalid)?;
        let percentage = parse_number(percentage.as_c_str(), 0, 1000)?;
        return percentage_share(curval, percentage, minval, maxval);
    }
    parse_number(value, minval, maxval)
}

/// Converts the last string value stored for `flag` to a bounded integer.
///
/// # Safety
/// `args` must point to a valid argument store.
pub unsafe fn args_strtonum_result(
    args: *mut args,
    flag: u_char,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
) -> Result<i64, ArgumentValueError> {
    let value = args_last_string(&*args, flag).ok_or(ArgumentValueError::Missing)?;
    parse_number(value, minval, maxval)
}

/// Converts the last string value after format expansion to a bounded integer.
///
/// # Safety
/// `args` must point to a valid argument store and `item` must be valid for
/// format expansion.
pub unsafe fn args_strtonum_and_expand_result(
    args: *mut args,
    flag: u_char,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
    item_handle: Option<&std::rc::Rc<std::cell::UnsafeCell<cmdq_item>>>,
) -> Result<i64, ArgumentValueError> {
    let value = args_last_string(&*args, flag).ok_or(ArgumentValueError::Missing)?;
    let formatted = format_single_from_target_cstring(
        (item_handle).expect("command queue item"),
        value.as_ptr(),
    );
    parse_number(formatted.as_c_str(), minval, maxval)
}

/// Converts the last stored string as an integer or percentage.
///
/// # Safety
/// `args` must point to a valid argument store.
pub unsafe fn args_percentage_result(
    args: *mut args,
    flag: u_char,
    minval: ::core::ffi::c_longlong,
    maxval: ::core::ffi::c_longlong,
    curval: ::core::ffi::c_longlong,
) -> Result<i64, ArgumentValueError> {
    if !(&*args).tree.contains_key(&flag) {
        return Err(ArgumentValueError::Missing);
    }
    let value = args_last_value(&*args, flag).ok_or(ArgumentValueError::Empty)?;
    if value.type_0() as ::core::ffi::c_uint
        != ARGS_STRING as ::core::ffi::c_int as ::core::ffi::c_uint
        || value.as_string().is_none()
    {
        return Err(ArgumentValueError::Missing);
    }
    parse_percentage(
        value.as_string().expect("string argument"),
        minval,
        maxval,
        curval,
    )
}

#[cfg(test)]
mod tests;
