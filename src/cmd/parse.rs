use crate::arguments::ParseValue;
use crate::cmd::queue::{cmdq_append, cmdq_get_command, cmdq_print};
use crate::cmd::{
    cmd_get_alias, cmd_list_append, cmd_list_append_all, cmd_list_move, cmd_list_new,
    cmd_list_print_cstring, cmd_parse,
};
use crate::environ::environ_put;
use crate::format::{format_create, format_defaults, format_expand_cstring, format_true};
use crate::log::{log_bytes, log_debug};
use crate::shared::abi::*;
use crate::shared::client::ClientRef;
use crate::shared::command::*;
use crate::shared::command::{cmd_find_state, cmd_list, cmdq_state};
use crate::shared::command::{cmd_parse_input, cmd_parse_result};
use crate::shared::command::{
    CMD_PARSE_MAX_ENVIRON_LEN, CMD_PARSE_NOALIAS, CMD_PARSE_ONEGROUP, CMD_PARSE_PARSEONLY,
    CMD_PARSE_VERBOSE,
};
use crate::shared::environment::ENVIRON_HIDDEN;
use crate::shared::format::{FORMAT_NOJOBS, FORMAT_NONE};
use crate::shared::limits::UINT_MAX;
use crate::tmux::global_environ_mut;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::rc::Rc;

/// The command set and its children retain their separate heap allocations.
struct cmd_parse_commands {
    items: Vec<cmd_parse_command>,
}

struct cmd_parse_command {
    line: u_int,
    arguments: cmd_parse_arguments,
}

/// Inline in a command, owning each separately allocated argument.
type cmd_parse_arguments = VecDeque<Box<cmd_parse_argument>>;

/// Each variant owns exactly the payload that its parser argument needs.
enum cmd_parse_argument {
    String(CString),
    Commands(Box<cmd_parse_commands>),
    ParsedCommands(Rc<std::cell::RefCell<cmd_list>>),
}
mod lexer;
use lexer::Lexer;

/// Shared only for the duration of a parse. Grammar actions and the lazy lexer
/// borrow it in turn, so assignments remain visible to subsequent words.
struct ParseSession<'a> {
    input: &'a mut cmd_parse_input,
    error: Option<CString>,
}

impl ParseSession<'_> {
    fn record_error(&mut self, error: &CStr) {
        if self.error.is_none() {
            self.error = Some(cmd_parse_get_error(
                self.input.file.as_deref(),
                self.input.line,
                error,
            ));
        }
    }
}

fn cmd_parse_get_error(file: Option<&CStr>, line: u_int, error: &CStr) -> CString {
    let Some(file) = file else {
        return error.to_owned();
    };
    let mut bytes = file.to_bytes().to_vec();
    bytes.push(b':');
    bytes.extend_from_slice(line.to_string().as_bytes());
    bytes.extend_from_slice(b": ");
    bytes.extend_from_slice(error.to_bytes());
    CString::new(bytes).expect("parser diagnostic contains no NUL")
}

pub fn cmd_parse_error_uppercase_first(error: &mut Option<CString>) {
    let Some(cause) = error.as_mut() else {
        return;
    };
    let mut bytes = cause.as_bytes_with_nul().to_vec();
    if let Some(first) = bytes.first_mut() {
        *first = first.to_ascii_uppercase();
    }
    *cause = CString::from_vec_with_nul(bytes).expect("cause remains NUL terminated");
}
fn cmd_parse_print_commands(pi: &cmd_parse_input, cmdlist: &cmd_list) {
    if !pi.flags & CMD_PARSE_VERBOSE != 0 {
        return;
    }
    let Some(item_owner) = pi.item.upgrade() else {
        return;
    };
    let s = cmd_list_print_cstring(cmdlist, 0);
    if pi.file.is_some() {
        cmdq_print(&item_owner, |out| {
            out.write_all(pi.file.as_ref().expect("parse file").to_bytes())?;
            write!(out, ":{}: ", { pi.line })?;
            out.write_all(s.as_bytes())
        });
    } else {
        cmdq_print(&item_owner, |out| {
            write!(out, "{}: ", { pi.line })?;
            out.write_all(s.as_bytes())
        });
    }
}
fn cmd_parse_new_command(line: u_int) -> cmd_parse_command {
    cmd_parse_command {
        line,
        arguments: VecDeque::new(),
    }
}

fn cmd_parse_new_commands() -> Box<cmd_parse_commands> {
    Box::new(cmd_parse_commands { items: Vec::new() })
}

