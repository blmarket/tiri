//! Byte-oriented command lexer. Expansion happens as each token is requested,
//! after any preceding grammar actions have updated the application environment.
use super::{ParseSession, RefCell};
use crate::environ::environ_find;
use crate::ffi::libc::wctomb;
use crate::shared::stdio::EOF;
use crate::tmux::global_environ;
use libc::{getpwnam, getpwuid, getuid};
use std::ffi::{c_char, CStr, CString};
use tiri_cmdparse::{Token, TokenText};

type LocatedToken = (usize, Token, usize);

#[derive(Debug)]
enum LexError {
    Syntax,
    InvalidOctal,
    InvalidUnicode(u8),
    InvalidVariable,
    VariableTooLong,
    UserTooLong,
}

impl LexError {
    fn message(&self) -> &CStr {
        match self {
            Self::Syntax => c"syntax error",
            Self::InvalidOctal => c"invalid octal escape",
            Self::InvalidUnicode(b'u') => c"invalid \\u argument",
            Self::InvalidUnicode(_) => c"invalid \\U argument",
            Self::InvalidVariable => c"invalid environment variable",
            Self::VariableTooLong => c"environment variable is too long",
            Self::UserTooLong => c"user name is too long",
        }
    }
}

/// Both sources are in memory; they differ only in how a raw byte widens,
/// as tmux reads buffers as `char` and files through `getc`.
enum Source<'a> {
    Bytes(&'a [u8]),
    File(&'a [u8]),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

pub(super) struct Lexer<'a, 'input> {
    source: Source<'a>,
    session: &'a RefCell<ParseSession<'input>>,
    offset: usize,
    escapes: usize,
    condition: bool,
    eol: bool,
    eof: bool,
    finished: bool,
}

impl<'a, 'input> Lexer<'a, 'input> {
    pub(super) fn from_bytes(bytes: &'a [u8], session: &'a RefCell<ParseSession<'input>>) -> Self {
        Self::new(Source::Bytes(bytes), session)
    }

    pub(super) fn from_file(bytes: &'a [u8], session: &'a RefCell<ParseSession<'input>>) -> Self {
        Self::new(Source::File(bytes), session)
    }

    fn new(source: Source<'a>, session: &'a RefCell<ParseSession<'input>>) -> Self {
        Self {
            source,
            session,
            offset: 0,
            escapes: 0,
            condition: false,
            eol: false,
            eof: false,
            finished: false,
        }
    }

    fn advance_line(&self) {
        let mut session = self.session.borrow_mut();
        session.input.line = session.input.line.wrapping_add(1);
    }

    fn read_byte(&mut self) -> i32 {
        let (Source::Bytes(bytes) | Source::File(bytes)) = self.source;
        let Some(&byte) = bytes.get(self.offset) else {
            return EOF;
        };
        self.offset += 1;
        match self.source {
            Source::Bytes(_) => i32::from(byte as c_char),
            Source::File(_) => i32::from(byte),
        }
    }

    fn unread(&mut self, ch: i32) {
        if ch == EOF {
            return;
        }
        self.offset = self.offset.saturating_sub(1);
    }

    fn read(&mut self) -> i32 {
        if self.escapes != 0 {
            self.escapes -= 1;
            return i32::from(b'\\');
        }
        loop {
            let ch = self.read_byte();
            if ch == i32::from(b'\\') {
                self.escapes += 1;
            } else if ch == i32::from(b'\n') && self.escapes % 2 == 1 {
                self.advance_line();
                self.escapes -= 1;
            } else {
                if self.escapes != 0 {
                    self.unread(ch);
                    self.escapes -= 1;
                    return i32::from(b'\\');
                }
                return ch;
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        if self.eol {
            self.advance_line();
        }
        self.eol = false;
        let condition = std::mem::take(&mut self.condition);
        loop {
            let mut ch = self.read();
            if ch == EOF {
                return if std::mem::replace(&mut self.eof, true) {
                    Ok(None)
                } else {
                    Ok(Some(Token::Newline))
                };
            }
            if ch == i32::from(b' ') || ch == i32::from(b'\t') {
                continue;
            }
            if ch == i32::from(b'\r') {
                ch = self.read();
                if ch != i32::from(b'\n') {
                    self.unread(ch);
                    ch = i32::from(b'\r');
                }
            }
            let token = match ch {
                10 => {
                    self.eol = true;
                    Token::Newline
                }
                59 => Token::Semicolon,
                123 => Token::OpenBrace,
                125 => Token::CloseBrace,
                35 => {
                    let mut next = self.read();
                    if condition && next == i32::from(b'{') {
                        Token::Format(text(self.format()?))
                    } else {
                        while next != i32::from(b'\n') && next != EOF {
                            next = self.read();
                        }
                        if next == EOF {
                            continue;
                        }
                        self.advance_line();
                        Token::Newline
                    }
                }
                37 => {
                    let word = self.directive();
                    if word.as_bytes().iter().all(|&b| b == b'%' || is_digit(b)) {
                        Token::Word(text(word))
                    } else {
                        self.condition = true;
                        match word.as_bytes() {
                            b"%hidden" => Token::Hidden,
                            b"%if" => Token::If,
                            b"%else" => Token::Else,
                            b"%elif" => Token::Elif,
                            b"%endif" => Token::Endif,
                            _ => return Err(LexError::Syntax),
                        }
                    }
                }
                _ => {
                    let word = self.word(ch)?;
                    let bytes = word.as_bytes();
                    let assignment = bytes.iter().position(|&b| b == b'=').is_some_and(|end| {
                        end > 0
                            && is_variable(bytes[0], true)
                            && bytes[1..end].iter().all(|&b| is_variable(b, false))
                    });
                    if assignment {
                        Token::Equals(text(word))
                    } else {
                        Token::Word(text(word))
                    }
                }
            };
            return Ok(Some(token));
        }
    }

    fn directive(&mut self) -> CString {
        let mut bytes = vec![b'%'];
        loop {
            let ch = self.read();
            if ch == EOF || matches!(ch, 0 | 32 | 9 | 10) {
                self.unread(ch);
                return finish(bytes);
            }
            bytes.push(ch as u8);
        }
    }

    fn format(&mut self) -> Result<CString, LexError> {
        let mut bytes = b"#{".to_vec();
        let mut brackets = 1;
        loop {
            let mut ch = self.read();
            if ch == EOF || ch == 10 {
                return Err(LexError::Syntax);
            }
            if ch == i32::from(b'#') {
                ch = self.read();
                if ch == EOF || ch == 10 {
                    return Err(LexError::Syntax);
                }
                if ch == i32::from(b'{') {
                    brackets += 1;
                }
                bytes.push(b'#');
            } else if ch == i32::from(b'}') {
                brackets -= 1;
                if brackets == 0 {
                    bytes.push(b'}');
                    return Ok(finish(bytes));
                }
            }
            bytes.push(ch as u8);
        }
    }

    fn word(&mut self, mut ch: i32) -> Result<CString, LexError> {
        let mut bytes = Vec::new();
        let mut quote = Quote::None;
        let mut last = None;
        loop {
            if ch == EOF {
                break;
            }
            if quote == Quote::None && ch == 13 {
                ch = self.read();
                if ch != 10 {
                    self.unread(ch);
                    ch = 13;
                }
            }
            if ch == 10 {
                if quote == Quote::None {
                    break;
                }
                self.advance_line();
            }
            if quote == Quote::None && matches!(ch, 32 | 9 | 59 | 125) {
                break;
            }
            if ch == 10 && quote != Quote::None {
                bytes.push(b'\n');
                loop {
                    ch = self.read();
                    if !matches!(ch, 32 | 9) {
                        break;
                    }
                }
                if ch != i32::from(b'#') {
                    continue;
                }
                ch = self.read();
                if matches!(ch, 0 | 44 | 35 | 123 | 125 | 58) {
                    self.unread(ch);
                    ch = i32::from(b'#');
                } else {
                    loop {
                        ch = self.read();
                        if ch == 10 || ch == EOF {
                            break;
                        }
                    }
                }
                continue;
            }
            if ch == i32::from(b'\\') && quote != Quote::Single {
                self.escape(&mut bytes)?;
            } else if ch == i32::from(b'~') && last != Some(quote) && quote != Quote::Single {
                self.tilde(&mut bytes)?;
            } else if ch == i32::from(b'$') && quote != Quote::Single {
                self.variable(&mut bytes)?;
            } else if ch == i32::from(b'\'') && quote != Quote::Double {
                quote = if quote == Quote::None {
                    Quote::Single
                } else {
                    Quote::None
                };
                ch = self.read();
                continue;
            } else if ch == i32::from(b'"') && quote != Quote::Single {
                quote = if quote == Quote::None {
                    Quote::Double
                } else {
                    Quote::None
                };
                ch = self.read();
                continue;
            } else {
                bytes.push(ch as u8);
            }
            last = Some(quote);
            ch = self.read();
        }
        self.unread(ch);
        Ok(finish(bytes))
    }

    fn escape(&mut self, bytes: &mut Vec<u8>) -> Result<(), LexError> {
        let ch = self.read();
        if (i32::from(b'4')..=i32::from(b'7')).contains(&ch) {
            return Err(LexError::InvalidOctal);
        }
        if (i32::from(b'0')..=i32::from(b'3')).contains(&ch) {
            let second = self.read();
            if !(48..=55).contains(&second) {
                return Err(LexError::InvalidOctal);
            }
            let third = self.read();
            if !(48..=55).contains(&third) {
                return Err(LexError::InvalidOctal);
            }
            bytes.push((64 * (ch - 48) + 8 * (second - 48) + third - 48) as u8);
            return Ok(());
        }
        let byte = match ch {
            EOF => return Err(LexError::Syntax),
            97 => 7,
            98 => 8,
            101 => 27,
            102 => 12,
            115 => 32,
            118 => 11,
            114 => 13,
            110 => 10,
            116 => 9,
            117 | 85 => {
                let mut value = 0u32;
                for _ in 0..if ch == 117 { 4 } else { 8 } {
                    let digit = self.read();
                    if digit == EOF || digit == 10 {
                        return Err(LexError::Syntax);
                    }
                    let digit = char::from(digit as u8)
                        .to_digit(16)
                        .ok_or(LexError::InvalidUnicode(ch as u8))?;
                    value = (value << 4) | digit;
                }
                let mut encoded = [0 as c_char; 16];
                let len = unsafe { wctomb(encoded.as_mut_ptr(), value as libc::wchar_t) };
                if len <= 0 || len as usize > encoded.len() {
                    return Err(LexError::InvalidUnicode(ch as u8));
                }
                bytes.extend(encoded[..len as usize].iter().map(|&b| b as u8));
                return Ok(());
            }
            _ => ch as u8,
        };
        bytes.push(byte);
        Ok(())
    }

    fn variable(&mut self, bytes: &mut Vec<u8>) -> Result<(), LexError> {
        let ch = self.read();
        if ch == EOF {
            return Err(LexError::Syntax);
        }
        let brackets = ch == i32::from(b'{');
        let mut name = Vec::new();
        if !brackets {
            if !is_variable(ch as u8, true) {
                bytes.push(b'$');
                self.unread(ch);
                return Ok(());
            }
            name.push(ch as u8);
        }
        loop {
            let ch = self.read();
            if brackets && ch == i32::from(b'}') {
                break;
            }
            if ch == EOF || !is_variable(ch as u8, false) {
                if brackets {
                    return Err(LexError::InvalidVariable);
                }
                self.unread(ch);
                break;
            }
            if name.len() == 1022 {
                return Err(LexError::VariableTooLong);
            }
            name.push(ch as u8);
        }
        let name = finish(name);
        unsafe {
            let entry = environ_find(global_environ.as_deref().expect("environment"), &name);
            if let Some(entry) = entry {
                if let Some(value) = &entry.value {
                    bytes.extend_from_slice(value.as_bytes());
                }
            }
        }
        Ok(())
    }

    fn tilde(&mut self, bytes: &mut Vec<u8>) -> Result<(), LexError> {
        let mut name = Vec::new();
        loop {
            let ch = self.read();
            if ch == EOF || matches!(ch, 0 | 47 | 32 | 9 | 10 | 34 | 39) {
                self.unread(ch);
                break;
            }
            if name.len() == 1022 {
                return Err(LexError::UserTooLong);
            }
            name.push(ch as u8);
        }
        let name = finish(name);
        unsafe {
            let pw = if name.is_empty() {
                let entry = environ_find(global_environ.as_deref().expect("environment"), c"HOME");
                if let Some(entry) = entry {
                    if let Some(home) = &entry.value {
                        if !home.is_empty() {
                            bytes.extend_from_slice(home.as_bytes());
                            return Ok(());
                        }
                    }
                }
                getpwuid(getuid())
            } else {
                getpwnam(name.as_ptr())
            };
            if pw.is_null() || (*pw).pw_dir.is_null() {
                return Err(LexError::Syntax);
            }
            bytes.extend_from_slice(CStr::from_ptr((*pw).pw_dir).to_bytes());
        }
        Ok(())
    }
}

impl Iterator for Lexer<'_, '_> {
    type Item = Result<LocatedToken, tiri_cmdparse::LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let start = self.offset;
        match self.next_token() {
            Ok(Some(token)) => Some(Ok((start, token, self.offset))),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(error) => {
                self.finished = true;
                self.session.borrow_mut().record_error(error.message());
                Some(Err(tiri_cmdparse::LexError))
            }
        }
    }
}

fn text(value: CString) -> TokenText {
    TokenText::from_cstring(value)
}

fn finish(mut bytes: Vec<u8>) -> CString {
    if let Some(nul) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(nul);
    }
    CString::new(bytes).expect("token truncated at its first NUL")
}

fn is_digit(byte: u8) -> bool {
    unsafe { ::libc::isdigit(byte as ::core::ffi::c_int) != 0 }
}

fn is_variable(byte: u8, first: bool) -> bool {
    byte != b'='
        && !(first && is_digit(byte))
        && (byte == b'_' || unsafe { ::libc::isalnum(byte as ::core::ffi::c_int) != 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::command::cmd_parse_input;

    fn label(token: Token) -> (String, Vec<u8>) {
        match token {
            Token::Word(s) => ("word".into(), s.as_c_str().to_bytes().to_vec()),
            Token::Equals(s) => ("assignment".into(), s.as_c_str().to_bytes().to_vec()),
            Token::Format(s) => ("format".into(), s.as_c_str().to_bytes().to_vec()),
            other => (format!("{other:?}"), Vec::new()),
        }
    }

    fn scan(lexer: Lexer<'_, '_>) -> Vec<(String, Vec<u8>)> {
        lexer.map(|token| label(token.unwrap().1)).collect()
    }

    #[test]
    fn quotes_escapes_comments_and_directives() {
        let mut input = cmd_parse_input {
            line: 1,
            ..Default::default()
        };
        let session = RefCell::new(ParseSession {
            input: &mut input,
            error: None,
        });
        let source = b"one\" two\"' three' \\141\\000ignored;{ x } # comment\r\n%if #{?#{x},1,0}\n%elif 0\n%else\n%endif\n%hidden VAR=yes\n%12 %% \\u0041 \\U00000042\n";
        let actual = scan(Lexer::from_bytes(source, &session));
        let expected: &[(&str, &[u8])] = &[
            ("word", b"one two three"),
            ("word", b"a"),
            ("Semicolon", b""),
            ("OpenBrace", b""),
            ("word", b"x"),
            ("CloseBrace", b""),
            ("Newline", b""),
            ("If", b""),
            ("format", b"#{?#{x},1,0}"),
            ("Newline", b""),
            ("Elif", b""),
            ("word", b"0"),
            ("Newline", b""),
            ("Else", b""),
            ("Newline", b""),
            ("Endif", b""),
            ("Newline", b""),
            ("Hidden", b""),
            ("assignment", b"VAR=yes"),
            ("Newline", b""),
            ("word", b"%12"),
            ("word", b"%%"),
            ("word", b"A"),
            ("word", b"B"),
            ("Newline", b""),
            ("Newline", b""),
        ];
        assert_eq!(
            actual,
            expected
                .iter()
                .map(|(kind, text)| (kind.to_string(), text.to_vec()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn escaped_newlines_multiline_quotes_and_eof() {
        let mut input = cmd_parse_input {
            line: 1,
            ..Default::default()
        };
        let session = RefCell::new(ParseSession {
            input: &mut input,
            error: None,
        });
        let actual = scan(Lexer::from_bytes(
            b"ab\\\ncd \"one\n  # ignored\n  two\" 'unterminated",
            &session,
        ));
        assert_eq!(
            actual,
            vec![
                ("word".into(), b"abcd".to_vec()),
                ("word".into(), b"one\n\ntwo".to_vec()),
                ("word".into(), b"unterminated".to_vec()),
                ("Newline".into(), vec![]),
            ]
        );
        assert_eq!(session.borrow().input.line, 4);
    }

    #[test]
    fn errors_keep_file_line_and_stop_the_iterator() {
        for (source, error) in [
            (b"\\400".as_slice(), b"invalid octal escape".as_slice()),
            (b"\\12", b"invalid octal escape"),
            (b"\\uQ000", b"invalid \\u argument"),
            (b"\\UQ0000000", b"invalid \\U argument"),
            (b"${bad-}", b"invalid environment variable"),
            (b"%unknown", b"syntax error"),
            (b"%if #{unfinished", b"syntax error"),
            (b"\\", b"syntax error"),
        ] {
            let mut input = cmd_parse_input {
                line: 7,
                file: Some(c"test\xff.conf".to_owned()),
                ..Default::default()
            };
            let session = RefCell::new(ParseSession {
                input: &mut input,
                error: None,
            });
            let mut lexer = Lexer::from_bytes(source, &session);
            while lexer.next().is_some_and(|token| token.is_ok()) {}
            assert!(lexer.next().is_none());
            let expected = [b"test\xff.conf:7: ".as_slice(), error].concat();
            assert_eq!(
                session.borrow().error.as_ref().unwrap().as_bytes(),
                expected,
                "{source:?}"
            );
        }
    }

    #[test]
    fn interleaved_lexers_have_independent_state() {
        let mut first = cmd_parse_input {
            line: 1,
            ..Default::default()
        };
        let mut second = cmd_parse_input {
            line: 20,
            ..Default::default()
        };
        let first = RefCell::new(ParseSession {
            input: &mut first,
            error: None,
        });
        let second = RefCell::new(ParseSession {
            input: &mut second,
            error: None,
        });
        let mut a = Lexer::from_bytes(b"%if #{x}\na", &first);
        let mut b = Lexer::from_bytes(b"# comment\nb \\400", &second);
        assert!(matches!(a.next().unwrap().unwrap().1, Token::If));
        assert!(matches!(b.next().unwrap().unwrap().1, Token::Newline));
        assert!(matches!(a.next().unwrap().unwrap().1, Token::Format(_)));
        assert_eq!(
            label(b.next().unwrap().unwrap().1),
            ("word".into(), b"b".to_vec())
        );
        assert!(b.next().unwrap().is_err());
        assert_eq!(
            scan(a),
            vec![
                ("Newline".into(), vec![]),
                ("word".into(), b"a".to_vec()),
                ("Newline".into(), vec![])
            ]
        );
        assert_eq!(first.borrow().input.line, 2);
        assert_eq!(second.borrow().input.line, 21);
        assert!(first.borrow().error.is_none());
    }

    #[test]
    fn raw_ff_is_consumed_as_char_in_buffers_but_unsigned_in_files() {
        let mut input = cmd_parse_input::default();
        let session = RefCell::new(ParseSession {
            input: &mut input,
            error: None,
        });
        let bytes = b"\xffx";
        let mut buffer = Lexer::from_bytes(bytes, &session);
        assert_eq!(buffer.read_byte(), i32::from(0xff_u8 as c_char));
        assert_eq!(buffer.read_byte(), i32::from(b'x'));
        assert_eq!(buffer.read_byte(), EOF);
        let mut lexer = Lexer::from_file(bytes, &session);
        assert_eq!(lexer.read_byte(), 255);
        assert_eq!(lexer.read_byte(), i32::from(b'x'));
        assert_eq!(lexer.read_byte(), EOF);
    }

    #[test]
    fn file_and_buffer_preserve_high_bytes_and_escaped_ff() {
        let mut bytes = b"word '".to_vec();
        bytes.extend(128..=254);
        bytes.extend_from_slice(b"' \\377 end\r\n");
        let mut buffer_input = cmd_parse_input {
            line: 1,
            ..Default::default()
        };
        let buffer_session = RefCell::new(ParseSession {
            input: &mut buffer_input,
            error: None,
        });
        let expected = scan(Lexer::from_bytes(&bytes, &buffer_session));
        assert_eq!(expected[1].1, (128..=254).collect::<Vec<u8>>());
        assert_eq!(expected[2].1, vec![255]);
        let mut input = cmd_parse_input {
            line: 1,
            ..Default::default()
        };
        let session = RefCell::new(ParseSession {
            input: &mut input,
            error: None,
        });
        let actual = scan(Lexer::from_file(&bytes, &session));
        assert_eq!(actual, expected);
        assert_eq!(
            session.borrow().input.line,
            buffer_session.borrow().input.line
        );
    }
}
