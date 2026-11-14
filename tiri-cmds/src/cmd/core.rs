use tiri_core::arguments::{
    ArgsParseError, ParseValue, args, args_copy, args_parse, args_print_cstring,
};
use tiri_core::log::{log_bytes, log_debug};
use tiri_core::options::OptionsScope;

use crate::shared::command::{CMD_LIST_PRINT_ESCAPED, CMD_LIST_PRINT_NO_GROUPS};
use crate::shared::command::{CMD_READONLY, CMD_STARTSERVER};
use crate::shared::command::{cmd, cmd_entry, cmd_list};
use std::ffi::{CStr, CString};
use tiri_core::shared::abi::u_int;

pub const DQ: C2RustUnnamed_38 = 2;
pub type C2RustUnnamed_38 = ::core::ffi::c_uint;
pub const SQ: C2RustUnnamed_38 = 1;
pub const NQ: C2RustUnnamed_38 = 0;

pub fn cmd_get_entry(cmd: &cmd) -> &'static cmd_entry {
    cmd.entry
}
pub fn cmd_get_args(cmd: &cmd) -> &args {
    &cmd.args
}
pub fn cmd_get_source(cmd: &cmd) -> (Option<&CStr>, u32) {
    (cmd.file.as_deref(), cmd.line)
}
pub fn cmd_get_parse_flags(cmd: &cmd) -> ::core::ffi::c_int {
    cmd.parse_flags
}
pub fn cmd_get_alias(name: &CStr) -> Option<CString> {
    OptionsScope::GlobalServer
        .with_entry(c"command-alias", |entry| {
            entry
                .value
                .array()
                .into_iter()
                .flat_map(|array| array.iter())
                .find_map(|item| {
                    let value = item.value.string_ptr()?.to_bytes();
                    let (alias, command) =
                        value.split_at(value.iter().position(|&byte| byte == b'=')?);
                    (alias == name.to_bytes()).then(|| {
                        CString::new(&command[1..]).expect("alias command contains no NUL")
                    })
                })
        })
        .flatten()
}
pub fn cmd_find(name: &CStr) -> Result<&'static cmd_entry, CString> {
    let mut found = None;
    let mut ambiguous = false;
    for &entry in super::cmd_table() {
        if entry.alias == Some(name) {
            ambiguous = false;
            found = Some(entry);
            break;
        }
        if entry.name.to_bytes().starts_with(name.to_bytes()) {
            if found.is_some() {
                ambiguous = true;
            }
            found = Some(entry);
            if entry.name == name {
                break;
            }
        }
    }
    if ambiguous {
        let mut candidates = [0u8; 8192];
        let mut len = 0;
        'entries: for &entry in super::cmd_table() {
            if !entry.name.to_bytes().starts_with(name.to_bytes()) {
                continue;
            }
            for bytes in [entry.name.to_bytes(), b", ".as_slice()] {
                let available = candidates.len() - 1 - len;
                let count = bytes.len().min(available);
                candidates[len..len + count].copy_from_slice(&bytes[..count]);
                len += count;
                if count < bytes.len() {
                    break 'entries;
                }
            }
        }
        let mut error = b"ambiguous command: ".to_vec();
        error.extend_from_slice(name.to_bytes());
        error.extend_from_slice(b", could be: ");
        error.extend_from_slice(&candidates[..len - 2]);
        return Err(CString::new(error).expect("command diagnostic contains no NUL"));
    }
    found.ok_or_else(|| {
        let mut error = b"unknown command: ".to_vec();
        error.extend_from_slice(name.to_bytes());
        CString::new(error).expect("command diagnostic contains no NUL")
    })
}