struct ParserContext<'a, 'input>(&'a RefCell<ParseSession<'input>>);

impl tiri_cmdparse::Context for ParserContext<'_, '_> {
    fn line(&self) -> u32 {
        self.0.borrow().input.line
    }

    fn expand_format(&mut self, token: tiri_cmdparse::TokenText) -> tiri_cmdparse::TokenText {
        let mut session = self.0.borrow_mut();
        let pi = &mut *session.input;
        let client_owner = pi.c.upgrade();
        let mut fs: cmd_find_state = Default::default();
        let fsp = if pi.fs.is_valid() {
            &pi.fs
        } else {
            fs = cmd_find_state::find_from_client(client_owner.as_ref(), 0)
                .unwrap_or_else(|| cmd_find_state::clear_state(0));
            &fs
        };
        let item_owner = pi.item.upgrade();
        let mut ft_owner = format_create(
            client_owner.as_ref(),
            item_owner.as_ref(),
            FORMAT_NONE,
            FORMAT_NOJOBS,
        );
        let ft = &mut *ft_owner;
        format_defaults(
            ft,
            client_owner.as_ref(),
            fsp.session_handle().as_ref(),
            (fsp.winlink_handle()).clone(),
            fsp.pane_handle().as_ref(),
        );
        let expanded = format_expand_cstring(ft, token.as_c_str());
        drop(ft_owner);
        take_parser_token(expanded)
    }

    fn put_environ(
        &mut self,
        token: tiri_cmdparse::TokenText,
        hidden: bool,
        active: bool,
    ) -> Result<(), tiri_cmdparse::LexError> {
        {
            if token.as_c_str().to_bytes().len() > CMD_PARSE_MAX_ENVIRON_LEN as usize {
                self.0
                    .borrow_mut()
                    .record_error(c"environment variable is too long");
                return Err(tiri_cmdparse::LexError);
            }
            if self.0.borrow().input.flags & CMD_PARSE_PARSEONLY == 0 && active {
                global_environ_mut(|environment| {
                    environ_put(
                        environment,
                        token.as_c_str(),
                        if hidden { ENVIRON_HIDDEN } else { 0 },
                    )
                });
            }
        }
        Ok(())
    }

    fn is_true(&self, token: &tiri_cmdparse::TokenText) -> bool {
        format_true(token.as_c_str())
    }
}

fn take_parser_token(text: CString) -> tiri_cmdparse::TokenText {
    tiri_cmdparse::TokenText::from_cstring(text)
}

fn build_parser_commands(commands: Vec<tiri_cmdparse::ParseCommand>) -> Box<cmd_parse_commands> {
    let mut output = cmd_parse_new_commands();
    for command in commands {
        let mut cmd = cmd_parse_new_command(command.line);
        for argument in command.arguments {
            let arg = match argument {
                tiri_cmdparse::ParseArgument::String(text) => {
                    cmd_parse_argument::String(text.as_c_str().to_owned())
                }
                tiri_cmdparse::ParseArgument::Commands(commands) => {
                    cmd_parse_argument::Commands(build_parser_commands(commands))
                }
            };
            cmd.arguments.push_back(Box::new(arg));
        }
        output.items.push(cmd);
    }
    output
}

fn cmd_parse_run_parser(
    session: &RefCell<ParseSession<'_>>,
    lexer: Lexer<'_, '_>,
) -> Result<Box<cmd_parse_commands>, CString> {
    match tiri_cmdparse::parse(&mut ParserContext(session), lexer) {
        Ok(commands) => Ok(build_parser_commands(commands)),
        Err(_) => {
            let mut session = session.borrow_mut();
            session.record_error(c"syntax error");
            Err(session.error.take().expect("parser error was recorded"))
        }
    }
}
fn cmd_parse_do_file(
    bytes: &[u8],
    pi: &mut cmd_parse_input,
) -> Result<Box<cmd_parse_commands>, CString> {
    let session = RefCell::new(ParseSession {
        input: pi,
        error: None,
    });
    cmd_parse_run_parser(&session, Lexer::from_file(bytes, &session))
}

fn cmd_parse_do_buffer(
    bytes: &[u8],
    pi: &mut cmd_parse_input,
) -> Result<Box<cmd_parse_commands>, CString> {
    let session = RefCell::new(ParseSession {
        input: pi,
        error: None,
    });
    cmd_parse_run_parser(&session, Lexer::from_bytes(bytes, &session))
}

