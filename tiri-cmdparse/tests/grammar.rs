use tiri_cmdparse::{parse, Context, LexError, ParseArgument, ParseCommand, Token, TokenText};

#[derive(Default)]
struct Host {
    assignments: Vec<(Vec<u8>, bool, bool)>,
}
impl Context for Host {
    fn line(&self) -> u32 {
        7
    }
    fn expand_format(&mut self, token: TokenText) -> TokenText {
        token
    }
    fn put_environ(
        &mut self,
        token: TokenText,
        hidden: bool,
        active: bool,
    ) -> Result<(), LexError> {
        self.assignments
            .push((token.as_c_str().to_bytes().to_vec(), hidden, active));
        Ok(())
    }
    fn is_true(&self, token: &TokenText) -> bool {
        !matches!(token.as_c_str().to_bytes(), b"" | b"0")
    }
}
fn text(value: &str) -> TokenText {
    TokenText::from_cstring(std::ffi::CString::new(value).unwrap())
}
fn word(value: &str) -> Token {
    Token::Word(text(value))
}
fn run(host: &mut Host, tokens: Vec<Token>) -> Vec<ParseCommand> {
    parse(
        host,
        tokens
            .into_iter()
            .enumerate()
            .map(|(i, t)| Ok((i, t, i + 1))),
    )
    .unwrap()
}
fn name(command: &ParseCommand) -> &[u8] {
    match &command.arguments[0] {
        ParseArgument::String(text) => text.as_c_str().to_bytes(),
        _ => panic!("command name must be a string"),
    }
}

#[test]
fn separators_and_nested_command_arguments() {
    use Token::*;
    let commands = run(
        &mut Host::default(),
        vec![
            word("bind"),
            word("x"),
            OpenBrace,
            word("one"),
            Semicolon,
            word("two"),
            Newline,
            word("three"),
            CloseBrace,
            Semicolon,
            word("four"),
            Semicolon,
            Newline,
        ],
    );
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].line, 7);
    assert_eq!(name(&commands[1]), b"four");
    let ParseArgument::Commands(nested) = &commands[0].arguments[2] else {
        panic!()
    };
    assert_eq!(
        nested.iter().map(name).collect::<Vec<_>>(),
        [b"one".as_slice(), b"two", b"three"]
    );
}

#[test]
fn multiline_and_inline_conditionals_select_first_true_branch() {
    use Token::*;
    for separator in [vec![], vec![Newline]] {
        let mut tokens = vec![If, word("0")];
        tokens.extend(separator.clone());
        tokens.push(word("wrong"));
        tokens.extend(separator.clone());
        tokens.extend([Elif, word("1")]);
        tokens.extend(separator.clone());
        tokens.push(word("chosen"));
        tokens.extend(separator.clone());
        tokens.extend([Elif, word("1")]);
        tokens.extend(separator.clone());
        tokens.push(word("later"));
        tokens.extend(separator.clone());
        tokens.push(Else);
        tokens.extend(separator.clone());
        tokens.push(word("wrong"));
        tokens.extend(separator.clone());
        tokens.extend([Endif, Newline]);
        let commands = run(&mut Host::default(), tokens);
        assert_eq!(commands.len(), 1);
        assert_eq!(name(&commands[0]), b"chosen");
    }
}

#[test]
fn assignments_respect_all_enclosing_scopes_and_hidden_flag() {
    use Token::*;
    let mut host = Host::default();
    let commands = run(
        &mut host,
        vec![
            If,
            word("0"),
            Newline,
            If,
            word("1"),
            Newline,
            Hidden,
            Equals(text("A=skipped")),
            Newline,
            word("wrong"),
            Newline,
            Endif,
            Newline,
            Else,
            Newline,
            Equals(text("A=chosen")),
            Newline,
            word("chosen"),
            Newline,
            Endif,
            Newline,
        ],
    );
    assert_eq!(
        host.assignments,
        [
            (b"A=skipped".to_vec(), true, false),
            (b"A=chosen".to_vec(), false, true)
        ]
    );
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), b"chosen");
}