pub fn cmd_parse(
    values: &[ParseValue<'_>],
    file: Option<&CStr>,
    line: u_int,
    parse_flags: ::core::ffi::c_int,
) -> Result<cmd, CString> {
    let Some(&ParseValue::String(command)) = values.first() else {
        return Err(CString::new("no command").unwrap());
    };
    let entry = cmd_find(command)?;
    let args = match args_parse(&entry.args, values) {
        Ok(args) => args,
        Err(ArgsParseError::Usage) => {
            let mut error = b"usage: ".to_vec();
            error.extend_from_slice(entry.name.to_bytes());
            error.push(b' ');
            error.extend_from_slice(entry.usage.to_bytes());
            return Err(CString::new(error).expect("command diagnostic contains no NUL"));
        }
        Err(ArgsParseError::Message(message)) => {
            let mut error = b"command ".to_vec();
            error.extend_from_slice(entry.name.to_bytes());
            error.extend_from_slice(b": ");
            error.extend_from_slice(message.as_bytes());
            return Err(CString::new(error).expect("command diagnostic contains no NUL"));
        }
    };
    Ok(cmd {
        entry,
        args,
        file: file.map(CStr::to_owned),
        line,
        parse_flags,
    })
}
pub fn cmd_copy(cmd: &cmd, argv: &Vec<CString>) -> cmd {
    cmd {
        entry: cmd.entry,
        args: args_copy(&cmd.args, argv),
        file: cmd.file.clone(),
        line: cmd.line,
        parse_flags: 0,
    }
}

pub(crate) fn cmd_print_cstring(cmd: &cmd) -> CString {
    let args = args_print_cstring(&cmd.args);
    let arguments = args.as_bytes();
    let name = cmd.entry.name.to_bytes();
    let mut buf = Vec::with_capacity(
        name.len()
            + if arguments.is_empty() {
                0
            } else {
                arguments.len() + 1
            },
    );
    buf.extend_from_slice(name);
    if !arguments.is_empty() {
        buf.push(b' ');
        buf.extend_from_slice(arguments);
    }
    CString::new(buf).expect("command print contains no interior NUL")
}
pub fn cmd_list_new() -> cmd_list {
    cmd_list::default()
}
pub fn cmd_list_copy(cmdlist: &dyn tiri_core::actions::Program, argv: &Vec<CString>) -> cmd_list {
    let cmdlist = cmdlist
        .as_any()
        .downcast_ref::<cmd_list>()
        .expect("command program");
    let s = cmd_list_print_cstring(cmdlist, 0);
    log_debug(format_args!(
        "{}: {}",
        "cmd_list_copy",
        log_bytes(s.as_bytes())
    ));
    let owner = cmd_list {
        groups: cmdlist
            .groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|command| cmd_copy(command, argv))
                    .collect()
            })
            .collect(),
    };
    let s = cmd_list_print_cstring(&owner, 0);
    log_debug(format_args!(
        "{}: {}",
        "cmd_list_copy",
        log_bytes(s.as_bytes())
    ));
    owner
}
fn print_list(cmdlist: &cmd_list, flags: i32) -> CString {
    let mut buf = Vec::new();
    let mut commands = cmdlist
        .groups
        .iter()
        .enumerate()
        .flat_map(|(group, commands)| commands.iter().map(move |command| (group, command)))
        .peekable();
    while let Some((group, cmd)) = commands.next() {
        let this = cmd_print_cstring(cmd);
        buf.extend_from_slice(this.as_bytes());

        if let Some((next_group, _)) = commands.peek() {
            let grouped = flags & CMD_LIST_PRINT_NO_GROUPS == 0 && group != *next_group;
            let separator: &[u8] = match (flags & CMD_LIST_PRINT_ESCAPED != 0, grouped) {
                (false, false) => b" ; ",
                (false, true) => b" ;; ",
                (true, false) => b" \\; ",
                (true, true) => b" \\;\\; ",
            };
            buf.extend_from_slice(separator);
        }
    }
    CString::new(buf).expect("command list contains no interior NUL")
}

pub fn cmd_list_first(cmdlist: &dyn tiri_core::actions::Program) -> Option<&cmd> {
    let cmdlist = cmdlist
        .as_any()
        .downcast_ref::<cmd_list>()
        .expect("command program");
    cmdlist.groups.iter().flatten().next()
}
fn list_all_have(cmdlist: &cmd_list) -> ::core::ffi::c_int {
    let mut flag: ::core::ffi::c_int = CMD_READONLY;
    for cmd in cmdlist.groups.iter().flatten() {
        if !cmd.entry.flags & flag != 0 {
            return 0 as ::core::ffi::c_int;
        }
    }
    1 as ::core::ffi::c_int
}
fn list_any_have(cmdlist: &cmd_list) -> ::core::ffi::c_int {
    let mut flag: ::core::ffi::c_int = CMD_STARTSERVER;
    for cmd in cmdlist.groups.iter().flatten() {
        if cmd.entry.flags & flag != 0 {
            return 1 as ::core::ffi::c_int;
        }
    }
    0 as ::core::ffi::c_int
}

impl tiri_core::actions::Program for cmd_list {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn text(&self, flags: i32) -> CString {
        let text = print_list(self, flags & !tiri_core::actions::PROGRAM_ARGUMENT);
        if flags & tiri_core::actions::PROGRAM_ARGUMENT != 0 {
            CString::new([b"{ ".as_slice(), text.as_bytes(), b" }"].concat())
                .expect("command block has no NUL")
        } else {
            text
        }
    }
    fn substitute(&self, arguments: &Vec<CString>) -> tiri_core::actions::ProgramRef {
        std::rc::Rc::new(cmd_list_copy(self, arguments))
    }
    fn read_only(&self) -> bool {
        list_all_have(self) != 0
    }
    fn starts_server(&self) -> bool {
        list_any_have(self) != 0
    }
}
pub(crate) fn cmd_list_print_cstring(
    program: &dyn tiri_core::actions::Program,
    flags: i32,
) -> CString {
    program.text(flags & !tiri_core::actions::PROGRAM_ARGUMENT)
}
pub fn cmd_list_any_have(program: &dyn tiri_core::actions::Program) -> i32 {
    program.starts_server() as i32
}