fn cmd_parse_log_commands(cmds: &cmd_parse_commands, prefix: &CStr) {
    for (i, cmd) in cmds.items.iter().enumerate() {
        for (j, arg) in cmd.arguments.iter().enumerate() {
            match arg.as_ref() {
                cmd_parse_argument::String(text) => {
                    log_debug(format_args!(
                        "{} {}:{}: {}",
                        log_bytes(prefix.to_bytes()),
                        i,
                        j,
                        log_bytes(text.as_bytes())
                    ));
                }
                cmd_parse_argument::Commands(commands) => {
                    let mut nested = prefix.to_bytes().to_vec();
                    nested.extend_from_slice(format!(" {i}:{j}").as_bytes());
                    let nested = CString::new(nested).expect("parser log prefix has no NUL");
                    cmd_parse_log_commands(commands, &nested);
                }
                cmd_parse_argument::ParsedCommands(commands) => {
                    let printed = cmd_list_print_cstring(&commands.borrow(), 0);
                    log_debug(format_args!(
                        "{} {}:{}: {}",
                        log_bytes(prefix.to_bytes()),
                        i,
                        j,
                        log_bytes(printed.as_bytes())
                    ));
                }
            }
        }
    }
}

fn cmd_parse_expand_alias(
    cmd: &mut cmd_parse_command,
    pi: &mut cmd_parse_input,
    pr: &mut cmd_parse_result,
) -> bool {
    if pi.flags & CMD_PARSE_NOALIAS != 0 {
        return false;
    }
    *pr = cmd_parse_result::empty();
    let Some(cmd_parse_argument::String(name)) = cmd.arguments.front().map(Box::as_ref) else {
        pr.status = CMD_PARSE_SUCCESS;
        pr.cmdlist = Some(cmd_list_new());
        return true;
    };
    let Some(alias) = cmd_get_alias(name) else {
        return false;
    };
    log_debug(format_args!(
        "{}: {} alias {} = {}",
        "cmd_parse_expand_alias",
        pi.line,
        log_bytes(name.as_bytes()),
        log_bytes(alias.as_bytes())
    ));
    let mut cmds = match cmd_parse_do_buffer(alias.as_bytes(), pi) {
        Ok(cmds) => cmds,
        Err(cause) => {
            pr.status = CMD_PARSE_ERROR;
            pr.error = Some(cause);
            return true;
        }
    };
    let Some(last) = cmds.items.last_mut() else {
        pr.status = CMD_PARSE_SUCCESS;
        pr.cmdlist = Some(cmd_list_new());
        return true;
    };
    drop(
        cmd.arguments
            .pop_front()
            .expect("alias command has a first argument"),
    );
    last.arguments.append(&mut cmd.arguments);
    cmd_parse_log_commands(&cmds, c"cmd_parse_expand_alias");
    pi.flags |= CMD_PARSE_NOALIAS;
    cmd_parse_build_commands(&mut cmds, pi, pr);
    pi.flags &= !CMD_PARSE_NOALIAS;
    true
}

fn cmd_parse_build_command(
    cmd: &mut cmd_parse_command,
    pi: &mut cmd_parse_input,
    pr: &mut cmd_parse_result,
) {
    *pr = cmd_parse_result::empty();
    if cmd_parse_expand_alias(cmd, pi, pr) {
        return;
    }
    let mut built = Vec::new();
    for arg in &mut cmd.arguments {
        if let cmd_parse_argument::Commands(commands) = arg.as_mut() {
            cmd_parse_build_commands(commands, pi, pr);
            if pr.status != CMD_PARSE_SUCCESS {
                return;
            }
            built.push(pr.cmdlist.take().expect("successful command parse"));
        }
    }
    let mut built = built.iter();
    let values: Vec<ParseValue> = cmd
        .arguments
        .iter()
        .map(|arg| match arg.as_ref() {
            cmd_parse_argument::String(text) => ParseValue::String(text),
            cmd_parse_argument::Commands(_) => {
                ParseValue::Commands(built.next().expect("built command block"))
            }
            cmd_parse_argument::ParsedCommands(commands) => ParseValue::Commands(commands),
        })
        .collect();
    match cmd_parse(&values, pi.file.as_deref(), pi.line, pi.flags) {
        Ok(command) => {
            pr.status = CMD_PARSE_SUCCESS;
            pr.cmdlist = Some(cmd_list_new());
            cmd_list_append(pr.cmdlist.as_ref().expect("parsed command list"), command);
        }
        Err(cause) => {
            pr.status = CMD_PARSE_ERROR;
            pr.error = Some(cmd_parse_get_error(pi.file.as_deref(), pi.line, &cause));
        }
    }
}

