//! Command grammar extracted from tiri's LALRPOP migration.
//!
//! The caller supplies a lazy token stream and application services. Tokens must
//! include a final newline for a nonempty last statement, as the tiri lexer does.
//! Keep lexing lazy: grammar actions can assign variables used by later tokens.
#![forbid(unsafe_code)]
use std::ffi::CStr;

lalrpop_util::lalrpop_mod!(parse_grammar);

/// Application services used by grammar actions.
/// A lexer may share state with this context, but must release its borrow before
/// returning each token.
pub trait Context {
    fn line(&self) -> u32;
    fn expand_format(&mut self, token: TokenText) -> TokenText;
    /// Validate every assignment, including inactive ones. Only apply it when
    /// `active` and the application's parse-only setting permit side effects.
    fn put_environ(&mut self, token: TokenText, hidden: bool, active: bool)
        -> Result<(), LexError>;
    fn is_true(&self, token: &TokenText) -> bool;
}

pub type ParseError = lalrpop_util::ParseError<usize, Token, LexError>;

/// Parse a lazy stream of `(start, token, end)` spans into owned commands.
pub fn parse(
    context: &mut dyn Context,
    tokens: impl IntoIterator<Item = Result<(usize, Token, usize), LexError>>,
) -> Result<Vec<ParseCommand>, ParseError> {
    let tokens = tokens.into_iter().flat_map(|token| {
        let boundary = match &token {
            Ok((_, Token::Equals(_), end)) => Some(Ok((*end, Token::EqualsEnd, *end))),
            _ => None,
        };
        std::iter::once(token).chain(boundary)
    });
    parse_grammar::LinesParser::new().parse(&mut ParseState::new(context), tokens)
}

/// A NUL-terminated token string owned by the parser.
#[derive(Debug)]
pub struct TokenText(std::ffi::CString);
impl TokenText {
    pub fn from_cstring(text: std::ffi::CString) -> TokenText {
        TokenText(text)
    }
    pub fn as_c_str(&self) -> &CStr {
        self.0.as_c_str()
    }
}
impl Clone for TokenText {
    fn clone(&self) -> TokenText {
        TokenText(self.0.clone())
    }
}

/// A terminal of the command grammar.
#[derive(Clone, Debug)]
pub enum Token {
    Newline,
    Semicolon,
    OpenBrace,
    CloseBrace,
    Hidden,
    If,
    Else,
    Elif,
    Endif,
    Format(TokenText),
    Word(TokenText),
    Equals(TokenText),
    /// Internal lookahead boundary inserted by `parse`; lexers must not emit it.
    #[doc(hidden)]
    EqualsEnd,
}

/// The lexer or an application service aborted parsing. The host can retain
/// a more specific diagnostic alongside this marker.
#[derive(Debug)]
pub struct LexError;

/// One argument of a command, before it is built into a `struct cmd`.
#[derive(Debug)]
pub enum ParseArgument {
    String(TokenText),
    Commands(Vec<ParseCommand>),
}

/// One command, before it is built into a `struct cmd`.
#[derive(Debug)]
pub struct ParseCommand {
    pub line: u32,
    pub arguments: Vec<ParseArgument>,
}
impl ParseCommand {
    /// The `command : assignment` case, which carries no arguments.
    pub fn empty(line: u32) -> ParseCommand {
        ParseCommand {
            line,
            arguments: Vec::new(),
        }
    }
    pub fn new(line: u32, name: TokenText, arguments: Vec<ParseArgument>) -> ParseCommand {
        let mut arguments = arguments;
        arguments.insert(0, ParseArgument::String(name));
        ParseCommand { line, arguments }
    }
}

/// The value of an `elif` chain: whether a branch was taken, and its body.
pub(crate) struct ElifResult {
    pub flag: bool,
    pub commands: Vec<ParseCommand>,
}
impl ElifResult {
    pub fn taken(commands: Vec<ParseCommand>) -> ElifResult {
        ElifResult {
            flag: true,
            commands,
        }
    }
    pub fn skipped() -> ElifResult {
        ElifResult {
            flag: false,
            commands: Vec::new(),
        }
    }
}

pub(crate) fn concat(mut a: Vec<ParseCommand>, b: Vec<ParseCommand>) -> Vec<ParseCommand> {
    a.extend(b);
    a
}

pub(crate) fn prepend(a: ParseArgument, mut rest: Vec<ParseArgument>) -> Vec<ParseArgument> {
    rest.insert(0, a);
    rest
}

/// The `%if` scope stack, threaded through the grammar actions.
///
/// `scope` is the innermost `%if`, `stack` the enclosing ones, innermost
/// last.
pub(crate) struct ParseState<'a> {
    context: &'a mut dyn Context,
    scope: Option<bool>,
    stack: Vec<bool>,
}

impl<'a> ParseState<'a> {
    pub fn new(context: &'a mut dyn Context) -> Self {
        Self {
            context,
            scope: None,
            stack: Vec::new(),
        }
    }

    pub fn line(&self) -> u32 {
        self.context.line()
    }

    fn scope_active(&self) -> bool {
        self.scope.unwrap_or(true)
    }

    /// `statement : condition | commands` — a body under a false `%if` is
    /// discarded.
    pub fn keep_if_active(&self, commands: Vec<ParseCommand>) -> Vec<ParseCommand> {
        if self.scope_active() {
            commands
        } else {
            Vec::new()
        }
    }

    /// `commands : command`.
    pub fn start_commands(&self, command: ParseCommand) -> Vec<ParseCommand> {
        if !command.arguments.is_empty() && self.scope_active() {
            vec![command]
        } else {
            Vec::new()
        }
    }

    /// `commands : commands ';' command`. Under a false `%if` the list is
    /// discarded. Otherwise an argument-less command, such as a bare
    /// assignment, is skipped and the commands before it are kept; tmux
    /// discards them here.
    pub fn push_command(
        &self,
        mut commands: Vec<ParseCommand>,
        command: ParseCommand,
    ) -> Vec<ParseCommand> {
        if !self.scope_active() {
            return Vec::new();
        }
        if !command.arguments.is_empty() {
            commands.push(command);
        }
        commands
    }

    /// `expanded : format`.
    pub fn expand_format(&mut self, token: TokenText) -> TokenText {
        self.context.expand_format(token)
    }

    pub fn put_environ(&mut self, token: TokenText, hidden: bool) -> Result<(), LexError> {
        let active = self.scope_active() && self.stack.iter().all(|scope| *scope);
        self.context.put_environ(token, hidden, active)
    }

    /// `if_open : IF expanded`.
    pub fn push_scope(&mut self, expanded: &TokenText) -> bool {
        let flag = self.context.is_true(expanded);
        if let Some(scope) = self.scope {
            self.stack.push(scope);
        }
        self.scope = Some(flag);
        flag
    }

    /// `if_else : ELSE`.
    pub fn invert_scope(&mut self) {
        self.scope = Some(!self.scope_active());
    }

    /// `if_elif : ELIF expanded`.
    pub fn replace_scope(&mut self, expanded: &TokenText) -> bool {
        let flag = self.context.is_true(expanded);
        self.scope = Some(flag);
        flag
    }

    /// `if_close : ENDIF`.
    pub fn pop_scope(&mut self) {
        self.scope = self.stack.pop();
    }
}