#[test]
fn trailing_assignment_keeps_accumulated_commands() {
    use Token::*;
    let mut host = Host::default();
    let commands = run(
        &mut host,
        vec![word("one"), Semicolon, Equals(text("A=1")), Newline],
    );
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), b"one");
    assert_eq!(host.assignments, vec![(b"A=1".to_vec(), false, true)]);
}

#[test]
fn semicolon_lists_under_a_false_condition_are_discarded() {
    use Token::*;
    let commands = run(
        &mut Host::default(),
        vec![
            If,
            word("0"),
            Newline,
            word("one"),
            Semicolon,
            word("two"),
            Semicolon,
            word("three"),
            Newline,
            Endif,
            Newline,
            word("after"),
            Semicolon,
            word("last"),
            Newline,
        ],
    );
    assert_eq!(
        commands.iter().map(name).collect::<Vec<_>>(),
        [b"after".as_slice(), b"last"]
    );
}

#[test]
fn errors_and_empty_input() {
    assert!(run(&mut Host::default(), vec![]).is_empty());
    for tokens in [
        vec![Token::CloseBrace, Token::Newline],
        vec![Token::If, word("1"), Token::Newline],
    ] {
        assert!(parse(
            &mut Host::default(),
            tokens.into_iter().map(|t| Ok((0, t, 0)))
        )
        .is_err());
    }
    assert!(parse(&mut Host::default(), [Err(LexError)]).is_err());
}

#[test]
fn assignments_take_effect_before_lexing_the_next_word() {
    use std::{cell::RefCell, rc::Rc};

    struct Environment(Rc<RefCell<String>>);
    impl Context for Environment {
        fn line(&self) -> u32 {
            1
        }
        fn expand_format(&mut self, token: TokenText) -> TokenText {
            token
        }
        fn is_true(&self, token: &TokenText) -> bool {
            token.as_c_str().to_bytes() != b"0"
        }
        fn put_environ(
            &mut self,
            token: TokenText,
            _hidden: bool,
            active: bool,
        ) -> Result<(), LexError> {
            if active {
                *self.0.borrow_mut() = token
                    .as_c_str()
                    .to_str()
                    .unwrap()
                    .split_once('=')
                    .unwrap()
                    .1
                    .to_owned();
            }
            Ok(())
        }
    }

    for previous in ["", "stale"] {
        for active in [false, true] {
            let value = Rc::new(RefCell::new(previous.to_owned()));
            let mut host = Environment(value.clone());
            // Expand the command name lazily, as the application lexer does.
            let mut tokens = vec![
                Token::If,
                word(if active { "1" } else { "0" }),
                Token::Newline,
                Token::Equals(text("CMD=display-message")),
                word("$CMD"),
                Token::Newline,
                Token::Endif,
                Token::Newline,
            ]
            .into_iter();
            let tokens = std::iter::from_fn(|| {
                tokens.next().map(|token| {
                    let token = match token {
                        Token::Word(ref text) if text.as_c_str().to_bytes() == b"$CMD" => {
                            assert_eq!(
                                &*value.borrow(),
                                if active { "display-message" } else { previous }
                            );
                            word(&value.borrow())
                        }
                        token => token,
                    };
                    Ok((0, token, 0))
                })
            });
            let commands = parse(&mut host, tokens).unwrap();
            if active {
                assert_eq!(name(&commands[0]), b"display-message");
            } else {
                assert!(commands.is_empty());
            }
        }
    }
}

#[test]
fn equals_arguments_do_not_assign_environment_variables() {
    let mut host = Host::default();
    let commands = run(
        &mut host,
        vec![
            word("display-message"),
            Token::Equals(text("CMD=argument")),
            Token::Newline,
        ],
    );
    assert!(host.assignments.is_empty());
    let ParseArgument::String(argument) = &commands[0].arguments[1] else {
        panic!()
    };
    assert_eq!(argument.as_c_str().to_bytes(), b"CMD=argument");
}