fn cmd_parse_build_commands(
    cmds: &mut cmd_parse_commands,
    pi: &mut cmd_parse_input,
    pr: &mut cmd_parse_result,
) {
    let mut line: u_int = UINT_MAX;
    let mut current: Option<Rc<std::cell::RefCell<cmd_list>>> = None;
    *pr = cmd_parse_result::empty();
    let command_count = cmds.items.len();
    if command_count == 0 {
        pr.status = CMD_PARSE_SUCCESS;
        pr.cmdlist = Some(cmd_list_new());
        return;
    }
    cmd_parse_log_commands(cmds, c"cmd_parse_build_commands");
    let result_owner = cmd_list_new();
    let result = &result_owner;
    for cmd in &mut cmds.items {
        if !pi.flags & CMD_PARSE_ONEGROUP != 0 && cmd.line != line {
            if let Some(current) = current.take() {
                cmd_parse_print_commands(pi, &current.borrow());
                cmd_list_move(result, &current);
            }
        }
        let current = current.get_or_insert_with(cmd_list_new);
        pi.line = cmd.line;
        line = pi.line;
        cmd_parse_build_command(cmd, pi, pr);
        if pr.status as ::core::ffi::c_uint
            != CMD_PARSE_SUCCESS as ::core::ffi::c_int as ::core::ffi::c_uint
        {
            return;
        }
        cmd_list_append_all(current, pr.cmdlist.as_ref().expect("parsed command list"));
        drop(pr.cmdlist.take());
    }
    if let Some(current) = current {
        cmd_parse_print_commands(pi, &current.borrow());
        cmd_list_move(result, &current);
    }
    let s = cmd_list_print_cstring(&result.borrow(), 0);
    log_debug(format_args!(
        "{}: {}",
        "cmd_parse_build_commands",
        log_bytes(s.as_bytes())
    ));
    pr.status = CMD_PARSE_SUCCESS;
    pr.cmdlist = Some(result_owner);
}
/// Parse file contents already read into memory.
pub fn cmd_parse_from_file(bytes: &[u8], pi: Option<&mut cmd_parse_input>) -> cmd_parse_result {
    let mut input: cmd_parse_input = cmd_parse_input {
        flags: 0,
        file: None,
        line: 0,
        item: std::rc::Weak::new(),
        c: Default::default(),
        fs: cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        },
    };
    let mut pr = cmd_parse_result::empty();
    let pi = pi.unwrap_or(&mut input);
    let mut cmds = match cmd_parse_do_file(bytes, pi) {
        Ok(cmds) => cmds,
        Err(cause) => {
            pr.status = CMD_PARSE_ERROR;
            pr.error = Some(cause);
            return pr;
        }
    };
    cmd_parse_build_commands(&mut cmds, pi, &mut pr);
    pr
}
pub fn cmd_parse_from_string(s: &CStr, pi: Option<&mut cmd_parse_input>) -> cmd_parse_result {
    let mut input: cmd_parse_input = cmd_parse_input {
        flags: 0,
        file: None,
        line: 0,
        item: std::rc::Weak::new(),
        c: Default::default(),
        fs: cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        },
    };
    let pi = pi.unwrap_or(&mut input);
    pi.flags |= CMD_PARSE_ONEGROUP;
    cmd_parse_from_buffer(s.to_bytes(), Some(pi))
}
pub fn cmd_parse_and_append(
    s: &CStr,
    owner: Option<&ClientRef>,
    state: Option<&std::rc::Rc<cmdq_state>>,
) -> Result<cmd_parse_status, Option<CString>> {
    let item_allocation;
    let mut pr = cmd_parse_from_string(s, None);
    if pr.status == CMD_PARSE_ERROR {
        return Err(pr.error.take());
    }
    item_allocation = cmdq_get_command(
        pr.cmdlist.as_ref().expect("successful command parse"),
        state,
    );
    cmdq_append(owner, item_allocation);
    drop(pr.cmdlist.take());
    Ok(pr.status)
}
pub fn cmd_parse_from_buffer(buf: &[u8], pi: Option<&mut cmd_parse_input>) -> cmd_parse_result {
    let mut input: cmd_parse_input = cmd_parse_input {
        flags: 0,
        file: None,
        line: 0,
        item: std::rc::Weak::new(),
        c: Default::default(),
        fs: cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        },
    };
    let mut pr = cmd_parse_result::empty();
    let pi = pi.unwrap_or(&mut input);
    if buf.is_empty() {
        pr.status = CMD_PARSE_SUCCESS;
        pr.cmdlist = Some(cmd_list_new());
        return pr;
    }
    let mut cmds = match cmd_parse_do_buffer(buf, pi) {
        Ok(cmds) => cmds,
        Err(cause) => {
            pr.status = CMD_PARSE_ERROR;
            pr.error = Some(cause);
            return pr;
        }
    };
    cmd_parse_build_commands(&mut cmds, pi, &mut pr);
    pr
}
/// Parse argv while borrowing its strings for the duration of the parser call.
pub fn cmd_parse_from_argv(argv: &[CString]) -> cmd_parse_result {
    let values: Vec<ParseValue> = argv
        .iter()
        .map(|string| ParseValue::String(string))
        .collect();
    cmd_parse_from_arguments(&values, None)
}

pub fn cmd_parse_from_arguments(
    values: &[ParseValue<'_>],
    pi: Option<&mut cmd_parse_input>,
) -> cmd_parse_result {
    let mut input: cmd_parse_input = cmd_parse_input {
        flags: 0,
        file: None,
        line: 0,
        item: std::rc::Weak::new(),
        c: Default::default(),
        fs: cmd_find_state {
            flags: 0,
            s: Default::default(),
            wl: Default::default(),
            w: Default::default(),
            wp: Default::default(),
            idx: 0,
        },
    };
    let mut pr = cmd_parse_result::empty();
    let pi = pi.unwrap_or(&mut input);
    let mut cmds = cmd_parse_new_commands();
    let mut cmd = cmd_parse_new_command(pi.line);
    for value in values {
        let mut end = false;
        match *value {
            ParseValue::String(text) => {
                let mut bytes = text.to_bytes().to_vec();
                if bytes.last() == Some(&b';') {
                    bytes.pop();
                    if bytes.last() == Some(&b'\\') {
                        *bytes.last_mut().unwrap() = b';';
                    } else {
                        end = true;
                    }
                }
                if !end || !bytes.is_empty() {
                    cmd.arguments.push_back(Box::new(cmd_parse_argument::String(
                        CString::new(bytes).expect("argument contains no NUL"),
                    )));
                }
            }
            ParseValue::Commands(commands) => {
                cmd.arguments
                    .push_back(Box::new(cmd_parse_argument::ParsedCommands(
                        commands.clone(),
                    )));
            }
        }
        if end {
            cmds.items.push(cmd);
            cmd = cmd_parse_new_command(pi.line);
        }
    }
    if !cmd.arguments.is_empty() {
        cmds.items.push(cmd);
    }
    cmd_parse_build_commands(&mut cmds, pi, &mut pr);
    pr
}
#[cfg(test)]
mod parser_collection_tests {
    use super::*;

    fn parse_commands(input: &[u8]) -> Vec<Vec<Vec<u8>>> {
        let mut pi = cmd_parse_input {
            line: 1,
            ..Default::default()
        };
        let cmds = cmd_parse_do_buffer(input, &mut pi).expect("valid parser test input");
        let result = cmds
            .items
            .iter()
            .map(|cmd| {
                cmd.arguments
                    .iter()
                    .map(|arg| {
                        let cmd_parse_argument::String(text) = arg.as_ref() else {
                            panic!("expected parser string argument");
                        };
                        text.as_bytes().to_vec()
                    })
                    .collect()
            })
            .collect();
        result
    }

    fn string_command(line: u_int, strings: &[&CStr]) -> cmd_parse_command {
        let mut cmd = cmd_parse_new_command(line);
        cmd.arguments.extend(
            strings
                .iter()
                .map(|text| Box::new(cmd_parse_argument::String((*text).to_owned()))),
        );
        cmd
    }

    fn compiled_commands(source: &CStr) -> Rc<std::cell::RefCell<cmd_list>> {
        let mut input = cmd_parse_input {
            flags: CMD_PARSE_NOALIAS,
            ..Default::default()
        };
        let mut result = cmd_parse_from_string(source, Some(&mut input));
        assert_eq!(result.status, CMD_PARSE_SUCCESS, "{:?}", result.error);
        result.cmdlist.take().expect("successful command parse")
    }

    #[test]
    fn nested_parser_boxes_release_compiled_payloads_after_ownership_transfer() {
        let first = compiled_commands(c"display-message first");
        let second = compiled_commands(c"display-message second");
        let first_observer = Rc::downgrade(&first);
        let second_observer = Rc::downgrade(&second);
        let mut nested = cmd_parse_new_commands();
        let mut inner = cmd_parse_new_command(1);
        inner
            .arguments
            .push_back(Box::new(cmd_parse_argument::ParsedCommands(first)));
        nested.items.push(inner);
        let mut outer = cmd_parse_new_command(2);
        outer
            .arguments
            .push_back(Box::new(cmd_parse_argument::Commands(nested)));
        outer
            .arguments
            .push_back(Box::new(cmd_parse_argument::ParsedCommands(second)));
        let mut source = cmd_parse_new_commands();
        source.items.push(outer);
        let address = source.items[0].arguments[0].as_ref() as *const cmd_parse_argument as usize;
        let mut destination = cmd_parse_new_commands();
        destination.items.push(source.items.pop().unwrap());
        drop(source);
        assert_eq!(
            destination.items[0].arguments[0].as_ref() as *const cmd_parse_argument as usize,
            address
        );
        assert_eq!(first_observer.strong_count(), 1);
        assert_eq!(second_observer.strong_count(), 1);
        drop(destination);
        assert!(first_observer.upgrade().is_none());
        assert!(second_observer.upgrade().is_none());
    }

    #[test]
    fn built_commands_outlive_parser_boxes_and_errors_release_temporary_references() {
        for fail_nested in [false, true] {
            let compiled = compiled_commands(c"display-message -p nested");
            let observer = Rc::downgrade(&compiled);
            let mut command = string_command(1, &[c"if-shell", c"-F", c"1"]);
            command
                .arguments
                .push_back(Box::new(cmd_parse_argument::ParsedCommands(compiled)));
            if fail_nested {
                let mut nested = cmd_parse_new_commands();
                nested
                    .items
                    .push(string_command(7, &[c"unknown-parser-owner-command"]));
                command
                    .arguments
                    .push_back(Box::new(cmd_parse_argument::Commands(nested)));
            }
            let mut commands = cmd_parse_new_commands();
            commands.items.push(command);
            let mut input = cmd_parse_input {
                flags: CMD_PARSE_NOALIAS,
                file: Some(c"nested.conf".to_owned()),
                ..Default::default()
            };
            let mut result = cmd_parse_result::empty();
            cmd_parse_build_commands(&mut commands, &mut input, &mut result);
            if fail_nested {
                assert_eq!(result.status, CMD_PARSE_ERROR);
                assert_eq!(
                    result.error.as_deref(),
                    Some(c"nested.conf:7: unknown command: unknown-parser-owner-command")
                );
                assert_eq!(
                    observer.strong_count(),
                    1,
                    "failed build retained a temporary command value"
                );
                drop(commands);
                assert!(observer.upgrade().is_none());
            } else {
                assert_eq!(result.status, CMD_PARSE_SUCCESS, "{:?}", result.error);
                drop(commands);
                assert_eq!(
                    observer.strong_count(),
                    1,
                    "built command must retain its nested command list"
                );
                assert_eq!(
                    cmd_list_print_cstring(
                        &result
                            .cmdlist
                            .as_ref()
                            .expect("parsed command list")
                            .borrow(),
                        0
                    )
                    .as_bytes(),
                    b"if-shell -F 1 { display-message -p nested }"
                );
                drop(result.cmdlist.take());
                assert!(observer.upgrade().is_none());
            }
        }
    }

    #[test]
    fn parser_preserves_specific_errors_and_recovers_for_next_parse() {
        for (source, expected) in [
            (
                b"display-message \\400\n".as_slice(),
                b"input.conf:3: invalid octal escape".as_slice(),
            ),
            (
                b"%if 1\ndisplay-message missing-endif\n",
                b"input.conf:5: syntax error",
            ),
        ] {
            let mut input = cmd_parse_input {
                line: 3,
                file: Some(c"input.conf".to_owned()),
                ..Default::default()
            };
            let error = cmd_parse_do_buffer(source, &mut input)
                .err()
                .expect("invalid parser input");
            assert_eq!(error.as_bytes(), expected);
        }
        assert_eq!(
            parse_commands(b"display-message recovered"),
            vec![vec![b"display-message".to_vec(), b"recovered".to_vec()]]
        );
    }
}
